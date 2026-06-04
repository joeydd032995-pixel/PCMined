//! Solo Miner Dashboard — Rust core ("Lottery Ticket Terminal").
//!
//! This crate is an ORCHESTRATOR around third-party miner binaries and solo
//! pools. It does not implement hashing or the Stratum protocol: it spawns,
//! monitors, and configures miners, reads their telemetry, and renders it.
//!
//! Logic lives in plain modules so it stays unit-testable without a display;
//! `run()` wires those modules into the Tauri application.

pub mod commands;
pub mod config;
pub mod events;
pub mod miner;
pub mod supervisor;
pub mod wallet;

use std::sync::Mutex as StdMutex;

use tauri::Manager;
use tokio::sync::Mutex;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use config::store::ConfigStore;
use supervisor::Supervisor;

/// Shared application state managed by Tauri. The supervisor sits behind an
/// async mutex so command handlers can hold it across `.await` points (e.g.
/// while reaping a stopped child); the config store uses a plain mutex since
/// its operations are synchronous.
pub struct AppState {
    pub supervisor: Mutex<Supervisor>,
    pub config: StdMutex<ConfigStore>,
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
        .setup(|app| {
            // Resolve the per-user config dir and load (migrating) the config.
            let config_path = app
                .path()
                .app_config_dir()
                .map(|d| d.join("config.json"))
                .unwrap_or_else(|_| std::path::PathBuf::from("config.json"));
            tracing::info!(?config_path, "loading config");
            app.manage(AppState {
                supervisor: Mutex::new(Supervisor::new()),
                config: StdMutex::new(ConfigStore::load(config_path)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::list_miners,
            commands::list_coins,
            commands::resolve_default_miner,
            commands::validate_address,
            commands::save_profile,
            commands::load_profiles,
            commands::add_custom_pool,
            commands::test_pool,
            commands::start_miner,
            commands::confirm_fee_and_start,
            commands::stop_miner,
            commands::list_running,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
