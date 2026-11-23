// SPDX-License-Identifier: AGPL-3.0-or-later

//! Arrange pages (Page tools): the PDF's pages as a grid of thumbnails, to
//! delete, move and turn by sight. Drag a page to move it (the picked pages
//! move together); click to pick one, Ctrl+click for one more, Shift+click
//! for a run; then turn or delete what's picked. Ctrl+Z undoes, Ctrl+A
//! picks every page. Save writes a new PDF; the open one stays as it is.
//!
//! Dragging: the hand closes, the page itself follows the pointer, the pages
//! moving fade where they were, and a bar shows where they'll land (before
//! or after the page under the pointer, by which half it's over). Held near
//! the grid's top or bottom edge, the grid scrolls.

use super::Workspace;
use super::tools_ui::{dialog_button, stem};
use crate::engine::{DocEngine, DocHandle, PageOp};
use crate::icons::icon;
use crate::reader::Reader;
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, AppContext, ClickEvent, Context, CursorStyle, DragMoveEvent, ElementId, Entity,
    FontWeight, Hsla, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ObjectFit,
    ParentElement, Render, RenderImage, ScrollHandle, StatefulInteractiveElement, Styled,
    StyledImage, Window, div, img, point, prelude::FluentBuilder, px, relative, svg,
};
use image::Frame;
use kitab_core::{RenderRequest, SizePt};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// A tile's thumbnail box, in logical pixels: a portrait page fills its
/// height, a landscape one its width.
const BOX_W: f32 = 112.0;
const BOX_H: f32 = 150.0;

/// A page of the new PDF: the original page (0-based) and its turn, in
/// degrees clockwise (0, 90, 180 or 270) on top of its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Slot {
    page: u32,
    turn: u16,
}

pub(super) struct ArrangeSheet {
    reader: Entity<Reader>,
    doc: DocHandle,
    /// The original pages' sizes in points, as they show unturned.
    sizes: Vec<SizePt>,
    slots: Vec<Slot>,
    /// Earlier arrangements, for Undo.
    history: Vec<Vec<Slot>>,
    /// The original pages picked: they stay picked wherever they move.
    picked: HashSet<u32>,
    /// The page Shift+click runs from.
    anchor: Option<u32>,
    /// Thumbnails by original page and turn.
    thumbs: HashMap<(u32, u16), Arc<RenderImage>>,
    /// Thumbnails still to draw, in order; `drawing` while one is out.
    queue: VecDeque<(u32, u16)>,
    drawing: bool,
    /// Device pixels per logical pixel when the sheet opened.
    scale: f32,
    serial: u64,
    /// While a page is dragged: the page held, and where the pages would
    /// land (next to this page: after it when true).
    dragging: Option<u32>,
    drop_at: Option<(u32, bool)>,
    grid_scroll: ScrollHandle,
    /// Pixels a step the grid scrolls by while a drag is held near its
    /// edge (up is negative); `scrolling` while that loop runs.
    edge_speed: f32,
    scrolling: bool,
}

/// A tile being dragged, in the sheet with this serial.
#[derive(Clone, Copy)]
struct DraggedTile {
    page: u32,
    serial: u64,
}

/// What follows the pointer: the page itself, lifted, with how many pages
/// move when there are more.
struct TileGhost {
    image: Option<Arc<RenderImage>>,
    w: f32,
    h: f32,
    count: usize,
    accent: Hsla,
    on_accent: Hsla,
}

impl Render for TileGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Drawn where the tile was grabbed: the box's margin, then the page.
        div()
            .pl(px(8.0))
            .pt(px(8.0))
            .cursor(CursorStyle::ClosedHand)
            .child(
                div()
                    .relative()
                    .w(px(self.w))
                    .h(px(self.h))
                    .rounded(px(3.0))
                    .bg(gpui::white())
                    .opacity(0.92)
                    .shadow_lg()
                    .border_1()
                    .border_color(self.accent)
                    .children(
                        self.image
                            .clone()
                            .map(|i| img(i).size_full().object_fit(ObjectFit::Contain)),
                    )
                    .when(self.count > 1, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(-8.0))
                                .right(px(-8.0))
                                .min_w(px(22.0))
                                .h(px(22.0))
                                .px(px(6.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(self.accent)
                                .text_color(self.on_accent)
                                .text_size(px(11.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("{}", self.count)),
                        )
                    }),
            )
    }
}

