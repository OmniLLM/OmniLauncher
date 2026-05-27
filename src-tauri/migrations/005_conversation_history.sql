CREATE TABLE IF NOT EXISTS conversation_messages (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    role          TEXT NOT NULL,
    content       TEXT,
    tool_calls    TEXT,
    tool_call_id  TEXT,
    name          TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS conversation_messages_created_idx
    ON conversation_messages (created_at);
