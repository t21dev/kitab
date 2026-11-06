// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interim MuPDF engine for the app shell, until `kitab-mupdf` (built in
//! parallel) replaces it. One worker thread per open document owns the MuPDF
//! document; the UI talks to it through channels, never touching MuPDF itself.

use kitab_core::{
    Bitmap, DocInfo, FindHit, FindOptions, Format, Link, OpenError, OutlineItem, Permissions,
    RenderRequest, SizePt,
};
use kitab_ui::{DocEngine, DocHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

enum Job {
    Render(RenderRequest, Sender<Result<Option<Bitmap>, String>>),
    Close,
}

struct DocWorker {
    jobs: Sender<Job>,
    generation: Arc<AtomicU64>,
}

#[derive(Default)]
pub struct InterimEngine {
    docs: Mutex<HashMap<DocHandle, DocWorker>>,
    next: AtomicU64,
}

impl InterimEngine {
    pub fn new() -> Self {
        Self::default()
    }
}

fn to_outline(items: Vec<mupdf::Outline>) -> Vec<OutlineItem> {
    items
        .into_iter()
        .map(|o| OutlineItem {
            title: o.title,
            page: o.dest.map(|d| d.loc.page_number),
            uri: o.uri,
            open: false,
            children: to_outline(o.down),
        })
        .collect()
}

fn open_doc(path: &Path, password: Option<&str>) -> Result<(mupdf::Document, DocInfo), OpenError> {
    if !path.exists() {
        return Err(OpenError::NotFound);
    }
    let Some(p) = path.to_str() else {
        return Err(OpenError::Failed("path is not UTF-8".into()));
    };
    let mut doc = mupdf::Document::open(p).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("no objects found")
            || msg.contains("cannot recognize")
            || msg.contains("unknown")
        {
            OpenError::Unsupported
        } else {
            OpenError::Failed(msg)
        }
    })?;
    if doc.needs_password().unwrap_or(false) {
        match password {
            None => return Err(OpenError::NeedsPassword),
            Some(pw) => {
                if !doc.authenticate(pw).unwrap_or(false) {
                    return Err(OpenError::WrongPassword);
                }
            }
        }
    }
    let count = doc
        .page_count()
        .map_err(|e| OpenError::Failed(e.to_string()))?
        .max(0) as u32;
    let mut sizes = Vec::with_capacity(count as usize);
    for i in 0..count as i32 {
        let size = doc
            .load_page(i)
            .and_then(|pg| pg.bounds())
            .map(|b| SizePt {
                w: b.x1 - b.x0,
                h: b.y1 - b.y0,
            })
            .unwrap_or(SizePt { w: 612.0, h: 792.0 });
        sizes.push(size);
    }
    let meta = |n| {
        doc.metadata(n)
            .ok()
            .filter(|s: &String| !s.trim().is_empty())
    };
    let info = DocInfo {
        format: Format::Pdf,
        title: meta(mupdf::MetadataName::Title),
        author: meta(mupdf::MetadataName::Author),
        page_count: count,
        page_sizes: sizes,
        outline: doc.outlines().map(to_outline).unwrap_or_default(),
        permissions: Permissions::default(),
        encryption: meta(mupdf::MetadataName::Encryption),
        repaired: false,
    };
    Ok((doc, info))
}

/// Render at `scale` to BGRA on white paper (MuPDF's alpha is premultiplied).
fn render(doc: &mupdf::Document, req: &RenderRequest) -> Result<Bitmap, String> {
    let page = doc.load_page(req.page as i32).map_err(|e| e.to_string())?;
    let matrix = mupdf::Matrix::new_scale(req.scale, req.scale);
    let mut matrix = matrix;
    if req.rotation != 0 {
        matrix.concat(mupdf::Matrix::new_rotate(f32::from(req.rotation)));
    }
    let pixmap = page
        .to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), true, false)
        .map_err(|e| e.to_string())?;
    let (w, h, n) = (pixmap.width(), pixmap.height(), pixmap.n() as usize);
    if n != 4 {
        return Err(format!("expected RGBA pixmap, got {n} channels"));
    }
    let stride = pixmap.stride() as usize;
    let samples = pixmap.samples();
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h as usize {
        let line = samples
            .get(row * stride..row * stride + w as usize * 4)
            .ok_or("pixmap shorter than its size")?;
        for px in line.chunks_exact(4) {
            let paper = 255 - px[3];
            bgra.extend_from_slice(&[
                px[2].saturating_add(paper),
                px[1].saturating_add(paper),
                px[0].saturating_add(paper),
                255,
            ]);
        }
    }
    Ok(Bitmap {
        width: w,
        height: h,
        bgra,
    })
}

