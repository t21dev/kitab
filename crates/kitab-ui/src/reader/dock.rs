// SPDX-License-Identifier: AGPL-3.0-or-later

//! The quick dock (DESIGN.md § Quick dock): the everyday controls in one
//! floating panel over the page, at the top, the bottom (where it takes in the
//! page navigator), or upright on the left or right (Settings › Appearance).
//! With "Fade the dock while reading" it fades after a short pause and comes
//! back when the pointer moves or a key is pressed.

use super::tools::{Click, ToolButton};
use super::{Reader, ReaderEvent, Status};
use crate::icons::icon;
use crate::motion;
use crate::settings::DockAt;
use crate::style::Style;
use crate::toolbar::{DockAlign, Tool};
use crate::tooltip::Tip;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px, svg,
};
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
    /// Hide and show animate: whether the dock was hidden at the last frame,
    /// when it was hidden (it slides out until HIDE_FOR has passed), and how
    /// many times it came back (each return slides in again).
    pub seen_hidden: std::cell::Cell<Option<bool>>,
    pub hid_at: std::cell::Cell<Option<Instant>>,
    pub shown: std::cell::Cell<u64>,
    pub shown_at: std::cell::Cell<Option<Instant>>,
}

/// When Ctrl+. (or the dock's arrow) last hid or showed the dock. Only a
/// reader on screen at that moment animates: one that was in a background tab
/// takes the new state as it is when its tab comes forward.
static TOGGLED: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

