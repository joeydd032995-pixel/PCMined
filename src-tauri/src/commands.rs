//! Typed `#[tauri::command]` surface — the ONLY way the webview reaches the core.
//!
//! SECURITY: no command here takes a free-form executable path or shell string
//! that gets executed. Process spawning only ever launches a named, registered
//! adapter (validated in the supervisor); the webview cannot run arbitrary
//! programs.

use tauri::{AppHandle, State};

use crate::miner::MinerInfo;
use crate::supervisor::{RunningMiner, StartError, StartRequest};
use crate::AppState;

/// Health-check round-trip used to verify the IPC boundary during P0.
#[tauri::command]
pub fn ping(message: String) -> String {
    tracing::debug!(%message, "ping received");
    format!("pong: {message}")
}

/// Static metadata for every registered miner (id, fee, open-source, source,
/// license, telemetry kind). Drives the fee badges on miner cards.
#[tauri::command]
pub async fn list_miners(state: State<'_, AppState>) -> Result<Vec<MinerInfo>, String> {
    Ok(state.supervisor.lock().await.miner_infos())
}

/// Start a miner. Fails with `FeeConfirmationRequired` if the chosen miner
/// carries a dev fee and the user has not yet confirmed it.
#[tauri::command]
pub async fn start_miner(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartRequest,
) -> Result<String, StartError> {
    state.supervisor.lock().await.start(&app, request, false)
}

/// Start a miner after the user has explicitly confirmed its dev fee.
#[tauri::command]
pub async fn confirm_fee_and_start(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartRequest,
) -> Result<String, StartError> {
    state.supervisor.lock().await.start(&app, request, true)
}

/// Stop and reap a running miner instance by id.
#[tauri::command]
pub async fn stop_miner(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), StartError> {
    state.supervisor.lock().await.stop(&app, &id).await
}

/// List currently running instances (id, miner, coin, dev fee).
#[tauri::command]
pub async fn list_running(state: State<'_, AppState>) -> Result<Vec<RunningMiner>, String> {
    Ok(state.supervisor.lock().await.running())
}
