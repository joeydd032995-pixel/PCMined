//! Miner registry: static fee/source/license/telemetry metadata for the whole
//! roster, plus `resolve_default_miner` (prefer fee-free open source).
//!
//! Adapters for kawpowminer/ethminer/monitor land in P4; their metadata lives
//! here now so the registry, fee badges, and default resolution are complete.

use crate::miner::adapters::{cpuminer_opt, ethminer, kawpowminer, lolminer, monitor_device};
use crate::miner::MinerInfo;

/// Every miner known to the app (spawnable adapters + monitor-only). Each entry
/// is the canonical `INFO` const owned by its adapter — single source of truth.
pub fn all_miners() -> Vec<MinerInfo> {
    vec![
        cpuminer_opt::INFO,
        lolminer::INFO,
        kawpowminer::INFO,
        ethminer::INFO,
        monitor_device::INFO,
    ]
}

/// Look up miner metadata by id.
pub fn by_id(id: &str) -> Option<MinerInfo> {
    all_miners().into_iter().find(|m| m.id == id)
}

/// Candidate miners for a coin, in roster order.
fn candidates(coin_id: &str) -> &'static [&'static str] {
    match coin_id.to_ascii_lowercase().as_str() {
        "xmr" => &["cpuminer-opt"],
        "rvn" => &["kawpowminer"],
        "etc" => &["ethminer"],
        // lolMiner is auto-fetchable and covers these GPU algos.
        "kas" | "erg" | "kls" | "pyi" | "ethw" | "octa" | "clo" => &["lolminer"],
        "btc" | "ltc" | "doge" => &["monitor"],
        _ => &[],
    }
}

/// Choose the default miner for a coin: prefer an open-source, fee-free option;
/// otherwise the lowest disclosed dev fee. Returns `None` for unknown coins.
pub fn resolve_default_miner(coin_id: &str) -> Option<MinerInfo> {
    let cands: Vec<MinerInfo> = candidates(coin_id).iter().filter_map(|id| by_id(id)).collect();
    cands
        .iter()
        .find(|m| m.open_source && m.dev_fee_pct == 0.0)
        .or_else(|| {
            cands.iter().min_by(|a, b| {
                a.dev_fee_pct.partial_cmp(&b.dev_fee_pct).unwrap_or(std::cmp::Ordering::Equal)
            })
        })
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xmr_defaults_to_fee_free_cpuminer() {
        let m = resolve_default_miner("xmr").expect("xmr default");
        assert_eq!(m.id, "cpuminer-opt");
        assert_eq!(m.dev_fee_pct, 0.0);
        assert!(m.open_source);
    }

    #[test]
    fn kaspa_defaults_to_lolminer_with_disclosed_fee() {
        let m = resolve_default_miner("kas").expect("kas default");
        assert_eq!(m.id, "lolminer");
        assert!(m.dev_fee_pct > 0.0, "fee must be surfaced, not hidden");
    }

    #[test]
    fn registry_covers_roster() {
        assert!(by_id("cpuminer-opt").is_some());
        assert!(by_id("lolminer").is_some());
        assert!(by_id("kawpowminer").is_some());
        assert!(by_id("ethminer").is_some());
        assert!(by_id("monitor").is_some());
        assert!(by_id("ghost").is_none());
    }

    #[test]
    fn unknown_coin_has_no_default() {
        assert!(resolve_default_miner("zzz").is_none());
    }
}
