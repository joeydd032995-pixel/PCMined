//! Telemetry transport: how to reach a running miner's stats API, plus shared
//! low-level fetch helpers. Parsing into `MinerStats` is each adapter's job.

use std::time::Duration;

use anyhow::Context;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Concrete telemetry connection for a running instance. The port matches what
/// the adapter wired into the miner's CLI args.
#[derive(Debug, Clone)]
pub enum Telemetry {
    /// HTTP+JSON (e.g. lolMiner): GET `http://127.0.0.1:port{path}`.
    Http { port: u16, path: String, token: Option<String> },
    /// JSON-RPC over a raw TCP socket (ethminer / kawpowminer). Wired in P4.
    TcpJsonRpc { port: u16 },
    /// Line/`;`-delimited text over TCP (cpuminer-opt API).
    TcpText { port: u16 },
    /// Last-resort: parse stdout with regex (no socket).
    Stdout,
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const READ_TIMEOUT: Duration = Duration::from_secs(4);

/// Send `command` to a cgminer/cpuminer-style text API on loopback and return
/// the raw response. cpuminer-opt terminates the summary record with `|`.
pub async fn fetch_tcp_text(port: u16, command: &str) -> anyhow::Result<String> {
    let connect = TcpStream::connect(("127.0.0.1", port));
    let mut stream = tokio::time::timeout(CONNECT_TIMEOUT, connect)
        .await
        .context("telemetry connect timed out")?
        .context("telemetry connect failed")?;

    stream.write_all(command.as_bytes()).await?;
    stream.flush().await?;

    let mut buf = Vec::with_capacity(512);
    let read = stream.read_to_end(&mut buf);
    // Some API variants hold the socket open; cap the read so we don't block.
    let _ = tokio::time::timeout(READ_TIMEOUT, read).await;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// GET a miner's HTTP telemetry endpoint and return the raw body.
pub async fn fetch_http(port: u16, path: &str, token: Option<&str>) -> anyhow::Result<String> {
    let url = format!("http://127.0.0.1:{port}{path}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let mut req = client.get(&url);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await.with_context(|| format!("GET {url} failed"))?;
    Ok(resp.text().await?)
}

/// Convert a hashrate value + unit string (e.g. "MH/s") into raw H/s. Tolerant
/// of casing and missing "/s"; defaults to H/s when the unit is unrecognized.
pub fn hashrate_to_hs(value: f64, unit: &str) -> f64 {
    let u = unit.trim().to_ascii_lowercase();
    let mult = if u.starts_with("zh") {
        1e21
    } else if u.starts_with("eh") {
        1e18
    } else if u.starts_with("ph") {
        1e15
    } else if u.starts_with("th") {
        1e12
    } else if u.starts_with("gh") {
        1e9
    } else if u.starts_with("mh") {
        1e6
    } else if u.starts_with("kh") {
        1e3
    } else {
        1.0
    };
    value * mult
}

/// Extract a share/job difficulty from a miner stdout line. Tolerant of the
/// common shapes miners print, e.g. "diff 0.123", "(diff 1234)", "diff: 5e6".
/// Used as a best-effort source for `best_share_diff` when the structured API
/// doesn't expose it (notably cpuminer-opt).
pub fn extract_share_diff(line: &str) -> Option<f64> {
    let lower = line.to_ascii_lowercase();
    // Scan every "diff" occurrence and return the first that is followed by a
    // number. This skips the "diff" embedded in the word "difficulty".
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find("diff") {
        let idx = search_from + rel;
        search_from = idx + 4;
        let rest = lower[idx + 4..].trim_start_matches([' ', ':', '=', '(', ')', '\t']);
        let token: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == 'e' || *c == '+' || *c == '-')
            .collect();
        if let Some(d) = token.parse::<f64>().ok().filter(|d| d.is_finite() && *d > 0.0) {
            return Some(d);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashrate_units() {
        assert_eq!(hashrate_to_hs(1.0, "H/s"), 1.0);
        assert_eq!(hashrate_to_hs(1.5, "kH/s"), 1_500.0);
        assert_eq!(hashrate_to_hs(2.0, "MH/s"), 2_000_000.0);
        assert_eq!(hashrate_to_hs(3.0, "GH/s"), 3e9);
        assert_eq!(hashrate_to_hs(4.0, "th/s"), 4e12);
        // Unknown unit falls back to H/s.
        assert_eq!(hashrate_to_hs(7.0, "sols"), 7.0);
    }

    #[test]
    fn share_diff_shapes() {
        assert_eq!(extract_share_diff("Accepted 1/1, 1234 kH/s, diff 0.123"), Some(0.123));
        assert_eq!(extract_share_diff("yay!!! (diff 4096) accepted"), Some(4096.0));
        assert_eq!(extract_share_diff("stratum difficulty set to diff: 5e6"), Some(5e6));
        assert_eq!(extract_share_diff("no difficulty here"), None);
        assert_eq!(extract_share_diff("diff 0"), None); // zero is not a share
    }
}
