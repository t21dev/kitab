// SPDX-License-Identifier: AGPL-3.0-or-later

//! Home, built for big libraries: the header (title, search, buttons and the
//! shelf chips) stays put, and below it a virtual list draws only the rows
//! of cards on screen, so a watched folder of a thousand PDFs scrolls like
//! ten. Covers are the documents' own first pages, drawn in the background
//! for the cards on screen only, kept on disk by the library and a few
//! hundred at a time in memory. A scroll bar runs down the right.

use super::{RecentItem, Workspace, cover, section_title};
use crate::style::Style;
use gpui::{
    Animation, AnimationExt, AnyElement, AppContext, Bounds, Context, ElementId,
    InteractiveElement, IntoElement, ListAlignment, ListState, MouseButton, MouseDownEvent,
    ObjectFit, ParentElement, Pixels, RenderImage, Styled, StyledImage, Window, canvas, div, img,
    point, prelude::FluentBuilder, px,
};
use image::Frame;
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A card and the gap after it.
const CARD_W: f32 = 132.0;
const CARD_GAP: f32 = 20.0;
const SIDE_PAD: f32 = 40.0;
/// Covers kept in memory (a few screens of cards); more are dropped,
/// oldest first, and come back from the disk cache if their card does.
const COVERS_KEPT: usize = 96;
/// Covers waiting to be drawn; the oldest requests (cards long scrolled
/// past) are forgotten.
const QUEUE_MAX: usize = 48;
/// How far a card's cover rises under the pointer, and how quickly.
const LIFT: f32 = 6.0;
const LIFT_FOR: Duration = Duration::from_millis(160);

/// What a card shows for its cover right now.
enum CoverState {
    Ready(Arc<RenderImage>, Instant),
    Loading,
    /// No picture (missing file, or it couldn't be drawn): the generated cover.
    Generated,
}

/// How long a cover takes to fade in, and the wait before covers are drawn.
const COVER_FADE: std::time::Duration = std::time::Duration::from_millis(220);
const COVER_WAIT: std::time::Duration = std::time::Duration::from_millis(120);

enum Cover {
    Loading,
    /// The picture, and when it arrived (it fades in from then).
    Ready(Arc<RenderImage>, Instant),
    /// Couldn't be drawn: the generated cover stays.
    Failed,
}

pub(super) struct HomeView {
    list: ListState,
    /// What the rows were laid out for: items, columns, which shelf.
    shape: (usize, usize, u64),
    bar: Rc<Cell<Option<Bounds<Pixels>>>>,
    bar_grab: Option<f32>,
    covers: HashMap<PathBuf, Cover>,
    order: VecDeque<PathBuf>,
    queue: VecDeque<PathBuf>,
    busy: bool,
    /// Dropped covers, freed from the GPU on the next frame.
    evicted: Vec<Arc<RenderImage>>,
    /// The card under the pointer, and the one it just left (it settles).
    hover: Option<usize>,
    left: Option<usize>,
    /// The width Home has, from the last frame.
    pub(super) width: f32,
    /// The pixels a cover's longer side has on this screen: covers are
    /// drawn at that, no bigger.
    cover_px: u32,
    /// Middle-click scrolling, and which run of it is current.
    auto: Option<crate::autoscroll::AutoScroll>,
    auto_serial: u64,
    /// When the cards last came in (their entrance plays only then).
    shown_at: Instant,
}

impl HomeView {
    pub(super) fn new() -> Self {
        Self {
            list: ListState::new(0, ListAlignment::Top, px(600.0)),
            shape: (usize::MAX, 0, 0),
            bar: Rc::new(Cell::new(None)),
            bar_grab: None,
            covers: HashMap::new(),
            order: VecDeque::new(),
            queue: VecDeque::new(),
            busy: false,
            evicted: Vec::new(),
            hover: None,
            left: None,
            width: 1000.0,
            cover_px: 198,
            auto: None,
            auto_serial: 0,
            shown_at: Instant::now(),
        }
    }