/// A menu (one the dock opened, or any other) is on screen: the dock
/// doesn't fade away from under it. Set by the shell each frame.
static MENU_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn set_menu_open(on: bool) {
    MENU_OPEN.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The dock was just hidden or shown (the ToggleDock action).
pub(crate) fn note_toggle() {
    if let Ok(mut t) = TOGGLED.lock() {
        *t = Some(Instant::now());
    }
}

fn just_toggled() -> bool {
    TOGGLED
        .lock()
        .ok()
        .and_then(|t| *t)
        .is_some_and(|t| t.elapsed() < Duration::from_millis(250))
}

/// How long the dock takes to slide out when it's hidden (Ctrl+. or its arrow).
const HIDE_FOR: Duration = Duration::from_millis(220);

impl Reader {
    /// The pointer moved or a key was pressed: show a faded dock and restart
    /// the quiet timer. One timer at a time, so moving the mouse costs nothing.
    pub(crate) fn wake_dock(&mut self, cx: &mut Context<Self>) {
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
                let busy = Style::get(cx).dock_hidden
                    || MENU_OPEN.load(std::sync::atomic::Ordering::Relaxed)
                    || r.dock.hover
                    || r.find.open
                    || r.page_edit.is_some()
                    || r.dragging;
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
        let upright = at.upright();
        let compact = st.dock_compact;
        // The dock's buttons, in the order Settings › Appearance › Toolbar
        // gives (GitHub #25). When the title bar is too narrow for its
        // buttons, they join the end of the dock (all but Open file and More,
        // which stay up there).
        let squeezed = cx
            .try_global::<crate::shell::TitleSqueezed>()
            .is_some_and(|s| s.0);
        let mut tools: Vec<Tool> = st.toolbar.dock.clone();
        if squeezed {
            let up = crate::shell::title_actions::shown_tools(&st.toolbar, true);
            tools.extend(st.toolbar.title.iter().filter(|t| !up.contains(t)));
        }
        // The compact preset, and a narrow reading area, leave out the
        // least-used first (their keys and the palette still work), so the
        // dock never has to scroll at ordinary window sizes.
        let vw = self.viewport.0;
        let tight = !upright && vw > 0.0 && vw < 820.0;
        let tighter = !upright && vw > 0.0 && vw < 640.0;
        let tightest = !upright && vw > 0.0 && vw < 520.0;
        tools.retain(|t| {
            !(compact && t.trimmed_when_compact())
                && match t {
                    Tool::Sidebar => !tightest,
                    Tool::History | Tool::Rotate | Tool::RotateLeft => !tight,
                    Tool::Fit | Tool::Hand => !tighter,
                    _ => true,
                }
        });
        // Each tool's buttons for this document: a group of buttons (back
        // and forward, zoom, fit) gets its own well; single buttons in a row
        // share one.
        let mut groups: Vec<Vec<ToolButton>> = Vec::new();
        let mut run_open = false;
        for t in tools {
            let buttons = self.tool_buttons(t, cx);
            if buttons.is_empty() {
                continue;
            }
            let multi = matches!(t, Tool::History | Tool::Zoom | Tool::Fit);
            if multi {
                groups.push(buttons);
                run_open = false;
            } else if run_open && let Some(last) = groups.last_mut() {
                last.extend(buttons);
            } else {
                groups.push(buttons);
                run_open = true;
            }
        }
        let marks = self.unsaved_marks();
        let extras = marks > 0 || self.is_markdown();
        // A hidden page navigator still shows while a page number is typed (Ctrl+G).
        let nav = at == DockAt::Bottom
            && st.shows_page_nav()
            && (st.page_nav != crate::settings::PageNav::Hidden || self.page_edit.is_some());
        // Nothing left to show but the hide chevron: no dock at all, and no
        // handle to bring back an empty one (GitHub #23).
        if groups.is_empty() && !nav && !extras {
            return None;
        }
        // Hidden or shown again since the last frame: slide out, or in.
        let hidden = st.dock_hidden;
        if self.dock.seen_hidden.get() != Some(hidden) {
            // First frame, or a tab coming forward after the change: no motion.
            let first = self.dock.seen_hidden.get().is_none() || !just_toggled();
            self.dock.seen_hidden.set(Some(hidden));
            if !first && hidden && st.animations {
                self.dock.hid_at.set(Some(Instant::now()));
                // One more frame once it's out, to put the handle there.
                let timer = cx.background_executor().timer(HIDE_FOR);
                cx.spawn(async move |this, cx| {
                    timer.await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                })
                .detach();
            } else if !hidden {
                self.dock.hid_at.set(None);
                if !first {
                    self.dock.shown.set(self.dock.shown.get() + 1);
                    self.dock.shown_at.set(Some(Instant::now()));
                }
            } else {
                self.dock.hid_at.set(None);
            }
        }
        let hiding = hidden
            && self
                .dock
                .hid_at
                .get()
                .is_some_and(|t| t.elapsed() < HIDE_FOR);
        if hidden && !hiding {
            return Some(dock_handle(
                at,
                top_inset,
                bar,
                self.unsaved_marks(),
                self.is_editing(),
                st,
            ));
        }
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
            // The page's text cursor (I-beam) stays on the page: over the
            // dock it's the arrow, and the page underneath gets no hover.
            .occlude()
            .cursor_default()
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
            }));
        // At the bottom the dock is also the page navigator.
        if nav && let Some(parts) = self.nav_parts(st, window, cx) {
            dock = dock.child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(6.0))
                    .children(parts),
            );
            if !groups.is_empty() || extras {
                dock = dock.child(sep());
            }
        }
        for buttons in groups {
            let mut g = group();
            for b in buttons {
                g = g.child(self.dock_tool_button(b, upright, st, cx));
            }
            dock = dock.child(g);
        }
        if extras {
            let mut g = group();
            // Unsaved marks in a PDF: Save, with a count, and Discard.
            if marks > 0 {
                let n = marks;
                let saving = self.markup_saving();
                g = g
                    .child(
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
                                    .child(if n > 99 { "99+".to_string() } else { n.to_string() }),
                            ),
                    )
                    // Or let them go: asks first, the file stays as it is.
                    .child(
                        dock_button("dock-discard", "close", false, st)
                            .tip("Discard unsaved changes")
                            .when(saving, |b| b.opacity(0.5))
                            .on_click(cx.listener(move |_, _, window, cx| {
                                let answer = window.prompt(
                                    gpui::PromptLevel::Warning,
                                    &format!(
                                        "Discard {n} unsaved change{}?",
                                        if n == 1 { "" } else { "s" }
                                    ),
                                    Some("Marks, deletions and form values not saved yet go. The file stays as it is."),
                                    &["Discard", "Keep"],
                                    cx,
                                );
                                cx.spawn(async move |this, cx| {
                                    if answer.await.ok() == Some(0) {
                                        let _ = this.update(cx, |r, cx| r.discard_changes(cx));
                                    }
                                })
                                .detach();
                            })),
                    );
            }
            // Markdown: typeface and text size in a small panel.
            if self.is_markdown() {
                g = g.child(
                    dock_button("dock-md-type", "typography", false, st)
                        .tip("Typography: font and size")
                        .on_click(cx.listener(|_, e: &gpui::ClickEvent, _, cx| {
                            cx.emit(ReaderEvent::TypeMenu(e.position()))
                        })),
                );
            }
            dock = dock.child(g);
        }
        // Tuck the dock into a handle at its edge: always its last button.
        dock = dock.child(
            dock_button("dock-hide", toward_edge(at), false, st)
                .tip("Hide the dock  Ctrl+.")
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(crate::actions::ToggleDock), cx)
                }),
        );

        let faded = st.dock_fade && self.dock.idle;
        let from = match at {
            DockAt::Top => motion::From::Above(6.0),
            DockAt::Bottom => motion::From::Below(8.0),
            DockAt::Left | DockAt::Right => motion::From::Fade,
        };
        // Slides further toward its edge when it's being hidden.
        let toward = match at {
            DockAt::Top => motion::From::Above(40.0),
            DockAt::Bottom => motion::From::Below(40.0),
            DockAt::Left => motion::From::Left(40.0),
            DockAt::Right => motion::From::Fade,
        };
        let id = |name: &str| {
            ElementId::NamedInteger(
                name.to_string().into(),
                self.dock.serial * 1000 + self.dock.shown.get(),
            )
        };
        // Shown again (Ctrl+. or the handle): back the way it left, as far and
        // as long, so the return reads as clearly as the exit.
        let back = self
            .dock
            .shown_at
            .get()
            .is_some_and(|t| t.elapsed() < HIDE_FOR * 2);
        let dock = if hiding {
            motion::exit(dock, id("dock-hide"), toward, HIDE_FOR)
        } else if faded {
            motion::exit(dock, id("dock-out"), from, motion::EXIT_BASE * 2)
        } else {
            motion::enter(
                dock,
                id("dock-in"),
                st.animations,
                if back { toward } else { from },
                if back {
                    HIDE_FOR + Duration::from_millis(60)
                } else {
                    motion::BASE
                },
                Duration::ZERO,
            )
        };
        let place = div().absolute();
        // Editing Markdown: the dock floats over the preview, not the source.
        let editing = self.is_editing();
        // Where along its edge: the start, the middle or the end.
        let align = st.toolbar.dock_align;
        let along = |d: gpui::Div| match align {
            DockAlign::Start => d.justify_start(),
            DockAlign::Centre => d.justify_center(),
            DockAlign::End => d.justify_end(),
        };
        let across = |d: gpui::Div| match align {
            DockAlign::Start => d.items_start().pt(px(14.0 + top_inset)),
            DockAlign::Centre => d.items_center(),
            DockAlign::End => d.items_end().pb(px(14.0)),
        };
        let placed = match at {
            DockAt::Top => along(
                over_preview(place, editing)
                    .top(px(10.0 + top_inset))
                    .right_0()
                    .px(px(16.0))
                    .flex(),
            ),
            DockAt::Bottom => along(
                over_preview(place, editing)
                    .bottom(px(16.0 + bar))
                    .right_0()
                    .px(px(16.0))
                    .flex(),
            ),
            DockAt::Left => across(
                over_preview(place, editing)
                    .ml(px(14.0))
                    .top_0()
                    .bottom(px(bar))
                    .flex(),
            ),
            DockAt::Right => across(place.right(px(14.0)).top_0().bottom(px(bar)).flex()),
        };
        Some(placed.child(dock).into_any_element())
    }
}

