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
use serde::Serialize;

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

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: i64,
    pub title: String,
    pub created_at: String,
    pub last_active_at: String,
    pub message_count: i64,
}

fn short_title(text: &str, max: usize) -> String {
    let cleaned: String = text
        .trim()
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let chars: Vec<char> = cleaned.chars().collect();
    if chars.len() <= max {
        cleaned
    } else {
        let mut s: String = chars.into_iter().take(max).collect();
        s.push('…');
        s
    }
}

/// Most recently active session id, creating one on first use.
pub fn current_session_id() -> i64 {
    let conn = match open() {
        Ok(c) => c,
        Err(_) => return 1,
    };
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM conversation_sessions \
             ORDER BY datetime(last_active_at) DESC, id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return id;
    }
    let _ = conn.execute(
        "INSERT INTO conversation_sessions (title) VALUES (?1)",
        params!["New chat"],
    );
    conn.last_insert_rowid()
}

/// Create and return a brand-new session id.
pub fn start_new_session(title: Option<&str>) -> i64 {
    let conn = match open() {
        Ok(c) => c,
        Err(_) => return 1,
    };
    let t = title.unwrap_or("").to_string();
    if conn
        .execute(
            "INSERT INTO conversation_sessions (title) VALUES (?1)",
            params![t],
        )
        .is_err()
    {
        return current_session_id();
    }
    conn.last_insert_rowid()
}

fn touch_session(conn: &Connection, session_id: i64, first_user_msg: Option<&str>) {
    let _ = conn.execute(
        "UPDATE conversation_sessions SET last_active_at = datetime('now') WHERE id = ?1",
        params![session_id],
    );
    if let Some(msg) = first_user_msg {
        let title = short_title(msg, 60);
        if !title.is_empty() {
            let _ = conn.execute(
                "UPDATE conversation_sessions SET title = ?1 \
                 WHERE id = ?2 AND (title IS NULL OR title = '' OR title = 'New chat')",
                params![title, session_id],
            );
        }
    }
}

/// Persist a single user/assistant turn into the given session.
pub fn save_turn(session_id: i64, role: &str, content: &str) {
    if !(role == "user" || role == "assistant") {
        return;
    }
    if content.is_empty() {
        return;
    }
    if let Ok(conn) = open() {
        let _ = conn.execute(
            "INSERT INTO conversation_messages (role, content, session_id) VALUES (?1, ?2, ?3)",
            params![role, content, session_id],
        );
        let first_user = if role == "user" { Some(content) } else { None };
        touch_session(&conn, session_id, first_user);
    }
}

/// Load the most recent `limit` user/assistant messages from a session,
/// oldest first.
pub fn load_recent_for_session(session_id: i64, limit: usize) -> Vec<Message> {
    let conn = match open() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut stmt = match conn.prepare(
        "SELECT role, content FROM (
             SELECT id, role, content FROM conversation_messages
             WHERE session_id = ?1
             ORDER BY id DESC LIMIT ?2
         ) ORDER BY id ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map(params![session_id, limit as i64], |row| {
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

/// List all sessions (newest first), filtering out abandoned empty ones
/// except for the currently-active one.
pub fn list_sessions() -> Vec<SessionInfo> {
    let conn = match open() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut stmt = match conn.prepare(
        "SELECT s.id, COALESCE(s.title,''), COALESCE(s.created_at,''), \
                COALESCE(s.last_active_at,''), \
                (SELECT COUNT(*) FROM conversation_messages m WHERE m.session_id = s.id) \
         FROM conversation_sessions s \
         ORDER BY datetime(s.last_active_at) DESC, s.id DESC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map([], |r| {
        Ok(SessionInfo {
            id: r.get(0)?,
            title: r.get(1)?,
            created_at: r.get(2)?,
            last_active_at: r.get(3)?,
            message_count: r.get(4)?,
        })
    });
    let mut out: Vec<SessionInfo> = Vec::new();
    if let Ok(iter) = rows {
        for v in iter.flatten() {
            out.push(v);
        }
    }
    if out.len() > 1 {
        let head = out.remove(0);
        out.retain(|s| s.message_count > 0);
        out.insert(0, head);
    }
    out
}

/// Hard-delete a session and all of its messages.
pub fn delete_session(session_id: i64) -> bool {
    let conn = match open() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let _ = conn.execute(
        "DELETE FROM conversation_messages WHERE session_id = ?1",
        params![session_id],
    );
    conn.execute(
        "DELETE FROM conversation_sessions WHERE id = ?1",
        params![session_id],
    )
    .is_ok()
}

/// Bump a session's `last_active_at` so it becomes "current" on next lookup.
pub fn touch_for_switch(session_id: i64) {
    if let Ok(conn) = open() {
        touch_session(&conn, session_id, None);
    }
}
