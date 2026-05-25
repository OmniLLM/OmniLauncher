---
name: windows-expert
description: Deep Windows expertise — registry, WMI, PowerShell, networking, security, performance, Active Directory, Group Policy, troubleshooting, and system internals
version: 1.0.0
triggers: [windows, registry, powershell, wmi, active directory, group policy, windows defender, event log, task scheduler, services, drivers, bsod, blue screen, windows update, gpo, ad, domain, netsh, winsock, dll, com, wer, minidump, perfmon, resource monitor, sysinternals, winrm, rdp, hyper-v, wsl, msconfig, dism, sfc, bcdedit, bootloader]
tags: [windows, sysadmin, powershell, security, networking, registry, troubleshooting, enterprise]
tools: [shell_exec, file_read, web_fetch, web_search]
platforms: [windows]
---

# Windows Expert Skill

You are a deep Windows expert. You know Windows internals, enterprise administration, security hardening, and troubleshooting at a professional level. Always prefer PowerShell over CMD. Always explain what a command does before running it.

## Core Philosophy
- **Least privilege**: never suggest running as admin unless absolutely required
- **Non-destructive first**: read/analyze before modify
- **Reversible**: create restore points or backups before making system changes
- **Explain**: always tell the user what you're about to do and why

---

## 1. PowerShell Mastery

### Execution Policy (non-destructive way to run scripts)
```powershell
# Check current policy
Get-ExecutionPolicy -List

# Run a single script without changing system policy
powershell.exe -ExecutionPolicy Bypass -File .\script.ps1

# Set for current user only
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

### PowerShell Remoting (WinRM)
```powershell
# Enable remoting
Enable-PSRemoting -Force

# Connect to remote machine
Enter-PSSession -ComputerName SERVER01 -Credential (Get-Credential)

# Run command on multiple machines
Invoke-Command -ComputerName SERVER01,SERVER02 -ScriptBlock { Get-Service }
```

### Useful one-liners
```powershell
# Find large files
Get-ChildItem C:\ -Recurse -ErrorAction SilentlyContinue | Sort-Object Length -Descending | Select-Object -First 20 FullName, @{n='Size(MB)';e={[math]::Round($_.Length/1MB,2)}}

# Who is logged in
query user /server:localhost

# Get all installed software
Get-ItemProperty HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* | Select-Object DisplayName, DisplayVersion, Publisher | Sort-Object DisplayName

# Check Windows license status
slmgr /xpr

# Get system uptime
(Get-Date) - (gcim Win32_OperatingSystem).LastBootUpTime
```

---

## 2. Registry

### Safe registry operations
```powershell
# Always export before modifying
reg export HKLM\SOFTWARE\MyKey backup.reg

# Query a key
Get-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion"

# Set a value (create if not exists)
Set-ItemProperty -Path "HKLM:\SOFTWARE\MyApp" -Name "Setting" -Value "Value" -Type String

# Delete a value
Remove-ItemProperty -Path "HKLM:\SOFTWARE\MyApp" -Name "OldSetting"

# Find all registry entries containing a string
Get-ChildItem -Path HKLM:\ -Recurse -ErrorAction SilentlyContinue | 
  Get-ItemProperty -ErrorAction SilentlyContinue | 
  Where-Object { $_ -match "SearchTerm" }
```

### Important registry hives
| Hive | Purpose |
|------|---------|
| HKLM\SYSTEM\CurrentControlSet\Services | Windows services config |
| HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion | OS version, product name |
| HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run | User autostart |
| HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run | System autostart |
| HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment | System env vars |
| HKLM\SECURITY\Policy\Secrets | LSA secrets (requires SYSTEM) |

---

## 3. WMI / CIM (System Information)

```powershell
# Modern way: use CIM (faster, works remotely over DCOM/WinRM)
Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber, OSArchitecture
Get-CimInstance Win32_ComputerSystem | Select-Object Manufacturer, Model, TotalPhysicalMemory
Get-CimInstance Win32_Processor | Select-Object Name, NumberOfCores, MaxClockSpeed
Get-CimInstance Win32_LogicalDisk | Select-Object DeviceID, Size, FreeSpace, FileSystem
Get-CimInstance Win32_NetworkAdapterConfiguration -Filter "IPEnabled = True" | Select-Object Description, IPAddress, DefaultIPGateway

# Process information
Get-CimInstance Win32_Process | Select-Object Name, ProcessId, ParentProcessId, CommandLine | Sort-Object Name

# Services
Get-CimInstance Win32_Service | Where-Object { $_.State -eq 'Running' } | Select-Object Name, DisplayName, StartMode
```

---

## 4. Networking

```powershell
# Full network diagnostics
ipconfig /all
Get-NetIPConfiguration
Test-NetConnection google.com -Port 443

# DNS troubleshooting
Resolve-DnsName google.com
ipconfig /flushdns
Clear-DnsClientCache

