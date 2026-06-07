//! Per-coin wallet ADDRESS validation — regex/structure **plus checksum**.
//!
//! We never accept, store, or transmit private keys: only public receiving
//! addresses. Validation happens entirely in Rust before an address can be used
//! to start a miner, so a typo can't silently send a lifetime of lottery odds to
//! the void.
//!
//! Checksums implemented:
//! - BTC/LTC: bech32/bech32m (segwit) via the `bech32` crate, or Base58Check
//!   (legacy) via `bs58` — both verify a real checksum, not just a prefix.
//! - DOGE/RVN: Base58Check.
//! - ETC: EIP-55 mixed-case Keccak-256 checksum (or all-one-case = no checksum).
//! - XMR: Monero block-based Base58 + Keccak-256 4-byte checksum.
//! - KAS: Kaspa CashAddr-style bech32 with a 40-bit polymod checksum.
//! - ERG: Base58 + Blake2b-256 4-byte checksum.

use blake2::digest::consts::U32;
use blake2::Blake2b;
use serde::Serialize;
use sha3::{Digest, Keccak256};

type Blake2b256 = Blake2b<U32>;

/// Result of validating an address for a coin.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AddressCheck {
    pub valid: bool,
    /// Human label for the recognized address kind (when valid).
    pub kind: Option<String>,
    /// Why the address was rejected (when invalid).
    pub reason: Option<String>,
}

impl AddressCheck {
    fn ok(kind: &str) -> Self {
        AddressCheck { valid: true, kind: Some(kind.to_string()), reason: None }
    }
    fn bad(reason: impl Into<String>) -> Self {
        AddressCheck { valid: false, kind: None, reason: Some(reason.into()) }
    }
}

/// Validate `addr` for the coin identified by `coin` (e.g. "btc", "xmr").
pub fn validate(coin: &str, addr: &str) -> AddressCheck {
    let addr = addr.trim();
    if addr.is_empty() {
        return AddressCheck::bad("address is empty");
    }
    match coin.to_ascii_lowercase().as_str() {
        "btc" => validate_btc(addr),
        "ltc" => validate_ltc(addr),
        "doge" => base58check(addr, &[0x1e, 0x16], "Dogecoin Base58Check"),
        "rvn" => base58check(addr, &[0x3c, 0x7a], "Ravencoin Base58Check"),
        // EVM chains all share the EIP-55 hex address format.
        "etc" | "ethw" | "octa" | "clo" => validate_eip55(addr),
        "xmr" => validate_monero(addr),
        // Kaspa and its forks share the CashAddr-style scheme (same polymod).
        "kas" => validate_cashaddr(addr, &["kaspa", "kaspatest", "kaspasim", "kaspadev"], "Kaspa"),
        "kls" => validate_cashaddr(addr, &["karlsen", "karlsentest"], "Karlsen"),
        "pyi" => validate_cashaddr(addr, &["pyrin", "pyrintest"], "Pyrin"),
        "erg" => validate_ergo(addr),
        other => AddressCheck::bad(format!("address validation not supported for coin '{other}'")),
    }
}

// ---- Bitcoin / Litecoin (bech32 segwit or Base58Check) --------------------

fn validate_btc(addr: &str) -> AddressCheck {
    if is_segwit_prefix(addr, "bc1") {
        return validate_bech32_hrp(addr, "bc", "Bitcoin bech32 segwit");
    }
    base58check(addr, &[0x00, 0x05], "Bitcoin Base58Check")
}

fn validate_ltc(addr: &str) -> AddressCheck {
    if is_segwit_prefix(addr, "ltc1") {
        return validate_bech32_hrp(addr, "ltc", "Litecoin bech32 segwit");
    }
    base58check(addr, &[0x30, 0x32, 0x05], "Litecoin Base58Check")
}

fn is_segwit_prefix(addr: &str, prefix: &str) -> bool {
    let lower = addr.to_ascii_lowercase();
    lower.starts_with(prefix)
}

/// Verify a bech32/bech32m checksum and that the human-readable part matches.
fn validate_bech32_hrp(addr: &str, expected_hrp: &str, kind: &str) -> AddressCheck {
    // bech32 forbids mixed case; an all-uppercase address is valid, so normalize.
    let normalized = if addr.chars().all(|c| !c.is_ascii_lowercase()) {
        addr.to_ascii_lowercase()
    } else {
        addr.to_string()
    };
    match bech32::decode(&normalized) {
        Ok((hrp, _data)) if hrp.as_str() == expected_hrp => AddressCheck::ok(kind),
        Ok((hrp, _)) => AddressCheck::bad(format!("unexpected bech32 prefix '{}'", hrp.as_str())),
        Err(e) => AddressCheck::bad(format!("invalid bech32 checksum: {e}")),
    }
}

