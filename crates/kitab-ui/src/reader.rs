// SPDX-License-Identifier: AGPL-3.0-or-later

//! One open document: true-size page sheets in a virtualised list (only visible
//! pages render), zoom modes, rotation, keyboard navigation, the password prompt
//! and the error placeholders (PLAN.md Phase 1, DESIGN.md § Page sheets).

mod book;
mod dock;
mod editor;
mod find;
pub use dock::TOP_INSET;
mod geom;
mod info;
mod layer;
mod markdown;
mod markup;
mod select;

use crate::actions::*;
use crate::cache::{PageCache, PageKey, scale_key};
use crate::engine::{DocEngine, DocHandle};
use crate::icons::icon;
use crate::library::Library;
use crate::motion;
use crate::settings::{DockAt, PageNav};
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable,
    FontWeight, ImageSource, InteractiveElement, IntoElement, KeyDownEvent, ListAlignment,
    ListOffset, ListState, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels,
    Point, Render, RenderImage, ScrollWheelEvent, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, canvas, div, img, list, prelude::FluentBuilder, px, svg,
};
use image::{Frame, RgbaImage};
use kitab_core::{
    DocInfo, Locator, OpenError, OutlineItem, ReadingState, RenderRequest, SizePt, ZoomMode,
};
use std::cell::Cell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// Points to logical pixels at 100 %: 1 pt = 1/72 in, 1 px = 1/96 in.
const PX_PER_PT_AT_100: f32 = 96.0 / 72.0;
/// Gap between page sheets and around them (DESIGN.md: 22 px).
const GAP: f32 = crate::layout::PAGE_GAP;
const ZOOM_STEPS: &[f32] = &[
    0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0, 6.4,
];
/// Rendered pages kept per tab (bytes). Phase 2 makes this a setting shared across tabs.
const CACHE_BUDGET: usize = 96 * 1024 * 1024;
/// A page with nothing to show yet first gets a quick render at this share of
/// its sharp scale, so fast scrolling shows the page (soft) instead of paper.
const PREVIEW_SCALE: f32 = 0.25;
/// Pages kept wanted either side of the visible ones; queued renders further
/// away are skipped when their turn comes.
const WANTED_MARGIN: u32 = 2;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Opening,
    NeedsPassword { wrong: bool },
    Failed(OpenError),
    Open,
}

/// Events the shell listens to (tab title, outline).
pub enum ReaderEvent {
    Opened,
    PageChanged,
    /// Right click on the page area, at this window position.
    ContextMenu(Point<Pixels>),
    /// The dock's ⋯ button, at this window position.
    MoreMenu(Point<Pixels>),
    /// A link in a Markdown document points at another file: open it in a tab.
    OpenPath(PathBuf),
    /// Zoom in a book changed the text size (points), for every book.
    BookText(f32),
}

/// What "Save page as…" writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageExport {
    Image,
    Text,
    Pdf,
}

impl PageExport {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Image => "png",
            Self::Text => "txt",
            Self::Pdf => "pdf",
        }
    }
}

/// Resolution of pages copied or saved as images.
pub const EXPORT_DPI: f32 = 150.0;

/// Jumps remembered for Alt+Left / Alt+Right.
const HISTORY_MAX: usize = 100;

impl EventEmitter<ReaderEvent> for Reader {}

pub struct Reader {
    pub path: PathBuf,
    engine: Arc<dyn DocEngine>,
    library: Arc<dyn Library>,
    doc: Option<DocHandle>,
    pub info: Option<DocInfo>,
    pub status: Status,
    zoom: ZoomMode,
    rotation: u16,
    list: ListState,
    cache: PageCache,
    pending: HashSet<PageKey>,
    generation: u64,
    focus: FocusHandle,
    password: String,
    viewport: (f32, f32),
    last_page: usize,
    /// A save is scheduled (debounced: at most one write per 2 s).
    save_scheduled: bool,
    /// Where to go once the first layout has a viewport (restored reading state).
    restore: Option<ReadingState>,
    /// The page field (Ctrl+G or a click on the page number).
    page_focus: FocusHandle,
    /// The field's text while it is being edited.
    page_edit: Option<String>,
    /// The scrubber track's bounds from the last frame, for clicks and drags.
    track: Rc<Cell<Option<Bounds<Pixels>>>>,
    dragging: bool,
    back: Vec<usize>,
    forward: Vec<usize>,
    _subs: Vec<Subscription>,
    /// First and last page on screen last frame (packed hi << 32 | lo), read by
    /// render workers to skip pages that scrolled away.
    visible: Arc<std::sync::atomic::AtomicU64>,
    /// `KITAB_TRACE_RENDER=1`: log render times and blank pages to stderr.
    trace: bool,
    blank_since: std::collections::HashMap<u32, std::time::Instant>,
    /// Average render time (ms) of quick previews and of sharp pages. On
    /// scanned books decoding the page image dominates and a preview is no
    /// faster, so previews stop once that shows.
    quick_ms: Option<f32>,
    sharp_ms: Option<f32>,
    /// +1 scrolling down, -1 up: which neighbour to render ahead.
    scroll_dir: i8,
    last_top: usize,
    last_frame: Option<std::time::Instant>,
    /// The password that opened this document, if it needed one (memory only).
    unlock: Option<String>,
    find: find::FindState,
    layers: layer::Layers,
    select: select::SelectState,
    dock: dock::DockState,
    /// Room kept free above the first page (sidebar tabs: the window buttons).
    top_inset: f32,
    /// A Markdown document: its blocks are the list's items instead of pages.
    md: Option<markdown::MdState>,
    /// A book's layout (EPUB, MOBI, FB2, TXT); `None` for files with pages.
    book: Option<book::BookState>,
    /// Drawing and markup on the pages.
    markup: markup::MarkupState,
    /// The file's size in bytes, for the info chip (read off the UI thread).
    file_size: Option<u64>,
}

impl Reader {
    pub fn new(
        path: PathBuf,
        engine: Arc<dyn DocEngine>,
        library: Arc<dyn Library>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        let page_focus = cx.focus_handle();
        // Clicking away from the page field cancels the edit.
        let blur = cx.on_blur(&page_focus, window, |r, _, cx| {
            r.page_edit = None;
            cx.notify();
        });
        let mut this = Self {
            path,
            engine,
            library,
            doc: None,
            info: None,
            status: Status::Opening,
            zoom: ZoomMode::FitWidth,
            rotation: 0,
            list: ListState::new(0, ListAlignment::Top, px(800.0)),
            cache: PageCache::new(CACHE_BUDGET),
            pending: HashSet::new(),
            generation: 1,
            focus,
            password: String::new(),
            viewport: (0.0, 0.0),
            last_page: 0,
            save_scheduled: false,
            restore: None,
            page_focus,
            page_edit: None,
            track: Rc::new(Cell::new(None)),
            dragging: false,
            back: Vec::new(),
            forward: Vec::new(),
            _subs: vec![blur],
            visible: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            trace: std::env::var_os("KITAB_TRACE_RENDER").is_some(),
            blank_since: std::collections::HashMap::new(),
            quick_ms: None,
            sharp_ms: None,
            scroll_dir: 1,
            last_top: 0,
            last_frame: None,
            unlock: None,
            find: find::FindState::new(cx.focus_handle()),
            layers: layer::Layers::default(),
            select: select::SelectState::default(),
            dock: dock::DockState::default(),
            top_inset: 0.0,
            md: None,
            book: None,
            markup: markup::MarkupState::default(),
            file_size: None,
        };
        this.load_file_size(cx);
        this.open(None, cx);
        this
    }