/// Move the slots whose pages are in `moving` (keeping their order) next to
/// the slot of page `target`: after it when `after`, else before it. A
/// target that is itself moving leaves things as they are.
fn move_block(slots: &mut Vec<Slot>, moving: &HashSet<u32>, target: u32, after: bool) {
    if moving.contains(&target) {
        return;
    }
    let (block, mut rest): (Vec<Slot>, Vec<Slot>) =
        slots.iter().partition(|s| moving.contains(&s.page));
    let Some(at) = rest.iter().position(|s| s.page == target) else {
        return;
    };
    let at = if after { at + 1 } else { at };
    rest.splice(at..at, block);
    *slots = rest;
}

/// The size a page shows at, turned `turn` degrees.
fn turned(size: SizePt, turn: u16) -> SizePt {
    if turn % 180 == 90 {
        SizePt {
            w: size.h,
            h: size.w,
        }
    } else {
        size
    }
}

/// The thumbnail's size in the tile's box, in logical pixels.
fn fit(size: SizePt) -> (f32, f32) {
    let (w, h) = (size.w.max(1.0), size.h.max(1.0));
    let k = (BOX_W / w).min(BOX_H / h);
    (w * k, h * k)
}

impl Workspace {
    /// Arrange pages for `reader`, a PDF that is open.
    pub(super) fn open_arrange(
        &mut self,
        reader: Entity<Reader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !reader.read(cx).is_pdf() {
            self.toast("Arrange pages works on PDFs.", true, None, cx);
            return;
        }
        let (doc, sizes) = match reader.read(cx).print_source() {
            Ok(s) => s,
            Err(why) => {
                self.toast(why, true, None, cx);
                return;
            }
        };
        if let Some(ix) = self.tabs.iter().position(|t| *t == reader)
            && self.active != Some(ix)
        {
            self.activate(ix, window, cx);
        }
        self.menu = None;
        self.menu_closing = None;
        self.page_tools = None;
        self.menu_serial += 1;
        let count = sizes.len() as u32;
        // The page being read first, so the place you are shows soonest.
        let now = (reader.read(cx).current_page() as u32).min(count.saturating_sub(1));
        let queue = (now..count).chain(0..now).map(|p| (p, 0)).collect();
        self.arrange = Some(ArrangeSheet {
            reader,
            doc,
            sizes,
            slots: (0..count).map(|page| Slot { page, turn: 0 }).collect(),
            history: Vec::new(),
            picked: HashSet::new(),
            anchor: None,
            thumbs: HashMap::new(),
            queue,
            drawing: false,
            scale: window.scale_factor(),
            serial: self.menu_serial,
            dragging: None,
            drop_at: None,
            grid_scroll: ScrollHandle::new(),
            edge_speed: 0.0,
            scrolling: false,
        });
        window.focus(&self.tools.dialog_focus);
        self.arrange_draw(cx);
        cx.notify();
    }

    pub(super) fn close_arrange(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.arrange.is_none() {
            return;
        }
        self.exit_modal("arrange", window, cx, |ws, window, cx| {
            if ws.arrange.take().is_some() {
                ws.refocus(window, cx);
                cx.notify();
            }
        });
    }

