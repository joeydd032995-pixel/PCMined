//! Concrete `MinerAdapter` implementations, one per supported miner binary.
//!
//! Adding a coin/miner = one adapter here + one registry entry
//! (`config/miners.rs`). cpuminer-opt (TcpText, 0%) and lolMiner (HTTP, ~1%)
//! land in P1; kawpowminer + ethminer (TcpJsonRpc, 0%) and the monitor-only
//! device adapter (HTTP, no process) land in P4.

pub mod cpuminer_opt;
pub mod ethminer;
pub mod kawpowminer;
pub mod lolminer;
pub mod monitor_device;

use anyhow::{anyhow, Context};
use serde_json::Value;

use crate::miner::{MinerStats, ResolvedProfile, StatsSource, TelemetryBinding};

// ethminer & kawpowminer share the Claymore-style JSON-RPC API and an almost
// identical CLI. These helpers keep the two adapters thin.

/// Build args for an ethminer-family miner: the wallet/worker is embedded in
/// the stratum `-P` URL, and the API binds to the telemetry port.
pub fn ethminer_build_args(p: &ResolvedProfile, t: &TelemetryBinding) -> Vec<String> {
    let scheme = if p.tls { "stratum+ssl" } else { "stratum+tcp" };
    let mut args = vec![
        "-P".into(),
        format!("{scheme}://{}@{}:{}", p.user, p.pool_host, p.pool_port),
        "--api-port".into(),
        t.port.to_string(),
    ];
    args.extend(p.extra_args.iter().cloned());
    args
}

/// Parse an ethminer `miner_getstat1` response into normalized stats.
///
/// `result` layout (Claymore-compatible): [version, runtime_min,
/// "totalHashrate;accepted;rejected", per-GPU hashrates, …]. The total hashrate
/// is reported in kH/s by convention; we scale to H/s. Tolerant of drift.
pub fn parse_ethminer_getstat1(body: &str) -> anyhow::Result<MinerStats> {
    let v: Value = serde_json::from_str(body).context("getstat1 was not JSON")?;
    let result = v
        .get("result")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("getstat1 missing result array"))?;

    let runtime_min = result
        .get(1)
        .and_then(Value::as_str)
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let stat = result.get(2).and_then(Value::as_str).unwrap_or("");
    let mut parts = stat.split(';');
    let khs = parts.next().and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
    let accepted = parts.next().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
    let rejected = parts.next().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);

    let hashrate_hs = khs * 1_000.0;
    Ok(MinerStats {
        hashrate_hs,
        hashrate_avg: [hashrate_hs; 3],
        accepted,
        rejected,
        best_share_diff: 0.0,
        uptime_secs: runtime_min * 60,
        connected: hashrate_hs > 0.0 || accepted > 0,
        current_job_diff: None,
        source: StatsSource::Live,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_getstat1() {
        let body = r#"{"id":0,"jsonrpc":"2.0","result":
            ["ethminer-0.19","12","48720;7;1","16240;16240;16240","0;0;0","","60;55","pool:4444","0;0"]}"#;
        let s = parse_ethminer_getstat1(body).unwrap();
        assert_eq!(s.hashrate_hs, 48_720_000.0); // 48720 kH/s → H/s
        assert_eq!(s.accepted, 7);
        assert_eq!(s.rejected, 1);
        assert_eq!(s.uptime_secs, 12 * 60);
        assert!(s.connected);
    }

    #[test]
    fn tolerant_of_short_result() {
        let body = r#"{"result":["v","0","0;0;0"]}"#;
        let s = parse_ethminer_getstat1(body).unwrap();
        assert_eq!(s.hashrate_hs, 0.0);
        assert!(!s.connected);
    }

    #[test]
    fn rejects_non_json() {
        assert!(parse_ethminer_getstat1("nope").is_err());
        assert!(parse_ethminer_getstat1("{}").is_err());
    }
}
