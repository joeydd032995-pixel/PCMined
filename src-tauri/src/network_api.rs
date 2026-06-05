//! Live network data per coin (difficulty, network hashrate, block reward,
//! block time) with caching and graceful degradation.
//!
//! BTC is wired to mempool.space. Coins without a live source degrade to
//! registry-derived values that are clearly marked stale, so the UI never shows
//! unmarked guesses. Adding a live source = one `fetch_*` function.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::config::coins;

/// A snapshot of a coin's network state for the lottery odds view.
#[derive(Debug, Clone, Serialize)]
pub struct NetworkStats {
    pub coin: String,
    pub difficulty: f64,
    pub network_hashrate: f64,
    pub block_reward: f64,
    pub block_time: f64,
    pub fetched_at: u64,
    /// True when this is cached/derived data, not a fresh live read.
    pub stale: bool,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A block is found when the best share difficulty reaches network difficulty.
pub fn is_block_found(best_share_diff: f64, network_difficulty: f64) -> bool {
    network_difficulty > 0.0 && best_share_diff >= network_difficulty
}

/// BTC block subsidy (BTC) at a given height: 50 halving every 210,000 blocks.
pub fn btc_block_reward(height: u64) -> f64 {
    let halvings = height / 210_000;
    if halvings >= 64 {
        return 0.0;
    }
    50.0 / 2f64.powi(halvings as i32)
}

/// Parse mempool.space `/api/v1/mining/hashrate/3d` into (difficulty, hashrate).
pub fn parse_mempool_hashrate(body: &str) -> anyhow::Result<(f64, f64)> {
    let v: Value = serde_json::from_str(body).context("mempool hashrate was not JSON")?;
    let difficulty = v
        .get("currentDifficulty")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("missing currentDifficulty"))?;
    let hashrate = v
        .get("currentHashrate")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("missing currentHashrate"))?;
    Ok((difficulty, hashrate))
}

pub struct NetworkApi {
    client: reqwest::Client,
    cache: Mutex<HashMap<String, NetworkStats>>,
}

impl Default for NetworkApi {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkApi {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent("lottery-ticket-terminal")
            .build()
            .unwrap_or_default();
        NetworkApi { client, cache: Mutex::new(HashMap::new()) }
    }

    /// Get network stats for a coin, fetching live where possible and otherwise
    /// degrading to the last good cache (marked stale) or a registry placeholder.
    pub async fn get(&self, coin: &str) -> NetworkStats {
        match self.fetch(coin).await {
            Ok(stats) => {
                self.cache.lock().await.insert(coin.to_string(), stats.clone());
                stats
            }
            Err(e) => {
                tracing::debug!(%coin, error = %e, "live network fetch failed; degrading");
                if let Some(cached) = self.cache.lock().await.get(coin).cloned() {
                    NetworkStats { stale: true, ..cached }
                } else {
                    placeholder(coin)
                }
            }
        }
    }

    async fn fetch(&self, coin: &str) -> anyhow::Result<NetworkStats> {
        match coin.to_ascii_lowercase().as_str() {
            "btc" => self.fetch_btc().await,
            other => Err(anyhow!("no live network source for '{other}' yet")),
        }
    }

    async fn fetch_btc(&self) -> anyhow::Result<NetworkStats> {
        let body = self
            .client
            .get("https://mempool.space/api/v1/mining/hashrate/3d")
            .send()
            .await?
            .text()
            .await?;
        let (difficulty, network_hashrate) = parse_mempool_hashrate(&body)?;

        // Reward from current tip height (precise across halvings).
        let height: u64 = self
            .client
            .get("https://mempool.space/api/blocks/tip/height")
            .send()
            .await?
            .text()
            .await?
            .trim()
            .parse()
            .unwrap_or(0);
        let block_reward = if height > 0 {
            btc_block_reward(height)
        } else {
            coins::nominal_block_reward("btc")
        };

        Ok(NetworkStats {
            coin: "btc".to_string(),
            difficulty,
            network_hashrate,
            block_reward,
            block_time: coins::coin("btc").map(|c| c.block_time_secs).unwrap_or(600.0),
            fetched_at: now_secs(),
            stale: false,
        })
    }
}

/// Registry-derived placeholder when there is no cache and no live source.
fn placeholder(coin: &str) -> NetworkStats {
    let info = coins::coin(coin);
    NetworkStats {
        coin: coin.to_string(),
        difficulty: 0.0,
        network_hashrate: 0.0,
        block_reward: coins::nominal_block_reward(coin),
        block_time: info.map(|c| c.block_time_secs).unwrap_or(0.0),
        fetched_at: now_secs(),
        stale: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn btc_reward_halvings() {
        assert_eq!(btc_block_reward(0), 50.0);
        assert_eq!(btc_block_reward(210_000), 25.0);
        assert_eq!(btc_block_reward(630_000), 6.25);
        assert_eq!(btc_block_reward(840_000), 3.125); // 2024 halving
        assert_eq!(btc_block_reward(64 * 210_000), 0.0);
    }

    #[test]
    fn parses_mempool_hashrate() {
        let body = r#"{ "currentHashrate": 6.5e20, "currentDifficulty": 9.0e13, "hashrates": [] }"#;
        let (d, h) = parse_mempool_hashrate(body).unwrap();
        assert_eq!(d, 9.0e13);
        assert_eq!(h, 6.5e20);
    }

    #[test]
    fn rejects_bad_mempool_payload() {
        assert!(parse_mempool_hashrate("not json").is_err());
        assert!(parse_mempool_hashrate("{}").is_err());
    }

    #[test]
    fn placeholder_is_marked_stale() {
        let p = placeholder("btc");
        assert!(p.stale);
        assert_eq!(p.block_reward, 3.125);
        assert_eq!(p.block_time, 600.0);
    }

    #[test]
    fn block_found_threshold() {
        assert!(is_block_found(100.0, 100.0));
        assert!(is_block_found(150.0, 100.0));
        assert!(!is_block_found(99.9, 100.0));
        assert!(!is_block_found(100.0, 0.0));
    }
}
