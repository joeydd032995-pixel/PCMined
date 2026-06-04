//! lolMiner adapter — kHeavyHash (Kaspa), Autolykos2 (Ergo). ~1% disclosed dev
//! fee (no fee-free option for these coins). Telemetry: HTTP JSON.
//!
//! lolMiner's JSON shape drifts between versions (Session vs top-level totals,
//! "Workers" vs "GPUs", differing performance units), so the parser reads via
//! `serde_json::Value` with fallbacks rather than a fixed struct.

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use serde_json::Value;

use crate::miner::telemetry::{self, hashrate_to_hs, Telemetry};
use crate::miner::{
    Algo, MinerAdapter, MinerInfo, MinerStats, ResolvedProfile, StatsSource, TelemetryBinding,
    TelemetryKind,
};

pub const INFO: MinerInfo = MinerInfo {
    id: "lolminer",
    display: "lolMiner",
    algos: &[Algo::KHeavyHash, Algo::Autolykos2],
    dev_fee_pct: 1.0, // disclosed; confirm against the pinned version at integration
    open_source: false,
    source_url: "https://github.com/Lolliedieb/lolMiner-releases/releases",
    license: "Proprietary (freeware)",
    telemetry_kind: TelemetryKind::Http,
};

pub struct LolMiner;

fn algo_flag(algo: Algo) -> Option<&'static str> {
    match algo {
        Algo::KHeavyHash => Some("HEAVYHASH"),
        Algo::Autolykos2 => Some("AUTOLYKOS2"),
        _ => None,
    }
}

/// First finite f64 reachable at any of the given keys on `obj`.
fn first_f64(obj: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|k| obj.get(*k).and_then(Value::as_f64).filter(|v| v.is_finite()))
}

/// First non-empty string at any of the given keys.
fn first_str<'a>(obj: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| obj.get(*k).and_then(Value::as_str)).filter(|s| !s.is_empty())
}

/// The per-worker array, under whichever key this version uses.
fn workers(v: &Value) -> Option<&Vec<Value>> {
    v.get("Workers").or_else(|| v.get("GPUs")).and_then(Value::as_array)
}

/// Parse a lolMiner JSON telemetry body into normalized stats.
pub fn parse_summary(raw: &str) -> anyhow::Result<MinerStats> {
    let v: Value = serde_json::from_str(raw).context("lolMiner telemetry was not valid JSON")?;
    let session = v.get("Session").cloned().unwrap_or(Value::Null);

    // Performance: Session summary → top-level total → sum of workers.
    let perf = first_f64(&session, &["Performance_Summary", "Total_Performance"])
        .or_else(|| first_f64(&v, &["Total_Performance", "Performance_Summary"]))
        .or_else(|| {
            workers(&v).map(|ws| {
                ws.iter().filter_map(|w| first_f64(w, &["Performance"])).sum::<f64>()
            })
        })
        .ok_or_else(|| anyhow!("no performance figure in lolMiner telemetry"))?;

    let unit = first_str(&session, &["Performance_Unit"])
        .or_else(|| first_str(&v, &["Performance_Unit"]))
        .or_else(|| workers(&v).and_then(|ws| ws.first()).and_then(|w| first_str(w, &["Performance_Unit"])))
        .unwrap_or("H/s");
    let hashrate_hs = hashrate_to_hs(perf, unit);

    let accepted = first_f64(&session, &["Accepted"]).unwrap_or(0.0).max(0.0) as u64;
    let submitted = first_f64(&session, &["Submitted"]).unwrap_or(accepted as f64);
    let rejected = first_f64(&session, &["Rejected"])
        .unwrap_or((submitted - accepted as f64).max(0.0))
        .max(0.0) as u64;
    let uptime_secs = first_f64(&session, &["Uptime"]).unwrap_or(0.0).max(0.0) as u64;

    // Best share may live in Session or per-worker depending on version.
    let best_share_diff = first_f64(&session, &["Best_Share", "Best_Share_Diff"])
        .or_else(|| {
            workers(&v).and_then(|ws| {
                ws.iter()
                    .filter_map(|w| first_f64(w, &["Best_Share", "Best_Share_Diff"]))
                    .fold(None, |acc: Option<f64>, d| Some(acc.map_or(d, |a| a.max(d))))
            })
        })
        .unwrap_or(0.0);

    let active_gpus = first_f64(&session, &["Active_GPUs"]).unwrap_or(0.0);

    Ok(MinerStats {
        hashrate_hs,
        hashrate_avg: [hashrate_hs, hashrate_hs, hashrate_hs],
        accepted,
        rejected,
        best_share_diff,
        uptime_secs,
        connected: uptime_secs > 0 || active_gpus > 0.0 || hashrate_hs > 0.0,
        current_job_diff: first_f64(&session, &["Current_Difficulty"]).filter(|d| *d > 0.0),
        source: StatsSource::Live,
    })
}

