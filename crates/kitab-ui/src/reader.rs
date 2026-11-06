// SPDX-License-Identifier: AGPL-3.0-or-later

//! One open document: true-size page sheets in a virtualised list (only visible
//! pages render), zoom modes, rotation, keyboard navigation, the password prompt
//! and the error placeholders (PLAN.md Phase 1, DESIGN.md § Page sheets).

use crate::actions::*;
use crate::cache::{PageCache, PageKey, scale_key};
use crate::engine::{DocEngine, DocHandle};
use crate::icons::icon;
use crate::style::Style;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    ImageSource, InteractiveElement, IntoElement, KeyDownEvent, ListAlignment, ListOffset,
    ListState, ParentElement, Render, RenderImage, SharedString, Styled, Window, div, img, list,
    px, svg,
};
use image::{Frame, RgbaImage};
use kitab_core::{DocInfo, OpenError, OutlineItem, RenderRequest, SizePt, ZoomMode};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

/// Points to logical pixels at 100 %: 1 pt = 1/72 in, 1 px = 1/96 in.
const PX_PER_PT_AT_100: f32 = 96.0 / 72.0;
/// Gap between page sheets and around them (DESIGN.md: 22 px).
const GAP: f32 = crate::layout::PAGE_GAP;
const ZOOM_STEPS: &[f32] = &[
    0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0, 6.4,
];
/// Rendered pages kept per tab (bytes). Phase 2 makes this a setting shared across tabs.
const CACHE_BUDGET: usize = 96 * 1024 * 1024;

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
}

impl EventEmitter<ReaderEvent> for Reader {}

pub struct Reader {
    pub path: PathBuf,
    engine: Arc<dyn DocEngine>,
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
}

impl Reader {
    pub fn new(
        path: PathBuf,
        engine: Arc<dyn DocEngine>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        let mut this = Self {
            path,
            engine,
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
        };
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
        let engine = self.engine.clone();
        let path = self.path.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { engine.open(&path, password.as_deref()) })
                .await;
            let _ = this.update(cx, |this, cx| this.on_opened(result, cx));
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
                self.list.reset(info.page_count as usize);
                self.doc = Some(doc);
                self.info = Some(info);
                self.status = Status::Open;
                self.password.clear();
                cx.emit(ReaderEvent::Opened);
            }
            Err(OpenError::NeedsPassword) => self.status = Status::NeedsPassword { wrong: false },
            Err(OpenError::WrongPassword) => self.status = Status::NeedsPassword { wrong: true },
            Err(e) => self.status = Status::Failed(e),
        }
        cx.notify();
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

    /// Logical pixels per point for page `ix` in the current zoom mode. Fit modes
    /// fit each page on its own, so mixed sizes keep their shape (PLAN.md §3.3).
    fn px_per_pt(&self, ix: usize) -> f32 {
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
        self.zoom = zoom;
        self.relayout(cx);
    }

    fn step_zoom(&mut self, up: bool, cx: &mut Context<Self>) {
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

    pub fn jump_to(&mut self, page: u32, cx: &mut Context<Self>) {
        self.go_to_page(page as usize, cx);
    }

    // ── rendering ─────────────────────────────────────────────────────────

    fn request(&mut self, ix: usize, key: PageKey, scale: f32, cx: &mut Context<Self>) {
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
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { engine.render(doc, req) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.pending.remove(&key);
                match result {
                    Ok(Some(bmp)) => {
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
        let image = match self.cache.get(&key) {
            Some(i) => {
                crate::fire_first_paint(window, cx);
                Some(i)
            }
            None => {
                self.request(ix, key, device_scale, cx);
                self.cache.any_for_page(ix as u32, self.rotation)
            }
        };
        div()
            .w_full()
            .flex()
            .justify_center()
            .pt(px(if ix == 0 { GAP } else { GAP / 2.0 }))
            .pb(px(GAP / 2.0))
            .child(
                div()
                    .w(px(w))
                    .h(px(h))
                    .flex_none()
                    // Paper before the bitmap arrives; the page's own colour once it has.
                    .bg(gpui::white())
                    .rounded(px(2.0))
                    .shadow(st.sheet())
                    .overflow_hidden()
                    .children(image.map(|i| img(ImageSource::Render(i)).w(px(w)).h(px(h)))),
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
        let page = self.current_page();
        if page != self.last_page {
            self.last_page = page;
            cx.emit(ReaderEvent::PageChanged);
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
            .bg(st.c(|c| c.desk))
            .on_action(cx.listener(|r, _: &NextPage, _, cx| r.go_to_page(r.current_page() + 1, cx)))
            .on_action(cx.listener(|r, _: &PrevPage, _, cx| {
                r.go_to_page(r.current_page().saturating_sub(1), cx)
            }))
            .on_action(cx.listener(|r, _: &FirstPage, _, cx| r.go_to_page(0, cx)))
            .on_action(cx.listener(|r, _: &LastPage, _, cx| r.go_to_page(usize::MAX, cx)))
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
            .on_action(cx.listener(|r, _: &RotateRight, _, cx| {
                r.rotation = (r.rotation + 90) % 360;
                r.cache.clear();
                r.relayout(cx)
            }))
            .on_action(cx.listener(|r, _: &RotateLeft, _, cx| {
                r.rotation = (r.rotation + 270) % 360;
                r.cache.clear();
                r.relayout(cx)
            }))
            .child(
                list(self.list.clone(), move |ix, window, cx| {
                    this.update(cx, |r, cx| r.page_element(ix, window, cx))
                })
                .size_full(),
            )
            .children(status)
            .children((self.status == Status::Open).then(|| self.scrubber(&st, cx)))
    }
}

impl Reader {
    /// Floating scrubber (DESIGN.md § Scrubber): page number, a track with the
    /// reading position, and the total. Clicking the track jumps.
    fn scrubber(&self, st: &Style, _cx: &mut Context<Self>) -> AnyElement {
        let count = self.page_count().max(1);
        let page = self.current_page();
        let fraction = if count > 1 {
            page as f32 / (count - 1) as f32
        } else {
            0.0
        };
        let track_w = 220.0;
        div()
            .absolute()
            .bottom(px(18.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("scrubber")
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .h(px(36.0))
                    .px(px(16.0))
                    .rounded_full()
                    .bg(st.c(|c| c.raised))
                    .shadow(st.float())
                    .text_size(px(12.0))
                    .text_color(st.c(|c| c.muted))
                    .child(format!("{}", page + 1))
                    .child(
                        div()
                            .relative()
                            .w(px(track_w))
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
                    .child(format!("{count}")),
            )
            .into_any_element()
    }
}

/// Create a reader for `path` in its own entity.
pub fn open_reader(
    path: PathBuf,
    engine: Arc<dyn DocEngine>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Reader> {
    cx.new(|cx| Reader::new(path, engine, window, cx))
}
