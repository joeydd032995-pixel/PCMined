//! Monitor-only adapter for ASIC/Bitaxe devices exposing AxeOS, and similar
//! pool/device HTTP APIs. NO local process is ever spawned — we only read
//! telemetry from `GET http://<device-ip>/api/system/info`.

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use serde_json::Value;

use crate::miner::telemetry::{self, Telemetry};
use crate::miner::{
    Algo, MinerAdapter, MinerInfo, MinerStats, ResolvedProfile, StatsSource, TelemetryBinding,
    TelemetryKind,
};

pub const INFO: MinerInfo = MinerInfo {
    id: "monitor",
    display: "Device / pool monitor",
    algos: &[Algo::Sha256d, Algo::Scrypt],
    dev_fee_pct: 0.0,
    open_source: true,
    source_url: "https://github.com/bitaxeorg/ESP-Miner",
    license: "N/A (monitor-only)",
    telemetry_kind: TelemetryKind::Http,
};

pub struct MonitorDevice;

/// Parse a human difficulty string like "1.2k", "850", "3.4T" into a number.
pub fn parse_suffixed(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        return 0.0;
    }
    let (num, mult) = match s.chars().last().unwrap() {
        'k' | 'K' => (&s[..s.len() - 1], 1e3),
        'M' => (&s[..s.len() - 1], 1e6),
        'G' => (&s[..s.len() - 1], 1e9),
        'T' => (&s[..s.len() - 1], 1e12),
        'P' => (&s[..s.len() - 1], 1e15),
        _ => (s, 1.0),
    };
    num.trim().parse::<f64>().unwrap_or(0.0) * mult
}

/// Read a possibly-stringly-typed difficulty field.
fn diff_field(v: &Value, key: &str) -> f64 {
    match v.get(key) {
        Some(Value::String(s)) => parse_suffixed(s),
        Some(n) => n.as_f64().unwrap_or(0.0),
        None => 0.0,
    }
}

/// Parse an AxeOS `/api/system/info` body into normalized stats. `hashRate` is
/// reported in GH/s.
pub fn parse_axeos(body: &str) -> anyhow::Result<MinerStats> {
    let v: Value = serde_json::from_str(body).context("AxeOS info was not JSON")?;
    let gh = v.get("hashRate").and_then(Value::as_f64).unwrap_or(0.0);
    let hashrate_hs = gh * 1e9;
    let accepted = v.get("sharesAccepted").and_then(Value::as_f64).unwrap_or(0.0).max(0.0) as u64;
    let rejected = v.get("sharesRejected").and_then(Value::as_f64).unwrap_or(0.0).max(0.0) as u64;
    let uptime_secs = v.get("uptimeSeconds").and_then(Value::as_f64).unwrap_or(0.0).max(0.0) as u64;
    Ok(MinerStats {
        hashrate_hs,
        hashrate_avg: [hashrate_hs; 3],
        accepted,
        rejected,
        best_share_diff: diff_field(&v, "bestDiff"),
        uptime_secs,
        connected: hashrate_hs > 0.0,
        current_job_diff: None,
        source: StatsSource::Live,
    })
}

#[async_trait]
impl MinerAdapter for MonitorDevice {
    fn info(&self) -> &MinerInfo {
        &INFO
    }

    // Monitor-only: never spawned, so there are no CLI args.
    fn build_args(&self, _p: &ResolvedProfile, _t: &TelemetryBinding) -> Vec<String> {
        Vec::new()
    }

    async fn read(&self, t: &Telemetry) -> anyhow::Result<MinerStats> {
        let body = match t {
            Telemetry::HttpUrl { url, token } => {
                telemetry::fetch_http_url(url, token.as_deref()).await?
            }
            Telemetry::Http { port, path, token } => {
                telemetry::fetch_http(*port, path, token.as_deref()).await?
            }
            other => return Err(anyhow!("monitor expects HTTP telemetry, got {other:?}")),
        };
        parse_axeos(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_axeos_info() {
        let body = r#"{
            "hashRate": 485.5,
            "bestDiff": "1.2k",
            "sharesAccepted": 1024,
            "sharesRejected": 3,
            "uptimeSeconds": 7200
        }"#;
        let s = parse_axeos(body).unwrap();
        assert_eq!(s.hashrate_hs, 485.5e9);
        assert_eq!(s.best_share_diff, 1200.0);
        assert_eq!(s.accepted, 1024);
        assert_eq!(s.rejected, 3);
        assert_eq!(s.uptime_secs, 7200);
        assert!(s.connected);
    }

    #[test]
    fn suffix_parsing() {
        assert_eq!(parse_suffixed("850"), 850.0);
        assert_eq!(parse_suffixed("1.2k"), 1200.0);
        assert_eq!(parse_suffixed("3.4T"), 3.4e12);
        assert_eq!(parse_suffixed(""), 0.0);
    }
}