    /// Draw the next thumbnail in the queue off the UI thread, then the one
    /// after, until none is left (or the sheet closes).
    fn arrange_draw(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.arrange.as_mut() else {
            return;
        };
        if s.drawing {
            return;
        }
        let next = loop {
            match s.queue.pop_front() {
                Some(key) if s.thumbs.contains_key(&key) => continue,
                other => break other,
            }
        };
        let Some((page, turn)) = next else {
            return;
        };
        s.drawing = true;
        let size = turned(s.sizes[page as usize], turn);
        let (w, _) = fit(size);
        let req = RenderRequest {
            page,
            scale: w * s.scale / size.w.max(1.0),
            rotation: turn,
            tile: None,
            // Never stale: the reader's own generations don't concern it.
            generation: u64::MAX,
        };
        let (doc, serial) = (s.doc, s.serial);
        let engine: Arc<dyn DocEngine> = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let image = cx
                .background_spawn(async move {
                    let bitmap = engine.render(doc, req).ok()??;
                    // The engine draws BGRA, which GPUI takes as it is.
                    let buffer =
                        image::RgbaImage::from_raw(bitmap.width, bitmap.height, bitmap.bgra)?;
                    Some(Arc::new(RenderImage::new([Frame::new(buffer)])))
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                let Some(s) = ws.arrange.as_mut().filter(|s| s.serial == serial) else {
                    return;
                };
                s.drawing = false;
                if let Some(image) = image {
                    s.thumbs.insert((page, turn), image);
                }
                cx.notify();
                ws.arrange_draw(cx);
            });
        })
        .detach();
    }

    /// Change the arrangement, keeping the one before for Undo.
    fn arrange_change(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Vec<Slot>)) {
        let Some(s) = self.arrange.as_mut() else {
            return;
        };
        let before = s.slots.clone();
        f(&mut s.slots);
        if s.slots == before {
            return;
        }
        s.history.push(before);
        // Turned pages need thumbnails at their new turn: those first.
        let missing: Vec<(u32, u16)> = s
            .slots
            .iter()
            .map(|sl| (sl.page, sl.turn))
            .filter(|k| !s.thumbs.contains_key(k))
            .collect();
        for k in missing.into_iter().rev() {
            s.queue.push_front(k);
        }
        cx.notify();
        self.arrange_draw(cx);
    }

    fn arrange_undo(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.arrange.as_mut()
            && let Some(prev) = s.history.pop()
        {
            s.slots = prev;
            cx.notify();
        }
    }

    /// The pages a tile's button acts on: the picked ones when the tile is
    /// one of them, else that page alone.
    fn arrange_targets(&self, page: Option<u32>) -> HashSet<u32> {
        let Some(s) = self.arrange.as_ref() else {
            return HashSet::new();
        };
        match page {
            Some(p) if !s.picked.contains(&p) => HashSet::from([p]),
            _ => s.picked.clone(),
        }
    }

    fn arrange_turn(&mut self, page: Option<u32>, by: u16, cx: &mut Context<Self>) {
        let targets = self.arrange_targets(page);
        self.arrange_change(cx, |slots| {
            for s in slots.iter_mut().filter(|s| targets.contains(&s.page)) {
                s.turn = (s.turn + by) % 360;
            }
        });
    }

    fn arrange_delete(&mut self, page: Option<u32>, cx: &mut Context<Self>) {
        let targets = self.arrange_targets(page);
        self.arrange_change(cx, |slots| slots.retain(|s| !targets.contains(&s.page)));
        if let Some(s) = self.arrange.as_mut() {
            s.picked.retain(|p| !targets.contains(p));
        }
    }

    fn arrange_click(&mut self, page: u32, e: &ClickEvent, cx: &mut Context<Self>) {
        let Some(s) = self.arrange.as_mut() else {
            return;
        };
        let m = e.modifiers();
        let at = |p: u32| s.slots.iter().position(|sl| sl.page == p);
        if m.shift
            && let Some(a) = s.anchor.and_then(at)
            && let Some(b) = at(page)
        {
            let (lo, hi) = (a.min(b), a.max(b));
            let run: Vec<u32> = s.slots[lo..=hi].iter().map(|sl| sl.page).collect();
            s.picked.extend(run);
        } else if m.secondary() {
            if !s.picked.remove(&page) {
                s.picked.insert(page);
            }
            s.anchor = Some(page);
        } else {
            let only = s.picked.len() == 1 && s.picked.contains(&page);
            s.picked.clear();
            if !only {
                s.picked.insert(page);
            }
            s.anchor = Some(page);
        }
        cx.notify();
    }

    /// The pages that move when `page` is dragged: the picked ones if it's
    /// one of them, else it alone.
    fn arrange_moving(&self, page: u32) -> HashSet<u32> {
        match self.arrange.as_ref() {
            Some(s) if s.picked.contains(&page) => s.picked.clone(),
            _ => HashSet::from([page]),
        }
    }

    /// The pointer moved over tile `page` while a page is dragged: land the
    /// pages before it or after it, by which half the pointer is over.
    fn arrange_drag_over(
        &mut self,
        page: u32,
        e: &DragMoveEvent<DraggedTile>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let d = *e.drag(cx);
        let Some(s) = self.arrange.as_mut().filter(|s| s.serial == d.serial) else {
            return;
        };
        if s.dragging != Some(d.page) {
            s.dragging = Some(d.page);
            // The hand closes on what it holds.
            cx.set_active_drag_cursor_style(CursorStyle::ClosedHand, window);
            cx.notify();
        }
        if !e.bounds.contains(&e.event.position) {
            return;
        }
        let after = e.event.position.x > e.bounds.center().x;
        if s.drop_at != Some((page, after)) {
            s.drop_at = Some((page, after));
            cx.notify();
        }
    }

    /// A drag moved over the grid: near its top or bottom edge it scrolls,
    /// faster the nearer.
    fn arrange_drag_edge(&mut self, e: &DragMoveEvent<DraggedTile>, cx: &mut Context<Self>) {
        const ZONE: f32 = 48.0;
        let Some(s) = self.arrange.as_mut() else {
            return;
        };
        let (y, top, bottom) = (
            f32::from(e.event.position.y),
            f32::from(e.bounds.top()),
            f32::from(e.bounds.bottom()),
        );
        let inside = e.bounds.contains(&e.event.position);
        s.edge_speed = if !inside {
            0.0
        } else if y < top + ZONE {
            -14.0 * (top + ZONE - y) / ZONE
        } else if y > bottom - ZONE {
            14.0 * (y - (bottom - ZONE)) / ZONE
        } else {
            0.0
        };
        if s.edge_speed == 0.0 || s.scrolling {
            return;
        }
        s.scrolling = true;
        let serial = s.serial;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let go = this
                    .update(cx, |ws, cx| {
                        let dragging = cx.has_active_drag();
                        let Some(s) = ws.arrange.as_mut().filter(|s| s.serial == serial) else {
                            return false;
                        };
                        if !dragging || s.edge_speed == 0.0 {
                            s.scrolling = false;
                            return false;
                        }
                        let h = &s.grid_scroll;
                        let (now, max) = (h.offset(), h.max_offset());
                        let y = (now.y - px(s.edge_speed)).clamp(-max.height, px(0.0));
                        h.set_offset(point(now.x, y));
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !go {
                    break;
                }
            }
        })
        .detach();
    }

    /// The dragged page let go over the grid: the pages move to the bar.
    fn arrange_drop(&mut self, d: &DraggedTile, cx: &mut Context<Self>) {
        let Some(s) = self.arrange.as_mut().filter(|s| s.serial == d.serial) else {
            return;
        };
        let target = s.drop_at.take();
        s.dragging = None;
        s.edge_speed = 0.0;
        cx.notify();
        let Some((onto, after)) = target else {
            return;
        };
        let moving = self.arrange_moving(d.page);
        self.arrange_change(cx, |slots| move_block(slots, &moving, onto, after));
    }

    /// The operation the sheet describes, or why it can't be saved yet.
    fn arrange_op(&self) -> Result<PageOp, &'static str> {
        let s = self.arrange.as_ref().ok_or("")?;
        if s.slots.is_empty() {
            return Err("Every page is deleted; at least one has to stay.");
        }
        let same = s.slots.len() == s.sizes.len()
            && s.slots
                .iter()
                .enumerate()
                .all(|(i, sl)| sl.page == i as u32 && sl.turn == 0);
        if same {
            return Err("Nothing changed yet.");
        }
        Ok(PageOp::Arrange(
            s.slots
                .iter()
                .map(|sl| (sl.page, i32::from(sl.turn)))
                .collect(),
        ))
    }

    fn save_arrange(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(op) = self.arrange_op() else {
            return;
        };
        let Some(s) = self.arrange.take() else {
            return;
        };
        self.refocus(window, cx);
        cx.notify();
        let name = stem(&s.reader.read(cx).path);
        self.save_page_op(&s.reader, op, format!("{name} (arranged).pdf"), "pdf", cx);
    }

    fn arrange_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = e.keystroke.modifiers;
        match e.keystroke.key.as_str() {
            "enter" => self.save_arrange(window, cx),
            "escape" => self.close_arrange(window, cx),
            "delete" | "backspace" => self.arrange_delete(None, cx),
            "z" if m.secondary() => self.arrange_undo(cx),
            "a" if m.secondary() => {
                if let Some(s) = self.arrange.as_mut() {
                    s.picked = s.slots.iter().map(|sl| sl.page).collect();
                    cx.notify();
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn arrange_view(
        &mut self,
        st: &Style,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // A drag let go outside the grid (or Esc) leaves nothing behind.
        if !cx.has_active_drag()
            && let Some(s) = self.arrange.as_mut()
        {
            s.dragging = None;
            s.drop_at = None;
            s.edge_speed = 0.0;
        }
        let s = self.arrange.as_ref()?;
        let serial = s.serial;
        let title = s.reader.read(cx).title().to_string();
        let total = s.sizes.len();
        let kept = s.slots.len();
        let turned_n = s.slots.iter().filter(|sl| sl.turn != 0).count();
        let picked_n = s.picked.len();
        let op = self.arrange_op();
        let ready = op.is_ok();

        // The pages a drag is moving, faded where they are.
        let moving: Rc<HashSet<u32>> = Rc::new(
            s.dragging
                .map(|p| self.arrange_moving(p))
                .unwrap_or_default(),
        );
        let mut grid = div().flex().flex_wrap().gap(px(14.0)).p(px(12.0));
        for (i, sl) in s.slots.iter().enumerate() {
            grid = grid.child(self.arrange_tile(i, *sl, moving.clone(), st, cx));
        }
        if kept == 0 {
            grid = grid.child(
                div()
                    .w_full()
                    .py(px(40.0))
                    .flex()
                    .justify_center()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.faint))
                    .child("No pages left. Undo puts them back."),
            );
        }

        let tool = |id: &'static str, label: &'static str, ico: &'static str, on: bool| {
            crate::kbd::stop(div().id(id), st)
                .h(st.px(|m| m.control_height))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .rounded(st.px(|m| m.control_radius))
                .text_size(st.px(|m| m.ui_text))
                .when(on, |b| {
                    b.text_color(st.c(|c| c.muted))
                        .cursor_pointer()
                        .hover(|b| b.bg(st.hover_bg()).text_color(st.c(|c| c.text)))
                })
                .when(!on, |b| b.text_color(st.c(|c| c.ring)))
                .child(svg().path(icon(ico)).size(px(14.0)).text_color(if on {
                    st.c(|c| c.muted)
                } else {
                    st.c(|c| c.ring)
                }))
                .child(label)
        };
        let some = picked_n > 0;
        let can_undo = !s.history.is_empty();
        let toolbar = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                tool("arrange-left", "Turn left", "rotate-left", some)
                    .tip("Turn the picked pages a quarter to the left")
                    .on_click(cx.listener(|ws, _, _, cx| ws.arrange_turn(None, 270, cx))),
            )
            .child(
                tool("arrange-right", "Turn right", "rotate", some)
                    .tip("Turn the picked pages a quarter to the right")
                    .on_click(cx.listener(|ws, _, _, cx| ws.arrange_turn(None, 90, cx))),
            )
            .child(
                tool("arrange-delete", "Delete", "trash", some)
                    .tip("Leave the picked pages out  Del")
                    .on_click(cx.listener(|ws, _, _, cx| ws.arrange_delete(None, cx))),
            )
            .child(
                tool("arrange-undo", "Undo", "undo", can_undo)
                    .tip("Undo  Ctrl+Z")
                    .on_click(cx.listener(|ws, _, _, cx| ws.arrange_undo(cx))),
            )
            .child(
                div()
                    .ml_auto()
                    .text_size(px(12.0))
                    .text_color(st.c(|c| c.faint))
                    .child({
                        let mut parts = vec![if kept == 1 {
                            "1 page".to_string()
                        } else {
                            format!("{kept} pages")
                        }];
                        if picked_n > 0 {
                            parts.push(format!("{picked_n} picked"));
                        }
                        if kept < total {
                            parts.push(format!("{} deleted", total - kept));
                        }
                        if turned_n > 0 {
                            parts.push(format!("{turned_n} turned"));
                        }
                        parts.join(" · ")
                    }),
            );

        let panel = div()
            .id("arrange-pages")
            .track_focus(&self.tools.dialog_focus)
            .on_key_down(cx.listener(Self::arrange_key))
            .w(px(800.0))
            .max_w(relative(0.94))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(22.0))
            .rounded(px(16.0))
            .bg(st.c(|c| c.raised))
            .shadow(st.modal())
            .border_1()
            .border_color(st.c(|c| c.line))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(st.c(|c| c.text))
                    .child("Arrange pages"),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.muted))
                    .child(format!(
                        "Drag a page to move it. Click to pick it (Ctrl+click for more, Shift+click for a run), then turn or delete. From {title}; the original stays as it is."
                    )),
            )
            .child(toolbar)
            .child(
                div()
                    .id("arrange-grid")
                    .track_scroll(&s.grid_scroll)
                    .on_drag_move(cx.listener(
                        |ws, e: &DragMoveEvent<DraggedTile>, _, cx| ws.arrange_drag_edge(e, cx),
                    ))
                    .on_drop(cx.listener(|ws, d: &DraggedTile, _, cx| ws.arrange_drop(d, cx)))
                    // The rest of the sheet (title, words, buttons) and a
                    // margin take about this much: the grid gets what's left
                    // of a short window, so Save stays on screen.
                    .max_h(px((f32::from(window.viewport_size().height) - 330.0).clamp(150.0, 560.0)))
                    .overflow_y_scroll()
                    .rounded(px(12.0))
                    .bg(st.c(|c| c.hover))
                    .shadow(st.well())
                    .child(grid),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when_some(op.err().filter(|e| !e.is_empty()), |d, why| {
                        d.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(if kept == 0 {
                                    st.c(|c| c.bad)
                                } else {
                                    st.c(|c| c.faint)
                                })
                                .child(why),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        dialog_button(st, "arrange-cancel", "Cancel", false).on_click(
                            cx.listener(|ws, _, window, cx| ws.close_arrange(window, cx)),
                        ),
                    )
                    .child(
                        dialog_button(st, "arrange-save", "Save…", true)
                            .when(!ready, |b| b.opacity(0.45))
                            .on_click(cx.listener(|ws, _, window, cx| ws.save_arrange(window, cx))),
                    ),
            );
        Some(self.modal_layer(
            "arrange",
            serial,
            panel.into_any_element(),
            st,
            Self::close_arrange,
            cx,
        ))
    }

    fn arrange_tile(
        &self,
        i: usize,
        sl: Slot,
        moving: Rc<HashSet<u32>>,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(s) = self.arrange.as_ref() else {
            return div().into_any_element();
        };
        let serial = s.serial;
        let page = sl.page;
        let on = s.picked.contains(&page);
        let accent = st.c(|c| c.accent);
        let size = turned(s.sizes[page as usize], sl.turn);
        let (w, h) = fit(size);
        // The new turn's thumbnail, or the unturned one until it's drawn.
        let image = s
            .thumbs
            .get(&(page, sl.turn))
            .or_else(|| {
                s.thumbs
                    .get(&(page, 0))
                    .filter(|_| sl.turn.is_multiple_of(180))
            })
            .cloned();
        let thumb_image = image.clone();
        let thumb = div()
            .w(px(w))
            .h(px(h))
            .rounded(px(2.0))
            .overflow_hidden()
            .bg(gpui::white())
            .shadow(st.elev())
            .children(thumb_image.map(|i| img(i).size_full().object_fit(ObjectFit::Contain)));
        let button = |id: &'static str, ico: &'static str, tip: &'static str| {
            crate::kbd::stop(
                div().id(ElementId::NamedInteger(id.into(), u64::from(page))),
                st,
            )
            .size(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .bg(st.c(|c| c.raised))
            .border_1()
            .border_color(st.c(|c| c.line))
            .shadow(st.elev())
            .cursor_pointer()
            .hover(|b| b.bg(st.hover_bg()))
            .tip(tip)
            .child(
                svg()
                    .path(icon(ico))
                    .size(px(13.0))
                    .text_color(st.c(|c| c.muted)),
            )
        };
        let count = if on { s.picked.len() } else { 1 };
        let ghost_image = image.clone();
        let on_accent = st.c(|c| c.raised);
        let lifted = moving.contains(&page);
        // The bar where the dragged pages would land, in the gap beside it.
        let bar = s
            .drop_at
            .filter(|(p, _)| *p == page && !lifted && s.dragging.is_some())
            .map(|(_, after)| after);
        let label = if page as usize == i {
            format!("{}", i + 1)
        } else {
            format!("{}  (was {})", i + 1, page + 1)
        };
        div()
            .id(ElementId::NamedInteger(
                "arrange-tile".into(),
                u64::from(page),
            ))
            .group("arrange-tile")
            .flex()
            .flex_col()
            .items_center()
            .gap(px(5.0))
            .cursor(CursorStyle::OpenHand)
            .on_click(cx.listener(move |ws, e: &ClickEvent, _, cx| ws.arrange_click(page, e, cx)))
            .on_drag(DraggedTile { page, serial }, move |_, _, _, cx| {
                cx.new(|_| TileGhost {
                    image: ghost_image.clone(),
                    w,
                    h,
                    count,
                    accent,
                    on_accent,
                })
            })
            .on_drag_move(
                cx.listener(move |ws, e: &DragMoveEvent<DraggedTile>, window, cx| {
                    ws.arrange_drag_over(page, e, window, cx)
                }),
            )
            // Lifted: what's being moved stays as a faint outline.
            .when(lifted, |d| d.opacity(0.3))
            .child(
                div()
                    .relative()
                    .w(px(BOX_W + 12.0))
                    .h(px(BOX_H + 12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0))
                    .border_2()
                    .border_color(if on {
                        accent
                    } else {
                        gpui::transparent_black()
                    })
                    .when(on, |d| d.bg(st.c(|c| c.raised)))
                    .when_some(bar, |d, after| {
                        let bar = div()
                            .absolute()
                            .top(px(4.0))
                            .bottom(px(4.0))
                            .w(px(4.0))
                            .rounded_full()
                            .bg(accent);
                        // Centred in the 14 px gap between tiles.
                        d.child(if after {
                            bar.right(px(-11.0))
                        } else {
                            bar.left(px(-11.0))
                        })
                    })
                    .child(thumb)
                    .child(
                        div()
                            .absolute()
                            .top(px(4.0))
                            .right(px(4.0))
                            .flex()
                            .gap(px(4.0))
                            .opacity(0.0)
                            .when(s.dragging.is_none(), |d| {
                                d.group_hover("arrange-tile", |d| d.opacity(1.0))
                            })
                            .child(button("arrange-turn", "rotate", "Turn right").on_click(
                                cx.listener(move |ws, _, _, cx| {
                                    ws.arrange_turn(Some(page), 90, cx)
                                }),
                            ))
                            .child(button("arrange-del", "trash", "Delete  Del").on_click(
                                cx.listener(move |ws, _, _, cx| ws.arrange_delete(Some(page), cx)),
                            )),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(if on {
                        st.c(|c| c.text)
                    } else {
                        st.c(|c| c.faint)
                    })
                    .child(label),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Slot, move_block};
    use std::collections::HashSet;

    fn slots(pages: &[u32]) -> Vec<Slot> {
        pages.iter().map(|&page| Slot { page, turn: 0 }).collect()
    }

    fn pages(slots: &[Slot]) -> Vec<u32> {
        slots.iter().map(|s| s.page).collect()
    }

    #[test]
    fn picked_pages_move_together_in_their_order() {
        // One page down past another: it lands after it.
        let mut v = slots(&[0, 1, 2, 3]);
        move_block(&mut v, &HashSet::from([0]), 2, true);
        assert_eq!(pages(&v), [1, 2, 0, 3]);
        // One page up: before the one dropped on.
        let mut v = slots(&[0, 1, 2, 3]);
        move_block(&mut v, &HashSet::from([3]), 1, false);
        assert_eq!(pages(&v), [0, 3, 1, 2]);
        // Two apart, together to the end, keeping their order.
        let mut v = slots(&[0, 1, 2, 3, 4]);
        move_block(&mut v, &HashSet::from([3, 0]), 4, true);
        assert_eq!(pages(&v), [1, 2, 4, 0, 3]);
        // Dropped on one of the moving pages: nothing changes.
        let mut v = slots(&[0, 1, 2]);
        move_block(&mut v, &HashSet::from([0, 1]), 1, true);
        assert_eq!(pages(&v), [0, 1, 2]);
    }
}