    pub fn title(&self) -> SharedString {
        let from_meta = self
            .info
            .as_ref()
            .and_then(|i| i.title.clone())
            .filter(|t| !t.trim().is_empty());
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        SharedString::from(from_meta.unwrap_or(name))
    }

    pub fn author(&self) -> Option<String> {
        self.info
            .as_ref()
            .and_then(|i| i.author.clone())
            .filter(|a| !a.trim().is_empty())
    }

    pub fn outline(&self) -> &[OutlineItem] {
        self.info.as_ref().map_or(&[], |i| i.outline.as_slice())
    }

    pub fn page_count(&self) -> usize {
        self.info.as_ref().map_or(0, |i| i.page_count as usize)
    }

    pub fn current_page(&self) -> usize {
        self.list
            .logical_scroll_top()
            .item_ix
            .min(self.page_count().saturating_sub(1))
    }

    /// The zoom shown in the header, as a percentage of actual size, for the current page.
    pub fn zoom_percent(&self) -> u32 {
        let ppt = self.px_per_pt(self.current_page());
        (ppt / PX_PER_PT_AT_100 * 100.0).round() as u32
    }

    pub fn zoom_mode(&self) -> ZoomMode {
        self.zoom
    }

    fn open(&mut self, password: Option<String>, cx: &mut Context<Self>) {
        self.status = Status::Opening;
        if is_markdown_path(&self.path) {
            self.open_markdown(cx);
            return;
        }
        let engine = self.engine.clone();
        let library = self.library.clone();
        let path = self.path.clone();
        let tried = password.clone();
        cx.spawn(async move |this, cx| {
            let (result, saved) = cx
                .background_spawn(async move {
                    let result = engine.open(&path, password.as_deref());
                    // Per-file memory (PLAN.md Phase 1): ask the library while still off the UI thread.
                    let saved = result
                        .as_ref()
                        .ok()
                        .and_then(|(_, info)| library.opened(&path, info));
                    (result, saved)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.restore = saved;
                if result.is_ok() {
                    // Kept in memory only, for tools that reopen the file
                    // (extract, merge); never saved or logged (AGENTS.md rule 13).
                    this.unlock = tried;
                }
                this.on_opened(result, cx)
            });
        })
        .detach();
    }

