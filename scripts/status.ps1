param([switch]$Tail)

$appRunning = (Get-Process -Name omnilauncher -ErrorAction SilentlyContinue) -ne $null
$devPort = Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue
$devRunning = $devPort -ne $null
$relExists = Test-Path 'src-tauri/target/release/omnilauncher.exe'
$dbgExists = Test-Path 'src-tauri/target/dev/omnilauncher.exe'

Write-Host ''
Write-Host 'OmniLauncher Status:' -ForegroundColor Cyan
Write-Host '===================='
Write-Host ''

if ($appRunning) { Write-Host '  App Process:    ' -NoNewline; Write-Host 'Running' -ForegroundColor Green }
else { Write-Host '  App Process:    ' -NoNewline; Write-Host 'Not running' -ForegroundColor Yellow }

if ($devRunning) { Write-Host '  Dev Server:     ' -NoNewline; Write-Host 'Running (port 1420)' -ForegroundColor Green }
else { Write-Host '  Dev Server:     ' -NoNewline; Write-Host 'Not running' -ForegroundColor Yellow }

if ($relExists) { Write-Host '  Release Binary: ' -NoNewline; Write-Host 'Exists' -ForegroundColor Green }
else { Write-Host '  Release Binary: ' -NoNewline; Write-Host 'Not built' -ForegroundColor Red }

if ($dbgExists) { Write-Host '  Debug Binary:   ' -NoNewline; Write-Host 'Exists' -ForegroundColor Green }
else { Write-Host '  Debug Binary:   ' -NoNewline; Write-Host 'Not built' -ForegroundColor Red }

Write-Host ''
