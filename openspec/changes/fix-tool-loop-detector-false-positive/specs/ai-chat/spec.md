## ADDED Requirements

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
