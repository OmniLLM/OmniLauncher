param(
    [string]$BaseUrl = "http://127.0.0.1:1422",
    [int]$AiTimeoutSeconds = 30,
    [string]$Token = $env:OMNILAUNCHER_SERVER_TOKEN,
    [string]$TokenFile = (Join-Path $HOME ".config/omnilauncher/server-token")
)

<#
.SYNOPSIS
    End-to-end test for backend API flows.
.DESCRIPTION
    Tests backend API flows:
    1. Health check
    2. Load settings
    3. Load launcher config
    4. Load sessions / switch sessions
    5. Search
    6. AI query with SSE event listening (the critical path)
    7. Cancel AI
    8. Favorites CRUD
    9. Skills listing
    10. Plugins listing
    11. Slash command preview

    Auth token resolution order: -Token > $env:OMNILAUNCHER_SERVER_TOKEN > -TokenFile
    (default token file: ~/.config/omnilauncher/server-token). When a token is
    resolved it is sent as the X-OmniLauncher-Token header on every request,
    including the SSE listener connections.
#>

$ErrorActionPreference = "Stop"
$passed = 0
$failed = 0
$warnings = 0

# Resolve token: param/env (already in $Token) > file.
if (-not $Token -and (Test-Path $TokenFile)) {
    $Token = (Get-Content -Raw $TokenFile).Trim()
}

# Header table sent with every request (empty when no token resolved).
$TokenHeaders = @{}
if ($Token) { $TokenHeaders["X-OmniLauncher-Token"] = $Token }

function Pass($test) {
    Write-Host "  PASS  $test" -ForegroundColor Green
    $script:passed++
}

function Fail($test, $reason) {
    Write-Host "  FAIL  $test -> $reason" -ForegroundColor Red
    $script:failed++
}

function Warn($test, $reason) {
    Write-Host "  WARN  $test -> $reason" -ForegroundColor Yellow
    $script:warnings++
}