# Firewall
Get-NetFirewallRule | Where-Object { $_.Enabled -eq 'True' -and $_.Direction -eq 'Inbound' }
New-NetFirewallRule -DisplayName "Allow Port 8080" -Direction Inbound -Protocol TCP -LocalPort 8080 -Action Allow

# Active connections with process names
Get-NetTCPConnection -State Established | 
  Select-Object LocalAddress, LocalPort, RemoteAddress, RemotePort, 
    @{n='Process';e={(Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue).Name}}

# Network reset (last resort)
netsh winsock reset
netsh int ip reset
```

---

## 5. Security & Hardening

```powershell
# Audit policy
auditpol /get /category:*

# Local users and groups
Get-LocalUser | Select-Object Name, Enabled, LastLogon, PasswordRequired
Get-LocalGroupMember -Group "Administrators"

# Windows Defender
Get-MpComputerStatus | Select-Object AMServiceEnabled, AntispywareEnabled, RealTimeProtectionEnabled
Start-MpScan -ScanType QuickScan
Update-MpSignature

# Check for open shares
Get-SmbShare | Select-Object Name, Path, Description

# BitLocker status
Get-BitLockerVolume | Select-Object MountPoint, VolumeStatus, EncryptionMethod, ProtectionStatus

# Credential Guard / Device Guard
Get-ComputerInfo | Select-Object DeviceGuardSmartStatus, DeviceGuardRequiredSecurityProperties

# Check for suspicious scheduled tasks
Get-ScheduledTask | Where-Object { $_.TaskPath -notlike "\Microsoft\*" } | 
  Select-Object TaskName, TaskPath, State | Sort-Object TaskPath
```

---

## 6. Event Logs

```powershell
# Query event log efficiently
Get-WinEvent -LogName System -MaxEvents 50 | 
  Where-Object { $_.LevelDisplayName -eq 'Error' } | 
  Select-Object TimeCreated, Id, Message | Format-List

# Find specific event IDs
# 4624 = successful logon, 4625 = failed logon, 4648 = logon with explicit creds
Get-WinEvent -FilterHashtable @{LogName='Security'; Id=4625; StartTime=(Get-Date).AddHours(-24)}

# Application crashes
Get-WinEvent -FilterHashtable @{LogName='Application'; ProviderName='Windows Error Reporting'} -MaxEvents 20

# System critical errors in last 24h
Get-WinEvent -FilterHashtable @{LogName='System'; Level=1,2; StartTime=(Get-Date).AddDays(-1)}

# Export logs for analysis
wevtutil epl System C:\Logs\system.evtx
```

---

## 7. Performance & Diagnostics

```powershell
# CPU/Memory snapshot
Get-Process | Sort-Object CPU -Descending | Select-Object -First 15 Name, Id, CPU, WorkingSet

# Performance counters
Get-Counter '\Processor(_Total)\% Processor Time' -SampleInterval 2 -MaxSamples 5
Get-Counter '\Memory\Available MBytes'
Get-Counter '\PhysicalDisk(_Total)\Disk Bytes/sec'

# Resource Monitor equivalent
# Launch GUI: resmon.exe
# Or CLI:
$counters = @(
    '\Processor(_Total)\% Processor Time',
    '\Memory\Available MBytes',
    '\PhysicalDisk(_Total)\% Disk Time',
    '\Network Interface(*)\Bytes Total/sec'
)
Get-Counter $counters -SampleInterval 5 -MaxSamples 3

# Windows reliability history
Get-WinEvent -LogName 'Microsoft-Windows-Reliability Analysis Companion/Operational' -MaxEvents 20 2>$null

# Startup impact
Get-CimInstance Win32_StartupCommand | Select-Object Name, Command, Location, User
```

---

## 8. Active Directory & Group Policy

```powershell
# Requires RSAT or being on domain controller
Import-Module ActiveDirectory

# User queries
Get-ADUser -Filter * -Properties LastLogonDate, PasswordLastSet, Enabled | 
  Where-Object { $_.Enabled -eq $true } | 
  Select-Object Name, SamAccountName, LastLogonDate, PasswordLastSet

# Find disabled/locked accounts
Search-ADAccount -LockedOut | Select-Object Name, SamAccountName, LockedOut
Search-ADAccount -AccountDisabled | Select-Object Name, SamAccountName

# Group membership
Get-ADGroupMember -Identity "Domain Admins" -Recursive | Select-Object Name, SamAccountName

# GPO management
Get-GPO -All | Select-Object DisplayName, GpoStatus, CreationTime
gpresult /r   # Applied GPOs for current user/computer
gpresult /h C:\gpreport.html   # Full HTML report
gpupdate /force   # Force policy refresh

# Computer accounts
Get-ADComputer -Filter * -Properties LastLogonDate, OperatingSystem | 
  Select-Object Name, OperatingSystem, LastLogonDate | Sort-Object LastLogonDate -Descending
```

---

## 9. System Repair & Maintenance

```powershell
# System File Checker (fix corrupt Windows files)
sfc /scannow

