pub mod photos;
pub mod review;

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use rusqlite::Connection;

const SCHEMA_V1: &str = include_str!("schema.sql");

/// Single shared connection. SQLite serializes writers anyway and this server
/// is single-user, so a mutex beats a pool's complexity.
#[derive(Clone)]
pub struct DbPool {
    conn: Arc<Mutex<Connection>>,
}

impl DbPool {
    /// Runs `f` with the connection locked. Errors keep their `rusqlite::Error`
    /// as the anyhow source so callers can downcast to inspect constraint codes.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> anyhow::Result<T> {
        // A panic while holding the lock cannot leave SQLite itself inconsistent
        // (open transactions roll back on drop), so recovering from poison is safe.
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Ok(f(&conn)?)
    }
}

pub fn open(path: &Path) -> anyhow::Result<DbPool> {
    let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
    // journal_mode returns a row, so it cannot go through execute_batch/pragma_update.
    conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
        .context("enabling WAL")?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
    migrate(&conn)?;
    Ok(DbPool {
        conn: Arc::new(Mutex::new(conn)),
    })
}

fn migrate(conn: &Connection) -> anyhow::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        // One transaction so a failed first run cannot leave a half-built schema
        // that the next start would try to re-create.
        conn.execute_batch(&format!(
            "BEGIN;\n{SCHEMA_V1}\nPRAGMA user_version = 1;\nCOMMIT;"
        ))
        .context("applying schema v1")?;
    }
    Ok(())
}
