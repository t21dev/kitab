// SPDX-License-Identifier: AGPL-3.0-or-later

//! Kitab's local database (PLAN.md §3.6): the library, reading state, sessions,
//! closed tabs and bookmarks, in one SQLite file in WAL mode.
//!
//! # Threading
//!
//! Every call is synchronous and does disk I/O. The app owns one [`Store`] on
//! its io thread and sends it requests (PLAN.md §3.1). Never call it from the
//! GPUI thread (AGENTS.md rule 2). `Store` is `Send` but not `Sync`.
//!
//! # Start-up
//!
//! The start-up path calls [`Store::open`] then [`Store::last_session`], which
//! is one indexed query that also returns each tab's path, title and format,
//! so inactive tabs need no file access (PLAN.md §6.3). Only after that does
//! it call [`Store::begin_session`].
//!
//! # Schema
//!
//! Version [`SCHEMA_VERSION`], created and upgraded by a forward-only migrator
//! (`src/migrations/*.sql`). A database from a newer build is refused with
//! [`Error::SchemaTooNew`] and left untouched.

#![forbid(unsafe_code)]

mod bookmarks;
mod codec;
mod documents;
mod error;
mod identity;
mod marks;
mod migrate;
mod session;
mod store;

pub use bookmarks::{Bookmark, BookmarkId};
pub use documents::{DocMeta, DocumentId, DocumentRecord, RecentEntry};
pub use error::{Error, Result};
pub use identity::{FileIdentity, QUICK_HASH_SPAN, identify};
pub use marks::MarkId;
pub use migrate::SCHEMA_VERSION;
pub use session::{ClosedTab, ClosedTabId, LastSession, SessionId, SessionTab, TabRecord};
pub use store::Store;

/// The closed-tab stack keeps this many entries; older ones are dropped.
pub const MAX_CLOSED_TABS: usize = 100;
/// Sessions kept for crash recovery and history; older ones are dropped.
pub const MAX_SESSIONS: usize = 8;
/// Longest title or author kept from a document's metadata (characters).
/// Metadata is untrusted (AGENTS.md rule 8).
pub const MAX_META_CHARS: usize = 1024;
/// Upper bound for [`Store::recent`]'s `limit`.
pub const MAX_RECENT: usize = 1000;