/// Verify a Base58Check (double-SHA256) address whose version byte is in
/// `allowed_versions`. Payload is version(1) + hash160(20) = 21 bytes.
fn base58check(addr: &str, allowed_versions: &[u8], kind: &str) -> AddressCheck {
    match bs58::decode(addr).with_check(None).into_vec() {
        Ok(bytes) => {
            if bytes.len() != 21 {
                return AddressCheck::bad("unexpected decoded length for a P2PKH/P2SH address");
            }
            if !allowed_versions.contains(&bytes[0]) {
                return AddressCheck::bad(format!("address version byte 0x{:02x} not valid for this coin", bytes[0]));
            }
            AddressCheck::ok(kind)
        }
        Err(_) => AddressCheck::bad("invalid Base58Check checksum"),
    }
}

// ---- Ethereum Classic (EIP-55) --------------------------------------------

fn validate_eip55(addr: &str) -> AddressCheck {
    let body = addr.strip_prefix("0x").or_else(|| addr.strip_prefix("0X")).unwrap_or(addr);
    if body.len() != 40 || !body.bytes().all(|b| b.is_ascii_hexdigit()) {
        return AddressCheck::bad("ETC address must be 40 hex characters (optionally 0x-prefixed)");
    }
    let lower = body.to_ascii_lowercase();
    // All-lower or all-upper carries no EIP-55 checksum; accept structurally.
    if body == lower || body == body.to_ascii_uppercase() {
        return AddressCheck::ok("Ethereum-style (no EIP-55 checksum)");
    }
    let hash = Keccak256::digest(lower.as_bytes());
    for (i, ch) in body.chars().enumerate() {
        if ch.is_ascii_alphabetic() {
            let nibble = if i % 2 == 0 { hash[i / 2] >> 4 } else { hash[i / 2] & 0x0f };
            let should_upper = nibble >= 8;
            if should_upper != ch.is_ascii_uppercase() {
                return AddressCheck::bad("EIP-55 checksum mismatch (mixed-case address)");
            }
        }
    }
    AddressCheck::ok("Ethereum EIP-55")
}

// ---- Monero (block-based Base58 + Keccak-256) -----------------------------

const MONERO_ALPHABET: &[u8; 58] =
    b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const MONERO_ENCODED_BLOCK_SIZES: [usize; 9] = [0, 2, 3, 5, 6, 7, 9, 10, 11];

fn monero_decoded_block_size(enc_len: usize) -> Option<usize> {
    MONERO_ENCODED_BLOCK_SIZES.iter().position(|&s| s == enc_len)
}

fn monero_base58_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let enc_len = (bytes.len() - i).min(11);
        let dec_len = if enc_len == 11 { 8 } else { monero_decoded_block_size(enc_len)? };
        let mut num: u64 = 0;
        for &c in &bytes[i..i + enc_len] {
            let digit = MONERO_ALPHABET.iter().position(|&a| a == c)? as u64;
            num = num.checked_mul(58)?.checked_add(digit)?;
        }
        // Reject overlong encodings that don't fit the decoded block size.
        if dec_len < 8 && num >= (1u64 << (8 * dec_len as u32)) {
            return None;
        }
        for b in 0..dec_len {
            out.push((num >> (8 * (dec_len - 1 - b) as u32)) as u8);
        }
        i += enc_len;
    }
    Some(out)
}

fn validate_monero(addr: &str) -> AddressCheck {
    let bytes = match monero_base58_decode(addr) {
        Some(b) if b.len() >= 5 => b,
        _ => return AddressCheck::bad("not valid Monero Base58"),
    };
    let split = bytes.len() - 4;
    let expected = &Keccak256::digest(&bytes[..split])[..4];
    if expected != &bytes[split..] {
        return AddressCheck::bad("Monero checksum mismatch");
    }
    // Mainnet network bytes: 18 = standard, 19 = integrated, 42 = subaddress.
    match bytes[0] {
        18 | 19 | 42 => AddressCheck::ok("Monero (mainnet)"),
        other => AddressCheck::bad(format!("unexpected Monero network byte {other} (expected mainnet)")),
    }
}

