//! cpuminer-opt adapter — RandomX (Monero), Scrypt, SHA256d. Fee-free (0%),
//! open source. Telemetry: cgminer-style text API over TCP.
//!
//! Summary record format (from cpuminer-opt api.c), `;`-separated, `|`-terminated:
//!   NAME=…;VER=…;API=…;ALGO=…;CPUS=…;URL=…;HS=<h/s>;KHS=<kh/s>;ACC=…;REJ=…;
//!   SOL=…;ACCMN=…;DIFF=…;TEMP=…;FAN=…;FREQ=…;UPTIME=<s>;TS=…|
//! Field set drifts across versions, so the parser is key/value tolerant.

use std::collections::HashMap;

use anyhow::{anyhow, Context};
use async_trait::async_trait;

use crate::miner::telemetry::{self, Telemetry};
use crate::miner::{
    Algo, MinerAdapter, MinerInfo, MinerStats, ResolvedProfile, StatsSource, TelemetryBinding,
    TelemetryKind,
};

pub const INFO: MinerInfo = MinerInfo {
    id: "cpuminer-opt",
    display: "cpuminer-opt",
    algos: &[Algo::RandomX, Algo::Scrypt, Algo::Sha256d],
    dev_fee_pct: 0.0,
    open_source: true,
    source_url: "https://github.com/JayDDee/cpuminer-opt/releases",
    license: "GPL-2.0",
    telemetry_kind: TelemetryKind::TcpText,
};

pub struct CpuminerOpt;

/// Map our algo enum to cpuminer-opt's `-a` flag.
fn algo_flag(algo: Algo) -> Option<&'static str> {
    match algo {
        Algo::RandomX => Some("rx0"),
        Algo::Scrypt => Some("scrypt"),
        Algo::Sha256d => Some("sha256d"),
        _ => None,
    }
}

/// Parse a cpuminer-opt API summary record into normalized stats.
pub fn parse_summary(raw: &str) -> anyhow::Result<MinerStats> {
    let record = raw.split('|').next().unwrap_or(raw).trim();
    if record.is_empty() {
        return Err(anyhow!("empty cpuminer summary"));
    }
    let kv: HashMap<&str, &str> = record
        .split(';')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect();

    if kv.is_empty() {
        return Err(anyhow!("unrecognized cpuminer summary: {record:?}"));
    }

    let num = |key: &str| kv.get(key).and_then(|v| v.parse::<f64>().ok());

    // Prefer raw H/s; fall back to KHS (kilo-hash/s).
    let hashrate_hs = num("HS")
        .filter(|v| *v > 0.0)
        .or_else(|| num("KHS").map(|k| k * 1_000.0))
        .unwrap_or(0.0);

    let accepted = num("ACC").unwrap_or(0.0).max(0.0) as u64;
    let rejected = num("REJ").unwrap_or(0.0).max(0.0) as u64;
    let uptime_secs = num("UPTIME").unwrap_or(0.0).max(0.0) as u64;
    let current_job_diff = num("DIFF").filter(|d| *d > 0.0);

    Ok(MinerStats {
        hashrate_hs,
        hashrate_avg: [hashrate_hs, hashrate_hs, hashrate_hs],
        accepted,
        rejected,
        // best_share is not in the summary; the supervisor fills it from stdout.
        best_share_diff: 0.0,
        uptime_secs,
        // cpuminer's summary has no explicit pool-connected flag; treat a running
        // miner (uptime or hashing) as connected.
        connected: uptime_secs > 0 || hashrate_hs > 0.0,
        current_job_diff,
        source: StatsSource::Live,
    })
}

#[async_trait]
impl MinerAdapter for CpuminerOpt {
    fn info(&self) -> &MinerInfo {
        &INFO
    }

    fn build_args(&self, p: &ResolvedProfile, t: &TelemetryBinding) -> Vec<String> {
        let algo = algo_flag(p.algo).unwrap_or("rx0");
        let scheme = if p.tls { "stratum+ssl" } else { "stratum+tcp" };
        let mut args = vec![
            "-a".into(),
            algo.into(),
            "-o".into(),
            format!("{scheme}://{}:{}", p.pool_host, p.pool_port),
            "-u".into(),
            p.user.clone(),
            "-p".into(),
            p.pass.clone(),
            "--api-bind".into(),
            format!("{}:{}", t.host, t.port),
        ];
        if let Some(threads) = p.threads {
            args.push("-t".into());
            args.push(threads.to_string());
        }
        args.extend(p.extra_args.iter().cloned());
        args
    }

    async fn read(&self, t: &Telemetry) -> anyhow::Result<MinerStats> {
        let port = match t {
            Telemetry::TcpText { port } => *port,
            other => return Err(anyhow!("cpuminer-opt expects TcpText telemetry, got {other:?}")),
        };
        let raw = telemetry::fetch_tcp_text(port, "summary")
            .await
            .context("reading cpuminer-opt summary")?;
        parse_summary(&raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn profile() -> ResolvedProfile {
        ResolvedProfile {
            coin: "xmr".into(),
            algo: Algo::RandomX,
            binary_path: PathBuf::from("/bin/cpuminer"),
            pool_host: "pool.example".into(),
            pool_port: 4444,
            user: "4ADDR.rig1".into(),
            pass: "x".into(),
            tls: false,
            threads: Some(8),
            extra_args: vec![],
        }
    }

    #[test]
    fn builds_expected_args() {
        let args = CpuminerOpt.build_args(&profile(), &TelemetryBinding::loopback(4048));
        assert_eq!(
            args,
            vec![
                "-a", "rx0",
                "-o", "stratum+tcp://pool.example:4444",
                "-u", "4ADDR.rig1",
                "-p", "x",
                "--api-bind", "127.0.0.1:4048",
                "-t", "8",
            ]
        );
    }

    #[test]
    fn tls_uses_ssl_scheme() {
        let mut p = profile();
        p.tls = true;
        let args = CpuminerOpt.build_args(&p, &TelemetryBinding::loopback(4048));
        assert!(args.contains(&"stratum+ssl://pool.example:4444".to_string()));
    }

    #[test]
    fn parses_full_summary() {
        let raw = "NAME=cpuminer-opt;VER=3.23.0;API=1.0;ALGO=rx0;CPUS=8;URL=pool;\
                   HS=1234.50;KHS=1.23;ACC=12;REJ=1;SOL=0;ACCMN=0.250;DIFF=50000;\
                   TEMP=0.0;FAN=0;FREQ=0;UPTIME=3600;TS=1700000000|";
        let s = parse_summary(raw).unwrap();
        assert_eq!(s.hashrate_hs, 1234.50);
        assert_eq!(s.accepted, 12);
        assert_eq!(s.rejected, 1);
        assert_eq!(s.uptime_secs, 3600);
        assert_eq!(s.current_job_diff, Some(50000.0));
        assert!(s.connected);
    }

    #[test]
    fn falls_back_to_khs_when_hs_zero() {
        let raw = "NAME=cpuminer;HS=0.00;KHS=2.50;ACC=0;REJ=0;UPTIME=5|";
        let s = parse_summary(raw).unwrap();
        assert_eq!(s.hashrate_hs, 2_500.0);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_summary("").is_err());
        assert!(parse_summary("not a summary").is_err());
    }
}
