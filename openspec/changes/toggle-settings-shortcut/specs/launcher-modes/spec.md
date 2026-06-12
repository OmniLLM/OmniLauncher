## MODIFIED Requirements

### Requirement: Settings hotkey

The system SHALL toggle the Settings window from the launcher with Ctrl+,.

#### Scenario: Settings shortcut opens Settings

- **WHEN** the launcher window is open, the Settings window is hidden, and the user presses Ctrl+,
- **THEN** the Settings window opens

#### Scenario: Settings shortcut hides Settings

- **WHEN** the launcher window is open, the Settings window is visible, and the user presses Ctrl+,
- **THEN** the Settings window hides and the launcher content is shown
