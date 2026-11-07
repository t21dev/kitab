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
    Eraser,
}

impl Tool {
    pub const ALL: [Tool; 7] = [
        Tool::Pen,
        Tool::Highlighter,
        Tool::Line,
        Tool::Arrow,
        Tool::Rect,
        Tool::Ellipse,
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
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tool) = self.markup.tool.filter(|_| self.markup.open) else {
            return false;
        };
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
            Tool::Eraser => return true,
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
                move |bounds, _, window, _| {
                    for m in &marks {
                        paint_mark(m, bounds, size, rotation, ppt, window);
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
        cx.notify();
    }

    /// The markup bar: tools, colours, sizes, undo, redo, save.
    pub(super) fn markup_bar(&self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
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
        let picker = self.colour_picker(st, cx);
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
                    cx.notify();
                }))
                .into_any_element(),
        );
        out
    }

    /// The colour picker: a grid of hues and shades, then greys.
    fn colour_picker(&self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
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
                    let m = &mut r.markup;
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
                    m.picker = false;
                    cx.notify();
                }))
        });
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
                        ),
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
        MarkShape::Text { rect, .. } => quad(*rect, window),
        MarkShape::Note { .. } => quad(m.bounds(), window),
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
