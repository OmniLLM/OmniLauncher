//! Lightweight persistence for the AI conversation history.
//!
//! Stores only `user` and `assistant` turns so that a restart can re-hydrate
//! enough context for follow-up questions. Tool-call rounds are intentionally
//! *not* persisted: tool results are often stale on the next launch (file
//! contents changed, network call needs to be re-issued) and replaying them
//! to the model would be misleading.

use crate::ai::client::Message;
use crate::db;
use crate::path_config;
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

/// Persist a single user/assistant turn. Other roles (system, tool) are skipped.
pub fn save_turn(role: &str, content: &str) {
    if !(role == "user" || role == "assistant") {
        return;
    }
    if content.is_empty() {
        return;
    }
    if let Ok(conn) = open() {
        let _ = conn.execute(
            "INSERT INTO conversation_messages (role, content) VALUES (?1, ?2)",
            params![role, content],
        );
    }
}

/// Load the most recent `limit` user/assistant messages, oldest first.
pub fn load_recent(limit: usize) -> Vec<Message> {
    let conn = match open() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut stmt = match conn.prepare(
        "SELECT role, content FROM (
             SELECT id, role, content FROM conversation_messages
             ORDER BY id DESC LIMIT ?1
         ) ORDER BY id ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map(params![limit as i64], |row| {
        let role: String = row.get(0)?;
        let content: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
        Ok((role, content))
    });
    let mut out = Vec::new();
    if let Ok(iter) = rows {
        for r in iter.flatten() {
            match r.0.as_str() {
                "user" => out.push(Message::user(&r.1)),
                "assistant" => out.push(Message::assistant(&r.1)),
                _ => {}
            }
        }
    }
    out
}