    /// Cards came in again (Home shown, another shelf): play their entrance.
    pub(super) fn arrived(&mut self) {
        self.shown_at = Instant::now();
    }

    /// Let go of every cover (Advanced › Clear cached pages, or Home left
    /// for long). They come back from the disk cache.
    /// Free dropped covers' GPU copies (every frame, Home showing or not).
    pub(super) fn free_dropped(&mut self, window: &mut Window) {
        for image in self.evicted.drain(..) {
            window.drop_image(image).ok();
        }
    }

    /// The cards have scrolled under Home's pinned header.
    pub(super) fn scrolled(&self) -> bool {
        let top = self.list.logical_scroll_top();
        top.item_ix > 0 || top.offset_in_item > px(0.5)
    }

    pub(super) fn drop_covers(&mut self) {
        for (_, c) in self.covers.drain() {
            if let Cover::Ready(i, _) = c {
                self.evicted.push(i);
            }
        }
        self.order.clear();
        self.queue.clear();
    }
}

/// How far through a list is (0 at the top, 1 at the end) and how much of
/// it shows (0–1].
fn list_fraction(list: &ListState) -> (f32, f32) {
    let max = f32::from(list.max_offset_for_scrollbar().height);
    let at = -f32::from(list.scroll_px_offset_for_scrollbar().y);
    let vh = f32::from(list.viewport_bounds().size.height).max(1.0);
    if max <= 0.5 {
        return (0.0, 1.0);
    }
    (
        (at / max).clamp(0.0, 1.0),
        (vh / (max + vh)).clamp(0.0, 1.0),
    )
}

fn thumb(h: f32, shown: f32, at: f32) -> (f32, f32) {
    let th = (h * shown).clamp(28.0f32.min(h), h);
    (th, (h - th) * at)
}

impl Workspace {
    fn home_slice(&self) -> &[RecentItem] {
        if self.shelves.kind == crate::library::ShelfKind::Recent {
            &self.recent
        } else {
            self.shelves.items.as_deref().unwrap_or(&[])
        }
    }

