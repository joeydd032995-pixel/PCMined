//! Miner abstraction: the `MinerAdapter` trait plus the shared data contracts
//! (`Algo`, `MinerInfo`, `MinerStats`, telemetry binding types).
//!
//! Adapters are thin: they know how to turn a resolved profile into CLI args for
//! a specific third-party miner binary, and how to parse that miner's telemetry
//! into a normalized `MinerStats`. They never hash and never speak Stratum.

pub mod adapters;
pub mod instance;
pub mod telemetry;
pub mod watchdog;

use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use telemetry::Telemetry;

/// Proof-of-work algorithm a miner can run. Mirrors the coin roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Algo {
    RandomX,
    Scrypt,
    Sha256d,
    KawPow,
    EtcHash,
    Ethash,
    KHeavyHash,
    Autolykos2,
    KarlsenHash,
    PyrinHash,
}

/// How a miner exposes telemetry (used for UI/registry display; the concrete
/// connection details live in [`Telemetry`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryKind {
    Http,
    TcpJsonRpc,
    TcpText,
    Stdout,
}

/// Static metadata about a miner binary. `dev_fee_pct == 0.0` means fee-free.
///
/// Fee is first-class data, surfaced on every miner card and gated before start.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct MinerInfo {
    pub id: &'static str,
    pub display: &'static str,
    pub algos: &'static [Algo],
    pub dev_fee_pct: f32,
    pub open_source: bool,
    pub source_url: &'static str,
    pub license: &'static str,
    pub telemetry_kind: TelemetryKind,
}

/// Where a `MinerStats` snapshot came from — live read, or a stale cache that
/// the UI must visibly mark.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum StatsSource {
    Live,
    Stale { age_secs: u64 },
}

/// Normalized miner telemetry snapshot rendered by the dashboard.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MinerStats {
    /// Instantaneous hashrate in raw hashes per second.
    pub hashrate_hs: f64,
    /// Rolling averages [10s, 60s, 15m] where the miner reports them (else 0).
    pub hashrate_avg: [f64; 3],
    pub accepted: u64,
    pub rejected: u64,
    /// The lottery number: best share difficulty seen this session.
    pub best_share_diff: f64,
    pub uptime_secs: u64,
    pub connected: bool,
    /// Current job/network difficulty the miner is working against, if reported.
    pub current_job_diff: Option<f64>,
    pub source: StatsSource,
}

impl Default for MinerStats {
    fn default() -> Self {
        MinerStats {
            hashrate_hs: 0.0,
            hashrate_avg: [0.0; 3],
            accepted: 0,
            rejected: 0,
            best_share_diff: 0.0,
            uptime_secs: 0,
            connected: false,
            current_job_diff: None,
            source: StatsSource::Live,
        }
    }
}

/// A fully-resolved, validated launch profile. Built in P2 from a saved profile
/// + the coin/miner registries; consumed by adapters to produce CLI args.
///
/// NOTE: `user` is already templated (e.g. `wallet.worker`). No private key is
/// ever present here — addresses only.
#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub coin: String,
    pub algo: Algo,
    pub binary_path: PathBuf,
    pub pool_host: String,
    pub pool_port: u16,
    pub user: String,
    pub pass: String,
    pub tls: bool,
    pub threads: Option<u32>,
    pub extra_args: Vec<String>,
}

/// Local loopback endpoint the miner should bind its telemetry API to. The
/// supervisor allocates the port; the adapter wires it into both the CLI args
/// and the [`Telemetry`] reader so the two always agree.
#[derive(Debug, Clone)]
pub struct TelemetryBinding {
    pub host: String,
    pub port: u16,
}

impl TelemetryBinding {
    pub fn loopback(port: u16) -> Self {
        TelemetryBinding { host: "127.0.0.1".to_string(), port }
    }
}

/// Adapter contract for a specific third-party miner binary.
#[async_trait]
pub trait MinerAdapter: Send + Sync {
    fn info(&self) -> &MinerInfo;

    fn supports(&self, algo: Algo) -> bool {
        self.info().algos.contains(&algo)
    }

    /// Dev fee for a specific algorithm. Defaults to the miner's headline fee;
    /// miners whose fee varies by algorithm (e.g. lolMiner) override this so the
    /// fee gate and confirmation dialog disclose the exact percentage.
    fn dev_fee_for(&self, _algo: Algo) -> f32 {
        self.info().dev_fee_pct
    }

    /// Build the command-line arguments to launch this miner for `p`, binding
    /// its telemetry API to `t`.
    fn build_args(&self, p: &ResolvedProfile, t: &TelemetryBinding) -> Vec<String>;

    /// Read a single telemetry snapshot from a running instance.
    async fn read(&self, t: &Telemetry) -> anyhow::Result<MinerStats>;
}
