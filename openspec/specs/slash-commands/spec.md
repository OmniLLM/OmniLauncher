# Slash Commands

## Purpose

Slash mode (entered with the `/` prefix) gives the user a structured,
discoverable set of named commands for actions that don't fit naturally
into free-text launcher or AI input — launching apps, running shell
commands, file search, todo management, web search, and managing plugins
or skills.

## Requirements

### Requirement: Built-in command set

The system SHALL provide at least the following built-in slash commands:
`/app`, `/run`, `/open`, `/find`, `/grep`, `/ls`, `/todo`, `/web`,
`/skills`, `/plugins`.

#### Scenario: User invokes a built-in command

- **WHEN** the user types one of the built-in slash commands followed by
  its arguments
- **THEN** the system dispatches to the corresponding command handler and
  shows its results in the result list

### Requirement: Command discovery

The system SHALL show available slash commands as suggestions as the user
types, so commands are discoverable without prior knowledge.

#### Scenario: Empty slash prefix

- **WHEN** the user types `/` with no command name
- **THEN** the result list shows the available slash commands

### Requirement: Unknown command handling

The system SHALL display a clear "unknown command" result when a slash
command name does not match any registered command.

#### Scenario: Typo in command name

- **WHEN** the user types `/runn` (not a registered command)
- **THEN** the result list indicates the command is unknown rather than
  silently doing nothing
