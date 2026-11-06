// SPDX-License-Identifier: AGPL-3.0-or-later

//! The Settings modal (DESIGN.md § Settings modal): a left rail of sections and a
//! scrolling body of choice cards. Every choice shows a small live preview drawn
//! with the real tokens, and applies the moment it is picked.

use crate::icons::icon;
use crate::settings::{PageCorners, PageNav};
use crate::shell::Workspace;
use crate::style::{Style, hsla};
use crate::tooltip::Tip;
use gpui::{
    AnyElement, App, BoxShadow, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, WeakEntity, Window, div,
    point, prelude::FluentBuilder, px, relative, svg,
};
use kitab_theme::{Density, Theme};

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Section {
    #[default]
    Appearance,
    Reading,
}

impl Section {
    const ALL: [Section; 2] = [Section::Appearance, Section::Reading];

    fn label(self) -> &'static str {
        match self {
            Section::Appearance => "Appearance",
            Section::Reading => "Reading",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::Appearance => "sun",
            Section::Reading => "book",
        }
    }
}

/// The panel (without the scrim; the shell positions and animates both).
pub fn panel(
    ws: WeakEntity<Workspace>,
    section: Section,
    st: &Style,
    themes: &[Theme],
) -> AnyElement {
    let rail = {
        let mut rail = div()
            .w(px(208.0))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(10.0))
            .bg(st.c(|c| c.panel))
            .border_r_1()
            .border_color(st.c(|c| c.line))
            .child(
                div()
                    .px(px(10.0))
                    .pt(px(8.0))
                    .pb(px(12.0))
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(st.c(|c| c.text))
                    .child("Settings"),
            );
        for s in Section::ALL {
            let on = s == section;
            let ws = ws.clone();
            rail = rail.child(
                div()
                    .id(("settings-section", s as usize))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .h(st.px(|m| m.list_row))
                    .px(px(10.0))
                    .rounded(st.px(|m| m.control_radius))
                    .text_size(st.px(|m| m.ui_text))
                    .text_color(if on {
                        st.c(|c| c.text)
                    } else {
                        st.c(|c| c.muted)
                    })
                    .when(on, |d| d.bg(st.c(|c| c.raised)).shadow(st.elev()))
                    .when(!on, |d| {
                        d.hover(|d| d.bg(st.c(|c| c.hover)).text_color(st.c(|c| c.text)))
                    })
                    .on_click(move |_, _, cx| {
                        let _ = ws.update(cx, |ws, cx| ws.set_settings_section(s, cx));
                    })
                    .child(svg().path(icon(s.icon())).size(px(16.0)).text_color(if on {
                        st.c(|c| c.accent)
                    } else {
                        st.c(|c| c.muted)
                    }))
                    .child(s.label()),
            );
        }
        rail
    };

    let body = match section {
        Section::Appearance => appearance(&ws, st, themes),
        Section::Reading => reading(&ws, st),
    };
    let close_ws = ws.clone();
    let main = div()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .h(px(56.0))
                .px(px(24.0))
                .border_b_1()
                .border_color(st.c(|c| c.line))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(st.c(|c| c.text))
                        .child(section.label()),
                )
                .child(
                    div()
                        .id("settings-close")
                        .size(st.px(|m| m.icon_button))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(st.px(|m| m.control_radius))
                        .hover(|d| d.bg(st.c(|c| c.hover)))
                        .tip("Close  Esc")
                        .on_click(move |_, window, cx| {
                            let _ = close_ws.update(cx, |ws, cx| ws.close_settings(window, cx));
                        })
                        .child(
                            svg()
                                .path(icon("close"))
                                .size(px(14.0))
                                .text_color(st.c(|c| c.muted)),
                        ),
                ),
        )
        .child(
            div()
                .id("settings-body")
                .flex_1()
                .overflow_y_scroll()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(28.0))
                        .px(px(24.0))
                        .py(px(22.0))
                        .child(body),
                ),
        );

    div()
        .id("settings")
        .w(px(900.0))
        .max_w(relative(0.94))
        .h(px(600.0))
        .max_h(relative(0.9))
        .flex()
        .rounded(px(16.0))
        .overflow_hidden()
        .bg(st.c(|c| c.raised))
        .shadow(st.modal())
        .border_1()
        .border_color(st.c(|c| c.line))
        // Clicks inside don't reach the scrim.
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(rail)
        .child(main)
        .into_any_element()
}

