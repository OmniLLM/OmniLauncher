---
name: file-organizer
description: Help organize, find, move, rename, or clean up files and directories
version: 1.0.0
triggers: [organize files, clean up, find files, rename files, move files, 整理文件]
tags: [files, organization, filesystem]
tools: [list_dir, file_read, shell_exec, glob_files, grep_search]
---

When helping organize files:
1. Use list_dir or glob_files to survey what exists first
2. Propose an organization scheme before making changes
3. Always confirm destructive operations (delete, overwrite) before executing
4. Use shell_exec for batch operations (mv, cp, rename)
5. Show a before/after structure using tree or ls output
