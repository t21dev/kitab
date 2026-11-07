// SPDX-License-Identifier: AGPL-3.0-or-later

//! Drawing on pages (PLAN.md Phase 6, after Edge's PDF viewer): pen,
//! highlighter, line, arrow, rectangle, ellipse and eraser, with colours,
//! three widths, undo and redo.
//!
//! Every mark is kept in the library the moment it is drawn, so nothing is
//! lost. In a PDF, Save (Ctrl+S) writes the marks into the file as standard
//! annotations and the library forgets them; MuPDF draws them from then on.
//! Comics, XPS and fixed-layout books keep their marks in the library.
//! Re-flowing books and Markdown have no fixed pages to draw on.

use super::geom::{point_to_view, rect_to_view, view_to_point};
use super::*;
use gpui::{PathBuilder, PathStyle, canvas};
use kitab_core::{Mark, MarkShape, PointPt, RectPt, Rgb8, rect_between};
use kitab_theme::markup as ink;
use lyon_tessellation::{LineCap, LineJoin, StrokeOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Highlighter,
    Line,
    Arrow,
    Rect,
    Ellipse,
    /// Text written on the page.
    Text,
    /// A note pinned to the page.
    Note,
    Eraser,
}

impl Tool {
    pub const ALL: [Tool; 9] = [
        Tool::Pen,
        Tool::Highlighter,
        Tool::Line,
        Tool::Arrow,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Text,
        Tool::Note,
        Tool::Eraser,
    ];

    fn icon(self) -> &'static str {
        match self {
            Tool::Pen => "pen",
            Tool::Highlighter => "highlight",
            Tool::Line => "line",
            Tool::Arrow => "arrow-diagonal",
            Tool::Rect => "square",
            Tool::Ellipse => "circle",
            Tool::Text => "text",
            Tool::Note => "note",
            Tool::Eraser => "eraser",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tool::Pen => "Pen",
            Tool::Highlighter => "Highlighter",
            Tool::Line => "Line  Shift keeps it at 45°",
            Tool::Arrow => "Arrow  Shift keeps it at 45°",
            Tool::Rect => "Rectangle  Shift for a square",
            Tool::Ellipse => "Ellipse  Shift for a circle",
            Tool::Text => "Text  click where it goes",
            Tool::Note => "Note  click to pin one",
            Tool::Eraser => "Eraser  click a mark",
        }
    }
}

/// A mark on the page, with its library id once the library has kept it.
pub(super) struct Placed {
    pub local: u64,
    pub id: Option<i64>,
    pub mark: Mark,
}

/// A note or text box being written, at a page point.
pub(super) struct Writing {
    pub page: u32,
    pub at: PointPt,
    pub note: bool,
    pub field: Entity<crate::text_field::TextField>,
    /// The mark this replaces (editing an existing note or text).
    pub replaces: Option<u64>,
    pub colour: (u8, u8, u8),
    pub size: f32,
    _sub: gpui::Subscription,
}

/// Text sizes for the Text tool by width setting, in points.
const TEXT_SIZES: [f32; 3] = [12.0, 16.0, 22.0];
const TEXT_LINE: f32 = 1.3;

/// One step of undo history.
#[derive(Debug, Clone)]
pub(super) enum Edit {
    Added(Mark),
    Removed(Mark),
}

#[derive(Default)]
pub(super) struct MarkupState {
    /// The bar is showing.
    pub open: bool,
    pub tool: Option<Tool>,
    pub pen_colour: usize,
    pub highlighter_colour: usize,
    pub size: usize,
    pub marks: Vec<Placed>,
    /// A mark being drawn, and the page point the drag started at.
    pub drawing: Option<(Mark, PointPt)>,
    pub undo: Vec<Edit>,
    pub redo: Vec<Edit>,
    pub loaded: bool,
    pub saving: bool,
    /// The last save's outcome, shown in the bar.
    pub note: Option<SharedString>,
    /// Colours chosen in the picker, for the pen (and shapes) and the
    /// highlighter, and whether each is the one in use.
    pub custom_pen: Option<(u8, u8, u8)>,
    pub custom_highlighter: Option<(u8, u8, u8)>,
    pub use_custom_pen: bool,
    pub use_custom_highlighter: bool,
    /// The colour picker is open.
    pub picker: bool,
    /// A note or text box being written.
    pub writing: Option<Writing>,
    /// The hex digits being typed in the picker (without '#'), while editing.
    pub hex: Option<String>,
    pub hex_focus: Option<FocusHandle>,
    next_local: u64,
}

impl MarkupState {
    /// The colour the pen (or the highlighter) draws with now.
    pub fn colour(&self, highlighter: bool) -> (u8, u8, u8) {
        if highlighter {
            match self
                .custom_highlighter
                .filter(|_| self.use_custom_highlighter)
            {
                Some(c) => c,
                None => {
                    ink::HIGHLIGHTER_COLOURS
                        [self.highlighter_colour % ink::HIGHLIGHTER_COLOURS.len()]
                    .1
                }
            }
        } else {
            match self.custom_pen.filter(|_| self.use_custom_pen) {
                Some(c) => c,
                None => ink::PEN_COLOURS[self.pen_colour % ink::PEN_COLOURS.len()].1,
            }
        }
    }
}

/// Named ink colours, as kitab-theme lists them.
type Palette = &'static [(&'static str, (u8, u8, u8))];

fn rgb((r, g, b): (u8, u8, u8)) -> Rgb8 {
    Rgb8 { r, g, b }
}

use kitab_theme::markup::{hex_of, parse_hex};

/// A swatch fill for an ink colour.
fn rgba((r, g, b): (u8, u8, u8)) -> gpui::Rgba {
    gpui::Rgba {
        r: f32::from(r) / 255.0,
        g: f32::from(g) / 255.0,
        b: f32::from(b) / 255.0,
        a: 1.0,
    }
}

