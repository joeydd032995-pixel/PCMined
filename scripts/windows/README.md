# Windows launcher

One-click bootstrap that installs the build toolchain, builds the **Lottery Ticket Terminal**
from source, and runs it — for Windows 10/11 (x64).

## Use it

1. Download/clone this repository (or just these two files into the same folder).
2. Double-click **`launch.cmd`**.
3. Approve the **User Account Control (admin)** prompt — needed to install system tools.
4. Wait. The first run downloads a large toolchain (~2–4 GB) and the first build takes several
   minutes. When it finishes, the app opens and the installer (`.msi`/`.exe`) is revealed in
   `src-tauri\target\release\bundle\`.

Re-running `launch.cmd` is fast: already-installed tools are skipped.

## What it installs (via [winget](https://learn.microsoft.com/windows/package-manager/winget/))

| Tool | winget id | Why |
|------|-----------|-----|
| Git | `Git.Git` | fetch/update the source |
| Node.js LTS | `OpenJS.NodeJS.LTS` | build the frontend (`npm`) |
| Rust (rustup) | `Rustlang.Rustup` | build the Rust core (set to `stable-msvc`) |
| MSVC C++ Build Tools | `Microsoft.VisualStudio.2022.BuildTools` (VCTools workload) | C/C++ compiler + Windows SDK the Rust MSVC toolchain links against |
| WebView2 Runtime | `Microsoft.EdgeWebView2Runtime` | the Tauri webview (usually already present on Win10/11) |

> Note: the **miner binaries themselves are not installed here.** The app downloads and
> **SHA-256-verifies** them at runtime (Settings → Miner binaries → Download), so the launcher only
> needs the build toolchain.

## SmartScreen / antivirus

`launch.cmd` is an unsigned script, so Windows SmartScreen may warn. You can read both files first
(they're plain text) and then choose **More info → Run anyway**. Mining binaries downloaded later
are also commonly flagged as false positives — see the AV note in the project README.

## If something goes wrong

- **"'powershell' is not recognized" / error 9009** — your `PATH` is missing the
  `C:\Windows\System32\WindowsPowerShell\v1.0\` entry. The launcher now calls PowerShell by its
  full path via `%SystemRoot%`, so updating to the latest `launch.cmd` fixes this. (You can also add
  that folder back to your PATH, or run `bootstrap.ps1` directly from an elevated PowerShell.)
- **"winget was not found"** — install *App Installer* from the Microsoft Store, then re-run.
- **"linker `link.exe` not found"** — the MSVC C++ workload wasn't installed. The launcher now
  installs it via the official VS Build Tools bootstrapper (the `VCTools` workload) and loads the
  VS developer environment before building. Re-run `launch.cmd`; if it persists, open the *Visual
  Studio Installer* and add **"Desktop development with C++"**, then re-run.
- **"node/npm/cargo is still not on PATH"** — close the window and double-click `launch.cmd` again;
  a fresh shell picks up the newly installed tools.
- The PowerShell logic lives in `bootstrap.ps1`; you can run it directly in an elevated PowerShell:
  `powershell -ExecutionPolicy Bypass -File .\bootstrap.ps1`.

## Reminder

Solo mining is a **high-variance lottery, not income**. Use only on hardware you own.
