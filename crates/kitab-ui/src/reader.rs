// SPDX-License-Identifier: AGPL-3.0-or-later

//! One open document: true-size page sheets in a virtualised list (only visible
//! pages render), zoom modes, rotation, keyboard navigation, the password prompt
//! and the error placeholders (PLAN.md Phase 1, DESIGN.md § Page sheets).

mod book;
mod bookmarks;
mod dock;
pub(crate) use dock::note_toggle as note_dock_toggle;
pub(crate) use dock::set_menu_open;
pub(crate) use markdown::MONO_FONT;
mod editor;
mod find;
mod footnote;
mod form;
pub use bookmarks::BookmarkRow;
pub use dock::TOP_INSET;
pub use info::human_size;
pub use marks_list::{MarkRow, MarkSource};
mod geom;
mod info;
mod layer;
mod live;
mod markdown;
mod marks_list;
mod markup;
mod math_view;
mod md_select;

/// A code block's scroll bar thumb being dragged: which block, where the
/// pointer and the scroll were when it was grabbed.
#[derive(Clone, Copy)]
pub(crate) struct HDrag {
    key: u64,
    from_x: f32,
    from_offset: f32,
}
mod mermaid;
mod pages;
mod park;
mod place_sig;
mod present;
mod scrollbar;
mod select;
mod signatures;
mod speech;
pub(crate) use signatures::check_words;
pub(crate) use speech::{load_voices, preview as preview_voice, voices};

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
/// Rendered pages kept per tab (bytes), at least.
// The floor of the rendered-page cache. It always grows to hold the pages
// on screen and the next three (see the budget in `layout`), so this only
// decides how many pages further away are kept: 32 MB is a few at a typical
// size. 96 MB kept up to ~16 pages that were drawn again in 20 ms anyway,
// each costing RAM for its pixels as well as GPU memory.
const CACHE_BUDGET: usize = 32 * 1024 * 1024;
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
    /// Markdown's typography panel (typeface and text size), at the dock's button.
    TypeMenu(Point<Pixels>),
    /// A link in a Markdown document points at another file: open it in a tab.
    OpenPath(PathBuf),
    /// Zoom in a book changed the text size (points), for every book.
    BookText(f32),
    /// Presentation mode started or ended (the workspace goes full screen).
    Presenting(bool),
    /// The dock's Print button.
    Print,
    /// The dock's Password button.
    Password,
    /// The signatures were verified (or verified again).
    Signatures,
    /// Out of sight, it let go of rendered pages: the workspace frees
    /// their GPU memory (`Reader::take_released`).
    Released,
    /// The status bar's signature badge: show the Signatures pane.
    ShowSignatures,
    /// Sign…: the page and box (page points from the top-left) the user
    /// clicked, or `None` when placing was cancelled.
    SignaturePlaced(Option<(u32, [f32; 4])>),
    /// Something went wrong that the reader should hear about (a toast).
    Message(String),
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
    /// Pages painted from pixels in memory during the last frame: their
    /// textures are on the GPU now, so the copies in memory can go.
    painted: Rc<std::cell::RefCell<Vec<PageKey>>>,
    /// `gpui::kitab_image_misses` when this tab last looked.
    seen_misses: usize,
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
    /// The password prompt's field; focused when the prompt appears.
    password_focus: FocusHandle,
    focus_password: bool,
    /// The field's text while it is being edited.
    page_edit: Option<String>,
    /// The scrubber track's bounds from the last frame, for clicks and drags.
    track: Rc<Cell<Option<Bounds<Pixels>>>>,
    dragging: bool,
    /// The scroll bar's bounds from the last frame.
    bar: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Dragging the scroll bar: where the pointer holds the thumb (pixels
    /// from its top).
    bar_grab: Option<f32>,
    /// Middle-click scrolling, and which run of it is current.
    autoscroll: Option<crate::autoscroll::AutoScroll>,
    autoscroll_serial: u64,
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
    /// The reader has scrolled since opening: from then on the next page is
    /// rendered ahead. Opening renders only what shows (memory, CPU).
    scrolled: bool,
    /// Smooth scrolling under way: how far is left to go.
    glide: Option<Glide>,
    /// A pinch is under way until then: pages stretch to the new size and
    /// are drawn sharp once it settles, not at every step of the pinch.
    pinch_until: Option<std::time::Instant>,
    pinch_serial: u64,
    last_frame: Option<std::time::Instant>,
    /// The password that opened this document, if it needed one (memory only).
    unlock: Option<String>,
    find: find::FindState,
    layers: layer::Layers,
    select: select::SelectState,
    /// Text selection in Markdown (md_select.rs).
    md_sel: md_select::MdSelect,
    /// A code block's sideways scroll bar being dragged.
    md_hdrag: Option<HDrag>,
    /// Markdown's text scale when the file has no zoom of its own
    /// (Settings › Reading › Markdown text size); followed each frame.
    md_base: f32,
    dock: dock::DockState,
    /// Room kept free above the first page (sidebar tabs: the window buttons).
    top_inset: f32,
    /// A Markdown document: its blocks are the list's items instead of pages.
    md: Option<markdown::MdState>,
    /// A book's layout (EPUB, MOBI, FB2, TXT); `None` for files with pages.
    book: Option<book::BookState>,
    /// Drawing and markup on the pages.
    markup: markup::MarkupState,
    /// The PDF's form, as it is being filled in.
    form: form::FormState,
    /// The file's size in bytes, for the info chip (read off the UI thread).
    file_size: Option<u64>,
    /// The PDF's signatures, verified off the UI thread after opening.
    signatures: Option<Vec<kitab_core::SignatureCheck>>,
    /// Sign…: the next click on a page places the signature.
    placing: bool,
    /// A page to show once the document is laid out (a library search hit).
    pending_page: Option<u32>,
    /// The book style the document was opened in.
    opened_style: kitab_core::BookStyle,
    /// The anchor page's scale (×1000) and rotation at the last relayout:
    /// when they haven't changed, renders in flight are still right.
    layout_sig: Option<(i64, u16)>,
    /// The annotations already in the file, once the Marks panel asked.
    file_annots: Option<Vec<kitab_core::FileAnnot>>,
    /// Bookmarked pages (Ctrl+D), in page order.
    bookmark_rows: Vec<bookmarks::BookmarkRow>,
    /// The sidebar's Pages pane (thumbnails).
    pages: pages::PagesState,
    /// Presentation mode (F5), when on.
    present: Option<present::Present>,
    /// This is the tab showing (Markdown live reload looks at its file).
    watched: bool,
    /// A footnote shown beside its mark.
    note: Option<footnote::NotePop>,
    /// Bumped to end a live-reload loop.
    watch_gen: u64,
    /// Out of sight (another tab shows): it keeps only what it showed, and
    /// renders still arriving are let go (see `park`).
    parked: bool,
    /// Smaller stand-ins of its pages are being made (`park`).
    shrinking: bool,
    /// The window's scale factor at the last frame.
    scale_factor: f32,
    /// Read aloud.
    speech: speech::SpeechState,
}

