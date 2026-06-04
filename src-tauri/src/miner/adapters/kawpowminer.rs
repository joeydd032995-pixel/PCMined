//! kawpowminer adapter — KAWPOW (Ravencoin). Fee-free (0%), open source.
//! Telemetry: JSON-RPC over TCP (ethminer-style API).

use anyhow::anyhow;
use async_trait::async_trait;

use super::{ethminer_build_args, parse_ethminer_getstat1};
use crate::miner::telemetry::{self, Telemetry};
use crate::miner::{
    Algo, MinerAdapter, MinerInfo, MinerStats, ResolvedProfile, TelemetryBinding, TelemetryKind,
};

pub const INFO: MinerInfo = MinerInfo {
    id: "kawpowminer",
    display: "kawpowminer",
    algos: &[Algo::KawPow],
    dev_fee_pct: 0.0,
    open_source: true,
    source_url: "https://github.com/RavenCommunity/kawpowminer/releases",
    license: "GPL-3.0",
    telemetry_kind: TelemetryKind::TcpJsonRpc,
};

pub struct KawpowMiner;

#[async_trait]
impl MinerAdapter for KawpowMiner {
    fn info(&self) -> &MinerInfo {
        &INFO
    }

    fn build_args(&self, p: &ResolvedProfile, t: &TelemetryBinding) -> Vec<String> {
        ethminer_build_args(p, t)
    }

    async fn read(&self, t: &Telemetry) -> anyhow::Result<MinerStats> {
        let port = match t {
            Telemetry::TcpJsonRpc { port } => *port,
            other => return Err(anyhow!("kawpowminer expects TcpJsonRpc telemetry, got {other:?}")),
        };
        let body = telemetry::fetch_tcp_jsonrpc(port, "miner_getstat1").await?;
        parse_ethminer_getstat1(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_kawpow_only() {
        assert!(KawpowMiner.supports(Algo::KawPow));
        assert!(!KawpowMiner.supports(Algo::RandomX));
    }
}
