<#
.SYNOPSIS
    Helper for Makefile targets - start/stop/test the server and frontend.
.DESCRIPTION
    Called from the Makefile with positional args to avoid $-escaping issues
    with GNU make -> sh -> PowerShell.
#>
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet(
        'stop-frontend', 'stop-backend', 'stop-all',
        'start-frontend', 'start-backend',
        'start-wsl-backend', 'restart-wsl-backend',
        'prod-debug-backend', 'prod-debug-frontend', 'prod-debug',
        'test-backend', 'remove-binary', 'prepare-binaries',
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
$binDir      = Join-Path $PSScriptRoot '..\src-tauri\target\release'
$baseExe     = Join-Path $binDir 'omnilauncher.exe'
$frontendExe = Join-Path $binDir 'omnilauncher-frontend.exe'
$backendExe  = Join-Path $binDir 'omnilauncher-backend.exe'

# Copy the freshly-built omnilauncher binary into role-named file(s) and
# remove the generic source so we don't ship three identical files.
#   -Which frontend  -> only omnilauncher-frontend remains
#   -Which backend   -> only omnilauncher-backend remains
#   -Which both      -> both role files exist; bare omnilauncher gone
function Prepare-Binaries {
    param([string]$Which = 'both')
    if ($Which -notin @('frontend','backend','both')) {
        Write-Host "Prepare-Binaries: unknown role '$Which'" -ForegroundColor Red
        exit 2
    }

    if (-not (Test-Path $baseExe)) {
        $haveFe = Test-Path $frontendExe
        $haveBe = Test-Path $backendExe
        switch ($Which) {
            'frontend' { if ($haveFe) { return } }
            'backend'  { if ($haveBe) { return } }
            'both'     { if ($haveFe -and $haveBe) { return } }
        }
        Write-Host 'Release binary not found. Run: make build-frontend or make build-backend' -ForegroundColor Red
        exit 1
    }

    if ($Which -in @('frontend','both')) { Copy-Item -Force $baseExe $frontendExe }
    if ($Which -in @('backend','both'))  { Copy-Item -Force $baseExe $backendExe }
    Remove-Item -Force $baseExe -ErrorAction SilentlyContinue

    Write-Host "Prepared role binaries (role=$Which):" -ForegroundColor Green
    if (Test-Path $frontendExe) { Write-Host "  frontend: $frontendExe" }
    if (Test-Path $backendExe)  { Write-Host "  backend:  $backendExe" }
}

function Ensure-RoleBinaries {
    param([string]$Which = 'both')
    switch ($Which) {
        'frontend' { if (Test-Path $frontendExe) { return } }
        'backend'  { if (Test-Path $backendExe) { return } }
        'both'     { if ((Test-Path $frontendExe) -and (Test-Path $backendExe)) { return } }
    }
    Prepare-Binaries -Which $Which
}

function Remove-Binaries {
    Remove-Item -Force $baseExe -ErrorAction SilentlyContinue
    Remove-Item -Force $frontendExe -ErrorAction SilentlyContinue
    Remove-Item -Force $backendExe -ErrorAction SilentlyContinue
}

function Stop-Frontend {
    Get-Process omnilauncher-frontend, omnilauncher -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
}

function Stop-Backend {
    Get-Process omnilauncher-backend -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
    Get-NetTCPConnection -LocalPort $ServerPort -State Listen -ErrorAction SilentlyContinue |
        Select-Object -First 1 |
        ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }
}

function Start-Frontend {
    Ensure-RoleBinaries -Which frontend
    $env:OMNILAUNCHER_BACKEND_URL = $BackendUrl
    $argList = @()
    if ($DebugFlag) { $argList += '--debug' }
    if ($argList.Count -gt 0) {
        Start-Process -FilePath $frontendExe -ArgumentList $argList -WorkingDirectory (Get-Location)
    } else {
        Start-Process -FilePath $frontendExe -WorkingDirectory (Get-Location)
    }
}

function Start-Backend {
    Ensure-RoleBinaries -Which backend
    $env:OMNILAUNCHER_SERVER_HOST = $ServerHost
    $env:OMNILAUNCHER_SERVER_PORT = $ServerPort
    $argList = @('--server')
    if ($DebugFlag) { $argList += '--debug' }
    Start-Process -FilePath $backendExe -ArgumentList $argList -WorkingDirectory (Get-Location)
}

function Start-ProdDebugBackend {
    Ensure-RoleBinaries -Which backend
    $env:OMNILAUNCHER_SERVER_HOST = $ServerHost
    $env:OMNILAUNCHER_SERVER_PORT = $ServerPort
    Start-Process -FilePath $backendExe -ArgumentList '--server','--debug' -WorkingDirectory (Get-Location)
}

function Start-ProdDebugFrontend {
    Ensure-RoleBinaries -Which frontend
    $env:OMNILAUNCHER_BACKEND_URL = $BackendUrl
    Start-Process -FilePath $frontendExe -ArgumentList '--debug' -WorkingDirectory (Get-Location)
}