    /// Read and parse a Markdown file off the UI thread.
    fn open_markdown(&mut self, cx: &mut Context<Self>) {
        let (library, path) = (self.library.clone(), self.path.clone());
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move {
                    // Large enough for any real document, small enough to refuse a disk image.
                    const MAX_BYTES: u64 = 64 << 20;
                    let meta = std::fs::metadata(&path).map_err(|_| OpenError::NotFound)?;
                    if meta.len() > MAX_BYTES {
                        return Err(OpenError::Unsupported);
                    }
                    let bytes = std::fs::read(&path).map_err(|_| OpenError::NotFound)?;
                    let text = String::from_utf8_lossy(&bytes);
                    let doc = kitab_md::parse(&text);
                    let info = DocInfo {
                        format: kitab_core::Format::Markdown,
                        title: doc.title.clone(),
                        author: doc.author.clone(),
                        page_count: doc.blocks.len().max(1) as u32,
                        page_sizes: Vec::new(),
                        outline: markdown::outline(&doc),
                        permissions: Default::default(),
                        encryption: None,
                        repaired: false,
                        reflowable: false,
                        right_to_left: false,
                    };
                    let saved = library.opened(&path, &info);
                    Ok((doc, info, saved, text.into_owned()))
                })
                .await;
            let _ = this.update(cx, |this, cx| match loaded {
                Ok((doc, info, saved, source)) => {
                    this.restore = saved;
                    let dir = this
                        .path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_default();
                    this.md = Some(markdown::MdState {
                        block_lines: doc.block_lines.clone(),
                        doc: Arc::new(doc),
                        dir,
                        open_details: Default::default(),
                        source: source.into(),
                        edit: None,
                    });
                    if let Some(state) = this.restore {
                        this.zoom = state.zoom;
                    }
                    this.list.reset(info.page_count as usize);
                    this.info = Some(info);
                    this.status = Status::Open;
                    cx.emit(ReaderEvent::Opened);
                    cx.notify();
                }
                Err(e) => {
                    this.status = Status::Failed(e);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn on_opened(
        &mut self,
        result: Result<(DocHandle, DocInfo), OpenError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((doc, info)) => {
                // Books keep their saved place as a fraction (see book.rs).
                self.start_book(&info);
                if let Some(state) = self.restore {
                    self.zoom = state.zoom;
                    self.rotation = state.rotation;
                }
                self.list.reset(info.page_count as usize);
                self.doc = Some(doc);
                self.info = Some(info);
                self.status = Status::Open;
                self.password.clear();
                self.load_marks(cx);
                cx.emit(ReaderEvent::Opened);
            }
            Err(OpenError::NeedsPassword) => self.status = Status::NeedsPassword { wrong: false },
            Err(OpenError::WrongPassword) => self.status = Status::NeedsPassword { wrong: true },
            Err(e) => self.status = Status::Failed(e),
        }
        cx.notify();
    }

    /// The reading state to remember for this document.
    pub fn reading_state(&self) -> Option<ReadingState> {
        if self.status != Status::Open {
            return None;
        }
        let top = self.list.logical_scroll_top();
        let page = top.item_ix.min(self.page_count().saturating_sub(1));
        let ppt = self.px_per_pt(page).max(0.01);
        let count = self.page_count().max(1);
        Some(ReadingState {
            locator: Locator::Fixed {
                page: page as u32,
                x: 0.0,
                y: f32::from(top.offset_in_item) / ppt,
            },
            zoom: self.zoom,
            rotation: self.rotation,
            progress: if count > 1 {
                page as f32 / (count - 1) as f32
            } else {
                1.0
            },
        })
    }

    /// Save now (tab closing, app quitting). Runs the store call off the UI thread.
    pub fn save_now(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.reading_state() {
            let (library, path) = (self.library.clone(), self.path.clone());
            cx.background_spawn(async move { library.save_state(&path, state) })
                .detach();
        }
    }

    /// Save in at most 2 s, once, however much the reader scrolls meanwhile.
    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        if self.save_scheduled {
            return;
        }
        self.save_scheduled = true;
        let timer = cx.background_executor().timer(Duration::from_secs(2));
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                this.save_scheduled = false;
                this.save_now(cx);
            });
        })
        .detach();
    }

    // ── geometry ──────────────────────────────────────────────────────────

    fn page_size(&self, ix: usize) -> SizePt {
        let s = self
            .info
            .as_ref()
            .and_then(|i| i.page_sizes.get(ix).copied())
            .unwrap_or(SizePt { w: 612.0, h: 792.0 });
        if self.rotation % 180 == 90 {
            SizePt { w: s.h, h: s.w }
        } else {
            s
        }
    }

    /// Page `ix`'s size in points before the reader's rotation (the space
    /// text, links and hits are in).
    fn raw_page_size(&self, ix: usize) -> SizePt {
        self.info
            .as_ref()
            .and_then(|i| i.page_sizes.get(ix).copied())
            .unwrap_or(SizePt { w: 612.0, h: 792.0 })
    }

    /// Space above page `ix`'s sheet inside its list item.
    fn sheet_top_pad(&self, ix: usize) -> f32 {
        if ix == 0 {
            GAP + self.top_inset
        } else {
            GAP / 2.0
        }
    }

    /// Where page `ix`'s sheet was drawn last frame, in window pixels.
    fn sheet_bounds(&self, ix: usize) -> Option<Bounds<Pixels>> {
        let item = self.list.bounds_for_item(ix)?;
        let s = self.page_size(ix);
        let ppt = self.px_per_pt(ix);
        let (w, h) = (s.w * ppt, s.h * ppt);
        let x = item.origin.x + (item.size.width - px(w)) / 2.0;
        let y = item.origin.y + px(self.sheet_top_pad(ix));
        Some(Bounds::new(gpui::point(x, y), gpui::size(px(w), px(h))))
    }

    /// Page `ix`'s size in points as shown (rotation applied).
    pub fn page_size_pt(&self, ix: usize) -> SizePt {
        self.page_size(ix)
    }

    /// Logical pixels per point for page `ix` in the current zoom mode. Fit modes
    /// fit each page on its own, so mixed sizes keep their shape (PLAN.md §3.3).
    fn px_per_pt(&self, ix: usize) -> f32 {
        if self.md.is_some() {
            return self.md_scale() * PX_PER_PT_AT_100;
        }
        let s = self.page_size(ix);
        let (vw, vh) = self.viewport;
        let (vw, vh) = (
            if vw > 0.0 { vw } else { 1000.0 },
            if vh > 0.0 { vh } else { 800.0 },
        );
        let fit_w = ((vw - 2.0 * GAP).max(80.0)) / s.w.max(1.0);
        let fit_h = ((vh - 2.0 * GAP).max(80.0)) / s.h.max(1.0);
        match self.zoom {
            ZoomMode::FitWidth => fit_w,
            ZoomMode::FitPage => fit_w.min(fit_h),
            ZoomMode::ActualSize => PX_PER_PT_AT_100,
            ZoomMode::Custom(z) => z * PX_PER_PT_AT_100,
        }
    }

    /// Re-measure every page (zoom, rotation or viewport changed), keeping the reading position.
    fn relayout(&mut self, cx: &mut Context<Self>) {
        let top = self.list.logical_scroll_top();
        let page = top.item_ix.min(self.page_count().saturating_sub(1));
        let before = self.page_size(page).h * self.px_per_pt(page) + GAP;
        let fraction = if before > 0.0 {
            f32::from(top.offset_in_item) / before
        } else {
            0.0
        };
        self.generation += 1;
        if let Some(doc) = self.doc {
            let (engine, generation) = (self.engine.clone(), self.generation);
            cx.background_spawn(async move { engine.set_generation(doc, generation) })
                .detach();
        }
        self.pending.clear();
        self.list.reset(self.page_count());
        let after = self.page_size(page).h * self.px_per_pt(page) + GAP;
        self.list.scroll_to(ListOffset {
            item_ix: page,
            offset_in_item: px(fraction * after),
        });
        cx.notify();
    }

    fn set_zoom(&mut self, zoom: ZoomMode, cx: &mut Context<Self>) {
        if self.is_book() {
            // Actual size (Ctrl+0) puts the text size back; a book page
            // otherwise always fits the width.
            if zoom == ZoomMode::ActualSize {
                cx.emit(ReaderEvent::BookText(crate::settings::BOOK_TEXT_DEFAULT));
            }
            return;
        }
        self.zoom = zoom;
        self.relayout(cx);
    }

    fn step_zoom(&mut self, up: bool, cx: &mut Context<Self>) {
        // In a book, zoom is the text size: the page always fits the window.
        if self.is_book() {
            self.step_book_text(up, cx);
            return;
        }
        let current = self.px_per_pt(self.current_page()) / PX_PER_PT_AT_100;
        let next = if up {
            ZOOM_STEPS
                .iter()
                .copied()
                .find(|z| *z > current + 0.005)
                .unwrap_or(6.4)
        } else {
            ZOOM_STEPS
                .iter()
                .rev()
                .copied()
                .find(|z| *z < current - 0.005)
                .unwrap_or(0.1)
        };
        self.set_zoom(ZoomMode::Custom(next), cx);
    }

    fn go_to_page(&mut self, page: usize, cx: &mut Context<Self>) {
        let page = page.min(self.page_count().saturating_sub(1));
        self.list.scroll_to(ListOffset {
            item_ix: page,
            offset_in_item: px(0.0),
        });
        cx.notify();
    }

    /// Go to `page` and remember where the reader was, for Alt+Left.
    pub fn jump(&mut self, page: usize, cx: &mut Context<Self>) {
        let page = page.min(self.page_count().saturating_sub(1));
        let here = self.current_page();
        if here != page {
            self.back.push(here);
            if self.back.len() > HISTORY_MAX {
                self.back.remove(0);
            }
            self.forward.clear();
        }
        self.go_to_page(page, cx);
    }

    pub fn jump_to(&mut self, page: u32, cx: &mut Context<Self>) {
        self.jump(page as usize, cx);
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    pub fn history(&mut self, back: bool, cx: &mut Context<Self>) {
        let here = self.current_page();
        let (from, to) = if back {
            (&mut self.back, &mut self.forward)
        } else {
            (&mut self.forward, &mut self.back)
        };
        if let Some(page) = from.pop() {
            to.push(here);
            self.go_to_page(page, cx);
        }
    }

    pub fn zoom_step(&mut self, zoom_in: bool, cx: &mut Context<Self>) {
        self.step_zoom(zoom_in, cx);
    }

    pub fn set_zoom_mode(&mut self, mode: ZoomMode, cx: &mut Context<Self>) {
        self.set_zoom(mode, cx);
    }

    pub fn rotate(&mut self, clockwise: bool, cx: &mut Context<Self>) {
        self.rotation = (self.rotation + if clockwise { 90 } else { 270 }) % 360;
        self.cache.clear();
        self.relayout(cx);
    }

    /// Right or left arrow: next or previous page, the other way round in a
    /// book whose pages advance right to left (MANIFEST 04).
    fn page_arrow(&mut self, right: bool, cx: &mut Context<Self>) {
        if page_arrow_goes_forward(right, self.right_to_left()) {
            self.next_page(cx);
        } else {
            self.prev_page(cx);
        }
    }

    /// Pages advance right to left (from the book's package document).
    pub fn right_to_left(&self) -> bool {
        self.info.as_ref().is_some_and(|i| i.right_to_left)
    }

    pub fn next_page(&mut self, cx: &mut Context<Self>) {
        self.go_to_page(self.current_page() + 1, cx);
    }

    pub fn prev_page(&mut self, cx: &mut Context<Self>) {
        self.go_to_page(self.current_page().saturating_sub(1), cx);
    }

    // ── page field ──

    /// Start typing a page number (Ctrl+G, or a click on the number).
    pub fn edit_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.status != Status::Open || self.md.is_some() {
            return;
        }
        self.page_edit = Some(String::new());
        window.focus(&self.page_focus);
        cx.notify();
    }

    fn confirm_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.page_edit.take() {
            if let Some(n) = parse_page(&text, self.page_count()) {
                self.jump(n, cx);
            }
        }
        window.focus(&self.focus);
        cx.notify();
    }

    fn cancel_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page_edit = None;
        window.focus(&self.focus);
        cx.notify();
    }

    fn on_key_page(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.page_edit.as_mut() else {
            return;
        };
        let k = &e.keystroke;
        if k.key == "backspace" {
            text.pop();
        } else if let Some(ch) = k.key_char.as_ref() {
            let digits = ch.chars().all(|c| c.is_ascii_digit());
            if !k.modifiers.control && !k.modifiers.alt && digits && text.len() < 7 {
                text.push_str(ch);
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Scrubber: go to the page under window x `x`.
    fn seek_x(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(b) = self.track.get() else { return };
        let w = f32::from(b.size.width).max(1.0);
        let f = (f32::from(x - b.origin.x) / w).clamp(0.0, 1.0);
        let last = self.page_count().saturating_sub(1);
        self.go_to_page((f * last as f32).round() as usize, cx);
    }

    /// The password that opened the document, for tools that reopen the file.
    pub fn unlock(&self) -> Option<String> {
        self.unlock.clone()
    }

    /// A Markdown document (blocks, not pages).
    pub fn is_markdown(&self) -> bool {
        self.md.is_some()
    }

    pub fn is_pdf(&self) -> bool {
        self.info
            .as_ref()
            .is_some_and(|i| i.format == kitab_core::Format::Pdf)
    }

    /// Save one page as PNG (at `dpi`), plain text or a one-page PDF, at `out`.
    /// `done` gets an error message or `None`.
    pub fn save_page(
        &self,
        page: usize,
        kind: PageExport,
        dpi: f32,
        out: PathBuf,
        cx: &mut Context<Self>,
        done: impl FnOnce(Result<PathBuf, String>, &mut App) + 'static,
    ) {
        let Some(doc) = self.doc else { return };
        let engine = self.engine.clone();
        let (src, unlock, rotation, generation) = (
            self.path.clone(),
            self.unlock.clone(),
            self.rotation,
            self.generation,
        );
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_spawn(async move {
                    match kind {
                        PageExport::Image => {
                            let req = RenderRequest {
                                page: page as u32,
                                scale: dpi.clamp(36.0, 600.0) / 72.0,
                                rotation,
                                tile: None,
                                generation,
                            };
                            let bmp = engine
                                .render(doc, req)?
                                .ok_or("the page changed while saving; try again")?;
                            let png = png_from_bgra(bmp.width, bmp.height, bmp.bgra)
                                .ok_or("could not encode the image")?;
                            std::fs::write(&out, png)
                                .map_err(|e| format!("could not save: {e}"))?;
                        }
                        PageExport::Text => {
                            let text = engine.page_text(doc, page as u32).unwrap_or_default();
                            if text.trim().is_empty() {
                                return Err(
                                    "this page has no text (it may be a scanned image)".to_string()
                                );
                            }
                            std::fs::write(&out, text)
                                .map_err(|e| format!("could not save: {e}"))?;
                        }
                        PageExport::Pdf => {
                            engine.extract_pages(&src, unlock.as_deref(), &[page as u32], &out)?
                        }
                    }
                    Ok(out)
                })
                .await;
            let _ = cx.update(|cx| done(result, cx));
        })
        .detach();
    }

    // ── copy ──

    /// Copy one page's text to the clipboard (worked out off the UI thread).
    pub fn copy_page_text(&self, page: usize, cx: &mut Context<Self>) {
        if let Some(text) = self.md_block_text(page) {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            return;
        }
        let Some(doc) = self.doc.filter(|_| self.may_copy()) else {
            return;
        };
        let engine = self.engine.clone();
        cx.spawn(async move |_, cx| {
            let text = cx
                .background_spawn(async move { engine.page_text(doc, page as u32) })
                .await;
            if let Some(text) = text.filter(|t| !t.trim().is_empty()) {
                let _ =
                    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)));
            }
        })
        .detach();
    }

    /// Copy one page as a PNG at 150 dpi.
    pub fn copy_page_image(&self, page: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.doc else { return };
        let engine = self.engine.clone();
        let req = RenderRequest {
            page: page as u32,
            scale: EXPORT_DPI / 72.0,
            rotation: self.rotation,
            tile: None,
            generation: self.generation,
        };
        cx.spawn(async move |_, cx| {
            let png = cx
                .background_spawn(async move {
                    let bmp = engine.render(doc, req).ok().flatten()?;
                    png_from_bgra(bmp.width, bmp.height, bmp.bgra)
                })
                .await;
            if let Some(png) = png {
                let image = gpui::Image::from_bytes(gpui::ImageFormat::Png, png);
                let _ =
                    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_image(&image)));
            }
        })
        .detach();
    }

    // ── rendering ─────────────────────────────────────────────────────────

    fn request(&mut self, ix: usize, key: PageKey, scale: f32, cx: &mut Context<Self>) {
        self.request_as(ix, key, scale, false, cx)
    }

    /// `quick`: a soft preview (its time feeds the preview average).
    fn request_as(
        &mut self,
        ix: usize,
        key: PageKey,
        scale: f32,
        quick: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.doc else { return };
        if !self.pending.insert(key) {
            return;
        }
        let engine = self.engine.clone();
        let req = RenderRequest {
            page: ix as u32,
            scale,
            rotation: self.rotation,
            tile: None,
            generation: self.generation,
        };
        let visible = self.visible.clone();
        let page = ix as u32;
        let wanted: crate::engine::Wanted = Arc::new(move || {
            let v = visible.load(std::sync::atomic::Ordering::Relaxed);
            let (lo, hi) = ((v & 0xFFFF_FFFF) as u32, (v >> 32) as u32);
            page + WANTED_MARGIN >= lo && page <= hi + WANTED_MARGIN
        });
        let trace = self.trace;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let started = std::time::Instant::now();
                    let r = engine.render_when(doc, req, wanted);
                    let took = started.elapsed().as_secs_f32() * 1000.0;
                    if trace {
                        let what = match &r {
                            Ok(Some(b)) => format!("{}x{}", b.width, b.height),
                            Ok(None) => "skipped".into(),
                            Err(_) => "failed".into(),
                        };
                        eprintln!(
                            "render page={page} scale={:.2} {what} ms={}",
                            req.scale,
                            started.elapsed().as_millis()
                        );
                    }
                    (r, took)
                })
                .await;
            let (result, took) = result;
            let _ = this.update(cx, |this, cx| {
                this.pending.remove(&key);
                if matches!(result, Ok(Some(_))) {
                    let avg = if quick {
                        &mut this.quick_ms
                    } else {
                        &mut this.sharp_ms
                    };
                    *avg = Some(avg.map_or(took, |a| a * 0.7 + took * 0.3));
                }
                match result {
                    Ok(Some(bmp)) => {
                        if let Some(since) = this.blank_since.remove(&page) {
                            eprintln!("blank page={page} ms={}", since.elapsed().as_millis());
                        }
                        let bytes = bmp.byte_size();
                        if let Some(buffer) = RgbaImage::from_raw(bmp.width, bmp.height, bmp.bgra) {
                            let image = Arc::new(RenderImage::new([Frame::new(buffer)]));
                            this.cache.insert(key, image, bytes);
                            cx.notify();
                        }
                    }
                    Ok(None) => {} // stale generation: the view moved on
                    Err(e) => log::warn!("render failed: {e}"),
                }
            });
        })
        .detach();
    }

    fn page_element(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Frame timing: items of one frame are laid out microseconds apart, so
        // a gap over 2 ms starts a new frame. (The list is borrowed here; ask
        // it nothing.)
        if self.trace {
            let now = std::time::Instant::now();
            if let Some(last) = self.last_frame {
                let gap = now - last;
                if gap.as_micros() > 2000 {
                    eprintln!("frame us={}", gap.as_micros());
                }
            }
            self.last_frame = Some(now);
        }
        if self.md.is_some() {
            return self.md_element(ix, cx);
        }
        let st = Style::get(cx).clone();
        let size = self.page_size(ix);
        let ppt = self.px_per_pt(ix);
        let (w, h) = (size.w * ppt, size.h * ppt);
        let device_scale = ppt * window.scale_factor();
        let key = PageKey {
            page: ix as u32,
            scale_key: scale_key(device_scale),
            rotation: self.rotation,
        };
        // Corners scale with the page so they look the same at every zoom.
        let zoom = (ppt / PX_PER_PT_AT_100).clamp(0.5, 2.0);
        let radius = px(st.page_corners.radius() * zoom);
        let image = match self.cache.get(&key) {
            Some(i) => {
                crate::fire_first_paint(window, cx);
                self.ensure_links(ix as u32, cx);
                Some(i)
            }
            None => {
                let any = self.cache.any_for_page(ix as u32, self.rotation);
                // A preview only pays when it is clearly faster than the page.
                let preview_pays = match (self.quick_ms, self.sharp_ms) {
                    (Some(q), Some(s)) => q * 2.0 < s,
                    _ => true,
                };
                if any.is_none() && preview_pays {
                    // Nothing yet: a quick soft render first, then the sharp one.
                    let quick = (device_scale * PREVIEW_SCALE).max(0.05);
                    let quick_key = PageKey {
                        scale_key: scale_key(quick),
                        ..key
                    };
                    self.request_as(ix, quick_key, quick, true, cx);
                }
                if any.is_none() && self.trace {
                    self.blank_since
                        .entry(ix as u32)
                        .or_insert_with(std::time::Instant::now);
                }
                self.request(ix, key, device_scale, cx);
                any
            }
        };
        // Render the neighbours ahead of time, so the next page is ready when
        // it scrolls in (they queue behind the visible ones).
        let ahead = if self.scroll_dir >= 0 {
            ix + 1
        } else {
            ix.wrapping_sub(1)
        };
        for n in [ahead] {
            if n < self.page_count() {
                let s = self.px_per_pt(n) * window.scale_factor();
                let k = PageKey {
                    page: n as u32,
                    scale_key: scale_key(s),
                    rotation: self.rotation,
                };
                if !self.cache.contains(&k) {
                    self.request(n, k, s, cx);
                }
            }
        }
        div()
            .w_full()
            .flex()
            .justify_center()
            .pt(px(self.sheet_top_pad(ix)))
            .pb(px(GAP / 2.0))
            .child(
                div()
                    .w(px(w))
                    .h(px(h))
                    .flex_none()
                    // Paper before the bitmap arrives; the page's own colour once it has.
                    .bg(gpui::white())
                    .rounded(radius)
                    .shadow(st.sheet())
                    .overflow_hidden()
                    .relative()
                    .cursor(if self.markup.open && self.markup.tool.is_some() {
                        gpui::CursorStyle::Crosshair
                    } else {
                        gpui::CursorStyle::IBeam
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |r, e: &MouseDownEvent, window, cx| {
                            // A markup tool draws; otherwise the click selects text.
                            if !r.markup_down(ix, e, cx) {
                                r.select_down(ix, e, window, cx)
                            }
                        }),
                    )
                    // Characters for selection load the first time the pointer is over the page.
                    .on_mouse_move(cx.listener(move |r, _: &MouseMoveEvent, _, cx| {
                        r.ensure_chars(ix as u32, cx)
                    }))
                    .children(image.map(|i| {
                        img(ImageSource::Render(i))
                            .w(px(w))
                            .h(px(h))
                            .rounded(radius)
                    }))
                    .children(self.markup_overlay(ix, ppt))
                    .children(self.selection_marks(ix, ppt, &st))
                    .children(self.find_marks(ix, ppt, &st))
                    .children(self.link_marks(ix, ppt, &st, cx)),
            )
            .into_any_element()
    }

    // ── actions ───────────────────────────────────────────────────────────

    fn on_key_password(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        if k.key == "backspace" {
            self.password.pop();
        } else if let Some(ch) = k.key_char.as_ref() {
            if !k.modifiers.control && !k.modifiers.alt && ch.chars().all(|c| !c.is_control()) {
                self.password.push_str(ch);
            }
        }
        cx.notify();
    }

    fn submit_password(&mut self, _: &SubmitPassword, _: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.status, Status::NeedsPassword { .. }) && !self.password.is_empty() {
            let pw = std::mem::take(&mut self.password);
            self.open(Some(pw), cx);
            cx.notify();
        }
    }

    fn render_status(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let st = Style::get(cx).clone();
        let placeholder = |icon_name: &str, title: String, body: String| {
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.0))
                .bg(st.c(|c| c.bg))
                .child(
                    svg()
                        .path(icon(icon_name))
                        .size(px(56.0))
                        .text_color(st.c(|c| c.ring)),
                )
                .child(
                    div()
                        .text_size(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(st.c(|c| c.text))
                        .child(title),
                )
                .child(
                    div()
                        .max_w(px(420.0))
                        .text_size(st.px(|m| m.ui_text))
                        .text_color(st.c(|c| c.muted))
                        .text_center()
                        .child(body),
                )
        };
        match &self.status {
            Status::Open => None,
            Status::Opening => Some(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(st.c(|c| c.desk))
                    .text_color(st.c(|c| c.faint))
                    .text_size(st.px(|m| m.ui_text))
                    .child("Opening…")
                    .into_any_element(),
            ),
            Status::Failed(e) => {
                let body = match e {
                    OpenError::NotFound => "The file isn't there any more. It may have been moved or deleted.".to_string(),
                    OpenError::Unsupported => "This isn't a document Kitab can read, or the file is damaged beyond repair.".to_string(),
                    OpenError::Failed(_) => "The file is damaged or uses something Kitab can't read yet.".to_string(),
                    _ => "Something went wrong while opening the file.".to_string(),
                };
                Some(
                    placeholder("alert", "Kitab can't open this file".into(), body)
                        .into_any_element(),
                )
            }
            Status::NeedsPassword { wrong } => {
                let wrong = *wrong;
                let dots: String = "•".repeat(self.password.chars().count());
                let focused = self.focus.is_focused(window);
                Some(
                    placeholder(
                        "lock",
                        "This document is password protected".into(),
                        "Enter the password to open it. Kitab doesn't keep it.".into(),
                    )
                    .child(
                        div()
                            .id("password")
                            .key_context(PROMPT)
                            .on_action(cx.listener(Self::submit_password))
                            .on_key_down(cx.listener(Self::on_key_password))
                            .mt(px(6.0))
                            .w(px(280.0))
                            .h(st.px(|m| m.control_height))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded(st.px(|m| m.control_radius))
                            .bg(st.c(|c| c.hover))
                            .shadow(st.well())
                            .border_1()
                            .border_color(if wrong {
                                st.c(|c| c.bad)
                            } else if focused {
                                st.c(|c| c.accent)
                            } else {
                                st.c(|c| c.line)
                            })
                            .text_color(st.c(|c| c.text))
                            .text_size(st.px(|m| m.ui_text))
                            .child(if dots.is_empty() {
                                div().text_color(st.c(|c| c.faint)).child("Password")
                            } else {
                                div().child(dots)
                            }),
                    )
                    .child(
                        div()
                            .h(px(18.0))
                            .text_size(px(12.0))
                            .text_color(st.c(|c| c.bad))
                            .child(if wrong {
                                "That password isn't right. Try again."
                            } else {
                                ""
                            }),
                    )
                    .into_any_element(),
                )
            }
        }
    }
}

