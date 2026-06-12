## MODIFIED Requirements

### Requirement: Settings window tabs

The system SHALL group editable settings into at least General and AI tabs in the Settings window, and SHALL let the same Settings shortcut hide the Settings window when it is already visible.

#### Scenario: User opens Settings

- **WHEN** the user presses Ctrl+, (or the platform-equivalent shortcut) and the Settings window is hidden
- **THEN** the Settings window opens with General and AI tabs visible

#### Scenario: User hides Settings with shortcut

- **WHEN** the Settings window is visible and the user presses Ctrl+, (or the platform-equivalent shortcut)
- **THEN** the Settings window closes and the launcher content is visible