function Start-WslBackend {
    Write-Host 'Building backend inside WSL...' -ForegroundColor Cyan
    wsl -e bash -c 'cd /mnt/c/Users/jzhu/repos/OmniLauncher/src-tauri && cargo build --release'
    if ($LASTEXITCODE -ne 0) {
        Write-Host 'WSL build failed!' -ForegroundColor Red
        exit 1
    }
    Write-Host "Starting backend inside WSL on $ServerHost`:$ServerPort..." -ForegroundColor Cyan
    wsl -e bash -c "OMNILAUNCHER_SERVER_HOST=$ServerHost OMNILAUNCHER_SERVER_PORT=$ServerPort nohup /mnt/c/Users/jzhu/repos/OmniLauncher/src-tauri/target/release/omnilauncher --server >/dev/null 2>&1 &"
    Write-Host "WSL backend started. Frontend should connect via BACKEND_URL=$BackendUrl" -ForegroundColor Green
}

function Restart-WslBackend {
    Write-Host 'Stopping backend...' -ForegroundColor Cyan
    Stop-Backend
    wsl -e bash -c 'rm -f /mnt/c/Users/jzhu/repos/OmniLauncher/src-tauri/target/release/omnilauncher'
    Write-Host 'Building and starting WSL backend...' -ForegroundColor Cyan
    wsl -e bash -c 'cd /mnt/c/Users/jzhu/repos/OmniLauncher/src-tauri && cargo build --release && OMNILAUNCHER_SERVER_HOST=$env:ServerHost OMNILAUNCHER_SERVER_PORT=$env:ServerPort nohup /mnt/c/Users/jzhu/repos/OmniLauncher/src-tauri/target/release/omnilauncher --server >/dev/null 2>&1 &'
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

function Show-Status {
    Write-Host ''
    Write-Host '=== OmniLauncher Status ===' -ForegroundColor Cyan
    Write-Host ''

    # --- Binaries ---
    Write-Host '--- Binaries ---' -ForegroundColor Yellow
    if (Test-Path $frontendExe) {
        $sz = [math]::Round((Get-Item $frontendExe).Length / 1MB, 1)
        Write-Host "  frontend exe: OK  ($sz MB)" -ForegroundColor Green
    } else {
        Write-Host '  frontend exe: MISSING' -ForegroundColor Red
    }
    if (Test-Path $backendExe) {
        $sz = [math]::Round((Get-Item $backendExe).Length / 1MB, 1)
        Write-Host "  backend  exe: OK  ($sz MB)" -ForegroundColor Green
    } else {
        Write-Host '  backend  exe: MISSING' -ForegroundColor Red
    }

    # --- Processes ---
    Write-Host '--- Processes ---' -ForegroundColor Yellow
    $feProc = Get-Process omnilauncher-frontend -ErrorAction SilentlyContinue
    if ($feProc) {
        foreach ($p in $feProc) {
            $mem = [math]::Round($p.WorkingSet64 / 1MB, 1)
            Write-Host "  frontend: RUNNING  PID=$($p.Id)  MEM=${mem}MB" -ForegroundColor Green
        }
    } else {
        Write-Host '  frontend: STOPPED' -ForegroundColor Red
    }

    $beProc = Get-Process omnilauncher-backend -ErrorAction SilentlyContinue
    if ($beProc) {
        foreach ($p in $beProc) {
            $mem = [math]::Round($p.WorkingSet64 / 1MB, 1)
            Write-Host "  backend:  RUNNING  PID=$($p.Id)  MEM=${mem}MB" -ForegroundColor Green
        }
    } else {
        Write-Host '  backend:  STOPPED' -ForegroundColor Red
    }

    # --- Port ---
    Write-Host '--- Network ---' -ForegroundColor Yellow
    $listener = Get-NetTCPConnection -LocalPort $ServerPort -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($listener) {
        Write-Host "  port $ServerPort`: LISTENING  PID=$($listener.OwningProcess)" -ForegroundColor Green
    } else {
        Write-Host "  port $ServerPort`: NOT LISTENING" -ForegroundColor Red
    }

    # --- Health check ---
    Write-Host '--- Health ---' -ForegroundColor Yellow
    try {
        $resp = Invoke-WebRequest -Uri "$BackendUrl/health" -UseBasicParsing -TimeoutSec 3
        $body = $resp.Content | ConvertFrom-Json
        if ($body.ok -eq $true) {
            Write-Host "  $BackendUrl/health: OK" -ForegroundColor Green
        } else {
            Write-Host "  $BackendUrl/health: UNEXPECTED ($($resp.Content))" -ForegroundColor Yellow
        }
    }
    catch {
        Write-Host "  $BackendUrl/health: UNREACHABLE" -ForegroundColor Red
    }

    Write-Host ''
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
    'prepare-binaries'     { Prepare-Binaries -Which $Role }
    'remove-binary'        { Remove-Binaries }
    'clean-frontend'       { Remove-Item -Recurse -Force (Join-Path $PSScriptRoot '..\dist') -ErrorAction SilentlyContinue }
    'clean-backend'        { Remove-Item -Recurse -Force (Join-Path $PSScriptRoot '..\src-tauri\target') -ErrorAction SilentlyContinue }
    'clean'                { Remove-Item -Recurse -Force (Join-Path $PSScriptRoot '..\dist') -ErrorAction SilentlyContinue; Remove-Item -Recurse -Force (Join-Path $PSScriptRoot '..\src-tauri\target') -ErrorAction SilentlyContinue }
}