impl Focusable for Reader {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Reader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let st = Style::get(cx).clone();
        // Free GPU atlas space of pages the cache let go of.
        for image in self.cache.take_evicted() {
            let _ = window.drop_image(image);
        }
        // Fit modes follow the viewport: re-measure when it changes.
        let vb = self.list.viewport_bounds();
        let vp = (f32::from(vb.size.width), f32::from(vb.size.height));
        if vp.0 > 0.0 && (vp.0 - self.viewport.0).abs() > 0.5
            || (vp.1 - self.viewport.1).abs() > 0.5 && vp.1 > 0.0
        {
            self.viewport = vp;
            if !matches!(self.zoom, ZoomMode::ActualSize | ZoomMode::Custom(_))
                && self.status == Status::Open
            {
                self.relayout(cx);
            }
        }
        // The list's bounds above are last frame's: after a resize (or the
        // sidebar opening) the new ones are known only once this frame is laid
        // out. Look once more after it and redraw if they moved, or a fit mode
        // would keep the old width until something else redraws (seen with
        // EPUB pages that were already cached). Idle frames redraw nothing.
        cx.on_next_frame(window, |r, _, cx| {
            let vb = r.list.viewport_bounds();
            let (w, h) = (f32::from(vb.size.width), f32::from(vb.size.height));
            let moved = (w - r.viewport.0).abs() > 0.5 || (h - r.viewport.1).abs() > 0.5;
            if w > 0.0 && h > 0.0 && moved {
                cx.notify();
            }
        });
        if self.is_book() {
            self.check_book_layout(cx);
        }
        if self.status == Status::Open && self.viewport.0 > 0.0 {
            if let Some(ReadingState {
                locator: Locator::Fixed { page, y, .. },
                ..
            }) = self.restore.take()
            {
                let page = (page as usize).min(self.page_count().saturating_sub(1));
                let offset = y * self.px_per_pt(page);
                self.list.scroll_to(ListOffset {
                    item_ix: page,
                    offset_in_item: px(offset),
                });
                self.last_page = page;
            }
        }
        let inset = st.top_inset();
        if (inset - self.top_inset).abs() > 0.5 {
            self.top_inset = inset;
            if self.status == Status::Open {
                self.relayout(cx);
            }
        }
        {
            let top = self.list.logical_scroll_top().item_ix;
            if top != self.last_top {
                self.scroll_dir = if top > self.last_top { 1 } else { -1 };
                self.last_top = top;
            }
            let lo = top as u64;
            let ppt = self.px_per_pt(lo as usize);
            let page_h = (self.page_size(lo as usize).h * ppt + GAP).max(1.0);
            let on_screen = (self.viewport.1 / page_h).ceil() as u64 + 1;
            let hi = (lo + on_screen).min(self.page_count().saturating_sub(1) as u64);
            self.visible
                .store((hi << 32) | lo, std::sync::atomic::Ordering::Relaxed);
        }
        let page = self.current_page();
        if page != self.last_page {
            self.last_page = page;
            cx.emit(ReaderEvent::PageChanged);
            self.schedule_save(cx);
        }

