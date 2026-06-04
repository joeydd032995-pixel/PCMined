// Typed wrappers around the Rust `#[tauri::command]` surface.
//
// SECURITY: the webview can only call these named, Rust-validated commands. It
// holds NO ability to execute arbitrary shell commands — all process spawning
// lives behind the Rust core. Keep this file as the single source of truth for
// the IPC contract so the boundary stays auditable.
import { invoke } from "@tauri-apps/api/core";

/** Health-check round-trip used to verify the IPC boundary is wired. */
export function ping(message: string): Promise<string> {
  return invoke<string>("ping", { message });
}

// Additional commands (list_coins, validate_address, start_miner, …) are added
// phase-by-phase. Each must map to a typed Rust command — never a raw shell call.
