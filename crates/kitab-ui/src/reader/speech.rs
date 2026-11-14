// SPDX-License-Identifier: AGPL-3.0-or-later

//! Read aloud (Ctrl+Shift+U, or the dock's speaker): the system's voice reads
//! from the selection, or from the top of what is on screen, one sentence at
//! a time and page after page. The sentence being read is shaded and its
//! word marked, and the pages follow along.
//!
//! Markdown reads block by block: the block being read is shaded (its text
//! isn't laid out as page characters, so there is no word to mark).
//!
//! One document reads at a time: starting another takes the voice over, and
//! the first one simply finds itself stopped. The voice preview waits while a
//! document reads. Nothing is loaded until read aloud is first used.

use super::Reader;
use super::geom::rect_to_view;
use super::{ReaderEvent, Status};
use crate::style::Style;
use gpui::{AnyElement, Context, IntoElement, ListOffset, Styled, div, px};
use kitab_core::RectPt;
use kitab_core::spoken::{Spoken, sentences};
use kitab_speech::{Event, Speaker, Voice};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Pages in a row with nothing to read before giving up (a scanned book).
const EMPTY_PAGES_MAX: u32 = 30;

static SPEAKER: OnceLock<Speaker> = OnceLock::new();
/// The reading that has the voice now (0: none).
static OWNER: AtomicU64 = AtomicU64::new(0);
/// The preview's serial, when it was the last to take the voice.
static PREVIEW: AtomicU64 = AtomicU64::new(0);
static NEXT: AtomicU64 = AtomicU64::new(1);
/// The system's voices, once asked for.
static VOICES: Mutex<Option<Result<Vec<Voice>, String>>> = Mutex::new(None);

fn speaker() -> &'static Speaker {
    SPEAKER.get_or_init(Speaker::new)
}

/// The voices, if they have been loaded (`load_voices`).
pub(crate) fn voices() -> Option<Result<Vec<Voice>, String>> {
    VOICES.lock().ok()?.clone()
}

/// Ask the system for its voices. Blocks: call it off the UI thread.
pub(crate) fn load_voices() -> Result<Vec<Voice>, String> {
    if let Some(v) = voices() {
        return v;
    }
    let v = speaker().voices().map(|mut v| {
        v.sort_by(|a, b| a.language.cmp(&b.language).then(a.name.cmp(&b.name)));
        v
    });
    // An error isn't kept: the service may be there next time.
    if v.is_ok()
        && let Ok(mut cache) = VOICES.lock()
    {
        *cache = Some(v.clone());
    }
    v
}

/// Whether a document is reading aloud now (paused or not).
pub(crate) fn document_reading() -> bool {
    let owner = OWNER.load(Ordering::SeqCst);
    owner != 0 && owner != PREVIEW.load(Ordering::SeqCst)
}

/// Settings' Preview: a sentence in the chosen voice, speed and tone. While
/// a document reads, nothing: the reading itself picks the change up (the
/// caller restarts its sentence) instead of being cut off.
pub(crate) fn preview(s: &crate::settings::ReadAloud) {
    if document_reading() {
        return;
    }
    let serial = NEXT.fetch_add(1, Ordering::SeqCst);
    PREVIEW.store(serial, Ordering::SeqCst);
    OWNER.store(serial, Ordering::SeqCst);
    speaker().speak(
        "This is how Kitab reads your documents aloud.".into(),
        s.voice(),
        s.params(),
        Box::new(|_| {}),
    );
}

#[derive(Default)]
pub(super) struct SpeechState {
    /// This reading's number; 0 when not reading.
    serial: u64,
    paused: bool,
    page: u32,
    /// Where on `page` to start once its characters are in.
    from: usize,
    waiting: bool,
    sentences: Vec<Spoken>,
    at: usize,
    /// The word being spoken: page characters `a..b`.
    word: Option<(usize, usize)>,
    empty_pages: u32,
    /// Going back a sentence from a page's first: start the page loaded next
    /// at its last sentence.
    from_end: bool,
    tx: Option<smol::channel::Sender<(u64, Event)>>,
}

