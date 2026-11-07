// SPDX-License-Identifier: AGPL-3.0-or-later

//! What the UI needs from the library store. The app implements it on an io
//! thread that owns SQLite (`kitab-store`); calls block that thread, never the UI
//! thread, so views call them from background tasks (AGENTS.md rule 2).

use kitab_core::{DocInfo, Format, ReadingState};
use std::path::{Path, PathBuf};

/// One entry on Home.
#[derive(Debug, Clone, PartialEq)]
pub struct RecentItem {
    pub path: PathBuf,
    pub title: String,
    pub author: Option<String>,
    pub format: Format,
    /// 0.0..=1.0
    pub progress: f32,
    /// Unix milliseconds.
    pub last_opened_ms: i64,
    pub missing: bool,
    /// A favourite: its card wears a ribbon.
    pub favourite: bool,
}

/// A page that matches a library search.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryHit {
    pub path: PathBuf,
    pub title: String,
    pub format: Format,
    /// 0-based.
    pub page: u32,
    /// Words around the match; the matched words between `[` and `]`.
    pub snippet: String,
}

/// Reading stats for Home.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatsView {
    pub today_seconds: i64,
    pub week_seconds: i64,
    pub streak_days: u32,
    pub longest_streak: u32,
    /// Seconds per day, oldest first, ending today.
    pub recent_days: Vec<i64>,
    /// Read most in the last 30 days: (title, path, seconds).
    pub top: Vec<(String, PathBuf, i64)>,
    /// How many documents library search covers.
    pub indexed: u64,
}

/// A list Home can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShelfKind {
    /// Continue reading and Recent (opened documents, newest first).
    #[default]
    Recent,
    /// Every document the library knows.
    All,
    Favourites,
    InProgress,
    Finished,
    Collection(i64),
}

/// A collection and how many documents it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionInfo {
    pub id: i64,
    pub name: String,
    pub count: u32,
}

/// A tab to restore at start-up.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionTab {
    pub path: PathBuf,
    pub active: bool,
}

pub trait Library: Send + Sync + 'static {
    /// Record that `path` was opened (it moves to the top of Recent) and return
    /// the reading state saved for it, if any.
    fn opened(&self, path: &Path, info: &DocInfo) -> Option<ReadingState>;
    /// Save the reading state. The caller debounces (≤ 1 write per 2 s, plus on close).
    fn save_state(&self, path: &Path, state: ReadingState);
    fn recent(&self, limit: usize) -> Vec<RecentItem>;
    /// Replace the current session's tabs.
    fn save_session(&self, tabs: Vec<SessionTab>);
    /// Mark the session closed cleanly (on quit).
    fn end_session(&self);
    /// The tabs of the previous session.
    fn last_session(&self) -> Vec<SessionTab>;
    /// Marks kept for `path` (not yet saved into the file, or a format Kitab
    /// doesn't write into), with their store ids.
    fn marks(&self, path: &Path) -> Vec<(i64, kitab_core::Mark)> {
        let _ = path;
        Vec::new()
    }
    /// Keep a mark; its store id, or `None` if it couldn't be kept.
    fn add_mark(&self, path: &Path, mark: kitab_core::Mark) -> Option<i64> {
        let _ = (path, mark);
        None
    }
    fn delete_mark(&self, id: i64) {
        let _ = id;
    }
    /// Forget every mark of `path` (they were saved into the file).
    fn clear_marks(&self, path: &Path) {
        let _ = path;
    }
    /// Bookmarks of `path`: (library id, page, name).
    fn bookmarks(&self, path: &Path) -> Vec<(i64, u32, String)> {
        let _ = path;
        Vec::new()
    }
    fn add_bookmark(&self, path: &Path, page: u32, title: &str) -> Option<i64> {
        let _ = (path, page, title);
        None
    }
    fn remove_bookmark(&self, id: i64) {
        let _ = id;
    }
    /// Forget the Recent list and saved reading positions (Settings ›
    /// Advanced). Marks and bookmarks stay. Returns whether it worked.
    fn clear_history(&self) -> bool {
        false
    }
    /// Compact the database. Returns its size before and after, in bytes.
    fn optimize(&self) -> Option<(u64, u64)> {
        None
    }
    /// The database's size in bytes, if there is one.
    fn size_bytes(&self) -> Option<u64> {
        None
    }
    /// Whether `path`'s text needs (re)indexing for a file of `mtime`.
    fn needs_text(&self, path: &Path, mtime: i64) -> bool {
        let _ = (path, mtime);
        false
    }
    /// Keep `path`'s page text for library search.
    fn index_text(&self, path: &Path, mtime: i64, pages: Vec<String>) {
        let _ = (path, mtime, pages);
    }
    /// Pages across the library that match `query`, best first.
    fn search(&self, query: &str, limit: usize) -> Vec<LibraryHit> {
        let _ = (query, limit);
        Vec::new()
    }
    /// Add `seconds` of reading `path` on `day` ('YYYY-MM-DD', local).
    fn add_reading(&self, path: &Path, day: &str, seconds: i64) {
        let _ = (path, day, seconds);
    }
    /// Reading stats as of `today` ('YYYY-MM-DD', local).
    fn reading_stats(&self, today: &str) -> Option<StatsView> {
        let _ = today;
        None
    }
    /// A picture of the document's first page for its card on Home, as
    /// PNG, about `size` px on its longer side. Cached on disk, so it is
    /// drawn once per file version. `None` if it can't be drawn.
    fn cover(&self, path: &Path, size: u32) -> Option<Vec<u8>> {
        let _ = (path, size);
        None
    }
    /// The documents on a shelf (not Recent; that's [`Library::recent`]).
    fn shelf(&self, kind: ShelfKind) -> Vec<RecentItem> {
        let _ = kind;
        Vec::new()
    }
    fn collections(&self) -> Vec<CollectionInfo> {
        Vec::new()
    }
    /// A new collection (or the one with that name); its id.
    fn create_collection(&self, name: &str) -> Option<i64> {
        let _ = name;
        None
    }
    fn rename_collection(&self, id: i64, name: &str) {
        let _ = (id, name);
    }
    fn delete_collection(&self, id: i64) {
        let _ = id;
    }
    fn set_in_collection(&self, id: i64, path: &Path, inside: bool) {
        let _ = (id, path, inside);
    }
    /// A document's favourite flag and collections.
    fn doc_flags(&self, path: &Path) -> Option<(bool, Vec<i64>)> {
        let _ = path;
        None
    }
    fn set_favourite(&self, path: &Path, favourite: bool) {
        let _ = (path, favourite);
    }
    fn set_finished(&self, path: &Path, finished: bool) {
        let _ = (path, finished);
    }
    fn watched_folders(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn watch_folder(&self, path: &Path) {
        let _ = path;
    }
    fn unwatch_folder(&self, path: &Path) {
        let _ = path;
    }
    /// Add the documents in the watched folders that the library doesn't
    /// know yet. Returns how many were added.
    fn scan_watched(&self) -> u32 {
        0
    }
}

/// A library that remembers nothing (tests, or when the data folder can't be opened).
pub struct NoLibrary;

impl Library for NoLibrary {
    fn opened(&self, _: &Path, _: &DocInfo) -> Option<ReadingState> {
        None
    }
    fn save_state(&self, _: &Path, _: ReadingState) {}
    fn recent(&self, _: usize) -> Vec<RecentItem> {
        Vec::new()
    }
    fn save_session(&self, _: Vec<SessionTab>) {}
    fn end_session(&self) {}
    fn last_session(&self) -> Vec<SessionTab> {
        Vec::new()
    }
}
