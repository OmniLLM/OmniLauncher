param([string]$BaseUrl = "http://127.0.0.1:1422")

# Smoke test for the split backend HTTP endpoints. Start the backend first:
#   make backend-dev      (or)
#   $env:OMNILAUNCHER_SPLIT_PORT=1422; cargo run --manifest-path src-tauri/Cargo.toml -- --split-backend
# Then: pwsh -NoProfile -File scripts/smoke-endpoints.ps1

$ErrorActionPreference = "Stop"

function Check($method, $path, $body) {
    $uri = "$BaseUrl$path"
    if ($body) {
        $r = Invoke-WebRequest -UseBasicParsing -Method $method -Uri $uri -ContentType "application/json" -Body $body
    } else {
        $r = Invoke-WebRequest -UseBasicParsing -Method $method -Uri $uri
    }
    if ($r.StatusCode -ne 200) { throw "$method $path -> $($r.StatusCode)" }
    Write-Host "OK  $method $path"
}

Check GET "/health"
Check GET "/api/skills"
Check GET "/api/skills/usage"
Check GET "/api/plugins/collections"
Check GET "/api/plugins/runtime-deps"
Check POST "/api/slash/preview" '{"query":"/calc 2+2"}'

Write-Host "All smoke checks passed."
