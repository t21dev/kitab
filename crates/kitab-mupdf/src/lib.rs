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
//! document belongs to the worker that opened it**: text, links, find and
//! everything but rendering run there, in order. Documents are spread over
//! workers (the one with the fewest open documents wins). Rendering also uses
//! up to three helper workers, each opening its own handle on the same file
//! the first time it renders a page, and each render goes to the least busy
//! of them, so the pages of one long scanned book decode side by side.
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
pub mod epub;
mod ffi;
mod find;
pub mod fonts;
mod forms;
pub mod marks;
pub mod pages;
pub mod sigs;
pub mod tools;

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use kitab_core::{
    Bitmap, BookLayout, DocInfo, FindHit, FindOptions, Link, OpenError, RenderRequest,
};

pub use doc::{MAX_PAGES, MAX_PIXELS, MAX_SIDE_PX, REFLOW_EM, REFLOW_H, REFLOW_W, ink_ratio};

/// MuPDF's resource store limit (`FZ_STORE_DEFAULT`), shared by all workers.
pub const STORE_LIMIT_BYTES: usize = 256 << 20;

/// What MuPDF's store is trimmed back to after each render, as a share of
/// its limit: 48 MiB of fonts, images and decoded objects. Plenty for the
/// pages around the reader; its 256 MiB default made a long read grow by
/// tens of megabytes that were never needed again.
pub const STORE_KEEP_PERCENT: u32 = 19;

/// Stack size of a worker thread. MuPDF recurses on deeply nested content.
const WORKER_STACK_BYTES: usize = 8 << 20;

/// A job that panics this many times closes its document.
const CRASHES_BEFORE_QUARANTINE: u32 = 2;

/// Workers besides its own that may render a document's pages.
const RENDER_HELPERS: usize = 3;

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
    /// Extra workers that may render pages of this document, each opening its
    /// own handle on the file the first time (renders of one long scanned
    /// book run side by side instead of queueing on one thread).
    helpers: Vec<usize>,
    /// To open the file again on a helper. The password stays in memory only
    /// and is dropped with the document (AGENTS.md rule 13).
    path: PathBuf,
    password: Option<String>,
    page_count: u32,
    /// The layout a book is at; helpers open it the same way.
    layout: BookLayout,
    /// The newest generation the caller set; older render requests are dropped.
    generation: Arc<AtomicU64>,
    /// Form fields filled in since it opened, in order: a render helper
    /// that opens the file later fills them in too, so every page it draws
    /// shows them (they reach the file itself on Save).
    form: Vec<kitab_core::FieldEdit>,
}

/// The document engine: a pool of MuPDF workers. Cheap to create; threads
/// start on first use. `Send + Sync`, so share it behind an `Arc`.
pub struct Engine {
    workers: Mutex<Vec<Option<Sender<Job>>>>,
    /// Render jobs queued or running per worker, to send the next one to the
    /// least busy worker of a document.
    inflight: Vec<AtomicU64>,
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
        // Books in non-Latin scripts need the system's fonts (fonts.rs).
        fonts::install();
        Engine {
            workers: Mutex::new(vec![None; threads]),
            inflight: (0..threads).map(|_| AtomicU64::new(0)).collect(),
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
        self.open_with(path, password, BookLayout::DEFAULT)
    }

    /// [`Engine::open`], laying a book out at `layout` (ignored for files
    /// with their own pages).
    pub fn open_with(
        &self,
        path: &Path,
        password: Option<&str>,
        layout: BookLayout,
    ) -> Result<(DocId, DocInfo), OpenError> {
        let id = DocId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let worker = self.pick_worker();
        let path: PathBuf = path.to_path_buf();
        let password = password.map(str::to_string);
        let (keep_path, keep_password) = (path.clone(), password.clone());
        let opened = self
            .call(worker, None, move |state| {
                let (doc, info) = doc::open(&path, password.as_deref(), layout)?;
                state.docs.insert(id, DocSlot { doc, crashes: 0 });
                Ok(info)
            })
            .map_err(OpenError::Failed)?;
        let info: DocInfo = opened?;
        lock(&self.docs).insert(
            id,
            DocEntry {
                worker,
                helpers: self.helpers_for(worker),
                path: keep_path,
                password: keep_password,
                page_count: info.page_count,
                layout,
                generation: Arc::new(AtomicU64::new(0)),
                form: Vec::new(),
            },
        );
        Ok((id, info))
    }

