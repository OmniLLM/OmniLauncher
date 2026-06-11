# AI Chat

## Purpose

AI mode lets the user have a tool-augmented conversation with a configured
LLM provider. The user configures the provider URL, API key, and model in
Settings, and the chat UI streams responses, renders message history, and
lets the user queue follow-up prompts while a response is in flight.

## Requirements

### Requirement: Configurable provider

The system SHALL let the user configure the AI provider URL, API key, and
model name, and SHALL use those values for every AI request.

#### Scenario: User changes provider mid-session

- **WHEN** the user updates the AI provider URL, API key, or model in
  Settings and saves
- **THEN** the next AI request uses the new values

### Requirement: Tool-augmented chat

The system SHALL allow the AI to invoke tools (file read/write, web fetch,
shell, etc.) during a conversation, with a user-configurable per-request
tool iteration cap.

#### Scenario: Tool call within iteration cap

- **WHEN** the model emits a tool call and the current request is below
  the configured tool-iteration cap
- **THEN** the tool runs, the result is fed back to the model, and the
  conversation continues

#### Scenario: Tool iteration cap reached

- **WHEN** the model would emit a tool call but the request has already
  reached the configured tool-iteration cap
- **THEN** the system stops invoking tools for that request and returns
  the current state to the user

### Requirement: Transient-error retry

The system SHALL retry transient AI request failures with exponential
backoff up to a user-configurable maximum number of attempts.

#### Scenario: Transient failure recoverable within budget

- **WHEN** an AI request fails with a transient error and the retry budget
  is not exhausted
- **THEN** the system waits per the backoff schedule and retries; if a
  retry succeeds the user sees the successful response

#### Scenario: Retry budget exhausted

- **WHEN** every attempt within the configured retry budget fails
- **THEN** the user is shown an error explaining the request failed after
  the configured number of attempts

### Requirement: Streamed responses

The system SHALL render AI responses incrementally as chunks arrive,
rather than waiting for the full response before showing anything.

#### Scenario: Long completion

- **WHEN** the AI is producing a multi-second response
- **THEN** the chat bubble updates as new chunks arrive

### Requirement: Session history

The system SHALL persist past chat sessions and SHALL let the user switch
between them from a session picker.

#### Scenario: User picks a past session

- **WHEN** the user opens the session picker and selects a previous session
- **THEN** the messages from that session are loaded and displayed
