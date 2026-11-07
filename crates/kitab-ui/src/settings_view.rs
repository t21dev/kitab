// SPDX-License-Identifier: AGPL-3.0-or-later

//! The Settings modal (DESIGN.md § Settings modal): a left rail of sections and a
//! scrolling body of choice cards. Every choice shows a small live preview drawn
//! with the real tokens, and applies the moment it is picked.

use crate::icons::icon;
use crate::settings::{DockAt, Layout, PageCorners, PageNav, TabsAt};
use crate::shell::Workspace;
use crate::shortcuts;
use crate::style::{Style, hsla};
use crate::tooltip::Tip;
use gpui::{
    AnyElement, App, BoxShadow, ElementId, FocusHandle, FontWeight, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, SharedString, StatefulInteractiveElement, Styled,
    WeakEntity, Window, div, point, prelude::FluentBuilder, px, relative, svg,
};
use kitab_theme::{Accent, AccentPreset, Appearance, Density, Theme, hue_swatch};

mod system;
pub use system::{SystemInfo, UpdateState};

/// The custom accent's hex field.
pub struct HexState {
    /// The digits being typed (without '#'), while editing.
    pub edit: Option<String>,
    pub focus: FocusHandle,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Section {
    #[default]
    Appearance,
    Reading,
    FileTypes,
    Shortcuts,
    Signing,
    Advanced,
    About,
}

impl Section {
    const ALL: [Section; 7] = [
        Section::Appearance,
        Section::Reading,
        Section::FileTypes,
        Section::Shortcuts,
        Section::Signing,
        Section::Advanced,
        Section::About,
    ];

    fn label(self) -> &'static str {
        match self {
            Section::Appearance => "Appearance",
            Section::Reading => "Reading",
            Section::FileTypes => "File types",
            Section::Shortcuts => "Shortcuts",
            Section::Signing => "Signing",
            Section::Advanced => "Advanced",
            Section::About => "About",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::Appearance => "sun",
            Section::Reading => "book",
            Section::FileTypes => "file",
            Section::Shortcuts => "keyboard",
            Section::Signing => "shield",
            Section::Advanced => "settings",
            Section::About => "info",
        }
    }
}

