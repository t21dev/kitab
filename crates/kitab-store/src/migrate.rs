// SPDX-License-Identifier: AGPL-3.0-or-later

//! A small forward-only migrator on `PRAGMA user_version` (AGENTS.md rule 15).
//!
//! Hand-written instead of `rusqlite_migration`: the release of that crate that
//! matches our rusqlite needs Rust 1.95 (the workspace MSRV is 1.85), it ties
//! rusqlite upgrades to its own release cadence, and all we need is the loop
//! below. It uses the same `user_version` scheme, so switching later is free.

use rusqlite::Connection;

use crate::{Error, Result};

/// Migration `i` takes the schema from version `i` to `i + 1`. Append only:
/// a released entry is frozen (tests/migrations.rs checks this).
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_initial.sql"),
    include_str!("migrations/0002_marks.sql"),
    include_str!("migrations/0003_search_stats.sql"),
];

/// The schema version this build writes.
pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

/// The schema version stored in the database (0 for a new file).
pub(crate) fn current_version(conn: &Connection) -> Result<u32> {
    let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    u32::try_from(v).map_err(|_| Error::Corrupt("user_version"))
}

/// Bring the database up to [`SCHEMA_VERSION`]. Each step runs in its own
/// transaction together with its `user_version` bump, so a crash mid-way leaves
/// the database at the last complete version. A database from a newer build is
/// refused and left untouched.
pub(crate) fn migrate(conn: &mut Connection) -> Result<()> {
    let found = current_version(conn)?;
    if found > SCHEMA_VERSION {
        return Err(Error::SchemaTooNew {
            found,
            supported: SCHEMA_VERSION,
        });
    }
    for (step, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        // PRAGMA takes no bound parameters; the value is our own integer.
        tx.execute_batch(&format!("PRAGMA user_version = {}", step + 1))?;
        tx.commit()?;
    }
    Ok(())
}
