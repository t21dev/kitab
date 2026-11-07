// SPDX-License-Identifier: AGPL-3.0-or-later

//! Presentation mode (F5, PLAN.md Phase 3): one page at a time, fitted to the
//! whole screen on black, nothing else drawn. Right, Down, Space, PgDn, N, a
//! click or the wheel go forward; Left, Up, PgUp, P or a right click go back;
//! Home and End jump; Esc or F5 ends it on the page you were showing. Pages
//! come from the reader's renderer and share its byte-bounded cache; the next
//! page is rendered ahead so a click never waits.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct Present {
    /// The page on screen.
    pub page: usize,
    /// The same, for render jobs to check they're still wanted.
    pub shown: Arc<AtomicU64>,
    /// The last wheel step: one swipe of a touchpad sends many events, and
    /// should turn one page.
    pub last_wheel: Option<std::time::Instant>,
}

/// Wheel events closer than this to the last step turn no page.
const WHEEL_GAP: std::time::Duration = std::time::Duration::from_millis(350);

impl Reader {
    pub fn presenting(&self) -> bool {
        self.present.is_some()
    }

    /// Whether this tab can present: an open paged document, not Markdown.
    pub fn can_present(&self) -> bool {
        self.status == Status::Open && self.md.is_none() && self.page_count() > 0
    }

    pub fn start_presenting(&mut self, cx: &mut Context<Self>) {
        if !self.can_present() {
            return;
        }
        let page = self.current_page();
        self.find.open = false;
        if self.markup.open {
            self.toggle_markup(cx);
        }
        self.present = Some(Present {
            page,
            shown: Arc::new(AtomicU64::new(page as u64)),
            last_wheel: None,
        });
        cx.emit(ReaderEvent::Presenting(true));
        cx.notify();
    }

    pub fn stop_presenting(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.present.take() {
            // Reading goes on from the page that was showing.
            self.go_to_page(p.page, cx);
            cx.emit(ReaderEvent::Presenting(false));
            cx.notify();
        }
    }

    fn present_go(&mut self, page: usize, cx: &mut Context<Self>) {
        let last = self.page_count().saturating_sub(1);
        if let Some(p) = self.present.as_mut() {
            let page = page.min(last);
            if page != p.page {
                p.page = page;
                p.shown.store(page as u64, Ordering::Relaxed);
                cx.notify();
            }
        }
    }

    fn present_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(p) = self.present.as_ref() else {
            return;
        };
        let page = if forward {
            p.page + 1
        } else {
            p.page.saturating_sub(1)
        };
        self.present_go(page, cx);
    }

    /// Page `ix` fitted into `area` (logical px), from the cache or asked
    /// for; a smaller render of it stands in meanwhile.
    fn present_image(
        &mut self,
        ix: usize,
        area: (f32, f32),
        window: &Window,
        cx: &mut Context<Self>,
    ) -> (Option<Arc<RenderImage>>, (f32, f32)) {
        let size = self.page_size(ix);
        let fit = (area.0 / size.w.max(1.0)).min(area.1 / size.h.max(1.0));
        let shown = (size.w * fit, size.h * fit);
        let scale = fit * window.scale_factor();
        let key = PageKey {
            page: ix as u32,
            scale_key: scale_key(scale),
            rotation: self.rotation,
        };
        if let Some(image) = self.cache.get(&key) {
            return (Some(image), shown);
        }
        let Some(p) = self.present.as_ref() else {
            return (None, shown);
        };
        let on_screen = p.shown.clone();
        let page = ix as u64;
        let wanted: crate::engine::Wanted = Arc::new(move || {
            let now = on_screen.load(Ordering::Relaxed);
            page + 1 >= now && page <= now + 1
        });
        self.request_for(ix, key, scale, RenderFor::Thumbnail(wanted), cx);
        (self.cache.any_for_page(ix as u32, self.rotation), shown)
    }

    /// The whole reader while presenting.
    pub(super) fn present_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = self.present.as_ref().map_or(0, |p| p.page);
        let vp = window.viewport_size();
        let area = (f32::from(vp.width), f32::from(vp.height));
        let (image, (w, h)) = self.present_image(page, area, window, cx);
        // The next page, ready before it's asked for.
        if page + 1 < self.page_count() {
            let _ = self.present_image(page + 1, area, window, cx);
        }
        crate::fire_first_paint(window, cx);
        div()
            .id("reader")
            .key_context(READER)
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black())
            .cursor(gpui::CursorStyle::Arrow)
            .on_action(cx.listener(|r, _: &NextPage, _, cx| r.present_step(true, cx)))
            .on_action(cx.listener(|r, _: &PrevPage, _, cx| r.present_step(false, cx)))
            .on_action(cx.listener(|r, _: &PageRight, _, cx| {
                let forward = page_arrow_goes_forward(true, r.right_to_left());
                r.present_step(forward, cx)
            }))
            .on_action(cx.listener(|r, _: &PageLeft, _, cx| {
                let forward = page_arrow_goes_forward(false, r.right_to_left());
                r.present_step(forward, cx)
            }))
            .on_action(cx.listener(|r, _: &ScrollDown, _, cx| r.present_step(true, cx)))
            .on_action(cx.listener(|r, _: &ScrollUp, _, cx| r.present_step(false, cx)))
            .on_action(cx.listener(|r, _: &ScreenDown, _, cx| r.present_step(true, cx)))
            .on_action(cx.listener(|r, _: &ScreenUp, _, cx| r.present_step(false, cx)))
            .on_action(cx.listener(|r, _: &FirstPage, _, cx| r.present_go(0, cx)))
            .on_action(cx.listener(|r, _: &LastPage, _, cx| r.present_go(usize::MAX, cx)))
            .on_action(cx.listener(|r, _: &ClearSelection, _, cx| r.stop_presenting(cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|r, _, _, cx| r.present_step(true, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|r, _, _, cx| r.present_step(false, cx)),
            )
            .on_scroll_wheel(cx.listener(|r, e: &ScrollWheelEvent, _, cx| {
                let dy = f32::from(e.delta.pixel_delta(px(20.0)).y);
                let now = std::time::Instant::now();
                let Some(p) = r.present.as_mut() else { return };
                let rested = p
                    .last_wheel
                    .is_none_or(|t| now.duration_since(t) > WHEEL_GAP);
                if dy.abs() > 0.5 && rested {
                    p.last_wheel = Some(now);
                    r.present_step(dy < 0.0, cx);
                } else if dy.abs() > 0.5 {
                    // Still the same swipe: keep it from turning more pages.
                    p.last_wheel = Some(now);
                }
            }))
            .child(
                div()
                    .w(px(w))
                    .h(px(h))
                    .bg(gpui::white())
                    .children(image.map(|i| img(ImageSource::Render(i)).size_full())),
            )
            .into_any_element()
    }
}
