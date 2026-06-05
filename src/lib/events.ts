// Typed wrappers around Tauri events emitted by the Rust core.
//
// Event channel names mirror `src-tauri/src/events.rs`. Payload shapes are kept
// in sync with the Rust `serde` structs. Subscribe via the helpers below so the
// channel-name strings live in exactly one place.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Whether a stats snapshot is live or a marked-stale cache. */
export type StatsSource = { kind: "live" } | { kind: "stale"; age_secs: number };

/** Normalized miner telemetry (mirrors Rust `MinerStats`). */
export interface MinerStats {
  hashrate_hs: number;
  hashrate_avg: [number, number, number];
  accepted: number;
  rejected: number;
  best_share_diff: number;
  uptime_secs: number;
  connected: boolean;
  current_job_diff: number | null;
  source: StatsSource;
}

export type MinerState =
  | "idle"
  | "starting"
  | "running"
  | "reconnecting"
  | "crashed"
  | "stopped";

export interface StatsEvent {
  miner_id: string;
  stats: MinerStats;
}
export interface StatusEvent {
  miner_id: string;
  state: MinerState;
  detail: string | null;
}
export interface LogEvent {
  miner_id: string;
  line: string;
}
export interface BlockFoundEvent {
  miner_id: string;
  coin: string;
  share_diff: number;
  network_diff: number;
}

export const Channels = {
  stats: "miner://stats",
  status: "miner://status",
  log: "miner://log",
  blockFound: "miner://block-found",
  difficulty: "network://difficulty",
} as const;

/** Generic typed subscription helper. */
export function on<T>(
  channel: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>(channel, (event) => handler(event.payload));
}

export const onStats = (h: (e: StatsEvent) => void) => on<StatsEvent>(Channels.stats, h);
export const onStatus = (h: (e: StatusEvent) => void) => on<StatusEvent>(Channels.status, h);
export const onLog = (h: (e: LogEvent) => void) => on<LogEvent>(Channels.log, h);
export const onBlockFound = (h: (e: BlockFoundEvent) => void) =>
  on<BlockFoundEvent>(Channels.blockFound, h);
