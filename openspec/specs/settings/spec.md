# Settings

## Purpose

OmniLauncher persists user-editable configuration (backend URL, AI
provider details, retry budget, timeouts, UI preferences) in a single
JSON file under the OS config directory and exposes a Settings window
in the frontend for editing it. New fields added in later releases pick
up sensible defaults so existing installs upgrade without manual edits.

## Requirements

### Requirement: Single settings file

The system SHALL persist user-editable settings to
`~/.config/omnilauncher/settings.json` (or the platform-equivalent path
on Windows / macOS) and SHALL read from this file at startup.

#### Scenario: Settings saved from UI

- **WHEN** the user changes a value in the Settings window and clicks
  Save Settings
- **THEN** the new value is written to the settings file and is in effect
  for subsequent requests without requiring a restart

### Requirement: Default fallback for missing fields

The system SHALL fall back to documented defaults for any field absent from
the settings file, so older settings files remain valid after upgrades that
add new fields.

#### Scenario: Existing install upgrades to new version

- **WHEN** the app starts with a `settings.json` that predates a new field
  the current version expects
- **THEN** the missing field takes its documented default and the rest of
  the file is loaded unchanged; no save is required to make it valid

### Requirement: Settings window tabs

The system SHALL group editable settings into at least General and AI tabs
in the Settings window, and SHALL let the same Settings shortcut hide the
Settings window when it is already visible.

#### Scenario: User opens Settings

- **WHEN** the user presses Ctrl+, (or the platform-equivalent shortcut)
  and the Settings window is hidden
- **THEN** the Settings window opens with General and AI tabs visible

#### Scenario: User hides Settings with shortcut

- **WHEN** the Settings window is visible and the user presses Ctrl+, (or
  the platform-equivalent shortcut)
- **THEN** the Settings window closes and the launcher content is visible

### Requirement: Cross-process settings consistency

The system SHALL apply settings changes consistently across the backend
and frontend such that the next request honours the new values.

#### Scenario: AI timeout changed mid-session

- **WHEN** the user lowers the AI request timeout and saves
- **THEN** the next AI request uses the new timeout
