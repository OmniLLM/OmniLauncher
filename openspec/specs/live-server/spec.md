# Live Streaming Server

## Purpose

OmniLauncher exposes a streaming endpoint so AI responses can be delivered
to the frontend incrementally rather than as one large blocking reply.
This makes long completions feel responsive and lets the UI render tokens
as they arrive.

## Requirements

### Requirement: Incremental delivery

The system SHALL stream AI response chunks to the frontend as they are
produced by the upstream provider, rather than buffering the full response
before sending.

#### Scenario: Long AI completion in progress

- **WHEN** the frontend requests an AI completion that produces many
  tokens over several seconds
- **THEN** the frontend receives response chunks incrementally and can
  render them as they arrive

### Requirement: Stream authentication

The system SHALL apply the same backend-token authentication to streaming
endpoints as to non-streaming endpoints.

#### Scenario: Stream request with missing token

- **WHEN** a stream is opened without a valid backend token
- **THEN** the stream is refused before any chunks are produced

### Requirement: Stream termination on cancel

The system SHALL stop producing chunks and release upstream resources when
the frontend closes the streaming connection.

#### Scenario: User cancels mid-response

- **WHEN** the frontend closes the streaming connection mid-response
- **THEN** the backend stops producing chunks and the upstream request is
  cancelled rather than left running
