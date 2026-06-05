//! Hardware detection and thread/intensity suggestions, plus gating of
//! coin/miner combinations by detected hardware and OS.
//!
//! Pure helpers (`suggest_threads`, `gate_combo`) are unit-tested; the live
//! probes (`detect`) shell out best-effort and degrade to empty on headless or
//! GPU-less machines.

use serde::Serialize;
use sysinfo::System;

use crate::config::coins;
use crate::config::miners;
use crate::miner::Algo;

#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    pub vendor: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HardwareInfo {
    pub cpu_brand: String,
    pub physical_cores: u32,
    pub logical_cores: u32,
    pub total_memory_mb: u64,
    pub gpus: Vec<GpuInfo>,
    pub os: String,
    pub arch: String,
}

/// Detect CPU + memory via sysinfo. Returns (brand, physical, logical, mem_mb).
pub fn detect_cpu() -> (String, u32, u32, u64) {
    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    let brand = sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
    let logical = sys.cpus().len() as u32;
    let physical = System::physical_core_count().map(|n| n as u32).unwrap_or(logical.max(1));
    let mem_mb = sys.total_memory() / 1024 / 1024;
    (brand, physical, logical, mem_mb)
}

/// Best-effort GPU probe. Tries vendor tools; empty on headless/none.
pub async fn detect_gpus() -> Vec<GpuInfo> {
    let mut gpus = Vec::new();

    // NVIDIA: `nvidia-smi -L` → "GPU 0: NVIDIA GeForce RTX 4090 (UUID: …)"
    if let Ok(out) = tokio::process::Command::new("nvidia-smi").arg("-L").output().await {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(rest) = line.split_once(':').map(|x| x.1) {
                let name = rest.split("(UUID").next().unwrap_or(rest).trim().to_string();
                if !name.is_empty() {
                    gpus.push(GpuInfo { vendor: "NVIDIA".into(), name });
                }
            }
        }
    }
    // AMD: `rocm-smi --showproductname`
    if let Ok(out) = tokio::process::Command::new("rocm-smi")
        .arg("--showproductname")
        .output()
        .await
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some((_, name)) = line.split_once("Card series:") {
                let name = name.trim().to_string();
                if !name.is_empty() {
                    gpus.push(GpuInfo { vendor: "AMD".into(), name });
                }
            }
        }
    }
    gpus
}

/// Full hardware snapshot.
pub async fn detect() -> HardwareInfo {
    let (cpu_brand, physical_cores, logical_cores, total_memory_mb) = detect_cpu();
    HardwareInfo {
        cpu_brand,
        physical_cores,
        logical_cores,
        total_memory_mb,
        gpus: detect_gpus().await,
        os: System::long_os_version().or_else(System::name).unwrap_or_else(|| "unknown".into()),
        arch: System::cpu_arch(),
    }
}

/// Suggested thread count for a CPU-mined coin given hardware. GPU coins return
/// 0 (the GPU miner manages its own parallelism).
pub fn suggest_threads(coin: &str, physical_cores: u32, total_memory_mb: u64) -> u32 {
    let algo = coins::coin(coin).map(|c| c.algo);
    match algo {
        Some(Algo::RandomX) => {
            let phys = physical_cores.max(1);
            // Leave a core for the OS on multi-core machines.
            let by_cores = if phys > 2 { phys - 1 } else { phys };
            // RandomX fast mode: ~2080 MB shared dataset + ~256 MB per thread.
            let usable = total_memory_mb.saturating_sub(2080);
            let by_mem = (usable / 256).max(1) as u32;
            by_cores.min(by_mem).max(1)
        }
        // Other CPU algos (rare here): use cores minus one.
        Some(Algo::Scrypt) | Some(Algo::Sha256d) => physical_cores.saturating_sub(1).max(1),
        // GPU algos / unknown: not thread-driven.
        _ => 0,
    }
}

/// Whether a coin/miner combo is runnable on this hardware/OS. Returns a reason
/// when not (e.g. a GPU miner with no GPU present).
pub fn gate_combo(miner_id: &str, hw: &HardwareInfo) -> (bool, Option<String>) {
    if is_gpu_miner(miner_id) && hw.gpus.is_empty() {
        return (false, Some("no GPU detected — this miner requires a discrete GPU".into()));
    }
    if miner_id == "cpuminer-opt" && hw.physical_cores == 0 {
        return (false, Some("no CPU cores detected".into()));
    }
    (true, None)
}

/// Miners that require a GPU.
fn is_gpu_miner(miner_id: &str) -> bool {
    matches!(miner_id, "lolminer" | "kawpowminer" | "ethminer")
}

/// Resolve the display + telemetry kind for a miner (used by callers building
/// monitor/start UIs). Thin pass-through to the registry.
pub fn miner_is_known(miner_id: &str) -> bool {
    miners::by_id(miner_id).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw(cores: u32, mem: u64, gpus: usize) -> HardwareInfo {
        HardwareInfo {
            cpu_brand: "Test CPU".into(),
            physical_cores: cores,
            logical_cores: cores * 2,
            total_memory_mb: mem,
            gpus: (0..gpus).map(|i| GpuInfo { vendor: "T".into(), name: format!("g{i}") }).collect(),
            os: "test".into(),
            arch: "x86_64".into(),
        }
    }

    #[test]
    fn randomx_threads_leave_a_core_and_respect_memory() {
        // 8 cores, plenty of RAM → 7 threads.
        assert_eq!(suggest_threads("xmr", 8, 32_768), 7);
        // 8 cores but tight RAM (2080 + 2*256 ≈ 2592) → memory-bound to ~2.
        assert_eq!(suggest_threads("xmr", 8, 2_592), 2);
        // tiny RAM still yields at least 1.
        assert_eq!(suggest_threads("xmr", 8, 2_080), 1);
        // dual-core: no core reserved.
        assert_eq!(suggest_threads("xmr", 2, 32_768), 2);
    }

    #[test]
    fn gpu_coins_are_not_thread_driven() {
        assert_eq!(suggest_threads("kas", 8, 32_768), 0);
        assert_eq!(suggest_threads("rvn", 8, 32_768), 0);
    }

    #[test]
    fn gate_blocks_gpu_miner_without_gpu() {
        let (ok, reason) = gate_combo("lolminer", &hw(8, 32_768, 0));
        assert!(!ok);
        assert!(reason.unwrap().contains("GPU"));

        let (ok, _) = gate_combo("lolminer", &hw(8, 32_768, 1));
        assert!(ok);
    }

    #[test]
    fn gate_allows_cpu_miner_and_monitor() {
        assert!(gate_combo("cpuminer-opt", &hw(8, 32_768, 0)).0);
        assert!(gate_combo("monitor", &hw(8, 32_768, 0)).0);
    }
}