impl SpeechState {
    fn mine(&self) -> bool {
        self.serial != 0 && OWNER.load(Ordering::SeqCst) == self.serial
    }
}

impl Drop for SpeechState {
    fn drop(&mut self) {
        if self.mine()
            && let Some(s) = SPEAKER.get()
        {
            s.stop();
            OWNER.store(0, Ordering::SeqCst);
        }
    }
}

impl Reader {
    /// Read aloud works here: pages with text, and Markdown (not while
    /// it's being edited).
    pub fn can_read_aloud(&self) -> bool {
        self.status == Status::Open && self.md.as_ref().is_none_or(|m| m.edit.is_none())
    }

    /// Markdown: the block being read aloud (shaded).
    pub(super) fn speech_block(&self) -> Option<usize> {
        (self.md.is_some() && self.speech.mine() && !self.speech.waiting)
            .then_some(self.speech.page as usize)
    }

    /// Pages to read: a Markdown file's blocks, else the document's pages.
    fn speech_pages(&self) -> usize {
        match self.md.as_ref() {
            Some(md) => md.doc.blocks.len(),
            None => self.page_count(),
        }
    }

    /// Markdown block `ix` as characters to read: its text, one "line" per
    /// line of it. Code, diagrams and the like say nothing.
    fn md_chars(&self, ix: usize) -> Option<Vec<kitab_core::TextChar>> {
        use kitab_md::Block;
        let block = self.md.as_ref()?.doc.blocks.get(ix)?;
        if matches!(
            block,
            Block::Code { .. }
                | Block::Mermaid(_)
                | Block::Math(_)
                | Block::Rule
                | Block::Frontmatter(_)
        ) {
            return Some(Vec::new());
        }
        let mut line = 0;
        let mut out = Vec::new();
        for ch in kitab_md::plain_text(block).chars() {
            let (ch, next) = match ch {
                '\n' => (' ', line + 1),
                '\t' => (' ', line),
                c => (c, line),
            };
            out.push(kitab_core::TextChar {
                ch,
                rect: RectPt::default(),
                line,
            });
            line = next;
        }
        Some(out)
    }

    /// Skip `delta` sentences (-1 back, +1 on), across pages; 0 says the
    /// current one again (a new speed or voice is heard at once).
    pub fn skip_sentence(&mut self, delta: i32, cx: &mut Context<Self>) {
        if !self.speech.mine() || self.speech.waiting {
            return;
        }
        // The sentence being spoken is cut off; its last events arrive
        // under the old number and are ignored.
        let serial = NEXT.fetch_add(1, Ordering::SeqCst);
        OWNER.store(serial, Ordering::SeqCst);
        self.speech.serial = serial;
        speaker().stop();
        let at = self.speech.at as i64 + i64::from(delta);
        if at < 0 {
            if self.speech.page == 0 {
                self.speech.at = 0;
                self.speak_sentence(cx);
            } else {
                self.speech.from_end = true;
                self.read_page(self.speech.page - 1, 0, cx);
            }
        } else if (at as usize) >= self.speech.sentences.len() {
            self.read_next_page(cx);
        } else {
            self.speech.at = at as usize;
            self.speak_sentence(cx);
        }
        cx.notify();
    }

    /// The voice, speed or tone changed while reading: say the current
    /// sentence again with it (a paused reading picks it up on resume).
    pub fn read_aloud_settings_changed(&mut self, cx: &mut Context<Self>) {
        if self.reading_aloud() == Some(false) {
            self.skip_sentence(0, cx);
        }
    }

    /// The tab went out of sight: a reading in it pauses.
    pub(super) fn pause_read_aloud(&mut self, cx: &mut Context<Self>) {
        if self.reading_aloud() == Some(false) {
            self.toggle_read_aloud(cx);
        }
    }

    /// `Some(paused)` while this document is reading aloud.
    pub fn reading_aloud(&self) -> Option<bool> {
        self.speech.mine().then_some(self.speech.paused)
    }

