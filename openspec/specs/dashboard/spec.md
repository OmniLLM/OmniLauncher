# Dashboard

## Purpose

OmniLauncher includes a dashboard surface that aggregates several user-
facing views — past AI conversations, GitHub activity, background jobs,
on-disk tables, and todos — into a single place the user can browse and
search from the launcher without leaving the app.

## Requirements

### Requirement: Conversation history view

The system SHALL provide a dashboard view of past AI conversations and
SHALL let the user open any past conversation from that view.

#### Scenario: User opens past conversation

- **WHEN** the user selects a conversation row in the conversation view
- **THEN** the AI chat opens with that conversation's messages loaded

### Requirement: GitHub view

The system SHALL provide a dashboard view of GitHub activity (e.g. pull
requests, issues) for repositories the user has configured.

#### Scenario: User browses GitHub activity

- **WHEN** the user opens the GitHub dashboard view
- **THEN** activity for the configured repositories is displayed

### Requirement: Jobs view

The system SHALL provide a dashboard view of background jobs (long-
running tasks initiated from the launcher) with their status.

#### Scenario: Running job in list

- **WHEN** a background job is in progress
- **THEN** it appears in the jobs view with a status indicating it is
  running, and SHALL transition to a completed status when it finishes

### Requirement: Tables view

The system SHALL provide a dashboard view of on-disk tables / structured
data that the launcher manages.

#### Scenario: User browses a table

- **WHEN** the user opens the tables view and picks a table
- **THEN** the table's rows are displayed

### Requirement: Todos view

The system SHALL provide a dashboard view for managing todos that the
launcher's `/todo` command produces.

#### Scenario: Todo created via slash command appears in view

- **WHEN** the user creates a todo via `/todo` and then opens the todos
  view
- **THEN** the new todo appears in the list