impl Reader {
    /// A tool's button in the dock: its icon (or Zoom's percentage), lit when
    /// on, greyed when there's nothing to do, and what a click does.
    fn dock_tool_button(
        &self,
        b: ToolButton,
        upright: bool,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(icon_name) = b.icon else {
            return div()
                .when(!upright, |d| d.min_w(px(46.0)))
                .when(upright, |d| d.py(px(2.0)).text_size(px(10.5)))
                .flex()
                .justify_center()
                .text_color(st.c(|c| c.text))
                .child(b.text)
                .into_any_element();
        };
        if !b.enabled {
            return dock_button_off(b.id, icon_name, st).into_any_element();
        }
        let el = dock_button(b.id, icon_name, b.on, st).tip(b.tip);
        match b.click {
            Click::SpeechMenu => el
                .on_click(cx.listener(|_, e: &gpui::ClickEvent, _, cx| {
                    cx.emit(ReaderEvent::SpeechMenu(e.position()))
                }))
                .into_any_element(),
            Click::MoreMenu => el
                .on_click(cx.listener(|_, e: &gpui::ClickEvent, _, cx| {
                    cx.emit(ReaderEvent::MoreMenu(e.position()))
                }))
                .into_any_element(),
            Click::Act(action) => {
                let focus = self.focus.clone();
                el.on_click(move |_, window, cx| {
                    // The tab's own focus first, so the action reaches it
                    // (and through it the workspace's sheets).
                    window.focus(&focus);
                    window.dispatch_action(action.boxed_clone(), cx);
                })
                .into_any_element()
            }
        }
    }
}