    /// Start, pause or carry on (Ctrl+Shift+U, the dock's speaker).
    pub fn toggle_read_aloud(&mut self, cx: &mut Context<Self>) {
        match self.reading_aloud() {
            None => self.start_read_aloud(cx),
            Some(false) => {
                speaker().pause();
                self.speech.paused = true;
            }
            Some(true) => {
                speaker().resume();
                self.speech.paused = false;
            }
        }
        cx.notify();
    }

    pub fn stop_read_aloud(&mut self, cx: &mut Context<Self>) {
        if self.speech.mine() {
            speaker().stop();
            OWNER.store(0, Ordering::SeqCst);
        }
        self.end_reading(cx);
    }

    fn end_reading(&mut self, cx: &mut Context<Self>) {
        // Dropping the old state lets the voice go if it still had it.
        self.speech = SpeechState::default();
        cx.notify();
    }

    fn start_read_aloud(&mut self, cx: &mut Context<Self>) {
        if self.md.is_some() {
            // From the selected block, else the block at the top.
            let top = self.list.logical_scroll_top().item_ix;
            let block = self.md_sel.range().map_or(top, |((key, _), _)| key.0);
            self.read_aloud_from(block as u32, 0, cx);
            return;
        }
        let (page, from) = match self.select.range() {
            Some(((p, i), _)) => (p, i),
            None => self.first_visible_char(),
        };
        self.read_aloud_from(page, from, cx);
    }

    /// The page menu's "Read aloud from here": from the word at window
    /// point `at` (the page's start until its text is in).
    pub fn read_aloud_at(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let (page, from) = self
            .end_at(at, cx)
            .unwrap_or_else(|| (self.current_page() as u32, 0));
        // Back to the start of the word clicked in.
        let from = match self.layers.chars.get(&page) {
            Some(chars) => {
                let mut i = from.min(chars.len());
                while i > 0
                    && !chars[i - 1].ch.is_whitespace()
                    && chars[i - 1].line == chars[i.min(chars.len() - 1)].line
                {
                    i -= 1;
                }
                i
            }
            None => from,
        };
        self.read_aloud_from(page, from, cx);
    }

