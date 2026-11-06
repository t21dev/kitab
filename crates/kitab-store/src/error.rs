// SPDX-License-Identifier: AGPL-3.0-or-later

//! The store's error type. Messages never contain paths or document text
//! (AGENTS.md rule 13), so they are safe to log.

/// Everything that can go wrong in the store. Nothing here panics.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// SQLite failed (disk full, locked, corrupt file, constraint, ...).
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Reading a file or creating the data directory failed.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// The database was written by a newer Kitab. It is left untouched:
    /// migrations only go forward (AGENTS.md rule 15).
    #[error("database schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew { found: u32, supported: u32 },

    /// The path is not valid Unicode, which the `documents.path` column needs.
    #[error("path is not valid Unicode")]
    NonUnicodePath,

    /// A stored value could not be decoded (hand-edited or damaged database).
    #[error("corrupt value in column {0}")]
    Corrupt(&'static str),
}

/// Shorthand for `Result<T, kitab_store::Error>`.
pub type Result<T> = std::result::Result<T, Error>;