function Api($method, $path, $body) {
    $uri = "$BaseUrl$path"
    $params = @{
        Uri             = $uri
        Method          = $method
        UseBasicParsing = $true
        TimeoutSec      = 10
        Headers         = $TokenHeaders
    }
    if ($body) {
        $params.ContentType = "application/json; charset=utf-8"
        $params.Body        = [System.Text.Encoding]::UTF8.GetBytes($body)
    }
    return Invoke-WebRequest @params
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  OmniLauncher E2E API Test" -ForegroundColor Cyan
Write-Host "  Backend: $BaseUrl" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# ─────────────────────────────────────────────────────────────────────────────
# 1. Health Check
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 1. Health Check ---" -ForegroundColor Yellow
try {
    $r = Api GET "/health"
    $health = $r.Content | ConvertFrom-Json
    if ($health.ok -eq $true) { Pass "GET /health returns ok=true" }
    else { Fail "GET /health" "unexpected: $($r.Content)" }
} catch { Fail "GET /health" $_.Exception.Message }

# ─────────────────────────────────────────────────────────────────────────────
# 2. Settings
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 2. Settings ---" -ForegroundColor Yellow
$settings = $null
try {
    $r = Api GET "/api/settings"
    $settings = $r.Content | ConvertFrom-Json
    if ($settings.ai_model -and $settings.theme) { Pass "GET /api/settings has ai_model and theme" }
    else { Fail "GET /api/settings" "missing fields" }

    # Check AI config
    if (-not $settings.ai_base_url) {
        Warn "AI config" "ai_base_url is empty — AI queries will fail"
    } elseif (-not $settings.ai_api_key) {
        Warn "AI config" "ai_api_key is empty — AI queries may fail"
    } else {
        Pass "AI config has base_url=$($settings.ai_base_url) model=$($settings.ai_model)"
    }
} catch { Fail "GET /api/settings" $_.Exception.Message }

# ─────────────────────────────────────────────────────────────────────────────
# 3. Test AI API reachability (the actual upstream LLM endpoint)
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 3. AI Upstream Reachability ---" -ForegroundColor Yellow
if ($settings -and $settings.ai_base_url) {
    $aiUrl = $settings.ai_base_url.TrimEnd('/')
    try {
        # Try /v1/models as a lightweight probe
        $probe = Invoke-WebRequest -Uri "$aiUrl/v1/models" -UseBasicParsing -TimeoutSec 5 -ErrorAction Stop
        if ($probe.StatusCode -eq 200) {
            Pass "AI upstream $aiUrl/v1/models is reachable (status=200)"
        } else {
            Warn "AI upstream" "$aiUrl returned status $($probe.StatusCode)"
        }
    } catch {
        $errMsg = $_.Exception.Message
        if ($errMsg -match "timed? ?out|Timeout|canceled") {
            Fail "AI upstream $aiUrl/v1/models" "TIMEOUT — the LLM server is not responding. AI queries will hang!"
        } elseif ($errMsg -match "401|403|Unauthorized") {
            Warn "AI upstream" "$aiUrl returned auth error — check API key"
        } else {
            Fail "AI upstream $aiUrl" $errMsg
        }
    }
} else {
    Warn "AI upstream" "No ai_base_url configured, skipping"
}

# ─────────────────────────────────────────────────────────────────────────────
# 4. Launcher Config
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 4. Launcher Config ---" -ForegroundColor Yellow
try {
    $r = Api GET "/api/launcher-config"
    if ($r.StatusCode -eq 200) { Pass "GET /api/launcher-config" }
    else { Fail "GET /api/launcher-config" "status=$($r.StatusCode)" }
} catch { Fail "GET /api/launcher-config" $_.Exception.Message }

# ─────────────────────────────────────────────────────────────────────────────
# 5. Sessions
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 5. Sessions ---" -ForegroundColor Yellow
try {
    $r = Api GET "/api/sessions"
    $sessions = $r.Content | ConvertFrom-Json
    Pass "GET /api/sessions returned $($sessions.Count) session(s)"
} catch { Fail "GET /api/sessions" $_.Exception.Message }

try {
    $r = Api GET "/api/sessions/current"
    Pass "GET /api/sessions/current = $($r.Content)"
} catch { Fail "GET /api/sessions/current" $_.Exception.Message }

try {
    $r = Api POST "/api/sessions/clear"
    Pass "POST /api/sessions/clear (new session)"
} catch { Fail "POST /api/sessions/clear" $_.Exception.Message }

# ─────────────────────────────────────────────────────────────────────────────
# 6. Search
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 6. Search ---" -ForegroundColor Yellow
try {
    $r = Api POST "/api/search" '{"query":"calc"}'
    $results = $r.Content | ConvertFrom-Json
    if ($results.Count -gt 0) { Pass "POST /api/search 'calc' returned $($results.Count) result(s)" }
    else { Warn "Search" "no results for 'calc'" }
} catch { Fail "POST /api/search" $_.Exception.Message }

try {
    $r = Api POST "/api/search" '{"query":""}'
    Pass "POST /api/search empty query (no crash)"
} catch { Fail "POST /api/search empty" $_.Exception.Message }

# ─────────────────────────────────────────────────────────────────────────────
# 7. AI Query + SSE (critical path)
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 7. AI Query + SSE Event Flow ---" -ForegroundColor Yellow

# Cancel any stuck query first
try { Api POST "/api/ai/cancel" | Out-Null } catch {}

# Start SSE listeners BEFORE sending the query (like the frontend does)
$doneJob = Start-Job -ScriptBlock {
    param($url, $token)
    $client = [System.Net.WebClient]::new()
    if ($token) { $client.Headers.Add("X-OmniLauncher-Token", $token) }
    $stream = $client.OpenRead($url)
    $reader = [System.IO.StreamReader]::new($stream)
    $lines = @()
    $start = [datetime]::Now
    while (([datetime]::Now - $start).TotalSeconds -lt $using:AiTimeoutSeconds) {
        if ($reader.Peek() -ge 0) {
            $line = $reader.ReadLine()
            $lines += $line
            if ($line -match '^data: ') { break }
        }
        Start-Sleep -Milliseconds 50
    }
    $reader.Close(); $stream.Close()
    return ($lines -join "`n")
} -ArgumentList "$BaseUrl/api/events/omnilauncher%3A%2F%2Fai-done", $Token

$errorJob = Start-Job -ScriptBlock {
    param($url, $token)
    $client = [System.Net.WebClient]::new()
    if ($token) { $client.Headers.Add("X-OmniLauncher-Token", $token) }
    $stream = $client.OpenRead($url)
    $reader = [System.IO.StreamReader]::new($stream)
    $lines = @()
    $start = [datetime]::Now
    while (([datetime]::Now - $start).TotalSeconds -lt $using:AiTimeoutSeconds) {
        if ($reader.Peek() -ge 0) {
            $line = $reader.ReadLine()
            $lines += $line
            if ($line -match '^data: ') { break }
        }
        Start-Sleep -Milliseconds 50
    }
    $reader.Close(); $stream.Close()
    return ($lines -join "`n")
} -ArgumentList "$BaseUrl/api/events/omnilauncher%3A%2F%2Fai-error", $Token

Start-Sleep -Milliseconds 500  # let SSE connections establish

# Send the AI query
try {
    $r = Api POST "/api/ai/query" '{"query":"say hello in one word"}'
    if ($r.Content -eq "true") { Pass "POST /api/ai/query accepted (returned true)" }
    else { Fail "POST /api/ai/query" "unexpected: $($r.Content)" }
} catch {
    Fail "POST /api/ai/query" $_.Exception.Message
}

# Wait for either ai-done or ai-error
Write-Host "  ...   waiting up to ${AiTimeoutSeconds}s for SSE response..." -ForegroundColor Gray
$doneResult = $doneJob | Wait-Job -Timeout $AiTimeoutSeconds | Receive-Job
$errorResult = $errorJob | Wait-Job -Timeout 2 | Receive-Job

if ($doneResult -and $doneResult -match "data: ") {
    $payload = ($doneResult -split "`n" | Where-Object { $_ -match "^data: " } | Select-Object -First 1) -replace "^data: ", ""
    try {
        $parsed = $payload | ConvertFrom-Json
        if ($parsed.content) {
            Pass "SSE ai-done received: content='$($parsed.content.Substring(0, [Math]::Min(80, $parsed.content.Length)))...'"
        } else {
            Warn "SSE ai-done" "received but content is empty"
        }
    } catch {
        Warn "SSE ai-done" "received but JSON parse failed: $payload"
    }
} elseif ($errorResult -and $errorResult -match "data: ") {
    $errPayload = ($errorResult -split "`n" | Where-Object { $_ -match "^data: " } | Select-Object -First 1) -replace "^data: ", ""
    Fail "SSE ai-error received" $errPayload
} else {
    Fail "SSE timeout" "No ai-done or ai-error event within ${AiTimeoutSeconds}s. The AI query is stuck — check upstream LLM connectivity."
}

# Cleanup jobs
$doneJob, $errorJob | Stop-Job -ErrorAction SilentlyContinue
$doneJob, $errorJob | Remove-Job -Force -ErrorAction SilentlyContinue

# ─────────────────────────────────────────────────────────────────────────────
# 8. AI Cancel
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 8. AI Cancel ---" -ForegroundColor Yellow
try {
    $r = Api POST "/api/ai/cancel"
    Pass "POST /api/ai/cancel returned $($r.Content)"
} catch { Fail "POST /api/ai/cancel" $_.Exception.Message }

# ─────────────────────────────────────────────────────────────────────────────
# 9. Favorites CRUD
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 9. Favorites ---" -ForegroundColor Yellow
try {
    $r = Api GET "/api/favorites"
    Pass "GET /api/favorites"
} catch { Fail "GET /api/favorites" $_.Exception.Message }

# ─────────────────────────────────────────────────────────────────────────────
# 10. Skills
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 10. Skills ---" -ForegroundColor Yellow
try {
    $r = Api GET "/api/skills"
    $skills = $r.Content | ConvertFrom-Json
    Pass "GET /api/skills returned $($skills.Count) skill(s)"
} catch { Fail "GET /api/skills" $_.Exception.Message }

try {
    $r = Api GET "/api/skills/usage"
    Pass "GET /api/skills/usage"
} catch { Fail "GET /api/skills/usage" $_.Exception.Message }

# ─────────────────────────────────────────────────────────────────────────────
# 11. Plugins
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 11. Plugins ---" -ForegroundColor Yellow
try {
    $r = Api GET "/api/plugins/collections"
    Pass "GET /api/plugins/collections"
} catch { Fail "GET /api/plugins/collections" $_.Exception.Message }

try {
    $r = Api GET "/api/plugins/runtime-deps"
    Pass "GET /api/plugins/runtime-deps"
} catch { Fail "GET /api/plugins/runtime-deps" $_.Exception.Message }

# ─────────────────────────────────────────────────────────────────────────────
# 12. Slash Commands
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 12. Slash Commands ---" -ForegroundColor Yellow
try {
    $r = Api POST "/api/slash/preview" '{"query":"/calc 2+2"}'
    $preview = $r.Content | ConvertFrom-Json
    if ($preview.Count -gt 0) {
        $first = $preview[0]
        Pass "POST /api/slash/preview '/calc 2+2' -> $($first.title)"
    } else {
        Pass "POST /api/slash/preview returned empty (no matching command)"
    }
} catch { Fail "POST /api/slash/preview" $_.Exception.Message }

# ─────────────────────────────────────────────────────────────────────────────
# 13. CORS Preflight
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 13. CORS ---" -ForegroundColor Yellow
try {
    $r = Invoke-WebRequest -Uri "$BaseUrl/api/settings" -Method OPTIONS -UseBasicParsing -TimeoutSec 5 -Headers $TokenHeaders
    if ($r.Headers["Access-Control-Allow-Origin"] -eq "*") { Pass "OPTIONS CORS headers present" }
    else { Fail "CORS" "missing Access-Control-Allow-Origin header" }
} catch { Fail "OPTIONS CORS" $_.Exception.Message }

# ─────────────────────────────────────────────────────────────────────────────
# 14. Error handling - 404
# ─────────────────────────────────────────────────────────────────────────────
Write-Host "--- 14. Error Handling ---" -ForegroundColor Yellow
try {
    $r = Invoke-WebRequest -Uri "$BaseUrl/api/nonexistent" -UseBasicParsing -TimeoutSec 5 -Headers $TokenHeaders -ErrorAction SilentlyContinue
    Fail "GET /api/nonexistent" "expected 404 got $($r.StatusCode)"
} catch {
    if ($_.Exception.Response.StatusCode.value__ -eq 404) { Pass "GET /api/nonexistent returns 404" }
    else { Fail "GET /api/nonexistent" $_.Exception.Message }
}

# ═════════════════════════════════════════════════════════════════════════════
# Summary
# ═════════════════════════════════════════════════════════════════════════════
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Results" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Passed:   $passed" -ForegroundColor Green
Write-Host "  Failed:   $failed" -ForegroundColor $(if ($failed -gt 0) { "Red" } else { "Green" })
Write-Host "  Warnings: $warnings" -ForegroundColor $(if ($warnings -gt 0) { "Yellow" } else { "Green" })
Write-Host ""

if ($failed -gt 0) {
    Write-Host "  E2E TESTS FAILED" -ForegroundColor Red
    Write-Host ""
    exit 1
}
if ($warnings -gt 0) {
    Write-Host "  E2E TESTS PASSED WITH WARNINGS" -ForegroundColor Yellow
} else {
    Write-Host "  ALL E2E TESTS PASSED" -ForegroundColor Green
}
Write-Host ""
