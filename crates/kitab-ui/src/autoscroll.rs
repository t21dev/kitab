// SPDX-License-Identifier: AGPL-3.0-or-later

//! Middle-click scrolling, as in a browser: click the wheel and move the
//! pointer above or below the mark to scroll, faster the further away it
//! is; any click ends it. Pressing, dragging and letting go scrolls only
//! while the wheel is held.

use crate::style::Style;
use gpui::{
    AnyElement, IntoElement, ParentElement, Pixels, Point, Styled, anchored, deferred, div, point,
    px, svg,
};
use std::time::Duration;

/// How often the view moves while scrolling (about a frame).
pub const TICK: Duration = Duration::from_millis(16);
/// Pointer travel that doesn't scroll (and that tells a click from a drag).
const DEAD: f32 = 10.0;

pub struct AutoScroll {
    pub anchor: Point<Pixels>,
    pub pointer: Point<Pixels>,
    /// The pointer moved away with the wheel held: letting go ends it.
    pub dragged: bool,
}

impl AutoScroll {
    pub fn new(at: Point<Pixels>) -> Self {
        Self {
            anchor: at,
            pointer: at,
            dragged: false,
        }
    }

    pub fn moved(&mut self, to: Point<Pixels>, held: bool) {
        self.pointer = to;
        if held && (f32::from(to.y - self.anchor.y)).abs() > DEAD {
            self.dragged = true;
        }
    }

    /// Pixels to scroll this tick (down is positive).
    pub fn step(&self) -> f32 {
        let d = f32::from(self.pointer.y - self.anchor.y);
        let far = (d.abs() - DEAD).max(0.0);
        (far.powf(1.25) * 0.05).min(60.0) * d.signum()
    }
}

/// The mark where scrolling started: a small round with up and down arrows.
pub fn mark(at: Point<Pixels>, st: &Style) -> AnyElement {
    deferred(
        anchored()
            .position(point(at.x - px(15.0), at.y - px(15.0)))
            .child(
                div()
                    .size(px(30.0))
                    .rounded_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .bg(st.c(|c| c.raised))
                    .border_1()
                    .border_color(st.c(|c| c.line))
                    .shadow(st.float())
                    .child(
                        svg()
                            .path(crate::icons::icon("chevron-up"))
                            .size(px(11.0))
                            .text_color(st.c(|c| c.muted)),
                    )
                    .child(
                        svg()
                            .path(crate::icons::icon("chevron-down"))
                            .size(px(11.0))
                            .text_color(st.c(|c| c.muted)),
                    ),
            ),
    )
    .with_priority(2)
    .into_any_element()
}