impl DocEngine for InterimEngine {
    fn open(&self, path: &Path, password: Option<&str>) -> Result<(DocHandle, DocInfo), OpenError> {
        let (reply_tx, reply_rx) = channel::<Result<DocInfo, OpenError>>();
        let (jobs_tx, jobs_rx) = channel::<Job>();
        let generation = Arc::new(AtomicU64::new(0));
        let gen_worker = generation.clone();
        let path: PathBuf = path.to_path_buf();
        let password = password.map(str::to_string);
        std::thread::Builder::new()
            .name("kitab-doc".into())
            .spawn(move || {
                let doc = match std::panic::catch_unwind(|| open_doc(&path, password.as_deref())) {
                    Ok(Ok((doc, info))) => {
                        let _ = reply_tx.send(Ok(info));
                        doc
                    }
                    Ok(Err(e)) => {
                        let _ = reply_tx.send(Err(e));
                        return;
                    }
                    Err(_) => {
                        let _ = reply_tx.send(Err(OpenError::Failed(
                            "engine panicked while opening".into(),
                        )));
                        return;
                    }
                };
                while let Ok(job) = jobs_rx.recv() {
                    match job {
                        Job::Render(req, reply) => {
                            if req.generation < gen_worker.load(Ordering::Acquire) {
                                let _ = reply.send(Ok(None));
                                continue;
                            }
                            let out =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    render(&doc, &req)
                                }))
                                .unwrap_or_else(|_| Err("engine panicked while rendering".into()));
                            let stale = req.generation < gen_worker.load(Ordering::Acquire);
                            let _ = reply.send(out.map(|b| if stale { None } else { Some(b) }));
                        }
                        Job::Close => break,
                    }
                }
            })
            .map_err(|e| OpenError::Failed(e.to_string()))?;
        let info = reply_rx
            .recv()
            .map_err(|_| OpenError::Failed("engine thread ended".into()))??;
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        if let Ok(mut docs) = self.docs.lock() {
            docs.insert(
                id,
                DocWorker {
                    jobs: jobs_tx,
                    generation,
                },
            );
        }
        Ok((id, info))
    }

    fn render(&self, doc: DocHandle, req: RenderRequest) -> Result<Option<Bitmap>, String> {
        let (tx, rx) = channel();
        {
            let docs = self.docs.lock().map_err(|_| "engine lock poisoned")?;
            let w = docs.get(&doc).ok_or("document is closed")?;
            w.jobs
                .send(Job::Render(req, tx))
                .map_err(|_| "document worker ended")?;
        }
        rx.recv().map_err(|_| "document worker ended".to_string())?
    }

    fn set_generation(&self, doc: DocHandle, generation: u64) {
        if let Ok(docs) = self.docs.lock() {
            if let Some(w) = docs.get(&doc) {
                w.generation.store(generation, Ordering::Release);
            }
        }
    }

    fn links(&self, _doc: DocHandle, _page: u32) -> Vec<Link> {
        Vec::new() // kitab-mupdf provides links
    }

    fn find(
        &self,
        _doc: DocHandle,
        _query: &str,
        _options: FindOptions,
        _on_hit: &mut dyn FnMut(FindHit) -> bool,
    ) {
        // kitab-mupdf provides find
    }

    fn close(&self, doc: DocHandle) {
        if let Ok(mut docs) = self.docs.lock() {
            if let Some(w) = docs.remove(&doc) {
                let _ = w.jobs.send(Job::Close);
            }
        }
    }
}
