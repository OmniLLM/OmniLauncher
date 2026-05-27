-- Migration 006: introduce real conversation sessions.
-- Each `conversation_messages` row is now associated with a `session_id`,
-- and `conversation_sessions` tracks per-session metadata.

CREATE TABLE IF NOT EXISTS conversation_sessions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    title           TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    last_active_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Seed a default session so legacy messages have somewhere to belong.
INSERT OR IGNORE INTO conversation_sessions (id, title) VALUES (1, 'Default');

ALTER TABLE conversation_messages ADD COLUMN session_id INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS conversation_messages_session_idx
    ON conversation_messages (session_id, id);
