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
