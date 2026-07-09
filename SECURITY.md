# Security Policy

## Reporting

Please open a private security advisory via GitHub
(https://github.com/OmniLLM/OmniLauncher/security/advisories/new) for any
vulnerability reports. Avoid filing public issues for security topics.

## Dependency notes

OmniLauncher is backend-only. The former Tauri desktop shell and its Linux GTK
runtime dependency stack have been removed from this repository; Tauri-specific
GTK/glib advisories are no longer reachable through the project dependency graph.
