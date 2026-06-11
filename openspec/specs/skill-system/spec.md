# Skill System

## Purpose

Skills are reusable, AI-invocable units of behavior installed under
`~/.omnilauncher/`. The skill system discovers installed skills, lets the
AI invoke them at runtime with managed credentials, lets the user manage
them from the Skill Manager UI, and provides curation/consolidation
helpers to keep the installed set tidy.

## Requirements

### Requirement: Skill discovery

The system SHALL discover skills installed under the runtime data
directory and SHALL expose them as callable units to the AI.

#### Scenario: New skill installed

- **WHEN** the user installs a skill into the runtime data directory
- **THEN** the skill becomes available to the AI on the next request
  without requiring a restart

### Requirement: Skill invocation

The system SHALL let the AI invoke a discovered skill by name, pass it
arguments, and receive its output as a tool result.

#### Scenario: AI invokes a skill

- **WHEN** the AI emits a tool call for a discovered skill with valid
  arguments
- **THEN** the skill executes and the AI receives its output as the
  tool result

### Requirement: Credential isolation

The system SHALL store skill credentials in a dedicated credential store
separate from `settings.json`, and SHALL provide them to a skill only
when the skill is invoked.

#### Scenario: Skill needs a credential

- **WHEN** the user runs a skill that requires a credential and the
  credential exists in the credential store
- **THEN** the skill receives the credential at invocation time and the
  raw value is not written to `settings.json`

### Requirement: Skill Manager UI

The system SHALL provide a Skill Manager UI in the frontend for installing,
listing, and removing skills.

#### Scenario: User removes a skill from the UI

- **WHEN** the user removes a skill via the Skill Manager UI
- **THEN** the skill is no longer discovered on the next AI request

### Requirement: Curation and consolidation

The system SHALL provide helpers that curate or consolidate installed
skills (e.g. remove duplicates, merge related skills) on demand.

#### Scenario: User triggers consolidation

- **WHEN** the user triggers the consolidate action
- **THEN** the installed skill set is rewritten according to the
  consolidation rules and the result is reflected in the Skill Manager UI
