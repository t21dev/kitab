// SPDX-License-Identifier: AGPL-3.0-or-later

//! Where the reader is in the document, as a fraction, for the dock's
//! scrubber and the scroll bar on the right (Settings › Reading › Scroll
//! bar). Both move continuously with the pointer instead of jumping a whole
//! page, so a short document (two pages) scrubs as smoothly as a long one.

use super::{GAP, Reader};
use crate::style::Style;
use gpui::{
    AnyElement, Bounds, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, Pixels, Styled, canvas, div, point, px,
};
use gpui::{ListOffset, prelude::FluentBuilder};

/// The thumb is never shorter than this, in pixels.
const MIN_THUMB: f32 = 28.0;

impl Reader {
    /// Page `ix`'s height in the list: its sheet and the gaps around it.
    fn item_h(&self, ix: usize) -> f32 {
        (self.sheet_top_pad(ix) + self.page_size(ix).h * self.px_per_pt(ix) + GAP / 2.0).max(1.0)
    }

    /// The furthest the list scrolls, in pages (2.4 = 40 % into page 3):
    /// the last page's bottom at the bottom of the view.
    fn max_pos(&self) -> f32 {
        let mut room = self.viewport.1.max(1.0);
        for ix in (0..self.page_count()).rev() {
            let h = self.item_h(ix);
            if room <= h {
                return ix as f32 + (h - room) / h;
            }
            room -= h;
        }
        0.0
    }

    /// How far through the document the view is (0 at the top, 1 at the
    /// end) and how much of it is on screen (0–1].
    pub(super) fn scroll_fraction(&self) -> (f32, f32) {
        let vh = self.viewport.1.max(1.0);
        if self.md.is_some() {
            let max = f32::from(self.list.max_offset_for_scrollbar().height);
            let at = -f32::from(self.list.scroll_px_offset_for_scrollbar().y);
            if max <= 0.5 {
                return (0.0, 1.0);
            }
            return (
                (at / max).clamp(0.0, 1.0),
                (vh / (max + vh)).clamp(0.0, 1.0),
            );
        }
        let n = self.page_count();
        if n == 0 {
            return (0.0, 1.0);
        }
        let max = self.max_pos();
        let top = self.list.logical_scroll_top();
        let ix = top.item_ix.min(n - 1);
        let pos = ix as f32 + f32::from(top.offset_in_item) / self.item_h(ix);
        let total: f32 = (0..n).map(|i| self.item_h(i)).sum();
        let shown = (vh / total).clamp(0.0, 1.0);
        if max <= 0.0 {
            return (0.0, shown);
        }
        ((pos / max).clamp(0.0, 1.0), shown)
    }

    /// Scroll to `f` of the way through (0 = top, 1 = end).
    pub(super) fn scroll_to_fraction(&mut self, f: f32, cx: &mut Context<Self>) {
        let f = f.clamp(0.0, 1.0);
        self.scrolled = true;
        if self.md.is_some() {
            let max = f32::from(self.list.max_offset_for_scrollbar().height);
            self.list
                .set_offset_from_scrollbar(point(px(0.0), px(-f * max)));
            cx.notify();
            return;
        }
        let n = self.page_count();
        if n == 0 {
            return;
        }
        let pos = f * self.max_pos();
        let ix = (pos.floor() as usize).min(n - 1);
        let offset = (pos - ix as f32) * self.item_h(ix);
        self.list.scroll_to(ListOffset {
            item_ix: ix,
            offset_in_item: px(offset),
        });
        cx.notify();
    }

    /// The bar's thumb: its height and top, in pixels, in a bar `h` tall.
    fn thumb(&self, h: f32) -> (f32, f32) {
        let (at, shown) = self.scroll_fraction();
        let th = (h * shown).clamp(MIN_THUMB.min(h), h);
        (th, (h - th) * at)
    }

    /// Dragging the thumb: the pointer is at window y `y`.
    pub(super) fn bar_drag_to(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let (Some(b), Some(grab)) = (self.bar.get(), self.bar_grab) else {
            return;
        };
        let h = f32::from(b.size.height);
        let (th, _) = self.thumb(h);
        let travel = (h - th).max(1.0);
        let f = (f32::from(y - b.origin.y) - grab) / travel;
        self.scroll_to_fraction(f, cx);
    }

    /// The scroll bar along the right edge of the pages, when the document
    /// is longer than the view and the setting is on.
    pub(super) fn scroll_bar(&self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !st.scroll_bar || self.page_count() == 0 {
            return None;
        }
        let (_, shown) = self.scroll_fraction();
        if shown >= 0.999 {
            return None;
        }
        let cell = self.bar.clone();
        let h = self
            .bar
            .get()
            .map_or(0.0, |b: Bounds<Pixels>| f32::from(b.size.height));
        let (th, top) = self.thumb(h);
        let dragging = self.bar_grab.is_some();
        let thumb_color = if dragging {
            st.ca(|c| c.text, 0.45)
        } else {
            st.ca(|c| c.text, 0.22)
        };
        let hover_color = st.ca(|c| c.text, 0.38);
        Some(
            div()
                .id("scroll-bar")
                .absolute()
                .top(px(self.top_inset + 4.0))
                .bottom(px(4.0))
                .right(px(2.0))
                .w(px(12.0))
                .group("scroll-bar")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |r, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let Some(b) = r.bar.get() else { return };
                        let bh = f32::from(b.size.height);
                        let (th, top) = r.thumb(bh);
                        let y = f32::from(e.position.y - b.origin.y);
                        // On the thumb: keep the grab point under the pointer.
                        // Beside it: the thumb's middle jumps to the pointer.
                        r.bar_grab = Some(if y >= top && y <= top + th {
                            y - top
                        } else {
                            th / 2.0
                        });
                        r.bar_drag_to(e.position.y, cx);
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
                        .absolute()
                        .right(px(2.0))
                        .top(px(top))
                        .h(px(th))
                        .w(px(6.0))
                        .rounded_full()
                        .bg(thumb_color)
                        .when(!dragging, |d| {
                            d.group_hover("scroll-bar", move |d| d.bg(hover_color))
                        }),
                )
                .into_any_element(),
        )
    }
}
