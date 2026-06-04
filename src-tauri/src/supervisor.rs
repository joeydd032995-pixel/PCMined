//! The supervisor owns every running miner instance, resolves which adapter to
//! use, enforces the dev-fee confirmation gate, and caps concurrency.
//!
//! SECURITY: the only inputs that reach a spawned process are fields of a
//! [`StartRequest`] that the supervisor maps onto a named, registry-backed
//! adapter. There is no path from the webview to an arbitrary command line.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::config::miners;
use crate::events::{channel, LogEvent};
use crate::hardware;
use crate::miner::adapters::cpuminer_opt::CpuminerOpt;
use crate::miner::adapters::ethminer::EthMiner;
use crate::miner::adapters::kawpowminer::KawpowMiner;
use crate::miner::adapters::lolminer::LolMiner;
use crate::miner::adapters::monitor_device::MonitorDevice;
use crate::miner::instance::Instance;
use crate::miner::telemetry::Telemetry;
use crate::miner::{Algo, MinerAdapter, MinerInfo, ResolvedProfile, TelemetryBinding, TelemetryKind};

/// Hard cap on simultaneous spawned miners (CPU/GPU contention guard; P4 adds
/// hardware-aware warnings on top of this).
const MAX_CONCURRENT: usize = 8;
/// Loopback telemetry ports are handed out starting here.
const TELEMETRY_PORT_BASE: u16 = 4048;

/// A launch request from the frontend. In P2 this is produced by resolving a
/// saved profile against the coin/miner registries; for P1 it is passed
/// directly. Contains a wallet ADDRESS only — never a private key.
#[derive(Debug, Clone, Deserialize)]
pub struct StartRequest {
    pub miner_id: String,
    pub coin: String,
    pub algo: Algo,
    pub binary_path: String,
    pub pool_host: String,
    pub pool_port: u16,
    pub user: String,
    #[serde(default = "default_pass")]
    pub pass: String,
    #[serde(default)]
    pub tls: bool,
    #[serde(default)]
    pub threads: Option<u32>,
    #[serde(default)]
    pub extra_args: Vec<String>,
}

fn default_pass() -> String {
    "x".to_string()
}

/// Structured start failure. `FeeConfirmationRequired` tells the frontend to
/// show the one-time fee confirmation dialog before retrying via
/// `confirm_fee_and_start`.
#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StartError {
    #[error("no adapter registered for miner '{miner_id}'")]
    AdapterNotFound { miner_id: String },
    #[error("miner '{miner_id}' does not support the requested algorithm")]
    UnsupportedAlgo { miner_id: String },
    #[error("miner '{miner_id}' charges a {dev_fee_pct}% dev fee; confirmation required")]
    FeeConfirmationRequired {
        miner_id: String,
        dev_fee_pct: f32,
        source_url: String,
        license: String,
    },
    #[error("concurrency limit reached ({max} miners)")]
    ConcurrencyLimit { max: usize },
    #[error("failed to start miner: {message}")]
    SpawnFailed { message: String },
}

/// Summary of a running instance for the `list_running` command.
#[derive(Debug, Clone, Serialize)]
pub struct RunningMiner {
    pub id: String,
    pub miner_id: String,
    pub coin: String,
    pub dev_fee_pct: f32,
}

/// The dev-fee gate: a fee'd miner may not start until the user has confirmed.
/// Pure and unit-tested; the only place fee policy is enforced.
pub fn fee_gate(dev_fee_pct: f32, confirmed: bool) -> bool {
    dev_fee_pct <= 0.0 || confirmed
}

/// Warn (non-fatally) when a start would oversubscribe the CPU. Pure + tested.
pub fn contention_warning(requested_threads: u32, logical_cores: u32) -> Option<String> {
    if logical_cores > 0 && requested_threads > logical_cores {
        Some(format!(
            "requesting {requested_threads} threads on {logical_cores} logical cores may hurt performance"
        ))
    } else {
        None
    }
}

