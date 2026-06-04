//! Per-coin saved mining profiles and their pool configuration.

use serde::{Deserialize, Serialize};

fn default_user_template() -> String {
    "{wallet}.{worker}".to_string()
}

fn default_pass() -> String {
    "x".to_string()
}

/// A pool endpoint with the template used to build the miner's `user` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoolConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_user_template")]
    pub user_template: String,
    #[serde(default = "default_pass")]
    pub pass: String,
    #[serde(default)]
    pub tls: bool,
}

impl PoolConfig {
    /// Render the miner `user` field from wallet + worker per the template.
    pub fn render_user(&self, wallet: &str, worker: &str) -> String {
        self.user_template.replace("{wallet}", wallet).replace("{worker}", worker)
    }
}

/// A saved profile for one coin: which wallet/worker, threads, and pools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    /// Public receiving ADDRESS only — never a private key.
    pub wallet: String,
    #[serde(default)]
    pub worker: String,
    #[serde(default)]
    pub threads: Option<u32>,
    pub pool: PoolConfig,
    #[serde(default)]
    pub fallback_pools: Vec<PoolConfig>,
}

/// All saved state for one coin.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoinConfig {
    /// Overrides the registry's default miner for this coin, if set.
    #[serde(default)]
    pub miner: Option<String>,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// User-added pools (validated via `test_pool` before being relied on).
    #[serde(default)]
    pub custom_pools: Vec<PoolConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_user_template() {
        let pool = PoolConfig {
            host: "p".into(),
            port: 1,
            user_template: "{wallet}.{worker}".into(),
            pass: "x".into(),
            tls: false,
        };
        assert_eq!(pool.render_user("4ADDR", "rig1"), "4ADDR.rig1");
    }
}
