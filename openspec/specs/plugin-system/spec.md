# Plugin System

## Purpose

Plugins are units of functionality that supply launcher results, slash-
command handlers, and tools the AI can call. OmniLauncher ships with a
broad built-in plugin catalog (file I/O, web fetch, shell, calculator,
clipboard, screenshot, etc.) and supports external plugins installed by
the user. The plugin system handles discovery, lifecycle (install /
update / remove), and exposes a Plugin Manager UI.

## Requirements

### Requirement: Built-in plugin catalog

The system SHALL ship a built-in plugin catalog covering at minimum:
app launching, shell execution, file read / write / search, glob, grep,
ls, HTTP / web fetch, web search, calculator, clipboard, screenshot,
system info, scheduler, todo, snippets, env vars, hosts, network, color
picker, emoji picker, unit converter, translate, URL opener, window
resize, and code tools.

#### Scenario: Built-in plugin available on first run

- **WHEN** the user starts a fresh install of OmniLauncher
- **THEN** the built-in plugins are available without any explicit
  install step

### Requirement: External plugin installation

The system SHALL let the user install, update, and remove external
plugins from the Plugin Manager UI.

#### Scenario: Install external plugin

- **WHEN** the user installs an external plugin from the Plugin Manager
  UI and the install completes successfully
- **THEN** the plugin's commands and tools become available to the
  launcher and AI without restart

#### Scenario: Remove external plugin

- **WHEN** the user removes an external plugin from the Plugin Manager UI
- **THEN** the plugin's commands and tools stop appearing in results
  and stop being available to the AI

### Requirement: Plugin uniformity

The system SHALL expose built-in and external plugins to the rest of the
launcher (results, slash dispatch, AI tools) through the same interface,
so callers do not need to distinguish them.

#### Scenario: AI calls a plugin tool

- **WHEN** the AI invokes a plugin-provided tool by name
- **THEN** the tool runs identically whether the plugin is built-in or
  external

### Requirement: Plugin failure isolation

The system SHALL handle a plugin's failure (panic, exception, non-zero
exit, timeout) without crashing the launcher or backend.

#### Scenario: Plugin throws

- **WHEN** a plugin invocation fails with an error
- **THEN** the launcher (or AI tool result) shows the error, and the
  backend remains running and responsive to subsequent requests

### Requirement: Plugin Manager UI

The system SHALL provide a Plugin Manager UI listing installed plugins
and their state.

#### Scenario: User opens Plugin Manager

- **WHEN** the user opens the Plugin Manager UI
- **THEN** all built-in and installed external plugins are listed with
  their current state
