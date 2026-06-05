<#
.SYNOPSIS
  One-click bootstrap for the Lottery Ticket Terminal on Windows 10/11.

.DESCRIPTION
  Installs the build toolchain (Git, Node LTS, Rust MSVC, MSVC C++ Build Tools,
  WebView2) via winget, then builds and launches the app from source. Idempotent
  and safe to re-run — already-installed tools are skipped.

  The app itself downloads and SHA-256-verifies miner binaries at runtime, so
  this launcher only stands up the build toolchain and produces/runs the app.
#>
[CmdletBinding()]
param(
    # Branch to clone when the script is run outside a checkout. Defaults to the
    # feature branch until it merges to main.
    [string]$Branch = 'claude/solo-miner-p4-p7-KyA5T',
    [string]$RepoUrl = 'https://github.com/joeydd032995-pixel/PCMined.git'
)

$ErrorActionPreference = 'Stop'

function Write-Phase($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Write-Ok($msg) { Write-Host "    $msg" -ForegroundColor Green }
function Write-Warn2($msg) { Write-Host "    $msg" -ForegroundColor Yellow }

# Rebuild PATH from the registry (winget installs don't update the current shell)
# and make sure cargo's bin dir is present.
function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $cargo = Join-Path $env:USERPROFILE '.cargo\bin'
    $env:Path = (@($machine, $user, $cargo) | Where-Object { $_ }) -join ';'
}

function Test-Cmd($name) {
    return [bool](Get-Command $name -ErrorAction SilentlyContinue)
}

# Install a winget package if it isn't already present. Treats "already
# installed" / "no applicable upgrade" exit codes as success.
function Install-WingetPackage {
    param(
        [Parameter(Mandatory)] [string]$Id,
        [string]$Override
    )
    & winget list --id $Id -e --accept-source-agreements *> $null
    if ($LASTEXITCODE -eq 0) {
        Write-Ok "$Id already installed."
        return
    }

    Write-Phase "Installing $Id ..."
    $wingetArgs = @(
        'install', '--id', $Id, '-e', '--source', 'winget',
        '--accept-package-agreements', '--accept-source-agreements',
        '--disable-interactivity'
    )
    if ($Override) {
        $wingetArgs += @('--override', $Override)
    } else {
        $wingetArgs += '--silent'
    }
    & winget @wingetArgs
    $code = $LASTEXITCODE
    # 0 = ok; 0x8A15002B (-1978335189) = no applicable upgrade / already installed.
    if ($code -ne 0 -and $code -ne -1978335189) {
        throw "winget failed to install $Id (exit $code)"
    }
    Write-Ok "$Id installed."
}

# Find a Visual Studio install that has the MSVC x64 C++ toolchain, via vswhere.
function Get-VcInstallPath {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { return $null }
    $p = & $vswhere -latest -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath 2>$null
    if ($p) { return ($p | Select-Object -First 1) }
    return $null
}

# Ensure the MSVC C++ Build Tools (compiler + linker + Windows SDK) are present.
# winget's BuildTools package frequently installs the shell without the C++
# workload, so use the official bootstrapper with an explicit --add VCTools
# (idempotent: it modifies an existing install to add the workload).
function Install-MsvcBuildTools {
    $vc = Get-VcInstallPath
    if ($vc) { Write-Ok "MSVC C++ tools present: $vc"; return $vc }

    Write-Phase 'Installing MSVC C++ Build Tools (VCTools) - large download, please wait'
    $installer = Join-Path $env:TEMP 'vs_BuildTools.exe'
    Invoke-WebRequest -Uri 'https://aka.ms/vs/17/release/vs_BuildTools.exe' -OutFile $installer
    $proc = Start-Process -FilePath $installer -Wait -PassThru -ArgumentList @(
        '--quiet', '--wait', '--norestart', '--nocache',
        '--add', 'Microsoft.VisualStudio.Workload.VCTools', '--includeRecommended'
    )
    # 0 = ok; 3010 = ok but reboot recommended.
    if ($proc.ExitCode -ne 0 -and $proc.ExitCode -ne 3010) {
        throw "Visual Studio Build Tools installer failed (exit $($proc.ExitCode))."
    }
    $vc = Get-VcInstallPath
    if (-not $vc) {
        throw 'MSVC C++ tools still not detected. Reboot and re-run, or open the Visual Studio Installer and add the "Desktop development with C++" workload.'
    }
    Write-Ok "MSVC C++ tools installed: $vc"
    return $vc
}