    /// Read the selection aloud, from its start (stopping any other reading).
    pub fn read_selection_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(((page, from), _)) = self.select.range() {
            self.read_aloud_from(page, from, cx);
        }
    }

    fn read_aloud_from(&mut self, page: u32, from: usize, cx: &mut Context<Self>) {
        if !self.can_read_aloud() {
            return;
        }
        let serial = NEXT.fetch_add(1, Ordering::SeqCst);
        // Whatever else was reading stops here (it notices on its own).
        if let Some(s) = SPEAKER.get() {
            s.stop();
        }
        OWNER.store(serial, Ordering::SeqCst);
        let (tx, rx) = smol::channel::unbounded::<(u64, Event)>();
        let mut state = SpeechState::default();
        state.serial = serial;
        state.tx = Some(tx);
        self.speech = state;
        cx.spawn(async move |this, cx| {
            while let Ok((serial, event)) = rx.recv().await {
                if this
                    .update(cx, |r, cx| r.speech_event(serial, event, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        self.read_page(page, from, cx);
    }

    /// The current page, and its first character below the top of the view
    /// (reading starts with what you see, not with the part scrolled past).
    fn first_visible_char(&self) -> (u32, usize) {
        let page = self.current_page();
        let ix = page as u32;
        let (Some(chars), Some(sheet)) = (self.layers.chars.get(&ix), self.sheet_bounds(page))
        else {
            return (ix, 0);
        };
        let view = self.list.viewport_bounds();
        let above = f32::from(view.top() - sheet.top()) + self.top_inset;
        // Turned pages read from their start.
        if above <= 0.0 || !self.rotation.is_multiple_of(360) {
            return (ix, 0);
        }
        let y = above / self.px_per_pt(page).max(0.0001);
        let from = chars.iter().position(|c| c.rect.y + c.rect.h > y);
        (ix, from.unwrap_or(0))
    }

    /// Read `page` from its character `from`, once its text is in.
    fn read_page(&mut self, page: u32, from: usize, cx: &mut Context<Self>) {
        self.speech.page = page;
        self.speech.from = from;
        self.speech.word = None;
        let chars = if self.md.is_some() {
            let Some(chars) = self.md_chars(page as usize) else {
                self.end_reading(cx);
                return;
            };
            chars
        } else {
            let Some(chars) = self.layers.chars.get(&page) else {
                self.speech.waiting = true;
                self.ensure_chars(page, cx);
                return;
            };
            chars.to_vec()
        };
        self.speech.waiting = false;
        self.speech.sentences = sentences(&chars, from);
        self.speech.at = if std::mem::take(&mut self.speech.from_end) {
            self.speech.sentences.len().saturating_sub(1)
        } else {
            0
        };
        if self.speech.sentences.is_empty() {
            // A Markdown code block isn't a scanned page.
            if self.md.is_none() {
                self.speech.empty_pages += 1;
            }
            self.read_next_page(cx);
        } else {
            self.speech.empty_pages = 0;
            self.speak_sentence(cx);
        }
    }

    /// A page's characters came in (layer.rs).
    pub(super) fn speech_chars_in(&mut self, page: u32, cx: &mut Context<Self>) {
        if self.speech.mine() && self.speech.waiting && self.speech.page == page {
            self.read_page(page, self.speech.from, cx);
        }
    }

    fn read_next_page(&mut self, cx: &mut Context<Self>) {
        let next = self.speech.page + 1;
        if (next as usize) >= self.speech_pages() {
            self.end_reading(cx);
            return;
        }
        if self.speech.empty_pages >= EMPTY_PAGES_MAX {
            self.end_reading(cx);
            cx.emit(ReaderEvent::Message(
                "Read aloud stopped: these pages have no text (they are images).".into(),
            ));
            return;
        }
        self.read_page(next, 0, cx);
    }

    fn speak_sentence(&mut self, cx: &mut Context<Self>) {
        let (Some(s), Some(tx)) = (
            self.speech.sentences.get(self.speech.at),
            self.speech.tx.clone(),
        ) else {
            return;
        };
        let serial = self.speech.serial;
        let text = s.text.clone();
        let start = s.span().0;
        self.speech.word = None;
        let settings = Style::get(cx).read_aloud.clone();
        speaker().speak(
            text,
            settings.voice(),
            settings.params(),
            Box::new(move |e| {
                let _ = tx.try_send((serial, e));
            }),
        );
        if self.speech.paused {
            speaker().pause();
        }
        self.follow_speech(self.speech.page, start, cx);
        cx.notify();
    }

    fn speech_event(&mut self, serial: u64, event: Event, cx: &mut Context<Self>) {
        if serial != self.speech.serial {
            return;
        }
        if !self.speech.mine() {
            // Another document (or the preview) took the voice.
            self.end_reading(cx);
            return;
        }
        match event {
            Event::Word { start, len } => {
                let Some(s) = self.speech.sentences.get(self.speech.at) else {
                    return;
                };
                let (Some(&a), Some(&b)) = (
                    s.map.get(start),
                    s.map.get((start + len).min(s.map.len()).saturating_sub(1)),
                ) else {
                    return;
                };
                self.speech.word = Some((a, b + 1));
                self.follow_speech(self.speech.page, a, cx);
                cx.notify();
            }
            Event::Done => {
                self.speech.at += 1;
                if self.speech.at < self.speech.sentences.len() {
                    self.speak_sentence(cx);
                } else {
                    self.read_next_page(cx);
                }
            }
            Event::Failed(why) => {
                self.end_reading(cx);
                cx.emit(ReaderEvent::Message(format!(
                    "Read aloud couldn't start: {why}"
                )));
            }
        }
    }

    /// Scroll so page character `ch` is in view, when it isn't (unless
    /// following is off in the read-aloud panel).
    fn follow_speech(&mut self, page: u32, ch: usize, cx: &mut Context<Self>) {
        if !Style::get(cx).read_aloud.follow {
            return;
        }
        let ix = page as usize;
        if self.md.is_some() {
            // The block, when it isn't on screen.
            let view = self.list.viewport_bounds();
            let shown = self.list.bounds_for_item(ix).is_some_and(|b| {
                b.top() >= view.top() + px(self.top_inset) && b.top() < view.bottom() - px(160.0)
            });
            if !shown {
                super::markdown::reveal_block(self, ix);
                cx.notify();
            }
            return;
        }
        let Some(rect) = self
            .layers
            .chars
            .get(&page)
            .and_then(|c| c.get(ch))
            .map(|c| c.rect)
        else {
            return;
        };
        let v = rect_to_view(
            rect,
            self.raw_page_size(ix),
            self.rotation,
            self.px_per_pt(ix),
        );
        let view = self.list.viewport_bounds();
        let h = f32::from(view.size.height);
        if let Some(sheet) = self.sheet_bounds(ix) {
            let top = f32::from(sheet.top()) + v.y;
            let lo = f32::from(view.top()) + self.top_inset + 8.0;
            // The dock and the status bar float over the bottom.
            let hi = f32::from(view.bottom()) - 160.0;
            if top >= lo && top + v.h <= hi {
                return;
            }
        }
        self.list.scroll_to(ListOffset {
            item_ix: ix,
            offset_in_item: px((self.sheet_top_pad(ix) + v.y - h * 0.3).max(0.0)),
        });
        cx.notify();
    }

    /// The sentence being read (shaded) and its word (marked) on page `ix`.
    pub(super) fn speech_marks(&self, ix: usize, ppt: f32, st: &Style) -> Vec<AnyElement> {
        if !self.speech.mine() || self.speech.page as usize != ix || self.speech.waiting {
            return Vec::new();
        }
        let Some(chars) = self.layers.chars.get(&(ix as u32)) else {
            return Vec::new();
        };
        let size = self.raw_page_size(ix);
        let mut out = Vec::new();
        let boxes = |a: usize, b: usize, alpha: f32, out: &mut Vec<AnyElement>| {
            for r in line_boxes(chars, a, b) {
                let v = rect_to_view(r, size, self.rotation, ppt);
                out.push(
                    div()
                        .absolute()
                        .left(px(v.x - 1.0))
                        .top(px(v.y - 1.0))
                        .w(px(v.w + 2.0))
                        .h(px(v.h + 2.0))
                        .rounded(px(2.0))
                        .bg(st.c(|c| c.accent).opacity(alpha))
                        .into_any_element(),
                );
            }
        };
        if let Some(s) = self.speech.sentences.get(self.speech.at) {
            let (a, b) = s.span();
            boxes(a, b, 0.12, &mut out);
        }
        if let Some((a, b)) = self.speech.word {
            boxes(a, b, 0.35, &mut out);
        }
        out
    }
}

/// Boxes around characters `a..b`, one per line.
fn line_boxes(chars: &[kitab_core::TextChar], a: usize, b: usize) -> Vec<RectPt> {
    let mut out = Vec::new();
    let mut run: Option<(u32, RectPt)> = None;
    for c in &chars[a.min(chars.len())..b.min(chars.len())] {
        if c.ch.is_whitespace() {
            continue;
        }
        run = match run {
            Some((line, r)) if line == c.line => Some((line, super::select::union(r, c.rect))),
            Some((_, r)) => {
                out.push(r);
                Some((c.line, c.rect))
            }
            None => Some((c.line, c.rect)),
        };
    }
    if let Some((_, r)) = run {
        out.push(r);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::line_boxes;
    use kitab_core::{RectPt, TextChar};

    #[test]
    fn one_box_per_line() {
        let c = |ch, x: f32, line| TextChar {
            ch,
            rect: RectPt {
                x,
                y: line as f32 * 10.0,
                w: 5.0,
                h: 10.0,
            },
            line,
        };
        let chars = vec![
            c('a', 0.0, 0),
            c('b', 5.0, 0),
            c(' ', 10.0, 0),
            c('c', 0.0, 1),
        ];
        let b = line_boxes(&chars, 0, 4);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].w, 10.0);
        assert_eq!(b[1].y, 10.0);
    }
}