#[async_trait]
impl MinerAdapter for LolMiner {
    fn info(&self) -> &MinerInfo {
        &INFO
    }

    fn build_args(&self, p: &ResolvedProfile, t: &TelemetryBinding) -> Vec<String> {
        let algo = algo_flag(p.algo).unwrap_or("HEAVYHASH");
        let mut args = vec![
            "--algo".into(),
            algo.into(),
            "--pool".into(),
            format!("{}:{}", p.pool_host, p.pool_port),
            "--user".into(),
            p.user.clone(),
            "--pass".into(),
            p.pass.clone(),
            "--apiport".into(),
            t.port.to_string(),
            // Bind the API to loopback only — never expose miner telemetry to the network.
            "--apihost".into(),
            t.host.clone(),
        ];
        if p.tls {
            args.push("--tls".into());
            args.push("on".into());
        }
        args.extend(p.extra_args.iter().cloned());
        args
    }

    async fn read(&self, t: &Telemetry) -> anyhow::Result<MinerStats> {
        let (port, path, token) = match t {
            Telemetry::Http { port, path, token } => (*port, path.as_str(), token.as_deref()),
            other => return Err(anyhow!("lolMiner expects Http telemetry, got {other:?}")),
        };
        let raw = telemetry::fetch_http(port, path, token)
            .await
            .context("reading lolMiner telemetry")?;
        parse_summary(&raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn profile() -> ResolvedProfile {
        ResolvedProfile {
            coin: "kas".into(),
            algo: Algo::KHeavyHash,
            binary_path: PathBuf::from("/bin/lolMiner"),
            pool_host: "kaspa.pool".into(),
            pool_port: 4444,
            user: "kaspa:qaddr.rig1".into(),
            pass: "x".into(),
            tls: false,
            threads: None,
            extra_args: vec![],
        }
    }

    #[test]
    fn builds_expected_args() {
        let args = LolMiner.build_args(&profile(), &TelemetryBinding::loopback(4068));
        assert_eq!(
            args,
            vec![
                "--algo", "HEAVYHASH",
                "--pool", "kaspa.pool:4444",
                "--user", "kaspa:qaddr.rig1",
                "--pass", "x",
                "--apiport", "4068",
                "--apihost", "127.0.0.1",
            ]
        );
    }

    #[test]
    fn parses_session_summary() {
        let raw = r#"{
            "Software": "lolMiner 1.95",
            "Session": {
                "Uptime": 3600,
                "Active_GPUs": 1,
                "Performance_Summary": 145.5,
                "Performance_Unit": "MH/s",
                "Accepted": 20,
                "Submitted": 22,
                "Best_Share": 1.0e9
            },
            "Workers": [ { "Index": 0, "Performance": 145.5, "Performance_Unit": "MH/s" } ]
        }"#;
        let s = parse_summary(raw).unwrap();
        assert_eq!(s.hashrate_hs, 145_500_000.0);
        assert_eq!(s.accepted, 20);
        assert_eq!(s.rejected, 2); // submitted - accepted
        assert_eq!(s.uptime_secs, 3600);
        assert_eq!(s.best_share_diff, 1.0e9);
        assert!(s.connected);
    }

    #[test]
    fn falls_back_to_worker_sum() {
        // No Session totals: sum the per-worker performance, read unit from worker.
        let raw = r#"{
            "GPUs": [
                { "Performance": 10.0, "Performance_Unit": "MH/s" },
                { "Performance": 5.0, "Performance_Unit": "MH/s" }
            ]
        }"#;
        let s = parse_summary(raw).unwrap();
        assert_eq!(s.hashrate_hs, 15_000_000.0);
    }

    #[test]
    fn rejects_non_json() {
        assert!(parse_summary("<html>error</html>").is_err());
    }
}