/// Snap `to` so the line from `from` runs at a multiple of 45°.
fn snap_45(from: PointPt, to: PointPt) -> PointPt {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = (dx * dx + dy * dy).sqrt();
    let step = std::f32::consts::FRAC_PI_4;
    let angle = (dy.atan2(dx) / step).round() * step;
    PointPt {
        x: from.x + len * angle.cos(),
        y: from.y + len * angle.sin(),
    }
}

/// Make the box from `a` to `b` square (same width and height), keeping `a`.
fn square_from(a: PointPt, b: PointPt) -> PointPt {
    let side = (b.x - a.x).abs().max((b.y - a.y).abs());
    PointPt {
        x: a.x + side * (b.x - a.x).signum(),
        y: a.y + side * (b.y - a.y).signum(),
    }
}

/// Whether a finished mark is worth keeping (a click without a drag isn't a
/// line or a box).
fn worth_keeping(m: &Mark) -> bool {
    match &m.shape {
        MarkShape::Line { from, to, .. } => (to.x - from.x).hypot(to.y - from.y) >= 2.0,
        MarkShape::Rect { rect } | MarkShape::Ellipse { rect } => rect.w >= 2.0 && rect.h >= 2.0,
        _ => m.is_valid(),
    }
}

impl Reader {
    /// Drawing is possible here: a document with fixed pages, and the
    /// setting is on.
    pub fn can_markup(&self, cx: &App) -> bool {
        Style::get(cx).markup
            && self.status == Status::Open
            && !self.is_markdown()
            && !self.is_book()
    }

    /// Show or hide the markup bar (Ctrl+Shift+D, or the dock's pen).
    pub fn toggle_markup(&mut self, cx: &mut Context<Self>) {
        if !Style::get(cx).markup {
            return;
        }
        let m = &mut self.markup;
        m.open = !m.open;
        if m.open {
            if m.tool.is_none() {
                m.tool = Some(Tool::Pen);
            }
            self.clear_selection(cx);
        } else {
            m.tool = None;
            m.drawing = None;
        }
        cx.notify();
    }