/// Smooth scrolling under way.
struct Glide {
    /// Pixels still to scroll (down is positive).
    left: f32,
    /// When the last step was taken.
    at: std::time::Instant,
}

/// A pinch counts as over when the fingers have been still this long.
const PINCH_SETTLE: Duration = Duration::from_millis(140);

/// Smooth scrolling covers 63 % of what is left in this many seconds.
const GLIDE_TIME: f32 = 0.045;

/// What a page render is for: the reading view (a quick preview or the sharp
/// page) or a thumbnail in the Pages pane, with its own test for whether it
/// is still wanted.
enum RenderFor {
    Page { quick: bool },
    Thumbnail(crate::engine::Wanted),
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
            painted: Rc::default(),
            seen_misses: gpui::kitab_image_misses(),
            pending: HashSet::new(),
            generation: 1,
            focus,
            password: String::new(),
            viewport: (0.0, 0.0),
            last_page: 0,
            save_scheduled: false,
            restore: None,
            page_focus,
            password_focus: cx.focus_handle(),
            focus_password: false,
            page_edit: None,
            track: Rc::new(Cell::new(None)),
            dragging: false,
            bar: Rc::new(Cell::new(None)),
            bar_grab: None,
            autoscroll: None,
            autoscroll_serial: 0,
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
            scrolled: false,
            glide: None,
            pinch_until: None,
            pinch_serial: 0,
            last_frame: None,
            unlock: None,
            find: find::FindState::new(cx.focus_handle()),
            layers: layer::Layers::default(),
            select: select::SelectState::default(),
            md_sel: md_select::MdSelect::default(),
            md_hdrag: None,
            md_base: f32::from(Style::get(cx).md_text) / 100.0,
            dock: dock::DockState::default(),
            top_inset: 0.0,
            md: None,
            book: None,
            markup: markup::MarkupState::default(),
            form: form::FormState::default(),
            file_size: None,
            signatures: None,
            placing: false,
            pending_page: None,
            opened_style: kitab_core::BookStyle::DEFAULT,
            layout_sig: None,
            file_annots: None,
            bookmark_rows: Vec::new(),
            pages: pages::PagesState::default(),
            present: None,
            watched: false,
            note: None,
            watch_gen: 0,
            parked: false,
            shrinking: false,
            scale_factor: window.scale_factor(),
            speech: Default::default(),
        };
        this.load_file_size(cx);
        this.open(None, cx);
        this
    }

    /// The file name without its extension (the tab icon shows the type):
    /// what tabs, the window title and dialogs call the document. The embedded title is often missing, stale ("Microsoft
    /// Word - draft3.doc") or typed in a legacy font, so it only shows as
    /// [`Self::doc_title`].
    pub fn title(&self) -> SharedString {
        let name = self
            .path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        SharedString::from(name)
    }

    /// The document's own title, when it has a readable one that says more
    /// than the file name.
    pub fn doc_title(&self) -> Option<String> {
        let t = self.info.as_ref()?.title.as_deref()?;
        let t = kitab_core::legible(t)?;
        let stem = self.path.file_stem().map(|s| s.to_string_lossy());
        (stem.as_deref() != Some(t)).then(|| t.to_string())
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

    /// The page you are reading: the one taking up most of the view, or the
    /// last page once the scroll reaches the end (it may never fill the view).
    /// Not the page whose top is in view: a thin strip of the previous page
    /// at the top would otherwise count.
    pub fn current_page(&self) -> usize {
        let last = self.page_count().saturating_sub(1);
        let top = self.list.logical_scroll_top().item_ix.min(last);
        let view = self.list.viewport_bounds();
        if view.size.height <= px(0.0) {
            return top;
        }
        let (lo, hi) = (view.top() + px(self.top_inset), view.bottom());
        let whole = |ix: usize| {
            self.list
                .bounds_for_item(ix)
                .is_some_and(|b| b.top() >= lo - px(1.0) && b.bottom() <= hi + px(1.0))
        };
        // Scrolled to the end with the whole last page in view (and not the
        // whole page before it too, or Previous could never leave it).
        let max = self.list.max_offset_for_scrollbar().height;
        let at = -self.list.scroll_px_offset_for_scrollbar().y;
        if last > 0 && max > px(1.0) && at >= max - px(1.0) && whole(last) && !whole(last - 1) {
            return last;
        }
        let mut best = (top, px(0.0));
        for ix in top..=last {
            let Some(b) = self.list.bounds_for_item(ix) else {
                break;
            };
            if b.top() >= hi {
                break;
            }
            let seen = b.bottom().min(hi) - b.top().max(lo);
            if seen > best.1 {
                best = (ix, seen);
            }
        }
        best.0
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
        let style = Self::book_style(cx);
        self.opened_style = style;
        let path = self.path.clone();
        let tried = password.clone();
        cx.spawn(async move |this, cx| {
            let (result, saved) = cx
                .background_spawn(async move {
                    kitab_os::trace::mark("reader: open");
                    let result = engine.open_styled(&path, password.as_deref(), style);
                    kitab_os::trace::mark("reader: opened");
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
                    let mtime = meta.modified().ok();
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
                    Ok((doc, info, saved, text.into_owned(), mtime))
                })
                .await;
            let _ = this.update(cx, |this, cx| match loaded {
                Ok((doc, info, saved, source, mtime)) => {
                    this.restore = saved;
                    let dir = this
                        .path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_default();
                    this.md_sel.reset();
                    this.md = Some(markdown::MdState {
                        block_lines: doc.block_lines.clone(),
                        doc: Arc::new(doc),
                        dir,
                        open_details: Default::default(),
                        source: source.into(),
                        edit: None,
                        mtime,
                        diagrams: Default::default(),
                        code_scroll: Default::default(),
                    });
                    this.start_watch(cx);
                    if let Some(state) = this.restore {
                        this.zoom = state.zoom;
                    }
                    this.list.reset(info.page_count as usize);
                    this.info = Some(info);
                    this.status = Status::Open;
                    this.index_text_later(cx);
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
                } else if matches!(
                    info.format,
                    kitab_core::Format::Pdf | kitab_core::Format::Xps | kitab_core::Format::Cbz
                ) {
                    // Opened for the first time: sized as Settings › Reading says.
                    self.zoom = Style::get(cx).open_zoom.mode();
                }
                self.list.reset(info.page_count as usize);
                self.doc = Some(doc);
                self.info = Some(info);
                self.status = Status::Open;
                self.password.clear();
                self.load_marks(cx);
                self.load_bookmarks(cx);
                self.check_signatures(cx);
                self.index_text_later(cx);
                // Opened again after a save that failed: the fields as filled in.
                self.reapply_fields(cx);
                cx.emit(ReaderEvent::Opened);
            }
            Err(OpenError::NeedsPassword) => {
                self.status = Status::NeedsPassword { wrong: false };
                self.focus_password = true;
            }
            Err(OpenError::WrongPassword) => {
                self.status = Status::NeedsPassword { wrong: true };
                self.focus_password = true;
            }
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
        // Only a new scale or rotation makes queued renders stale. A taller
        // or shorter view, or the title bar settling at start-up, keeps them
        // (each page was otherwise rendered twice or three times on open).
        let sig = (
            (self.px_per_pt(page) * 1000.0).round() as i64,
            self.rotation,
        );
        if self.layout_sig != Some(sig) {
            self.layout_sig = Some(sig);
            self.generation += 1;
            if let Some(doc) = self.doc {
                let (engine, generation) = (self.engine.clone(), self.generation);
                cx.background_spawn(async move { engine.set_generation(doc, generation) })
                    .detach();
            }
            self.pending.clear();
        }
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

    /// A pinch (touch screen or touchpad): zoom by `factor`, continuously.
    /// The pages on screen stretch as the fingers move; the sharp ones are
    /// drawn once the pinch has been still for a moment.
    fn pinch_zoom(&mut self, factor: f32, cx: &mut Context<Self>) {
        // A book's page always fits the window (its zoom is the text size).
        if self.is_book() || !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let current = self.px_per_pt(self.current_page()) / PX_PER_PT_AT_100;
        let next = (current * factor).clamp(ZOOM_STEPS[0], ZOOM_STEPS[ZOOM_STEPS.len() - 1]);
        if (next - current).abs() < 0.0005 {
            return;
        }
        self.pinch_until = Some(std::time::Instant::now() + PINCH_SETTLE);
        self.pinch_serial += 1;
        let serial = self.pinch_serial;
        self.set_zoom(ZoomMode::Custom(next), cx);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PINCH_SETTLE).await;
            let _ = this.update(cx, |r, cx| {
                if r.pinch_serial == serial {
                    r.pinch_until = None;
                    cx.notify();
                }
            });
        })
        .detach();
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

    /// Show `page` now, or as soon as the document is open and laid out.
    pub fn show_page(&mut self, page: u32, cx: &mut Context<Self>) {
        if self.status == Status::Open && self.viewport.0 > 0.0 {
            self.jump(page as usize, cx);
        } else {
            self.pending_page = Some(page);
            self.restore = None;
        }
        cx.notify();
    }

    /// Keep the document's text for library search, a little after it
    /// opens and off the UI thread; skipped when the file hasn't changed
    /// since it was last read.
    fn index_text_later(&mut self, cx: &mut Context<Self>) {
        let (engine, library, path) =
            (self.engine.clone(), self.library.clone(), self.path.clone());
        let doc = self.doc;
        let pages = self.page_count().min(5_000) as u32;
        let source = self.md.as_ref().map(|m| m.source.to_string());
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(3))
                .await;
            cx.background_spawn(async move {
                let mtime = std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis() as i64);
                if !library.needs_text(&path, mtime) {
                    return;
                }
                let texts: Vec<String> = match (source, doc) {
                    (Some(src), _) => vec![src],
                    (None, Some(doc)) => {
                        let mut out = Vec::with_capacity(pages as usize);
                        for p in 0..pages {
                            match engine.page_text(doc, p) {
                                Some(t) => out.push(t),
                                // Closed while reading it: try next time.
                                None if p > 0 => return,
                                None => out.push(String::new()),
                            }
                        }
                        out
                    }
                    (None, None) => return,
                };
                library.index_text(&path, mtime, texts);
            })
            .await;
        })
        .detach();
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
        self.scroll_to_fraction(f, cx);
    }

    /// Scroll by `dy` pixels (down is positive): gliding there over a few
    /// frames when Settings › Reading › Smooth scrolling is on, at once
    /// otherwise. More turns of the wheel while it glides add up.
    fn glide_by(&mut self, dy: f32, cx: &mut Context<Self>) {
        let st = Style::get(cx);
        if st.smooth_scroll && st.animations {
            let glide = self.glide.get_or_insert(Glide {
                left: 0.0,
                // As if a frame had just gone by, so it moves on the very
                // next frame.
                at: std::time::Instant::now() - Duration::from_millis(16),
            });
            glide.left += dy;
        } else {
            self.list.scroll_by(px(dy));
        }
        self.scrolled = true;
        cx.notify();
    }

    /// One frame of smooth scrolling: cover a share of what is left that
    /// depends only on the time gone by (the same glide at 60 and 144 Hz),
    /// fast at first and easing out; about 150 ms in all.
    fn glide_step(&mut self, window: &mut Window) {
        let Some(glide) = self.glide.as_mut() else {
            return;
        };
        let now = std::time::Instant::now();
        let dt = (now - glide.at).as_secs_f32().min(0.05);
        glide.at = now;
        let mut step = glide.left * (1.0 - (-dt / GLIDE_TIME).exp());
        if step.abs() < 1.0 {
            step = glide.left.signum() * glide.left.abs().min(1.0);
        }
        glide.left -= step;
        let done = glide.left.abs() < 0.5;
        self.list.scroll_by(px(step));
        if done {
            self.glide = None;
        } else {
            window.request_animation_frame();
        }
    }

    /// Start middle-click scrolling at `at`, or end it.
    fn toggle_autoscroll(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.autoscroll.take().is_some() {
            cx.notify();
            return;
        }
        self.autoscroll = Some(crate::autoscroll::AutoScroll::new(at));
        self.autoscroll_serial += 1;
        let serial = self.autoscroll_serial;
        cx.notify();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(crate::autoscroll::TICK)
                    .await;
                let going = this
                    .update(cx, |r, cx| {
                        let Some(a) = r
                            .autoscroll
                            .as_ref()
                            .filter(|_| r.autoscroll_serial == serial)
                        else {
                            return false;
                        };
                        let step = a.step();
                        if step != 0.0 {
                            r.list.scroll_by(px(step));
                            r.scrolled = true;
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !going {
                    break;
                }
            }
        })
        .detach();
    }

    /// The password that opened the document, for tools that reopen the file.
    pub fn unlock(&self) -> Option<String> {
        self.unlock.clone()
    }

    /// A Markdown document (blocks, not pages).
    /// Scroll the Pages pane to the current page next time it shows.
    pub fn reveal_in_pages(&mut self) {
        self.pages.reveal = true;
    }

    /// What printing needs: the open document and its page sizes, or why
    /// this tab can't print.
    pub fn print_source(&self) -> Result<(DocHandle, Vec<SizePt>), &'static str> {
        if self.md.is_some() {
            // Laid out on paper first (Workspace::prepare_md_print).
            return Err("Markdown is laid out for paper before it prints.");
        }
        let (Some(doc), Some(info)) = (self.doc, self.info.as_ref()) else {
            return Err("The document isn't open yet.");
        };
        if self.status != Status::Open {
            return Err("The document isn't open yet.");
        }
        if !info.permissions.print {
            return Err("This PDF's owner doesn't allow printing it.");
        }
        let sizes = (0..self.page_count())
            .map(|i| self.page_size_pt(i))
            .collect();
        Ok((doc, sizes))
    }

    /// The parsed Markdown, for printing.
    pub fn markdown_doc(&self) -> Option<std::sync::Arc<kitab_md::MdDoc>> {
        self.md.as_ref().map(|m| m.doc.clone())
    }

    pub fn is_markdown(&self) -> bool {
        self.md.is_some()
    }

    pub fn is_pdf(&self) -> bool {
        self.info
            .as_ref()
            .is_some_and(|i| i.format == kitab_core::Format::Pdf)
    }

    /// A PDF's lock: whether it has a password (or an owner's lock), and
    /// whether its owner restricts what this unlock may do. `None` for other
    /// formats.
    pub fn lock_state(&self) -> Option<(bool, bool)> {
        let i = self
            .info
            .as_ref()
            .filter(|i| i.format == kitab_core::Format::Pdf)?;
        let p = &i.permissions;
        let full = p.print && p.copy && p.annotate && p.fill_forms && p.modify && p.assemble;
        Some((i.encryption.is_some(), !full))
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
        self.request_for(ix, key, scale, RenderFor::Page { quick }, cx)
    }

    fn request_for(
        &mut self,
        ix: usize,
        key: PageKey,
        scale: f32,
        what: RenderFor,
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
        let page = ix as u32;
        // Only the reading view's renders feed the preview timing.
        let (wanted, quick): (crate::engine::Wanted, Option<bool>) = match what {
            RenderFor::Page { quick } => {
                let visible = self.visible.clone();
                let wanted: crate::engine::Wanted = Arc::new(move || {
                    let v = visible.load(std::sync::atomic::Ordering::Relaxed);
                    let (lo, hi) = ((v & 0xFFFF_FFFF) as u32, (v >> 32) as u32);
                    page + WANTED_MARGIN >= lo && page <= hi + WANTED_MARGIN
                });
                (wanted, Some(quick))
            }
            RenderFor::Thumbnail(wanted) => (wanted, None),
        };
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
                if let (Ok(Some(_)), Some(quick)) = (&result, quick) {
                    let avg = if quick {
                        &mut this.quick_ms
                    } else {
                        &mut this.sharp_ms
                    };
                    *avg = Some(avg.map_or(took, |a| a * 0.7 + took * 0.3));
                }
                match result {
                    Ok(Some(bmp)) => {
                        kitab_os::trace::mark(&format!(
                            "page {page} rendered ({}x{})",
                            bmp.width, bmp.height
                        ));
                        if let Some(since) = this.blank_since.remove(&page) {
                            eprintln!("blank page={page} ms={}", since.elapsed().as_millis());
                        }
                        let bytes = bmp.byte_size();
                        if this.parked {
                            // The tab went out of sight meanwhile: it keeps
                            // only what it showed (it renders again on return).
                        } else if let Some(buffer) =
                            RgbaImage::from_raw(bmp.width, bmp.height, bmp.bgra)
                        {
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

    /// Pages painted from pixels in memory are on the GPU now: let the
    /// copies in memory go (see the `painted` field).
    fn slim_painted(&mut self) {
        let keys: Vec<PageKey> = self.painted.borrow_mut().drain(..).collect();
        for key in keys {
            self.cache.slim(&key);
        }
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
        let pinching = self
            .pinch_until
            .is_some_and(|t| std::time::Instant::now() < t);
        let image = match self.cache.get(&key) {
            Some(i) => {
                crate::fire_first_paint(window, cx);
                self.ensure_links(ix as u32, cx);
                self.ensure_fields(ix as u32, cx);
                Some(i)
            }
            None => {
                let any = self.cache.any_for_page(ix as u32, self.rotation);
                // A preview only pays when it is clearly faster than the page.
                let preview_pays = match (self.quick_ms, self.sharp_ms) {
                    (Some(q), Some(s)) => q * 2.0 < s,
                    // Sharp pages are slow here: a soft preview first.
                    (None, Some(s)) => s > 40.0,
                    // Nothing timed yet (opening: the splash already shows
                    // the page), so render it sharp straight away.
                    _ => false,
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
                // Mid-pinch the size changes every frame: what there is
                // stretches, and the sharp page is drawn once it settles.
                if !pinching || any.is_none() {
                    self.request(ix, key, device_scale, cx);
                }
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
        // The next page is always ready before it scrolls in: rendering it
        // only once scrolling began showed it soft, then sharp (flicker).
        if ahead < self.page_count() && !pinching {
            let s = self.px_per_pt(ahead) * window.scale_factor();
            let k = PageKey {
                page: ahead as u32,
                scale_key: scale_key(s),
                rotation: self.rotation,
            };
            if !self.cache.contains(&k) {
                self.request(ahead, k, s, cx);
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
                    // A book is printed on the theme's surface (its pages are
                    // rendered on it too); everything else on white paper.
                    .bg(if self.is_book() {
                        st.c(|c| c.bg)
                    } else {
                        gpui::white()
                    })
                    .rounded(radius)
                    .shadow(st.sheet())
                    .overflow_hidden()
                    .relative()
                    .cursor(
                        if self.placing || self.markup.open && self.markup.tool.is_some() {
                            gpui::CursorStyle::Crosshair
                        } else {
                            gpui::CursorStyle::IBeam
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |r, e: &MouseDownEvent, window, cx| {
                            // A click on the page (not a field) keeps what was
                            // typed in a field and closes a list's choices.
                            r.finish_typing(cx);
                            r.form.choosing = None;
                            // Placing a signature takes the click; a markup
                            // tool draws; otherwise the click selects text.
                            if r.placing {
                                r.place_signature(ix, e, cx);
                            } else if !r.markup_down(ix, e, window, cx) {
                                r.select_down(ix, e, window, cx)
                            }
                        }),
                    )
                    // Characters for selection load the first time the pointer is over the page.
                    .on_mouse_move(cx.listener(move |r, _: &MouseMoveEvent, _, cx| {
                        r.ensure_chars(ix as u32, cx)
                    }))
                    .children(image.as_ref().map(|i| {
                        img(ImageSource::Render(i.clone()))
                            .w(px(w))
                            .h(px(h))
                            .rounded(radius)
                    }))
                    // Painted right after the picture, so by then its
                    // texture is on the GPU (see `painted`). A husk whose
                    // texture has gone asks for another frame, which
                    // draws the page again.
                    .children(image.as_ref().map(|i| {
                        let (husk, painted) = (i.kitab_is_husk(), self.painted.clone());
                        let shown = self.cache.peek(&key).is_some_and(|k| k.id == i.id);
                        let seen = self.seen_misses;
                        let this = cx.weak_entity();
                        gpui::canvas(
                            |_, _, _| (),
                            move |_, _, window, cx| {
                                if husk {
                                    if gpui::kitab_image_misses() != seen {
                                        // After this frame: a refresh
                                        // asked for while drawing is
                                        // dropped.
                                        window.defer(cx, |window, _| window.refresh());
                                    }
                                } else if shown {
                                    let first = painted.borrow().is_empty();
                                    painted.borrow_mut().push(key);
                                    // Once the frame is done, not at the
                                    // next one: a window left alone draws
                                    // no next frame.
                                    if first {
                                        window.defer(cx, move |_, cx| {
                                            let _ = this.update(cx, |r, _| r.slim_painted());
                                        });
                                    }
                                }
                            },
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_0()
                    }))
                    .children(self.markup_overlay(ix, ppt))
                    .children(self.note_pins(ix, ppt, &st, cx))
                    .children(self.deleted_outlines(ix, ppt, &st))
                    .children(self.bookmark_ribbon(ix, &st))
                    .children(self.speech_marks(ix, ppt, &st))
                    .children(self.selection_marks(ix, ppt, &st))
                    .children(self.find_marks(ix, ppt, &st))
                    .children(self.link_marks(ix, ppt, &st, cx))
                    .children(self.select_layer(ix, ppt, &st, cx))
                    .children(self.form_layer(ix, ppt, &st, cx)),
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
                // Typing goes straight into the field when the prompt shows.
                if self.focus_password {
                    self.focus_password = false;
                    window.focus(&self.password_focus);
                }
                let focused = self.password_focus.is_focused(window);
                let can_open = !self.password.is_empty();
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
                            .track_focus(&self.password_focus)
                            .cursor_text()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|r, _, window, cx| {
                                    window.focus(&r.password_focus);
                                    cx.notify();
                                }),
                            )
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
                            // A long password's dots stay in the box, the newest in view.
                            .overflow_hidden()
                            .when(dots.chars().count() > 30, |d| d.justify_end())
                            .child(if dots.is_empty() {
                                div().text_color(st.c(|c| c.faint)).child("Password")
                            } else {
                                div().flex_none().whitespace_nowrap().child(dots)
                            }),
                    )
                    .child(
                        div()
                            .id("password-open")
                            .mt(px(4.0))
                            .h(st.px(|m| m.control_height))
                            .px(px(18.0))
                            .flex()
                            .items_center()
                            .rounded(st.px(|m| m.control_radius))
                            .text_size(st.px(|m| m.ui_text))
                            .font_weight(FontWeight::MEDIUM)
                            .bg(st.c(|c| c.accent))
                            .text_color(st.c(|c| c.bg))
                            .when(!can_open, |d| d.opacity(0.45))
                            .when(can_open, |d| d.cursor_pointer())
                            .on_click(cx.listener(|r, _, window, cx| {
                                r.submit_password(&SubmitPassword, window, cx)
                            }))
                            .child("Open"),
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
        self.scale_factor = window.scale_factor();
        // Settings › Reading › Markdown text size changed: an open Markdown
        // file without a zoom of its own takes the new size now.
        let md_base = f32::from(st.md_text) / 100.0;
        if (md_base - self.md_base).abs() > f32::EPSILON {
            self.md_base = md_base;
            if self.md.is_some() && !matches!(self.zoom, ZoomMode::Custom(_)) {
                self.relayout(cx);
            }
        }
        // Pages painted last frame are on the GPU: one copy is enough, so
        // the pixels in memory go (a page at fit width on a large screen is
        // tens of megabytes).
        self.slim_painted();
        // The GPU lost its textures (a driver reset, a lost device): pages
        // kept only there are drawn again.
        let misses = gpui::kitab_image_misses();
        if misses != self.seen_misses {
            self.seen_misses = misses;
            if self.cache.drop_husks() {
                cx.notify();
            }
        }
        // Free GPU atlas space of pages the cache let go of.
        for image in self.cache.take_evicted() {
            let _ = window.drop_image(image);
        }
        if self.present.is_some() {
            self.glide = None;
            return self.present_view(window, cx);
        }
        self.glide_step(window);
        // Markdown's text units are collected afresh as the blocks draw.
        if self.md.is_some() {
            self.md_sel.units.borrow_mut().clear();
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
            if let Some(page) = self.pending_page.take() {
                self.jump(page as usize, cx);
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
                self.scrolled = true;
            }
            let lo = top as u64;
            let ppt = self.px_per_pt(lo as usize);
            let page_h = (self.page_size(lo as usize).h * ppt + GAP).max(1.0);
            let on_screen = (self.viewport.1 / page_h).ceil() as u64 + 1;
            let hi = (lo + on_screen).min(self.page_count().saturating_sub(1) as u64);
            self.visible
                .store((hi << 32) | lo, std::sync::atomic::Ordering::Relaxed);
            // The cache must hold the pages on screen and the next ones,
            // whatever the window and screen: at fit width on a large
            // high-DPI screen one page is ~30 MB, and a fixed 64 MB evicted
            // pages still on screen while scrolling (they flickered as they
            // were drawn again).
            let s = self.page_size(lo as usize);
            let dev = ppt * window.scale_factor();
            let page_bytes = (s.w * dev) * (s.h * dev) * 4.0;
            // On screen, the one drawn ahead, and the one just left (so
            // turning back never redraws). Each is a texture of tens of
            // megabytes on a large screen; a third spare one was rarely
            // shown again before it was drawn anyway.
            let need = (page_bytes as usize).saturating_mul(on_screen as usize + 2);
            self.cache.set_budget(CACHE_BUDGET.max(need));
        }
        let page = self.current_page();
        if page != self.last_page {
            self.last_page = page;
            cx.emit(ReaderEvent::PageChanged);
            self.schedule_save(cx);
        }

        // The splash (a first frame drawn before the GPU window) goes once
        // this tab has something to show: an open Markdown file draws as
        // text, a prompt or an error at once; PDF pages fire when drawn.
        let markdown_open = self.md.is_some() && self.status == Status::Open;
        if markdown_open
            || matches!(
                self.status,
                Status::NeedsPassword { .. } | Status::Failed(_)
            )
        {
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
            // The pages above, the status bar pinned below them.
            .flex()
            .flex_col()
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
                // Esc closes a footnote, then puts the pen down, then clears
                // the selection.
                if r.close_note(cx) {
                } else if r.markup.open {
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
            .on_action(cx.listener(|r, _: &ToggleBookmark, _, cx| r.toggle_bookmark(cx)))
            .on_action(cx.listener(|r, _: &ReadAloud, _, cx| r.toggle_read_aloud(cx)))
            .on_action(cx.listener(|r, _: &StopReading, _, cx| r.stop_read_aloud(cx)))
            .on_action(cx.listener(|r, _: &HighlightSelection, _, cx| {
                if r.can_markup(cx) {
                    r.mark_selection("highlight", cx)
                }
            }))
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
            .on_key_down(cx.listener(|r, e: &gpui::KeyDownEvent, _, cx| {
                if r.placing && e.keystroke.key == "escape" {
                    r.stop_placing(cx);
                    cx.stop_propagation();
                    return;
                }
                // Select: Delete takes the picked mark away, Esc lets it go.
                if r.markup.selected.is_some() {
                    match e.keystroke.key.as_str() {
                        "delete" | "backspace" => {
                            r.delete_selected_mark(cx);
                            cx.stop_propagation();
                            return;
                        }
                        "escape" => {
                            r.markup.selected = None;
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                        _ => {}
                    }
                }
                r.wake_dock(cx)
            }))
            // Middle-click scrolling: the wheel starts (or ends) it, any
            // other click ends it without doing anything else.
            .capture_any_mouse_down(cx.listener(|r, e: &MouseDownEvent, _, cx| {
                if r.autoscroll.is_some() && e.button != MouseButton::Middle {
                    r.autoscroll = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|r, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    r.toggle_autoscroll(e.position, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|r, _, _, cx| {
                    if r.autoscroll.as_ref().is_some_and(|a| a.dragged) {
                        r.autoscroll = None;
                        cx.notify();
                    }
                }),
            )
            .when(self.autoscroll.is_some(), |d| {
                d.cursor(gpui::CursorStyle::ResizeUpDown)
            })
            .children(
                self.autoscroll
                    .as_ref()
                    .map(|a| crate::autoscroll::mark(a.anchor, &st)),
            )
            .on_mouse_move(cx.listener(|r, e: &MouseMoveEvent, window, cx| {
                // Markdown editing: a drag selection or a scroll bar thumb,
                // followed even when the pointer leaves the editor.
                r.editor_mouse_move(e, window, cx);
                if let Some(a) = r.autoscroll.as_mut() {
                    a.moved(e.position, e.pressed_button == Some(MouseButton::Middle));
                }
                r.wake_dock(cx);
                if r.markup.drawing.is_some() || r.markup.moving.is_some() {
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
                if let Some(g) = r.md_hdrag {
                    if e.pressed_button == Some(MouseButton::Left) {
                        r.md_hdrag_to(g, f32::from(e.position.x), cx);
                    } else {
                        r.md_hdrag = None;
                        cx.notify();
                    }
                }
                if r.md_sel.dragging {
                    if e.pressed_button == Some(MouseButton::Left) {
                        r.md_select_move(e.position, cx);
                    } else {
                        r.md_sel.dragging = false;
                    }
                }
                if r.dragging {
                    if e.pressed_button == Some(MouseButton::Left) {
                        r.seek_x(e.position.x, cx);
                    } else {
                        r.dragging = false;
                    }
                }
                if r.bar_grab.is_some() {
                    if e.pressed_button == Some(MouseButton::Left) {
                        r.bar_drag_to(e.position.y, cx);
                    } else {
                        r.bar_grab = None;
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|r, _, _, cx| {
                    r.dragging = false;
                    if r.bar_grab.take().is_some() {
                        cx.notify();
                    }
                    r.select_up();
                    r.md_sel.dragging = false;
                    if r.md_hdrag.take().is_some() {
                        cx.notify();
                    }
                    r.markup_up(cx);
                    r.editor_mouse_up(cx);
                }),
            )
            .on_action(cx.listener(|r, _: &ScrollDown, _, cx| r.glide_by(64.0, cx)))
            .on_action(cx.listener(|r, _: &ScrollUp, _, cx| r.glide_by(-64.0, cx)))
            .on_action(cx.listener(|r, _: &ScreenDown, _, cx| r.glide_by(r.viewport.1 * 0.9, cx)))
            .on_action(cx.listener(|r, _: &ScreenUp, _, cx| r.glide_by(-r.viewport.1 * 0.9, cx)))
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
                let body = match self.editor_view(&st, window, cx) {
                    Some(editor) => div()
                        .flex()
                        .size_full()
                        .child(div().w(gpui::relative(0.5)).h_full().child(editor))
                        .child(div().flex_1().min_w_0().h_full().relative().child(pages))
                        .into_any_element(),
                    None => pages.into_any_element(),
                };
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .child(body)
                    .children(self.scroll_bar(&st, cx))
                    .children(status.map(|s| div().absolute().inset_0().child(s)))
            })
            .children(self.status_bar(&st, cx))
            // Ctrl+wheel zooms. This layer is painted after the list so it sees the
            // wheel first; it has no other handlers, so clicks pass through.
            .child(
                div()
                    .id("wheel-zoom")
                    .absolute()
                    .inset_0()
                    .on_scroll_wheel(cx.listener(|r, e: &ScrollWheelEvent, _, cx| {
                        r.scrolled = true;
                        if e.modifiers.control || e.modifiers.platform {
                            // A pinch (touch screen, touchpad): fine steps,
                            // 200 px for each e-fold of the zoom.
                            if let gpui::ScrollDelta::Pixels(by) = e.delta {
                                r.pinch_zoom((f32::from(by.y) / 200.0).exp(), cx);
                                cx.stop_propagation();
                                return;
                            }
                            let dy = f32::from(e.delta.pixel_delta(px(20.0)).y);
                            if dy.abs() > 0.5 {
                                r.step_zoom(dy > 0.0, cx);
                            }
                            cx.stop_propagation();
                            return;
                        }
                        // A mouse wheel turns in notches: glide to where it
                        // scrolls instead of jumping there. A touchpad sends
                        // its own fine steps, which go to the list as they are.
                        let gpui::ScrollDelta::Lines(lines) = e.delta else {
                            return;
                        };
                        let st = Style::get(cx);
                        if !(st.smooth_scroll && st.animations)
                            || lines.x != 0.0
                            || lines.y == 0.0
                            || r.present.is_some()
                            || !r.list.viewport_bounds().contains(&e.position)
                        {
                            return;
                        }
                        // The list's own step: 20 px a line.
                        r.glide_by(-lines.y * 20.0, cx);
                        cx.stop_propagation();
                    })),
            )
            .children(self.page_nav(&st, window, cx))
            .children(self.dock_view(&st, inset, window, cx))
            .children(self.edit_button(&st, cx))
            .children(self.find_bar(&st, window, cx))
            .children(self.placing_banner(&st, cx))
            .children(self.markup_bar(&st, window, cx))
            .children(self.writer(&st, cx))
            .children(self.note_view(&st, cx))
            .into_any_element()
    }
}

impl Reader {
    /// Settings › Advanced › Clear cached pages: drop every rendered page;
    /// the visible ones render again on the next frame.
    pub fn drop_cached_pages(&mut self, cx: &mut Context<Self>) {
        self.cache.clear();
        cx.notify();
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
            // How far down it is, from the scroll position itself (the
            // blocks aren't pages; a short document is all in view: 100 %).
            let (fraction, shown) = self.scroll_fraction();
            let percent = if shown >= 0.999 {
                100
            } else {
                (fraction * 100.0).round() as u32
            };
            let read = div().child(format!("{percent} %")).into_any_element();
            // Markdown has no pages: the same styles, in how far through it
            // you are, and the steppers move a screen at a time.
            return Some(match st.page_nav {
                PageNav::Stepper => vec![
                    nav_button("prev-screen", "chevron-left", st)
                        .tip("Up a screen  Page Up")
                        .on_click(cx.listener(|r, _, _, cx| {
                            r.list.scroll_by(px(-r.viewport.1 * 0.9));
                            cx.notify()
                        }))
                        .into_any_element(),
                    read,
                    nav_button("next-screen", "chevron-right", st)
                        .tip("Down a screen  Page Down")
                        .on_click(cx.listener(|r, _, _, cx| {
                            r.list.scroll_by(px(r.viewport.1 * 0.9));
                            cx.notify()
                        }))
                        .into_any_element(),
                ],
                PageNav::Counter => vec![read],
                _ => vec![self.track_element(st, cx), read],
            });
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
            // An arrow here, not the page's text cursor beneath.
            .occlude()
            .cursor_default()
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
        let bar = if self.status_bar_shown(st) {
            info::BAR_H
        } else {
            0.0
        };
        let place = div().absolute().bottom(px(18.0 + bar));
        Some(
            if corner {
                place.right(px(22.0)).child(body)
            } else {
                // Editing Markdown: centred on the preview, off the source.
                dock::over_preview(place, self.is_editing())
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(body)
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
            .when(!editing, |d| d.hover(|d| d.bg(st.hover_bg())))
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
        let fraction = self.scroll_fraction().0;
        // Shorter in a narrow reading area, so the dock fits without scrolling.
        let vw = self.viewport.0;
        let track_w = if vw > 0.0 && vw < 820.0 {
            if vw < 520.0 {
                (vw - 330.0).clamp(60.0, 100.0)
            } else {
                (vw - 470.0).clamp(100.0, 220.0)
            }
        } else {
            220.0
        };
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
                    .top_0()
                    .left_0()
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
        .hover(|d| d.bg(st.hover_bg()))
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
