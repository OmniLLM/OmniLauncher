/// Lightweight SQLite migration runner for OmniLauncher.
///
/// Migrations are numbered SQL files embedded at compile-time via
/// `include_str!`. Each migration runs exactly once; progress is tracked
/// in a `_migrations` table inside the same database.
///
/// Each statement within a migration is executed individually so that
/// additive changes (e.g. `ALTER TABLE … ADD COLUMN`) are tolerated on
/// databases that were created by older code and already have those columns.
use rusqlite::{Connection, Result};

pub mod conversation;
pub mod favorites;

/// A single migration: a version number and the SQL to execute.
pub struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

/// All known migrations in ascending order.
/// Add new entries here when the schema changes.
pub fn migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            sql: include_str!("../../migrations/001_create_tables.sql"),
        },
        Migration {
            version: 2,
            sql: include_str!("../../migrations/002_add_extra_columns.sql"),
        },
        Migration {
            version: 3,
            sql: include_str!("../../migrations/003_add_status_to_todos.sql"),
        },
        Migration {
            version: 4,
            sql: include_str!("../../migrations/004_scheduled_jobs.sql"),
        },
        Migration {
            version: 5,
            sql: include_str!("../../migrations/005_conversation_history.sql"),
        },
        Migration {
            version: 6,
            sql: include_str!("../../migrations/006_conversation_sessions.sql"),
        },
        Migration {
            version: 7,
            sql: include_str!("../../migrations/007_favorites.sql"),
        },
    ]
}

/// Returns true for errors that mean the schema change is already present
/// (e.g. column already exists, table already exists).
fn is_already_exists_error(e: &rusqlite::Error) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("duplicate column name")
        || msg.contains("already exists")
        || msg.contains("table already exists")
}

/// Execute a multi-statement SQL string one statement at a time.
/// Errors that indicate the change is already applied are silently ignored;
/// all other errors are returned immediately.
fn exec_statements(conn: &Connection, sql: &str) -> Result<()> {
    for raw in sql.split(';') {
        let stmt = raw.trim();
        // Skip blank lines and comment-only lines
        let meaningful = stmt
            .lines()
            .filter(|l| !l.trim_start().starts_with("--") && !l.trim().is_empty())
            .count();
        if meaningful == 0 {
            continue;
        }
        if let Err(e) = conn.execute_batch(stmt) {
            if !is_already_exists_error(&e) {
                return Err(e);
            }
        }
    }
    Ok(())
}

/// Ensures the `_migrations` tracking table exists, then runs every
/// migration whose version is not yet recorded.
pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    for m in migrations() {
        // ✅ Propagate errors so callers know if the DB is broken instead of
        // silently treating query failures as "not applied".
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM _migrations WHERE version = ?1",
            [m.version],
            |row| row.get(0),
        )?;
        let already_applied = count > 0;

        if already_applied {
            continue;
        }

        exec_statements(conn, m.sql)?;

        conn.execute(
            "INSERT OR IGNORE INTO _migrations (version) VALUES (?1)",
            [m.version],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod db_tests {
    use super::*;

    #[test]
    fn test_run_migrations_idempotent() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        run_migrations(&conn).expect("first migration run");
        run_migrations(&conn).expect("second migration run should be idempotent");
    }

    #[test]
    fn test_migration_tracking_records_version() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        run_migrations(&conn).expect("migrations");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |r| r.get(0))
            .expect("query");
        assert!(count > 0, "at least one migration should be recorded");
    }
}
