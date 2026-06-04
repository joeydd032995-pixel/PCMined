//! Typed `#[tauri::command]` surface — the ONLY way the webview reaches the core.
//!
//! SECURITY: no command here takes a free-form executable path or shell string
//! that gets executed. Process spawning only ever launches a named, registered
//! adapter (validated in the supervisor); the webview cannot run arbitrary
//! programs.

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::config::coins::{self, CoinInfo};
use crate::config::miners;
use crate::config::profiles::{PoolConfig, Profile};
use crate::events::channel;
use crate::miner::MinerInfo;
use crate::network_api::NetworkStats;
use crate::supervisor::{RunningMiner, StartError, StartRequest};
use crate::wallet::{self, AddressCheck};
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

/// All supported coins (algo, default pools, monitor-only flag).
#[tauri::command]
pub fn list_coins() -> Vec<CoinInfo> {
    coins::COINS.to_vec()
}

/// The default miner the app would pick for a coin (fee-free open source first).
#[tauri::command]
pub fn resolve_default_miner(coin: String) -> Option<MinerInfo> {
    miners::resolve_default_miner(&coin)
}

/// Validate a wallet ADDRESS for a coin (checksum, not just regex), in Rust.
#[tauri::command]
pub fn validate_address(coin: String, address: String) -> AddressCheck {
    wallet::validate(&coin, &address)
}

/// Persist a profile. Rejects an invalid wallet address with a reason so a typo
/// can never be saved (and therefore never started).
#[tauri::command]
pub fn save_profile(
    state: State<'_, AppState>,
    coin: String,
    profile: Profile,
) -> Result<(), String> {
    let check = wallet::validate(&coin, &profile.wallet);
    if !check.valid {
        return Err(check.reason.unwrap_or_else(|| "invalid wallet address".into()));
    }
    let mut store = state.config.lock().map_err(|e| e.to_string())?;
    store.save_profile(&coin, profile).map_err(|e| e.to_string())
}

/// Load saved profiles for a coin.
#[tauri::command]
pub fn load_profiles(state: State<'_, AppState>, coin: String) -> Result<Vec<Profile>, String> {
    let store = state.config.lock().map_err(|e| e.to_string())?;
    Ok(store.profiles(&coin))
}

/// Add a user-defined pool for a coin.
#[tauri::command]
pub fn add_custom_pool(
    state: State<'_, AppState>,
    coin: String,
    pool: PoolConfig,
) -> Result<(), String> {
    let mut store = state.config.lock().map_err(|e| e.to_string())?;
    store.add_custom_pool(&coin, pool).map_err(|e| e.to_string())
}

/// Result of probing a pool's reachability.
#[derive(Debug, Clone, Serialize)]
pub struct PoolTestResult {
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Test that a pool endpoint accepts a TCP connection. A dead pool is flagged
/// before the user relies on it. (Full Stratum subscribe handshake is layered
/// in later; reachability already catches the common "wrong host/port" case.)
#[tauri::command]
pub async fn test_pool(pool: PoolConfig) -> PoolTestResult {
    let addr = format!("{}:{}", pool.host, pool.port);
    let started = Instant::now();
    let connect = tokio::net::TcpStream::connect(&addr);
    match tokio::time::timeout(Duration::from_secs(5), connect).await {
        Ok(Ok(_stream)) => PoolTestResult {
            reachable: true,
            latency_ms: Some(started.elapsed().as_millis() as u64),
            error: None,
        },
        Ok(Err(e)) => PoolTestResult { reachable: false, latency_ms: None, error: Some(e.to_string()) },
        Err(_) => PoolTestResult {
            reachable: false,
            latency_ms: None,
            error: Some("connection timed out".into()),
        },
    }
}

/// Detect CPU/RAM/GPU and OS for this machine.
#[tauri::command]
pub async fn detect_hardware() -> crate::hardware::HardwareInfo {
    crate::hardware::detect().await
}

/// Suggest a thread count for a CPU-mined coin given detected hardware.
#[tauri::command]
pub fn suggest_threads(coin: String, physical_cores: u32, total_memory_mb: u64) -> u32 {
    crate::hardware::suggest_threads(&coin, physical_cores, total_memory_mb)
}

/// Start a monitor-only instance for a device/pool telemetry URL (no process).
#[tauri::command]
pub async fn start_monitor(
    app: AppHandle,
    state: State<'_, AppState>,
    coin: String,
    device_url: String,
) -> Result<String, StartError> {
    state.supervisor.lock().await.start_monitor(&app, coin, device_url)
}

/// Fetch live network stats for a coin (cached/degrading) and broadcast them on
/// the `network://difficulty` channel for any open dashboards.
#[tauri::command]
pub async fn get_network_stats(
    app: AppHandle,
    state: State<'_, AppState>,
    coin: String,
) -> Result<NetworkStats, String> {
    let stats = state.network.get(&coin).await;
    let _ = app.emit(channel::DIFFICULTY, &stats);
    Ok(stats)
}
