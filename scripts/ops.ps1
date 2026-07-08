<#
.SYNOPSIS
    Helper for Makefile targets - start/stop/test the server and desktop shell.
.DESCRIPTION
    Called from the Makefile with positional args to avoid $-escaping issues
    with GNU make -> sh -> PowerShell.

    Single self-dispatching binary: `omnilauncher.exe` owns every runtime mode
    (GUI, serve, and the `ol` CLI). The historical frontend/backend role copies
    are gone — lifecycle commands delegate to the binary's own subcommands
    (start / stop / status / gui), which track PID files under
    ~/.omnilauncher/run/.
#>
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet(
        'stop-frontend', 'stop-backend', 'stop-all',
        'start-frontend', 'start-backend',
        'start-wsl-backend', 'restart-wsl-backend',
        'prod-debug-backend', 'prod-debug-frontend', 'prod-debug',
        'test-backend', 'remove-binary',
        'clean-frontend', 'clean-backend', 'clean',
        'status'
    )]
    [string]$Action,

    [string]$ServerHost   = '0.0.0.0',
    [string]$ServerPort    = '1422',
    [string]$BackendUrl   = 'http://127.0.0.1:1422',
    [string]$Role         = 'both',
    [switch]$DebugFlag
)

$ErrorActionPreference = 'SilentlyContinue'
$binDir  = Join-Path $PSScriptRoot '..\src-tauri\target\release'
$baseExe = Join-Path $binDir 'omnilauncher.exe'
$guiPidFile = Join-Path $env:USERPROFILE '.omnilauncher\run\omnilauncher-gui.pid'

function Ensure-Binary {
    if (-not (Test-Path $baseExe)) {
        Write-Host "Release binary not found at $baseExe" -ForegroundColor Red
        Write-Host 'Run: make build' -ForegroundColor Red
        exit 1
    }
}

function Remove-Binaries {
    Remove-Item -Force $baseExe -ErrorAction SilentlyContinue
    Write-Host "Removed $baseExe" -ForegroundColor Green
}

# Backend lifecycle delegates to the self-contained binary, which spawns a
# detached `serve`, tracks its PID under ~/.omnilauncher/run/, and waits for
# /health.
function Start-Backend {
    Ensure-Binary
    $env:OMNILAUNCHER_SERVER_HOST = $ServerHost
    $env:OMNILAUNCHER_SERVER_PORT = $ServerPort
    if ($DebugFlag) { & $baseExe start --debug } else { & $baseExe start }
}

function Stop-Backend {
    Ensure-Binary
    & $baseExe stop
}

# Desktop shell lifecycle. `gui --detached` backgrounds the shell and writes
# ~/.omnilauncher/run/omnilauncher-gui.pid; we stop it via that file.
function Start-Frontend {
    Ensure-Binary
    $env:OMNILAUNCHER_BACKEND_URL = $BackendUrl
    if ($DebugFlag) { & $baseExe gui --detached --debug } else { & $baseExe gui --detached }
}

function Stop-Frontend {
    if (Test-Path $guiPidFile) {
        $procId = Get-Content $guiPidFile -ErrorAction SilentlyContinue
        if ($procId) {
            Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
        }
        Remove-Item -Force $guiPidFile -ErrorAction SilentlyContinue
    }
}

function Start-ProdDebugBackend {
    Ensure-Binary
    $env:OMNILAUNCHER_SERVER_HOST = $ServerHost
    $env:OMNILAUNCHER_SERVER_PORT = $ServerPort
    & $baseExe start --debug
}

function Start-ProdDebugFrontend {
    Ensure-Binary
    $env:OMNILAUNCHER_BACKEND_URL = $BackendUrl
    & $baseExe gui --detached --debug
}

