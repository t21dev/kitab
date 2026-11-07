// SPDX-License-Identifier: AGPL-3.0-or-later

//! The quick dock (DESIGN.md § Quick dock): the everyday controls in one
//! floating panel over the page, at the top, the bottom (where it takes in the
//! page navigator), or upright on the left or right (Settings › Appearance).
//! With "Fade the dock while reading" it fades after a short pause and comes
//! back when the pointer moves or a key is pressed.

use super::{Reader, ReaderEvent, Status};
use crate::actions::*;
use crate::icons::icon;
use crate::motion;
use crate::settings::DockAt;
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px, svg,
};
use kitab_core::ZoomMode;
use std::time::{Duration, Instant};

/// Quiet time before a fading dock goes.
const IDLE: Duration = Duration::from_millis(1800);

/// Space the sidebar-tabs layout keeps free at the top of the page area for
/// the window buttons and dragging the window.
pub const TOP_INSET: f32 = 36.0;

#[derive(Default)]
pub(super) struct DockState {
    pub idle: bool,
    pub hover: bool,
    /// Bumped each time the dock comes back, so it fades in again.
    pub serial: u64,
    pub last_activity: Option<Instant>,
    pub timer: bool,
    /// The dock scrolls along its length when the window is too small for it.
    pub scroll: gpui::ScrollHandle,
}

impl Reader {
    /// The pointer moved or a key was pressed: show a faded dock and restart
    /// the quiet timer. One timer at a time, so moving the mouse costs nothing.
    pub(super) fn wake_dock(&mut self, cx: &mut Context<Self>) {
        if !Style::get(cx).dock_fade {
            return;
        }
        self.dock.last_activity = Some(Instant::now());
        if self.dock.idle {
            self.dock.idle = false;
            self.dock.serial += 1;
            cx.notify();
        }
        if !self.dock.timer {
            self.dock.timer = true;
            self.arm_dock_timer(IDLE, cx);
        }
    }