    /// Up to [`RENDER_HELPERS`] other workers, starting after `worker`.
    fn helpers_for(&self, worker: usize) -> Vec<usize> {
        let threads = self.threads();
        (1..threads)
            .map(|i| (worker + i) % threads)
            .take(RENDER_HELPERS)
            .collect()
    }

    /// The least busy of `doc`'s workers for a render, with what a helper
    /// needs to open the file.
    #[allow(clippy::type_complexity)]
    fn render_worker(
        &self,
        doc: DocId,
    ) -> Result<
        (
            usize,
            bool,
            PathBuf,
            Option<String>,
            BookLayout,
            Vec<kitab_core::FieldEdit>,
        ),
        String,
    > {
        let docs = lock(&self.docs);
        let e = docs
            .get(&doc)
            .ok_or_else(|| "the document is not open".to_string())?;
        let load = |w: usize| {
            self.inflight
                .get(w)
                .map_or(u64::MAX, |n| n.load(Ordering::Relaxed))
        };
        let mut best = e.worker;
        for &h in &e.helpers {
            if load(h) < load(best) {
                best = h;
            }
        }
        let helper = best != e.worker;
        Ok((
            best,
            helper,
            e.path.clone(),
            e.password.clone(),
            e.layout,
            // Only a helper might open the file afresh and need them.
            if helper { e.form.clone() } else { Vec::new() },
        ))
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
        self.render_when(doc, req, || true)
    }

    /// [`Engine::render`], also dropped (`Ok(None)`) when `wanted` says no at
    /// those same three points: a page scrolled away while its job waited.
    pub fn render_when(
        &self,
        doc: DocId,
        req: RenderRequest,
        wanted: impl Fn() -> bool + Send + Clone + 'static,
    ) -> Result<Option<Bitmap>, String> {
        let (_, _, generation) = self.entry(doc)?;
        let stale = {
            let generation = generation.clone();
            move || req.generation < generation.load(Ordering::Relaxed) || !wanted()
        };
        if stale() {
            return Ok(None);
        }
        let (worker, helper, path, password, layout, form) = self.render_worker(doc)?;
        let stale_on_worker = stale.clone();
        let counter = &self.inflight[worker];
        counter.fetch_add(1, Ordering::Relaxed);
        let result = self.call(worker, Some(doc), move |state| {
            if stale_on_worker() {
                return Ok::<_, String>(None);
            }
            if helper && !state.docs.contains_key(&doc) {
                let (mut opened, _) = doc::open(&path, password.as_deref(), layout)
                    .map_err(|e| format!("a render helper could not open the file: {e:?}"))?;
                // The form as the reader has filled it in so far.
                let pages = opened.page_count;
                if let doc::Handle::Pdf(pdf) = &mut opened.pdf {
                    for e in &form {
                        let _ = forms::apply(pdf, pages, e);
                    }
                }
                state.docs.insert(
                    doc,
                    DocSlot {
                        doc: opened,
                        crashes: 0,
                    },
                );
            }
            let slot = state
                .docs
                .get(&doc)
                .ok_or_else(|| "the document was closed".to_string())?;
            let bitmap = doc::render(&slot.doc, &req)?;
            // Keep MuPDF's shared store near STORE_KEEP_BYTES: least
            // recently used fonts and images go first, and only those no
            // render holds. Nearly free when it's already under.
            ffi::shrink_store(STORE_KEEP_PERCENT);
            Ok((!stale_on_worker()).then_some(bitmap))
        });
        counter.fetch_sub(1, Ordering::Relaxed);
        let bitmap = result??;
        Ok(bitmap.filter(|_| !stale()))
    }

