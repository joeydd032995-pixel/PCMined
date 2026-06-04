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
pub mod hardware;
pub mod miner;
pub mod network_api;
pub mod supervisor;
pub mod wallet;

use std::sync::Mutex as StdMutex;

use tauri::Manager;
use tokio::sync::Mutex;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use config::store::ConfigStore;
use network_api::NetworkApi;
use supervisor::Supervisor;

/// Shared application state managed by Tauri. The supervisor sits behind an
/// async mutex so command handlers can hold it across `.await` points (e.g.
/// while reaping a stopped child); the config store uses a plain mutex since
/// its operations are synchronous.
pub struct AppState {
    pub supervisor: Mutex<Supervisor>,
    pub config: StdMutex<ConfigStore>,
    pub network: NetworkApi,
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
                network: NetworkApi::new(),
            });
            build_tray(app.handle())?;
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
            commands::get_network_stats,
            commands::detect_hardware,
            commands::suggest_threads,
            commands::start_monitor,
            commands::report_block_found,
            commands::simulate_block_found,
            commands::start_miner,
            commands::confirm_fee_and_start,
            commands::stop_miner,
            commands::list_running,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Graceful shutdown: reap every miner before exit so nothing is
            // orphaned. Webhooks/CI never deliver this — it must be wired here.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state = app.state::<AppState>();
                tauri::async_runtime::block_on(async {
                    let mut sup = state.supervisor.lock().await;
                    sup.stop_all(app).await;
                });
                tracing::info!("all miners stopped on exit");
            }
        });
}

/// Build the system tray: quick actions + single-click to show the window.
fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::TrayIconBuilder;

    let show = MenuItemBuilder::with_id("show", "Show dashboard").build(app)?;
    let stop_all = MenuItemBuilder::with_id("stop_all", "Stop all miners").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&show, &stop_all]).separator().item(&quit).build()?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("Lottery Ticket Terminal")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "stop_all" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    let mut sup = state.supervisor.lock().await;
                    sup.stop_all(&app).await;
                });
            }
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