        if matches!(
            self.status,
            Status::NeedsPassword { .. } | Status::Failed(_)
        ) {
            crate::fire_first_paint(window, cx);
        }
        let status = self.render_status(window, cx);
        let this = cx.entity();
        div()
            .id("reader")
            .key_context(READER)
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .bg(if self.md.is_some() {
                st.c(|c| c.bg)
            } else {
                st.c(|c| c.desk)
            })
            .on_action(cx.listener(|r, _: &NextPage, _, cx| r.go_to_page(r.current_page() + 1, cx)))
            .on_action(cx.listener(|r, _: &PrevPage, _, cx| {
                r.go_to_page(r.current_page().saturating_sub(1), cx)
            }))
            .on_action(cx.listener(|r, _: &PageRight, _, cx| r.page_arrow(true, cx)))
            .on_action(cx.listener(|r, _: &PageLeft, _, cx| r.page_arrow(false, cx)))
            .on_action(cx.listener(|r, _: &FirstPage, _, cx| r.jump(0, cx)))
            .on_action(cx.listener(|r, _: &LastPage, _, cx| r.jump(usize::MAX, cx)))
            .on_action(cx.listener(|r, _: &GoToPage, window, cx| r.edit_page(window, cx)))
            .on_action(cx.listener(|r, _: &Find, window, cx| r.open_find(window, cx)))
            .on_action(cx.listener(|r, _: &ToggleEdit, window, cx| r.toggle_edit(window, cx)))
            .on_action(cx.listener(|r, _: &Copy, _, cx| r.copy_selection(cx)))
            .on_action(cx.listener(|r, _: &ClearSelection, _, cx| {
                // Esc puts the pen down first, then clears the selection.
                if r.markup.open {
                    r.toggle_markup(cx)
                } else {
                    r.clear_selection(cx)
                }
            }))
            .on_action(cx.listener(|r, _: &ToggleMarkup, _, cx| {
                if r.can_markup(cx) {
                    r.toggle_markup(cx)
                }
            }))
            .on_action(cx.listener(|r, _: &MarkupUndo, _, cx| r.markup_undo(cx)))
            .on_action(cx.listener(|r, _: &MarkupRedo, _, cx| r.markup_redo(cx)))
            .on_action(cx.listener(|r, _: &SaveMarks, _, cx| r.save_marks(cx)))
            .on_action(cx.listener(|r, _: &FindNext, window, cx| r.find_step(true, window, cx)))
            .on_action(cx.listener(|r, _: &FindPrev, window, cx| r.find_step(false, window, cx)))
            .on_action(cx.listener(|r, _: &HistoryBack, _, cx| r.history(true, cx)))
            .on_action(cx.listener(|r, _: &HistoryForward, _, cx| r.history(false, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|r, e: &MouseDownEvent, _, cx| {
                    if r.status == Status::Open {
                        cx.emit(ReaderEvent::ContextMenu(e.position));
                    }
                }),
            )
            // The mouse's back and forward buttons follow history, as in a browser.
            .on_mouse_down(
                MouseButton::Navigate(gpui::NavigationDirection::Back),
                cx.listener(|r, _, _, cx| r.history(true, cx)),
            )
            .on_mouse_down(
                MouseButton::Navigate(gpui::NavigationDirection::Forward),
                cx.listener(|r, _, _, cx| r.history(false, cx)),
            )
            .on_key_down(cx.listener(|r, _: &gpui::KeyDownEvent, _, cx| r.wake_dock(cx)))
            .on_mouse_move(cx.listener(|r, e: &MouseMoveEvent, _, cx| {
                r.wake_dock(cx);
                if r.markup.drawing.is_some() {
                    if e.pressed_button == Some(MouseButton::Left) {
                        r.markup_move(e.position, e.modifiers.shift, cx);
                    } else {
                        r.markup_up(cx);
                    }
                }
                if r.select.dragging {
                    if e.pressed_button == Some(MouseButton::Left) {
                        r.select_move(e.position, cx);
                    } else {
                        r.select_up();
                    }
                }
                if r.dragging {
                    if e.pressed_button == Some(MouseButton::Left) {
                        r.seek_x(e.position.x, cx);
                    } else {
                        r.dragging = false;
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|r, _, _, cx| {
                    r.dragging = false;
                    r.select_up();
                    r.markup_up(cx);
                }),
            )
            .on_action(cx.listener(|r, _: &ScrollDown, _, cx| {
                r.list.scroll_by(px(64.0));
                cx.notify()
            }))
            .on_action(cx.listener(|r, _: &ScrollUp, _, cx| {
                r.list.scroll_by(px(-64.0));
                cx.notify()
            }))
            .on_action(cx.listener(|r, _: &ScreenDown, _, cx| {
                r.list.scroll_by(px(r.viewport.1 * 0.9));
                cx.notify()
            }))
            .on_action(cx.listener(|r, _: &ScreenUp, _, cx| {
                r.list.scroll_by(px(-r.viewport.1 * 0.9));
                cx.notify()
            }))
            .on_action(cx.listener(|r, _: &ZoomIn, _, cx| r.step_zoom(true, cx)))
            .on_action(cx.listener(|r, _: &ZoomOut, _, cx| r.step_zoom(false, cx)))
            .on_action(cx.listener(|r, _: &FitPage, _, cx| r.set_zoom(ZoomMode::FitPage, cx)))
            .on_action(cx.listener(|r, _: &FitWidth, _, cx| r.set_zoom(ZoomMode::FitWidth, cx)))
            .on_action(cx.listener(|r, _: &ActualSize, _, cx| r.set_zoom(ZoomMode::ActualSize, cx)))
            .on_action(cx.listener(|r, _: &RotateRight, _, cx| r.rotate(true, cx)))
            .on_action(cx.listener(|r, _: &RotateLeft, _, cx| r.rotate(false, cx)))
            .child({
                let pages = list(self.list.clone(), move |ix, window, cx| {
                    this.update(cx, |r, cx| r.page_element(ix, window, cx))
                })
                .size_full();
                // Editing Markdown: the source beside the live preview.
                match self.editor_view(&st, window, cx) {
                    Some(editor) => div()
                        .flex()
                        .size_full()
                        .child(div().w(gpui::relative(0.5)).h_full().child(editor))
                        .child(div().flex_1().min_w_0().h_full().relative().child(pages))
                        .into_any_element(),
                    None => pages.into_any_element(),
                }
            })
            // Ctrl+wheel zooms. This layer is painted after the list so it sees the
            // wheel first; it has no other handlers, so clicks pass through.
            .child(
                div()
                    .id("wheel-zoom")
                    .absolute()
                    .inset_0()
                    .on_scroll_wheel(cx.listener(|r, e: &ScrollWheelEvent, _, cx| {
                        if e.modifiers.control || e.modifiers.platform {
                            let dy = f32::from(e.delta.pixel_delta(px(20.0)).y);
                            if dy.abs() > 0.5 {
                                r.step_zoom(dy > 0.0, cx);
                            }
                            cx.stop_propagation();
                        }
                    })),
            )
            .children(status)
            .children(self.page_nav(&st, window, cx))
            .children(self.dock_view(&st, inset, window, cx))
            .children(self.edit_button(&st, cx))
            .children(self.find_bar(&st, window, cx))
            .children(self.markup_bar(&st, window, cx))
            .children(self.info_chip(&st))
    }
}

impl Reader {
    /// The page navigator chosen in Settings (DESIGN.md § Scrubber). With
    /// "Hidden", only the Ctrl+G field shows, while it is open.
    /// The navigator's pieces for the chosen style: the page field with a
    /// track, steppers or a total. `None` when nothing shows (Hidden, not editing).
    pub(super) fn nav_parts(
        &self,
        st: &Style,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Vec<AnyElement>> {
        let count = self.page_count().max(1);
        if self.md.is_some() {
            if st.page_nav == PageNav::Hidden {
                return None;
            }
            let at = self.current_page();
            let percent = if count > 1 {
                (at as f32 / (count - 1) as f32 * 100.0).round() as u32
            } else {
                100
            };
            return Some(vec![
                self.track_element(st, cx),
                div().child(format!("{percent} %")).into_any_element(),
            ]);
        }
        let field = self.page_field(st, window, cx);
        Some(match st.page_nav {
            PageNav::Scrubber => vec![
                field,
                self.track_element(st, cx),
                div().child(format!("{count}")).into_any_element(),
            ],
            PageNav::Stepper => vec![
                nav_button("prev-page", "chevron-left", st)
                    .tip("Previous page  Left")
                    .on_click(cx.listener(|r, _, _, cx| r.prev_page(cx)))
                    .into_any_element(),
                field,
                div().child(format!("of {count}")).into_any_element(),
                nav_button("next-page", "chevron-right", st)
                    .tip("Next page  Right")
                    .on_click(cx.listener(|r, _, _, cx| r.next_page(cx)))
                    .into_any_element(),
            ],
            PageNav::Counter => vec![field, div().child(format!("/ {count}")).into_any_element()],
            PageNav::Hidden => {
                self.page_edit.as_ref()?;
                vec![
                    div().child("Go to page").into_any_element(),
                    field,
                    div().child(format!("of {count}")).into_any_element(),
                ]
            }
        })
    }

    /// The page navigator chosen in Settings (DESIGN.md § Scrubber), as its own
    /// floating pill. With the dock at the bottom it lives in the dock instead.
    fn page_nav(&self, st: &Style, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        // At the bottom the navigator is part of the dock, unless minimal mode
        // hides the dock but keeps the navigator.
        let in_dock = st.dock == DockAt::Bottom && st.shows_dock();
        if self.status != Status::Open || in_dock || !st.shows_page_nav() {
            return None;
        }
        let parts = self.nav_parts(st, window, cx)?;
        let corner = st.page_nav == PageNav::Counter;
        let body = div()
            .flex()
            .items_center()
            .gap(px(if corner { 4.0 } else { 10.0 }))
            .h(px(if corner { 30.0 } else { 36.0 }))
            .px(px(if corner { 6.0 } else { 12.0 }))
            .rounded_full()
            .bg(st.c(|c| c.raised))
            .shadow(st.float())
            .text_size(px(12.0))
            .text_color(st.c(|c| c.muted))
            .children(parts);
        let body = motion::enter(
            body.relative(),
            ("page-nav", st.page_nav as usize),
            st.animations,
            motion::From::Below(8.0),
            motion::BASE,
            std::time::Duration::ZERO,
        );
        let place = div().absolute().bottom(px(18.0));
        Some(
            if corner {
                place.right(px(22.0)).child(body)
            } else {
                place.left_0().right_0().flex().justify_center().child(body)
            }
            .into_any_element(),
        )
    }

    /// The current page as a field: click it (or Ctrl+G) and type a number.
    /// Enter goes there; Escape or clicking away cancels.
    fn page_field(&self, st: &Style, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let editing = self.page_edit.is_some();
        let focused = self.page_focus.is_focused(window);
        let typed = self.page_edit.clone().unwrap_or_default();
        let current = format!("{}", self.current_page() + 1);
        div()
            .id("page-field")
            .key_context(PAGE_FIELD)
            .track_focus(&self.page_focus)
            .on_action(cx.listener(|r, _: &ConfirmPage, window, cx| r.confirm_page(window, cx)))
            .on_action(cx.listener(|r, _: &CancelPage, window, cx| r.cancel_page(window, cx)))
            .on_key_down(cx.listener(Self::on_key_page))
            .on_click(cx.listener(|r, _, window, cx| {
                if r.page_edit.is_none() {
                    r.edit_page(window, cx);
                }
            }))
            .min_w(px(40.0))
            .h(px(24.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .text_size(px(12.0))
            .text_color(st.c(|c| c.text))
            .cursor_text()
            .border_1()
            .border_color(if editing && focused {
                st.c(|c| c.accent)
            } else {
                st.ca(|c| c.line, 0.0)
            })
            .when(editing, |d| d.bg(st.c(|c| c.hover)).shadow(st.well()))
            .when(!editing, |d| d.hover(|d| d.bg(st.c(|c| c.hover))))
            .tip("Go to page  Ctrl+G")
            .child(if editing {
                div()
                    .flex()
                    .items_center()
                    .child(if typed.is_empty() {
                        div().text_color(st.c(|c| c.faint)).child(current)
                    } else {
                        div().child(typed)
                    })
                    // Caret.
                    .child(
                        div()
                            .ml(px(1.0))
                            .w(px(1.0))
                            .h(px(14.0))
                            .bg(st.c(|c| c.accent)),
                    )
            } else {
                div().child(current)
            })
            .into_any_element()
    }

    /// The scrubber track: a well with the reading position. Click to jump,
    /// drag to scrub.
    fn track_element(&self, st: &Style, cx: &mut Context<Self>) -> AnyElement {
        let count = self.page_count().max(1);
        let page = self.current_page();
        let fraction = if count > 1 {
            page as f32 / (count - 1) as f32
        } else {
            0.0
        };
        let track_w = 220.0;
        let cell = self.track.clone();
        div()
            .id("track")
            .relative()
            .w(px(track_w))
            .h(px(20.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|r, e: &MouseDownEvent, _, cx| {
                    r.dragging = true;
                    r.seek_x(e.position.x, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(6.0))
                    .rounded_full()
                    .bg(st.c(|c| c.hover))
                    .shadow(st.well())
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(px(track_w * fraction))
                            .rounded_full()
                            .bg(st.c(|c| c.accent)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(-4.0))
                            .left(px(track_w * fraction - 7.0))
                            .size(px(14.0))
                            .rounded_full()
                            .bg(st.c(|c| c.text))
                            .shadow(st.elev()),
                    ),
            )
            .into_any_element()
    }
}

fn nav_button(id: &'static str, name: &str, st: &Style) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .hover(|d| d.bg(st.c(|c| c.hover)))
        .child(
            svg()
                .path(icon(name))
                .size(px(15.0))
                .text_color(st.c(|c| c.muted)),
        )
}

/// Encode a rendered page (BGRA) as PNG.
pub fn png_from_bgra(width: u32, height: u32, mut bgra: Vec<u8>) -> Option<Vec<u8>> {
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let mut out = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut out),
        &bgra,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )
    .ok()?;
    Some(out)
}

/// A typed page number (1-based) as a page index, clamped to the document.
fn parse_page(text: &str, count: usize) -> Option<usize> {
    let n: usize = text.trim().parse().ok()?;
    (n >= 1 && count > 0).then(|| (n - 1).min(count - 1))
}

/// Create a reader for `path` in its own entity.
pub fn open_reader(
    path: PathBuf,
    engine: Arc<dyn DocEngine>,
    library: Arc<dyn Library>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Reader> {
    cx.new(|cx| Reader::new(path, engine, library, window, cx))
}

/// A code token's colour with the current code palette (Settings previews).
pub fn code_color(st: &Style, kind: kitab_md::TokenKind) -> gpui::Hsla {
    markdown::token_color(st, kind)
}

/// Files Kitab reads as Markdown.
pub fn is_markdown_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| matches!(e.as_str(), "md" | "markdown" | "mdown" | "mkd" | "mkdn"))
}

#[cfg(test)]
mod page_field_tests {
    use super::parse_page;

    #[test]
    fn page_numbers_are_one_based_and_clamped() {
        assert_eq!(parse_page("1", 40), Some(0));
        assert_eq!(parse_page(" 12 ", 40), Some(11));
        assert_eq!(parse_page("999", 40), Some(39));
        assert_eq!(parse_page("0", 40), None);
        assert_eq!(parse_page("", 40), None);
        assert_eq!(parse_page("3", 0), None);
    }
}

/// Whether an arrow key moves forward through the book.
fn page_arrow_goes_forward(right: bool, right_to_left: bool) -> bool {
    right != right_to_left
}

#[cfg(test)]
mod arrow_tests {
    #[test]
    fn arrows_follow_the_reading_direction() {
        use super::page_arrow_goes_forward as forward;
        assert!(forward(true, false) && !forward(false, false));
        // Right to left: the left arrow is the next page.
        assert!(forward(false, true) && !forward(true, true));
    }
}
