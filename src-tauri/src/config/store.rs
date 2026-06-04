//! JSON config persistence: schema versioning, migrate-on-load, atomic writes,
//! and a `.bak` backup so a bad write can never destroy a working config.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::profiles::{CoinConfig, PoolConfig, Profile};

/// Current config schema version. Bump when the shape changes and add a step to
/// [`migrate`].
pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub binaries_dir: Option<String>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_true() -> bool {
    true
}
fn default_log_level() -> String {
    "info".to_string()
}

impl Default for GlobalSettings {
    fn default() -> Self {
        GlobalSettings { minimize_to_tray: true, binaries_dir: None, log_level: default_log_level() }
    }
}

/// The full persisted configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub global: GlobalSettings,
    #[serde(default)]
    pub coins: HashMap<String, CoinConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Config { version: SCHEMA_VERSION, global: GlobalSettings::default(), coins: HashMap::new() }
    }
}

/// Migrate a raw parsed config value up to [`SCHEMA_VERSION`].
///
/// Unknown/older versions are upgraded field-by-field; serde defaults fill any
/// gaps. A config that fails to parse entirely falls back to defaults (the
/// caller keeps the original file as a `.bak`).
pub fn migrate(mut value: serde_json::Value) -> Config {
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version < SCHEMA_VERSION as u64 {
        // v0/v1 → v2: no field renames so far; just stamp the version and let
        // serde defaults backfill any newly added fields.
        if let Some(obj) = value.as_object_mut() {
            obj.insert("version".into(), serde_json::json!(SCHEMA_VERSION));
        }
    }
    serde_json::from_value(value).unwrap_or_default()
}

/// Loaded config plus its on-disk location.
pub struct ConfigStore {
    path: PathBuf,
    config: Config,
}

impl ConfigStore {
    /// Load from `path`, migrating on the way in. A missing file yields defaults;
    /// a corrupt file is preserved as `.corrupt` and defaults are used.
    pub fn load(path: PathBuf) -> Self {
        let config = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(value) => migrate(value),
                Err(e) => {
                    tracing::warn!(?path, error = %e, "config is corrupt; preserving as .corrupt and using defaults");
                    let _ = std::fs::rename(&path, path.with_extension("corrupt"));
                    Config::default()
                }
            },
            Err(_) => Config::default(),
        };
        ConfigStore { path, config }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Atomically persist the current config, keeping a `.bak` of the previous
    /// good file. Write to a temp sibling, fsync, then rename over the target.
    pub fn save(&self) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&self.config)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        atomic_write(&self.path, json.as_bytes())
    }

    /// Profiles saved for a coin (empty if none).
    pub fn profiles(&self, coin: &str) -> Vec<Profile> {
        self.config.coins.get(coin).map(|c| c.profiles.clone()).unwrap_or_default()
    }

    /// Insert or replace a profile by name, then persist.
    pub fn save_profile(&mut self, coin: &str, profile: Profile) -> std::io::Result<()> {
        let entry = self.config.coins.entry(coin.to_string()).or_default();
        if let Some(existing) = entry.profiles.iter_mut().find(|p| p.name == profile.name) {
            *existing = profile;
        } else {
            entry.profiles.push(profile);
        }
        self.save()
    }

    /// Add a user-defined pool for a coin (deduped by host:port), then persist.
    pub fn add_custom_pool(&mut self, coin: &str, pool: PoolConfig) -> std::io::Result<()> {
        let entry = self.config.coins.entry(coin.to_string()).or_default();
        if !entry.custom_pools.iter().any(|p| p.host == pool.host && p.port == pool.port) {
            entry.custom_pools.push(pool);
        }
        self.save()
    }
}

/// Write `bytes` to `path` atomically, backing up any existing file to `.bak`.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        let _ = std::fs::copy(path, path.with_extension("bak"));
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profiles::PoolConfig;

    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("pcmined-test-{tag}-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p.push("config.json");
        p
    }

    fn sample_profile() -> Profile {
        Profile {
            name: "Default".into(),
            wallet: "4ADDR".into(),
            worker: "rig1".into(),
            threads: Some(8),
            pool: PoolConfig {
                host: "pool.example".into(),
                port: 4444,
                user_template: "{wallet}.{worker}".into(),
                pass: "x".into(),
                tls: false,
            },
            fallback_pools: vec![],
        }
    }

    #[test]
    fn profiles_persist_across_reload() {
        let path = temp_path("persist");
        let mut store = ConfigStore::load(path.clone());
        store.save_profile("xmr", sample_profile()).unwrap();

        let reloaded = ConfigStore::load(path);
        let profiles = reloaded.profiles("xmr");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "Default");
        assert_eq!(profiles[0].wallet, "4ADDR");
    }

    #[test]
    fn save_creates_backup_of_previous() {
        let path = temp_path("backup");
        let mut store = ConfigStore::load(path.clone());
        store.save().unwrap(); // first write, no backup yet
        store.save_profile("kas", sample_profile()).unwrap(); // second write → .bak
        assert!(path.with_extension("bak").exists());
    }

    #[test]
    fn migrate_stamps_version_and_keeps_data() {
        let legacy = serde_json::json!({
            "version": 1,
            "coins": { "xmr": { "profiles": [] } }
        });
        let cfg = migrate(legacy);
        assert_eq!(cfg.version, SCHEMA_VERSION);
        assert!(cfg.coins.contains_key("xmr"));
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let path = temp_path("corrupt");
        std::fs::write(&path, b"{not valid json").unwrap();
        let store = ConfigStore::load(path.clone());
        assert_eq!(store.config().version, SCHEMA_VERSION);
        assert!(path.with_extension("corrupt").exists());
    }
}