pub(super) fn dock_button(
    id: impl Into<ElementId>,
    name: &'static str,
    on: bool,
    st: &Style,
) -> gpui::Stateful<gpui::Div> {
    let fg = if on {
        st.c(|c| c.accent)
    } else {
        st.c(|c| c.muted)
    };
    crate::kbd::stop(div().id(id), st)
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

/// A dock button that can't be used right now: `ring` glyph, no hover.
pub(super) fn dock_button_off(
    id: impl Into<ElementId>,
    name: &'static str,
    st: &Style,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(28.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            svg()
                .path(icon(name))
                .size(px(16.0))
                .text_color(st.c(|c| c.ring)),
        )
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
/// The hidden dock's handle. `pending` changes not saved yet show as a
/// badge on it, so they aren't forgotten behind a hidden dock.
fn dock_handle(
    at: DockAt,
    top_inset: f32,
    bar: f32,
    pending: usize,
    editing: bool,
    st: &Style,
) -> AnyElement {
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
        // The fill stays solid on hover: a see-through highlight let the
        // shadow beneath show as a grey glow. The border and arrow light up.
        .group("dock-handle")
        .hover(|d| d.border_color(st.c(|c| c.ring)))
        .relative()
        .cursor_pointer()
        .tip(if pending > 0 {
            gpui::SharedString::from(format!(
                "Show the dock  Ctrl+.  ·  {pending} unsaved change{}",
                if pending == 1 { "" } else { "s" }
            ))
        } else {
            gpui::SharedString::from("Show the dock  Ctrl+.")
        })
        .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::actions::ToggleDock), cx))
        .when(pending > 0, |d| {
            d.child(
                div()
                    .absolute()
                    .top(px(-7.0))
                    .right(px(-7.0))
                    .min_w(px(16.0))
                    .h(px(16.0))
                    .px(px(4.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(st.c(|c| c.accent))
                    .border_2()
                    .border_color(st.c(|c| c.bg))
                    .text_size(px(9.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(st.c(|c| c.bg))
                    .child(if pending > 9 {
                        "9+".to_string()
                    } else {
                        pending.to_string()
                    }),
            )
        })
        .child(
            svg()
                .path(icon(away_from_edge(at)))
                .size(px(14.0))
                .text_color(st.c(|c| c.muted))
                .group_hover("dock-handle", |s| s.text_color(st.c(|c| c.text))),
        );
    let place = div().absolute();
    match at {
        DockAt::Bottom => over_preview(place, editing)
            .bottom(px(6.0 + bar))
            .right_0()
            .flex()
            .justify_center(),
        DockAt::Top => over_preview(place, editing)
            .top(px(6.0 + top_inset))
            .right_0()
            .flex()
            .justify_center(),
        DockAt::Left => over_preview(place, editing)
            .ml(px(6.0))
            .top_0()
            .bottom_0()
            .flex()
            .items_center(),
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

/// A floating box's left edge: the tab's, or, while Markdown is being
/// edited, the preview's (the right half), so it never covers the source.
pub(super) fn over_preview(place: gpui::Div, editing: bool) -> gpui::Div {
    if editing {
        place.left(gpui::relative(0.5))
    } else {
        place.left_0()
    }
}
