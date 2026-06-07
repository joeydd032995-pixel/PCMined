//! Coin registry: per-coin algorithm, address rules, default solo pools, and the
//! algo-specific difficulty→hashrate conversion.
//!
//! Difficulty→hashrate lives HERE, never as a global constant, because the
//! relationship differs by algorithm family.

use serde::Serialize;

use crate::miner::Algo;

/// A default pool endpoint. Pools are runtime-verified data (use `test_pool`
/// before relying on one), not hard guarantees — endpoints rot over time.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PoolDef {
    pub name: &'static str,
    pub host: &'static str,
    pub port: u16,
    pub tls: bool,
    /// Pool fee on a found block, if any (separate from miner dev fee).
    pub fee_pct: f32,
    pub user_template: &'static str,
}

/// Static metadata for a supported coin.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CoinInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub algo: Algo,
    /// True for ASIC/device coins we only monitor (no local miner process).
    pub monitor_only: bool,
    /// Nominal target block interval in seconds (for lottery odds).
    pub block_time_secs: f64,
    pub default_pools: &'static [PoolDef],
    /// Suggested consumer wallet for receiving this coin (a desktop webview
    /// can't connect to a browser-extension wallet, so we suggest + link).
    pub wallet_name: &'static str,
    pub wallet_url: &'static str,
}

const NO_POOLS: &[PoolDef] = &[];

// Solo pool defaults. Kept intentionally small and clearly labeled — they are
// starting points the user verifies with `test_pool`, not endorsements.
const BTC_POOLS: &[PoolDef] = &[
    PoolDef {
        name: "Public Pool (solo, 0%)",
        host: "public-pool.io",
        port: 21496,
        tls: false,
        fee_pct: 0.0,
        user_template: "{wallet}.{worker}",
    },
    PoolDef {
        name: "Solo CKPool (~2% on block)",
        host: "solo.ckpool.org",
        port: 3333,
        tls: false,
        fee_pct: 2.0,
        user_template: "{wallet}.{worker}",
    },
];

// HeroMiners runs solo mode via a `solo:` prefix on the wallet (same host/port
// as the pool). 2Miners runs solo via dedicated `solo-<coin>` hosts. Both are
// runtime-verifiable with `test_pool` — endpoints/fees drift, so confirm before
// relying on one.
const KAS_POOLS: &[PoolDef] = &[PoolDef {
    name: "HeroMiners Kaspa (SOLO, ~0.9%)",
    host: "kaspa.herominers.com",
    port: 1206,
    tls: false,
    fee_pct: 0.9,
    user_template: "solo:{wallet}.{worker}",
}];

const ERG_POOLS: &[PoolDef] = &[PoolDef {
    name: "2Miners Ergo (SOLO, ~1.5%)",
    host: "solo-erg.2miners.com",
    port: 8888,
    tls: false,
    fee_pct: 1.5,
    user_template: "{wallet}.{worker}",
}];

const RVN_POOLS: &[PoolDef] = &[PoolDef {
    name: "2Miners Ravencoin (SOLO, ~1.5%)",
    host: "solo-rvn.2miners.com",
    port: 6060,
    tls: false,
    fee_pct: 1.5,
    user_template: "{wallet}.{worker}",
}];

const ETC_POOLS: &[PoolDef] = &[PoolDef {
    name: "2Miners Ethereum Classic (SOLO, ~1.5%)",
    host: "solo-etc.2miners.com",
    port: 1010,
    tls: false,
    fee_pct: 1.5,
    user_template: "{wallet}.{worker}",
}];

const W_METAMASK: &str = "https://metamask.io/download/";

