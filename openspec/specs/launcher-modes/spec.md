# Launcher Modes

## Purpose

OmniLauncher's input bar is a single text field that routes to one of three
modes based on the leading characters: the default launcher (apps, files,
URLs, actions), AI chat (`?` or `ai ` prefix), and slash commands (`/`
prefix). The user can open, close, and reach Settings from anywhere via
global hotkeys.

## Requirements

### Requirement: Mode dispatch by prefix

The system SHALL select the launcher mode from the leading characters of
the input: bare text → launcher mode; `?` or `ai ` → AI mode; `/` → slash
mode.

#### Scenario: Bare text input

- **WHEN** the user types text that does not start with `?`, `ai `, or `/`
- **THEN** results are produced by launcher mode (apps, files, URLs,
  built-in actions)

#### Scenario: AI prefix

- **WHEN** the user types `?` followed by a question, or `ai ` followed by
  a question
- **THEN** the input is dispatched to AI mode

#### Scenario: Slash prefix

- **WHEN** the user types `/` followed by a command name
- **THEN** the input is dispatched to slash-command mode

### Requirement: Global open/close hotkey

The system SHALL open the launcher window from anywhere on the user's
desktop with Ctrl+Shift+O, and SHALL close it with Esc.

#### Scenario: Hotkey while another app is focused

- **WHEN** the user presses Ctrl+Shift+O while a different application
  has focus
- **THEN** the OmniLauncher window appears and receives keyboard focus

#### Scenario: Esc dismisses

- **WHEN** the launcher window is open and the user presses Esc
- **THEN** the window closes and focus returns to the previously focused
  application

### Requirement: Settings hotkey

The system SHALL toggle the Settings window from the launcher with Ctrl+,.

#### Scenario: Settings shortcut opens Settings

- **WHEN** the launcher window is open, the Settings window is hidden, and the user presses Ctrl+,
- **THEN** the Settings window opens

#### Scenario: Settings shortcut hides Settings

- **WHEN** the launcher window is open, the Settings window is visible, and the user presses Ctrl+,
- **THEN** the Settings window hides and the launcher content is shown

### Requirement: Result list keyboard navigation

The system SHALL let the user navigate results with the arrow keys and
activate the selected result with Enter, with no mouse required.

#### Scenario: Selecting and activating a result

- **WHEN** results are displayed and the user presses Down twice then Enter
- **THEN** the third result is activated
