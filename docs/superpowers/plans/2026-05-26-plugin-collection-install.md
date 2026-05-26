# Plugin Collection Install Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow OmniLauncher Plugin Manager to install either a single plugin directory/repo or a collection repo whose immediate subdirectories are plugins.

**Architecture:** Keep the UI unchanged and update the Rust backend installer to classify the staged source directory after clone/copy. A root `plugin.json` installs as one plugin; otherwise valid immediate child plugin directories are installed into the target plugin directory and the command returns a human-readable summary.

**Tech Stack:** Rust, Tauri command bridge, React PluginManager UI, Cargo tests.

---

### Task 1: Add collection detection tests

**Files:**
- Modify: `src-tauri/src/plugins/plugin_manager_cmd.rs`

- [ ] Add unit tests that create a temporary source directory with two child plugin folders, each containing `plugin.json` and an entry file.
- [ ] Call the installer with the collection directory and a temporary target directory.
- [ ] Assert both plugin directories are installed and the returned summary names both plugins.
- [ ] Add a single-plugin regression test that verifies root-level `plugin.json` behavior remains unchanged.

### Task 2: Refactor install flow around a staged source directory

**Files:**
- Modify: `src-tauri/src/plugins/plugin_manager_cmd.rs`

- [ ] Keep existing Git clone/local path behavior for preparing `dest`.
- [ ] Replace top-level-only validation with a helper that installs from the prepared `dest`.
- [ ] If `dest/plugin.json` is valid and its entry exists, return the existing single plugin success path.

### Task 3: Install immediate child plugins for collection repos

**Files:**
- Modify: `src-tauri/src/plugins/plugin_manager_cmd.rs`

- [ ] When the root is not a plugin, scan immediate child directories only.
- [ ] For each child with a valid manifest and existing entry, move/copy that child into the target plugin directory under the child directory name.
- [ ] Skip invalid child folders.
- [ ] If no valid child plugins exist, remove the staged source and return `No valid plugin.json found in the plugin directory or its immediate subdirectories.`
- [ ] If one or more plugins install, remove the now-empty/staged collection directory and return `Installed N plugins: a, b`.

### Task 4: Update frontend success text compatibility

**Files:**
- Modify: `src/components/PluginManager.tsx`

- [ ] Keep invoking `install_plugin` as a string-returning command.
- [ ] Change success status to display the backend summary directly instead of wrapping it in `Installed "..."`, so both single and collection messages read naturally.

### Task 5: Verify

**Files:**
- Test: `src-tauri/src/plugins/plugin_manager_cmd.rs`

- [ ] Run `cargo test plugin_manager_cmd` from `src-tauri`.
- [ ] Run the app if practical and manually install a collection repo/local collection directory.
- [ ] Confirm installed plugins appear in the Plugin Manager list.