// ── sections ─────────────────────────────────────────────────────────────

fn appearance(ws: &WeakEntity<Workspace>, st: &Style, themes: &[Theme]) -> AnyElement {
    let mut theme_cards = div().flex().flex_wrap().gap(px(12.0));
    for t in themes {
        let selected = t.id == st.theme.id;
        let theme = t.clone();
        theme_cards = theme_cards.child(choice_card(
            ElementId::Name(format!("theme-{}", t.id).into()),
            selected,
            t.name.clone().into(),
            None,
            theme_preview(t),
            160.0,
            st,
            change(ws, move |s| s.theme = theme.clone()),
        ));
    }
    let mut density_cards = div().flex().flex_wrap().gap(px(12.0));
    for (ix, (d, label, hint)) in [
        (Density::Dense, "Dense", "Most on screen"),
        (Density::Compact, "Compact", "The default"),
        (Density::Standard, "Standard", "Roomier controls"),
    ]
    .into_iter()
    .enumerate()
    {
        density_cards = density_cards.child(choice_card(
            ("density", ix).into(),
            st.density == d,
            label.into(),
            Some(hint),
            density_preview(d, st),
            160.0,
            st,
            change(ws, move |s| s.density = d),
        ));
    }
    let on = st.animations;
    div()
        .flex()
        .flex_col()
        .gap(px(28.0))
        .child(group("Theme", None, theme_cards, st))
        .child(group("Density", None, density_cards, st))
        .child(group(
            "Motion",
            None,
            well_row(
                "Animations",
                "Menus, dialogs, panels and lists fade and slide in and out.",
                switch(
                    "animations",
                    on,
                    st,
                    change(ws, move |s| s.animations = !on),
                ),
                st,
            ),
            st,
        ))
        .into_any_element()
}

fn reading(ws: &WeakEntity<Workspace>, st: &Style) -> AnyElement {
    let mut nav_cards = div().flex().flex_wrap().gap(px(12.0));
    for (ix, nav) in PageNav::ALL.into_iter().enumerate() {
        nav_cards = nav_cards.child(choice_card(
            ("page-nav", ix).into(),
            st.page_nav == nav,
            nav.label().into(),
            Some(nav.hint()),
            nav_preview(nav, st),
            196.0,
            st,
            change(ws, move |s| s.page_nav = nav),
        ));
    }
    let mut corner_cards = div().flex().flex_wrap().gap(px(12.0));
    for (ix, c) in PageCorners::ALL.into_iter().enumerate() {
        corner_cards = corner_cards.child(choice_card(
            ("corners", ix).into(),
            st.page_corners == c,
            c.label().into(),
            None,
            corners_preview(c, st),
            160.0,
            st,
            change(ws, move |s| s.page_corners = c),
        ));
    }
    div()
        .flex()
        .flex_col()
        .gap(px(28.0))
        .child(group(
            "Page navigator",
            Some("Click the page number in any style, or press Ctrl+G, to type a page."),
            nav_cards,
            st,
        ))
        .child(group("Page corners", None, corner_cards, st))
        .into_any_element()
}

// ── pieces ───────────────────────────────────────────────────────────────

type Click = Box<dyn Fn(&mut Window, &mut App)>;

/// A click handler that changes the look and saves settings.json.
fn change(ws: &WeakEntity<Workspace>, f: impl Fn(&mut Style) + 'static) -> Click {
    let ws = ws.clone();
    Box::new(move |_, cx| {
        let _ = ws.update(cx, |ws, cx| ws.change_style(&f, cx));
    })
}