// ---- Kaspa (CashAddr-style bech32, 40-bit polymod) ------------------------

const KASPA_CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

fn kaspa_polymod(values: &[u8]) -> u64 {
    let mut c: u64 = 1;
    for &d in values {
        let c0 = (c >> 35) as u8;
        c = ((c & 0x07ffffffff) << 5) ^ (d as u64);
        if c0 & 0x01 != 0 {
            c ^= 0x98f2bc8e61;
        }
        if c0 & 0x02 != 0 {
            c ^= 0x79b76d99e2;
        }
        if c0 & 0x04 != 0 {
            c ^= 0xf33e5fb3c4;
        }
        if c0 & 0x08 != 0 {
            c ^= 0xae2eabe2a8;
        }
        if c0 & 0x10 != 0 {
            c ^= 0x1e4f43e470;
        }
    }
    c ^ 1
}

/// Validate a Kaspa-style CashAddr address (Kaspa and its forks Karlsen, Pyrin
/// all share the same encoding and polymod, differing only in the prefix).
fn validate_cashaddr(addr: &str, allowed_prefixes: &[&str], coin: &str) -> AddressCheck {
    let (prefix, data) = match addr.split_once(':') {
        Some(x) => x,
        None => return AddressCheck::bad(format!("{coin} address must include a '{}:' prefix", allowed_prefixes[0])),
    };
    if !allowed_prefixes.contains(&prefix) {
        return AddressCheck::bad(format!("unknown {coin} network prefix '{prefix}'"));
    }
    let mut data5 = Vec::with_capacity(data.len());
    for &c in data.as_bytes() {
        match KASPA_CHARSET.iter().position(|&x| x == c) {
            Some(p) => data5.push(p as u8),
            None => return AddressCheck::bad(format!("invalid character in {coin} address")),
        }
    }
    if data5.len() < 8 {
        return AddressCheck::bad(format!("{coin} address too short"));
    }
    let (payload5, checksum5) = data5.split_at(data5.len() - 8);

    let mut input: Vec<u8> = prefix.bytes().map(|b| b & 0x1f).collect();
    input.push(0); // separator
    input.extend_from_slice(payload5);
    input.extend_from_slice(&[0u8; 8]);
    let expected = kaspa_polymod(&input);

    let actual = checksum5.iter().fold(0u64, |acc, &g| (acc << 5) | g as u64);
    if expected != actual {
        return AddressCheck::bad(format!("{coin} checksum mismatch"));
    }
    AddressCheck::ok(&format!("{coin} CashAddr"))
}

/// Encode a CashAddr address from a prefix + 5-bit payload (test helper / used
/// to derive deterministic vectors for the Kaspa-fork coins).
#[cfg(test)]
fn encode_cashaddr(prefix: &str, payload5: &[u8]) -> String {
    let mut input: Vec<u8> = prefix.bytes().map(|b| b & 0x1f).collect();
    input.push(0);
    input.extend_from_slice(payload5);
    input.extend_from_slice(&[0u8; 8]);
    let checksum = kaspa_polymod(&input);
    let checksum5: Vec<u8> = (0..8).map(|i| ((checksum >> (5 * (7 - i))) & 0x1f) as u8).collect();
    let mut out = String::from(prefix);
    out.push(':');
    for &g in payload5.iter().chain(checksum5.iter()) {
        out.push(KASPA_CHARSET[g as usize] as char);
    }
    out
}

// ---- Ergo (Base58 + Blake2b-256) ------------------------------------------

