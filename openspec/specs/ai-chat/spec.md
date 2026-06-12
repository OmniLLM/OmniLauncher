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

### Requirement: Loop-detector result sensitivity

The system SHALL halt the agentic tool loop with a "stuck in a loop" final
response only when the last three iterations have BOTH identical tool-call
fingerprints AND identical tool-result fingerprints. Three identical calls
whose results differ SHALL NOT be treated as a loop, so legitimate retries
of transient errors and iterated discovery patterns are allowed to proceed.

#### Scenario: Three identical calls with differing results

- **WHEN** the agentic loop runs three consecutive iterations with
  identical tool-call arguments but each iteration's tool result differs
  (for example, three calls retrying after distinct error messages)
- **THEN** the loop continues to the next iteration rather than halting,
  and no "stuck in a loop" message is shown to the user

#### Scenario: Three identical request-and-result pairs

- **WHEN** the agentic loop runs three consecutive iterations where both
  the tool-call arguments and the tool-result content are identical
- **THEN** the loop halts and the user is shown the loop-trip surface
  described by the "Loop-detector failure surface" requirement

### Requirement: Loop-detector failure surface

When the loop detector halts the agentic loop, the system SHALL surface
to the user the repeated tool name, a credential-masked snippet of its
arguments, a credential-masked snippet of its last result, and a one-line
hint about what to do next.

#### Scenario: Detector trips on a tool that received a credential

- **WHEN** the loop detector halts and the repeated tool's arguments
  contain a credential-shaped value
- **THEN** the surfaced message shows the credential value replaced by
  the same redaction marker used by the logging-and-masking capability,
  not the raw secret

#### Scenario: Detector trips on a tool with a very large result

- **WHEN** the loop detector halts and the last tool result exceeds the
  surfacing budget
- **THEN** the message includes a truncated snippet of the result with
  an explicit indication that the rest was elided, rather than the full
  result or no result at all

### Requirement: User override for loop detector

The system SHALL provide a user-editable setting to disable the loop
detector for advanced debugging, with a default of enabled. When the
setting is disabled, the iteration cap (`max_tool_iterations`) remains
the upper bound on agentic-loop execution.

#### Scenario: User disables the detector and re-runs the failing query

- **WHEN** the user disables the loop detector in Settings and re-runs a
  query that previously tripped the detector
- **THEN** the agentic loop runs up to `max_tool_iterations` rounds
  rather than halting at the third repeated iteration

#### Scenario: Setting absent from settings file

- **WHEN** the app starts with a `settings.json` that predates this
  setting (i.e. the field is missing)
- **THEN** the detector is enabled by default and the settings file is
  loaded without error