    /// Load the marks the library keeps for this file (once, after opening).
    pub(super) fn load_marks(&mut self, cx: &mut Context<Self>) {
        if self.markup.loaded || self.is_markdown() || self.is_book() {
            return;
        }
        self.markup.loaded = true;
        let (library, path) = (self.library.clone(), self.path.clone());
        cx.spawn(async move |this, cx| {
            let marks = cx
                .background_spawn(async move { library.marks(&path) })
                .await;
            let _ = this.update(cx, |r, cx| {
                let pages = r.page_count() as u32;
                for (id, mark) in marks.into_iter().filter(|(_, m)| m.page < pages) {
                    let local = r.next_local();
                    r.markup.marks.push(Placed {
                        local,
                        id: Some(id),
                        mark,
                    });
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn next_local(&mut self) -> u64 {
        self.markup.next_local += 1;
        self.markup.next_local
    }

    /// Keep a mark: on the page now, in the library in the background.
    fn place(&mut self, mark: Mark, cx: &mut Context<Self>) -> u64 {
        let local = self.next_local();
        self.markup.marks.push(Placed {
            local,
            id: None,
            mark: mark.clone(),
        });
        let (library, path) = (self.library.clone(), self.path.clone());
        cx.spawn(async move |this, cx| {
            let id = cx
                .background_spawn(async move { library.add_mark(&path, mark) })
                .await;
            let _ = this.update(cx, |r, cx| {
                let Some(id) = id else { return };
                match r.markup.marks.iter_mut().find(|p| p.local == local) {
                    Some(p) => p.id = Some(id),
                    // Erased or undone before the library answered.
                    None => {
                        let library = r.library.clone();
                        cx.background_spawn(async move { library.delete_mark(id) })
                            .detach();
                    }
                }
            });
        })
        .detach();
        self.markup.note = None;
        cx.notify();
        local
    }

    /// Take a mark off the page and out of the library.
    fn unplace(&mut self, local: u64, cx: &mut Context<Self>) -> Option<Mark> {
        let at = self.markup.marks.iter().position(|p| p.local == local)?;
        let placed = self.markup.marks.remove(at);
        if let Some(id) = placed.id {
            let library = self.library.clone();
            cx.background_spawn(async move { library.delete_mark(id) })
                .detach();
        }
        cx.notify();
        Some(placed.mark)
    }

    /// The newest placed mark equal to `mark`.
    fn find_placed(&self, mark: &Mark) -> Option<u64> {
        self.markup
            .marks
            .iter()
            .rev()
            .find(|p| p.mark == *mark)
            .map(|p| p.local)
    }

    pub fn markup_undo(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.markup.undo.pop() else {
            return;
        };
        match edit {
            Edit::Added(mark) => {
                if let Some(local) = self.find_placed(&mark) {
                    self.unplace(local, cx);
                }
                self.markup.redo.push(Edit::Added(mark));
            }
            Edit::Removed(mark) => {
                self.place(mark.clone(), cx);
                self.markup.redo.push(Edit::Removed(mark));
            }
        }
    }

    pub fn markup_redo(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.markup.redo.pop() else {
            return;
        };
        match edit {
            Edit::Added(mark) => {
                self.place(mark.clone(), cx);
                self.markup.undo.push(Edit::Added(mark));
            }
            Edit::Removed(mark) => {
                if let Some(local) = self.find_placed(&mark) {
                    self.unplace(local, cx);
                }
                self.markup.undo.push(Edit::Removed(mark));
            }
        }
    }

    /// The page point under window point `p` on page `ix`.
    fn page_point(&self, ix: usize, p: Point<Pixels>) -> Option<PointPt> {
        let b = self.sheet_bounds(ix)?;
        let size = self.raw_page_size(ix);
        let (x, y) = view_to_point(
            f32::from(p.x - b.origin.x),
            f32::from(p.y - b.origin.y),
            size,
            self.rotation,
            self.px_per_pt(ix),
        );
        Some(PointPt {
            x: x.clamp(0.0, size.w),
            y: y.clamp(0.0, size.h),
        })
    }

    /// Left button down on page `ix` with a tool: start drawing, or erase.
    /// Returns false when no tool is active (the click selects text).
    pub(super) fn markup_down(
        &mut self,
        ix: usize,
        e: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tool) = self.markup.tool.filter(|_| self.markup.open) else {
            return false;
        };
        // Drawing puts the colour picker away.
        self.markup.picker = false;
        self.markup.hex = None;
        let Some(at) = self.page_point(ix, e.position) else {
            return true;
        };
        let page = ix as u32;
        if tool == Tool::Eraser {
            // A few pixels of slack, in points.
            let slack = 5.0 / self.px_per_pt(ix).max(0.01);
            let hit = self
                .markup
                .marks
                .iter()
                .rev()
                .find(|p| p.mark.page == page && p.mark.hits(at, slack))
                .map(|p| p.local);
            if let Some(mark) = hit.and_then(|local| self.unplace(local, cx)) {
                self.markup.undo.push(Edit::Removed(mark));
                self.markup.redo.clear();
            }
            return true;
        }
        if matches!(tool, Tool::Text | Tool::Note) {
            self.start_writing(
                page,
                at,
                tool == Tool::Note,
                None,
                String::new(),
                window,
                cx,
            );
            return true;
        }
        let highlighter = tool == Tool::Highlighter;
        let m = &self.markup;
        let colour = m.colour(highlighter);
        let (width, opacity) = if highlighter {
            (
                ink::HIGHLIGHTER_WIDTHS[m.size % 3],
                ink::HIGHLIGHTER_OPACITY,
            )
        } else {
            (ink::PEN_WIDTHS[m.size % 3], 1.0)
        };
        let shape = match tool {
            Tool::Pen | Tool::Highlighter => MarkShape::Ink {
                strokes: vec![vec![at]],
            },
            Tool::Line | Tool::Arrow => MarkShape::Line {
                from: at,
                to: at,
                arrow: tool == Tool::Arrow,
            },
            Tool::Rect => MarkShape::Rect {
                rect: rect_between(at, at),
            },
            Tool::Ellipse => MarkShape::Ellipse {
                rect: rect_between(at, at),
            },
            Tool::Eraser | Tool::Text | Tool::Note => return true,
        };
        self.markup.drawing = Some((
            Mark {
                page,
                shape,
                color: rgb(colour),
                width,
                opacity,
            },
            at,
        ));
        cx.notify();
        true
    }

    /// The pointer moved with a mark being drawn.
    pub(super) fn markup_move(&mut self, p: Point<Pixels>, shift: bool, cx: &mut Context<Self>) {
        let Some((mark, start)) = self.markup.drawing.as_ref() else {
            return;
        };
        let (ix, start) = (mark.page as usize, *start);
        let Some(at) = self.page_point(ix, p) else {
            return;
        };
        // Pen points closer than half a pixel add nothing.
        let min_step = 0.5 / self.px_per_pt(ix).max(0.01);
        let Some((mark, _)) = self.markup.drawing.as_mut() else {
            return;
        };
        match &mut mark.shape {
            MarkShape::Ink { strokes } => {
                if let Some(stroke) = strokes.last_mut() {
                    let far = stroke
                        .last()
                        .is_none_or(|l| (at.x - l.x).hypot(at.y - l.y) >= min_step);
                    if far && stroke.len() < kitab_core::MAX_POINTS {
                        stroke.push(at);
                    }
                }
            }
            MarkShape::Line { to, .. } => *to = if shift { snap_45(start, at) } else { at },
            MarkShape::Rect { rect } | MarkShape::Ellipse { rect } => {
                let end = if shift { square_from(start, at) } else { at };
                *rect = rect_between(start, end);
            }
            _ => {}
        }
        cx.notify();
    }

    /// The button came up: keep the mark if it is worth keeping.
    pub(super) fn markup_up(&mut self, cx: &mut Context<Self>) {
        let Some((mark, _)) = self.markup.drawing.take() else {
            return;
        };
        if worth_keeping(&mark) {
            self.place(mark.clone(), cx);
            self.markup.undo.push(Edit::Added(mark));
            self.markup.redo.clear();
        }
        cx.notify();
    }

    /// Open the writer for a note or a text box at `at` on `page`.
    #[allow(clippy::too_many_arguments)]
    fn start_writing(
        &mut self,
        page: u32,
        at: PointPt,
        note: bool,
        replaces: Option<u64>,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = if note {
            "Write a note"
        } else {
            "Type on the page"
        };
        let field = cx.new(|cx| crate::text_field::TextField::new(&text, true, placeholder, cx));
        field.read(cx).focus(window);
        // The click that opened the writer is still being handled (the page
        // takes focus back on it): focus the field again once it's drawn.
        let again = field.clone();
        cx.on_next_frame(window, move |_, window, cx| again.read(cx).focus(window));
        let sub = cx.subscribe(
            &field,
            |r, _, ev: &crate::text_field::FieldEvent, cx| match ev {
                crate::text_field::FieldEvent::Submit => r.finish_writing(cx),
                crate::text_field::FieldEvent::Cancel => r.cancel_writing(cx),
                crate::text_field::FieldEvent::Changed => cx.notify(),
            },
        );
        let m = &self.markup;
        self.markup.writing = Some(Writing {
            page,
            at,
            note,
            field,
            replaces,
            colour: m.colour(false),
            size: TEXT_SIZES[m.size % 3],
            _sub: sub,
        });
        cx.notify();
    }

    fn cancel_writing(&mut self, cx: &mut Context<Self>) {
        self.markup.writing = None;
        cx.notify();
    }

    /// Keep what was written (Ctrl+Enter or Done). Empty text keeps nothing,
    /// and empties an edited note away.
    pub(super) fn finish_writing(&mut self, cx: &mut Context<Self>) {
        let Some(w) = self.markup.writing.take() else {
            return;
        };
        let text = w.field.read(cx).text().trim_end().to_string();
        if let Some(old) = w.replaces.and_then(|l| self.unplace(l, cx)) {
            self.markup.undo.push(Edit::Removed(old));
        }
        if text.trim().is_empty() {
            cx.notify();
            return;
        }
        let shape = if w.note {
            MarkShape::Note { at: w.at, text }
        } else {
            // A box the text fits: about half an em per character.
            let longest = text.lines().map(|l| l.chars().count()).max().unwrap_or(1) as f32;
            let lines = text.lines().count().max(1) as f32;
            MarkShape::Text {
                rect: RectPt {
                    x: w.at.x,
                    y: w.at.y,
                    w: (longest * w.size * 0.55).max(w.size * 2.0),
                    h: lines * w.size * TEXT_LINE,
                },
                text,
                size: w.size,
            }
        };
        let mark = Mark {
            page: w.page,
            shape,
            color: rgb(w.colour),
            width: 1.0,
            opacity: 1.0,
        };
        if mark.is_valid() {
            self.place(mark.clone(), cx);
            self.markup.undo.push(Edit::Added(mark));
            self.markup.redo.clear();
        }
        cx.notify();
    }

    /// Highlight, underline or strike the selected text: one box per line.
    pub fn mark_selection(&mut self, kind: &str, cx: &mut Context<Self>) {
        let Some(((p0, i0), (p1, i1))) = self.select.range() else {
            return;
        };
        let highlighter = kind == "highlight";
        let colour = rgb(self.markup.colour(highlighter));
        for page in p0..=p1 {
            let Some(chars) = self.layers.chars.get(&page).cloned() else {
                continue;
            };
            let a = if page == p0 { i0 } else { 0 };
            let b = if page == p1 { i1 } else { chars.len() };
            let mut lines: Vec<(u32, RectPt)> = Vec::new();
            for c in chars.iter().take(b.min(chars.len())).skip(a) {
                if c.ch.is_whitespace() && lines.last().is_none_or(|(l, _)| *l != c.line) {
                    continue;
                }
                match lines.last_mut() {
                    Some((l, r)) if *l == c.line => {
                        let (x1, y1) = (
                            (r.x + r.w).max(c.rect.x + c.rect.w),
                            (r.y + r.h).max(c.rect.y + c.rect.h),
                        );
                        r.x = r.x.min(c.rect.x);
                        r.y = r.y.min(c.rect.y);
                        r.w = x1 - r.x;
                        r.h = y1 - r.y;
                    }
                    _ => lines.push((c.line, c.rect)),
                }
            }
            let rects: Vec<RectPt> = lines
                .into_iter()
                .map(|(_, r)| r)
                .filter(|r| r.w > 0.0)
                .collect();
            if rects.is_empty() {
                continue;
            }
            let (shape, opacity) = match kind {
                "underline" => (MarkShape::Underline { rects }, 1.0),
                "strike" => (MarkShape::Strike { rects }, 1.0),
                _ => (MarkShape::Highlight { rects }, ink::HIGHLIGHTER_OPACITY),
            };
            let mark = Mark {
                page,
                shape,
                color: colour,
                width: 1.5,
                opacity,
            };
            if mark.is_valid() {
                self.place(mark.clone(), cx);
                self.markup.undo.push(Edit::Added(mark));
            }
        }
        self.markup.redo.clear();
        self.clear_selection(cx);
    }

    /// Notes on page `ix`: pins with their text as a tooltip; a click edits.
    pub(super) fn note_pins(
        &self,
        ix: usize,
        ppt: f32,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let page = ix as u32;
        let (size, rotation) = (self.raw_page_size(ix), self.rotation);
        self.markup
            .marks
            .iter()
            .filter(|p| p.mark.page == page)
            .filter_map(|p| match &p.mark.shape {
                MarkShape::Note { at, text } => Some((p.local, *at, text.clone(), p.mark.color)),
                _ => None,
            })
            .map(|(local, at, text, c)| {
                let (u, v) = point_to_view(at.x, at.y, size, rotation);
                let fill = rgba((c.r, c.g, c.b));
                div()
                    .id(("note-pin", local))
                    .absolute()
                    .left(px(u * ppt))
                    .top(px(v * ppt))
                    .size(px(20.0))
                    .rounded(px(5.0))
                    .bg(fill)
                    .shadow(st.elev())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        svg()
                            .path(icon("note"))
                            .size(px(13.0))
                            .text_color(gpui::white()),
                    )
                    .tip(SharedString::from(text.clone()))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |r, _, window, cx| {
                        if r.markup.tool == Some(Tool::Eraser) && r.markup.open {
                            if let Some(mark) = r.unplace(local, cx) {
                                r.markup.undo.push(Edit::Removed(mark));
                            }
                            return;
                        }
                        r.start_writing(page, at, true, Some(local), text.clone(), window, cx);
                    }))
                    .into_any_element()
            })
            .collect()
    }

    /// The writer for a note or text box, by where it goes on the page.
    pub(super) fn writer(&self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        let w = self.markup.writing.as_ref()?;
        let ix = w.page as usize;
        let b = self.sheet_bounds(ix)?;
        let (u, v) = point_to_view(w.at.x, w.at.y, self.raw_page_size(ix), self.rotation);
        let ppt = self.px_per_pt(ix);
        let at = gpui::point(b.origin.x + px(u * ppt), b.origin.y + px(v * ppt + 22.0));
        let title = if w.note { "Note" } else { "Text" };
        let button = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .px(px(10.0))
                .h(px(26.0))
                .flex()
                .items_center()
                .rounded(px(7.0))
                .text_size(px(12.0))
                .when(primary, |d| {
                    d.bg(st.c(|c| c.accent)).text_color(st.c(|c| c.bg))
                })
                .when(!primary, |d| d.hover(|d| d.bg(st.c(|c| c.hover))))
                .child(label)
        };
        Some(
            gpui::deferred(
                gpui::anchored().position(at).child(
                    div()
                        .id("markup-writer")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .w(px(280.0))
                        .p(px(10.0))
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .rounded(px(12.0))
                        .bg(st.c(|c| c.raised))
                        .shadow(st.float())
                        .border_1()
                        .border_color(st.c(|c| c.line))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(st.c(|c| c.muted))
                                .child(format!("{title}  ·  Ctrl+Enter to keep, Esc to drop")),
                        )
                        .child(
                            div()
                                .min_h(px(60.0))
                                .px(px(8.0))
                                .py(px(6.0))
                                .rounded(px(8.0))
                                .bg(st.c(|c| c.hover))
                                .child(w.field.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(6.0))
                                .child(
                                    button("writer-cancel", "Cancel", false)
                                        .on_click(cx.listener(|r, _, _, cx| r.cancel_writing(cx))),
                                )
                                .child(
                                    button("writer-done", "Done", true)
                                        .on_click(cx.listener(|r, _, _, cx| r.finish_writing(cx))),
                                ),
                        ),
                ),
            )
            .into_any_element(),
        )
    }

