//! A managed miner instance: its child process, the tasks that pump its logs
//! and poll its telemetry, and the lifecycle that guarantees a clean reap.
//!
//! P1 implements `InstanceKind::Spawned` (we launch the miner). `Monitor`
//! (device/pool API, no local process) arrives in P4.

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

use crate::events::{channel, LogEvent, MinerState, StatsEvent, StatusEvent};
use crate::miner::telemetry::{extract_share_diff, Telemetry};
use crate::miner::{MinerAdapter, MinerStats, ResolvedProfile, TelemetryBinding};

const LOG_RING_CAP: usize = 500;
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Whether we spawned the miner or are only monitoring an external device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceKind {
    Spawned,
    Monitor,
}

/// Mutable state shared between an instance and its background tasks.
struct Shared {
    /// Best share difficulty observed via stdout (the lottery number).
    best_share: Mutex<f64>,
    last_stats: Mutex<MinerStats>,
    logs: Mutex<VecDeque<String>>,
}

impl Shared {
    fn new() -> Self {
        Shared {
            best_share: Mutex::new(0.0),
            last_stats: Mutex::new(MinerStats::default()),
            logs: Mutex::new(VecDeque::with_capacity(LOG_RING_CAP)),
        }
    }
}

/// A running, supervised miner.
pub struct Instance {
    pub id: String,
    pub miner_id: String,
    pub coin: String,
    pub kind: InstanceKind,
    pub dev_fee_pct: f32,
    child: Child,
    tasks: Vec<JoinHandle<()>>,
    shared: Arc<Shared>,
}

impl Instance {
    /// Spawn `adapter`'s miner binary for `profile`, wiring telemetry to
    /// `binding`, and start the log + poll tasks that emit events on `app`.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        id: String,
        coin: String,
        adapter: Arc<dyn MinerAdapter>,
        profile: ResolvedProfile,
        binding: TelemetryBinding,
        telemetry: Telemetry,
        app: AppHandle,
    ) -> anyhow::Result<Instance> {
        let info = adapter.info();
        let miner_id = info.id.to_string();
        let dev_fee_pct = info.dev_fee_pct;
        let args = adapter.build_args(&profile, &binding);

        emit_status(&app, &id, MinerState::Starting, None);
        tracing::info!(%id, %miner_id, ?args, "spawning miner");

        let mut child = Command::new(&profile.binary_path)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Belt-and-suspenders: if the Instance is dropped without stop(),
            // the OS still reaps the child rather than orphaning it.
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("failed to spawn miner binary {:?}", profile.binary_path))?;

        let shared = Arc::new(Shared::new());
        let mut tasks = Vec::new();

        if let Some(stdout) = child.stdout.take() {
            tasks.push(spawn_log_reader(stdout, id.clone(), shared.clone(), app.clone()));
        }
        if let Some(stderr) = child.stderr.take() {
            tasks.push(spawn_log_reader(stderr, id.clone(), shared.clone(), app.clone()));
        }
        tasks.push(spawn_poll_task(id.clone(), adapter, telemetry, shared.clone(), app.clone()));

        emit_status(&app, &id, MinerState::Running, None);

        Ok(Instance {
            id,
            miner_id,
            coin,
            kind: InstanceKind::Spawned,
            dev_fee_pct,
            child,
            tasks,
            shared,
        })
    }

    /// Latest telemetry snapshot.
    pub fn snapshot(&self) -> MinerStats {
        self.shared.last_stats.lock().expect("stats lock").clone()
    }

    /// Buffered recent log lines (oldest first).
    pub fn logs(&self) -> Vec<String> {
        self.shared.logs.lock().expect("log lock").iter().cloned().collect()
    }

    /// Stop the instance: abort its tasks, kill the child, and reap it so no
    /// orphan process is left behind.
    pub async fn stop(mut self, app: &AppHandle) -> anyhow::Result<()> {
        for t in &self.tasks {
            t.abort();
        }
        // Best-effort kill, then wait to reap the zombie.
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        emit_status(app, &self.id, MinerState::Stopped, None);
        tracing::info!(id = %self.id, "miner stopped and reaped");
        Ok(())
    }
}

fn emit_status(app: &AppHandle, id: &str, state: MinerState, detail: Option<String>) {
    let _ = app.emit(
        channel::STATUS,
        StatusEvent { miner_id: id.to_string(), state, detail },
    );
}

/// Read a child stream line-by-line: update best-share, ring-buffer the line,
/// and forward it to the UI log viewer.
fn spawn_log_reader<R>(reader: R, id: String, shared: Arc<Shared>, app: AppHandle) -> JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(d) = extract_share_diff(&line) {
                let mut best = shared.best_share.lock().expect("best_share lock");
                if d > *best {
                    *best = d;
                }
            }
            {
                let mut logs = shared.logs.lock().expect("log lock");
                if logs.len() >= LOG_RING_CAP {
                    logs.pop_front();
                }
                logs.push_back(line.clone());
            }
            let _ = app.emit(channel::LOG, LogEvent { miner_id: id.clone(), line });
        }
    })
}

/// Poll telemetry every `POLL_INTERVAL`, merge in the stdout best-share, and
/// emit a stats event. Errors are logged but never kill the loop (the watchdog
/// in P5 owns recovery decisions).
fn spawn_poll_task(
    id: String,
    adapter: Arc<dyn MinerAdapter>,
    telemetry: Telemetry,
    shared: Arc<Shared>,
    app: AppHandle,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            match adapter.read(&telemetry).await {
                Ok(mut stats) => {
                    // Reconcile best-share between the structured API and stdout,
                    // keeping the running maximum in both places.
                    {
                        let mut best = shared.best_share.lock().expect("best_share lock");
                        *best = best.max(stats.best_share_diff);
                        stats.best_share_diff = *best;
                    }
                    *shared.last_stats.lock().expect("stats lock") = stats.clone();
                    let _ = app.emit(channel::STATS, StatsEvent { miner_id: id.clone(), stats });
                }
                Err(e) => {
                    tracing::debug!(%id, error = %e, "telemetry read failed");
                }
            }
        }
    })
}