    fn shelf_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{:?}", self.shelves.kind).hash(&mut h);
        h.finish()
    }

    /// The cards below the pinned header: a virtual list with a scroll bar.
    pub(super) fn home_list(
        &mut self,
        st: &Style,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.home.cover_px = (CARD_W * 1.5 * window.scale_factor()).ceil() as u32;
        let n = self.home_slice().len();
        let room = self.home.width - 2.0 * SIDE_PAD + CARD_GAP;
        let cols = ((room / (CARD_W + CARD_GAP)).floor() as usize).max(1);
        let rows = n.div_ceil(cols);
        let shape = (n, cols, self.shelf_key());
        if shape != self.home.shape {
            let count = 1 + rows;
            if shape.2 != self.home.shape.2 {
                // Another shelf: from the top.
                self.home.list.reset(count);
                self.home.arrived();
            } else {
                let old = self.home.list.item_count();
                self.home.list.splice(0..old, count);
            }
            self.home.shape = shape;
        }
        let this = cx.entity();
        let style = st.clone();
        let list = gpui::list(self.home.list.clone(), move |ix, window, cx| {
            this.update(cx, |ws, cx| ws.home_item(ix, cols, &style, window, cx))
        })
        .size_full();
        let (at, shown) = list_fraction(&self.home.list);
        let bar = (shown < 0.999).then(|| self.home_bar(st, at, shown, cx));
        div()
            .id("home-body")
            .relative()
            .flex_1()
            .min_h_0()
            .capture_any_mouse_down(cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                if ws.home.auto.is_some() && e.button != MouseButton::Middle {
                    ws.home.auto = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.toggle_home_autoscroll(e.position, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|ws, _, _, cx| {
                    if ws.home.auto.as_ref().is_some_and(|a| a.dragged) {
                        ws.home.auto = None;
                        cx.notify();
                    }
                }),
            )
            .when(self.home.auto.is_some(), |d| {
                d.cursor(gpui::CursorStyle::ResizeUpDown)
            })
            .children(
                self.home
                    .auto
                    .as_ref()
                    .map(|a| crate::autoscroll::mark(a.anchor, st)),
            )
            .on_mouse_move(cx.listener(|ws, e: &gpui::MouseMoveEvent, _, cx| {
                if let Some(a) = ws.home.auto.as_mut() {
                    a.moved(e.position, e.pressed_button == Some(MouseButton::Middle));
                }
                if ws.home.bar_grab.is_some() {
                    if e.pressed_button == Some(MouseButton::Left) {
                        ws.home_bar_drag(e.position.y, cx);
                    } else {
                        ws.home.bar_grab = None;
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    if ws.home.bar_grab.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .child(list)
            .children(bar)
            .into_any_element()
    }

    fn home_bar(&self, st: &Style, at: f32, shown: f32, cx: &mut Context<Self>) -> AnyElement {
        let cell = self.home.bar.clone();
        let h = self
            .home
            .bar
            .get()
            .map_or(0.0, |b| f32::from(b.size.height));
        let (th, top) = thumb(h, shown, at);
        let dragging = self.home.bar_grab.is_some();
        let (idle, hover) = (
            st.ca(|c| c.text, if dragging { 0.45 } else { 0.22 }),
            st.ca(|c| c.text, 0.38),
        );
        div()
            .id("home-bar")
            .absolute()
            .top(px(4.0))
            .bottom(px(4.0))
            .right(px(2.0))
            .w(px(12.0))
            .group("home-bar")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let Some(b) = ws.home.bar.get() else { return };
                    let (at, shown) = list_fraction(&ws.home.list);
                    let (th, top) = thumb(f32::from(b.size.height), shown, at);
                    let y = f32::from(e.position.y - b.origin.y);
                    ws.home.bar_grab = Some(if y >= top && y <= top + th {
                        y - top
                    } else {
                        th / 2.0
                    });
                    ws.home.list.scrollbar_drag_started();
                    ws.home_bar_drag(e.position.y, cx);
                }),
            )
            .child(
                canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .right(px(2.0))
                    .top(px(top))
                    .h(px(th))
                    .w(px(6.0))
                    .rounded_full()
                    .bg(idle)
                    .when(!dragging, |d| {
                        d.group_hover("home-bar", move |d| d.bg(hover))
                    }),
            )
            .into_any_element()
    }

    fn toggle_home_autoscroll(&mut self, at: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        if self.home.auto.take().is_some() {
            cx.notify();
            return;
        }
        self.home.auto = Some(crate::autoscroll::AutoScroll::new(at));
        self.home.auto_serial += 1;
        let serial = self.home.auto_serial;
        cx.notify();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(crate::autoscroll::TICK)
                    .await;
                let going = this
                    .update(cx, |ws, cx| {
                        let Some(a) = ws
                            .home
                            .auto
                            .as_ref()
                            .filter(|_| ws.home.auto_serial == serial)
                        else {
                            return false;
                        };
                        let step = a.step();
                        if step != 0.0 {
                            ws.home.list.scroll_by(px(step));
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

    fn home_bar_drag(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let (Some(b), Some(grab)) = (self.home.bar.get(), self.home.bar_grab) else {
            return;
        };
        let (_, shown) = list_fraction(&self.home.list);
        let h = f32::from(b.size.height);
        let (th, _) = thumb(h, shown, 0.0);
        let f = ((f32::from(y - b.origin.y) - grab) / (h - th).max(1.0)).clamp(0.0, 1.0);
        let max = f32::from(self.home.list.max_offset_for_scrollbar().height);
        self.home
            .list
            .set_offset_from_scrollbar(point(px(0.0), px(-f * max)));
        cx.notify();
    }

    /// One item of the list: the stats and the section's title first, then
    /// a row of cards.
    fn home_item(
        &mut self,
        ix: usize,
        cols: usize,
        st: &Style,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if ix == 0 {
            return self.home_top(st, cx);
        }
        let row = ix - 1;
        let items: Vec<RecentItem> = self
            .home_slice()
            .iter()
            .skip(row * cols)
            .take(cols)
            .cloned()
            .collect();
        let last = (row + 1) * cols >= self.home_slice().len();
        let fresh = self.home.shown_at.elapsed() < Duration::from_millis(700);
        let mut el = div()
            .flex()
            .gap(px(CARD_GAP))
            .px(px(SIDE_PAD))
            .pt(px(if row == 0 { 12.0 } else { 0.0 }))
            .pb(px(if last { 40.0 } else { CARD_GAP }));
        for (k, item) in items.into_iter().enumerate() {
            let i = row * cols + k;
            let card = self.recent_card(i, item, st, cx);
            el = el.child(if fresh && row < 3 {
                self.card_enter(card, "card-in", i, st)
            } else {
                card
            });
        }
        el.into_any_element()
    }

    /// What sits above the cards: the reading stats, Continue reading and
    /// the section's title on Recent; the shelf's title, or why it's empty,
    /// on the others.
    fn home_top(&mut self, st: &Style, cx: &mut Context<Self>) -> AnyElement {
        let mut col = div().flex().flex_col();
        if self.shelves.kind == crate::library::ShelfKind::Recent {
            col = col.children(self.stats_strip(st));
            let continuing: Vec<RecentItem> = self
                .recent
                .iter()
                .filter(|r| !r.missing && r.progress > 0.0 && r.progress < 0.995)
                .take(3)
                .cloned()
                .collect();
            if !continuing.is_empty() {
                let mut row = div().flex().gap(px(16.0)).px(px(SIDE_PAD)).pt(px(12.0));
                for (i, item) in continuing.into_iter().enumerate() {
                    let card = self.continue_card(i, item, st, cx);
                    row = row.child(self.card_enter(card, "continue-in", i, st));
                }
                col = col.child(section_title("Continue reading", st)).child(row);
            }
            col.child(section_title("Recent", st)).into_any_element()
        } else {
            col.child(self.shelf_heading(st)).into_any_element()
        }
    }

    /// A card's cover: the document's first page once drawn, its generated
    /// cover until then (or if it can't be drawn). Rises under the pointer.
    pub(super) fn book_cover(
        &mut self,
        i: usize,
        item: &RecentItem,
        width: f32,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = if item.missing {
            CoverState::Generated
        } else {
            self.cover_image(&item.path, cx)
        };
        let face = match state {
            CoverState::Ready(image, at) => {
                let picture = div()
                    .w(px(width))
                    .h(px(width * 1.5))
                    .flex_none()
                    .rounded(px(4.0))
                    .overflow_hidden()
                    .bg(gpui::white())
                    .shadow(st.elev())
                    .child(
                        img(image)
                            .w(px(width))
                            .h(px(width * 1.5))
                            .object_fit(ObjectFit::Cover),
                    );
                // A cover that just arrived fades in instead of popping.
                if st.animations && at.elapsed() < COVER_FADE {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    item.path.hash(&mut h);
                    picture
                        .with_animation(
                            ElementId::NamedInteger("cover-in".into(), h.finish()),
                            Animation::new(COVER_FADE).with_easing(gpui::ease_out_quint()),
                            |el, v| el.opacity(v),
                        )
                        .into_any_element()
                } else {
                    picture.into_any_element()
                }
            }
            // Still coming: a quiet block the size of the cover, so nothing
            // flashes a generated cover first.
            CoverState::Loading => div()
                .w(px(width))
                .h(px(width * 1.5))
                .flex_none()
                .rounded(px(4.0))
                .bg(st.c(|c| c.raised))
                .into_any_element(),
            CoverState::Generated => cover(&item.title, width, st),
        };
        let lifted = self.home.hover == Some(i);
        let settling = !lifted && self.home.left == Some(i);
        let el = div()
            .relative()
            .rounded(px(4.0))
            .when(lifted, |d| d.shadow(st.float()))
            .child(face);
        if !(lifted || settling) || !st.animations {
            return el.when(lifted, |d| d.top(px(-LIFT))).into_any_element();
        }
        let name = if lifted { "lift-up" } else { "lift-down" };
        el.with_animation(
            ElementId::NamedInteger(name.into(), i as u64),
            Animation::new(LIFT_FOR).with_easing(gpui::ease_out_quint()),
            move |el, v| {
                let t = if lifted { v } else { 1.0 - v };
                el.top(px(-LIFT * t))
            },
        )
        .into_any_element()
    }

    /// The pointer is over card `i`, or left it.
    pub(super) fn card_hover(&mut self, i: usize, over: bool, cx: &mut Context<Self>) {
        if over {
            if self.home.hover != Some(i) {
                self.home.left = self.home.hover;
                self.home.hover = Some(i);
                cx.notify();
            }
        } else if self.home.hover == Some(i) {
            self.home.hover = None;
            self.home.left = Some(i);
            cx.notify();
        }
    }

    /// The cover for `path` if it's drawn; otherwise ask for it.
    fn cover_image(&mut self, path: &Path, cx: &mut Context<Self>) -> CoverState {
        match self.home.covers.get(path) {
            Some(Cover::Ready(image, at)) => return CoverState::Ready(image.clone(), *at),
            Some(Cover::Loading) => return CoverState::Loading,
            Some(Cover::Failed) => return CoverState::Generated,
            None => {}
        }
        self.home.covers.insert(path.to_path_buf(), Cover::Loading);
        self.home.queue.push_back(path.to_path_buf());
        // Cards long scrolled past: forget their requests.
        while self.home.queue.len() > QUEUE_MAX {
            if let Some(old) = self.home.queue.pop_front() {
                self.home.covers.remove(&old);
            }
        }
        // A moment's wait before drawing: cards scrolled straight past drop
        // out of the queue above and are never drawn.
        let timer = cx.background_executor().timer(COVER_WAIT);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |ws, cx| ws.pump_covers(cx));
        })
        .detach();
        CoverState::Loading
    }

    /// Draw the next cover (the newest request first: it's on screen).
    fn pump_covers(&mut self, cx: &mut Context<Self>) {
        if self.home.busy {
            return;
        }
        let Some(path) = self.home.queue.pop_back() else {
            return;
        };
        self.home.busy = true;
        let library = self.library.clone();
        let size = self.home.cover_px;
        cx.spawn(async move |this, cx| {
            let p = path.clone();
            let image = cx
                .background_spawn(async move {
                    let png = library.cover(&p, size)?;
                    let mut rgba = image::load_from_memory(&png).ok()?.to_rgba8();
                    // GPUI takes BGRA.
                    for px in rgba.pixels_mut() {
                        px.0.swap(0, 2);
                    }
                    Some(Arc::new(RenderImage::new([Frame::new(rgba)])))
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                let h = &mut ws.home;
                h.busy = false;
                if h.covers.contains_key(&path) {
                    h.covers.insert(
                        path.clone(),
                        image.map_or(Cover::Failed, |i| Cover::Ready(i, Instant::now())),
                    );
                    h.order.push_back(path);
                    while h.order.len() > COVERS_KEPT {
                        if let Some(old) = h.order.pop_front() {
                            if let Some(Cover::Ready(i, _)) = h.covers.remove(&old) {
                                h.evicted.push(i);
                            }
                        }
                    }
                    cx.notify();
                }
                ws.pump_covers(cx);
            });
        })
        .detach();
    }
}
