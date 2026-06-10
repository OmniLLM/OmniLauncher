//! Server-side persistence for favorited launcher results.
//!
//! Favorites were previously stored only in the browser's `localStorage`, which
//! made them invisible to the backend and inconsistent with every other piece
//! of persisted state. Each favorite is a full `QueryResult` snapshot so it can
//! be rendered and re-executed without re-running the originating search.

use crate::db;
use crate::path_config;
use crate::plugins::QueryResult;
use rusqlite::{params, Connection};

fn db_path() -> std::path::PathBuf {
    path_config::data_dir().join("omnilauncher.sqlite")
}

fn open() -> rusqlite::Result<Connection> {
    let dir = path_config::data_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Err(rusqlite::Error::InvalidPath(
            format!("Failed to create data dir {:?}: {}", dir, e).into(),
        ));
    }
    let conn = Connection::open(db_path())?;
    db::run_migrations(&conn)?;
    Ok(conn)
}

/// All favorites in insertion order (oldest first), as full `QueryResult`s.
pub fn list_favorites() -> Vec<QueryResult> {
    let conn = match open() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut stmt = match conn.prepare(
        "SELECT fav_id, title, subtitle, icon, score, action_type, action_data, source
         FROM favorites ORDER BY position ASC, id ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map([], |row| {
        Ok(QueryResult {
            id: row.get(0)?,
            title: row.get(1)?,
            subtitle: row.get(2)?,
            icon: row.get(3)?,
            score: row.get(4)?,
            action_type: row.get(5)?,
            action_data: row.get(6)?,
            source: row.get(7)?,
        })
    });
    match rows {
        Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Add (or upsert) a favorite. The `id` field of the `QueryResult` is the stable
/// favorite key. Appends to the end of the ordered list.
pub fn add_favorite(result: &QueryResult) -> Result<(), String> {
    let conn = open().map_err(|e| format!("DB open failed: {e}"))?;
    let next_pos: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM favorites",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO favorites
            (fav_id, title, subtitle, icon, score, action_type, action_data, source, position)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(fav_id) DO UPDATE SET
            title=excluded.title, subtitle=excluded.subtitle, icon=excluded.icon,
            score=excluded.score, action_type=excluded.action_type,
            action_data=excluded.action_data, source=excluded.source",
        params![
            result.id,
            result.title,
            result.subtitle,
            result.icon,
            result.score,
            result.action_type,
            result.action_data,
            result.source,
            next_pos,
        ],
    )
    .map_err(|e| format!("Failed to add favorite: {e}"))?;
    Ok(())
}

/// Remove a favorite by its id. No-op if it does not exist.
pub fn remove_favorite(fav_id: &str) -> Result<(), String> {
    let conn = open().map_err(|e| format!("DB open failed: {e}"))?;
    conn.execute("DELETE FROM favorites WHERE fav_id = ?1", params![fav_id])
        .map_err(|e| format!("Failed to remove favorite: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> QueryResult {
        QueryResult {
            id: id.to_string(),
            title: format!("Title {id}"),
            subtitle: Some("sub".into()),
            icon: Some("⭐".into()),
            score: 100,
            action_type: "open".into(),
            action_data: "data".into(),
            source: Some("app_launcher".into()),
        }
    }

    #[test]
    fn add_list_remove_roundtrip() {
        // Hold the shared env-lock for the full test so concurrent tests in
        // other modules can't mutate OMNILAUNCHER_CONFIG_DIR mid-way and
        // redirect add/list/remove to different DBs.
        let _guard = path_config::CONFIG_DIR_ENV_LOCK.blocking_lock();
        // Uses the real data dir DB; guard the test behind a unique id so it is
        // self-cleaning and doesn't collide with other rows.
        let id = "test-fav-roundtrip-zzz";
        let _ = remove_favorite(id);
        add_favorite(&sample(id)).expect("add");
        assert!(list_favorites().iter().any(|f| f.id == id));
        remove_favorite(id).expect("remove");
        assert!(!list_favorites().iter().any(|f| f.id == id));
    }
}
