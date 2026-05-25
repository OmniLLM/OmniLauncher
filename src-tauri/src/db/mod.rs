/// Lightweight SQLite migration runner for OmniLauncher.
///
/// Migrations are numbered SQL files embedded at compile-time via
/// `include_str!`. Each migration runs exactly once; progress is tracked
/// in a `_migrations` table inside the same database.
use rusqlite::{Connection, Result};

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
    ]
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
        let already_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM _migrations WHERE version = ?1",
                [m.version],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;

        if already_applied {
            continue;
        }

        // Execute the migration SQL (may contain multiple statements).
        conn.execute_batch(m.sql)?;

        conn.execute(
            "INSERT INTO _migrations (version) VALUES (?1)",
            [m.version],
        )?;
    }

    Ok(())
}
