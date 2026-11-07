// SPDX-License-Identifier: AGPL-3.0-or-later

//! Kitab's document engine: MuPDF behind a pool of worker threads
//! (PLAN.md §3.1).
//!
//! # Threading
//!
//! MuPDF only ever runs on the pool's threads, never on the caller's. Each
//! worker has its own `fz_context`, cloned from one base context by the
//! `mupdf` crate on the thread's first MuPDF call. Workers start lazily, one
//! at a time, when a document is first assigned to them, so creating an
//! [`Engine`] costs nothing on the start-up path (AGENTS.md rule 1).
//!
//! The `mupdf` crate's `Document` is neither `Send` nor `Sync`, so **each
//! document is pinned to the worker that opened it**: every job for that
//! document runs there, in order. Documents are spread over workers (the one
//! with the fewest open documents wins), so several documents render in
//! parallel, while the pages of one document render one after another.
//!
//! The public methods block until their worker answers. Call them from a
//! background task (`cx.background_spawn`), never from the UI thread.
//!
//! # Memory
//!
//! MuPDF caches fonts, images and decoded objects in one store shared by every
//! context. The `mupdf` crate creates the base context with
//! `FZ_STORE_DEFAULT`, so the store is bounded at [`STORE_LIMIT_BYTES`]
//! (256 MiB) and cannot be resized through the crate. [`Engine::shrink_caches`]
//! evicts it further under memory pressure. The engine keeps no caches of its
//! own; rendered bitmaps belong to the caller's tile cache.
//!
//! # Safety
//!
//! Every document is treated as hostile (AGENTS.md rule 8): MuPDF errors become
//! values, panics in a job are caught on the worker and reported as errors, and
//! a document whose jobs panic twice is closed (quarantined).

mod doc;
mod ffi;
mod find;
pub mod tools;

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use kitab_core::{Bitmap, DocInfo, FindHit, FindOptions, Link, OpenError, RenderRequest};

pub use doc::{MAX_PAGES, MAX_PIXELS, MAX_SIDE_PX, ink_ratio};

/// MuPDF's resource store limit (`FZ_STORE_DEFAULT`), shared by all workers.
pub const STORE_LIMIT_BYTES: usize = 256 << 20;

/// Stack size of a worker thread. MuPDF recurses on deeply nested content.
const WORKER_STACK_BYTES: usize = 8 << 20;

/// A job that panics this many times closes its document.
const CRASHES_BEFORE_QUARANTINE: u32 = 2;

/// An open document. Ids are never reused within one [`Engine`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocId(u64);

/// One document as its worker holds it.
struct DocSlot {
    doc: doc::OpenDoc,
    crashes: u32,
}

/// State owned by one worker thread.
#[derive(Default)]
struct WorkerState {
    docs: HashMap<DocId, DocSlot>,
}

struct Job {
    /// The document the job works on, for quarantine after panics.
    doc: Option<DocId>,
    run: Box<dyn FnOnce(&mut WorkerState) + Send>,
}

/// What the engine knows about an open document on the caller's side.
struct DocEntry {
    worker: usize,
    page_count: u32,
    /// The newest generation the caller set; older render requests are dropped.
    generation: Arc<AtomicU64>,
}

/// The document engine: a pool of MuPDF workers. Cheap to create; threads
/// start on first use. `Send + Sync`, so share it behind an `Arc`.
pub struct Engine {
    workers: Mutex<Vec<Option<Sender<Job>>>>,
    docs: Mutex<HashMap<DocId, DocEntry>>,
    next_id: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // The maps stay consistent across a panic (single inserts and removals).
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn worker_loop(rx: Receiver<Job>) {
    let mut state = WorkerState::default();
    while let Ok(job) = rx.recv() {
        let Job { doc, run } = job;
        let outcome = catch_unwind(AssertUnwindSafe(|| run(&mut state)));
        if outcome.is_err()
            && let Some(id) = doc
            && let Some(slot) = state.docs.get_mut(&id)
        {
            slot.crashes += 1;
            if slot.crashes >= CRASHES_BEFORE_QUARANTINE {
                state.docs.remove(&id);
            }
        }
    }
}

impl Engine {
    /// A pool of up to `threads` workers (at least one). Nothing starts yet.
    /// PLAN.md §3.1 suggests [`Engine::default_threads`].
    pub fn new(threads: usize) -> Engine {
        let threads = threads.max(1);
        Engine {
            workers: Mutex::new(vec![None; threads]),
            docs: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Physical-ish cores minus one for the UI thread, at least one.
    pub fn default_threads() -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1))
            .unwrap_or(1)
            .max(1)
    }

    /// The most workers this engine will start.
    pub fn threads(&self) -> usize {
        lock(&self.workers).len()
    }

