// Typed wrappers around the Rust `#[tauri::command]` surface.
//
// SECURITY: the webview can only call these named, Rust-validated commands. It
// holds NO ability to execute arbitrary shell commands — all process spawning
// lives behind the Rust core. Keep this file as the single source of truth for
// the IPC contract so the boundary stays auditable.
import { invoke } from "@tauri-apps/api/core";

/** Proof-of-work algorithms (mirrors Rust `Algo`, serde snake_case). */
export type Algo =
  | "random_x"
  | "scrypt"
  | "sha256d"
  | "kaw_pow"
  | "etc_hash"
  | "k_heavy_hash"
  | "autolykos2";

export type TelemetryKind = "http" | "tcp_json_rpc" | "tcp_text" | "stdout";

/** Static miner metadata — fee is first-class, surfaced on every miner card. */
export interface MinerInfo {
  id: string;
  display: string;
  algos: Algo[];
  dev_fee_pct: number;
  open_source: boolean;
  source_url: string;
  license: string;
  telemetry_kind: TelemetryKind;
}

/** A launch request. Carries a wallet ADDRESS only — never a private key. */
export interface StartRequest {
  miner_id: string;
  coin: string;
  algo: Algo;
  binary_path: string;
  pool_host: string;
  pool_port: number;
  user: string;
  pass?: string;
  tls?: boolean;
  threads?: number | null;
  extra_args?: string[];
}

export interface RunningMiner {
  id: string;
  miner_id: string;
  coin: string;
  dev_fee_pct: number;
}

/** Structured start failure (serde tag = "kind"). */
export type StartError =
  | { kind: "adapter_not_found"; miner_id: string }
  | { kind: "unsupported_algo"; miner_id: string }
  | {
      kind: "fee_confirmation_required";
      miner_id: string;
      dev_fee_pct: number;
      source_url: string;
      license: string;
    }
  | { kind: "concurrency_limit"; max: number }
  | { kind: "spawn_failed"; message: string };

/** Health-check round-trip used to verify the IPC boundary is wired. */
export function ping(message: string): Promise<string> {
  return invoke<string>("ping", { message });
}

/** Static metadata for every registered miner. */
export function listMiners(): Promise<MinerInfo[]> {
  return invoke<MinerInfo[]>("list_miners");
}

/**
 * Start a miner. Rejects with `StartError`; a `fee_confirmation_required`
 * error means the UI must show the one-time fee dialog, then call
 * {@link confirmFeeAndStart}.
 */
export function startMiner(request: StartRequest): Promise<string> {
  return invoke<string>("start_miner", { request });
}

/** Start a miner after the user has explicitly confirmed its dev fee. */
export function confirmFeeAndStart(request: StartRequest): Promise<string> {
  return invoke<string>("confirm_fee_and_start", { request });
}

/** Stop and reap a running miner instance. */
export function stopMiner(id: string): Promise<void> {
  return invoke<void>("stop_miner", { id });
}

/** List currently running instances. */
export function listRunning(): Promise<RunningMiner[]> {
  return invoke<RunningMiner[]>("list_running");
}

// ---- P2: coins, wallet validation, profiles, pools ------------------------

export interface PoolDef {
  name: string;
  host: string;
  port: number;
  tls: boolean;
  fee_pct: number;
  user_template: string;
}

export interface CoinInfo {
  id: string;
  name: string;
  algo: Algo;
  monitor_only: boolean;
  block_time_secs: number;
  default_pools: PoolDef[];
}

export interface PoolConfig {
  host: string;
  port: number;
  user_template?: string;
  pass?: string;
  tls?: boolean;
}

export interface Profile {
  name: string;
  /** Public receiving ADDRESS only — never a private key. */
  wallet: string;
  worker?: string;
  threads?: number | null;
  pool: PoolConfig;
  fallback_pools?: PoolConfig[];
}

/** Result of validating a wallet address (checksum-verified in Rust). */
export interface AddressCheck {
  valid: boolean;
  kind: string | null;
  reason: string | null;
}

export interface PoolTestResult {
  reachable: boolean;
  latency_ms: number | null;
  error: string | null;
}

/** All supported coins. */
export function listCoins(): Promise<CoinInfo[]> {
  return invoke<CoinInfo[]>("list_coins");
}

/** The default miner the app picks for a coin (fee-free open source first). */
export function resolveDefaultMiner(coin: string): Promise<MinerInfo | null> {
  return invoke<MinerInfo | null>("resolve_default_miner", { coin });
}

/** Validate a wallet address for a coin (checksum, not just regex). */
export function validateAddress(coin: string, address: string): Promise<AddressCheck> {
  return invoke<AddressCheck>("validate_address", { coin, address });
}

/** Persist a profile. Rejects an invalid wallet address with a reason. */
export function saveProfile(coin: string, profile: Profile): Promise<void> {
  return invoke<void>("save_profile", { coin, profile });
}

/** Load saved profiles for a coin. */
export function loadProfiles(coin: string): Promise<Profile[]> {
  return invoke<Profile[]>("load_profiles", { coin });
}

/** Add a user-defined pool for a coin. */
export function addCustomPool(coin: string, pool: PoolConfig): Promise<void> {
  return invoke<void>("add_custom_pool", { coin, pool });
}

/** Probe a pool endpoint's reachability before relying on it. */
export function testPool(pool: PoolConfig): Promise<PoolTestResult> {
  return invoke<PoolTestResult>("test_pool", { pool });
}

// ---- P3: live network stats -----------------------------------------------

/** Live (or cached/degraded) network state for the lottery odds view. */
export interface NetworkStats {
  coin: string;
  difficulty: number;
  network_hashrate: number;
  block_reward: number;
  block_time: number;
  fetched_at: number;
  /** True when this is cached/derived data, not a fresh live read. */
  stale: boolean;
}

/**
 * Fetch live network stats for a coin (cached, degrades gracefully). Also
 * broadcast on the `network://difficulty` channel for other listeners.
 */
export function getNetworkStats(coin: string): Promise<NetworkStats> {
  return invoke<NetworkStats>("get_network_stats", { coin });
}
