//! Managed miner binaries: download-on-demand from official URLs, pinned
//! version, **SHA-256 verified before extraction/execution**.
//!
//! SECURITY: a downloaded archive is hashed and compared to a pinned digest
//! while still in a temp file. Only on a match is the miner binary extracted,
//! placed in the managed dir, and given the executable bit — a tampered or
//! corrupted download is refused before anything runs.
//!
//! The pinned digests below were computed from the actual official release
//! artifacts (see each entry). Bump the version + url + sha256 together when
//! pinning a new release.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// How the downloaded artifact is packaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ArchiveKind {
    /// gzip-compressed tar (Linux/macOS).
    TarGz,
    /// zip (Windows).
    Zip,
    /// the download is the bare binary (no archive).
    Raw,
}

/// A pinned, verifiable miner release for one platform.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ManagedBinary {
    pub miner_id: &'static str,
    /// Target OS as `std::env::consts::OS` ("linux", "windows", "macos").
    pub os: &'static str,
    /// Target arch as `std::env::consts::ARCH` ("x86_64", "aarch64").
    pub arch: &'static str,
    pub version: &'static str,
    /// Direct download URL of the official release artifact.
    pub url: &'static str,
    /// Lowercase hex SHA-256 of the downloaded artifact.
    pub sha256: &'static str,
    pub archive: ArchiveKind,
    /// Path of the binary inside the archive (ignored for `Raw`).
    pub member: &'static str,
    /// Output filename in the managed dir.
    pub filename: &'static str,
}

/// Pinned releases. Digests computed from the official GitHub release assets.
///
/// lolMiner ships prebuilt binaries with a permissive freeware license. Other
/// miners (e.g. cpuminer-opt) do not publish prebuilt Linux binaries on their
/// official releases, so they are installed manually rather than auto-fetched.
pub static RELEASES: &[ManagedBinary] = &[
    ManagedBinary {
        miner_id: "lolminer",
        os: "linux",
        arch: "x86_64",
        version: "1.98.0", // upstream tag "1.98a"
        url: "https://github.com/Lolliedieb/lolMiner-releases/releases/download/1.98a/lolMiner_v1.98a_Lin64.tar.gz",
        sha256: "0b8078299654a12846e4967f1db3506409cfb8b1031687a910965d1a99c6f270",
        archive: ArchiveKind::TarGz,
        member: "1.98a/lolMiner",
        filename: "lolMiner",
    },
    ManagedBinary {
        miner_id: "lolminer",
        os: "windows",
        arch: "x86_64",
        version: "1.98.0",
        url: "https://github.com/Lolliedieb/lolMiner-releases/releases/download/1.98a/lolMiner_v1.98a_Win64.zip",
        sha256: "f2bbda2d2255155d50935967b8c55105b9aeefbd27cda3e8d01beaf535a16762",
        archive: ArchiveKind::Zip,
        member: "1.98a/lolMiner.exe",
        filename: "lolMiner.exe",
    },
];

/// The pinned release for a miner on a specific platform.
pub fn release_for_platform(miner_id: &str, os: &str, arch: &str) -> Option<&'static ManagedBinary> {
    RELEASES.iter().find(|r| r.miner_id == miner_id && r.os == os && r.arch == arch)
}

/// The pinned release for a miner on the *current* platform.
pub fn current_release(miner_id: &str) -> Option<&'static ManagedBinary> {
    release_for_platform(miner_id, std::env::consts::OS, std::env::consts::ARCH)
}

/// Whether the app can auto-fetch a verified binary for `miner_id` here.
pub fn is_fetchable(miner_id: &str) -> bool {
    current_release(miner_id).is_some()
}

/// Information about an available update for an installed miner.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub miner_id: String,
    pub installed: Option<String>,
    pub available: Option<String>,
    pub update_available: bool,
    pub fetchable: bool,
}

/// Compute the lowercase hex SHA-256 of a file.
pub fn file_sha256(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {path:?}"))?;
    Ok(sha256_hex(&bytes))
}

/// Compute the lowercase hex SHA-256 of a byte slice.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Verify a file matches an expected SHA-256 (case-insensitive hex). Returns an
/// error describing the mismatch — the caller must NOT proceed on error.
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
        None => true,
    }
}

