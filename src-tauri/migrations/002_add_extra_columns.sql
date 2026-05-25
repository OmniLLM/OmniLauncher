-- Migration 002: add extended columns to todos
-- rusqlite: ALTER TABLE … ADD COLUMN is safe to re-run only when guarded
-- by the migrations table, so these are plain statements here.
ALTER TABLE todos ADD COLUMN description  TEXT    NOT NULL DEFAULT '';
ALTER TABLE todos ADD COLUMN priority     INTEGER NOT NULL DEFAULT 3;
ALTER TABLE todos ADD COLUMN due_date     TEXT    NOT NULL DEFAULT '';
ALTER TABLE todos ADD COLUMN tags         TEXT    NOT NULL DEFAULT '';
ALTER TABLE todos ADD COLUMN completed_at TEXT    NOT NULL DEFAULT '';
