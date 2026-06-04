//! Managed miner binaries: download-on-demand from official URLs, pinned
//! version, **SHA-256 verified before the file is ever made executable**.
//!
//! SECURITY: a downloaded artifact is hashed and compared to a pinned digest
//! while still in a temp file. Only on a match is it moved into the managed dir
//! and given the executable bit — a tampered or corrupted download is refused
//! before it can run. The hashes below must be filled with the real digest of
//! each pinned release at integration time (fail-closed until then).

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// A pinned, verifiable miner release.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ManagedBinary {
    pub miner_id: &'static str,
    pub version: &'static str,
    pub url: &'static str,
    /// Lowercase hex SHA-256 of the downloaded artifact. MUST be the real digest
    /// of the pinned release before enabling downloads in production.
    pub sha256: &'static str,
    pub filename: &'static str,
}

/// Pinned releases. URLs point at official release pages; digests are
/// placeholders (all-zero) and intentionally reject real downloads until a
/// maintainer pins the verified hash for the exact version.
pub static RELEASES: &[ManagedBinary] = &[
    ManagedBinary {
        miner_id: "cpuminer-opt",
        version: "3.23.0",
        url: "https://github.com/JayDDee/cpuminer-opt/releases",
        sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        filename: "cpuminer",
    },
    ManagedBinary {
        miner_id: "lolminer",
        version: "1.95",
        url: "https://github.com/Lolliedieb/lolMiner-releases/releases",
        sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        filename: "lolMiner",
    },
];

/// Look up the pinned release for a miner.
pub fn release_for(miner_id: &str) -> Option<&'static ManagedBinary> {
    RELEASES.iter().find(|r| r.miner_id == miner_id)
}

/// Information about an available update for an installed miner.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub miner_id: String,
    pub installed: Option<String>,
    pub available: String,
    pub update_available: bool,
}

/// Compute the lowercase hex SHA-256 of a file.
pub fn file_sha256(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {path:?}"))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

/// Verify a file matches an expected SHA-256 (case-insensitive hex). Returns an
/// error describing the mismatch — the caller must NOT execute on error.
pub fn verify_file(path: &Path, expected_sha256: &str) -> anyhow::Result<()> {
    let actual = file_sha256(path)?;
    if actual.eq_ignore_ascii_case(expected_sha256) {
        Ok(())
    } else {
        bail!("SHA-256 mismatch: expected {expected_sha256}, got {actual}")
    }
}

/// Is `available` a newer semver than `installed`?
pub fn is_update_available(installed: Option<&str>, available: &str) -> bool {
    let avail = match semver::Version::parse(available) {
        Ok(v) => v,
        Err(_) => return false,
    };
    match installed.and_then(|s| semver::Version::parse(s).ok()) {
        Some(inst) => avail > inst,
        None => true, // nothing installed → an install is "available"
    }
}

/// Manages the on-disk directory of verified miner binaries.
pub struct BinaryManager {
    dir: PathBuf,
    client: reqwest::Client,
}

impl BinaryManager {
    pub fn new(dir: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .user_agent("lottery-ticket-terminal")
            .build()
            .unwrap_or_default();
        BinaryManager { dir, client }
    }

    pub fn path_for(&self, filename: &str) -> PathBuf {
        self.dir.join(filename)
    }

    pub fn is_installed(&self, miner_id: &str) -> bool {
        release_for(miner_id).map(|r| self.path_for(r.filename).exists()).unwrap_or(false)
    }

    /// Download, verify, and install the pinned binary for `miner_id`. The
    /// artifact is hashed in a temp file and only moved into place + made
    /// executable if the SHA-256 matches the pinned digest.
    pub async fn fetch(&self, miner_id: &str) -> anyhow::Result<PathBuf> {
        let release = release_for(miner_id)
            .ok_or_else(|| anyhow!("no pinned release for miner '{miner_id}'"))?;

        std::fs::create_dir_all(&self.dir).with_context(|| format!("creating {:?}", self.dir))?;

        let bytes = self
            .client
            .get(release.url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let tmp = self.dir.join(format!("{}.download", release.filename));
        std::fs::write(&tmp, &bytes)?;

        // Verify BEFORE the file is ever placed/executed.
        if let Err(e) = verify_file(&tmp, release.sha256) {
            let _ = std::fs::remove_file(&tmp);
            bail!("refusing to install unverified binary: {e}");
        }

        let dest = self.path_for(release.filename);
        std::fs::rename(&tmp, &dest)?;
        set_executable(&dest)?;
        Ok(dest)
    }

    /// Check for updates across all pinned miners given currently installed
    /// versions (installed version tracking is best-effort; here we report based
    /// on presence + pinned version).
    pub fn check_updates(&self) -> Vec<UpdateInfo> {
        RELEASES
            .iter()
            .map(|r| {
                let installed = if self.is_installed(r.miner_id) {
                    // We don't yet persist installed versions; treat presence as
                    // "the pinned version" so a version bump shows as an update.
                    Some(r.version.to_string())
                } else {
                    None
                };
                UpdateInfo {
                    miner_id: r.miner_id.to_string(),
                    update_available: is_update_available(installed.as_deref(), r.version),
                    installed,
                    available: r.version.to_string(),
                }
            })
            .collect()
    }
}

#[cfg(unix)]
fn set_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(tag: &str, content: &[u8]) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("pcmined-bin-{tag}-{nanos}.bin"));
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn verify_accepts_correct_hash_and_rejects_tamper() {
        let path = temp_file("verify", b"hello miner");
        // SHA-256 of "hello miner".
        let expected = file_sha256(&path).unwrap();
        assert!(verify_file(&path, &expected).is_ok());
        // Uppercase hex is accepted (case-insensitive).
        assert!(verify_file(&path, &expected.to_uppercase()).is_ok());

        // Tamper a byte → verification must fail (pre-exec refusal).
        std::fs::write(&path, b"hello miner!").unwrap();
        assert!(verify_file(&path, &expected).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn known_sha256_vector() {
        // SHA-256("abc") is a well-known test vector.
        let path = temp_file("abc", b"abc");
        assert_eq!(
            file_sha256(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn semver_update_logic() {
        assert!(is_update_available(Some("1.0.0"), "1.0.1"));
        assert!(is_update_available(Some("1.0.0"), "2.0.0"));
        assert!(!is_update_available(Some("1.2.0"), "1.2.0"));
        assert!(!is_update_available(Some("2.0.0"), "1.9.9"));
        assert!(is_update_available(None, "1.0.0")); // nothing installed
        assert!(!is_update_available(Some("1.0.0"), "not-semver"));
    }

    #[test]
    fn releases_have_pinned_entries() {
        assert!(release_for("cpuminer-opt").is_some());
        assert!(release_for("lolminer").is_some());
        assert!(release_for("ghost").is_none());
    }
}