/// The panel (without the scrim; the shell positions and animates both).
pub fn panel(
    ws: WeakEntity<Workspace>,
    section: Section,
    st: &Style,
    themes: &[Theme],
    hex: &HexState,
    sys: &SystemInfo,
    window: &Window,
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
            // The modal clips to its box, not its rounded corners: round the
            // rail's own left corners to match (16 px less the 1 px border).
            .rounded_tl(px(15.0))
            .rounded_bl(px(15.0))
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
        Section::Appearance => appearance(&ws, st, themes, hex, window),
        Section::Reading => reading(&ws, st),
        Section::FileTypes => system::file_types(&ws, sys, st),
        Section::Shortcuts => shortcuts_section(&ws, st),
        Section::Signing => system::signing(&ws, sys, st),
        Section::Advanced => system::advanced(&ws, sys, st),
        Section::About => system::about(&ws, sys, st),
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

    // Sized from the window: the panel sits in auto-sized wrappers (motion),
    // so a relative max width would resolve against its own content.
    let vp = window.viewport_size();
    div()
        .id("settings")
        .w(px(900.0).min(vp.width * 0.94))
        .h(px(600.0).min(vp.height * 0.9))
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

fn appearance(
    ws: &WeakEntity<Workspace>,
    st: &Style,
    themes: &[Theme],
    hex: &HexState,
    window: &Window,
) -> AnyElement {
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
            change(ws, move |s| s.set_theme(theme.clone())),
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
    let mut tab_cards = div().flex().flex_wrap().gap(px(12.0));
    for (ix, t) in TabsAt::ALL.into_iter().enumerate() {
        tab_cards = tab_cards.child(choice_card(
            ("tabs-at", ix).into(),
            st.tabs == t,
            t.label().into(),
            Some(t.hint()),
            layout_preview(t, st.dock, st),
            196.0,
            st,
            change(ws, move |s| s.tabs = t),
        ));
    }
    let mut dock_cards = div().flex().flex_wrap().gap(px(12.0));
    for (ix, d) in DockAt::ALL.into_iter().enumerate() {
        dock_cards = dock_cards.child(choice_card(
            ("dock-at", ix).into(),
            st.dock == d,
            d.label().into(),
            Some(d.hint()),
            layout_preview(st.tabs, d, st),
            150.0,
            st,
            change(ws, move |s| s.dock = d),
        ));
    }
    let fade = st.dock_fade;
    let current = st.settings().layout();
    let mut layout_cards = div().flex().flex_wrap().gap(px(12.0));
    for (ix, l) in [Layout::Full, Layout::Minimal, Layout::Custom]
        .into_iter()
        .enumerate()
    {
        let preview = match l {
            Layout::Full => layout_preview(TabsAt::Sidebar, DockAt::Bottom, st),
            Layout::Minimal => layout_preview(TabsAt::Top, DockAt::Bottom, st),
            Layout::Custom => layout_preview(st.tabs, st.dock, st),
        };
        layout_cards = layout_cards.child(choice_card(
            ("layout", ix).into(),
            current == l,
            l.label().into(),
            Some(l.hint()),
            preview,
            180.0,
            st,
            change(ws, move |s| s.apply_layout(l)),
        ));
    }
    let (flat, compact) = (st.ultra, st.dock_compact);
    div()
        .flex()
        .flex_col()
        .gap(px(28.0))
        .child(group(
            "Layout",
            Some("A preset sets motion, depth, the dock and the page navigator. Change any of them and the layout becomes Custom."),
            layout_cards,
            st,
        ))
        .child(group("Theme", None, theme_cards, st))
        .child(group(
            "Accent",
            Some("Kitab adjusts the lightness so the accent stays visible on every theme."),
            accent_group(ws, st, hex, window),
            st,
        ))
        .child(group("Density", None, density_cards, st))
        .child(group("Tabs", None, tab_cards, st))
        .child(group(
            "Quick dock",
            Some("Zoom, fit, find, rotate and more, floating over the page. At the bottom it also holds the page navigator."),
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(dock_cards)
                .child(well_row(
                    "Compact dock",
                    "Only zoom, find and the ⋯ menu.",
                    switch(
                        "dock-compact",
                        compact,
                        st,
                        change(ws, move |s| s.dock_compact = !compact),
                    ),
                    st,
                ))
                .child(well_row(
                    "Fade the dock while reading",
                    "It comes back when you move the pointer or press a key.",
                    switch(
                        "dock-fade",
                        fade,
                        st,
                        change(ws, move |s| s.dock_fade = !fade),
                    ),
                    st,
                )),
            st,
        ))
        .child(group(
            "Toolbar",
            Some("Put each button in the title bar, beside the window buttons, or in the dock over the page, or turn it off. Title bar buttons act on the open tab. The compact dock trims the dock further."),
            toolbar_rows(ws, st),
            st,
        ))
        .child(group(
            "Motion",
            None,
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(well_row(
                    "Animations",
                    "Menus, dialogs, panels and lists fade and slide in and out.",
                    switch(
                        "animations",
                        on,
                        st,
                        change(ws, move |s| s.animations = !on),
                    ),
                    st,
                ))
                .child(well_row(
                    "Flat surfaces",
                    "Shadows become hairlines: less to draw, a quieter look.",
                    switch("flat", flat, st, change(ws, move |s| s.ultra = !flat)),
                    st,
                )),
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
    let mut code_cards = div().flex().flex_wrap().gap(px(12.0));
    for (ix, t) in kitab_theme::CodeTheme::ALL.into_iter().enumerate() {
        code_cards = code_cards.child(choice_card(
            ("code-theme", ix).into(),
            st.code_theme == t,
            t.label().into(),
            Some(t.hint()),
            code_preview(t, st),
            150.0,
            st,
            change(ws, move |s| s.code_theme = t),
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
        .child(group(
            "Code colours",
            Some("Syntax colours in Markdown code blocks."),
            code_cards,
            st,
        ))
        .child(group(
            "Books",
            Some("Typeface, line spacing and margins for EPUB, MOBI, FB2 and text files. Book's own keeps what the book sets. Zoom in a book changes its text size."),
            book_rows(ws, st),
            st,
        ))
        .child(group(
            "Status bar",
            Some("A slim bar along the bottom of the page: facts about the file on the left (point at them for the file's location), where you are on the right. Ctrl+I shows or hides it."),
            status_rows(ws, st),
            st,
        ))
        .child(group(
            "Drawing and markup",
            Some("Pen, highlighter, lines, arrows and shapes on PDFs, comics and XPS (Ctrl+Shift+D). In a PDF, Ctrl+S saves them into the file."),
            well_row(
                "Show the markup tools",
                "",
                switch("markup-tools", st.markup, st, change(ws, |s: &mut Style| s.markup = !s.markup)),
                st,
            ),
            st,
        ))
        .child(group(
            "Minimal mode",
            Some("F11 hides these for distraction-free reading. Point at the top edge for the window buttons and a way back."),
            minimal_rows(ws, st),
            st,
        ))
        .into_any_element()
}

/// Where a toolbar button sits.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    Title,
    Dock,
    Off,
}

/// A button's two switches: shown in the dock, moved to the title bar.
type Slot = (
    fn(&mut crate::settings::DockItems) -> &mut bool,
    fn(&mut crate::settings::TitleItems) -> &mut bool,
);

/// One row per toolbar button: title bar, dock or off. More can't be off;
/// the dock's hide chevron always stays.
fn toolbar_rows(ws: &WeakEntity<Workspace>, st: &Style) -> AnyElement {
    let rows: [(&'static str, Slot); 11] = [
        ("Sidebar", (|d| &mut d.sidebar, |t| &mut t.sidebar)),
        ("Back and forward", (|d| &mut d.history, |t| &mut t.history)),
        ("Zoom", (|d| &mut d.zoom, |t| &mut t.zoom)),
        ("Fit width and fit page", (|d| &mut d.fit, |t| &mut t.fit)),
        ("Find", (|d| &mut d.find, |t| &mut t.find)),
        ("Bookmark", (|d| &mut d.bookmark, |t| &mut t.bookmark)),
        ("Draw and mark up", (|d| &mut d.markup, |t| &mut t.markup)),
        ("Rotate", (|d| &mut d.rotate, |t| &mut t.rotate)),
        ("Minimal mode", (|d| &mut d.minimal, |t| &mut t.minimal)),
        ("Print", (|d| &mut d.print, |t| &mut t.print)),
        ("Password", (|d| &mut d.password, |t| &mut t.password)),
    ];
    let mut col = div().flex().flex_col().gap(px(8.0));
    for (label, (dock, title)) in rows {
        let (mut d, mut t) = (st.dock_items, st.title_items);
        let now = if *title(&mut t) {
            Place::Title
        } else if *dock(&mut d) {
            Place::Dock
        } else {
            Place::Off
        };
        let choices = [
            ("Title bar", Place::Title),
            ("Dock", Place::Dock),
            ("Off", Place::Off),
        ]
        .into_iter()
        .map(|(name, place)| {
            let set = move |s: &mut Style| {
                *title(&mut s.title_items) = place == Place::Title;
                // A button moved to the title bar keeps its dock switch,
                // so moving it back puts it where it was.
                if place != Place::Title {
                    *dock(&mut s.dock_items) = place == Place::Dock;
                } else {
                    *dock(&mut s.dock_items) = true;
                }
            };
            (SharedString::from(name), now == place, change(ws, set))
        })
        .collect();
        col = col.child(well_row(label, "", segmented(choices, st), st));
    }
    let more = [("Title bar", true), ("Dock", false)]
        .into_iter()
        .map(|(name, up)| {
            (
                SharedString::from(name),
                st.title_items.more == up,
                change(ws, move |s: &mut Style| s.title_items.more = up),
            )
        })
        .collect();
    col.child(well_row(
        "More (⋯)",
        "Everything else lives here, so it is always in one of the two.",
        segmented(more, st),
        st,
    ))
    .into_any_element()
}

fn minimal_rows(ws: &WeakEntity<Workspace>, st: &Style) -> AnyElement {
    let h = st.minimal_hides;
    let row = |id: &'static str, label: &'static str, on: bool, f: fn(&mut Style)| {
        well_row(label, "", switch(id, on, st, change(ws, f)), st)
    };
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(row("min-tabs", "Hide the tabs", h.tabs, |s| {
            s.minimal_hides.tabs = !s.minimal_hides.tabs
        }))
        .child(row("min-sidebar", "Hide the sidebar", h.sidebar, |s| {
            s.minimal_hides.sidebar = !s.minimal_hides.sidebar
        }))
        .child(row("min-dock", "Hide the quick dock", h.dock, |s| {
            s.minimal_hides.dock = !s.minimal_hides.dock
        }))
        .child(row(
            "min-nav",
            "Hide the page navigator",
            h.page_navigator,
            |s| s.minimal_hides.page_navigator = !s.minimal_hides.page_navigator,
        ))
        .into_any_element()
}

/// A few lines of Rust in a code palette.
fn code_preview(t: kitab_theme::CodeTheme, st: &Style) -> AnyElement {
    let mut preview = st.clone();
    preview.code_theme = t;
    let k = |kind| crate::reader::code_color(&preview, kind);
    use kitab_md::TokenKind as K;
    let line = |parts: Vec<(&'static str, K)>| {
        let mut row = div().flex().whitespace_nowrap();
        for (text, kind) in parts {
            row = row.child(div().text_color(k(kind)).child(text));
        }
        row
    };
    let bg = preview
        .code_palette()
        .map_or(st.c(|c| c.panel), |p| crate::style::hsla(p.background));
    div()
        .size_full()
        .px(px(10.0))
        .py(px(8.0))
        .bg(bg)
        .flex()
        .flex_col()
        .font_family("Consolas")
        .text_size(px(10.5))
        .line_height(px(15.0))
        .child(line(vec![("// cache", K::Comment)]))
        .child(line(vec![
            ("fn ", K::Keyword),
            ("evict", K::Function),
            ("(n: ", K::Plain),
            ("u32", K::Type),
            (") {", K::Punctuation),
        ]))
        .child(line(vec![
            ("  let s = ", K::Plain),
            ("\"kitab\"", K::String),
            (";", K::Punctuation),
        ]))
        .child(line(vec![("  n + ", K::Plain), ("42", K::Number)]))
        .child(line(vec![("}", K::Punctuation)]))
        .into_any_element()
}

// ── pieces ───────────────────────────────────────────────────────────────

pub(crate) type Click = Box<dyn Fn(&mut Window, &mut App)>;

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
    icon_row(None, title, body, control, st)
}

/// A well row with a muted 18 px icon before its title.
fn icon_row(
    name: Option<&str>,
    title: &str,
    body: &str,
    control: AnyElement,
    st: &Style,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(16.0))
        .px(px(14.0))
        .py(px(12.0))
        .rounded(px(12.0))
        .bg(st.c(|c| c.panel))
        .shadow(st.well())
        .children(name.map(|n| {
            svg()
                .path(icon(n))
                .flex_none()
                .size(px(18.0))
                .mr(px(-4.0))
                .text_color(st.c(|c| c.muted))
        }))
        .child(
            div()
                .flex_1()
                // Long notes wrap instead of pushing the control out.
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(st.px(|m| m.ui_text))
                        .text_color(st.c(|c| c.text))
                        .child(title.to_string()),
                )
                .when(!body.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(12.0))
                            .text_color(st.c(|c| c.muted))
                            .child(body.to_string()),
                    )
                }),
        )
        .child(div().flex_none().child(control))
        .into_any_element()
}

