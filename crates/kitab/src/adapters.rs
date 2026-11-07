// SPDX-License-Identifier: AGPL-3.0-or-later

//! Connect the UI's traits to the real engine (kitab-mupdf) and store
//! (kitab-store). Both are blocking APIs; the UI only calls them from
//! background tasks, and the store lives on its own io thread (PLAN.md §3.1).

use kitab_core::{
    Bitmap, DocInfo, FindHit, FindOptions, Link, OpenError, ReadingState, RenderRequest,
};
use kitab_mupdf::{DocId, Engine};
use kitab_store::{DocMeta, SessionId, Store, TabRecord};
use kitab_ui::{DocEngine, DocHandle, Library, RecentItem, SessionTab};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

// ── engine ──────────────────────────────────────────────────────────────

pub struct MupdfEngine {
    engine: Engine,
    ids: Mutex<HashMap<DocHandle, DocId>>,
    next: AtomicU64,
}

impl MupdfEngine {
    pub fn new() -> Self {
        Self {
            engine: Engine::new(Engine::default_threads()),
            ids: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
        }
    }

    fn id(&self, h: DocHandle) -> Option<DocId> {
        self.ids.lock().ok()?.get(&h).copied()
    }
}

impl DocEngine for MupdfEngine {
    fn shrink_caches(&self) {
        let _ = self.engine.shrink_caches(0);
    }

    fn open(&self, path: &Path, password: Option<&str>) -> Result<(DocHandle, DocInfo), OpenError> {
        let (id, info) = self.engine.open(path, password)?;
        let h = self.next.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut ids) = self.ids.lock() {
            ids.insert(h, id);
        }
        Ok((h, info))
    }

    fn render(&self, doc: DocHandle, req: RenderRequest) -> Result<Option<Bitmap>, String> {
        let id = self.id(doc).ok_or("document is closed")?;
        self.engine.render(id, req)
    }

    fn render_when(
        &self,
        doc: DocHandle,
        req: RenderRequest,
        wanted: kitab_ui::engine::Wanted,
    ) -> Result<Option<Bitmap>, String> {
        let id = self.id(doc).ok_or("document is closed")?;
        self.engine.render_when(id, req, move || wanted())
    }

    fn annotations(&self, doc: DocHandle) -> Vec<kitab_core::FileAnnot> {
        self.id(doc)
            .and_then(|id| self.engine.annotations(id).ok())
            .unwrap_or_default()
    }

    fn write_marks(
        &self,
        src: &Path,
        password: Option<&str>,
        marks: &[kitab_core::Mark],
        out: &Path,
    ) -> Result<usize, String> {
        kitab_mupdf::marks::write_marks(src, password, marks, out)
    }

    fn relayout(
        &self,
        doc: DocHandle,
        layout: kitab_core::BookLayout,
    ) -> Result<kitab_core::DocInfo, String> {
        let id = self.id(doc).ok_or("document is closed")?;
        self.engine.relayout(id, layout)
    }

    fn set_generation(&self, doc: DocHandle, generation: u64) {
        if let Some(id) = self.id(doc) {
            self.engine.set_generation(id, generation);
        }
    }

    fn links(&self, doc: DocHandle, page: u32) -> Vec<Link> {
        self.id(doc)
            .and_then(|id| self.engine.links(id, page).ok())
            .unwrap_or_default()
    }

    fn find(
        &self,
        doc: DocHandle,
        query: &str,
        options: FindOptions,
        on_hit: &mut dyn FnMut(FindHit) -> bool,
    ) {
        if let Some(id) = self.id(doc) {
            let _ = self.engine.find(id, query, options, on_hit);
        }
    }

    fn find_page(
        &self,
        doc: DocHandle,
        page: u32,
        query: &str,
        options: FindOptions,
    ) -> Vec<FindHit> {
        self.id(doc)
            .and_then(|id| self.engine.find_page(id, page, query, options).ok())
            .unwrap_or_default()
    }

    fn page_chars(&self, doc: DocHandle, page: u32) -> Vec<kitab_core::TextChar> {
        self.id(doc)
            .and_then(|id| self.engine.page_chars(id, page).ok())
            .unwrap_or_default()
    }

    fn page_text(&self, doc: DocHandle, page: u32) -> Option<String> {
        self.id(doc)
            .and_then(|id| self.engine.page_plain_text(id, page).ok())
    }

    fn extract_pages(
        &self,
        src: &Path,
        password: Option<&str>,
        pages: &[u32],
        out: &Path,
    ) -> Result<(), String> {
        kitab_mupdf::tools::extract_pages(src, password, pages, out)
    }

    fn merge(&self, inputs: &[(PathBuf, Option<String>)], out: &Path) -> Result<u32, String> {
        kitab_mupdf::tools::merge(inputs, out)
    }

    fn close(&self, doc: DocHandle) {
        let id = self.ids.lock().ok().and_then(|mut ids| ids.remove(&doc));
        if let Some(id) = id {
            self.engine.close(id);
        }
    }
}

