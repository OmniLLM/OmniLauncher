# Agent Context Loading

## Purpose

OmniLauncher loads `AGENTS.md` files from layered locations and concatenates
them into a single AI system prompt, so users can keep global, per-user, and
per-project context separate without manually merging them.

## Requirements

### Requirement: Layered discovery

The system SHALL load `AGENTS.md` files in load order — most general first,
most specific last — so later files extend or override earlier ones.

#### Scenario: All three layers present

- **WHEN** `~/.config/omnilauncher/AGENTS.md`, an `AGENT.md` walking up from
  the current working directory, and `~/AGENT.md` all exist
- **THEN** the AI system prompt contains all three concatenated in that
  order (global → cwd-walk → home)

#### Scenario: Some layers missing

- **WHEN** only the global `~/.config/omnilauncher/AGENTS.md` exists and the
  cwd-walk and home-level files do not
- **THEN** the missing files are silently skipped and only the present file
  contributes to the prompt

### Requirement: Legacy AGENT.md fallback

The system SHALL fall back to `~/.config/omnilauncher/AGENT.md` (singular,
legacy) at the global path when `AGENTS.md` (plural, current) is absent.

#### Scenario: Only legacy file exists

- **WHEN** `~/.config/omnilauncher/AGENT.md` exists but `AGENTS.md` does not
- **THEN** the legacy file is loaded as the global layer
