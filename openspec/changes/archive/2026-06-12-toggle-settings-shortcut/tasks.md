## 1. Shortcut Behavior

- [x] 1.1 Inspect `src/hooks/useGlobalKeyboard.ts` and confirm the existing `Ctrl+,` / platform-equivalent Settings shortcut path.
- [x] 1.2 Change the Settings shortcut handler to toggle the existing `showSettings` state with a functional update.
- [x] 1.3 Preserve existing `preventDefault()` behavior and all unrelated keyboard shortcuts.

## 2. UI Integration

- [x] 2.1 Verify `src/App.tsx` continues to own Settings visibility state and passes the setter to the keyboard hook.
- [x] 2.2 Verify `src/components/LauncherBody.tsx` continues to render launcher content when Settings is hidden and `SettingsWindow` when visible.
- [x] 2.3 Keep Settings close button behavior idempotent by setting visibility to false.

## 3. Tests

- [x] 3.1 Add or update frontend tests covering `Ctrl+,` opening Settings when hidden.
- [x] 3.2 Add frontend coverage that pressing `Ctrl+,` again hides Settings and returns to launcher content.
- [x] 3.3 Run the relevant Vitest suite and fix any regressions.
