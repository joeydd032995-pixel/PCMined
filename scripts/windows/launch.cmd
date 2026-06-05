@echo off
setlocal
title Lottery Ticket Terminal - Launcher

REM This launcher installs the build toolchain (Rust, Node, MSVC Build Tools,
REM WebView2) via winget, builds the app from source, and runs it.

REM Resolve a PowerShell executable by FULL PATH so a broken/minimal PATH (where
REM "powershell" isn't found, error 9009) still works. Fall back progressively.
set "PS=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
if not exist "%PS%" set "PS=%windir%\System32\WindowsPowerShell\v1.0\powershell.exe"
if not exist "%PS%" set "PS=powershell.exe"
where pwsh >nul 2>&1 && if not exist "%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" set "PS=pwsh.exe"

REM winget machine-scope installs and the VS Build Tools need administrator
REM rights, so self-elevate if we're not already running as admin.
net session >nul 2>&1
if %errorlevel% NEQ 0 (
    echo Requesting administrator privileges...
    "%PS%" -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)

set "SCRIPT_DIR=%~dp0"
"%PS%" -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%bootstrap.ps1" %*
set "RC=%errorlevel%"

echo.
if "%RC%"=="0" (
    echo Launcher finished successfully.
) else (
    echo Launcher exited with error code %RC%.
    if "%RC%"=="9009" (
        echo.
        echo PowerShell could not be found on this system. Expected at:
        echo   %SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe
        echo If that file exists, your PATH may be missing C:\Windows\System32.
    )
)
echo Press any key to close this window.
pause >nul
