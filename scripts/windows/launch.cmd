@echo off
setlocal
title Lottery Ticket Terminal - Launcher

REM This launcher installs the build toolchain (Rust, Node, MSVC Build Tools,
REM WebView2) via winget, builds the app from source, and runs it.
REM winget machine-scope installs and the VS Build Tools need administrator
REM rights, so self-elevate if we're not already running as admin.

net session >nul 2>&1
if %errorlevel% NEQ 0 (
    echo Requesting administrator privileges...
    powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)

set "SCRIPT_DIR=%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%bootstrap.ps1" %*
set "RC=%errorlevel%"

echo.
if "%RC%"=="0" (
    echo Launcher finished successfully.
) else (
    echo Launcher exited with error code %RC%.
)
echo Press any key to close this window.
pause >nul
