// SPDX-License-Identifier: AGPL-3.0-or-later

//! Books (EPUB, MOBI, FB2, TXT): pages laid out to the window, and the text
//! size (PLAN.md Phase 4).
//!
//! A book page is the reading area's size at 100 %, so fit width shows one
//! page per screen. When the window settles at a new size, or zoom changes
//! the text size, the engine lays the book out again on a worker and the
//! reader keeps its place: the same chapter, the same fraction through it.

use super::*;
use kitab_core::BookLayout;

/// A resize lays out again once the window has been still this long.
const SETTLE: Duration = Duration::from_millis(220);

pub(super) struct BookState {
    /// The layout the engine has now.
    pub layout: BookLayout,
    /// A newer layout waiting to run, or running.
    pub wanted: Option<BookLayout>,
    /// Bumped per request; a timer for an older one does nothing.
    pub serial: u64,
    pub busy: bool,
    /// The saved position (0..=1 through the book), applied after the first
    /// layout to the window: saved page numbers belong to an older layout.
    pub restore: Option<f32>,
}

/// Where the reader is, in layout-free terms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Anchor {
    /// Index into the top-level contents entries that have a page.
    chapter: Option<usize>,
    /// 0..1 through that chapter (or through the book without contents).
    fraction: f32,
}

/// Top-level contents pages, in order.
fn chapter_starts(outline: &[OutlineItem]) -> Vec<u32> {
    let mut starts: Vec<u32> = outline.iter().filter_map(|o| o.page).collect();
    starts.sort_unstable();
    starts.dedup();
    starts
}

/// Where `page` is: chapter and fraction through it.
pub(super) fn anchor_of(page: u32, page_count: u32, outline: &[OutlineItem]) -> Anchor {
    let starts = chapter_starts(outline);
    let span = |from: u32, to: u32| (to.saturating_sub(from)).max(1) as f32;
    match starts.iter().rposition(|&s| s <= page) {
        Some(i) => {
            let from = starts[i];
            let to = starts.get(i + 1).copied().unwrap_or(page_count);
            Anchor {
                chapter: Some(i),
                fraction: ((page - from) as f32 / span(from, to)).clamp(0.0, 1.0),
            }
        }
        None => Anchor {
            chapter: None,
            fraction: (page as f32 / span(0, page_count)).clamp(0.0, 1.0),
        },
    }
}

/// The page for `anchor` in a new layout.
pub(super) fn page_of(anchor: Anchor, page_count: u32, outline: &[OutlineItem]) -> u32 {
    let starts = chapter_starts(outline);
    let last = page_count.saturating_sub(1);
    let (from, to) = match anchor.chapter.and_then(|i| starts.get(i).map(|&s| (i, s))) {
        Some((i, from)) => (from, starts.get(i + 1).copied().unwrap_or(page_count)),
        None => (0, page_count),
    };
    let page = from as f32 + anchor.fraction * (to.saturating_sub(from)) as f32;
    // Nearest page, but never past the chapter's last one.
    (page.round() as u32)
        .min(to.saturating_sub(1).max(from))
        .min(last)
}

impl Reader {
    /// A book: pages come from a layout, not from the file.
    pub fn is_book(&self) -> bool {
        self.book.is_some()
    }

    /// The book's text size in points (the dock shows it instead of a zoom).
    pub fn book_text(&self) -> Option<f32> {
        self.book.as_ref().map(|b| b.wanted.unwrap_or(b.layout).em)
    }

    /// A newly opened document: books start tracking their layout.
    pub(super) fn start_book(&mut self, info: &DocInfo) {
        if !info.reflowable {
            self.book = None;
            return;
        }
        // Saved page numbers belong to whatever layout the book had then.
        let restore = self.restore.take().map(|s| s.progress);
        self.zoom = ZoomMode::FitWidth;
        self.book = Some(BookState {
            layout: BookLayout::DEFAULT,
            wanted: None,
            serial: 0,
            busy: false,
            restore,
        });
    }

    /// The layout that fills the reading area at 100 %, at text size `em`.
    fn layout_for_window(&self, em: f32) -> Option<BookLayout> {
        let (vw, vh) = self.viewport;
        if vw <= 0.0 || vh <= 0.0 {
            return None;
        }
        let side = |v: f32| (v / PX_PER_PT_AT_100).round().clamp(100.0, 10_000.0);
        Some(BookLayout {
            width: side(vw - 2.0 * GAP),
            height: side(vh - 2.0 * GAP - self.top_inset),
            em: crate::settings::clamp_book_text(em),
        })
    }

