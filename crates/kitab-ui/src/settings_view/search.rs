// SPDX-License-Identifier: AGPL-3.0-or-later

//! Search in Settings: every setting by its title, with a few other words
//! people might type for it. Typing in the rail's field lists the matches in
//! place of the section; picking one opens its section.

use super::Section;
use crate::icons::icon;
use crate::shell::Workspace;
use crate::style::Style;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    WeakEntity, div, px, svg,
};

/// (section, title, other words to match).
const INDEX: &[(Section, &str, &str)] = &[
    (
        Section::Appearance,
        "Layout",
        "tabs side top sidebar arrangement",
    ),
    (
        Section::Appearance,
        "Theme",
        "dark light sepia colour color mode night",
    ),
    (
        Section::Appearance,
        "Accent",
        "colour color highlight custom hex",
    ),
    (
        Section::Appearance,
        "Density",
        "compact dense standard spacing size",
    ),
    (
        Section::Appearance,
        "Interface size",
        "scale zoom bigger smaller dpi 125 150 200 text size ui",
    ),
    (
        Section::Appearance,
        "Messages",
        "toast notification popup position top bottom left right centre center",
    ),
    (Section::Appearance, "Tabs", "tab bar"),
    (
        Section::Appearance,
        "The + button",
        "new tab plus open file home",
    ),
    (
        Section::Appearance,
        "Show how many tabs are open",
        "tab count number badge",
    ),
    (
        Section::Appearance,
        "Menu bar",
        "menubar file edit view go tools help classic alt menus",
    ),
    (
        Section::Appearance,
        "Quick dock",
        "dock toolbar bottom top left right position",
    ),
    (Section::Appearance, "Compact dock", "dock small"),
    (
        Section::Appearance,
        "Fade the dock while reading",
        "dock hide transparent",
    ),
    (
        Section::Appearance,
        "Toolbar",
        "title bar buttons dock print password rotate bookmark zoom find more",
    ),
    (
        Section::Appearance,
        "Animations",
        "motion reduce transitions",
    ),
    (Section::Appearance, "Flat surfaces", "shadows depth"),
    (
        Section::Reading,
        "Page navigator",
        "scrubber stepper counter hidden slider page number go to",
    ),
    (Section::Reading, "Page corners", "rounded square pages"),
    (
        Section::Reading,
        "Show the scroll bar",
        "scrollbar scrolling",
    ),
    (
        Section::Reading,
        "Smooth scrolling",
        "glide wheel mouse animation scroll inertia fluid",
    ),
    (
        Section::Reading,
        "New documents open at",
        "zoom fit width fit page actual size default open maximized whole page size",
    ),
    (
        Section::Reading,
        "Markdown typeface",
        "font face serif sans mono system markdown text",
    ),
    (
        Section::Reading,
        "Code colours",
        "markdown syntax highlighting",
    ),
    (
        Section::Reading,
        "Books",
        "epub typeface font line spacing margins ebook",
    ),
    (Section::Reading, "Typeface", "font serif sans books"),
    (Section::Reading, "Line spacing", "books leading"),
    (Section::Reading, "Margins", "books width"),
    (
        Section::Reading,
        "Status bar",
        "file size pages word count reading time paper size position zoom path folder location",
    ),
    (
        Section::Reading,
        "File path",
        "status bar path folder location where file",
    ),
    (
        Section::Reading,
        "Show the markup tools",
        "drawing annotate highlight pen ink",
    ),
    (
        Section::Reading,
        "Focus mode",
        "minimal distraction free hide tabs sidebar dock navigator",
    ),
    (
        Section::Home,
        "Layout",
        "home covers list grid view books shelf",
    ),
    (
        Section::Home,
        "Cover size",
        "home small medium large thumbnails covers",
    ),
    (
        Section::Home,
        "Reading stats",
        "home streak today week time",
    ),
    (Section::Home, "Continue reading", "home in progress resume"),
    (
        Section::Library,
        "Watched folders",
        "folder scan library add",
    ),
    (
        Section::FileTypes,
        "Default app",
        "file types associations open with pdf epub default",
    ),
    (Section::FileTypes, "Thumbnails", "explorer preview icons"),
    (
        Section::FileTypes,
        "Remove Kitab's file types",
        "unregister",
    ),
    (
        Section::Shortcuts,
        "Keyboard shortcuts",
        "keys keymap hotkeys bindings",
    ),
    (Section::Shortcuts, "Your own keys", "keymap custom json"),
    (
        Section::Signing,
        "Trusted certificates",
        "signatures sign certificate trust",
    ),
    (Section::Advanced, "Clear cached pages", "memory cache ram"),
    (Section::Advanced, "Welcome guide", "help tutorial sample"),
    (
        Section::Advanced,
        "Restore default settings",
        "reset defaults",
    ),
    (
        Section::Advanced,
        "Clear reading history",
        "history recent privacy",
    ),
    (
        Section::Advanced,
        "Optimize the database",
        "library database vacuum",
    ),
    (Section::Advanced, "Data folder", "location storage files"),
    (Section::About, "Check for updates", "update version new"),
    (Section::About, "Check on start-up", "automatic updates"),
    (Section::About, "Source code", "github"),
    (Section::About, "What's new", "changelog release notes"),
    (Section::About, "Report a problem", "bug issue feedback"),
    (Section::About, "Privacy policy", "telemetry privacy"),
    (Section::About, "Licence", "license agpl"),
    (
        Section::About,
        "Third-party notices",
        "mupdf gpui open source",
    ),
];