// ── library ─────────────────────────────────────────────────────────────

type Job = Box<dyn FnOnce(&mut IoState) + Send>;

struct IoState {
    store: Option<Store>,
    /// The session this run writes to; begun lazily, after the previous one was read.
    session: Option<SessionId>,
}

/// The library on an io thread that owns SQLite. If the database can't be
/// opened, every call answers as if the library were empty.
pub struct StoreLibrary {
    jobs: Mutex<Sender<Job>>,
}

impl StoreLibrary {
    /// Starts the io thread; the database opens there, off the start-up path.
    pub fn start(db_path: Option<PathBuf>) -> Arc<Self> {
        let (tx, rx) = channel::<Job>();
        let _ = std::thread::Builder::new()
            .name("kitab-io".into())
            .spawn(move || {
                let store = db_path.and_then(|p| match Store::open(&p) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        log::warn!("library database unavailable: {e}");
                        None
                    }
                });
                let mut state = IoState {
                    store,
                    session: None,
                };
                while let Ok(job) = rx.recv() {
                    job(&mut state);
                }
            });
        Arc::new(Self {
            jobs: Mutex::new(tx),
        })
    }

    /// Run `f` on the io thread and wait for its answer (callers are background tasks).
    fn call<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut IoState) -> R + Send + 'static,
    ) -> Option<R> {
        let (tx, rx) = channel();
        let job: Job = Box::new(move |s| {
            let _ = tx.send(f(s));
        });
        self.jobs.lock().ok()?.send(job).ok()?;
        rx.recv().ok()
    }
}

fn title_or_name(title: Option<String>, path: &Path) -> String {
    title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    })
}

impl StoreLibrary {
    /// The document row for `path`, if the library knows it.
    fn doc_id(store: &kitab_store::Store, path: &Path) -> Option<kitab_store::DocumentId> {
        store.document_by_path(path).ok().flatten().map(|d| d.id)
    }
}

impl Library for StoreLibrary {
    fn marks(&self, path: &Path) -> Vec<(i64, kitab_core::Mark)> {
        let path = path.to_path_buf();
        self.call(move |s| {
            let store = s.store.as_ref()?;
            let id = Self::doc_id(store, &path)?;
            let marks = store.marks(id).ok()?;
            Some(marks.into_iter().map(|(m, mark)| (m.0, mark)).collect())
        })
        .flatten()
        .unwrap_or_default()
    }

    fn add_mark(&self, path: &Path, mark: kitab_core::Mark) -> Option<i64> {
        let path = path.to_path_buf();
        self.call(move |s| {
            let store = s.store.as_mut()?;
            let id = Self::doc_id(store, &path)?;
            store.add_mark(id, &mark).ok().map(|m| m.0)
        })
        .flatten()
    }

    fn bookmarks(&self, path: &Path) -> Vec<(i64, u32, String)> {
        let path = path.to_path_buf();
        self.call(move |s| {
            let store = s.store.as_ref()?;
            let id = Self::doc_id(store, &path)?;
            let rows = store.bookmarks(id).ok()?;
            Some(
                rows.into_iter()
                    .map(|b| {
                        let kitab_core::Locator::Fixed { page, .. } = b.locator;
                        (b.id.0, page, b.title)
                    })
                    .collect(),
            )
        })
        .flatten()
        .unwrap_or_default()
    }

    fn add_bookmark(&self, path: &Path, page: u32, title: &str) -> Option<i64> {
        let (path, title) = (path.to_path_buf(), title.to_string());
        self.call(move |s| {
            let store = s.store.as_mut()?;
            let id = Self::doc_id(store, &path)?;
            let at = kitab_core::Locator::Fixed {
                page,
                x: 0.0,
                y: 0.0,
            };
            store.add_bookmark(id, &at, &title).ok().map(|b| b.0)
        })
        .flatten()
    }

    fn remove_bookmark(&self, id: i64) {
        let _ = self.call(move |s| {
            let store = s.store.as_mut()?;
            store.remove_bookmark(kitab_store::BookmarkId(id)).ok()
        });
    }

    fn delete_mark(&self, id: i64) {
        let _ = self.call(move |s| {
            let store = s.store.as_mut()?;
            store.delete_mark(kitab_store::MarkId(id)).ok()
        });
    }

    fn clear_history(&self) -> bool {
        self.call(|s| s.store.as_mut()?.clear_history().ok())
            .flatten()
            .is_some()
    }

