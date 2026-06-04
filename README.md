# Lottery Ticket Terminal

A dark, minimalist desktop dashboard for **honest solo crypto mining**. It presents solo
mining for what it is — a **high-variance lottery, not income** — shows live odds, and manages
miners reliably.

> **Core thesis:** this app is an **orchestrator and dashboard around existing third-party
> miner binaries and solo pools**. It does **not** implement hashing algorithms and does **not**
> implement the Stratum protocol. The miner binaries hash and talk to the pool; this app
> spawns/monitors them, configures them, reads their telemetry, and renders it.

## Principles

- **Orchestrator, not an engine.** No hashing, no Stratum. We wrap miners and read their stats.
- **Fee-transparent.** Prefer 0% open-source miners; any dev fee is shown on the miner card and
  requires explicit one-time confirmation before starting.
- **Security-first.** All process spawning lives behind typed Rust commands — the webview never
  receives `shell:allow-execute`. Addresses only, never private keys. Inputs validated (with
  checksums) in Rust. Downloaded binaries are SHA-256 verified before execution.
- **Ethical.** For mining your **own** hardware with consent. No silent/headless/background path.

## Stack

Tauri 2 · React 18 + TypeScript + Vite · Rust core (`tokio`, `reqwest`, `serde`, `sysinfo`).

## Project layout

```
src/                  React + TypeScript frontend
src-tauri/src/        Rust core (lib.rs = testable logic, main.rs = thin binary)
  commands.rs         typed #[tauri::command] surface (the only IPC entrypoint)
  events.rs           typed event payloads + channel names
src-tauri/capabilities/default.json   least-privilege; NO shell:allow-execute
```

## Development

Prerequisites: Rust (stable), Node 18+, and the [Tauri 2 system
dependencies](https://tauri.app/start/prerequisites/) for your OS (on Debian/Ubuntu:
`libwebkit2gtk-4.1-dev libsoup-3.0-dev libgtk-3-dev librsvg2-dev`).

```bash
npm install
npm run tauri dev      # run the desktop app (requires a display)
npm run build          # typecheck + build frontend
npm test               # frontend unit tests (vitest)
cd src-tauri && cargo test && cargo clippy --all-targets
```

## Status

Under active construction, phase by phase:

- **P0 — Scaffold** ✅ Tauri 2 + React/TS app, dark theme, typed IPC/events, `ping` round-trip,
  least-privilege capabilities.
- **P1 — Process engine + telemetry** ✅ `MinerAdapter` trait, `tokio::process` spawn/poll/reap,
  cpuminer-opt (TcpText, 0%) + lolMiner (HTTP, ~1%) adapters, supervisor with dev-fee gate.
- **P2 — Config / profiles / wallet validation** ✅ versioned atomic config store, coin + miner
  registries (`resolve_default_miner`), checksum-verified address validation for 8 coins.
- **P3 — Lottery dashboard + network APIs** ✅ `network_api.rs` (BTC live via mempool.space,
  graceful degradation + stale marking), lottery math, log-scale `LotteryBar`, uPlot hashrate
  chart, fee-badged miner cards.
- P4–P7 — Multi-miner/hardware + monitor-only, reliability/tray, binary verification/packaging,
  polish/docs (not yet implemented).

### Extending: add a coin or miner

The roster is data-driven. To add a miner: implement one `MinerAdapter`
(`src-tauri/src/miner/adapters/`) and add one entry to the registry
(`src-tauri/src/config/miners.rs`). To add a coin: add a `CoinInfo` to
`src-tauri/src/config/coins.rs` (algo, default pools, difficulty→hashrate) and an address
validator branch in `src-tauri/src/wallet.rs`.

### Verifying this build

This repository is developed in a headless CI environment, so the automated bar is:
`cargo build` + `cargo clippy --all-targets -- -D warnings` clean, `cargo test` green
(parsers, wallet checksums, fee resolution, lottery math, config round-trip), and frontend
`npm run build` + `npm test` green.

The following require a desktop with a display and are verified manually on your machine:

- `npm run tauri dev` launches the window and the dashboard renders live BTC network data.
- A real miner binary mines against a low-difficulty pool with ~2s telemetry updates and a clean
  stop that leaves no orphaned process.
- The log-scale lottery bar tracks the best share; expected-time-to-block matches the hand calc
  (e.g. 6 TH/s vs ~1.024 ZH/s at 600s ≈ ~3,250 years).
