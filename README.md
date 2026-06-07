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

### Windows quick start (one click)

On Windows 10/11, double-click **`scripts\windows\launch.cmd`**. It installs the whole toolchain
(Rust, Node, MSVC C++ Build Tools, WebView2) via winget, builds the app from source, and launches
it. See [`scripts/windows/README.md`](scripts/windows/README.md) for details. (First run downloads
a large toolchain and takes several minutes; re-runs are fast.)

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
- **P4 — Multi-miner + hardware + monitor-only** ✅ `hardware.rs` (CPU/RAM/GPU detect, thread
  suggestions, hardware gating), kawpowminer/ethminer JSON-RPC adapters, Bitaxe AxeOS
  monitor-only adapter, concurrent instances + contention warning.
- **P5 — Reliability** ✅ `watchdog.rs` (backoff+jitter, circuit breaker, pool rotation),
  reconnect/recovery in the poll task, graceful shutdown (zero orphans), system tray,
  block-found notifications.
- **P6 — Binaries + packaging** ✅ `binaries.rs` (download + SHA-256 verify before exec, semver
  update check), Tauri bundle config, release CI (tauri-action matrix), CI verification workflow.
- **P7 — Polish + docs** ✅ tabbed UI, live log viewer (verbose toggle + export), settings
  (theme/tray/binaries dir/config backup+restore), keyboard shortcuts, status dots.

## Contributing: add a coin or miner

The roster is data-driven — most additions are a single adapter + a single registry entry.

**Add a miner** (e.g. a new GPU miner):

1. Create `src-tauri/src/miner/adapters/<miner>.rs` implementing `MinerAdapter`
   (`info`, `build_args`, `read`). Define its `pub const INFO: MinerInfo` with the **dev fee,
   open-source flag, source URL, license, and telemetry kind** — fee is mandatory and shown in
   the UI. Reuse a telemetry transport in `miner/telemetry.rs` (or add one) and a tolerant parser.
2. Register it: add the module to `miner/adapters/mod.rs`, add `INFO` to `all_miners()` in
   `config/miners.rs`, and (if spawnable) add it to the `Supervisor::new` adapter list and the
   `candidates()` map. Write parser/`build_args` unit tests.

**Add a coin:**

1. Add a `CoinInfo` to `COINS` in `config/coins.rs` (algo, block time, default pools) and, if its
   difficulty→hashrate relationship differs, extend `network_hashrate_from_difficulty`.
2. Add an address-validation branch in `src-tauri/src/wallet.rs` — **with a real checksum**, not
   just a regex — and a test against a known-good public address.
3. Map the coin to its miner(s) in `candidates()` (`config/miners.rs`).

No core/UI changes are required: fee badges, lottery odds, validation, and the dashboard all
read from these registries.

## Coins & wallets

Roster: Monero, Ravencoin, Ethereum Classic, **EthereumPoW, Octa Space, Callisto** (EVM/Ethash),
Kaspa, **Karlsen, Pyrin** (Kaspa-fork CashAddr), Ergo, and Bitcoin/Litecoin/Dogecoin (monitor-only).
Every coin has a **real checksum** address validator (EIP-55 for EVM chains, the CashAddr polymod
for the Kaspa family, Base58Check, Monero base-58+Keccak, bech32, Blake2b for Ergo) and a
**suggested wallet** with an install/open link in the dashboard.

> Wallet note: a desktop app's webview has no browser extensions, so it can't "connect" to
> MetaMask/Phantom to read an address. The supported, secure flow is: pick the suggested wallet,
> copy your **public receiving address**, paste it into a profile, and the app validates the
> checksum before any miner can use it. Private keys are never involved.

### Packaging & signing

Cross-platform installers are built by `.github/workflows/release.yml` (tauri-action
matrix: macOS arm64 + x86_64, Linux, Windows) on a `v*` tag push. Code-signing and
notarization run only when the corresponding repo secrets are present:

- **macOS:** `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
  `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` (notarization via notarytool).
- **Windows:** a code-signing certificate / Azure Trusted Signing wired into the
  tauri-action step.

These cannot run in a headless Linux CI container, so signed artifacts are produced by the
release workflow (with secrets) or on a maintainer's machine.

**Managed binaries & antivirus.** Miner binaries are downloaded on demand from official
release URLs into a managed dir and **SHA-256-verified before they are ever extracted or made
executable** (`binaries.rs`); a tampered or corrupt download is refused pre-exec. **lolMiner
1.98a** is pinned with the real SHA-256 of its official Linux (`Lin64.tar.gz`) and Windows
(`Win64.zip`) release assets and is auto-fetchable from Settings → "Download". Miners that don't
publish prebuilt binaries on their official releases (e.g. cpuminer-opt on Linux) are installed
manually — point a profile's binary path at your own build. Mining binaries are frequently
flagged by antivirus as false positives; we mitigate by signing + notarizing, downloading only
from official sources, and never obfuscating. Document this for users so they can allowlist the app.

The end-to-end fetch path (download → verify real SHA-256 → extract → set exec bit) is covered by
an opt-in integration test against the real release:

```bash
cd src-tauri && cargo test -- --ignored real_fetch_lolminer
```

### First real run (mine your own hardware)

1. Pick a GPU coin whose miner is auto-fetchable — **Kaspa** or **Ergo** (both use lolMiner).
2. Settings → Miner binaries → **Check for updates** → **Download** (verifies SHA-256, installs).
3. On the coin's panel, create a profile with your **own** wallet address. A default solo pool is
   prefilled (HeroMiners for Kaspa with the `solo:` prefix; 2Miners `solo-*` hosts for Ergo/RVN/ETC)
   — verify it with **Test pool** first; endpoints and fees drift.
4. Start the miner. Live hashrate / shares / best-share appear within ~2s; the log-scale lottery bar
   tracks your best share toward network difficulty. Remember: high-variance lottery, not income.

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