    fn arm_dock_timer(&mut self, wait: Duration, cx: &mut Context<Self>) {
        let timer = cx.background_executor().timer(wait);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |r, cx| {
                let since = r.dock.last_activity.map_or(IDLE, |t| t.elapsed());
                if since < IDLE {
                    r.arm_dock_timer(IDLE - since, cx);
                    return;
                }
                r.dock.timer = false;
                let busy = r.dock.hover || r.find.open || r.page_edit.is_some() || r.dragging;
                if !busy && Style::get(cx).dock_fade {
                    r.dock.idle = true;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The dock, placed for the current setting.
    pub(super) fn dock_view(
        &self,
        st: &Style,
        top_inset: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.status != Status::Open || !st.shows_dock() {
            return None;
        }
        let at = st.dock;
        // The status bar, when shown, is the bottom edge the dock floats over.
        let bar = if self.status_bar_shown(st) {
            super::info::BAR_H
        } else {
            0.0
        };
        if st.dock_hidden {
            return Some(dock_handle(at, top_inset, bar, st));
        }
        let upright = at.upright();
        let mode = self.zoom;
        let group = || {
            div()
                .flex_none()
                .flex()
                .when(upright, |d| d.flex_col())
                .items_center()
                .gap(px(2.0))
                .p(px(2.0))
                .rounded(px(9.0))
                .bg(st.c(|c| c.hover))
                .shadow(st.well())
        };
        let sep = || {
            div().flex_none().bg(st.c(|c| c.line)).map(|d| {
                if upright {
                    d.w(px(18.0)).h(px(1.0))
                } else {
                    d.w(px(1.0)).h(px(18.0))
                }
            })
        };
        // A book shows its text size: zoom there changes the text, not the page.
        let zoom_label = match self.book_text() {
            Some(pt) => format!("{pt:.0} pt"),
            None => format!("{} %", self.zoom_percent()),
        };
        let compact = st.dock_compact;
        // What the dock shows (Settings › Appearance › Dock items); the
        // compact preset trims the rest. More and Hide are always there.
        // What the title bar shows is left out of the dock (all but More come
        // back here when the window is too narrow for them up there).
        let squeezed = cx
            .try_global::<crate::shell::TitleSqueezed>()
            .is_some_and(|s| s.0);
        let t = if squeezed {
            crate::settings::TitleItems {
                more: st.title_items.more,
                ..crate::settings::TitleItems::NONE
            }
        } else {
            st.title_items
        };
        let mut items = st.dock_items;
        items.sidebar &= !t.sidebar;
        items.history &= !t.history;
        items.zoom &= !t.zoom;
        items.fit &= !t.fit;
        items.find &= !t.find;
        items.markup &= !t.markup;
        items.rotate &= !t.rotate;
        items.bookmark &= !t.bookmark;
        items.minimal &= !t.minimal;
        let show_sidebar = items.sidebar && !compact;
        let show_history = items.history && !compact;
        let show_fit = items.fit && !compact;
        let show_rotate = items.rotate && !compact;
        let mut dock = div()
            .id("dock")
            .flex()
            .when(upright, |d| d.flex_col())
            .items_center()
            .gap(px(6.0))
            .p(px(5.0))
            .rounded(px(14.0))
            .bg(st.c(|c| c.raised))
            .shadow(st.float())
            .border_1()
            .border_color(st.c(|c| c.line))
            .text_size(px(12.0))
            .text_color(st.c(|c| c.muted))
            .on_hover(cx.listener(|r, over: &bool, _, _| r.dock.hover = *over))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            // Never wider (or taller) than the reading area: in a small window
            // the dock scrolls along its length instead of running off it.
            .map(|d| {
                let (vw, vh) = self.viewport;
                if upright && vh > 0.0 {
                    d.max_h(px((vh - top_inset - bar - 28.0).max(40.0)))
                        .overflow_y_scroll()
                } else if !upright && vw > 0.0 {
                    d.max_w(px((vw - 16.0).max(40.0))).overflow_x_scroll()
                } else {
                    d
                }
            })
            .track_scroll(&self.dock.scroll)
            .on_scroll_wheel(cx.listener(move |r, e: &gpui::ScrollWheelEvent, _, cx| {
                // A mouse wheel turns a sideways dock that doesn't fit.
                let room = r.dock.scroll.max_offset();
                let d = e.delta.pixel_delta(px(20.0));
                if !upright && room.width > px(0.5) && d.y.abs() > d.x.abs() {
                    let mut at = r.dock.scroll.offset();
                    at.x = (at.x + d.y).clamp(-room.width, px(0.0));
                    r.dock.scroll.set_offset(at);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .when(show_sidebar, |d| {
                d.child(
                    group().child(
                        dock_button("dock-sidebar", "sidebar", false, st)
                            .tip("Sidebar  Ctrl+B")
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ToggleSidebar), cx)
                            }),
                    ),
                )
            })
            .when(show_history, |d| {
                d.child(
                    group()
                        .child(
                            dock_button("dock-back", "arrow-left", false, st)
                                .when(!self.can_go_back(), |b| b.opacity(0.4))
                                .tip("Back  Alt+Left")
                                .on_click(cx.listener(|r, _, _, cx| r.history(true, cx))),
                        )
                        .child(
                            dock_button("dock-forward", "arrow-right", false, st)
                                .when(!self.can_go_forward(), |b| b.opacity(0.4))
                                .tip("Forward  Alt+Right")
                                .on_click(cx.listener(|r, _, _, cx| r.history(false, cx))),
                        ),
                )
            });
        // At the bottom the dock is also the page navigator.
        if at == DockAt::Bottom && st.shows_page_nav() {
            if let Some(parts) = self.nav_parts(st, window, cx) {
                dock = dock.child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(6.0))
                        .children(parts),
                );
                dock = dock.child(sep());
            }
        }
        dock = dock
            .when(items.zoom || show_fit, |d| {
                d.child(
                    group()
                        .when(items.zoom, |g| {
                            g.child(
                                dock_button("dock-zoom-out", "minus", false, st)
                                    .tip("Zoom out  Ctrl+−")
                                    .on_click(cx.listener(|r, _, _, cx| r.zoom_step(false, cx))),
                            )
                            .child(
                                div()
                                    .when(!upright, |d| d.min_w(px(46.0)))
                                    .when(upright, |d| d.py(px(2.0)).text_size(px(10.5)))
                                    .flex()
                                    .justify_center()
                                    .text_color(st.c(|c| c.text))
                                    .child(zoom_label),
                            )
                            .child(
                                dock_button("dock-zoom-in", "plus", false, st)
                                    .tip("Zoom in  Ctrl+=")
                                    .on_click(cx.listener(|r, _, _, cx| r.zoom_step(true, cx))),
                            )
                        })
                        .when(show_fit, |g| {
                            g.child(
                                dock_button(
                                    "dock-fit-width",
                                    "fit-width",
                                    mode == ZoomMode::FitWidth,
                                    st,
                                )
                                .tip("Fit width  Ctrl+2")
                                .on_click(cx.listener(
                                    |r, _, _, cx| r.set_zoom_mode(ZoomMode::FitWidth, cx),
                                )),
                            )
                            .child(
                                dock_button(
                                    "dock-fit-page",
                                    "fit-page",
                                    mode == ZoomMode::FitPage,
                                    st,
                                )
                                .tip("Fit page  Ctrl+0")
                                .on_click(
                                    cx.listener(|r, _, _, cx| {
                                        r.set_zoom_mode(ZoomMode::FitPage, cx)
                                    }),
                                ),
                            )
                        }),
                )
            })
            .child(
                group()
                    .when(items.find, |g| {
                        g.child(
                            dock_button("dock-find", "search", self.find.open, st)
                                .tip("Find  Ctrl+F")
                                .on_click(cx.listener(|r, _, window, cx| {
                                    if r.find.open && !r.find.closing {
                                        r.close_find(window, cx)
                                    } else {
                                        r.open_find(window, cx)
                                    }
                                })),
                        )
                    })
                    .when(items.bookmark && self.can_bookmark(), |g| {
                        let marked = self.is_bookmarked(self.current_page() as u32);
                        g.child(
                            dock_button("dock-bookmark", "bookmark", marked, st)
                                .tip(if marked {
                                    "Remove bookmark  Ctrl+D"
                                } else {
                                    "Bookmark this page  Ctrl+D"
                                })
                                .on_click(cx.listener(|r, _, _, cx| r.toggle_bookmark(cx))),
                        )
                    })
                    .when(items.markup && self.can_markup(cx), |g| {
                        g.child(
                            dock_button("dock-markup", "pen", self.markup.open, st)
                                .tip("Draw and mark up  Ctrl+Shift+D")
                                .on_click(cx.listener(|r, _, _, cx| r.toggle_markup(cx))),
                        )
                    })
                    // Unsaved marks in a PDF: Save sits beside the pen, with a count.
                    .when(self.unsaved_marks() > 0, |g| {
                        let n = self.unsaved_marks();
                        let saving = self.markup_saving();
                        g.child(
                            dock_button("dock-save-marks", "save", false, st)
                                .relative()
                                .tip(format!(
                                    "Save {n} mark{} into the PDF  Ctrl+S",
                                    if n == 1 { "" } else { "s" }
                                ))
                                .when(saving, |b| b.opacity(0.5))
                                .on_click(cx.listener(|r, _, _, cx| r.save_marks(cx)))
                                .child(
                                    div()
                                        .absolute()
                                        .top(px(1.0))
                                        .right(px(1.0))
                                        .min_w(px(14.0))
                                        .h(px(14.0))
                                        .px(px(3.0))
                                        .rounded_full()
                                        .bg(st.c(|c| c.accent))
                                        .text_color(st.c(|c| c.bg))
                                        .text_size(px(9.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(if n > 99 {
                                            "99+".to_string()
                                        } else {
                                            n.to_string()
                                        }),
                                ),
                        )
                    })
                    .when(show_rotate, |g| {
                        g.child(
                            dock_button("dock-rotate", "rotate", false, st)
                                .tip("Rotate  Ctrl+Shift+=")
                                .on_click(cx.listener(|r, _, _, cx| r.rotate(true, cx))),
                        )
                    })
                    .when(items.minimal, |g| {
                        g.child(
                            dock_button("dock-minimal", "focus", st.minimal, st)
                                .tip("Minimal mode  F11")
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(
                                        Box::new(crate::actions::ToggleMinimal),
                                        cx,
                                    )
                                }),
                        )
                    })
                    .when(!t.more, |g| {
                        g.child(
                            dock_button("dock-more", "more-vertical", false, st)
                                .tip("More")
                                .on_click(cx.listener(|_, e: &gpui::ClickEvent, _, cx| {
                                    cx.emit(ReaderEvent::MoreMenu(e.position()))
                                })),
                        )
                    })
                    // Tuck the dock into a handle at its edge.
                    .child(
                        dock_button("dock-hide", toward_edge(at), false, st)
                            .tip("Hide the dock  Ctrl+.")
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(crate::actions::ToggleDock), cx)
                            }),
                    ),
            );

        let faded = st.dock_fade && self.dock.idle;
        let from = match at {
            DockAt::Top => motion::From::Above(6.0),
            DockAt::Bottom => motion::From::Below(8.0),
            DockAt::Left | DockAt::Right => motion::From::Fade,
        };
        let id = |name: &str| ElementId::NamedInteger(name.to_string().into(), self.dock.serial);
        let dock = if faded {
            motion::exit(dock, id("dock-out"), from, motion::EXIT_BASE * 2)
        } else {
            motion::enter(
                dock,
                id("dock-in"),
                st.animations,
                from,
                motion::BASE,
                Duration::ZERO,
            )
        };
        let place = div().absolute();
        let placed = match at {
            DockAt::Top => place
                .top(px(10.0 + top_inset))
                .left_0()
                .right_0()
                .flex()
                .justify_center(),
            DockAt::Bottom => place
                .bottom(px(16.0 + bar))
                .left_0()
                .right_0()
                .flex()
                .justify_center(),
            DockAt::Left => place
                .left(px(14.0))
                .top_0()
                .bottom(px(bar))
                .flex()
                .items_center(),
            DockAt::Right => place
                .right(px(14.0))
                .top_0()
                .bottom(px(bar))
                .flex()
                .items_center(),
        };
        Some(placed.child(dock).into_any_element())
    }
}