/// The settings that match `query`: every word must appear in the title,
/// the other words or the section's name.
pub(super) fn matches(query: &str) -> Vec<(Section, &'static str)> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return Vec::new();
    }
    INDEX
        .iter()
        .filter(|(s, title, extra)| {
            let hay = format!("{} {} {}", title, extra, s.label()).to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .map(|(s, t, _)| (*s, *t))
        .collect()
}

/// The results, in place of a section.
pub(super) fn results(ws: &WeakEntity<Workspace>, query: &str, st: &Style) -> AnyElement {
    let found = matches(query);
    if found.is_empty() {
        return div()
            .pt(px(24.0))
            .text_size(px(13.0))
            .text_color(st.c(|c| c.muted))
            .child(format!(
                "No setting matches \u{201c}{}\u{201d}.",
                query.trim()
            ))
            .into_any_element();
    }
    let mut list = div().flex().flex_col().gap(px(6.0));
    for (i, (section, title)) in found.into_iter().enumerate() {
        let ws = ws.clone();
        list = list.child(
            crate::kbd::stop_fill(div().id(("settings-hit", i)), st, st.c(|c| c.raised))
                .flex()
                .items_center()
                .gap(px(12.0))
                .px(px(14.0))
                .py(px(10.0))
                .rounded(px(10.0))
                .bg(st.c(|c| c.raised))
                .cursor_pointer()
                .hover(|d| d.bg(st.raised_hover()))
                .on_click(move |_, window, cx| {
                    let _ = ws.update(cx, |ws, cx| ws.pick_setting(section, title, window, cx));
                })
                .child(
                    svg()
                        .path(icon(section.icon()))
                        .size(px(16.0))
                        .text_color(st.c(|c| c.muted)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(13.5))
                        .text_color(st.c(|c| c.text))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(st.c(|c| c.muted))
                        .child(section.label()),
                ),
        );
    }
    list.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_by_title_extra_words_and_section() {
        assert!(matches("theme").iter().any(|(_, t)| *t == "Theme"));
        assert!(matches("dark").iter().any(|(_, t)| *t == "Theme"));
        assert!(
            matches("tab count")
                .iter()
                .any(|(_, t)| t.contains("tabs are open"))
        );
        assert!(
            matches("about licence")
                .iter()
                .any(|(_, t)| *t == "Licence")
        );
        assert!(matches("   ").is_empty());
        assert!(matches("zzzz").is_empty());
    }
}
