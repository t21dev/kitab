// SPDX-License-Identifier: AGPL-3.0-or-later

//! Menus: the ⋯ menu and every right-click menu (DESIGN.md § Menus). A `float`
//! panel of rows with an icon, a label and the shortcut, separators between
//! groups. Opening fades and slides 4 px from the side it is anchored to;
//! choosing a row or clicking outside closes it.

use crate::icons::icon;
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, Pixels, Point, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px, svg,
};
use std::rc::Rc;

pub type Handler = Rc<dyn Fn(&mut Window, &mut App)>;
/// Draws a custom row with the current style.
pub type RowBuilder = Rc<dyn Fn(&Style, &mut App) -> AnyElement>;

#[derive(Clone)]
pub enum MenuItem {
    Row {
        icon: Option<&'static str>,
        label: SharedString,
        shortcut: Option<&'static str>,
        /// Shown with a check (a toggle that is on, the current choice).
        checked: bool,
        disabled: bool,
        run: Handler,
    },
    Separator,
    /// A small heading over a group.
    Heading(SharedString),
    /// A custom row (icon rows for themes and density in the ⋯ menu).
    Custom(RowBuilder),
}

impl MenuItem {
    pub fn new(
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self::Row {
            icon: None,
            label: label.into(),
            shortcut: None,
            checked: false,
            disabled: false,
            run: Rc::new(run),
        }
    }

    pub fn icon(mut self, name: &'static str) -> Self {
        if let Self::Row { icon, .. } = &mut self {
            *icon = Some(name);
        }
        self
    }

    pub fn key(mut self, keys: &'static str) -> Self {
        if let Self::Row { shortcut, .. } = &mut self {
            *shortcut = Some(keys);
        }
        self
    }

    pub fn checked(mut self, on: bool) -> Self {
        if let Self::Row { checked, .. } = &mut self {
            *checked = on;
        }
        self
    }

    pub fn disabled(mut self, off: bool) -> Self {
        if let Self::Row { disabled, .. } = &mut self {
            *disabled = off;
        }
        self
    }
}

/// One icon button in a menu toolbar row.
pub struct Tool {
    pub icon: &'static str,
    pub tip: SharedString,
    /// Shown selected (the current zoom mode).
    pub on: bool,
    pub disabled: bool,
    pub run: Handler,
}

impl Tool {
    pub fn new(
        icon: &'static str,
        tip: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            icon,
            tip: tip.into(),
            on: false,
            disabled: false,
            run: Rc::new(run),
        }
    }

    pub fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }

    pub fn disabled(mut self, off: bool) -> Self {
        self.disabled = off;
        self
    }
}

/// A row of icon buttons in groups, each group in a `well` (the page menu's
/// navigation, zoom and rotation). The menu stays open, so zoom can be
/// pressed again; `label` (like "125 %") sits after group `label_after`.
pub fn tool_row(
    key: &'static str,
    groups: Vec<Vec<Tool>>,
    label_after: Option<usize>,
    label: Option<SharedString>,
    st: &Style,
) -> AnyElement {
    let mut row = div()
        .px(px(6.0))
        .py(px(4.0))
        .flex()
        .items_center()
        .gap(px(6.0));
    for (g, group) in groups.into_iter().enumerate() {
        let mut well = div()
            .p(px(2.0))
            .flex()
            .gap(px(2.0))
            .rounded(st.px(|m| m.control_radius))
            .bg(st.c(|c| c.hover))
            .shadow(st.well());
        for (i, t) in group.into_iter().enumerate() {
            let run = t.run.clone();
            let fg = if t.disabled {
                st.c(|c| c.ring)
            } else if t.on {
                st.c(|c| c.accent)
            } else {
                st.c(|c| c.muted)
            };
            well = well.child(
                div()
                    .id(SharedString::from(format!("{key}-{g}-{i}")))
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .when(t.on, |d| d.bg(st.c(|c| c.raised)).shadow(st.elev()))
                    .when(!t.disabled && !t.on, |d| {
                        d.hover(|d| d.bg(st.c(|c| c.raised)))
                    })
                    .when(!t.disabled, |d| {
                        d.on_click(move |_, window, cx| run(window, cx))
                    })
                    .tip(t.tip)
                    .child(svg().path(icon(t.icon)).size(px(16.0)).text_color(fg)),
            );
        }
        row = row.child(well);
        if Some(g) == label_after {
            if let Some(l) = label.clone() {
                row = row.child(
                    div()
                        .min_w(px(44.0))
                        .text_size(px(12.0))
                        .text_color(st.c(|c| c.muted))
                        .flex()
                        .justify_center()
                        .child(l),
                );
            }
        }
    }
    row.into_any_element()
}

