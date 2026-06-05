//! ethminer / ETChash fork adapter — ETCHash (Ethereum Classic). Fee-free (0%),
//! open source, but aging (flagged in the UI). Telemetry: JSON-RPC over TCP.

use anyhow::anyhow;
use async_trait::async_trait;

use super::{ethminer_build_args, parse_ethminer_getstat1};
use crate::miner::telemetry::{self, Telemetry};
use crate::miner::{
    Algo, MinerAdapter, MinerInfo, MinerStats, ResolvedProfile, TelemetryBinding, TelemetryKind,
};

pub const INFO: MinerInfo = MinerInfo {
    id: "ethminer",
    display: "ethminer (ETChash fork)",
    algos: &[Algo::EtcHash],
    dev_fee_pct: 0.0,
    open_source: true,
    source_url: "https://github.com/etc-mining/ethminer/releases",
    license: "GPL-3.0",
    telemetry_kind: TelemetryKind::TcpJsonRpc,
};

pub struct EthMiner;

#[async_trait]
impl MinerAdapter for EthMiner {
    fn info(&self) -> &MinerInfo {
        &INFO
    }

    fn build_args(&self, p: &ResolvedProfile, t: &TelemetryBinding) -> Vec<String> {
        ethminer_build_args(p, t)
    }

    async fn read(&self, t: &Telemetry) -> anyhow::Result<MinerStats> {
        let port = match t {
            Telemetry::TcpJsonRpc { port } => *port,
            other => return Err(anyhow!("ethminer expects TcpJsonRpc telemetry, got {other:?}")),
        };
        let body = telemetry::fetch_tcp_jsonrpc(port, "miner_getstat1").await?;
        parse_ethminer_getstat1(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn builds_stratum_p_url() {
        let p = ResolvedProfile {
            coin: "etc".into(),
            algo: Algo::EtcHash,
            binary_path: PathBuf::from("/bin/ethminer"),
            pool_host: "etc.pool".into(),
            pool_port: 4444,
            user: "0xADDR.rig1".into(),
            pass: "x".into(),
            tls: false,
            threads: None,
            extra_args: vec![],
        };
        let args = EthMiner.build_args(&p, &TelemetryBinding::loopback(3333));
        assert_eq!(
            args,
            vec!["-P", "stratum+tcp://0xADDR.rig1@etc.pool:4444", "--api-port", "3333"]
        );
    }
}
