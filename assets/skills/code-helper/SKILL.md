---
name: code-helper
description: Explain, review, debug, or fix code snippets in any language
version: 1.0.0
triggers: [explain code, review code, debug, fix bug, refactor, what does this code, 解释代码, 代码审查]
tags: [coding, debugging, review, refactor]
tools: [shell_exec, file_read, grep_search]
---

When helping with code:
1. First identify the programming language
2. For explanations: break down the code block by block, explain the purpose of each part
3. For debugging: identify the root cause, explain why it fails, provide the fix
4. For reviews: check for correctness, performance issues, security vulnerabilities, style
5. Always provide a corrected/improved version in a code block
6. For shell/terminal errors: read the full error, identify the command, explain the fix
