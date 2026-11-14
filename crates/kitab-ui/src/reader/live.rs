// SPDX-License-Identifier: AGPL-3.0-or-later

//! Live reload for Markdown (PLAN.md Phase 5): while a Markdown tab is the
//! one showing, its file is looked at once a second (a metadata read off the
//! UI thread) and, when it changed on disk, read and parsed again off the UI
//! thread and shown in place, keeping the reading position. Tabs in the
//! background aren't looked at, so idle cost doesn't grow with tabs
//! (AGENTS.md rule 4). Not while editing in Kitab: the editor owns the text.
//!
//! PDFs (and comics, DjVu, XPS) are looked at the same way: a file written
//! again (LaTeX building it, a download finishing) opens again at the same
//! place once it has stopped changing, unless marks or form fields not yet
//! saved would be lost.

use super::*;

/// How often the showing file is looked at.
const EVERY: Duration = Duration::from_secs(1);
/// The same cap as opening.
const MAX_BYTES: u64 = 64 << 20;

impl Reader {
    /// The workspace says whether this tab is the one showing.
    pub fn set_watched(&mut self, on: bool, cx: &mut Context<Self>) {
        if on == self.watched {
            return;
        }
        self.watched = on;
        self.watch_gen += 1;
        if on {
            self.start_watch(cx);
        }
    }

    /// Start looking (when shown, or when a shown tab's Markdown finished
    /// opening). A newer start or `set_watched(false)` ends this loop.
    pub(super) fn start_watch(&mut self, cx: &mut Context<Self>) {
        if !self.watched {
            return;
        }
        if self.md.is_none() {
            self.start_page_watch(cx);
            return;
        }
        let generation = self.watch_gen;
        let path = self.path.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(EVERY).await;
                let known = this.update(cx, |r, _| {
                    let md = r.md.as_ref()?;
                    let live = r.watched && r.watch_gen == generation;
                    live.then(|| (md.mtime, md.edit.is_some()))
                });
                let Ok(Some((known, editing))) = known else {
                    return;
                };
                if editing {
                    continue;
                }
                let p = path.clone();
                let changed = cx
                    .background_spawn(async move {
                        let meta = std::fs::metadata(&p).ok()?;
                        let mtime = meta.modified().ok()?;
                        if Some(mtime) == known || meta.len() > MAX_BYTES {
                            return None;
                        }
                        let bytes = std::fs::read(&p).ok()?;
                        let text = String::from_utf8_lossy(&bytes).into_owned();
                        let doc = kitab_md::parse(&text);
                        let lines = markdown::block_lines(&text, &doc);
                        Some((mtime, text, doc, lines))
                    })
                    .await;
                let Some((mtime, text, doc, lines)) = changed else {
                    continue;
                };
                let alive = this.update(cx, |r, cx| {
                    if r.watch_gen != generation {
                        return false;
                    }
                    let Some(md) = r.md.as_mut() else {
                        return false;
                    };
                    md.mtime = Some(mtime);
                    // Touched but the same text (a save from Kitab itself).
                    if *md.source != *text {
                        r.file_size = Some(text.len() as u64);
                        md.source = text.into();
                        r.show_parsed(doc, lines, cx);
                    }
                    true
                });
                if !matches!(alive, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
    }

    /// A page document's file changing on disk: reopen it in place.
    fn start_page_watch(&mut self, cx: &mut Context<Self>) {
        let pages = self.status == Status::Open
            && self.doc.is_some()
            && self.info.as_ref().is_some_and(|i| !i.reflowable);
        if !pages {
            return;
        }
        let generation = self.watch_gen;
        let path = self.path.clone();
        let stamp = |p: &std::path::Path| {
            let m = std::fs::metadata(p).ok()?;
            Some((m.modified().ok()?, m.len()))
        };
        cx.spawn(async move |this, cx| {
            let p = path.clone();
            let mut known = cx.background_spawn(async move { stamp(&p) }).await;
            // A new stamp seen once; reloaded when the next look agrees.
            let mut settling = None;
            loop {
                cx.background_executor().timer(EVERY).await;
                let live = this.update(cx, |r, _| {
                    r.watched && r.watch_gen == generation && !r.closing
                });
                if !matches!(live, Ok(true)) {
                    return;
                }
                let p = path.clone();
                let now = cx.background_spawn(async move { stamp(&p) }).await;
                // Gone for a moment (written by delete-and-create): wait.
                if now.is_none() || now == known {
                    settling = None;
                    continue;
                }
                if settling != now {
                    settling = now;
                    continue;
                }
                known = now;
                settling = None;
                let reopened = this.update(cx, |r, cx| r.reload_from_disk(cx));
                if matches!(reopened, Ok(true)) {
                    // The reopened document starts its own watch.
                    return;
                }
            }
        })
        .detach();
    }

    /// The file changed on disk: open it again where you are. False when
    /// it can't now (edits not yet saved into it, a save running).
    fn reload_from_disk(&mut self, cx: &mut Context<Self>) -> bool {
        if self.has_unsaved_edits() || self.markup_saving() || self.status != Status::Open {
            return false;
        }
        self.save_now(cx);
        if let Some(doc) = self.doc.take() {
            let engine = self.engine.clone();
            cx.background_spawn(async move { engine.close(doc) })
                .detach();
        }
        self.form.fields.clear();
        self.form.typing = None;
        self.form.choosing = None;
        self.file_annots = None;
        self.cache.clear();
        self.layers = layer::Layers::default();
        self.generation += 1;
        let password = self.unlock.clone();
        self.open(password, cx);
        self.load_file_size(cx);
        cx.notify();
        true
    }
}
