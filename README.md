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
- P1 — Process engine + telemetry (cpuminer-opt, lolMiner)
- P2 — Config / profiles / wallet checksum validation
- P3 — Lottery dashboard + live network APIs
- P4–P7 — Multi-miner/hardware, reliability/tray, binary verification/packaging, polish/docs

See `MinerAdapter` + `MinerRegistry` (added in P1/P2) for the coin/miner extension points.
