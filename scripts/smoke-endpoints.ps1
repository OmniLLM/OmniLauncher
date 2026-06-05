param([string]$BaseUrl = "http://127.0.0.1:1422")

# Smoke test for the split backend HTTP endpoints. Start the backend first:
#   make start-backend      (or)
#   $env:OMNILAUNCHER_SPLIT_PORT=1422; cargo run --manifest-path src-tauri/Cargo.toml -- --split-backend
# Then: pwsh -NoProfile -File scripts/smoke-endpoints.ps1

$ErrorActionPreference = "Stop"
$passed = 0
$failed = 0

function Check($method, $path, $body, $expectedStatus) {
    if (-not $expectedStatus) { $expectedStatus = 200 }
    $uri = "$BaseUrl$path"
    try {
        if ($body) {
            $r = Invoke-WebRequest -UseBasicParsing -Method $method -Uri $uri -ContentType "application/json" -Body $body
        } else {
            $r = Invoke-WebRequest -UseBasicParsing -Method $method -Uri $uri
        }
        if ($r.StatusCode -ne $expectedStatus) {
            Write-Host "FAIL $method $path -> expected $expectedStatus got $($r.StatusCode)" -ForegroundColor Red
            $script:failed++
            return
        }
        Write-Host "OK   $method $path (status=$($r.StatusCode))" -ForegroundColor Green
        $script:passed++
    } catch {
        Write-Host "FAIL $method $path -> $($_.Exception.Message)" -ForegroundColor Red
        $script:failed++
    }
}

function CheckJsonField($method, $path, $body, $field) {
    $uri = "$BaseUrl$path"
    try {
        if ($body) {
            $r = Invoke-WebRequest -UseBasicParsing -Method $method -Uri $uri -ContentType "application/json" -Body $body
        } else {
            $r = Invoke-WebRequest -UseBasicParsing -Method $method -Uri $uri
        }
        $json = $r.Content | ConvertFrom-Json
        if ($null -eq $json.$field) {
            Write-Host "FAIL $method $path -> missing field '$field'" -ForegroundColor Red
            $script:failed++
            return
        }
        Write-Host "OK   $method $path (field '$field' present)" -ForegroundColor Green
        $script:passed++
    } catch {
        Write-Host "FAIL $method $path -> $($_.Exception.Message)" -ForegroundColor Red
        $script:failed++
    }
}

Write-Host ""
Write-Host "=== OmniLauncher Backend Smoke Tests ===" -ForegroundColor Cyan
Write-Host "Backend: $BaseUrl"
Write-Host ""

# ─── Health ──────────────────────────────────────────────────────────────
Write-Host "--- Health ---" -ForegroundColor Yellow
Check GET "/health"
CheckJsonField GET "/health" $null "ok"

# ─── Settings ────────────────────────────────────────────────────────────
Write-Host "--- Settings ---" -ForegroundColor Yellow
Check GET "/api/settings"
CheckJsonField GET "/api/settings" $null "ai_model"
CheckJsonField GET "/api/settings" $null "theme"

# ─── Launcher Config ────────────────────────────────────────────────────
Write-Host "--- Launcher Config ---" -ForegroundColor Yellow
Check GET "/api/launcher-config"

# ─── Skills ──────────────────────────────────────────────────────────────
Write-Host "--- Skills ---" -ForegroundColor Yellow
Check GET "/api/skills"
Check GET "/api/skills/usage"

# ─── Plugins ─────────────────────────────────────────────────────────────
Write-Host "--- Plugins ---" -ForegroundColor Yellow
Check GET "/api/plugins/collections"
Check GET "/api/plugins/runtime-deps"

# ─── Search ──────────────────────────────────────────────────────────────
Write-Host "--- Search ---" -ForegroundColor Yellow
Check POST "/api/search" '{"query":"calc"}'
Check POST "/api/search" '{"query":""}'

# ─── Slash Commands ──────────────────────────────────────────────────────
Write-Host "--- Slash Commands ---" -ForegroundColor Yellow
Check POST "/api/slash/preview" '{"query":"/calc 2+2"}'

# ─── Sessions ────────────────────────────────────────────────────────────
Write-Host "--- Sessions ---" -ForegroundColor Yellow
Check GET "/api/sessions"
Check GET "/api/sessions/current"
Check POST "/api/sessions/clear"

# ─── Favorites ───────────────────────────────────────────────────────────
Write-Host "--- Favorites ---" -ForegroundColor Yellow
Check GET "/api/favorites"

# ─── AI Cancel (no task in flight = returns false, still 200) ────────────
Write-Host "--- AI Cancel ---" -ForegroundColor Yellow
Check POST "/api/ai/cancel"

# ─── CORS Preflight ─────────────────────────────────────────────────────
Write-Host "--- CORS ---" -ForegroundColor Yellow
try {
    $r = Invoke-WebRequest -UseBasicParsing -Method OPTIONS -Uri "$BaseUrl/api/settings"
    if ($r.Headers["Access-Control-Allow-Origin"] -eq "*") {
        Write-Host "OK   OPTIONS /api/settings (CORS headers present)" -ForegroundColor Green
        $passed++
    } else {
        Write-Host "FAIL OPTIONS /api/settings -> missing CORS header" -ForegroundColor Red
        $failed++
    }
} catch {
    Write-Host "FAIL OPTIONS /api/settings -> $($_.Exception.Message)" -ForegroundColor Red
    $failed++
}

# ─── 404 for unknown paths ──────────────────────────────────────────────
Write-Host "--- Error Handling ---" -ForegroundColor Yellow
try {
    $r = Invoke-WebRequest -UseBasicParsing -Method GET -Uri "$BaseUrl/api/nonexistent" -ErrorAction SilentlyContinue
    Write-Host "FAIL GET /api/nonexistent -> expected 404 got $($r.StatusCode)" -ForegroundColor Red
    $failed++
} catch {
    if ($_.Exception.Response.StatusCode.value__ -eq 404) {
        Write-Host "OK   GET /api/nonexistent (404 as expected)" -ForegroundColor Green
        $passed++
    } else {
        Write-Host "FAIL GET /api/nonexistent -> unexpected error: $($_.Exception.Message)" -ForegroundColor Red
        $failed++
    }
}

# ─── Summary ─────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "=== Results ===" -ForegroundColor Cyan
Write-Host "Passed: $passed" -ForegroundColor Green
Write-Host "Failed: $failed" -ForegroundColor $(if ($failed -gt 0) { "Red" } else { "Green" })
Write-Host ""

if ($failed -gt 0) {
    Write-Host "SMOKE TESTS FAILED" -ForegroundColor Red
    exit 1
}
Write-Host "All smoke checks passed." -ForegroundColor Green
