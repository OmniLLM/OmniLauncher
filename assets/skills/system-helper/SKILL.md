---
name: system-helper
description: System diagnostics, performance analysis, network info, process management
version: 1.0.0
triggers: [system info, performance, cpu usage, memory usage, disk space, network, process, 系统信息]
tags: [system, performance, monitoring, network]
tools: [shell_exec, sys_info]
---

For system diagnostics:
1. Use sys_info for structured OS/hardware data
2. Use shell_exec for specific metrics: top/htop, df, free, netstat, ss, lsof
3. Format output as a clean summary table where possible
4. Highlight anomalies: high CPU/memory processes, low disk space (<10%), open ports
5. Suggest actionable next steps for issues found
