// SPDX-License-Identifier: AGPL-3.0-or-later

//! Settings › Appearance › Toolbar (GitHub #25, DESIGN.md § Toolbar): the
//! title bar and the dock drawn as they look, edited in place. Drag a button
//! along its bar or onto the other one; click it for Earlier, Later, Move and
//! Take away; the accent + at the end of each bar adds one. A tip says so
//! until it's put away.

use super::Workspace;
use crate::icons::icon;
use crate::style::Style;
use crate::toolbar::{Bar, DockAlign, Tool};
use crate::tooltip::Tip;
use gpui::{
    AnyElement, App, AppContext, Context, CursorStyle, ElementId, Hsla, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    WeakEntity, Window, div, prelude::FluentBuilder, px, svg,
};

/// A toolbar button being dragged in Settings.
#[derive(Clone, Copy)]
pub(crate) struct DraggedTool(pub Tool);

/// What follows the pointer while a button is dragged: the button itself.
struct Ghost {
    icon: &'static str,
    fg: Hsla,
    bg: Hsla,
    line: Hsla,
}

impl Render for Ghost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .bg(self.bg)
            .border_1()
            .border_color(self.line)
            .shadow_md()
            .cursor(CursorStyle::ClosedHand)
            .child(
                svg()
                    .path(icon(self.icon))
                    .size(px(15.0))
                    .text_color(self.fg),
            )
    }
}

/// Run `f` on the workspace (a click in the editor).
fn on_ws(
    ws: &WeakEntity<Workspace>,
    f: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
    let ws = ws.clone();
    move |_, window, cx| {
        let _ = ws.update(cx, |ws, cx| f(ws, window, cx));
    }
}

/// Change the toolbar setting and keep `sel` selected (None: nothing).
fn edit(
    ws: &WeakEntity<Workspace>,
    sel: Option<Tool>,
    f: impl Fn(&mut crate::toolbar::Toolbar) + 'static,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
    on_ws(ws, move |ws, _, cx| {
        ws.toolbar_sel = sel;
        ws.change_style(&|s: &mut Style| f(&mut s.toolbar), cx);
    })
}

pub(super) fn editor(ws: &WeakEntity<Workspace>, st: &Style, sel: Option<Tool>) -> AnyElement {
    let sel = sel.filter(|t| st.toolbar.place(*t).is_some());
    let sel_bar = sel.and_then(|t| st.toolbar.place(t)).map(|(b, _)| b);
    let faint = st.c(|c| c.faint);
    let label = |name: &'static str| {
        div()
            .flex()
            .gap(px(4.0))
            .text_size(px(12.0))
            .text_color(faint)
            .child(name)
            .child(
                div()
                    .text_color(st.c(|c| c.ring))
                    .child("·  drag to reorder"),
            )
    };
    let mut col = div().flex().flex_col().gap(px(10.0));
    if !st.toolbar_tip_seen {
        col = col.child(tip(ws, st));
    }
    col = col
        .child(label("Title bar"))
        .child(title_strip(ws, st, sel))
        .when(sel_bar == Some(Bar::Title), |d| {
            d.children(sel.map(|t| action_line(ws, st, t)))
        })
        .child(
            div()
                .flex()
                .items_center()
                .mt(px(4.0))
                .child(div().flex_1().child(label("Dock")))
                .child(align_group(ws, st)),
        )
        .child(dock_strip(ws, st, sel))
        .when(sel_bar == Some(Bar::Dock), |d| {
            d.children(sel.map(|t| action_line(ws, st, t)))
        });
    col.into_any_element()
}

