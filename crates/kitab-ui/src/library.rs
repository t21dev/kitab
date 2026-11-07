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