    /// The marks on page `ix` (and the one being drawn), painted over the page.
    pub(super) fn markup_overlay(&self, ix: usize, ppt: f32) -> Option<AnyElement> {
        let page = ix as u32;
        let mut marks: Vec<Mark> = self
            .markup
            .marks
            .iter()
            .filter(|p| p.mark.page == page)
            .map(|p| p.mark.clone())
            .collect();
        if let Some((m, _)) = self.markup.drawing.as_ref().filter(|(m, _)| m.page == page) {
            marks.push(m.clone());
        }
        if marks.is_empty() {
            return None;
        }
        let (size, rotation) = (self.raw_page_size(ix), self.rotation);
        Some(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| {
                    for m in &marks {
                        paint_mark(m, bounds, size, rotation, ppt, window, cx);
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .into_any_element(),
        )
    }

    /// A save is running.
    pub fn markup_saving(&self) -> bool {
        self.markup.saving
    }

    /// Unsaved marks in a PDF (they are kept in the library meanwhile).
    pub fn unsaved_marks(&self) -> usize {
        if self.is_pdf() {
            self.markup.marks.len()
        } else {
            0
        }
    }

    /// Ctrl+S in a PDF: write the marks into the file as annotations. The
    /// document is closed while the file is replaced (Windows can't rename
    /// over an open file), then opened again at the same place.
    pub fn save_marks(&mut self, cx: &mut Context<Self>) {
        if !self.is_pdf() || self.markup.saving || self.markup.marks.is_empty() {
            return;
        }
        let Some(doc) = self.doc else { return };
        self.markup.saving = true;
        self.markup.note = Some("Saving…".into());
        self.save_now(cx);
        let marks: Vec<Mark> = self.markup.marks.iter().map(|p| p.mark.clone()).collect();
        let (engine, library, path) =
            (self.engine.clone(), self.library.clone(), self.path.clone());
        let password = self.unlock.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    engine.close(doc);
                    let result = engine.write_marks(&path, password.as_deref(), &marks, &path);
                    if result.is_ok() {
                        library.clear_marks(&path);
                    }
                    result
                })
                .await;
            let _ = this.update(cx, |r, cx| r.after_save(result, cx));
        })
        .detach();
    }

    fn after_save(&mut self, result: Result<usize, String>, cx: &mut Context<Self>) {
        self.markup.saving = false;
        match result {
            Ok(n) => {
                // The file draws them now.
                self.markup.marks.clear();
                self.markup.undo.clear();
                self.markup.redo.clear();
                let what = if n == 1 { "mark" } else { "marks" };
                self.markup.note = Some(format!("Saved {n} {what} into the PDF").into());
            }
            Err(e) => {
                // The marks stay, on the page and in the library.
                log::warn!("saving marks failed: {e}");
                self.markup.note = Some(format!("Couldn't save: {e}").into());
            }
        }
        // Open the file again (it was closed for the save), at the same place.
        self.doc = None;
        self.cache.clear();
        self.layers = layer::Layers::default();
        let password = self.unlock.clone();
        self.open(password, cx);
        // The file grew by the marks.
        self.load_file_size(cx);
        cx.notify();
    }

    /// The markup bar: tools, colours, sizes, undo, redo, save.
    pub(super) fn markup_bar(
        &self,
        st: &Style,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.markup.open {
            return None;
        }
        let m = &self.markup;
        let sep = || {
            div()
                .w(px(1.0))
                .h(px(18.0))
                .flex_none()
                .bg(st.c(|c| c.line))
        };
        let mut bar = div()
            .id("markup-bar")
            .absolute()
            .top(px(self.top_inset + 10.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .p(px(5.0))
                    .rounded(px(14.0))
                    .bg(st.c(|c| c.raised))
                    .shadow(st.float())
                    .border_1()
                    .border_color(st.c(|c| c.line))
                    .text_size(px(12.0))
                    .text_color(st.c(|c| c.muted))
                    .id("markup-bar-inner")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(Tool::ALL.into_iter().map(|t| {
                        dock::dock_button(t.icon(), t.icon(), m.tool == Some(t), st)
                            .tip(t.label())
                            .on_click(cx.listener(move |r, _, _, cx| {
                                r.markup.tool = Some(t);
                                // Picking a tool puts the colour picker away.
                                r.markup.picker = false;
                                r.markup.hex = None;
                                cx.notify();
                            }))
                            .into_any_element()
                    }))
                    .child(sep())
                    .children(self.colour_swatches(st, cx))
                    .child(sep())
                    .children((0..3).map(|i| {
                        let dot = 4.0 + 3.0 * i as f32;
                        div()
                            .id(("markup-size", i))
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .when(m.size == i, |d| d.bg(st.c(|c| c.hover)))
                            .hover(|d| d.bg(st.c(|c| c.hover)))
                            .tip(["Thin", "Medium", "Thick"][i])
                            .child(div().size(px(dot)).rounded_full().bg(st.c(|c| c.text)))
                            .on_click(cx.listener(move |r, _, _, cx| {
                                r.markup.size = i;
                                cx.notify();
                            }))
                            .into_any_element()
                    }))
                    .child(sep())
                    .child(
                        dock::dock_button("markup-undo", "undo", false, st)
                            .tip("Undo  Ctrl+Z")
                            .when(m.undo.is_empty(), |b| b.opacity(0.4))
                            .on_click(cx.listener(|r, _, _, cx| r.markup_undo(cx))),
                    )
                    .child(
                        dock::dock_button("markup-redo", "redo", false, st)
                            .tip("Redo  Ctrl+Y")
                            .when(m.redo.is_empty(), |b| b.opacity(0.4))
                            .on_click(cx.listener(|r, _, _, cx| r.markup_redo(cx))),
                    ),
            );
        let picker = self.colour_picker(st, window, cx);
        // The picker takes the note's place while it is open.
        let note = m
            .note
            .clone()
            .or_else(|| (!self.is_pdf() && !m.marks.is_empty()).then(|| "Kept in Kitab".into()))
            .filter(|_| !m.picker);
        bar = bar.child(
            div()
                .absolute()
                .top(px(48.0))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .children(note.map(|n| {
                    div()
                        .px(px(10.0))
                        .py(px(4.0))
                        .rounded(px(8.0))
                        .bg(st.c(|c| c.raised))
                        .shadow(st.elev())
                        .text_size(px(12.0))
                        .text_color(st.c(|c| c.muted))
                        .child(n)
                })),
        );
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(bar)
                .children(picker)
                .into_any_element(),
        )
    }

    fn colour_swatches(&self, st: &Style, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let m = &self.markup;
        let highlighter = m.tool == Some(Tool::Highlighter);
        let (colours, current, custom_on, custom): (Palette, usize, bool, _) = if highlighter {
            (
                &ink::HIGHLIGHTER_COLOURS,
                m.highlighter_colour,
                m.use_custom_highlighter,
                m.custom_highlighter,
            )
        } else {
            (
                &ink::PEN_COLOURS,
                m.pen_colour,
                m.use_custom_pen,
                m.custom_pen,
            )
        };
        let swatch = |id: (&'static str, usize), selected: bool, fill: Option<(u8, u8, u8)>| {
            div()
                .id(id)
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .when(selected, |d| d.border_2().border_color(st.c(|c| c.accent)))
                .child(match fill {
                    Some(c) => div()
                        .size(px(14.0))
                        .rounded_full()
                        .bg(rgba(c))
                        .into_any_element(),
                    None => svg()
                        .path(icon("plus"))
                        .size(px(14.0))
                        .text_color(st.c(|c| c.muted))
                        .into_any_element(),
                })
        };
        let mut out: Vec<AnyElement> = colours
            .iter()
            .enumerate()
            .map(|(i, (name, c))| {
                swatch(("markup-colour", i), i == current && !custom_on, Some(*c))
                    .tip(*name)
                    .on_click(cx.listener(move |r, _, _, cx| {
                        let m = &mut r.markup;
                        if m.tool == Some(Tool::Highlighter) {
                            m.highlighter_colour = i;
                            m.use_custom_highlighter = false;
                        } else {
                            m.pen_colour = i;
                            m.use_custom_pen = false;
                            if matches!(m.tool, Some(Tool::Eraser) | None) {
                                m.tool = Some(Tool::Pen);
                            }
                        }
                        m.picker = false;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        out.push(
            swatch(("markup-colour", 99), custom_on || m.picker, custom)
                .tip("More colours")
                .on_click(cx.listener(|r, _, _, cx| {
                    r.markup.picker = !r.markup.picker;
                    r.markup.hex = None;
                    cx.notify();
                }))
                .into_any_element(),
        );
        out
    }

    /// Use `c` as the custom colour of the pen (or the highlighter).
    fn use_custom(&mut self, c: (u8, u8, u8)) {
        let m = &mut self.markup;
        if m.tool == Some(Tool::Highlighter) {
            m.custom_highlighter = Some(c);
            m.use_custom_highlighter = true;
        } else {
            m.custom_pen = Some(c);
            m.use_custom_pen = true;
            if matches!(m.tool, Some(Tool::Eraser) | None) {
                m.tool = Some(Tool::Pen);
            }
        }
    }

    /// Start typing a hex colour (a click on the field).
    fn edit_hex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let highlighter = self.markup.tool == Some(Tool::Highlighter);
        let now = hex_of(self.markup.colour(highlighter));
        let focus = self
            .markup
            .hex_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        self.markup.hex = Some(now);
        window.focus(&focus);
        cx.notify();
    }

    /// Typing in the hex field: hex digits and Backspace; six digits apply at
    /// once; Enter applies three or six and closes; Esc closes.
    fn hex_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.markup.hex.as_mut() else {
            return;
        };
        let k = &e.keystroke;
        match k.key.as_str() {
            "backspace" => {
                text.pop();
            }
            "enter" => {
                if let Some(c) = parse_hex(text) {
                    self.use_custom(c);
                }
                self.markup.hex = None;
                self.markup.picker = false;
                window.focus(&self.focus);
            }
            "escape" => {
                self.markup.hex = None;
                self.markup.picker = false;
                window.focus(&self.focus);
            }
            _ => {
                if let Some(ch) = k.key_char.as_ref() {
                    let ch = ch.trim_start_matches('#');
                    if !k.modifiers.control
                        && ch.chars().all(|c| c.is_ascii_hexdigit())
                        && text.len() + ch.len() <= 6
                    {
                        text.push_str(&ch.to_uppercase());
                    }
                }
            }
        }
        if let Some(c) = self
            .markup
            .hex
            .as_deref()
            .filter(|t| t.len() == 6)
            .and_then(parse_hex)
        {
            self.use_custom(c);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn hex_field(&self, st: &Style, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let highlighter = self.markup.tool == Some(Tool::Highlighter);
        let now = self.markup.colour(highlighter);
        let editing = self.markup.hex.is_some();
        let focused = self
            .markup
            .hex_focus
            .as_ref()
            .is_some_and(|f| f.is_focused(window));
        let shown = match &self.markup.hex {
            Some(t) => format!("#{t}"),
            None => format!("#{}", hex_of(now)),
        };
        let mut field = div()
            .id("markup-hex")
            .key_context(crate::actions::MARKUP_HEX)
            .on_key_down(cx.listener(|r, e: &KeyDownEvent, window, cx| r.hex_key(e, window, cx)))
            .on_click(cx.listener(|r, _, window, cx| r.edit_hex(window, cx)))
            .w(px(104.0))
            .h(px(26.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .rounded(px(7.0))
            .bg(st.c(|c| c.hover))
            .shadow(st.well())
            .border_1()
            .border_color(if editing && focused {
                st.c(|c| c.accent)
            } else {
                st.c(|c| c.line)
            })
            .cursor_text()
            .text_size(px(12.0))
            .text_color(st.c(|c| c.text))
            .tip("Type a hex colour, like #2E8B57")
            .child(shown)
            .when(editing, |d| {
                d.child(
                    div()
                        .ml(px(1.0))
                        .w(px(1.0))
                        .h(px(13.0))
                        .bg(st.c(|c| c.accent)),
                )
            });
        if let Some(f) = self.markup.hex_focus.as_ref() {
            field = field.track_focus(f);
        }
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .pt(px(8.0))
            .child(
                div()
                    .size(px(20.0))
                    .rounded(px(4.0))
                    .bg(rgba(now))
                    .border_1()
                    .border_color(st.c(|c| c.line)),
            )
            .child(field)
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(st.c(|c| c.faint))
                    .child("Hex, e.g. #2E8B57"),
            )
            .into_any_element()
    }

    /// The colour picker: a grid of hues and shades, then greys.
    fn colour_picker(
        &self,
        st: &Style,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.markup.picker {
            return None;
        }
        let highlighter = self.markup.tool == Some(Tool::Highlighter);
        let now = self.markup.colour(highlighter);
        let cells = ink::picker_grid().into_iter().enumerate().map(|(i, c)| {
            div()
                .id(("markup-pick", i))
                .size(px(20.0))
                .rounded(px(4.0))
                .bg(rgba(c))
                // A hairline so white and the palest shades show on a light panel.
                .border_1()
                .border_color(st.c(|c| c.line))
                .when(c == now, |d| d.border_2().border_color(st.c(|c| c.text)))
                .hover(|d| d.border_color(st.c(|c| c.text)))
                .on_click(cx.listener(move |r, _, _, cx| {
                    r.use_custom(c);
                    r.markup.picker = false;
                    r.markup.hex = None;
                    cx.notify();
                }))
        });
        let cells: Vec<_> = cells.collect();
        let hex = self.hex_field(st, window, cx);
        let cell = 20.0;
        let gap = 4.0;
        let width = ink::PICKER_HUES as f32 * (cell + gap) - gap;
        Some(
            div()
                .absolute()
                .top(px(self.top_inset + 62.0))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("markup-picker")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .p(px(10.0))
                        .rounded(px(12.0))
                        .bg(st.c(|c| c.raised))
                        .shadow(st.float())
                        .border_1()
                        .border_color(st.c(|c| c.line))
                        .child(
                            div()
                                .w(px(width))
                                .flex()
                                .flex_wrap()
                                .gap(px(gap))
                                .children(cells),
                        )
                        .child(hex),
                )
                .into_any_element(),
        )
    }
}

/// Paint one mark onto the page sheet whose bounds are `bounds`.
fn paint_mark(
    m: &Mark,
    bounds: Bounds<Pixels>,
    page: SizePt,
    rotation: u16,
    ppt: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let to_px = |p: PointPt| {
        let (u, v) = point_to_view(p.x, p.y, page, rotation);
        gpui::point(bounds.origin.x + px(u * ppt), bounds.origin.y + px(v * ppt))
    };
    let colour = gpui::Rgba {
        r: f32::from(m.color.r) / 255.0,
        g: f32::from(m.color.g) / 255.0,
        b: f32::from(m.color.b) / 255.0,
        a: m.opacity,
    };
    let width = (m.width * ppt).max(1.0);
    let stroke = |points: &[gpui::Point<Pixels>], closed: bool, window: &mut Window| {
        let [first, rest @ ..] = points else { return };
        let options = StrokeOptions::default()
            .with_line_width(width)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);
        let mut path = PathBuilder::stroke(px(width)).with_style(PathStyle::Stroke(options));
        path.move_to(*first);
        if rest.is_empty() {
            // A dot: a stroke needs some length to have round caps.
            path.line_to(gpui::point(first.x + px(0.01), first.y));
        }
        for p in rest {
            path.line_to(*p);
        }
        if closed {
            path.close();
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, colour);
        }
    };
    let quad = |r: RectPt, window: &mut Window| {
        let v = rect_to_view(r, page, rotation, ppt);
        window.paint_quad(gpui::fill(
            Bounds::new(
                gpui::point(bounds.origin.x + px(v.x), bounds.origin.y + px(v.y)),
                gpui::size(px(v.w), px(v.h)),
            ),
            colour,
        ));
    };
    match &m.shape {
        MarkShape::Ink { strokes } => {
            for s in strokes {
                let pts: Vec<_> = s.iter().map(|p| to_px(*p)).collect();
                stroke(&pts, false, window);
            }
        }
        MarkShape::Line { from, to, arrow } => {
            stroke(&[to_px(*from), to_px(*to)], false, window);
            if *arrow {
                let (dx, dy) = (to.x - from.x, to.y - from.y);
                let len = dx.hypot(dy).max(0.001);
                let head = (4.0 * m.width).max(8.0).min(len * 0.6);
                let angle = dy.atan2(dx);
                for side in [-0.45f32, 0.45] {
                    let a = angle + std::f32::consts::PI + side;
                    let tip = PointPt {
                        x: to.x + head * a.cos(),
                        y: to.y + head * a.sin(),
                    };
                    stroke(&[to_px(tip), to_px(*to)], false, window);
                }
            }
        }
        MarkShape::Rect { rect } => {
            let c = [
                PointPt {
                    x: rect.x,
                    y: rect.y,
                },
                PointPt {
                    x: rect.x + rect.w,
                    y: rect.y,
                },
                PointPt {
                    x: rect.x + rect.w,
                    y: rect.y + rect.h,
                },
                PointPt {
                    x: rect.x,
                    y: rect.y + rect.h,
                },
            ];
            let pts: Vec<_> = c.iter().map(|p| to_px(*p)).collect();
            stroke(&pts, true, window);
        }
        MarkShape::Ellipse { rect } => {
            let (rx, ry) = (rect.w / 2.0, rect.h / 2.0);
            let (cx, cy) = (rect.x + rx, rect.y + ry);
            let pts: Vec<_> = (0..64)
                .map(|i| {
                    let t = i as f32 / 64.0 * std::f32::consts::TAU;
                    to_px(PointPt {
                        x: cx + rx * t.cos(),
                        y: cy + ry * t.sin(),
                    })
                })
                .collect();
            stroke(&pts, true, window);
        }
        MarkShape::Highlight { rects } => rects.iter().for_each(|r| quad(*r, window)),
        MarkShape::Underline { rects } => rects.iter().for_each(|r| {
            let y = r.y + r.h - 1.0;
            stroke(
                &[
                    to_px(PointPt { x: r.x, y }),
                    to_px(PointPt { x: r.x + r.w, y }),
                ],
                false,
                window,
            )
        }),
        MarkShape::Strike { rects } => rects.iter().for_each(|r| {
            let y = r.y + r.h / 2.0;
            stroke(
                &[
                    to_px(PointPt { x: r.x, y }),
                    to_px(PointPt { x: r.x + r.w, y }),
                ],
                false,
                window,
            )
        }),
        MarkShape::Text { rect, text, size } => {
            let font = window.text_style().font();
            let colour: gpui::Hsla = colour.into();
            for (i, line) in text.lines().enumerate() {
                if line.is_empty() {
                    continue;
                }
                let origin = to_px(PointPt {
                    x: rect.x,
                    y: rect.y + i as f32 * size * TEXT_LINE,
                });
                let run = gpui::TextRun {
                    len: line.len(),
                    font: font.clone(),
                    color: colour,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(
                    SharedString::from(line.to_string()),
                    px(size * ppt),
                    &[run],
                    None,
                );
                let _ = shaped.paint(origin, px(size * TEXT_LINE * ppt), window, cx);
            }
        }
        // Notes are pins with tooltips (note_pins), not paint.
        MarkShape::Note { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32) -> PointPt {
        PointPt { x, y }
    }

    #[test]
    fn shift_snaps_lines_to_45_degrees() {
        let s = snap_45(pt(0.0, 0.0), pt(10.0, 1.0));
        assert!((s.y).abs() < 1e-4 && (s.x - 10.0499).abs() < 0.01);
        let d = snap_45(pt(0.0, 0.0), pt(10.0, 9.0));
        assert!((d.x - d.y).abs() < 1e-3);
    }

    #[test]
    fn shift_makes_squares_in_any_direction() {
        assert_eq!(square_from(pt(10.0, 10.0), pt(14.0, 30.0)), pt(30.0, 30.0));
        assert_eq!(square_from(pt(10.0, 10.0), pt(4.0, 2.0)), pt(2.0, 2.0));
    }

    #[test]
    fn clicks_without_a_drag_are_not_kept() {
        let m = |shape| Mark {
            page: 0,
            shape,
            color: Rgb8 { r: 0, g: 0, b: 0 },
            width: 2.0,
            opacity: 1.0,
        };
        assert!(!worth_keeping(&m(MarkShape::Line {
            from: pt(1.0, 1.0),
            to: pt(1.5, 1.0),
            arrow: false
        })));
        assert!(worth_keeping(&m(MarkShape::Line {
            from: pt(1.0, 1.0),
            to: pt(9.0, 1.0),
            arrow: true
        })));
        assert!(!worth_keeping(&m(MarkShape::Rect {
            rect: rect_between(pt(0.0, 0.0), pt(1.0, 40.0))
        })));
        // A pen dot is a mark.
        assert!(worth_keeping(&m(MarkShape::Ink {
            strokes: vec![vec![pt(3.0, 3.0)]]
        })));
    }
}