/// The first-time tip: drag to move, + to add.
fn tip(ws: &WeakEntity<Workspace>, st: &Style) -> AnyElement {
    let accent = st.c(|c| c.accent);
    let item = |glyph: AnyElement, title: &'static str, body: &'static str| {
        div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(glyph)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().text_color(st.c(|c| c.text)).child(title))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(st.c(|c| c.muted))
                            .child(body),
                    ),
            )
    };
    let drag_glyph = div()
        .size(px(30.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .bg(st.c(|c| c.raised))
        .child(svg().path(icon("hand")).size(px(16.0)).text_color(accent))
        .into_any_element();
    let add_glyph = div()
        .size(px(30.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_dashed()
        .border_color(accent.opacity(0.7))
        .child(svg().path(icon("plus")).size(px(13.0)).text_color(accent))
        .into_any_element();
    div()
        .flex()
        .items_center()
        .gap(px(18.0))
        .p(px(10.0))
        .pl(px(14.0))
        .rounded(px(10.0))
        .bg(accent.opacity(0.08))
        .border_1()
        .border_color(accent.opacity(0.3))
        .text_size(st.px(|m| m.ui_text))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(item(
                    drag_glyph,
                    "Drag a button to move it",
                    "Along its bar, or onto the other one. Or click it for Earlier, Later and Take away.",
                ))
                .child(item(
                    add_glyph,
                    "+ adds a button at the end",
                    "Every button Kitab has is in its list, even ones you haven't seen.",
                )),
        )
        .child(
            crate::kbd::stop_solid(div().id("toolbar-tip-ok"), st)
                .flex_none()
                .h(px(28.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .rounded(px(8.0))
                .bg(st.c(|c| c.raised))
                .shadow(st.elev())
                .cursor_pointer()
                .text_color(st.c(|c| c.text))
                .child("Got it")
                .on_click(on_ws(ws, |ws, _, cx| {
                    ws.change_style(&|s: &mut Style| s.toolbar_tip_seen = true, cx)
                })),
        )
        .into_any_element()
}

/// The icons a tool shows on a bar (None: Zoom's percentage).
fn parts(tool: Tool) -> &'static [Option<&'static str>] {
    match tool {
        Tool::History => &[Some("arrow-left"), Some("arrow-right")],
        Tool::Zoom => &[Some("minus"), None, Some("plus")],
        Tool::Fit => &[Some("fit-width"), Some("fit-page")],
        Tool::Sidebar => &[Some("sidebar")],
        Tool::Find => &[Some("search")],
        Tool::Bookmark => &[Some("bookmark")],
        Tool::Hand => &[Some("hand")],
        Tool::TwoPages => &[Some("pages")],
        Tool::BookView => &[Some("book")],
        Tool::RotateLeft => &[Some("rotate-left")],
        Tool::Rotate => &[Some("rotate")],
        Tool::Focus => &[Some("focus")],
        Tool::Markup => &[Some("pen")],
        Tool::ReadAloud => &[Some("speaker")],
        Tool::Present => &[Some("play")],
        Tool::Password => &[Some("lock")],
        Tool::Open => &[Some("folder-open")],
        Tool::Print => &[Some("print")],
        Tool::Export => &[Some("download")],
        Tool::More => &[Some("more-vertical")],
    }
}

/// One tool on a bar: its buttons as they look there, draggable as one,
/// selected on click, and a drop target that puts what's dropped before it.
fn tool_item(
    ws: &WeakEntity<Workspace>,
    st: &Style,
    bar: Bar,
    tool: Tool,
    selected: bool,
) -> AnyElement {
    let accent = st.c(|c| c.accent);
    let muted = st.c(|c| c.muted);
    let text = st.c(|c| c.text);
    let size = st.px(|m| m.icon_button);
    let mut item = div()
        .id(ElementId::Name(format!("tb-{bar:?}-{tool:?}").into()))
        .flex()
        .items_center()
        .gap(px(2.0))
        .rounded(st.px(|m| m.control_radius))
        .border_2()
        .border_color(if selected {
            accent
        } else {
            gpui::transparent_black()
        })
        .when(selected, |d| d.bg(st.c(|c| c.raised)))
        .when(!selected, |d| d.hover(|d| d.bg(st.c(|c| c.hover))))
        .cursor(CursorStyle::OpenHand)
        .tip(format!("{}: drag to move, click for options", tool.label()));
    for part in parts(tool) {
        item = item.child(match part {
            Some(name) => div()
                .size(size)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(icon(name))
                        .size(st.px(|m| m.icon_size))
                        .text_color(if selected { accent } else { muted }),
                ),
            None => div()
                .h(size)
                .min_w(px(46.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(text)
                .child("100 %"),
        });
    }
    let ghost_icon = tool.icon();
    let (fg, bg, line) = (accent, st.c(|c| c.raised), st.c(|c| c.line));
    let drop_ws = ws.clone();
    item.on_click(on_ws(ws, move |ws, _, cx| {
        ws.toolbar_sel = if ws.toolbar_sel == Some(tool) {
            None
        } else {
            Some(tool)
        };
        cx.notify();
    }))
    .on_drag(DraggedTool(tool), move |_, _, _, cx| {
        cx.new(|_| Ghost {
            icon: ghost_icon,
            fg,
            bg,
            line,
        })
    })
    // Where it lands: an accent bar on this button's leading edge.
    .drag_over::<DraggedTool>(move |s, d: &DraggedTool, _, _| {
        if d.0 == tool {
            s
        } else {
            s.border_l_2().border_color(accent)
        }
    })
    .on_drop(move |d: &DraggedTool, _, cx| {
        let moved = d.0;
        let _ = drop_ws.update(cx, |ws, cx| {
            ws.toolbar_sel = Some(moved);
            ws.change_style(&|s: &mut Style| s.toolbar.put(moved, bar, Some(tool)), cx);
        });
    })
    .into_any_element()
}

/// The accent + that ends a bar: the add menu.
fn add_button(ws: &WeakEntity<Workspace>, st: &Style, bar: Bar) -> AnyElement {
    let accent = st.c(|c| c.accent);
    let ws = ws.clone();
    crate::kbd::stop_solid(
        div().id(ElementId::Name(format!("tb-add-{bar:?}").into())),
        st,
    )
    .size(st.px(|m| m.icon_button))
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .ml(px(2.0))
    .rounded(st.px(|m| m.control_radius))
    .border_1()
    .border_dashed()
    .border_color(accent.opacity(0.7))
    .cursor_pointer()
    .hover(|d| d.bg(accent.opacity(0.12)))
    .tip(format!("Add a button to the {}", bar.name()))
    .child(svg().path(icon("plus")).size(px(13.0)).text_color(accent))
    .on_click(move |e: &gpui::ClickEvent, window, cx| {
        let at = e.position();
        let _ = ws.update(cx, |ws, cx| ws.toolbar_add_menu(bar, at, window, cx));
    })
    .into_any_element()
}

/// A bar's whole area takes a drop at its end.
fn drop_zone(
    ws: &WeakEntity<Workspace>,
    st: &Style,
    bar: Bar,
    el: gpui::Stateful<gpui::Div>,
) -> gpui::Stateful<gpui::Div> {
    let accent = st.c(|c| c.accent);
    let ws = ws.clone();
    el.drag_over::<DraggedTool>(move |s, _, _, _| s.border_color(accent.opacity(0.6)))
        .on_drop(move |d: &DraggedTool, _, cx| {
            let moved = d.0;
            let _ = ws.update(cx, |ws, cx| {
                ws.toolbar_sel = Some(moved);
                ws.change_style(&|s: &mut Style| s.toolbar.put(moved, bar, None), cx);
            });
        })
}

fn title_strip(ws: &WeakEntity<Workspace>, st: &Style, sel: Option<Tool>) -> AnyElement {
    let ring = st.c(|c| c.ring);
    let glyph = |name: &str| svg().path(icon(name)).size(px(12.0)).text_color(ring);
    let mut buttons = div().flex().items_center().gap(px(2.0));
    for &tool in &st.toolbar.title {
        buttons = buttons.child(tool_item(ws, st, Bar::Title, tool, sel == Some(tool)));
    }
    buttons = buttons.child(add_button(ws, st, Bar::Title));
    drop_zone(
        ws,
        st,
        Bar::Title,
        div()
            .id("tb-title-bar")
            .h(st.px(|m| m.title_bar))
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(12.0))
            .rounded(px(10.0))
            .bg(st.c(|c| c.bg))
            .border_1()
            .border_color(st.c(|c| c.line)),
    )
    .child(svg().path(icon("home")).size(px(15.0)).text_color(ring))
    .child(
        div()
            .w(px(1.0))
            .h(px(18.0))
            .mx(px(4.0))
            .bg(st.c(|c| c.line)),
    )
    .child(
        div()
            .h(px(28.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .rounded(px(8.0))
            .bg(st.c(|c| c.hover))
            .text_size(px(12.5))
            .text_color(st.c(|c| c.faint))
            .child("Your book"),
    )
    .child(div().flex_1())
    .child(buttons)
    .child(
        div()
            .flex()
            .gap(px(18.0))
            .pl(px(14.0))
            .child(glyph("win-min"))
            .child(glyph("win-max"))
            .child(glyph("close")),
    )
    .into_any_element()
}

fn dock_strip(ws: &WeakEntity<Workspace>, st: &Style, sel: Option<Tool>) -> AnyElement {
    let group = || {
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .p(px(2.0))
            .rounded(px(9.0))
            .bg(st.c(|c| c.hover))
            .shadow(st.well())
    };
    let mut dock = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .p(px(5.0))
        .rounded(px(14.0))
        .bg(st.c(|c| c.raised))
        .shadow(st.float())
        .border_1()
        .border_color(st.c(|c| c.line));
    // Groups as the real dock makes them: a tool of several buttons alone,
    // single buttons in a row together.
    let mut run: Option<gpui::Div> = None;
    for &tool in &st.toolbar.dock {
        let el = tool_item(ws, st, Bar::Dock, tool, sel == Some(tool));
        if matches!(tool, Tool::History | Tool::Zoom | Tool::Fit) {
            if let Some(r) = run.take() {
                dock = dock.child(r);
            }
            dock = dock.child(group().child(el));
        } else {
            run = Some(run.unwrap_or_else(group).child(el));
        }
    }
    if let Some(r) = run.take() {
        dock = dock.child(r);
    }
    dock = dock.child(add_button(ws, st, Bar::Dock)).child(
        div()
            .size(st.px(|m| m.icon_button))
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path(icon("chevron-down"))
                    .size(px(14.0))
                    .text_color(st.c(|c| c.ring)),
            ),
    );
    let justify = st.toolbar.dock_align;
    drop_zone(
        ws,
        st,
        Bar::Dock,
        div()
            .id("tb-dock-bar")
            .h(px(68.0))
            .flex()
            .items_center()
            .map(|d| match justify {
                DockAlign::Start => d.justify_start(),
                DockAlign::Centre => d.justify_center(),
                DockAlign::End => d.justify_end(),
            })
            .px(px(14.0))
            .rounded(px(10.0))
            .bg(st.c(|c| c.desk))
            .border_1()
            .border_color(st.c(|c| c.line)),
    )
    .child(dock)
    .into_any_element()
}

/// Where the dock's buttons sit: three icon buttons in a well.
fn align_group(ws: &WeakEntity<Workspace>, st: &Style) -> AnyElement {
    let upright = st.dock.upright();
    let now = st.toolbar.dock_align;
    let mut g = div()
        .flex()
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(px(8.0))
        .bg(st.c(|c| c.panel))
        .shadow(st.well());
    for (a, name, tip) in [
        (
            DockAlign::Start,
            "align-left",
            if upright {
                "Dock at the top"
            } else {
                "Dock on the left"
            },
        ),
        (DockAlign::Centre, "align-centre", "Dock in the middle"),
        (
            DockAlign::End,
            "align-right",
            if upright {
                "Dock at the bottom"
            } else {
                "Dock on the right"
            },
        ),
    ] {
        let on = now == a;
        g = g.child(
            crate::kbd::stop_solid(
                div().id(ElementId::Name(format!("tb-align-{a:?}").into())),
                st,
            )
            .w(px(26.0))
            .h(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .cursor_pointer()
            .when(on, |d| d.bg(st.c(|c| c.raised)).shadow(st.elev()))
            .tip(tip)
            .child(svg().path(icon(name)).size(px(14.0)).text_color(if on {
                st.c(|c| c.accent)
            } else {
                st.c(|c| c.muted)
            }))
            .on_click({
                let set = super::change(ws, move |s: &mut Style| s.toolbar.dock_align = a);
                move |_, window, cx| set(window, cx)
            }),
        );
    }
    g.into_any_element()
}

/// The selected button's line: its name and place, then what to do with it.
fn action_line(ws: &WeakEntity<Workspace>, st: &Style, tool: Tool) -> AnyElement {
    let Some((bar, i)) = st.toolbar.place(tool) else {
        return div().into_any_element();
    };
    let n = st.toolbar.list(bar).len();
    let text = st.c(|c| c.text);
    let button = |id: &'static str, label: SharedString, enabled: bool, colour: Hsla| {
        crate::kbd::stop_solid(div().id(id), st)
            .h(px(26.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .rounded(px(7.0))
            .text_size(px(12.5))
            .when(enabled, |d| {
                d.bg(st.c(|c| c.bg))
                    .border_1()
                    .border_color(st.c(|c| c.line))
                    .text_color(colour)
                    .cursor_pointer()
                    .hover(|d| d.bg(st.c(|c| c.hover)))
            })
            .when(!enabled, |d| d.text_color(st.c(|c| c.ring)))
            .child(label)
    };
    let to_other = match bar {
        Bar::Title => "Move to the dock",
        Bar::Dock => "Move to the title bar",
    };
    let other = bar.other();
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .p(px(4.0))
        .pl(px(12.0))
        .rounded(px(9.0))
        .bg(st.c(|c| c.raised))
        .shadow(st.elev())
        .text_size(st.px(|m| m.ui_text))
        .child(div().flex_1().text_color(text).child(format!(
            "{} · {} of {} in the {}",
            tool.label(),
            i + 1,
            n,
            bar.name()
        )))
        .child(
            button("tb-earlier", "‹ Earlier".into(), i > 0, text).when(i > 0, |d| {
                d.on_click(edit(ws, Some(tool), move |t| t.shift(tool, -1)))
            }),
        )
        .child(
            button("tb-later", "Later ›".into(), i + 1 < n, text).when(i + 1 < n, |d| {
                d.on_click(edit(ws, Some(tool), move |t| t.shift(tool, 1)))
            }),
        )
        .child(
            button("tb-move", to_other.into(), true, text).on_click(edit(
                ws,
                Some(tool),
                move |t| t.put(tool, other, None),
            )),
        )
        .child(if tool == Tool::More {
            button("tb-remove", "More always stays".into(), false, text)
        } else {
            button("tb-remove", "Take away".into(), true, st.c(|c| c.bad)).on_click(edit(
                ws,
                None,
                move |t| t.remove(tool),
            ))
        })
        .child(
            crate::kbd::stop_solid(div().id("tb-done"), st)
                .size(px(26.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.0))
                .cursor_pointer()
                .hover(|d| d.bg(st.c(|c| c.hover)))
                .tip("Done")
                .child(
                    svg()
                        .path(icon("close"))
                        .size(px(13.0))
                        .text_color(st.c(|c| c.faint)),
                )
                .on_click(on_ws(ws, |ws, _, cx| {
                    ws.toolbar_sel = None;
                    cx.notify();
                })),
        )
        .into_any_element()
}