pub(super) fn dock_button(
    id: &'static str,
    name: &'static str,
    on: bool,
    st: &Style,
) -> gpui::Stateful<gpui::Div> {
    let fg = if on {
        st.c(|c| c.accent)
    } else {
        st.c(|c| c.muted)
    };
    div()
        .id(id)
        .size(px(28.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .when(on, |d| d.bg(st.c(|c| c.raised)).shadow(st.elev()))
        .hover(|d| d.bg(st.c(|c| c.raised)))
        .child(svg().path(icon(name)).size(px(16.0)).text_color(fg))
}

/// The chevron that points at the dock's edge (hide) or away from it (show).
fn toward_edge(at: DockAt) -> &'static str {
    match at {
        DockAt::Bottom => "chevron-down",
        DockAt::Top => "chevron-up",
        DockAt::Left => "chevron-left",
        DockAt::Right => "chevron-right",
    }
}

fn away_from_edge(at: DockAt) -> &'static str {
    match at {
        DockAt::Bottom => "chevron-up",
        DockAt::Top => "chevron-down",
        DockAt::Left => "chevron-right",
        DockAt::Right => "chevron-left",
    }
}

/// The hidden dock: a small handle at its edge. A click (or Ctrl+.) shows it.
fn dock_handle(at: DockAt, top_inset: f32, bar: f32, st: &Style) -> AnyElement {
    let upright = at.upright();
    let handle = div()
        .id("dock-handle")
        .when(upright, |d| d.w(px(20.0)).h(px(64.0)))
        .when(!upright, |d| d.w(px(64.0)).h(px(20.0)))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(9.0))
        .bg(st.c(|c| c.raised))
        .border_1()
        .border_color(st.c(|c| c.line))
        // Floats like the dock, so it reads over a white page.
        .shadow(st.float())
        .hover(|d| d.bg(st.c(|c| c.hover)))
        .tip("Show the dock  Ctrl+.")
        .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::actions::ToggleDock), cx))
        .child(
            svg()
                .path(icon(away_from_edge(at)))
                .size(px(14.0))
                .text_color(st.c(|c| c.muted)),
        );
    let place = div().absolute();
    match at {
        DockAt::Bottom => place
            .bottom(px(6.0 + bar))
            .left_0()
            .right_0()
            .flex()
            .justify_center(),
        DockAt::Top => place
            .top(px(6.0 + top_inset))
            .left_0()
            .right_0()
            .flex()
            .justify_center(),
        DockAt::Left => place.left(px(6.0)).top_0().bottom_0().flex().items_center(),
        DockAt::Right => place
            .right(px(6.0))
            .top_0()
            .bottom_0()
            .flex()
            .items_center(),
    }
    .child(handle)
    .into_any_element()
}
