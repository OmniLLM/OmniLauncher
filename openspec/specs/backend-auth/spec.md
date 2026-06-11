# Backend Authentication

## Purpose

OmniLauncher's backend is a local HTTP server that the frontend talks to.
Because the backend can run on a different machine than the frontend, every
request is authenticated with a shared token. This capability covers how the
token is sourced, stored, and presented to the user.

## Requirements

### Requirement: Token source precedence

The system SHALL source the backend authentication token from, in order:
the `OMNILAUNCHER_AUTH_TOKEN` environment variable; the token file at
`~/.config/omnilauncher/server-token`; or, if neither is present, a randomly
generated token written to the token file for reuse.

#### Scenario: Environment variable set

- **WHEN** the backend starts with `OMNILAUNCHER_AUTH_TOKEN` exported
- **THEN** the backend uses that value as its auth token and does not write
  to the token file

#### Scenario: Single-machine first run

- **WHEN** the backend starts with no env var and no existing token file
- **THEN** the backend generates a random token, persists it to the token
  file, and uses it for the session

### Requirement: Frontend token prompt

The system SHALL prompt the user for the backend token on first frontend
launch when no saved connection token exists, and SHALL store the
user-provided token at `~/.config/omnilauncher/backend-token` for reuse.

#### Scenario: First launch, no saved token

- **WHEN** the frontend starts and `~/.config/omnilauncher/backend-token`
  does not exist
- **THEN** the user is shown a prompt for the backend token; entering a
  value stores it at the saved-token path

### Requirement: Token isolation from settings

The system SHALL keep the backend token out of `settings.json` and out of
the Settings UI surface.

#### Scenario: Settings page rendered

- **WHEN** the user opens the Settings window
- **THEN** no field exposes the backend token value, and `settings.json`
  on disk contains no backend-token field

### Requirement: Authenticated request rejection

The system SHALL reject backend requests whose token header does not match
the active backend token.

#### Scenario: Mismatched token

- **WHEN** the frontend (or any client) sends a request with a token that
  does not match the backend's active token
- **THEN** the backend returns an authentication failure and does not
  perform the requested action