fn group(title: &str, note: Option<&str>, content: impl IntoElement, st: &Style) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(st.c(|c| c.text))
                .child(title.to_string()),
        )
        .children(note.map(|n| {
            div()
                .mt(px(-4.0))
                .text_size(px(12.0))
                .text_color(st.c(|c| c.muted))
                .child(n.to_string())
        }))
        .child(content)
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn choice_card(
    id: ElementId,
    selected: bool,
    label: SharedString,
    hint: Option<&str>,
    preview: AnyElement,
    width: f32,
    st: &Style,
    on_click: Click,
) -> AnyElement {
    let ring = BoxShadow {
        color: st.c(|c| c.accent),
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(1.0),
    };
    let mut shadow = st.elev();
    shadow.push(ring);
    div()
        .id(id)
        .w(px(width))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(8.0))
        .rounded(px(12.0))
        .border_1()
        .cursor_pointer()
        .when(selected, |d| {
            d.border_color(st.c(|c| c.accent))
                .bg(st.c(|c| c.raised))
                .shadow(shadow)
        })
        .when(!selected, |d| {
            d.border_color(st.c(|c| c.line))
                .hover(|d| d.bg(st.c(|c| c.hover)))
        })
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(
            div()
                .h(px(96.0))
                .rounded(px(8.0))
                .overflow_hidden()
                .child(preview),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(2.0))
                .text_size(st.px(|m| m.ui_text))
                .text_color(st.c(|c| c.text))
                .child(div().flex_1().child(label))
                .when(selected, |d| {
                    d.child(
                        svg()
                            .path(icon("check"))
                            .size(px(14.0))
                            .text_color(st.c(|c| c.accent)),
                    )
                }),
        )
        .children(hint.map(|h| {
            div()
                .mt(px(-4.0))
                .px(px(2.0))
                .text_size(px(11.5))
                .text_color(st.c(|c| c.faint))
                .child(h.to_string())
        }))
        .into_any_element()
}

fn well_row(title: &str, body: &str, control: AnyElement, st: &Style) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(16.0))
        .px(px(14.0))
        .py(px(12.0))
        .rounded(px(12.0))
        .bg(st.c(|c| c.panel))
        .shadow(st.well())
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(st.px(|m| m.ui_text))
                        .text_color(st.c(|c| c.text))
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(st.c(|c| c.muted))
                        .child(body.to_string()),
                ),
        )
        .child(control)
        .into_any_element()
}

fn switch(id: &'static str, on: bool, st: &Style, on_click: Click) -> AnyElement {
    div()
        .id(id)
        .w(px(36.0))
        .h(px(20.0))
        .p(px(2.0))
        .flex()
        .when(on, |d| d.justify_end())
        .rounded_full()
        .cursor_pointer()
        .bg(if on {
            st.c(|c| c.accent)
        } else {
            st.c(|c| c.hover)
        })
        .shadow(st.well())
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(
            div()
                .size(px(16.0))
                .rounded_full()
                .bg(st.c(|c| c.text))
                .shadow(st.elev()),
        )
        .into_any_element()
}

// ── previews ─────────────────────────────────────────────────────────────

/// A page sheet with a few text lines, `w`×`h` px.
fn mini_page(w: f32, h: f32, radius: f32) -> gpui::Div {
    let line = |len: f32| {
        div()
            .h(px(2.0))
            .w(relative(len))
            .rounded_full()
            .bg(gpui::black().opacity(0.18))
    };
    div()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .p(px(w * 0.12))
        .rounded(px(radius))
        .bg(gpui::white())
        .child(
            div()
                .h(px(3.0))
                .w(relative(0.5))
                .rounded_full()
                .bg(gpui::black().opacity(0.4)),
        )
        .child(line(1.0))
        .child(line(0.9))
        .child(line(0.95))
        .child(line(0.7))
}

