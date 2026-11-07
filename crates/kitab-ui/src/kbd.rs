// SPDX-License-Identifier: AGPL-3.0-or-later

//! Keyboard navigation (DESIGN.md § Focus). Tab and Shift+Tab move through
//! the window's controls; Enter or Space presses the focused one (GPUI turns
//! them into a click). Controls are tab stops, with an accent focus ring,
//! only while the keyboard is driving: the first Tab turns that on, any
//! mouse press turns it off. So a click never moves focus off the page
//! (the reader's single-key shortcuts keep working) and the ring never
//! shows for the mouse, like `:focus-visible` on the web.

use crate::style::Style;
use gpui::{BoxShadow, InteractiveElement, Styled, point, px};
use std::sync::atomic::{AtomicBool, Ordering};

static ON: AtomicBool = AtomicBool::new(false);

/// Whether the keyboard is driving (Tab was pressed since the last click).
pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

pub(crate) fn set(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// The ring: a shadow just outside the control. GPUI draws a shadow as a
/// filled shape under the control (and nothing with no blur), so a control
/// without its own fill gets one (`stop`) to leave only the ring showing.
fn ring(st: &Style) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: st.c(|c| c.accent),
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(1.0),
        spread_radius: px(2.0),
    }]
}

/// Make a control reachable with Tab, ringed while it has focus, when the
/// keyboard is driving; untouched otherwise. For controls drawn without a
/// fill of their own (icon buttons, rows): focused, they take `raised`.
pub fn stop<E: InteractiveElement + Styled>(el: E, st: &Style) -> E {
    if !on() {
        return el;
    }
    let (ring, fill) = (ring(st), st.c(|c| c.raised));
    el.tab_index(0).focus(move |s| s.bg(fill).shadow(ring))
}

/// [`stop`] for a control with a solid fill (buttons, switches): the ring
/// alone, its own colours kept.
pub fn stop_solid<E: InteractiveElement + Styled>(el: E, st: &Style) -> E {
    if !on() {
        return el;
    }
    let ring = ring(st);
    el.tab_index(0).focus(move |s| s.shadow(ring))
}

/// [`stop`] with the fill to show while focused: a choice that isn't the
/// picked one keeps its container's colour, so focus doesn't look like a pick.
pub fn stop_fill<E: InteractiveElement + Styled>(el: E, st: &Style, fill: gpui::Hsla) -> E {
    if !on() {
        return el;
    }
    let ring = ring(st);
    el.tab_index(0).focus(move |s| s.bg(fill).shadow(ring))
}