pub static COINS: &[CoinInfo] = &[
    CoinInfo { id: "xmr", name: "Monero", algo: Algo::RandomX, monitor_only: false, block_time_secs: 120.0, default_pools: NO_POOLS, wallet_name: "Cake Wallet / Monero GUI", wallet_url: "https://www.getmonero.org/downloads/" },
    CoinInfo { id: "rvn", name: "Ravencoin", algo: Algo::KawPow, monitor_only: false, block_time_secs: 60.0, default_pools: RVN_POOLS, wallet_name: "Ravencoin Core", wallet_url: "https://ravencoin.org/wallet/" },
    CoinInfo { id: "etc", name: "Ethereum Classic", algo: Algo::EtcHash, monitor_only: false, block_time_secs: 13.0, default_pools: ETC_POOLS, wallet_name: "MetaMask", wallet_url: W_METAMASK },
    CoinInfo { id: "ethw", name: "EthereumPoW", algo: Algo::Ethash, monitor_only: false, block_time_secs: 13.0, default_pools: NO_POOLS, wallet_name: "MetaMask", wallet_url: W_METAMASK },
    CoinInfo { id: "octa", name: "Octa Space", algo: Algo::Ethash, monitor_only: false, block_time_secs: 16.0, default_pools: NO_POOLS, wallet_name: "MetaMask (custom RPC)", wallet_url: W_METAMASK },
    CoinInfo { id: "clo", name: "Callisto", algo: Algo::Ethash, monitor_only: false, block_time_secs: 13.0, default_pools: NO_POOLS, wallet_name: "MetaMask (custom RPC)", wallet_url: W_METAMASK },
    CoinInfo { id: "kas", name: "Kaspa", algo: Algo::KHeavyHash, monitor_only: false, block_time_secs: 1.0, default_pools: KAS_POOLS, wallet_name: "Kasware", wallet_url: "https://www.kasware.xyz/" },
    CoinInfo { id: "kls", name: "Karlsen", algo: Algo::KarlsenHash, monitor_only: false, block_time_secs: 1.0, default_pools: NO_POOLS, wallet_name: "Karlsen Wallet", wallet_url: "https://karlsencoin.com/" },
    CoinInfo { id: "pyi", name: "Pyrin", algo: Algo::PyrinHash, monitor_only: false, block_time_secs: 1.0, default_pools: NO_POOLS, wallet_name: "Pyrin Wallet", wallet_url: "https://pyrin.network/" },
    CoinInfo { id: "erg", name: "Ergo", algo: Algo::Autolykos2, monitor_only: false, block_time_secs: 120.0, default_pools: ERG_POOLS, wallet_name: "Nautilus", wallet_url: "https://nautiluswallet.com/" },
    CoinInfo { id: "btc", name: "Bitcoin", algo: Algo::Sha256d, monitor_only: true, block_time_secs: 600.0, default_pools: BTC_POOLS, wallet_name: "Sparrow / Electrum", wallet_url: "https://electrum.org/" },
    CoinInfo { id: "ltc", name: "Litecoin", algo: Algo::Scrypt, monitor_only: true, block_time_secs: 150.0, default_pools: NO_POOLS, wallet_name: "Litecoin Core / Electrum-LTC", wallet_url: "https://litecoin.org/" },
    CoinInfo { id: "doge", name: "Dogecoin", algo: Algo::Scrypt, monitor_only: true, block_time_secs: 60.0, default_pools: NO_POOLS, wallet_name: "Dogecoin Core / MyDoge", wallet_url: "https://dogecoin.com/" },
];

/// Look up a coin by id (case-insensitive).
pub fn coin(id: &str) -> Option<&'static CoinInfo> {
    let id = id.to_ascii_lowercase();
    COINS.iter().find(|c| c.id == id)
}

/// Nominal current block reward (coin units) used as a fallback when a live
/// source is unavailable. These drift (halvings, deflation schedules) — a live
/// fetcher overrides them where one exists (e.g. BTC reward from tip height).
pub fn nominal_block_reward(coin_id: &str) -> f64 {
    match coin_id.to_ascii_lowercase().as_str() {
        "btc" => 3.125,
        "ltc" => 6.25,
        "doge" => 10_000.0,
        "xmr" => 0.6,
        "rvn" => 2_500.0,
        "etc" => 2.56,
        "ethw" => 2.0,
        "octa" => 0.42,
        "clo" => 39.0,
        "kas" => 75.0,
        "kls" => 28.0,
        "pyi" => 50.0,
        "erg" => 3.0,
        _ => 0.0,
    }
}

/// Estimated network hashrate (H/s) implied by a difficulty and block time.
/// The conversion is algo-family specific — kept here, never global.
pub fn network_hashrate_from_difficulty(algo: Algo, difficulty: f64, block_time_secs: f64) -> f64 {
    if block_time_secs <= 0.0 || difficulty <= 0.0 {
        return 0.0;
    }
    match algo {
        // Bitcoin/Litecoin-style: each unit of difficulty ~ 2^32 hashes of work.
        Algo::Sha256d | Algo::Scrypt => difficulty * 4_294_967_296.0 / block_time_secs,
        // RandomX, ethash-family, kHeavyHash, Autolykos2: difficulty is already
        // expressed as expected hashes per block.
        _ => difficulty / block_time_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_coins_present() {
        for id in ["xmr", "rvn", "etc", "kas", "erg", "btc", "ltc", "doge"] {
            assert!(coin(id).is_some(), "missing coin {id}");
        }
        assert!(coin("nope").is_none());
        assert!(coin("BTC").is_some(), "lookup is case-insensitive");
    }

    #[test]
    fn sha256_difficulty_to_hashrate_matches_hand_calc() {
        // BTC: difficulty D over 600s blocks → D * 2^32 / 600 H/s.
        let d = 1.0e14;
        let h = network_hashrate_from_difficulty(Algo::Sha256d, d, 600.0);
        assert!((h - d * 4_294_967_296.0 / 600.0).abs() < 1.0);
    }

    #[test]
    fn randomx_uses_direct_ratio() {
        assert_eq!(network_hashrate_from_difficulty(Algo::RandomX, 1200.0, 120.0), 10.0);
    }

    #[test]
    fn zero_inputs_are_safe() {
        assert_eq!(network_hashrate_from_difficulty(Algo::Sha256d, 1.0, 0.0), 0.0);
        assert_eq!(network_hashrate_from_difficulty(Algo::Sha256d, 0.0, 600.0), 0.0);
    }
}
