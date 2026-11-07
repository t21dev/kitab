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

/// Where trusted signing certificates live (PRIVACY.md: in Kitab's data).
fn trust_dir() -> Option<PathBuf> {
    kitab_os::data_dir().map(|d| d.join("trusted-certs"))
}

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

    fn open_styled(
        &self,
        path: &Path,
        password: Option<&str>,
        style: kitab_core::BookStyle,
    ) -> Result<(DocHandle, DocInfo), OpenError> {
        let layout = kitab_core::BookLayout {
            style,
            ..kitab_core::BookLayout::DEFAULT
        };
        let (id, info) = self.engine.open_with(path, password, layout)?;
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
        edits: &[kitab_core::AnnotEdit],
        out: &Path,
    ) -> Result<usize, String> {
        kitab_mupdf::marks::write_marks(src, password, marks, edits, out)
    }

    fn write_pdf(&self, doc: DocHandle, pages: &[u32], out: &Path) -> Result<u32, String> {
        let id = self.id(doc).ok_or("the document is not open")?;
        self.engine.write_pdf(id, pages.to_vec(), out.to_path_buf())
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

    fn signatures(
        &self,
        src: &Path,
        password: Option<&str>,
    ) -> Result<Vec<kitab_core::SignatureCheck>, String> {
        let fields = kitab_mupdf::sigs::signed_fields(src, password)?;
        if fields.is_empty() {
            return Ok(Vec::new());
        }
        let bytes = std::fs::read(src).map_err(|e| e.to_string())?;
        let trust = trust_dir()
            .map(|d| kitab_sign::store::load(&d))
            .unwrap_or_default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Ok(fields
            .iter()
            .map(|f| kitab_sign::verify(&bytes, f, &trust, now))
            .collect())
    }

    fn page_tool(
        &self,
        src: &Path,
        password: Option<&str>,
        op: &kitab_ui::engine::PageOp,
        out: &Path,
    ) -> Result<String, String> {
        use kitab_mupdf::pages as p;
        use kitab_ui::engine::PageOp;
        let n = |count: usize, one: &str, many: &str| {
            if count == 1 {
                format!("1 {one}")
            } else {
                format!("{count} {many}")
            }
        };
        Ok(match op {
            PageOp::Delete(pages) => {
                let left = p::delete_pages(src, password, pages, out)?;
                format!(
                    "Deleted {}; {} left",
                    n(pages.len(), "page", "pages"),
                    n(left as usize, "page", "pages")
                )
            }
            PageOp::Move(pages, before) => {
                p::move_pages(src, password, pages, *before, out)?;
                format!("Moved {}", n(pages.len(), "page", "pages"))
            }
            PageOp::Rotate(pages, deg) => {
                p::rotate_pages(src, password, pages, *deg, out)?;
                format!("Turned {}", n(pages.len(), "page", "pages"))
            }
            PageOp::Compress => {
                let (before, after) = p::compress(src, password, out)?;
                let pct = (after * 100)
                    .checked_div(before)
                    .map_or(0, |share| 100 - share.min(100));
                format!(
                    "{} → {} ({pct}% smaller)",
                    kitab_ui::reader::human_size(before),
                    kitab_ui::reader::human_size(after)
                )
            }
            PageOp::Number(first) => {
                let count = p::number_pages(src, password, *first, out)?;
                format!("Numbered {}", n(count as usize, "page", "pages"))
            }
            PageOp::Watermark(text) => {
                let count = p::watermark(src, password, text, out)?;
                format!("Watermarked {}", n(count as usize, "page", "pages"))
            }
            PageOp::Text => {
                let count = p::extract_text(src, password, out)?;
                format!("Saved the text of {}", n(count as usize, "page", "pages"))
            }
            PageOp::Images => {
                let count = p::extract_images(src, password, out)?;
                if count == 0 {
                    return Err("this PDF has no pictures".into());
                }
                format!("Saved {}", n(count as usize, "picture", "pictures"))
            }
        })
    }

    fn signer_of(&self, key_file: &Path, password: &str) -> Result<(String, String), String> {
        let bytes = std::fs::read(key_file).map_err(|e| e.to_string())?;
        let key = kitab_sign::sign::load_pfx(&bytes, password)?;
        Ok((key.name(), key.subject()))
    }

    fn sign_pdf(
        &self,
        src: &Path,
        req: &kitab_ui::engine::SignRequest,
        out: &Path,
    ) -> Result<(), String> {
        if out == src {
            return Err("choose a different file name than the original".into());
        }
        let bytes = std::fs::read(&req.key_file).map_err(|e| e.to_string())?;
        let key = kitab_sign::sign::load_pfx(&bytes, &req.password)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let stamp = kitab_sign::format_unix(now);
        let digits: String = stamp.chars().filter(char::is_ascii_digit).collect();
        let taken = kitab_mupdf::sigs::signed_fields(src, None)
            .map(|f| f.len())
            .unwrap_or(0);
        let mut lines = vec![format!("Digitally signed by {}", key.name()), stamp.clone()];
        if let Some(r) = &req.reason {
            lines.push(format!("Reason: {r}"));
        }
        let new = kitab_mupdf::sigs::NewSignature {
            field: format!("Signature{}", taken + 1),
            page: req.page,
            rect: req.rect,
            lines,
            name: Some(key.name()),
            reason: req.reason.clone(),
            location: req.location.clone(),
            when: format!("D:{digits}00Z"),
        };
        let mut pdf = kitab_mupdf::sigs::prepare_signature(src, &new)?;
        kitab_sign::sign::fill(
            &mut pdf,
            kitab_mupdf::sigs::RANGE_MARK,
            kitab_mupdf::sigs::SIG_ROOM,
            &key,
            now,
        )?;
        let tmp = out.with_extension("pdf.kitab-tmp");
        std::fs::write(&tmp, &pdf).map_err(|e| format!("could not write: {e}"))?;
        std::fs::rename(&tmp, out).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("could not save: {e}")
        })
    }

    fn trusted_certs(&self) -> Vec<kitab_ui::engine::TrustedCert> {
        trust_dir()
            .map(|d| kitab_sign::store::list(&d))
            .unwrap_or_default()
            .into_iter()
            .map(|t| kitab_ui::engine::TrustedCert {
                subject: t.subject,
                fingerprint: t.fingerprint,
                path: t.path,
            })
            .collect()
    }

    fn trust_cert(&self, file: &Path) -> Result<Vec<String>, String> {
        let dir = trust_dir().ok_or("Kitab has no data folder")?;
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        kitab_sign::store::add(&dir, &bytes)
    }

    fn untrust_cert(&self, path: &Path) -> Result<(), String> {
        let dir = trust_dir().ok_or("Kitab has no data folder")?;
        kitab_sign::store::remove(&dir, path)
    }

    fn set_password(
        &self,
        src: &Path,
        password: Option<&str>,
        new_password: Option<&str>,
        out: &Path,
    ) -> Result<(), String> {
        kitab_mupdf::tools::set_password(src, password, new_password, out)
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

/// The most files one scan of the watched folders looks at, and how deep.
const SCAN_FILES: usize = 20_000;
const SCAN_DEPTH: u32 = 8;

/// Documents Kitab opens under `dir` (not image files, not hidden folders).
fn walk_documents(dir: &Path, depth: u32, out: &mut Vec<(PathBuf, kitab_core::Format)>) {
    if depth > SCAN_DEPTH || out.len() >= SCAN_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let hidden = path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        let Ok(kind) = e.file_type() else { continue };
        if hidden || kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            walk_documents(&path, depth + 1, out);
        } else if let Some(format) = path
            .extension()
            .and_then(|x| kitab_core::Format::from_extension(&x.to_string_lossy()))
            .filter(|f| *f != kitab_core::Format::Image)
        {
            out.push((path, format));
            if out.len() >= SCAN_FILES {
                return;
            }
        }
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

    fn needs_text(&self, path: &Path, mtime: i64) -> bool {
        let path = path.to_path_buf();
        self.call(move |s| {
            let store = s.store.as_ref()?;
            let id = Self::doc_id(store, &path)?;
            Some(store.text_indexed(id).ok()? != Some(mtime))
        })
        .flatten()
        .unwrap_or(false)
    }

    fn index_text(&self, path: &Path, mtime: i64, pages: Vec<String>) {
        let path = path.to_path_buf();
        let _ = self.call(move |s| {
            let store = s.store.as_mut()?;
            let id = Self::doc_id(store, &path)?;
            store.index_text(id, mtime, &pages).ok()
        });
    }

    fn search(&self, query: &str, limit: usize) -> Vec<kitab_ui::LibraryHit> {
        let query = query.to_string();
        self.call(move |s| {
            let store = s.store.as_ref()?;
            let hits = store.search_text(&query, limit).ok()?;
            Some(
                hits.into_iter()
                    .filter_map(|h| {
                        let doc = store.document(h.document_id).ok()??;
                        Some(kitab_ui::LibraryHit {
                            title: title_or_name(doc.title.clone(), &doc.path),
                            path: doc.path,
                            format: doc.format,
                            page: h.page,
                            snippet: h.snippet,
                        })
                    })
                    .collect(),
            )
        })
        .flatten()
        .unwrap_or_default()
    }

    fn add_reading(&self, path: &Path, day: &str, seconds: i64) {
        let (path, day) = (path.to_path_buf(), day.to_string());
        let _ = self.call(move |s| {
            let store = s.store.as_mut()?;
            let id = Self::doc_id(store, &path)?;
            store.add_reading(id, &day, seconds).ok()
        });
    }

    fn reading_stats(&self, today: &str) -> Option<kitab_ui::StatsView> {
        let today = today.to_string();
        self.call(move |s| {
            let store = s.store.as_ref()?;
            let st = store.reading_stats(&today, 28).ok()?;
            let top = st
                .top
                .iter()
                .filter_map(|(id, secs)| {
                    let doc = store.document(*id).ok()??;
                    Some((title_or_name(doc.title.clone(), &doc.path), doc.path, *secs))
                })
                .collect();
            Some(kitab_ui::StatsView {
                today_seconds: st.today_seconds,
                week_seconds: st.week_seconds,
                streak_days: st.streak_days,
                longest_streak: st.longest_streak,
                recent_days: st.recent_days,
                top,
                indexed: store.indexed_count().unwrap_or(0),
            })
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

    fn shelf(&self, kind: kitab_ui::ShelfKind) -> Vec<RecentItem> {
        use kitab_store::Shelf;
        use kitab_ui::ShelfKind as K;
        let shelf = match kind {
            K::Recent => return self.recent(kitab_store::MAX_SHELF),
            K::All => Shelf::All,
            K::Favourites => Shelf::Favourites,
            K::InProgress => Shelf::InProgress,
            K::Finished => Shelf::Finished,
            K::Collection(id) => Shelf::Collection(kitab_store::CollectionId(id)),
        };
        self.call(move |s| {
            let store = s.store.as_ref()?;
            Some(
                store
                    .shelf(shelf)
                    .ok()?
                    .into_iter()
                    .map(|e| RecentItem {
                        title: title_or_name(e.title, &e.path),
                        missing: e.missing || !e.path.exists(),
                        path: e.path,
                        author: e.author,
                        format: e.format,
                        progress: e.progress.unwrap_or(0.0),
                        last_opened_ms: e.last_opened_at,
                    })
                    .collect(),
            )
        })
        .flatten()
        .unwrap_or_default()
    }

    fn collections(&self) -> Vec<kitab_ui::CollectionInfo> {
        self.call(|s| {
            let store = s.store.as_ref()?;
            Some(
                store
                    .collections()
                    .ok()?
                    .into_iter()
                    .map(|c| kitab_ui::CollectionInfo {
                        id: c.id.0,
                        name: c.name,
                        count: c.count,
                    })
                    .collect(),
            )
        })
        .flatten()
        .unwrap_or_default()
    }

    fn create_collection(&self, name: &str) -> Option<i64> {
        let name = name.to_string();
        self.call(move |s| s.store.as_mut()?.create_collection(&name).ok().map(|c| c.0))
            .flatten()
    }

    fn rename_collection(&self, id: i64, name: &str) {
        let name = name.to_string();
        let _ = self.call(move |s| {
            s.store
                .as_mut()?
                .rename_collection(kitab_store::CollectionId(id), &name)
                .ok()
        });
    }

    fn delete_collection(&self, id: i64) {
        let _ = self.call(move |s| {
            s.store
                .as_mut()?
                .delete_collection(kitab_store::CollectionId(id))
                .ok()
        });
    }

    fn set_in_collection(&self, id: i64, path: &Path, inside: bool) {
        let path = path.to_path_buf();
        let _ = self.call(move |s| {
            let store = s.store.as_mut()?;
            let doc = Self::doc_id(store, &path)?;
            store
                .set_in_collection(kitab_store::CollectionId(id), doc, inside)
                .ok()
        });
    }

    fn doc_flags(&self, path: &Path) -> Option<(bool, Vec<i64>)> {
        let path = path.to_path_buf();
        self.call(move |s| {
            let store = s.store.as_ref()?;
            let doc = Self::doc_id(store, &path)?;
            let fav = store.is_favourite(doc).ok()?;
            let cs = store
                .collections_of(doc)
                .ok()?
                .into_iter()
                .map(|c| c.0)
                .collect();
            Some((fav, cs))
        })
        .flatten()
    }

    fn set_favourite(&self, path: &Path, favourite: bool) {
        let path = path.to_path_buf();
        let _ = self.call(move |s| {
            let store = s.store.as_mut()?;
            let doc = Self::doc_id(store, &path)?;
            store.set_favourite(doc, favourite).ok()
        });
    }

    fn set_finished(&self, path: &Path, finished: bool) {
        let path = path.to_path_buf();
        let _ = self.call(move |s| {
            let store = s.store.as_mut()?;
            let doc = Self::doc_id(store, &path)?;
            store.set_finished(doc, finished).ok()
        });
    }

    fn watched_folders(&self) -> Vec<PathBuf> {
        self.call(|s| s.store.as_ref()?.watched_folders().ok())
            .flatten()
            .unwrap_or_default()
    }

    fn watch_folder(&self, path: &Path) {
        let path = path.to_path_buf();
        let _ = self.call(move |s| s.store.as_mut()?.watch_folder(&path).ok());
    }

    fn unwatch_folder(&self, path: &Path) {
        let path = path.to_path_buf();
        let _ = self.call(move |s| s.store.as_mut()?.unwatch_folder(&path).ok());
    }

    fn scan_watched(&self) -> u32 {
        let folders = self.watched_folders();
        // Walk off the io thread; only the new files go to the store.
        let mut found = Vec::new();
        for f in folders {
            walk_documents(&f, 0, &mut found);
        }
        // Which are new (on the io thread, a lookup each), then their
        // identity (reading the files, here), then one write.
        let unknown: Vec<(PathBuf, kitab_core::Format)> = self
            .call(move |s| {
                let store = s.store.as_ref()?;
                Some(
                    found
                        .into_iter()
                        .filter(|(p, _)| store.document_by_path(p).ok().flatten().is_none())
                        .collect(),
                )
            })
            .flatten()
            .unwrap_or_default();
        let new: Vec<_> = unknown
            .into_iter()
            .filter_map(|(p, f)| kitab_store::identify(&p).ok().map(|i| (p, f, i)))
            .collect();
        self.call(move |s| {
            let store = s.store.as_mut()?;
            let mut added = 0u32;
            for (path, format, ident) in new {
                let mut meta = kitab_store::DocMeta::new(format);
                meta.title = path.file_stem().map(|s| s.to_string_lossy().to_string());
                if store.upsert_document(&ident, &path, &meta).is_ok() {
                    added += 1;
                }
            }
            Some(added)
        })
        .flatten()
        .unwrap_or(0)
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
