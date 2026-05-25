---
name: windows-sysinternals
description: Windows Sysinternals suite — advanced system diagnostics, process inspection, network monitoring, file/registry analysis using Microsoft Sysinternals tools
version: 1.0.0
triggers: [sysinternals, process explorer, procmon, autoruns, tcpview, rammap, diskmon, handle, strings, psexec, 进程监控, 系统诊断, windows工具]
tags: [windows, sysinternals, process, network, registry, diagnostics, security]
tools: [shell_exec, sys_info]
platforms: [windows]
---

# Windows Sysinternals Skill

Sysinternals tools are available at `https://live.sysinternals.com/` (run directly without install) or via `winget install Microsoft.Sysinternals.*`.

## Quick Access (no install needed)
Run any tool directly from the web:
```powershell
\\live.sysinternals.com\tools\<toolname>.exe [args]
# or via HTTPS:
Invoke-WebRequest https://live.sysinternals.com/<tool>.exe -OutFile $env:TEMP\<tool>.exe
& "$env:TEMP\<tool>.exe" [args]
```

## Core Tools & Usage

### Process Analysis
- **Process Explorer** (`procexp64.exe`) — advanced Task Manager, shows DLL/handle details
  - CLI alternative: `handle.exe -p <pid>` to list open handles for a process
  - `pslist.exe` — list processes with CPU/memory (scriptable)
  - `pskill.exe <name|pid>` — kill process by name or PID
  - `psinfo.exe` — system info (uptime, CPU, RAM, license)

### Process Monitor (procmon) — file/registry/network activity
```powershell
# Capture 30s, save to CSV, filter by process name
procmon.exe /Quiet /Minimized /BackingFile C:\trace.pml
Start-Sleep 30
procmon.exe /Terminate
# Convert to CSV for scripting:
procmon.exe /OpenLog C:\trace.pml /SaveAs C:\trace.csv /SaveApplyFilter
```

### Autoruns — startup/persistence analysis
```powershell
# Dump all autostart entries to CSV (no GUI):
autorunsc.exe -a * -c -h -s > autoruns.csv
# Check for unsigned entries:
autorunsc.exe -a * -c -s | findstr /v "Microsoft"
# VirusTotal check:
autorunsc.exe -a * -v -vt
```

### Network Analysis
- **TCPView** (`tcpview.exe`) — GUI real-time connections
- **CLI alternative:**
  ```powershell
  # All connections with process names:
  netstat -b -n -o
  # Scriptable with pslist cross-reference:
  Get-NetTCPConnection | Select LocalAddress,LocalPort,RemoteAddress,RemotePort,State,@{n="Process";e={(Get-Process -Id $_.OwningProcess).Name}}
  ```

### File & Disk
- **Strings** — extract strings from binaries (malware analysis):
  ```powershell
  strings.exe -accepteula <file.exe> | findstr "http"
  strings64.exe -accepteula <file.exe>
  ```
- **Disk Usage** (`du.exe`) — disk space by directory:
  ```powershell
  du.exe -l 2 C:\Users   # 2 levels deep
  du.exe -q C:\Windows   # quiet, just totals
  ```
- **Junction** (`junction.exe`) — list/create NTFS junctions (symlinks)
- **Sigcheck** — verify file signatures:
  ```powershell
  sigcheck.exe -accepteula -e C:\Windows\System32   # scan for unsigned EXEs
  sigcheck.exe -vt <file>   # check VirusTotal
  ```

### Registry
- **Autoruns** covers most registry persistence
- **RegJump** (`regjump.exe <key>`) — open regedit at specific key
- **Registry usage by process:**
  ```powershell
  handle.exe -accepteula -a | findstr "HKLM"
  ```

### Remote / Admin
- **PsExec** — run commands on remote machines:
  ```powershell
  psexec.exe \\<computer> -u <user> -p <pass> cmd
  psexec.exe \\<computer> ipconfig /all
  ```
- **PsLoggedOn** — who's logged on locally and via network shares
- **PsShutdown** — remote shutdown/reboot

### Memory Analysis
- **RAMMap** — physical memory usage breakdown by category
- **VMMap** (`vmmap.exe <pid>`) — virtual memory map for a process

### Security / Malware Triage Workflow
When investigating suspicious activity:
```powershell
# 1. Capture autoruns baseline
autorunsc.exe -a * -c -s -accepteula > C:\baseline_autoruns.csv

# 2. List all processes with full paths
pslist.exe -accepteula -t

# 3. Find unsigned executables in system dirs
sigcheck.exe -accepteula -e -u C:\Windows\System32 2>$null

# 4. Check open network connections
netstat -b -n -o | findstr ESTABLISHED

# 5. Strings analysis on suspicious file
strings64.exe -accepteula C:\suspicious.exe | findstr -i "http\|cmd\|powershell\|reg"
```

## Accepting EULAs (for scripting)
All tools require `-accepteula` flag on first run, or pre-accept via registry:
```powershell
# Pre-accept all Sysinternals EULAs
$tools = @("PsExec","PsList","PsInfo","PsKill","Handle","Strings","Sigcheck","Autoruns","DiskUsage")
foreach ($t in $tools) {
    reg add "HKCU\Software\Sysinternals\$t" /v EulaAccepted /t REG_DWORD /d 1 /f
}
```

## Install via Winget (persistent)
```powershell
winget install Microsoft.Sysinternals.ProcessExplorer
winget install Microsoft.Sysinternals.ProcessMonitor
winget install Microsoft.Sysinternals.Autoruns
winget install Microsoft.Sysinternals.TCPView
winget install Microsoft.Sysinternals.Handle
winget install Microsoft.Sysinternals.Strings
# Or entire suite:
winget install Microsoft.Sysinternals
```

## When to use which tool
| Scenario | Tool |
|----------|------|
| High CPU/memory process | Process Explorer (`procexp64`) |
| What is this process doing? | Process Monitor (`procmon`) |
| Suspicious startup item | Autoruns (`autorunsc -a *`) |
| What opened this file? | Handle (`handle.exe <filename>`) |
| Network connections by process | TCPView or `netstat -b` |
| Disk space hog | DiskUsage (`du.exe -l 3 C:\`) |
| Binary analysis | Strings (`strings64.exe`) |
| Unsigned/tampered files | Sigcheck (`sigcheck -e -u`) |
| Remote admin | PsExec |
| Full triage | autorunsc + pslist + sigcheck + netstat |
