//! Typed event payloads and channel names emitted to the frontend.
//!
//! Channel-name constants are the single source of truth shared (by convention)
//! with `src/lib/events.ts`. Payload structs derive `Serialize` so the wire
//! shape stays in sync with the TypeScript types.

use serde::Serialize;

use crate::miner::MinerStats;

/// Event channel names. Keep in lockstep with `Channels` in `src/lib/events.ts`.
pub mod channel {
    pub const STATS: &str = "miner://stats";
    pub const STATUS: &str = "miner://status";
    pub const LOG: &str = "miner://log";
    pub const BLOCK_FOUND: &str = "miner://block-found";
    pub const DIFFICULTY: &str = "network://difficulty";
}

/// A telemetry snapshot for one running miner instance.
#[derive(Debug, Clone, Serialize)]
pub struct StatsEvent {
    pub miner_id: String,
    pub stats: MinerStats,
}

/// A single line of miner stdout/stderr forwarded to the UI log viewer.
#[derive(Debug, Clone, Serialize)]
pub struct LogEvent {
    pub miner_id: String,
    pub line: String,
}

/// Lifecycle state of a managed miner instance.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MinerState {
    Idle,
    Starting,
    Running,
    Reconnecting,
    Crashed,
    Stopped,
}

/// Status transition for a miner instance.
#[derive(Debug, Clone, Serialize)]
pub struct StatusEvent {
    pub miner_id: String,
    pub state: MinerState,
    pub detail: Option<String>,
}

/// A found block (best share reached network difficulty) — the rare win.
#[derive(Debug, Clone, Serialize)]
pub struct BlockFoundEvent {
    pub miner_id: String,
    pub coin: String,
    pub share_diff: f64,
    pub network_diff: f64,
}
