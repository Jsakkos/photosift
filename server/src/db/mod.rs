pub mod helper;
pub mod migrations;
pub mod photos;
pub mod review;

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use rusqlite::{Connection, OpenFlags};

pub const SCHEMA_V1: &str = include_str!("schema.sql");

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

    /// Runs `f` in one transaction, committed when it returns `Ok`. Unlike
    /// `with`, `f` may fail with any error (e.g. a typed domain error).
    pub fn with_tx<T>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.unchecked_transaction()?;
        let value = f(&tx)?;
        tx.commit()?;
        Ok(value)
    }
}

pub fn open(path: &Path) -> anyhow::Result<DbPool> {
    refuse_foreign(path)?;
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

/// Tables every PhotoSift schema since v1 has. The removed Tauri prototype
/// kept its own `photosift.db` in the same data roots with none of them.
const V1_TABLES: &[&str] = &["shoots", "photos", "files", "camera_offsets"];

/// Refuses a database that has tables but not PhotoSift's own. `user_version`
/// alone cannot tell: the prototype's numbers overlap this schema's steps, so
/// `migrate` would skip steps and fail partway. Runs on a read-only connection
/// before `open` switches to WAL, which would rewrite the file header.
fn refuse_foreign(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("opening {} read-only", path.display()))?;
    let tables: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()
        .with_context(|| format!("reading the tables of {}", path.display()))?;
    let has_v1 = V1_TABLES.iter().all(|t| tables.iter().any(|n| n == t));
    if tables.is_empty() || has_v1 {
        return Ok(());
    }
    anyhow::bail!(
        "{} is a database from the old PhotoSift prototype (or another program), not one this \
         version can use. Nothing was changed. Move it aside (for example, rename it to \
         photosift-prototype.db) and start PhotoSift again; it will create a new database.",
        path.display()
    )
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
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for &(step, sql) in migrations::STEPS.iter().filter(|(n, _)| *n > version) {
        conn.execute_batch(&format!(
            "BEGIN;
{sql}
PRAGMA user_version = {step};
COMMIT;"
        ))
        .with_context(|| format!("applying migration step {step}"))?;
    }
    Ok(())
}