pub struct Supervisor {
    adapters: HashMap<&'static str, Arc<dyn MinerAdapter>>,
    monitor_adapter: Arc<dyn MinerAdapter>,
    instances: HashMap<String, Instance>,
    next_port: u16,
    next_seq: u64,
    logical_cores: u32,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl Supervisor {
    pub fn new() -> Self {
        let mut adapters: HashMap<&'static str, Arc<dyn MinerAdapter>> = HashMap::new();
        for adapter in [
            Arc::new(CpuminerOpt) as Arc<dyn MinerAdapter>,
            Arc::new(LolMiner) as Arc<dyn MinerAdapter>,
            Arc::new(KawpowMiner) as Arc<dyn MinerAdapter>,
            Arc::new(EthMiner) as Arc<dyn MinerAdapter>,
        ] {
            adapters.insert(adapter.info().id, adapter);
        }
        let (_, _, logical_cores, _) = hardware::detect_cpu();
        Supervisor {
            adapters,
            monitor_adapter: Arc::new(MonitorDevice),
            instances: HashMap::new(),
            next_port: TELEMETRY_PORT_BASE,
            next_seq: 0,
            logical_cores,
        }
    }

    /// Static metadata for the whole roster (spawnable + monitor-only) so fee /
    /// source / license badges are complete in the UI.
    pub fn miner_infos(&self) -> Vec<MinerInfo> {
        miners::all_miners()
    }

    /// Buffered recent log lines for one instance (oldest first).
    pub fn logs(&self, id: &str) -> Option<Vec<String>> {
        self.instances.get(id).map(|i| i.logs())
    }

    pub fn running(&self) -> Vec<RunningMiner> {
        self.instances
            .values()
            .map(|i| RunningMiner {
                id: i.id.clone(),
                miner_id: i.miner_id.clone(),
                coin: i.coin.clone(),
                dev_fee_pct: i.dev_fee_pct,
            })
            .collect()
    }

    fn alloc_port(&mut self) -> u16 {
        let port = self.next_port;
        // Wrap within a small ephemeral-ish band; collisions are unlikely given
        // the concurrency cap and are surfaced as spawn/telemetry errors.
        self.next_port = if self.next_port >= TELEMETRY_PORT_BASE + 200 {
            TELEMETRY_PORT_BASE
        } else {
            self.next_port + 1
        };
        port
    }

    fn telemetry_for(kind: TelemetryKind, port: u16) -> Telemetry {
        match kind {
            TelemetryKind::TcpText => Telemetry::TcpText { port },
            TelemetryKind::Http => Telemetry::Http {
                port,
                path: "/".to_string(),
                token: None,
            },
            TelemetryKind::TcpJsonRpc => Telemetry::TcpJsonRpc { port },
            TelemetryKind::Stdout => Telemetry::Stdout,
        }
    }

    /// Start a miner. Returns the new instance id, or a structured error. When
    /// the chosen miner carries a dev fee and `fee_confirmed` is false, returns
    /// `FeeConfirmationRequired` instead of spawning.
    pub fn start(
        &mut self,
        app: &AppHandle,
        req: StartRequest,
        fee_confirmed: bool,
    ) -> Result<String, StartError> {
        let adapter = self
            .adapters
            .get(req.miner_id.as_str())
            .cloned()
            .ok_or_else(|| StartError::AdapterNotFound { miner_id: req.miner_id.clone() })?;

        let info = *adapter.info();

        if !adapter.supports(req.algo) {
            return Err(StartError::UnsupportedAlgo { miner_id: req.miner_id.clone() });
        }

        if !fee_gate(info.dev_fee_pct, fee_confirmed) {
            return Err(StartError::FeeConfirmationRequired {
                miner_id: info.id.to_string(),
                dev_fee_pct: info.dev_fee_pct,
                source_url: info.source_url.to_string(),
                license: info.license.to_string(),
            });
        }

        if self.instances.len() >= MAX_CONCURRENT {
            return Err(StartError::ConcurrencyLimit { max: MAX_CONCURRENT });
        }

        let port = self.alloc_port();
        let binding = TelemetryBinding::loopback(port);
        let telemetry = Self::telemetry_for(info.telemetry_kind, port);

        self.next_seq += 1;
        let id = format!("{}:{}#{}", req.coin, info.id, self.next_seq);

        let profile = ResolvedProfile {
            coin: req.coin.clone(),
            algo: req.algo,
            binary_path: PathBuf::from(req.binary_path),
            pool_host: req.pool_host,
            pool_port: req.pool_port,
            user: req.user,
            pass: req.pass,
            tls: req.tls,
            threads: req.threads,
            extra_args: req.extra_args,
        };

        let instance = Instance::spawn(
            id.clone(),
            req.coin,
            adapter,
            profile,
            binding,
            telemetry,
            app.clone(),
        )
        .map_err(|e| StartError::SpawnFailed { message: e.to_string() })?;

        // Surface a non-fatal CPU-contention warning into the instance log.
        if let Some(warning) =
            contention_warning(req.threads.unwrap_or(0), self.logical_cores)
        {
            tracing::warn!(%id, %warning, "cpu contention");
            let _ = app.emit(
                channel::LOG,
                LogEvent { miner_id: id.clone(), line: format!("[warn] {warning}") },
            );
        }

        self.instances.insert(id.clone(), instance);
        Ok(id)
    }

    /// Start a monitor-only instance for a device/pool HTTP telemetry endpoint
    /// (e.g. a Bitaxe running AxeOS). No local process is spawned.
    pub fn start_monitor(
        &mut self,
        app: &AppHandle,
        coin: String,
        device_url: String,
    ) -> Result<String, StartError> {
        if self.instances.len() >= MAX_CONCURRENT {
            return Err(StartError::ConcurrencyLimit { max: MAX_CONCURRENT });
        }
        // Accept a base host or a full URL; default to the AxeOS info path.
        let base = if device_url.starts_with("http://") || device_url.starts_with("https://") {
            device_url
        } else {
            format!("http://{device_url}")
        };
        let url = if base.contains("/api/") {
            base
        } else {
            format!("{}/api/system/info", base.trim_end_matches('/'))
        };

        self.next_seq += 1;
        let id = format!("{coin}:monitor#{}", self.next_seq);
        let telemetry = Telemetry::HttpUrl { url, token: None };
        let instance =
            Instance::monitor(id.clone(), coin, self.monitor_adapter.clone(), telemetry, app.clone());
        self.instances.insert(id.clone(), instance);
        Ok(id)
    }

    /// Stop and reap one instance.
    pub async fn stop(&mut self, app: &AppHandle, id: &str) -> Result<(), StartError> {
        let instance = self
            .instances
            .remove(id)
            .ok_or_else(|| StartError::AdapterNotFound { miner_id: id.to_string() })?;
        instance
            .stop(app)
            .await
            .map_err(|e| StartError::SpawnFailed { message: e.to_string() })
    }

    /// Stop and reap every instance (used on shutdown). Best-effort.
    pub async fn stop_all(&mut self, app: &AppHandle) {
        let ids: Vec<String> = self.instances.keys().cloned().collect();
        for id in ids {
            if let Some(instance) = self.instances.remove(&id) {
                let _ = instance.stop(app).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_gate_allows_free_miners() {
        assert!(fee_gate(0.0, false));
        assert!(fee_gate(0.0, true));
    }

    #[test]
    fn fee_gate_blocks_unconfirmed_fee() {
        assert!(!fee_gate(1.0, false));
        assert!(fee_gate(1.0, true));
    }

    #[test]
    fn registers_full_roster() {
        let sup = Supervisor::new();
        let ids: Vec<&str> = sup.miner_infos().iter().map(|i| i.id).collect();
        for id in ["cpuminer-opt", "lolminer", "kawpowminer", "ethminer", "monitor"] {
            assert!(ids.contains(&id), "missing {id}");
        }
    }

    #[test]
    fn contention_warning_only_when_oversubscribed() {
        assert!(contention_warning(4, 8).is_none());
        assert!(contention_warning(8, 8).is_none());
        assert!(contention_warning(16, 8).is_some());
        assert!(contention_warning(4, 0).is_none()); // unknown core count
    }

    #[test]
    fn telemetry_matches_kind() {
        assert!(matches!(
            Supervisor::telemetry_for(TelemetryKind::TcpText, 5),
            Telemetry::TcpText { port: 5 }
        ));
        assert!(matches!(
            Supervisor::telemetry_for(TelemetryKind::Http, 6),
            Telemetry::Http { port: 6, .. }
        ));
    }
}
