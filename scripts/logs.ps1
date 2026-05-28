$logPath = Join-Path $env:USERPROFILE '.omnilauncher\omnilauncher.log'

if (-not (Test-Path $logPath)) {
    Write-Host ''
    Write-Host 'No log file found at:' -ForegroundColor Yellow
    Write-Host "  $logPath" -ForegroundColor Yellow
    Write-Host ''
    Write-Host 'Run with --debug to enable logging:' -ForegroundColor Cyan
    Write-Host '  make prod-debug' -ForegroundColor Cyan
    Write-Host ''
    exit 0
}

Write-Host ''
Write-Host 'Tailing log file:' -ForegroundColor Cyan
Write-Host "  $logPath" -ForegroundColor Cyan
Write-Host '(Press Ctrl+C to stop)' -ForegroundColor DarkGray
Write-Host ''

Get-Content -Path $logPath -Wait -Tail 50
