-- Scheduled jobs table
CREATE TABLE IF NOT EXISTS scheduled_jobs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    label       TEXT    NOT NULL,
    schedule    TEXT    NOT NULL,  -- "every:N:unit" or 5-field cron "* * * * *"
    command     TEXT    NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    last_run    TEXT,              -- ISO8601 datetime
    next_run    TEXT,              -- ISO8601 datetime (pre-computed)
    run_count   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);
