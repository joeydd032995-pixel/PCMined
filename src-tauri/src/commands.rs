//! Typed `#[tauri::command]` surface — the ONLY way the webview reaches the core.
//!
//! SECURITY: no command here ever takes a free-form executable path or shell
//! string from the frontend. Process spawning (added in P1) only ever launches
//! named, registry-validated miner profiles resolved entirely in Rust.

/// Health-check round-trip used to verify the IPC boundary during P0.
#[tauri::command]
pub fn ping(message: String) -> String {
    tracing::debug!(%message, "ping received");
    format!("pong: {message}")
}
