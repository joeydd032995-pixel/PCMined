//! Solo Miner Dashboard — Rust core ("Lottery Ticket Terminal").
//!
//! This crate is an ORCHESTRATOR around third-party miner binaries and solo
//! pools. It does not implement hashing or the Stratum protocol: it spawns,
//! monitors, and configures miners, reads their telemetry, and renders it.
//!
//! Logic lives in plain modules so it stays unit-testable without a display;
//! `run()` wires those modules into the Tauri application.

pub mod commands;
pub mod events;
pub mod miner;
pub mod supervisor;

use tokio::sync::Mutex;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use supervisor::Supervisor;

/// Shared application state managed by Tauri. The supervisor sits behind an
/// async mutex so command handlers can hold it across `.await` points (e.g.
/// while reaping a stopped child).
pub struct AppState {
    pub supervisor: Mutex<Supervisor>,
}

/// Initialize structured logging. Idempotent-safe for tests via `try_init`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,solo_miner_dashboard_lib=debug"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .try_init();
}

/// Build and run the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    tauri::Builder::default()
        // Single-instance must be registered first so a second launch focuses
        // the existing window instead of starting a duplicate supervisor.
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .manage(AppState {
            supervisor: Mutex::new(Supervisor::new()),
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::list_miners,
            commands::start_miner,
            commands::confirm_fee_and_start,
            commands::stop_miner,
            commands::list_running,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