    fn optimize(&self) -> Option<(u64, u64)> {
        self.call(|s| s.store.as_mut()?.optimize().ok()).flatten()
    }

    fn size_bytes(&self) -> Option<u64> {
        self.call(|s| s.store.as_ref()?.size_bytes().ok()).flatten()
    }

    fn clear_marks(&self, path: &Path) {
        let path = path.to_path_buf();
        let _ = self.call(move |s| {
            let store = s.store.as_mut()?;
            let id = Self::doc_id(store, &path)?;
            store.clear_marks(id).ok()
        });
    }

    fn opened(&self, path: &Path, info: &DocInfo) -> Option<ReadingState> {
        let (path, meta) = (path.to_path_buf(), DocMeta::from(info));
        self.call(move |s| {
            let store = s.store.as_mut()?;
            let identity = kitab_store::identify(&path).ok()?;
            let id = store.upsert_document(&identity, &path, &meta).ok()?;
            let _ = store.mark_opened(id);
            store.reading_state(id).ok().flatten()
        })
        .flatten()
    }

    fn save_state(&self, path: &Path, state: ReadingState) {
        let path = path.to_path_buf();
        let _ = self.call(move |s| {
            let store = s.store.as_mut()?;
            let doc = store.document_by_path(&path).ok().flatten()?;
            store.save_reading_state(doc.id, &state).ok()
        });
    }

    fn recent(&self, limit: usize) -> Vec<RecentItem> {
        self.call(move |s| {
            let store = s.store.as_ref()?;
            let entries = store.recent(limit).ok()?;
            Some(
                entries
                    .into_iter()
                    .map(|e| RecentItem {
                        title: title_or_name(e.title, &e.path),
                        path: e.path,
                        author: e.author,
                        format: e.format,
                        progress: e.progress.unwrap_or(0.0),
                        last_opened_ms: e.last_opened_at,
                        missing: e.missing,
                    })
                    .collect(),
            )
        })
        .flatten()
        .unwrap_or_default()
    }

    fn save_session(&self, tabs: Vec<SessionTab>) {
        let _ = self.call(move |s| {
            let store = s.store.as_mut()?;
            let session = match s.session {
                Some(id) => id,
                None => {
                    let id = store.begin_session().ok()?;
                    s.session = Some(id);
                    id
                }
            };
            let records: Vec<TabRecord> = tabs
                .iter()
                .filter_map(|t| {
                    let doc = store.document_by_path(&t.path).ok().flatten()?;
                    Some(TabRecord {
                        document_id: doc.id,
                        pinned: false,
                        active: t.active,
                        scroll: None,
                    })
                })
                .collect();
            store.save_session_tabs(session, &records).ok()
        });
    }

    fn end_session(&self) {
        let _ = self.call(move |s| {
            let (store, id) = (s.store.as_mut()?, s.session?);
            store.end_session_cleanly(id).ok()
        });
    }

    fn last_session(&self) -> Vec<SessionTab> {
        self.call(move |s| {
            let last = s.store.as_ref()?.last_session().ok().flatten()?;
            Some(
                last.tabs
                    .into_iter()
                    .map(|t| SessionTab {
                        path: t.path,
                        active: t.tab.active,
                    })
                    .collect(),
            )
        })
        .flatten()
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pdf/01-text-outline.pdf")
    }

    /// The session saved after a document opened comes back on the next run,
    /// with its reading state (the shell saves again on `Opened` for this reason).
    #[test]
    fn session_and_state_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("kitab-adapters-{}", std::process::id()));
        // A reused process id must not find an earlier run's database.
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let db = dir.join("library.sqlite3");
        let path = fixture();
        let engine = MupdfEngine::new();
        let (h, info) = engine.open(&path, None).expect("fixture opens");

        let lib = StoreLibrary::start(Some(db.clone()));
        // Before the library has seen the document, a session can't name it.
        lib.save_session(vec![SessionTab {
            path: path.clone(),
            active: true,
        }]);
        assert!(lib.opened(&path, &info).is_none());
        lib.save_session(vec![SessionTab {
            path: path.clone(),
            active: true,
        }]);
        let state = ReadingState {
            locator: kitab_core::Locator::Fixed {
                page: 3,
                x: 0.0,
                y: 12.0,
            },
            zoom: kitab_core::ZoomMode::FitWidth,
            rotation: 0,
            progress: 0.1,
        };
        lib.save_state(&path, state);
        lib.end_session();
        drop(lib);

        let lib = StoreLibrary::start(Some(db));
        assert_eq!(
            lib.last_session(),
            vec![SessionTab {
                path: path.clone(),
                active: true
            }]
        );
        assert_eq!(lib.opened(&path, &info), Some(state));
        assert_eq!(lib.recent(10).len(), 1);
        engine.close(h);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
