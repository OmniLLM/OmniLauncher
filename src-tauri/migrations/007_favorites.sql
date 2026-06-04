-- Favorites: starred launcher results, persisted server-side.
-- Each row stores a full QueryResult snapshot so the favorite can be rendered
-- and re-executed without re-running the originating search.
CREATE TABLE IF NOT EXISTS favorites (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    fav_id      TEXT    NOT NULL UNIQUE,
    title       TEXT    NOT NULL,
    subtitle    TEXT,
    icon        TEXT,
    score       INTEGER NOT NULL DEFAULT 0,
    action_type TEXT    NOT NULL,
    action_data TEXT    NOT NULL,
    source      TEXT,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);