# Load the VS developer environment into this session so link.exe, the C/C++
# headers, and the Windows SDK libs are on PATH/INCLUDE/LIB for the Rust build.
function Enter-DevEnv($vcPath) {
    $dll = Join-Path $vcPath 'Common7\Tools\Microsoft.VisualStudio.DevShell.dll'
    if (-not (Test-Path $dll)) { return $false }
    Import-Module $dll -ErrorAction Stop
    Enter-VsDevShell -VsInstallPath $vcPath -SkipAutomaticLocation `
        -DevCmdArguments '-arch=x64 -host_arch=x64 -no_logo' | Out-Null
    return $true
}

try {
    Write-Host "Lottery Ticket Terminal - Windows launcher" -ForegroundColor Magenta
    Write-Host "This installs a build toolchain (~2-4 GB) and builds the app from source." -ForegroundColor Magenta

    # --- Preflight -----------------------------------------------------------
    Write-Phase 'Checking for winget (App Installer)'
    if (-not (Test-Cmd 'winget')) {
        throw @'
winget was not found. Install "App Installer" from the Microsoft Store, then re-run:
  https://apps.microsoft.com/detail/9NBLGGH4NNS1
'@
    }
    Write-Ok 'winget present.'

    # --- Toolchain -----------------------------------------------------------
    Install-WingetPackage -Id 'Git.Git'
    Install-WingetPackage -Id 'OpenJS.NodeJS.LTS'
    Install-WingetPackage -Id 'Rustlang.Rustup'
    Install-WingetPackage -Id 'Microsoft.EdgeWebView2Runtime'

    Update-SessionPath

    # Ensure the Rust MSVC toolchain is the default.
    if (Test-Cmd 'rustup') {
        Write-Phase 'Configuring Rust (stable, MSVC)'
        & rustup default stable-x86_64-pc-windows-msvc
        Update-SessionPath
    } else {
        Write-Warn2 'rustup not on PATH yet; you may need to re-run this launcher once.'
    }

    # MSVC C++ Build Tools provide link.exe + Windows SDK that the Rust MSVC
    # target links against. Installed via the official bootstrapper (reliable).
    $vcPath = Install-MsvcBuildTools

    # --- Source --------------------------------------------------------------
    Write-Phase 'Locating project source'
    $repoRoot = $null
    $candidate = Join-Path $PSScriptRoot '..\..'
    if ((Test-Path (Join-Path $candidate 'package.json')) -and (Test-Path (Join-Path $candidate 'src-tauri'))) {
        $repoRoot = (Resolve-Path $candidate).Path
        Write-Ok "Using existing checkout: $repoRoot"
        if ((Test-Cmd 'git') -and (Test-Path (Join-Path $repoRoot '.git'))) {
            Push-Location $repoRoot
            & git pull --ff-only 2>$null
            Pop-Location
        }
    } else {
        $cloneDir = Join-Path $env:LOCALAPPDATA 'PCMined'
        if (Test-Path (Join-Path $cloneDir '.git')) {
            Write-Ok "Updating clone at $cloneDir"
            Push-Location $cloneDir
            & git fetch origin $Branch
            & git checkout $Branch
            & git pull --ff-only
            Pop-Location
        } else {
            Write-Ok "Cloning $RepoUrl ($Branch) into $cloneDir"
            & git clone --branch $Branch --depth 1 $RepoUrl $cloneDir
        }
        $repoRoot = (Resolve-Path $cloneDir).Path
    }

    Set-Location $repoRoot

    # --- Sanity checks -------------------------------------------------------
    foreach ($tool in 'node', 'npm', 'cargo') {
        if (-not (Test-Cmd $tool)) {
            throw "$tool is still not on PATH. Close this window and run the launcher again (a fresh shell picks up the new PATH)."
        }
    }
    Write-Ok ("node {0}, npm {1}, cargo present." -f (& node --version), (& npm --version))

    # Load the VS developer environment so the Rust build finds link.exe.
    Write-Phase 'Loading Visual Studio developer environment'
    if (Enter-DevEnv $vcPath) {
        Set-Location $repoRoot  # in case the dev shell changed the location
        if (Test-Cmd 'link') { Write-Ok 'MSVC linker (link.exe) is available.' }
        else { Write-Warn2 'link.exe still not visible; the build may fail.' }
    } else {
        Write-Warn2 'Could not load the VS DevShell module; relying on cargo auto-detection.'
    }

    # --- Build ---------------------------------------------------------------
    Write-Phase 'Installing JS dependencies (npm ci)'
    & npm ci
    if ($LASTEXITCODE -ne 0) {
        Write-Warn2 'npm ci failed; falling back to npm install.'
        & npm install
        if ($LASTEXITCODE -ne 0) { throw 'npm install failed.' }
    }

    Write-Phase 'Building the app (npm run tauri build) - first build takes several minutes'
    & npm run tauri build
    if ($LASTEXITCODE -ne 0) { throw 'tauri build failed.' }

    # --- Launch --------------------------------------------------------------
    Write-Phase 'Launching the app'
    $exe = Join-Path $repoRoot 'src-tauri\target\release\Lottery Ticket Terminal.exe'
    $bundleDir = Join-Path $repoRoot 'src-tauri\target\release\bundle'
    if (Test-Path $exe) {
        Start-Process -FilePath $exe
        Write-Ok "Started: $exe"
    } else {
        Write-Warn2 "Built exe not found at expected path; opening the bundle folder instead."
    }
    if (Test-Path $bundleDir) {
        Write-Ok "Installers are in: $bundleDir"
        Start-Process -FilePath 'explorer.exe' -ArgumentList $bundleDir
    }

    Write-Host "`nDone. The app is running and the installer (.msi/.exe) is in the bundle folder." -ForegroundColor Green
}
catch {
    Write-Host "`nLAUNCHER ERROR:" -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    Write-Host "`nTip: re-running the launcher often fixes PATH-timing issues after a fresh toolchain install." -ForegroundColor Yellow
    exit 1
}