fn validate_ergo(addr: &str) -> AddressCheck {
    let bytes = match bs58::decode(addr).into_vec() {
        Ok(b) if b.len() >= 5 => b,
        Ok(_) => return AddressCheck::bad("Ergo address too short"),
        Err(_) => return AddressCheck::bad("invalid Base58 in Ergo address"),
    };
    let split = bytes.len() - 4;
    let expected = &Blake2b256::digest(&bytes[..split])[..4];
    if expected != &bytes[split..] {
        return AddressCheck::bad("Ergo checksum mismatch");
    }
    let prefix = bytes[0];
    let network = prefix & 0xf0;
    let addr_type = prefix & 0x0f;
    if network != 0x00 {
        return AddressCheck::bad("Ergo address is not on mainnet");
    }
    if !(1..=3).contains(&addr_type) {
        return AddressCheck::bad("unrecognized Ergo address type");
    }
    AddressCheck::ok("Ergo (mainnet)")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Known-good public addresses (no private keys involved).
    const BTC_BECH32: &str = "bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq";
    const BTC_P2PKH: &str = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
    const LTC_P2PKH: &str = "LdP8Qox1VAhCzLJNqrr74YovaWYyNBUWvL";
    const DOGE: &str = "DH5yaieqoZN36fDVciNyRueRGvGLR3mr7L";
    const XMR: &str =
        "44AFFq5kSiGBoZ4NMDwYtN18obc8AemS33DBLWs3H7otXft3XjrpDtQGv7SqSsaBYBb98uNbr2VBBEt7f2wfn3RVGQBEP3A";
    const KAS: &str = "kaspa:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j";
    const ERG: &str = "9fRAWhdxEsTcdb8PhGNrZfwqa65zfkuYHAMmkQLcic1gdLSV5vA";
    // EIP-55 mixed-case Ethereum address.
    const ETH_EIP55: &str = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed";

    #[test]
    fn accepts_known_good() {
        for (coin, addr) in [
            ("btc", BTC_BECH32),
            ("btc", BTC_P2PKH),
            ("ltc", LTC_P2PKH),
            ("doge", DOGE),
            ("xmr", XMR),
            ("kas", KAS),
            ("erg", ERG),
            ("etc", ETH_EIP55),
        ] {
            let r = validate(coin, addr);
            assert!(r.valid, "{coin} address should be valid: {addr} -> {:?}", r.reason);
        }
    }

    #[test]
    fn rejects_corrupted_checksums() {
        // Flip the last character of each address.
        let mutate = |s: &str| {
            let mut c: Vec<char> = s.chars().collect();
            let last = c.len() - 1;
            c[last] = if c[last] == 'q' { 'p' } else { 'q' };
            c.into_iter().collect::<String>()
        };
        for (coin, addr) in [("btc", BTC_BECH32), ("xmr", XMR), ("kas", KAS), ("erg", ERG)] {
            let r = validate(coin, &mutate(addr));
            assert!(!r.valid, "corrupted {coin} address should be rejected");
            assert!(r.reason.is_some());
        }
    }

    #[test]
    fn rejects_wrong_coin_and_empty() {
        assert!(!validate("btc", XMR).valid);
        assert!(!validate("xmr", BTC_BECH32).valid);
        assert!(!validate("btc", "").valid);
        assert!(!validate("unknowncoin", BTC_P2PKH).valid);
    }

    #[test]
    fn cashaddr_forks_validate_under_their_prefixes() {
        // Take the real Kaspa example's 5-bit payload and re-encode it under the
        // karlsen/pyrin prefixes (they are Kaspa forks with the same scheme).
        let (_, data) = KAS.split_once(':').unwrap();
        let data5: Vec<u8> = data
            .bytes()
            .map(|c| KASPA_CHARSET.iter().position(|&x| x == c).unwrap() as u8)
            .collect();
        let payload5 = &data5[..data5.len() - 8];
        for (coin, pfx) in [("kls", "karlsen"), ("pyi", "pyrin")] {
            let addr = encode_cashaddr(pfx, payload5);
            let r = validate(coin, &addr);
            assert!(r.valid, "{coin} {addr} -> {:?}", r.reason);
            // Wrong network prefix for the coin is rejected.
            assert!(!validate(coin, &encode_cashaddr("kaspa", payload5)).valid);
            // A corrupted last char fails the checksum.
            let mut chars: Vec<char> = addr.chars().collect();
            let last = chars.len() - 1;
            chars[last] = if chars[last] == 'q' { 'p' } else { 'q' };
            assert!(!validate(coin, &chars.into_iter().collect::<String>()).valid);
        }
    }

    #[test]
    fn evm_coins_share_eip55() {
        for coin in ["ethw", "octa", "clo"] {
            assert!(validate(coin, ETH_EIP55).valid, "{coin}");
            assert!(validate(coin, &ETH_EIP55.to_lowercase()).valid);
        }
    }

    #[test]
    fn eip55_all_lowercase_ok_but_mixed_typo_fails() {
        assert!(validate("etc", &ETH_EIP55.to_lowercase()).valid);
        // Corrupt the case of one alpha char to break the EIP-55 checksum.
        let mut c: Vec<char> = ETH_EIP55.chars().collect();
        // index 2 is 'a' in "0x5aAe..."; flip its case.
        c[3] = c[3].to_ascii_uppercase();
        let corrupted: String = c.into_iter().collect();
        assert!(!validate("etc", &corrupted).valid);
    }
}
