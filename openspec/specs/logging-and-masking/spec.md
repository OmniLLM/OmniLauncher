# Logging and Credential Masking

## Purpose

OmniLauncher writes diagnostic logs that may include process arguments,
environment values, and request bodies. The logging layer redacts known
credential shapes (API keys, tokens, password-like values) before anything
is written to disk so debug logs are safe to share without leaking secrets.

## Requirements

### Requirement: Credential redaction in argv

The system SHALL mask credential-shaped substrings inside logged process
arguments before they reach the log sink.

#### Scenario: API key in argv

- **WHEN** a child process is logged with an argument that contains a value
  matching a known credential shape (e.g. `sk-...`, long base64-ish token)
- **THEN** the logged argument shows the value replaced by a fixed redaction
  marker rather than the raw secret

### Requirement: Credential redaction in structured fields

The system SHALL mask values of fields whose names indicate credentials
(`api_key`, `token`, `password`, `authorization`) before logging structured
records.

#### Scenario: AI request logged at debug

- **WHEN** an AI request is logged at debug level and the request payload
  contains an `api_key` or `Authorization` header
- **THEN** the field value in the log line is replaced by a redaction marker
  and the original key is preserved so the record remains parseable

### Requirement: Pass-through for non-credential content

The system SHALL leave non-credential content untouched so logs remain
useful for debugging.

#### Scenario: Ordinary argument

- **WHEN** a logged argument contains no credential-shaped substring
- **THEN** the argument is written to the log unchanged