/// A row of choices, one of them on (Settings › Reading › Books).
pub(crate) fn segmented(choices: Vec<(SharedString, bool, Click)>, st: &Style) -> AnyElement {
    let mut row = div()
        .flex()
        .flex_none()
        .p(px(2.0))
        .gap(px(2.0))
        .rounded(px(8.0))
        .bg(st.c(|c| c.hover))
        .shadow(st.well());
    for (i, (label, on, click)) in choices.into_iter().enumerate() {
        row = row.child(
            div()
                .id(ElementId::NamedInteger(label.clone(), i as u64))
                .h(px(26.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded(px(6.0))
                .text_size(px(12.5))
                .cursor_pointer()
                .when(on, |d| {
                    d.bg(st.c(|c| c.raised))
                        .shadow(st.elev())
                        .text_color(st.c(|c| c.text))
                })
                .when(!on, |d| {
                    d.text_color(st.c(|c| c.muted))
                        .hover(|d| d.text_color(st.c(|c| c.text)))
                })
                .on_click(move |_, window, cx| click(window, cx))
                .child(label),
        );
    }
    row.into_any_element()
}

/// The status bar's switch and its items.
fn status_rows(ws: &WeakEntity<Workspace>, st: &Style) -> AnyElement {
    let i = st.status_items;
    let row =
        |id: &'static str, label: &'static str, note: &'static str, on: bool, f: fn(&mut Style)| {
            well_row(label, note, switch(id, on, st, change(ws, f)), st)
        };
    let mut col = div().flex().flex_col().gap(px(8.0)).child(row(
        "status-bar",
        "Show the status bar",
        "",
        st.file_info,
        |s| s.file_info = !s.file_info,
    ));
    if st.file_info {
        col = col
            .child(row(
                "status-format",
                "Format",
                "PDF, EPUB, Markdown…",
                i.format,
                |s| s.status_items.format = !s.status_items.format,
            ))
            .child(row("status-size", "File size", "", i.size, |s| {
                s.status_items.size = !s.status_items.size
            }))
            .child(row(
                "status-pages",
                "Pages",
                "Lines in Markdown.",
                i.pages,
                |s| s.status_items.pages = !s.status_items.pages,
            ))
            .child(row(
                "status-words",
                "Word count",
                "Markdown.",
                i.words,
                |s| s.status_items.words = !s.status_items.words,
            ))
            .child(row(
                "status-time",
                "Reading time",
                "Markdown, at 230 words a minute.",
                i.reading_time,
                |s| s.status_items.reading_time = !s.status_items.reading_time,
            ))
            .child(row(
                "status-paper",
                "Paper size",
                "The current page's, or a book's text size.",
                i.paper,
                |s| s.status_items.paper = !s.status_items.paper,
            ))
            .child(row(
                "status-position",
                "Position",
                "On the right: page 3 of 40, or how far through.",
                i.position,
                |s| s.status_items.position = !s.status_items.position,
            ))
            .child(row("status-zoom", "Zoom", "On the right.", i.zoom, |s| {
                s.status_items.zoom = !s.status_items.zoom
            }));
    }
    col.into_any_element()
}

/// Typeface, line spacing and margins for flowing books.
fn book_rows(ws: &WeakEntity<Workspace>, st: &Style) -> AnyElement {
    use kitab_core::BookFace;
    let b = st.book_style;
    let faces = [
        ("Book's own", BookFace::Book),
        ("Serif", BookFace::Serif),
        ("Sans", BookFace::Sans),
        ("Mono", BookFace::Mono),
    ]
    .into_iter()
    .map(|(label, f)| {
        (
            SharedString::from(label),
            b.face == f,
            change(ws, move |s: &mut Style| s.book_style.face = f),
        )
    })
    .collect();
    let lines = [
        ("Book's own", None),
        ("1.2", Some(1.2)),
        ("1.5", Some(1.5)),
        ("1.8", Some(1.8)),
        ("2.2", Some(2.2)),
    ]
    .into_iter()
    .map(|(label, v): (&str, Option<f32>)| {
        (
            SharedString::from(label),
            b.line_spacing == v,
            change(ws, move |s: &mut Style| s.book_style.line_spacing = v),
        )
    })
    .collect();
    let margins = [
        ("Book's own", None),
        ("Narrow", Some(1.0)),
        ("Medium", Some(2.5)),
        ("Wide", Some(4.0)),
    ]
    .into_iter()
    .map(|(label, v): (&str, Option<f32>)| {
        (
            SharedString::from(label),
            b.margin == v,
            change(ws, move |s: &mut Style| s.book_style.margin = v),
        )
    })
    .collect();
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(well_row("Typeface", "", segmented(faces, st), st))
        .child(well_row("Line spacing", "", segmented(lines, st), st))
        .child(well_row("Margins", "", segmented(margins, st), st))
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

/// A tiny window: tabs on top or in a sidebar, and the dock where it sits.
fn layout_preview(tabs: TabsAt, dock: DockAt, st: &Style) -> AnyElement {
    let bar = |w: f32, h: f32| {
        div()
            .w(px(w))
            .h(px(h))
            .rounded(px(2.0))
            .bg(st.c(|c| c.ring))
    };
    let dock_el = {
        let upright = dock.upright();
        let mut d = div()
            .flex()
            .when(upright, |d| d.flex_col())
            .gap(px(2.0))
            .p(px(3.0))
            .rounded(px(5.0))
            .bg(st.c(|c| c.raised))
            .shadow(st.float())
            .border_1()
            .border_color(st.c(|c| c.line));
        for i in 0..4 {
            d = d.child(div().size(px(5.0)).rounded(px(1.5)).bg(if i == 0 {
                st.c(|c| c.accent)
            } else {
                st.c(|c| c.muted)
            }));
        }
        d
    };
    let placed = match dock {
        DockAt::Top => div()
            .absolute()
            .top(px(5.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center(),
        DockAt::Bottom => div()
            .absolute()
            .bottom(px(5.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center(),
        DockAt::Left => div()
            .absolute()
            .left(px(5.0))
            .top_0()
            .bottom_0()
            .flex()
            .items_center(),
        DockAt::Right => div()
            .absolute()
            .right(px(5.0))
            .top_0()
            .bottom_0()
            .flex()
            .items_center(),
    }
    .child(dock_el);
    let desk = div()
        .relative()
        .flex_1()
        .h_full()
        .bg(st.c(|c| c.desk))
        .flex()
        .justify_center()
        .pt(px(8.0))
        .child(mini_page(40.0, 56.0, 1.0).shadow(st.sheet()))
        .child(placed);
    let side = div()
        .w(px(34.0))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .p(px(4.0))
        .bg(st.c(|c| c.panel))
        .border_r_1()
        .border_color(st.c(|c| c.line));
    let body = match tabs {
        TabsAt::Top => div().flex().flex_1().child(desk),
        TabsAt::Sidebar => div()
            .flex()
            .flex_1()
            .child(
                side.child(bar(24.0, 4.0).bg(st.c(|c| c.accent)))
                    .child(bar(20.0, 4.0))
                    .child(bar(22.0, 4.0)),
            )
            .child(desk),
    };
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(st.c(|c| c.bg))
        .when(tabs == TabsAt::Top, |d| {
            d.child(
                div()
                    .h(px(10.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .px(px(4.0))
                    .border_b_1()
                    .border_color(st.c(|c| c.line))
                    .child(bar(18.0, 4.0).bg(st.c(|c| c.accent)))
                    .child(bar(14.0, 4.0))
                    .child(bar(16.0, 4.0)),
            )
        })
        .child(body)
        .into_any_element()
}

// ── accent ───────────────────────────────────────────────────────────────

fn accent_group(
    ws: &WeakEntity<Workspace>,
    st: &Style,
    hex: &HexState,
    window: &Window,
) -> AnyElement {
    let base = st.base_theme();
    let mut row = div().flex().flex_wrap().gap(px(14.0));
    for (ix, p) in AccentPreset::ALL.into_iter().enumerate() {
        let selected = st.accent == Accent::Preset(p);
        row = row.child(swatch(
            ("accent", ix).into(),
            Accent::Preset(p).resolve(base),
            p.name(),
            selected,
            st,
            change(ws, move |s| s.set_accent(Accent::Preset(p))),
        ));
    }
    let custom = match st.accent {
        Accent::Custom(c) => Some(c),
        Accent::Preset(_) => None,
    };
    // Custom starts from a teal on the hue strip, or keeps the colour already chosen.
    let start = custom.unwrap_or_else(|| hue_swatch(0.45));
    row = row.child(swatch(
        "accent-custom".into(),
        Accent::Custom(start).resolve(base),
        "Custom",
        custom.is_some(),
        st,
        change(ws, move |s| s.set_accent(Accent::Custom(start))),
    ));
    let mut col = div().flex().flex_col().gap(px(14.0)).child(row);
    if let Some(c) = custom {
        col = col.child(custom_picker(ws, st, c, hex, window));
    }
    col.into_any_element()
}

fn swatch(
    id: ElementId,
    color: kitab_theme::Rgba,
    label: &str,
    selected: bool,
    st: &Style,
    on_click: Click,
) -> AnyElement {
    div()
        .id(id)
        .w(px(52.0))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(
            div()
                .size(px(34.0))
                .p(px(3.0))
                .rounded_full()
                .border_2()
                .border_color(if selected {
                    st.c(|c| c.text)
                } else {
                    st.ca(|c| c.line, 0.0)
                })
                .hover(|d| d.border_color(st.c(|c| c.ring)))
                .child(
                    div()
                        .size_full()
                        .rounded_full()
                        .bg(hsla(color))
                        .shadow(st.elev()),
                ),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(if selected {
                    st.c(|c| c.text)
                } else {
                    st.c(|c| c.muted)
                })
                .child(label.to_string()),
        )
        .into_any_element()
}

/// The custom accent: a hex field (click and type; it applies at six digits),
/// a hue strip to pick from, and the measured contrast on this theme.
fn custom_picker(
    ws: &WeakEntity<Workspace>,
    st: &Style,
    base: kitab_theme::Rgba,
    hex: &HexState,
    window: &Window,
) -> AnyElement {
    let theme = st.base_theme();
    let (ratio, adjusted) = Accent::Custom(base).measure(theme);
    let resolved = st.c(|c| c.accent);
    let editing = hex.edit.is_some();
    let focused = hex.focus.is_focused(window);
    let shown = match &hex.edit {
        Some(t) => format!("#{t}"),
        None => base.to_string(),
    };
    let note = if adjusted {
        let way = if theme.appearance == Appearance::Light {
            "darkened"
        } else {
            "lightened"
        };
        format!(
            "{ratio:.1}:1 on {}. Kitab {way} it to 3:1 so marks stay visible.",
            theme.name
        )
    } else {
        format!("{ratio:.1}:1 on {}. Passes as it is.", theme.name)
    };
    let (edit_ws, key_ws) = (ws.clone(), ws.clone());
    let field = div()
        .id("hex-field")
        .key_context("HexField")
        .track_focus(&hex.focus)
        .on_key_down(move |e: &KeyDownEvent, window, cx| {
            let _ = key_ws.update(cx, |ws, cx| ws.hex_key(e, window, cx));
        })
        .on_click(move |_, window, cx| {
            let _ = edit_ws.update(cx, |ws, cx| ws.edit_hex(window, cx));
        })
        .w(px(120.0))
        .h(st.px(|m| m.control_height))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(st.px(|m| m.control_radius))
        .bg(st.c(|c| c.hover))
        .shadow(st.well())
        .border_1()
        .border_color(if editing && focused {
            st.c(|c| c.accent)
        } else {
            st.c(|c| c.line)
        })
        .cursor_text()
        .text_size(st.px(|m| m.ui_text))
        .text_color(st.c(|c| c.text))
        .tip("Type a hex colour")
        .child(shown)
        .when(editing, |d| {
            d.child(
                div()
                    .ml(px(1.0))
                    .w(px(1.0))
                    .h(px(14.0))
                    .bg(st.c(|c| c.accent)),
            )
        });
    let mut strip = div()
        .flex()
        .w_full()
        .h(px(22.0))
        .rounded(px(7.0))
        .overflow_hidden()
        .shadow(st.well());
    const STEPS: usize = 36;
    for i in 0..STEPS {
        let color = hue_swatch(i as f32 / STEPS as f32);
        strip = strip.child(
            div()
                .id(("hue", i))
                .flex_1()
                .h_full()
                .cursor_pointer()
                .bg(hsla(color))
                .hover(|d| d.opacity(0.8))
                .on_click({
                    let pick = change(ws, move |s| s.set_accent(Accent::Custom(color)));
                    move |_, window, cx| pick(window, cx)
                }),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(14.0))
        .rounded(px(12.0))
        .bg(st.c(|c| c.panel))
        .shadow(st.well())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(14.0))
                .child(
                    div()
                        .size(px(38.0))
                        .rounded(px(10.0))
                        .bg(resolved)
                        .shadow(st.elev()),
                )
                .child(field)
                .child(
                    div()
                        .flex_1()
                        .text_size(px(12.0))
                        .text_color(st.c(|c| c.muted))
                        .child(note),
                ),
        )
        .child(strip)
        .into_any_element()
}

// ── shortcuts ────────────────────────────────────────────────────────────

fn shortcuts_section(ws: &WeakEntity<Workspace>, st: &Style) -> AnyElement {
    let mut col = div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .child(system::keymap_row(ws, st));
    for g in shortcuts::GROUPS {
        let mut list = div()
            .flex()
            .flex_col()
            .rounded(px(12.0))
            .bg(st.c(|c| c.panel))
            .shadow(st.well());
        let last = g.rows.len().saturating_sub(1);
        for (i, (what, keys)) in g.rows.iter().enumerate() {
            let mut caps = div().flex().items_center().gap(px(4.0));
            for (ai, alt) in keys.split(" / ").enumerate() {
                if ai > 0 {
                    caps = caps.child(
                        div()
                            .px(px(4.0))
                            .text_size(px(11.0))
                            .text_color(st.c(|c| c.faint))
                            .child("or"),
                    );
                }
                for cap in shortcuts::caps(alt) {
                    caps = caps.child(keycap(cap, st));
                }
            }
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .min_h(px(38.0))
                    .px(px(14.0))
                    .when(i < last, |d| d.border_b_1().border_color(st.c(|c| c.line)))
                    .child(
                        div()
                            .flex_1()
                            .text_size(st.px(|m| m.ui_text))
                            .text_color(st.c(|c| c.text))
                            .child(*what),
                    )
                    .child(caps),
            );
        }
        col = col.child(group(g.title, None, list, st));
    }
    col.into_any_element()
}

fn keycap(text: &str, st: &Style) -> AnyElement {
    div()
        .min_w(px(22.0))
        .h(px(22.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .bg(st.c(|c| c.raised))
        .shadow(st.elev())
        .text_size(px(11.0))
        .text_color(st.c(|c| c.muted))
        .child(text.to_string())
        .into_any_element()
}