/// An open menu: where, what, and a serial so each opening animates afresh.
#[derive(Clone)]
pub struct OpenMenu {
    pub at: Point<Pixels>,
    /// Anchor the menu's right edge at `at` (the ⋯ button) instead of its left.
    pub align_right: bool,
    pub items: Vec<MenuItem>,
    pub serial: u64,
    pub width: f32,
    /// The row picked with the arrow keys.
    pub hi: Option<usize>,
}

impl OpenMenu {
    /// The row after (or before) the picked one that can be chosen,
    /// wrapping round; the first (or last) when none is picked.
    pub fn step(&self, down: bool) -> Option<usize> {
        let rows: Vec<usize> = (0..self.items.len())
            .filter(|&i| {
                matches!(
                    self.items[i],
                    MenuItem::Row {
                        disabled: false,
                        ..
                    }
                )
            })
            .collect();
        let n = rows.len();
        if n == 0 {
            return None;
        }
        let at = self.hi.and_then(|h| rows.iter().position(|&r| r == h));
        let next = match (at, down) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(p), true) => (p + 1) % n,
            (Some(p), false) => (p + n - 1) % n,
        };
        Some(rows[next])
    }

    /// What the picked row runs.
    pub fn picked(&self) -> Option<Handler> {
        match self.items.get(self.hi?)? {
            MenuItem::Row {
                run,
                disabled: false,
                ..
            } => Some(run.clone()),
            _ => None,
        }
    }
}

/// The menu panel. `close` runs after a row's action. A menu taller than
/// `max_h` (a short window) scrolls instead of running off the screen.
pub fn panel(
    menu: &OpenMenu,
    st: &Style,
    close: Handler,
    max_h: Pixels,
    cx: &mut App,
) -> AnyElement {
    let mut rows: Vec<AnyElement> = Vec::new();
    for (i, item) in menu.items.iter().enumerate() {
        match item {
            MenuItem::Separator => rows.push(
                div()
                    .my(px(4.0))
                    .mx(px(6.0))
                    .h(px(1.0))
                    .bg(st.c(|c| c.line))
                    .into_any_element(),
            ),
            MenuItem::Heading(t) => rows.push(
                div()
                    .px(px(10.0))
                    .pt(px(6.0))
                    .pb(px(4.0))
                    .text_size(px(11.0))
                    .text_color(st.c(|c| c.faint))
                    .child(t.clone())
                    .into_any_element(),
            ),
            MenuItem::Custom(f) => rows.push(f(st, cx)),
            MenuItem::Row {
                icon: ic,
                label,
                shortcut,
                checked,
                disabled,
                run,
            } => {
                let run = run.clone();
                let close = close.clone();
                let disabled = *disabled;
                let fg = if disabled {
                    st.c(|c| c.faint)
                } else {
                    st.c(|c| c.text)
                };
                rows.push(
                    div()
                        .id(("menu-row", i))
                        .h(st.px(|m| m.control_height))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .rounded(st.px(|m| m.control_radius))
                        .text_size(st.px(|m| m.ui_text))
                        .text_color(fg)
                        .when(menu.hi == Some(i), |d| d.bg(st.c(|c| c.hover)))
                        .when(!disabled, |d| {
                            d.hover(|d| d.bg(st.hover_bg()))
                                .on_click(move |_, window, cx| {
                                    close(window, cx);
                                    run(window, cx);
                                })
                        })
                        .child(
                            div().size(px(16.0)).flex_none().children(
                                ic.map(|name| {
                                    svg()
                                        .path(icon(name))
                                        .size(px(16.0))
                                        .text_color(st.c(|c| c.muted))
                                })
                                .or_else(|| {
                                    checked.then(|| {
                                        svg()
                                            .path(icon("check"))
                                            .size(px(16.0))
                                            .text_color(st.c(|c| c.accent))
                                    })
                                }),
                            ),
                        )
                        // A long label (a document title, a collection name)
                        // ends in "…" rather than pushing the key off the menu.
                        .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                        .when(*checked && ic.is_some(), |d| {
                            d.child(
                                svg()
                                    .path(icon("check"))
                                    .size(px(14.0))
                                    .text_color(st.c(|c| c.accent)),
                            )
                        })
                        .children(shortcut.map(|k| {
                            div()
                                .flex_none()
                                .text_size(px(11.0))
                                .text_color(st.c(|c| c.faint))
                                .child(k)
                        }))
                        .into_any_element(),
                );
            }
        }
    }
    div()
        .id("menu")
        .w(px(menu.width))
        .max_h(max_h)
        .overflow_y_scroll()
        .p(px(4.0))
        .flex()
        .flex_col()
        .rounded(px(12.0))
        .bg(st.c(|c| c.raised))
        .shadow(st.float())
        .border_1()
        .border_color(st.c(|c| c.line))
        // Clicks inside don't reach the catcher behind the menu.
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .children(rows)
        .into_any_element()
}
