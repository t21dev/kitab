// SPDX-License-Identifier: AGPL-3.0-or-later

//! Live reload for Markdown (PLAN.md Phase 5): while a Markdown tab is the
//! one showing, its file is looked at once a second (a metadata read off the
//! UI thread) and, when it changed on disk, read and parsed again off the UI
//! thread and shown in place, keeping the reading position. Tabs in the
//! background aren't looked at, so idle cost doesn't grow with tabs
//! (AGENTS.md rule 4). Not while editing in Kitab: the editor owns the text.

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
        if !self.watched || self.md.is_none() {
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
}
