// SPDX-License-Identifier: AGPL-3.0-or-later

//! Opening the database: pragmas, migrations and the store's clock.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::Result;
use crate::migrate;

/// The open database. One per process, owned by the io thread.
pub struct Store {
    pub(crate) conn: Connection,
    /// Last timestamp handed out by [`Store::now_ms`].
    last_ts: i64,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No path: logs never contain file paths (AGENTS.md rule 13).
        f.debug_struct("Store").finish_non_exhaustive()
    }
}

impl Store {
    /// Open (or create) the database at `path`, creating its parent directory,
    /// switch it to WAL and bring the schema up to date.
    ///
    /// Fails with [`crate::Error::SchemaTooNew`] if a newer Kitab wrote it, and
    /// with [`crate::Error::Sqlite`] if the file is not a database.
    pub fn open(path: &Path) -> Result<Store> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        // WAL: readers never block the writer, and a crash loses at most the
        // last uncommitted transaction (PLAN.md §3.6 crash safety).
        // `journal_mode` returns the resulting mode as a row.
        let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        Self::init(conn)
    }

    /// A private in-memory database, for tests and for running without a data
    /// directory. Same schema; nothing is written to disk.
    pub fn open_in_memory() -> Result<Store> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Store> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA temp_store = MEMORY;
             PRAGMA cache_size = -2048;",
        )?;
        // Bounded statement cache (AGENTS.md rule 5).
        conn.set_prepared_statement_cache_capacity(32);
        migrate::migrate(&mut conn)?;
        Ok(Store { conn, last_ts: 0 })
    }

    /// The schema version stored in the open database.
    pub fn schema_version(&self) -> Result<u32> {
        migrate::current_version(&self.conn)
    }

    /// Wall-clock Unix milliseconds, made strictly increasing within this
    /// store so "most recent" orderings never tie.
    pub(crate) fn now_ms(&mut self) -> i64 {
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_millis()).ok())
            .unwrap_or(0);
        self.last_ts = wall.max(self.last_ts.saturating_add(1));
        self.last_ts
    }
}

/// Bool to the 0/1 integer the schema stores.
pub(crate) fn b(v: bool) -> i64 {
    i64::from(v)
}
