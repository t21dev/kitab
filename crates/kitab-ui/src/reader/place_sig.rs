// SPDX-License-Identifier: AGPL-3.0-or-later

//! Placing a new signature (Sign…): after the key is chosen, a click on a
//! page puts the signature's box there. A banner says so; Esc or Cancel
//! stops. The workspace takes it from there (confirm, save, sign).

use super::{Reader, ReaderEvent};
use crate::style::Style;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, StatefulInteractiveElement, Styled, div, px,
};

/// The signature box, in points.
pub const SIG_W: f32 = 180.0;
pub const SIG_H: f32 = 56.0;

impl Reader {
    /// Wait for a click on a page.
    pub fn start_placing(&mut self, cx: &mut Context<Self>) {
        self.placing = true;
        cx.notify();
    }

    pub fn stop_placing(&mut self, cx: &mut Context<Self>) {
        if self.placing {
            self.placing = false;
            cx.emit(ReaderEvent::SignaturePlaced(None));
            cx.notify();
        }
    }

    pub fn is_placing(&self) -> bool {
        self.placing
    }

    /// A click on page `ix` while placing: the box, centred on the click and
    /// kept on the page.
    pub(super) fn place_signature(
        &mut self,
        ix: usize,
        e: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(at) = self.page_point(ix, e.position) else {
            return;
        };
        let size = self.raw_page_size(ix);
        let x0 = (at.x - SIG_W / 2.0).clamp(0.0, (size.w - SIG_W).max(0.0));
        let y0 = (at.y - SIG_H / 2.0).clamp(0.0, (size.h - SIG_H).max(0.0));
        let rect = [x0, y0, (x0 + SIG_W).min(size.w), (y0 + SIG_H).min(size.h)];
        self.placing = false;
        cx.emit(ReaderEvent::SignaturePlaced(Some((ix as u32, rect))));
        cx.stop_propagation();
        cx.notify();
    }

    /// The banner across the top while placing.
    pub(super) fn placing_banner(&self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.placing {
            return None;
        }
        Some(
            div()
                .absolute()
                .top(px(12.0))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("placing-banner")
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .pl(px(14.0))
                        .pr(px(6.0))
                        .h(px(36.0))
                        .rounded(px(10.0))
                        .bg(st.c(|c| c.raised))
                        .shadow(st.modal())
                        .border_1()
                        .border_color(st.c(|c| c.line))
                        .text_size(px(12.5))
                        .text_color(st.c(|c| c.text))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child("Click on a page where the signature goes.")
                        .child(
                            div()
                                .id("placing-cancel")
                                .px(px(10.0))
                                .h(px(26.0))
                                .flex()
                                .items_center()
                                .rounded(px(7.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(st.c(|c| c.muted))
                                .cursor_pointer()
                                .hover(|d| d.bg(st.hover_bg()))
                                .child("Cancel")
                                .on_click(cx.listener(|r, _, _, cx| r.stop_placing(cx))),
                        ),
                )
                .into_any_element(),
        )
    }
}
