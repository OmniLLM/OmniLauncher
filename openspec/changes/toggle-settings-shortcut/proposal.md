## Why

Pressing `Ctrl+,` opens Settings, but pressing the same shortcut again leaves the Settings window open. Users expect repeated shortcut presses to toggle transient UI surfaces, so the Settings shortcut should also hide the Settings window when it is already visible.

## What Changes

- Update the Settings shortcut behavior so `Ctrl+,` toggles the Settings window visibility from the launcher.
- Keep the existing behavior that `Ctrl+,` opens Settings when it is not already visible.
- Add coverage for the repeated-shortcut hide scenario.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `launcher-modes`: The Settings hotkey requirement changes from open-only behavior to toggle open/hide behavior.
- `settings`: The Settings window opening scenario changes to include hiding the already-visible Settings window with the same shortcut.

## Impact

- Frontend keyboard handling for the launcher and Settings window visibility state.
- Existing Settings window components and tests around `Ctrl+,` behavior.
- No API, dependency, or persistent settings format changes.