/// Extract a single member from a .tar.gz into `dest`.
fn extract_tar_gz(archive: &Path, member: &str, dest: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::open(archive)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    for entry in tar.entries()? {
        let mut entry = entry?;
        if entry.path()?.to_string_lossy() == member {
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    bail!("member {member:?} not found in archive")
}

/// Extract a single member from a .zip into `dest`.
fn extract_zip(archive: &Path, member: &str, dest: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let mut entry = zip
        .by_name(member)
        .with_context(|| format!("member {member:?} not found in zip"))?;
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf)?;
    std::fs::write(dest, &buf)?;
    Ok(())
}

/// Manages the on-disk directory of verified miner binaries.
pub struct BinaryManager {
    dir: PathBuf,
    client: reqwest::Client,
}

impl BinaryManager {
    pub fn new(dir: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .user_agent("lottery-ticket-terminal")
            .build()
            .unwrap_or_default();
        BinaryManager { dir, client }
    }

    pub fn path_for(&self, filename: &str) -> PathBuf {
        self.dir.join(filename)
    }

    pub fn is_installed(&self, miner_id: &str) -> bool {
        current_release(miner_id).map(|r| self.path_for(r.filename).exists()).unwrap_or(false)
    }

    /// Download, verify, and install the pinned binary for `miner_id` on the
    /// current platform. The archive is hashed in a temp file and only extracted
    /// + made executable if the SHA-256 matches the pinned digest.
    pub async fn fetch(&self, miner_id: &str) -> anyhow::Result<PathBuf> {
        let release = current_release(miner_id).ok_or_else(|| {
            anyhow!(
                "no pinned release for '{miner_id}' on {}/{} — install it manually",
                std::env::consts::OS,
                std::env::consts::ARCH
            )
        })?;

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

        // Verify BEFORE extracting or executing anything.
        if let Err(e) = verify_file(&tmp, release.sha256) {
            let _ = std::fs::remove_file(&tmp);
            bail!("refusing to install unverified binary: {e}");
        }

        let dest = self.path_for(release.filename);
        let result = match release.archive {
            ArchiveKind::TarGz => extract_tar_gz(&tmp, release.member, &dest),
            ArchiveKind::Zip => extract_zip(&tmp, release.member, &dest),
            ArchiveKind::Raw => std::fs::rename(&tmp, &dest).map_err(Into::into),
        };
        // Clean up the archive (rename already consumed it for Raw).
        if release.archive != ArchiveKind::Raw {
            let _ = std::fs::remove_file(&tmp);
        }
        result?;
        set_executable(&dest)?;
        Ok(dest)
    }

    /// Report fetchability + update status for every miner that has a pinned
    /// release on this platform.
    pub fn check_updates(&self) -> Vec<UpdateInfo> {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for r in RELEASES {
            if !seen.insert(r.miner_id) {
                continue;
            }
            let rel = current_release(r.miner_id);
            let available = rel.map(|x| x.version.to_string());
            let installed = if self.is_installed(r.miner_id) {
                available.clone()
            } else {
                None
            };
            out.push(UpdateInfo {
                miner_id: r.miner_id.to_string(),
                update_available: is_update_available(
                    installed.as_deref(),
                    available.as_deref().unwrap_or("0.0.0"),
                ),
                installed,
                available,
                fetchable: rel.is_some(),
            });
        }
        out
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
    use std::io::Write;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("pcmined-bin-{tag}-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn verify_accepts_correct_hash_and_rejects_tamper() {
        let dir = temp_dir("verify");
        let path = dir.join("a.bin");
        std::fs::write(&path, b"hello miner").unwrap();
        let expected = file_sha256(&path).unwrap();
        assert!(verify_file(&path, &expected).is_ok());
        assert!(verify_file(&path, &expected.to_uppercase()).is_ok());
        std::fs::write(&path, b"hello miner!").unwrap();
        assert!(verify_file(&path, &expected).is_err());
    }

    #[test]
    fn known_sha256_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn semver_update_logic() {
        assert!(is_update_available(Some("1.0.0"), "1.0.1"));
        assert!(!is_update_available(Some("1.2.0"), "1.2.0"));
        assert!(!is_update_available(Some("2.0.0"), "1.9.9"));
        assert!(is_update_available(None, "1.0.0"));
    }

    #[test]
    fn pinned_lolminer_releases_present() {
        assert!(release_for_platform("lolminer", "linux", "x86_64").is_some());
        assert!(release_for_platform("lolminer", "windows", "x86_64").is_some());
        assert!(release_for_platform("lolminer", "plan9", "x86_64").is_none());
        // Pinned hashes are real 64-hex-char digests, not placeholders.
        for r in RELEASES {
            assert_eq!(r.sha256.len(), 64);
            assert!(r.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert_ne!(r.sha256, "0".repeat(64));
        }
    }

    #[test]
    fn extracts_member_from_tar_gz_after_verify() {
        // Build a tiny .tar.gz containing dir/bin with known content.
        let dir = temp_dir("targz");
        let archive = dir.join("a.tar.gz");
        {
            let f = std::fs::File::create(&archive).unwrap();
            let enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
            let mut tar = tar::Builder::new(enc);
            let content = b"#!/bin/true\n";
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, "pkg/lolMiner", &content[..]).unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        let dest = dir.join("lolMiner");
        extract_tar_gz(&archive, "pkg/lolMiner", &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"#!/bin/true\n");
        // Missing member is an error.
        assert!(extract_tar_gz(&archive, "pkg/nope", &dest).is_err());
    }

    #[test]
    fn extracts_member_from_zip() {
        let dir = temp_dir("zip");
        let archive = dir.join("a.zip");
        {
            let f = std::fs::File::create(&archive).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts: zip::write::FileOptions<()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zw.start_file("pkg/lolMiner.exe", opts).unwrap();
            zw.write_all(b"MZfake").unwrap();
            zw.finish().unwrap();
        }
        let dest = dir.join("lolMiner.exe");
        extract_zip(&archive, "pkg/lolMiner.exe", &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"MZfake");
    }

    /// End-to-end against the real pinned lolMiner Linux release: download,
    /// verify the real SHA-256, extract the binary, and confirm it lands
    /// executable. Network + ~14 MB; run explicitly:
    ///   cargo test -- --ignored real_fetch_lolminer
    #[test]
    #[ignore]
    fn real_fetch_lolminer() {
        if std::env::consts::OS != "linux" || std::env::consts::ARCH != "x86_64" {
            return;
        }
        let dir = temp_dir("realfetch");
        let mgr = BinaryManager::new(dir.clone());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let path = rt.block_on(mgr.fetch("lolminer")).expect("fetch lolMiner");
        assert!(path.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0, "binary should be executable");
        }
        assert!(std::fs::metadata(&path).unwrap().len() > 1_000_000);
    }
}
