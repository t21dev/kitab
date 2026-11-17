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
    /// The page the words are on (0-based); `None` for a match on the
    /// title or file name (it opens where you left off).
    pub page: Option<u32>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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
    /// How many documents the library holds (Home's "All").
    fn library_size(&self) -> usize {
        0
    }
    /// Why the library couldn't be opened, in words for the user (once, at
    /// start-up); `None` when it's fine.
    fn problem(&self) -> Option<String> {
        None
    }
    /// Quitting: save these reading states and (when given) the tabs, ahead
    /// of [`Library::end_session`]. Must not wait for the work to finish:
    /// the window is closing.
    fn save_on_quit(&self, states: Vec<(PathBuf, ReadingState)>, tabs: Option<Vec<SessionTab>>) {
        for (path, state) in states {
            self.save_state(&path, state);
        }
        if let Some(tabs) = tabs {
            self.save_session(tabs);
        }
    }
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
    /// `missing` is what the library last knew: the files aren't looked at
    /// here (thousands of them, maybe on a slow drive), so a shelf shows at
    /// once; [`missing_paths`] checks them after.
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
    /// Take `path` off Home (Recent, shelves, collections, search). The file
    /// itself isn't touched; opening it again brings it back.
    fn forget(&self, path: &Path) -> bool {
        let _ = path;
        false
    }
    /// A missing file was found at `to`: its history follows it. False when
    /// `to` is already in the library as another document.
    fn relocate(&self, from: &Path, to: &Path) -> bool {
        let _ = (from, to);
        false
    }
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
    /// Stop watching a folder; the documents only it brought in (never
    /// opened, not favourites, in no collection) leave the library. Returns
    /// how many left.
    fn unwatch_folder(&self, path: &Path) -> usize {
        let _ = path;
        0
    }
    /// Add the documents in the watched folders that the library doesn't
    /// know yet. Returns how many were added.
    fn scan_watched(&self) -> u32 {
        0
    }
    /// Put every document in `dir` (and its subfolders) in collection `id`,
    /// adding the ones the library doesn't know yet (GitHub #10). The
    /// folder isn't watched. Returns how many documents the folder held.
    fn collect_folder(&self, dir: &Path, id: i64) -> usize {
        let _ = (dir, id);
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

/// Which of `paths` aren't there. A drive or share that isn't there at all
/// (unplugged, offline) is noticed once and its files counted missing
/// without asking for each; the rest are looked at a few threads at a time,
/// since one slow drive answers each question slowly.
pub fn missing_paths(paths: &[PathBuf]) -> std::collections::HashSet<PathBuf> {
    use std::collections::{HashMap, HashSet};
    // The drive (C:\, \\server\share\, /) each path is on.
    let root = |p: &Path| p.ancestors().last().map(Path::to_path_buf);
    let mut roots: HashMap<PathBuf, bool> = HashMap::new();
    let mut gone = HashSet::new();
    let mut ask = Vec::new();
    for p in paths {
        let here = match root(p) {
            Some(r) if !r.as_os_str().is_empty() => {
                *roots.entry(r.clone()).or_insert_with(|| r.exists())
            }
            _ => true,
        };
        if here {
            ask.push(p);
        } else {
            gone.insert(p.clone());
        }
    }
    if ask.is_empty() {
        return gone;
    }
    let workers = 8.min(ask.len());
    let chunk = ask.len().div_ceil(workers);
    std::thread::scope(|s| {
        let jobs: Vec<_> = ask
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .filter(|p| !p.exists())
                        .map(|p| (*p).clone())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for job in jobs {
            gone.extend(job.join().unwrap_or_default());
        }
    });
    gone
}

/// Which of `paths` are gone for good: the folder is there and the file
/// isn't. A file whose folder can't be seen (an unplugged drive, an offline
/// share, a folder moved away) is left out, since it may come back.
pub fn gone_for_good(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| !p.exists() && p.parent().is_some_and(Path::is_dir))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{gone_for_good, missing_paths};

    #[test]
    fn only_files_in_folders_that_are_there_are_gone_for_good() {
        let dir = std::env::temp_dir().join(format!("kitab-gone-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let here = dir.join("here.pdf");
        std::fs::write(&here, b"%PDF").unwrap();
        let gone = dir.join("gone.pdf");
        let away = dir.join("no-such-folder").join("book.pdf");
        let found = gone_for_good(&[here, gone.clone(), away]);
        assert_eq!(found, vec![gone]);
        let _ = std::fs::remove_dir_all(&dir);
    }
    use std::path::PathBuf;

    #[test]
    fn missing_files_and_absent_drives_are_found() {
        let dir = std::env::temp_dir().join(format!("kitab-missing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let here = dir.join("here.pdf");
        std::fs::write(&here, b"%PDF").unwrap();
        let gone = dir.join("gone.pdf");
        // A drive letter nothing is on (an unplugged drive).
        #[cfg(windows)]
        let away = ('D'..='Z')
            .rev()
            .map(|c| PathBuf::from(format!(r"{c}:\")))
            .find(|r| !r.exists())
            .expect("a free drive letter")
            .join("book.pdf");
        #[cfg(not(windows))]
        let away = PathBuf::from("/kitab-no-such-dir/book.pdf");
        let mut paths = vec![here.clone(), gone.clone(), away.clone()];
        // Enough to be spread over several threads.
        for i in 0..40 {
            paths.push(dir.join(format!("also-gone-{i}.pdf")));
        }
        let missing = missing_paths(&paths);
        assert!(!missing.contains(&here));
        assert!(missing.contains(&gone));
        assert!(missing.contains(&away));
        assert_eq!(missing.len(), paths.len() - 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
