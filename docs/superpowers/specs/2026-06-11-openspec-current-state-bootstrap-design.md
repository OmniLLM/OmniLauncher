# OpenSpec Current-State Bootstrap

Date: 2026-06-11
Status: Approved

## Problem

OpenSpec is initialized in the repo (`openspec/config.yaml`, empty `specs/`,
empty `changes/archive/`), but there are no specs to delta against. Future
changes have nowhere to land. The codebase already exists, so we need a
one-time "current state" seed: a small set of capability specs that describe
what OmniLauncher does today, after which all further edits go through the
normal OpenSpec change flow.

## Goals

- Seed `openspec/specs/` with capability specs for OmniLauncher's existing
  behavior, so future changes have a baseline to modify.
- Slice the codebase by user-facing capability (not by code module), so spec
  names match how the README and users talk about the product.
- Keep specs lightweight on the first pass — 2–5 requirements per capability,
  1–2 scenarios per requirement — so the bootstrap finishes in one session
  rather than dragging across many.
- Fill in `openspec/config.yaml`'s `context:` block with OmniLauncher's tech
  stack and conventions so future AI-generated changes are grounded.

## Non-goals

- Documenting every individual plugin. The ~50 plugins under
  `src-tauri/src/plugins/` are listed as part of `plugin-system`, not split
  into 50 separate specs.
- Reverse-engineering undocumented behavior. If a behavior isn't covered by
  the README, an existing design doc, or an obvious read of the source, the
  spec leaves it out. A future change can ADD the requirement when that
  behavior is actually touched.
- Writing `proposal.md` / `tasks.md` for the bootstrap. These specs are
  seeded directly into `openspec/specs/` and skip the change/proposal flow,
  because the change is "document what already exists" — there's no real
  proposal to write.
- Touching source code. The bootstrap is documentation only.
- Per-plugin behavior. Each built-in plugin's surface is treated as
  implementation detail of `plugin-system` until a future change touches it.

## Design

### Capability inventory

11 capabilities, each becoming `openspec/specs/<id>/spec.md`:

| # | Capability id | What it covers |
|---|---|---|
| 1 | `launcher-modes` | Mode switching by prefix (bare text / `?` or `ai ` / `/`), the search bar, the result list, the global hotkeys (Ctrl+Shift+O / Esc / Ctrl+,) |
| 2 | `ai-chat` | AI mode: provider URL + API key + model configuration, retry budget, tool iterations, request timeout, chat bubble UI, session history |
| 3 | `agent-context` | `AGENTS.md` discovery & layering (global → cwd-walk → home), legacy `AGENT.md` fallbacks |
| 4 | `slash-commands` | The `/` mode and the built-in slash command set listed in the README (`/app`, `/run`, `/open`, `/find`, `/grep`, `/ls`, `/todo`, `/web`, `/skills`, `/plugins`) |
| 5 | `plugin-system` | Plugin discovery, lifecycle (install / update / remove), the plugin manager UI, the built-in plugin catalog |
| 6 | `skill-system` | Skill discovery, consolidate / curate, skill credentials, skill runner, skill manager UI |
| 7 | `settings` | `~/.config/omnilauncher/settings.json`, the Settings window (General / AI tabs), token prompts, serde defaults for missing fields |
| 8 | `backend-auth` | `OMNILAUNCHER_AUTH_TOKEN`, token file fallback, frontend saved token, single-machine vs split-machine modes |
| 9 | `live-server` | The streaming / live server endpoint(s) used during AI requests |
| 10 | `dashboard` | Conversation / GitHub / jobs / tables / todos dashboards |
| 11 | `logging-and-masking` | Credential masking in debug logs, log redaction policy |

### Spec template

Every spec follows the OpenSpec `spec-driven` schema exactly (so
`openspec validate` passes):

```markdown
# <Capability Name>

## Purpose

<1–3 sentence purpose statement — what this capability is for>

## Requirements

### Requirement: <name>
The system SHALL <observable behavior, present tense>.

#### Scenario: <name>
- **WHEN** <user action / trigger>
- **THEN** <observable outcome>
```

(The `## Purpose` heading is required by OpenSpec's `spec-driven` schema
validator — confirmed by validating the smallest spec first.)

### Sizing budget

- Purpose statement: 1–3 sentences.
- 2–5 Requirements per capability — only the load-bearing behaviors a future
  change might want to delta against.
- 1–2 Scenarios per requirement — the canonical happy path plus one edge case
  where it matters.
- Target: each spec file ~40–120 lines.

### What the specs deliberately exclude

- File paths and line numbers. Those belong in change proposals when
  something is actually being modified, not in long-lived specs.
- Implementation details (Rust types, crate names, struct fields). Specs are
  behavioral; the implementation can change without changing the spec.
- Settings field names from `AppSettings`. Those belong to a future
  `settings`-touching change, not the seed.
- Per-plugin behavior. `plugin-system` describes the plugin system, not each
  plugin's surface.

### `openspec/config.yaml` update

Fill in the currently-empty `context:` block with:

```yaml
context: |
  Tech stack: Tauri 2 + Rust (backend, `src-tauri/`) and React + TypeScript +
  Vite (frontend, `src/`). Tests: cargo test for Rust, vitest for frontend.
  Conventional commit messages (`feat:`, `fix:`, `docs(spec):`, etc.).
  Domain: keyboard-first launcher with three modes (launcher / AI / slash),
  plugin and skill systems, optional split-machine backend with token auth.
  Config lives under `~/.config/omnilauncher/`; runtime data under
  `~/.omnilauncher/`.
```

### Writing order

Small-to-large, so template issues surface on cheap specs first:

1. `agent-context`
2. `logging-and-masking`
3. `backend-auth`
4. `live-server`
5. `settings`
6. `launcher-modes`
7. `ai-chat`
8. `slash-commands`
9. `dashboard`
10. `skill-system`
11. `plugin-system`

### Validation gate

Between writing and committing:

- `openspec list --specs` shows all 11 capabilities.
- `openspec validate` passes for every spec (no structure errors).
- `openspec spec show <id>` renders cleanly for at least two specs as a
  smoke test.

### Commit

One commit, message:

    docs(spec): seed current-state capability specs

Author per repo convention: James Zhu <zhujian0805@gmail.com>. No
`Co-Authored-By` trailer. Files in the commit: `openspec/config.yaml` plus
the 11 new `openspec/specs/<id>/spec.md` files plus this design doc.

## Rollout

Single PR / single commit. No code changes, no migrations, no test impact.
After merge, every subsequent change to OmniLauncher's behavior goes through
`openspec new change <name>` → proposal + tasks + delta spec →
`openspec archive`.