    /// How many workers have started so far.
    pub fn started_threads(&self) -> usize {
        lock(&self.workers).iter().filter(|w| w.is_some()).count()
    }

    /// The sender for worker `index`, starting its thread if needed.
    fn worker(&self, index: usize) -> Result<Sender<Job>, String> {
        let mut workers = lock(&self.workers);
        let slot = workers
            .get_mut(index)
            .ok_or_else(|| format!("no worker {index}"))?;
        if let Some(tx) = slot {
            return Ok(tx.clone());
        }
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name(format!("kitab-render-{index}"))
            .stack_size(WORKER_STACK_BYTES)
            .spawn(move || worker_loop(rx))
            .map_err(|e| format!("could not start a render worker: {e}"))?;
        *slot = Some(tx.clone());
        Ok(tx)
    }

    /// Run `f` on worker `index` and wait for its answer.
    fn call<R: Send + 'static>(
        &self,
        index: usize,
        doc: Option<DocId>,
        f: impl FnOnce(&mut WorkerState) -> R + Send + 'static,
    ) -> Result<R, String> {
        let (tx, rx) = mpsc::sync_channel(1);
        let run = Box::new(move |state: &mut WorkerState| {
            // The caller may have given up; nothing to do then.
            let _ = tx.send(f(state));
        });
        self.worker(index)?
            .send(Job { doc, run })
            .map_err(|_| "the render worker stopped".to_string())?;
        rx.recv()
            .map_err(|_| "the engine failed on this document (a panic was caught)".to_string())
    }

    /// The worker with the fewest open documents (lowest index on ties).
    fn pick_worker(&self) -> usize {
        let threads = self.threads();
        let docs = lock(&self.docs);
        let mut load = vec![0usize; threads];
        for entry in docs.values() {
            if let Some(n) = load.get_mut(entry.worker) {
                *n += 1;
            }
        }
        load.iter()
            .enumerate()
            .min_by_key(|&(i, &n)| (n, i))
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn entry(&self, doc: DocId) -> Result<(usize, u32, Arc<AtomicU64>), String> {
        lock(&self.docs)
            .get(&doc)
            .map(|e| (e.worker, e.page_count, e.generation.clone()))
            .ok_or_else(|| "the document is not open".to_string())
    }

    /// Run `f` with the open document on its worker.
    fn with_doc<R: Send + 'static>(
        &self,
        doc: DocId,
        f: impl FnOnce(&doc::OpenDoc) -> Result<R, String> + Send + 'static,
    ) -> Result<R, String> {
        let (worker, _, _) = self.entry(doc)?;
        self.call(worker, Some(doc), move |state| {
            let slot = state
                .docs
                .get(&doc)
                .ok_or_else(|| "the document was closed".to_string())?;
            f(&slot.doc)
        })?
    }

    /// Open a document and describe it: format, metadata, every page's own
    /// size, outline, permissions, encryption and whether it was repaired.
    ///
    /// Blocks until a worker has opened the file; MuPDF never runs on the
    /// calling thread. For an encrypted file, `None` yields
    /// [`OpenError::NeedsPassword`] and a wrong password
    /// [`OpenError::WrongPassword`]. The password is not kept.
    pub fn open(&self, path: &Path, password: Option<&str>) -> Result<(DocId, DocInfo), OpenError> {
        let id = DocId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let worker = self.pick_worker();
        let path: PathBuf = path.to_path_buf();
        let password = password.map(str::to_string);
        let opened = self
            .call(worker, None, move |state| {
                let (doc, info) = doc::open(&path, password.as_deref())?;
                state.docs.insert(id, DocSlot { doc, crashes: 0 });
                Ok(info)
            })
            .map_err(OpenError::Failed)?;
        let info: DocInfo = opened?;
        lock(&self.docs).insert(
            id,
            DocEntry {
                worker,
                page_count: info.page_count,
                generation: Arc::new(AtomicU64::new(0)),
            },
        );
        Ok((id, info))
    }

    /// Mark every render request for `doc` older than `generation` as stale.
    /// Generations only move forward; a smaller value is ignored.
    pub fn set_generation(&self, doc: DocId, generation: u64) {
        if let Ok((_, _, g)) = self.entry(doc) {
            g.fetch_max(generation, Ordering::Relaxed);
        }
    }

    /// Render a page, or a tile of it, at `req.scale` device pixels per point
    /// with `req.rotation` degrees on top of the page's own /Rotate.
    ///
    /// Returns BGRA, top-down, opaque, on white paper. Returns `Ok(None)` when
    /// `req.generation` is older than the document's generation, checked when
    /// the job is queued, when it starts and when it finishes.
    pub fn render(&self, doc: DocId, req: RenderRequest) -> Result<Option<Bitmap>, String> {
        let (worker, _, generation) = self.entry(doc)?;
        let stale = {
            let generation = generation.clone();
            move || req.generation < generation.load(Ordering::Relaxed)
        };
        if stale() {
            return Ok(None);
        }
        let stale_on_worker = stale.clone();
        let bitmap = self.call(worker, Some(doc), move |state| {
            if stale_on_worker() {
                return Ok::<_, String>(None);
            }
            let slot = state
                .docs
                .get(&doc)
                .ok_or_else(|| "the document was closed".to_string())?;
            let bitmap = doc::render(&slot.doc, &req)?;
            Ok((!stale_on_worker()).then_some(bitmap))
        })??;
        Ok(bitmap.filter(|_| !stale()))
    }

    /// The links on a page (0-based), with rectangles in page points.
    pub fn links(&self, doc: DocId, page: u32) -> Result<Vec<Link>, String> {
        self.with_doc(doc, move |d| doc::links(d, page))
    }

    /// Whether the page has a text layer. `false` means "this page is an
    /// image; no text to search" (PLAN.md §3.5).
    pub fn page_has_text(&self, doc: DocId, page: u32) -> Result<bool, String> {
        self.with_doc(doc, move |d| Ok(doc::page_text(d, page)?.has_text()))
    }

    /// A page's text in reading order, one line per line of text (Copy page text).
    pub fn page_plain_text(&self, doc: DocId, page: u32) -> Result<String, String> {
        self.with_doc(doc, move |d| {
            Ok(doc::page_text(d, page)?.lines.join(
                "
",
            ))
        })
    }

    /// A page's characters in reading order with their boxes (text selection).
    pub fn page_chars(&self, doc: DocId, page: u32) -> Result<Vec<kitab_core::TextChar>, String> {
        self.with_doc(doc, move |d| {
            Ok(doc::page_text(d, page)?
                .chars
                .into_iter()
                .map(|c| kitab_core::TextChar {
                    ch: c.ch,
                    rect: c.rect,
                    line: c.line as u32,
                })
                .collect())
        })
    }

    /// Every hit of `query` on one page, in reading order.
    pub fn find_page(
        &self,
        doc: DocId,
        page: u32,
        query: &str,
        options: FindOptions,
    ) -> Result<Vec<FindHit>, String> {
        let query = query.to_string();
        self.with_doc(doc, move |d| {
            let text = doc::page_text(d, page)?;
            Ok(find::find_in_page(&text, &query, options)
                .into_iter()
                .map(|hit| FindHit {
                    page,
                    quads: hit.rects,
                    context: text.lines.get(hit.line).cloned().unwrap_or_default(),
                })
                .collect())
        })
    }

    /// Search the whole document page by page, calling `on_hit` for each hit
    /// in order. `on_hit` returns `false` to stop. A page that fails to load
    /// is skipped. Returns the number of hits delivered.
    pub fn find(
        &self,
        doc: DocId,
        query: &str,
        options: FindOptions,
        mut on_hit: impl FnMut(FindHit) -> bool,
    ) -> Result<usize, String> {
        let (_, page_count, _) = self.entry(doc)?;
        let mut delivered = 0;
        for page in 0..page_count {
            let hits = match self.find_page(doc, page, query, options) {
                Ok(hits) => hits,
                Err(_) => {
                    // Stop if the document went away; skip a damaged page.
                    self.entry(doc)?;
                    continue;
                }
            };
            for hit in hits {
                delivered += 1;
                if !on_hit(hit) {
                    return Ok(delivered);
                }
            }
        }
        Ok(delivered)
    }

    /// Close a document and free its MuPDF objects. Pending jobs for it fail
    /// with "the document was closed". Returns immediately.
    pub fn close(&self, doc: DocId) {
        let Some(entry) = lock(&self.docs).remove(&doc) else {
            return;
        };
        if let Ok(tx) = self.worker(entry.worker) {
            let _ = tx.send(Job {
                doc: None,
                run: Box::new(move |state| {
                    state.docs.remove(&doc);
                }),
            });
        }
    }

    /// Evict MuPDF's shared store down to `percent` % of its limit (0 empties
    /// everything not in use). Call it on memory pressure or when tabs go
    /// inactive. Returns whether the store was reached; with no worker started
    /// there is nothing to shrink and it returns `true`.
    pub fn shrink_caches(&self, percent: u32) -> bool {
        let first = lock(&self.workers).iter().position(Option::is_some);
        match first {
            None => true,
            Some(index) => self
                .call(index, None, move |_| ffi::shrink_store(percent))
                .unwrap_or(false),
        }
    }

    /// How many documents are open.
    pub fn open_documents(&self) -> usize {
        lock(&self.docs).len()
    }
}
