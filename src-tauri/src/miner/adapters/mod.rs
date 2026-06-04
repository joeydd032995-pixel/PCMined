//! Concrete `MinerAdapter` implementations, one per supported miner binary.
//!
//! Adding a coin/miner = one adapter here + one registry entry (see P2). P1
//! ships the two that exercise both telemetry styles and both fee tiers:
//! cpuminer-opt (TcpText, 0%) and lolMiner (HTTP JSON, ~1% disclosed).

pub mod cpuminer_opt;
pub mod lolminer;