    /// Each frame: lay the book out again when the window or the text size
    /// moved. A text size change runs at once; a resize waits until the
    /// window is still.
    pub(super) fn check_book_layout(&mut self, cx: &mut Context<Self>) {
        if self.status != Status::Open {
            return;
        }
        let em = Style::get(cx).book_text;
        let Some(want) = self.layout_for_window(em) else {
            return;
        };
        let Some(book) = self.book.as_mut() else {
            return;
        };
        let now = book.wanted.unwrap_or(book.layout);
        let resized =
            (want.width - now.width).abs() > 4.0 || (want.height - now.height).abs() > 4.0;
        let retexted = (want.em - now.em).abs() > 0.01;
        if !resized && !retexted {
            return;
        }
        // The first layout to the window runs at once too.
        let first = book.layout == BookLayout::DEFAULT && book.wanted.is_none();
        book.wanted = Some(want);
        book.serial += 1;
        let serial = book.serial;
        let delay = if retexted || first {
            Duration::ZERO
        } else {
            SETTLE
        };
        let timer = cx.background_executor().timer(delay);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |r, cx| r.run_book_layout(serial, cx));
        })
        .detach();
    }

    fn run_book_layout(&mut self, serial: u64, cx: &mut Context<Self>) {
        let anchor = self.book_anchor();
        let (Some(doc), Some(book)) = (self.doc, self.book.as_mut()) else {
            return;
        };
        // A newer request has its own timer; a running one picks this up when done.
        if book.serial != serial || book.busy {
            return;
        }
        let Some(want) = book.wanted else {
            return;
        };
        book.busy = true;
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { engine.relayout(doc, want) })
                .await;
            let _ = this.update(cx, |r, cx| r.finish_book_layout(want, anchor, result, cx));
        })
        .detach();
    }

    fn book_anchor(&self) -> Anchor {
        let info = self.info.as_ref();
        let count = info.map_or(1, |i| i.page_count);
        let outline = info.map_or(&[][..], |i| i.outline.as_slice());
        anchor_of(self.current_page() as u32, count, outline)
    }

    fn finish_book_layout(
        &mut self,
        want: BookLayout,
        anchor: Anchor,
        result: Result<DocInfo, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(book) = self.book.as_mut() else {
            return;
        };
        book.busy = false;
        match result {
            Ok(info) => {
                book.layout = want;
                if book.wanted == Some(want) {
                    book.wanted = None;
                }
                let restore = book.restore.take();
                let page = match restore {
                    Some(p) => (p.clamp(0.0, 1.0) * info.page_count.saturating_sub(1) as f32)
                        .round() as u32,
                    None => page_of(anchor, info.page_count, &info.outline),
                };
                self.info = Some(info);
                // Everything per page belongs to the old layout.
                self.cache.clear();
                self.layers = layer::Layers::default();
                self.select = select::SelectState::default();
                self.relayout(cx);
                self.list.scroll_to(ListOffset {
                    item_ix: page as usize,
                    offset_in_item: px(0.0),
                });
                self.last_page = page as usize;
                if self.find.open && !self.find.query.trim().is_empty() {
                    self.search(cx);
                }
                cx.emit(ReaderEvent::PageChanged);
            }
            Err(e) => {
                // The book stays at its old layout; no path in the log (rule 13).
                log::warn!("book layout failed: {e}");
                book.wanted = None;
            }
        }
        // A newer size arrived while this one ran.
        if let Some(book) = self.book.as_ref().filter(|b| b.wanted.is_some()) {
            let serial = book.serial;
            self.run_book_layout(serial, cx);
        }
        cx.notify();
    }

    /// Zoom in a book: one point of text size up or down, for every book.
    pub(super) fn step_book_text(&mut self, up: bool, cx: &mut Context<Self>) {
        let now = self
            .book_text()
            .unwrap_or(crate::settings::BOOK_TEXT_DEFAULT);
        let next = crate::settings::clamp_book_text(if up { now + 1.0 } else { now - 1.0 });
        if (next - now).abs() > 0.01 {
            cx.emit(ReaderEvent::BookText(next));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(page: u32) -> OutlineItem {
        OutlineItem {
            title: format!("Chapter at {page}"),
            page: Some(page),
            uri: None,
            open: false,
            children: Vec::new(),
        }
    }

    #[test]
    fn the_place_survives_a_new_layout() {
        // 3 chapters over 30 pages; reader 4 pages into the second (10..20).
        let before = [chapter(0), chapter(10), chapter(20)];
        let a = anchor_of(14, 30, &before);
        assert_eq!(a.chapter, Some(1));
        assert!((a.fraction - 0.4).abs() < 1e-6);
        // Wider pages: the same chapters at 0, 5, 10 of 15.
        let after = [chapter(0), chapter(5), chapter(10)];
        assert_eq!(page_of(a, 15, &after), 7);
        // Same layout back: the same page.
        assert_eq!(page_of(anchor_of(14, 30, &before), 30, &before), 14);
    }

    #[test]
    fn books_without_contents_use_the_whole_book() {
        let a = anchor_of(50, 101, &[]);
        assert_eq!(a.chapter, None);
        assert_eq!(page_of(a, 201, &[]), 100);
        assert_eq!(page_of(a, 1, &[]), 0);
    }

    #[test]
    fn before_the_first_chapter_and_past_the_end() {
        let outline = [chapter(3), chapter(8)];
        let a = anchor_of(1, 10, &outline);
        assert_eq!(a.chapter, None);
        // A last page that no longer exists clamps to the end.
        let end = anchor_of(9, 10, &outline);
        assert!(page_of(end, 4, &[chapter(1), chapter(2)]) <= 3);
    }
}