# WSL split-machine deploy: build + run the (single) binary inside WSL in
# `serve` mode, with the Windows desktop shell connecting via BackendUrl. This
# stays Windows-only because it drives wsl.exe.
function Start-WslBackend {
    Write-Host 'Building backend inside WSL...' -ForegroundColor Cyan
    wsl -e bash -c 'cd /mnt/c/Users/jzhu/repos/OmniLauncher/src-tauri && cargo build --release'
    if ($LASTEXITCODE -ne 0) {
        Write-Host 'WSL build failed!' -ForegroundColor Red
        exit 1
    }
    Write-Host "Starting backend inside WSL on $ServerHost`:$ServerPort..." -ForegroundColor Cyan
    wsl -e bash -c "OMNILAUNCHER_SERVER_HOST=$ServerHost OMNILAUNCHER_SERVER_PORT=$ServerPort nohup /mnt/c/Users/jzhu/repos/OmniLauncher/src-tauri/target/release/omnilauncher serve >/dev/null 2>&1 &"
    Write-Host "WSL backend started. Frontend should connect via BACKEND_URL=$BackendUrl" -ForegroundColor Green
}

function Restart-WslBackend {
    Write-Host 'Stopping backend...' -ForegroundColor Cyan
    Stop-Backend
    wsl -e bash -c 'rm -f /mnt/c/Users/jzhu/repos/OmniLauncher/src-tauri/target/release/omnilauncher'
    Write-Host 'Building and starting WSL backend...' -ForegroundColor Cyan
    wsl -e bash -c 'cd /mnt/c/Users/jzhu/repos/OmniLauncher/src-tauri && cargo build --release && OMNILAUNCHER_SERVER_HOST=$env:ServerHost OMNILAUNCHER_SERVER_PORT=$env:ServerPort nohup /mnt/c/Users/jzhu/repos/OmniLauncher/src-tauri/target/release/omnilauncher serve >/dev/null 2>&1 &'
    Write-Host 'WSL backend restarted.' -ForegroundColor Green
}

function Test-Backend {
    Write-Host "Checking backend health at $BackendUrl/health ..." -ForegroundColor Cyan
    try {
        $resp = Invoke-WebRequest -Uri "$BackendUrl/health" -UseBasicParsing -TimeoutSec 5
        Write-Host 'Backend is running:' -ForegroundColor Green
        Write-Host $resp.Content
    }
    catch {
        Write-Host "Backend is NOT responding at $BackendUrl" -ForegroundColor Red
        exit 1
    }
}

# Delegate to the binary's own rich status. Informational, so we don't
# propagate its exit code (it exits non-zero when no managed backend runs).
function Show-Status {
    Ensure-Binary
    & $baseExe status
}

switch ($Action) {
    'stop-frontend'        { Stop-Frontend }
    'stop-backend'         { Stop-Backend }
    'stop-all'             { Stop-Frontend; Stop-Backend }
    'start-frontend'       { Start-Frontend }
    'start-backend'        { Start-Backend }
    'prod-debug-backend'   { Start-ProdDebugBackend }
    'prod-debug-frontend'  { Start-ProdDebugFrontend }
    'prod-debug'           { Start-ProdDebugBackend; Start-ProdDebugFrontend }
    'start-wsl-backend'    { Start-WslBackend }
    'restart-wsl-backend'  { Restart-WslBackend }
    'test-backend'         { Test-Backend }
    'status'               { Show-Status }
    'remove-binary'        { Remove-Binaries }
    'clean-frontend'       { Remove-Item -Recurse -Force (Join-Path $PSScriptRoot '..\dist') -ErrorAction SilentlyContinue }
    'clean-backend'        { Remove-Item -Recurse -Force (Join-Path $PSScriptRoot '..\src-tauri\target') -ErrorAction SilentlyContinue }
    'clean'                { Remove-Item -Recurse -Force (Join-Path $PSScriptRoot '..\dist') -ErrorAction SilentlyContinue; Remove-Item -Recurse -Force (Join-Path $PSScriptRoot '..\src-tauri\target') -ErrorAction SilentlyContinue }
}
