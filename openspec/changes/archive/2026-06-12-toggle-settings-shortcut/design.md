## Context

The launcher already owns a `showSettings` visibility flag in `src/App.tsx` and renders `SettingsWindow` through `src/components/LauncherBody.tsx` when that flag is true. The global keyboard hook in `src/hooks/useGlobalKeyboard.ts` handles `Ctrl+,` / platform-equivalent shortcut input and currently sets the flag to true unconditionally.

This change is a small frontend behavior adjustment: the shortcut should use the existing visibility state as the source of truth and flip it when pressed, instead of only opening Settings.

## Goals / Non-Goals

**Goals:**

- Make repeated `Ctrl+,` presses toggle Settings visibility from the launcher.
- Preserve the existing first-press behavior that opens Settings when it is hidden.
- Preserve existing Settings load/save behavior, tabs, close button, and settings persistence.
- Add frontend test coverage for shortcut open and shortcut hide behavior.

**Non-Goals:**

- Changing the global launcher open/close hotkey.
- Adding a user-configurable shortcut system.
- Changing settings schema, persistence, or backend APIs.
- Changing Settings window layout beyond visibility toggling.

## Decisions

- Use the existing `showSettings` state as the single source of truth.
  - Rationale: `App.tsx` already owns Settings visibility and `LauncherBody` already gates rendering on it.
  - Alternative considered: introduce local state inside `SettingsWindow`; rejected because visibility belongs to the parent that decides whether the Settings component is mounted.

- Change the Settings shortcut handler from open-only to functional state toggling.
  - Rationale: using `setShowSettings(current => !current)` avoids stale-closure behavior and naturally handles repeated shortcut presses.
  - Alternative considered: pass `showSettings` into the keyboard hook and call `setShowSettings(!showSettings)`; rejected because functional updates are safer for event listeners.

- Keep the close button behavior unchanged.
  - Rationale: the close button is already explicit hide behavior and should continue setting Settings visibility to false.
  - Alternative considered: route close through the same toggle helper; rejected because close should be idempotent rather than toggling.

## Risks / Trade-offs

- Keyboard shortcut events might fire while focus is inside Settings form fields → keep `preventDefault()` for the shortcut and test the user-visible behavior.
- If the keyboard hook listener uses stale callback references, shortcut behavior could become inconsistent → use React functional state updates in the setter.
- Existing tests may assume `Ctrl+,` only opens Settings → update or extend tests to assert both open and hide states.