    /// Lay a book out again at `layout` and describe it anew (page count,
    /// sizes and contents all change). Its render helpers drop their handles
    /// and open the book again, at the new layout, on their next render.
    /// Set a new generation afterwards so pages drawn at the old layout are
    /// dropped.
    pub fn relayout(&self, doc: DocId, layout: BookLayout) -> Result<DocInfo, String> {
        let (worker, helpers, path, password, old) = {
            let docs = lock(&self.docs);
            let e = docs
                .get(&doc)
                .ok_or_else(|| "the document is not open".to_string())?;
            (
                e.worker,
                e.helpers.clone(),
                e.path.clone(),
                e.password.clone(),
                e.layout,
            )
        };
        // A book keeps the styles it was opened with: new typography means
        // opening it again; a new size or text size is a layout.
        let restyle = layout.style != old.style;
        let info = self.call(worker, Some(doc), move |state| {
            let slot = state
                .docs
                .get_mut(&doc)
                .ok_or_else(|| "the document was closed".to_string())?;
            if restyle {
                if !slot.doc.reflowable || !layout.is_valid() {
                    return Err("only a book can be restyled, at a valid layout".to_string());
                }
                let (fresh, info) =
                    doc::open(&path, password.as_deref(), layout).map_err(|e| format!("{e:?}"))?;
                slot.doc = fresh;
                Ok(info)
            } else {
                doc::relayout(&mut slot.doc, layout)
            }
        })??;
        if let Some(e) = lock(&self.docs).get_mut(&doc) {
            e.page_count = info.page_count;
            e.layout = layout;
        }
        // Queued behind any render already sent to them, ahead of any later one.
        for h in helpers {
            let _ = self.call(h, None, move |state| {
                state.docs.remove(&doc);
            });
        }
        Ok(info)
    }

    /// Write `pages` of an open document to a new PDF (vector pages, as laid
    /// out now): what Kitab sends to CUPS on macOS and Linux.
    pub fn write_pdf(&self, doc: DocId, pages: Vec<u32>, out: PathBuf) -> Result<u32, String> {
        self.with_doc(doc, move |d| doc::write_pdf(d, &pages, &out))
    }

    /// The annotations already in a PDF (for the Marks panel); empty for other formats.
    pub fn annotations(&self, doc: DocId) -> Result<Vec<kitab_core::FileAnnot>, String> {
        self.with_doc(doc, doc::annotations)
    }

    /// The form fields on a page (0-based), in file order; none for other formats.
    pub fn form_fields(&self, doc: DocId, page: u32) -> Result<Vec<kitab_core::FormField>, String> {
        self.with_doc(doc, move |d| match &d.pdf {
            doc::Handle::Pdf(pdf) => forms::fields(pdf, page),
            doc::Handle::Other(_) => Ok(Vec::new()),
        })
    }

    /// Fill in a form field in the open document (not the file: Save writes
    /// it). Returns the pages whose look changed; set a new generation and
    /// render them again. Render helpers fill it in too, in order, before
    /// any page they draw after this.
    pub fn set_field(&self, doc: DocId, edit: kitab_core::FieldEdit) -> Result<Vec<u32>, String> {
        let (worker, _, _) = self.entry(doc)?;
        let mine = edit.clone();
        let changed = self.call(worker, Some(doc), move |state| {
            let slot = state
                .docs
                .get_mut(&doc)
                .ok_or_else(|| "the document was closed".to_string())?;
            let pages = slot.doc.page_count;
            match &mut slot.doc.pdf {
                doc::Handle::Pdf(pdf) => {
                    if !doc::can_fill_forms(pdf) {
                        return Err("this PDF doesn't allow filling in its form".to_string());
                    }
                    forms::apply(pdf, pages, &mine)
                }
                doc::Handle::Other(_) => Err("only a PDF has form fields".to_string()),
            }
        })??;
        let helpers = {
            let mut docs = lock(&self.docs);
            let Some(e) = docs.get_mut(&doc) else {
                return Ok(changed);
            };
            e.form.push(edit.clone());
            e.helpers.clone()
        };
        let started: Vec<bool> = lock(&self.workers).iter().map(Option::is_some).collect();
        for h in helpers {
            if !started.get(h).copied().unwrap_or(false) {
                continue;
            }
            let edit = edit.clone();
            if let Ok(tx) = self.worker(h) {
                let _ = tx.send(Job {
                    doc: Some(doc),
                    run: Box::new(move |state| {
                        if let Some(slot) = state.docs.get_mut(&doc) {
                            let pages = slot.doc.page_count;
                            if let doc::Handle::Pdf(pdf) = &mut slot.doc.pdf {
                                let _ = forms::apply(pdf, pages, &edit);
                            }
                        }
                    }),
                });
            }
        }
        Ok(changed)
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
        let started: Vec<bool> = lock(&self.workers).iter().map(Option::is_some).collect();
        for w in std::iter::once(entry.worker).chain(entry.helpers.iter().copied()) {
            if w != entry.worker && !started.get(w).copied().unwrap_or(false) {
                continue; // a helper that never started has nothing open
            }
            if let Ok(tx) = self.worker(w) {
                let _ = tx.send(Job {
                    doc: None,
                    run: Box::new(move |state| {
                        state.docs.remove(&doc);
                    }),
                });
            }
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