fn theme_preview(t: &Theme) -> AnyElement {
    let c = &t.colors;
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(hsla(c.bg))
        .child(
            div()
                .h(px(14.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(6.0))
                .border_b_1()
                .border_color(hsla(c.line))
                .child(
                    div()
                        .w(px(26.0))
                        .h(px(7.0))
                        .rounded(px(2.0))
                        .bg(hsla(c.raised)),
                )
                .child(div().size(px(4.0)).rounded_full().bg(hsla(c.accent))),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .child(
                    div()
                        .w(px(36.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .p(px(5.0))
                        .bg(hsla(c.panel))
                        .child(
                            div()
                                .h(px(3.0))
                                .w(relative(0.9))
                                .rounded_full()
                                .bg(hsla(c.accent)),
                        )
                        .child(
                            div()
                                .h(px(3.0))
                                .w(relative(0.7))
                                .rounded_full()
                                .bg(hsla(c.muted)),
                        )
                        .child(
                            div()
                                .h(px(3.0))
                                .w(relative(0.8))
                                .rounded_full()
                                .bg(hsla(c.faint)),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .justify_center()
                        .pt(px(8.0))
                        .bg(hsla(c.desk))
                        .child(mini_page(54.0, 70.0, 1.5)),
                ),
        )
        .into_any_element()
}

fn density_preview(d: Density, st: &Style) -> AnyElement {
    let (row, gap) = match d {
        Density::Dense => (8.0, 3.0),
        Density::Compact => (10.0, 5.0),
        Density::Standard => (12.0, 8.0),
    };
    let mut col = div()
        .size_full()
        .flex()
        .flex_col()
        .gap(px(gap))
        .p(px(12.0))
        .bg(st.c(|c| c.panel));
    for (i, w) in [0.8, 0.6, 0.7, 0.5, 0.65, 0.55].into_iter().enumerate() {
        col = col.child(
            div()
                .h(px(row))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(5.0))
                .px(px(4.0))
                .rounded(px(3.0))
                .when(i == 0, |d| d.bg(st.c(|c| c.raised)))
                .child(div().size(px(4.0)).rounded_full().bg(if i == 0 {
                    st.c(|c| c.accent)
                } else {
                    st.c(|c| c.ring)
                }))
                .child(
                    div()
                        .h(px(2.0))
                        .w(relative(w))
                        .rounded_full()
                        .bg(st.c(|c| c.muted)),
                ),
        );
    }
    col.into_any_element()
}

/// The desk with a page, and the navigator drawn small at its real place.
fn nav_preview(nav: PageNav, st: &Style) -> AnyElement {
    let pill = || {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .h(px(14.0))
            .px(px(5.0))
            .rounded_full()
            .bg(st.c(|c| c.raised))
            .shadow(st.float())
            .text_size(px(7.0))
            .text_color(st.c(|c| c.muted))
    };
    let num = |s: &str| div().text_color(st.c(|c| c.text)).child(s.to_string());
    let widget: Option<(gpui::Div, bool)> = match nav {
        PageNav::Scrubber => Some((
            pill()
                .child(num("12"))
                .child(
                    div()
                        .relative()
                        .w(px(64.0))
                        .h(px(3.0))
                        .rounded_full()
                        .bg(st.c(|c| c.hover))
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(20.0))
                                .rounded_full()
                                .bg(st.c(|c| c.accent)),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(-2.0))
                                .left(px(17.0))
                                .size(px(7.0))
                                .rounded_full()
                                .bg(st.c(|c| c.text)),
                        ),
                )
                .child("40"),
            false,
        )),
        PageNav::Stepper => Some((
            pill()
                .gap(px(5.0))
                .child(
                    svg()
                        .path(icon("chevron-left"))
                        .size(px(8.0))
                        .text_color(st.c(|c| c.muted)),
                )
                .child(num("12"))
                .child("of 40")
                .child(
                    svg()
                        .path(icon("chevron-right"))
                        .size(px(8.0))
                        .text_color(st.c(|c| c.muted)),
                ),
            false,
        )),
        PageNav::Counter => Some((pill().child(num("12")).child("/ 40"), true)),
        PageNav::Hidden => None,
    };
    let mut desk = div()
        .relative()
        .size_full()
        .flex()
        .justify_center()
        .pt(px(10.0))
        .bg(st.c(|c| c.desk))
        .child(mini_page(92.0, 120.0, 1.5));
    desk = match widget {
        Some((w, true)) => desk.child(div().absolute().bottom(px(8.0)).right(px(8.0)).child(w)),
        Some((w, false)) => desk.child(
            div()
                .absolute()
                .bottom(px(8.0))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(w),
        ),
        None => desk.child(
            div()
                .absolute()
                .bottom(px(8.0))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .px(px(5.0))
                        .py(px(1.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(st.c(|c| c.line))
                        .bg(st.c(|c| c.panel))
                        .text_size(px(7.0))
                        .text_color(st.c(|c| c.faint))
                        .child("Ctrl+G"),
                ),
        ),
    };
    desk.into_any_element()
}

fn corners_preview(c: PageCorners, st: &Style) -> AnyElement {
    // The preview pages are about half size; halve the radius with them.
    let r = c.radius() * 0.5;
    let sheet = |w: f32, h: f32| mini_page(w, h, r).shadow(st.sheet());
    div()
        .size_full()
        .flex()
        .items_start()
        .justify_center()
        .gap(px(10.0))
        .pt(px(12.0))
        .bg(st.c(|c| c.desk))
        .child(sheet(54.0, 72.0))
        .child(sheet(54.0, 72.0))
        .into_any_element()
}