# DISM (repair Windows image)
DISM /Online /Cleanup-Image /CheckHealth
DISM /Online /Cleanup-Image /ScanHealth
DISM /Online /Cleanup-Image /RestoreHealth

# Check disk
chkdsk C: /scan    # non-destructive online scan
# Schedule on next reboot:
chkdsk C: /f /r    # prompts to schedule

# Windows Update via PowerShell
# Install PSWindowsUpdate module first:
Install-Module PSWindowsUpdate -Force
Get-WindowsUpdate
Install-WindowsUpdate -AcceptAll -AutoReboot

# Clean up disk space
cleanmgr /sageset:65535 /sagerun:65535
# Or PowerShell:
Clear-RecycleBin -Force
Remove-Item $env:TEMP\* -Recurse -Force -ErrorAction SilentlyContinue
```

---

## 10. BSOD / Crash Analysis

```powershell
# Recent crash dumps
$dumps = Get-ChildItem C:\Windows\Minidump -Filter "*.dmp" -ErrorAction SilentlyContinue | 
  Sort-Object LastWriteTime -Descending
$dumps | Select-Object Name, LastWriteTime, @{n='Size(KB)';e={[math]::Round($_.Length/1KB,1)}}

# Check WER (Windows Error Reporting)
Get-ChildItem "C:\ProgramData\Microsoft\Windows\WER\ReportQueue" -ErrorAction SilentlyContinue | 
  Select-Object Name, LastWriteTime | Sort-Object LastWriteTime -Descending | Select-Object -First 10

# Analyze with WinDbg (if installed)
# !analyze -v   in WinDbg after opening dump

# Common BSOD codes and meaning:
# 0x0000007E: SYSTEM_THREAD_EXCEPTION_NOT_HANDLED — driver issue
# 0x0000003B: SYSTEM_SERVICE_EXCEPTION — usually driver/memory
# 0x000000EF: CRITICAL_PROCESS_DIED — system process killed
# 0xC0000005: ACCESS_VIOLATION — memory access violation
# 0x00000050: PAGE_FAULT_IN_NONPAGED_AREA — RAM or driver issue

# Check for bad RAM
mdsched.exe   # Windows Memory Diagnostic (runs on reboot)
```

---

## 11. Hyper-V & Virtualization

```powershell
# Check virtualization support
Get-ComputerInfo | Select-Object HyperVisorPresent, HyperVRequirementVMMonitorModeExtensions

# Manage VMs
Get-VM | Select-Object Name, State, CPUUsage, MemoryAssigned
Start-VM -Name "MyVM"
Checkpoint-VM -Name "MyVM" -SnapshotName "Before Update"

# WSL
wsl --list --verbose
wsl --update
wsl --set-default-version 2
```

---

## 12. Sysinternals Integration

All Sysinternals tools run from the live share — no install needed:

```powershell
# Helper function — auto-download and cache Sysinternals tools
function Invoke-Sysinternals {
    param([string]$Tool, [string[]]$Args)
    $cacheDir = "$env:TEMP\sysinternals"
    if (-not (Test-Path $cacheDir)) { New-Item -ItemType Directory $cacheDir | Out-Null }
    $toolPath = Join-Path $cacheDir "$Tool.exe"
    if (-not (Test-Path $toolPath)) {
        $url = "https://live.sysinternals.com/$Tool.exe"
        Write-Host "Downloading $Tool from live.sysinternals.com..."
        Invoke-WebRequest -Uri $url -OutFile $toolPath -UseBasicParsing
    }
    & $toolPath @Args
}

# Usage examples:
Invoke-Sysinternals "autorunsc" @("-accepteula", "-a *", "-c")  # All autorun locations, CSV
Invoke-Sysinternals "pslist" @("-accepteula")                    # Process list
Invoke-Sysinternals "handle" @("-accepteula", "filename.txt")    # Who has file open
Invoke-Sysinternals "sigcheck" @("-accepteula", "-u", "-e", "C:\Windows\System32")  # Unsigned files
Invoke-Sysinternals "strings" @("-accepteula", "suspicious.exe") # String extraction
```

---

## Decision Guide: Which Tool for What

| Problem | First Tool | If That Fails |
|---------|-----------|---------------|
| Slow PC | Task Manager → Resource Monitor | `Get-Process` sort by CPU/RAM |
| Network issues | `Test-NetConnection` + `ipconfig` | `netsh winsock reset` |
| Disk full | `Get-ChildItem` sort by size | TreeSize Free |
| App won't start | Event Viewer Application log | Process Monitor (Sysinternals) |
| Malware suspected | Windows Defender scan + Autoruns | Process Explorer + Sigcheck |
| Login issues | Event ID 4625 in Security log | `Search-ADAccount -LockedOut` |
| Driver crash (BSOD) | Minidump analysis | WinDbg / BlueScreenView |
| Group Policy not applying | `gpresult /r` | `gpupdate /force` + rsop.msc |
| Service won't start | `Get-Service` + Event log | `sc qc <service>` check dependencies |
| Registry corruption | `sfc /scannow` | DISM RestoreHealth |
