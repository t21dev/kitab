// SPDX-License-Identifier: AGPL-3.0-or-later

//! The menu bar (Settings › Appearance › Menu bar, off by default): File,
//! Edit, View, Go, Tools and Help in a row under the title bar, for people
//! who look for a command where desktop programs have always kept them.
//! Each title opens an ordinary menu (`menu.rs`) whose rows run the same
//! actions as the keys and the command palette. Alt and a title's first
//! letter opens it; Left and Right move between the menus.
//!
//! On macOS the same menus are also the system's menu bar at the top of the
//! screen (`set_native`), always there, with a Kitab menu before them and a
//! Window menu before Help.

use super::Workspace;
use crate::actions::*;
use crate::menu::MenuItem;
use crate::style::Style;
use gpui::{
    Action, AnyElement, App, Bounds, Context, InteractiveElement, IntoElement, KeyBinding,
    KeyDownEvent, Menu, MenuItem as OsItem, ParentElement, Pixels, Point,
    StatefulInteractiveElement, Styled, SystemMenuType, Window, actions, canvas, div, point,
    prelude::FluentBuilder, px,
};

actions!(
    kitab,
    [
        // The system's menu bar on macOS.
        Quit,
        HideApp,
        HideOtherApps,
        ShowAllApps,
        MinimizeWindow,
        ZoomWindow,
    ]
);

/// What a row needs to make sense.
#[derive(Clone, Copy, PartialEq)]
enum Needs {
    Nothing,
    /// A document in the active tab.
    Doc,
    Pdf,
    Markdown,
}

struct Entry {
    label: &'static str,
    /// The default shortcut, as shown ("" when it has none).
    keys: &'static str,
    needs: Needs,
    action: fn() -> Box<dyn Action>,
}

struct BarMenu {
    title: &'static str,
    /// The letter Alt opens it with (its first).
    key: &'static str,
    /// `None` is a separator.
    rows: &'static [Option<Entry>],
}

macro_rules! row {
    ($label:expr, $keys:expr, $needs:ident, $action:expr) => {
        Some(Entry {
            label: $label,
            keys: $keys,
            needs: Needs::$needs,
            action: || Box::new($action),
        })
    };
}

const MENUS: &[BarMenu] = &[
    BarMenu {
        title: "File",
        key: "f",
        rows: &[
            row!("Open a file…", "Ctrl+O", Nothing, OpenFile),
            row!("Home", "Ctrl+Shift+H", Nothing, ShowHome),
            row!(
                "Reopen closed tab",
                "Ctrl+Shift+T",
                Nothing,
                ReopenClosedTab
            ),
            None,
            row!("Save marks into the PDF", "Ctrl+S", Pdf, SaveMarks),
            row!("Save a copy with marks…", "", Pdf, SaveCopyWithMarks),
            row!(
                "Save pages as a new PDF…",
                "Ctrl+Shift+E",
                Pdf,
                ExtractPages
            ),
            row!("Print…", "Ctrl+P", Doc, Print),
            None,
            row!("Close tab", "Ctrl+W", Doc, CloseTab),
            None,
            row!("Settings…", "Ctrl+,", Nothing, OpenSettings),
        ],
    },
    BarMenu {
        title: "Edit",
        key: "e",
        rows: &[
            row!("Undo a mark", "Ctrl+Z", Pdf, MarkupUndo),
            row!("Redo a mark", "Ctrl+Y", Pdf, MarkupRedo),
            None,
            row!("Copy", "Ctrl+C", Doc, Copy),
            None,
            row!("Find in document", "Ctrl+F", Doc, Find),
            row!("Next result", "F3", Doc, FindNext),
            row!("Previous result", "Shift+F3", Doc, FindPrev),
            None,
            row!("Highlight the selected text", "H", Pdf, HighlightSelection),
            row!("Bookmark this page", "Ctrl+D", Doc, ToggleBookmark),
            None,
            row!("Edit Markdown", "Ctrl+E", Markdown, ToggleEdit),
        ],
    },
    BarMenu {
        title: "View",
        key: "v",
        rows: &[
            row!("Zoom in", "Ctrl+=", Doc, ZoomIn),
            row!("Zoom out", "Ctrl+-", Doc, ZoomOut),
            row!("Fit width", "Ctrl+2", Doc, FitWidth),
            row!("Fit page", "Ctrl+0", Doc, FitPage),
            row!("Actual size", "Ctrl+1", Doc, ActualSize),
            None,
            row!("Single page", "Ctrl+6", Doc, SinglePage),
            row!("Two pages side by side", "Ctrl+7", Doc, TwoPages),
            row!("Book view", "Ctrl+8", Doc, BookView),
            None,
            row!("Rotate clockwise", "Ctrl+Shift+=", Doc, RotateRight),
            row!("Rotate counter-clockwise", "Ctrl+Shift+-", Doc, RotateLeft),
            None,
            row!("Contents", "F12", Nothing, ToggleSidebar),
            row!("Hide or show the dock", "Ctrl+.", Doc, ToggleDock),
            row!("Show or hide the status bar", "Ctrl+I", Doc, ToggleFileInfo),
            None,
            row!("Focus mode", "F11", Nothing, ToggleMinimal),
            row!("Presentation mode", "F5", Doc, Present),
            row!("Split view with the next tab", "Ctrl+\\", Doc, ToggleSplit),
            None,
            row!("Next theme", "Ctrl+Shift+L", Nothing, CycleTheme),
        ],
    },
    BarMenu {
        title: "Go",
        key: "g",
        rows: &[
            row!("Next page", "N", Doc, NextPage),
            row!("Previous page", "P", Doc, PrevPage),
            row!("First page", "Home", Doc, FirstPage),
            row!("Last page", "End", Doc, LastPage),
            row!("Go to page…", "Ctrl+G", Doc, GoToPage),
            None,
            row!("Back", "Alt+Left", Doc, HistoryBack),
            row!("Forward", "Alt+Right", Doc, HistoryForward),
            None,
            row!("Next tab", "Ctrl+Tab", Doc, NextTab),
            row!("Previous tab", "Ctrl+Shift+Tab", Doc, PrevTab),
        ],
    },
    BarMenu {
        title: "Tools",
        key: "t",
        rows: &[
            row!("Draw and mark up", "Ctrl+Shift+D", Pdf, ToggleMarkup),
            row!("Read aloud", "Ctrl+Shift+U", Doc, ReadAloud),
            row!("Stop reading", "", Doc, StopReading),
            row!("Page tools…", "", Pdf, PageTools),
            row!("Sign…", "", Pdf, SignPdf),
            row!("Password…", "", Pdf, PdfPassword),
            None,
            row!("Command palette", "Ctrl+K", Nothing, CommandPalette),
        ],
    },
    BarMenu {
        title: "Help",
        key: "h",
        rows: &[
            row!("Keyboard shortcuts", "Ctrl+/", Nothing, ShowShortcuts),
            row!("Edit keymap.json", "", Nothing, OpenKeymap),
            None,
            row!("Check for updates", "", Nothing, CheckForUpdates),
        ],
    },
];

/// macOS: the keys and commands only its menu bar has, and the bar itself.
/// Elsewhere this does nothing.
pub(crate) fn init_native(cx: &mut App) {
    if !cfg!(target_os = "macos") {
        // Ctrl+Q quits on Windows and Linux too (SumatraPDF's key), asking
        // about unsaved changes as closing the window does.
        cx.bind_keys([KeyBinding::new(
            "ctrl-q",
            Quit,
            Some(crate::actions::WORKSPACE),
        )]);
        return;
    }
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", HideApp, None),
        KeyBinding::new("cmd-alt-h", HideOtherApps, None),
        KeyBinding::new("cmd-m", MinimizeWindow, None),
    ]);
    // With a window open its own Quit runs first (it asks about unsaved
    // changes); this one is for when there is none.
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &HideApp, cx| cx.hide());
    cx.on_action(|_: &HideOtherApps, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAllApps, cx| cx.unhide_other_apps());
    cx.on_action(|_: &MinimizeWindow, cx| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.minimize_window());
        }
    });
    cx.on_action(|_: &ZoomWindow, cx| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.zoom_window());
        }
    });
    set_native(cx);
}

/// macOS: hand the menus to the system's menu bar. It shows each command's
/// key as bound right now, so this runs again when keymap.json changes them.
pub(crate) fn set_native(cx: &mut App) {
    if cfg!(target_os = "macos") {
        cx.set_menus(native_menus());
    }
}

/// The system menu bar's menus: Kitab, the in-window bar's six, and Window
/// before Help. Settings and the update check live in the Kitab menu, as
/// they do in every Mac program, so they are left out of File and Help.
fn native_menus() -> Vec<Menu> {
    let in_app_menu = [OpenSettings.name(), CheckForUpdates.name()];
    let mut menus = vec![Menu {
        name: "Kitab".into(),
        items: vec![
            OsItem::action("Settings…", OpenSettings),
            OsItem::action("Check for Updates", CheckForUpdates),
            OsItem::separator(),
            OsItem::os_submenu("Services", SystemMenuType::Services),
            OsItem::separator(),
            OsItem::action("Hide Kitab", HideApp),
            OsItem::action("Hide Others", HideOtherApps),
            OsItem::action("Show All", ShowAllApps),
            OsItem::separator(),
            OsItem::action("Quit Kitab", Quit),
        ],
    }];
    for menu in MENUS {
        if menu.title == "Help" {
            menus.push(Menu {
                name: "Window".into(),
                items: vec![
                    OsItem::action("Minimize", MinimizeWindow),
                    OsItem::action("Zoom", ZoomWindow),
                ],
            });
        }
        let mut items: Vec<OsItem> = Vec::new();
        for row in menu.rows {
            match row {
                // No separator first, and never two together.
                None => {
                    if matches!(items.last(), Some(OsItem::Action { .. })) {
                        items.push(OsItem::separator());
                    }
                }
                Some(e) => {
                    let action = (e.action)();
                    if !in_app_menu.contains(&action.name()) {
                        items.push(OsItem::Action {
                            name: e.label.into(),
                            action,
                            os_action: None,
                        });
                    }
                }
            }
        }
        if matches!(items.last(), Some(OsItem::Separator)) {
            items.pop();
        }
        menus.push(Menu {
            name: menu.title.into(),
            items,
        });
    }
    menus
}

/// Contexts where keys are typed. The reader's keys are unbound in them.
const TYPING: &[&str] = &[
    FIND_BAR,
    MD_EDITOR,
    MARKUP_HEX,
    PROMPT,
    PAGE_FIELD,
    crate::text_field::TEXT_FIELD,
];

/// How many menus the bar has.
pub(super) const COUNT: usize = MENUS.len();

/// The bar's height.
const HEIGHT: f32 = 28.0;
/// The width of the menus it opens.
const MENU_WIDTH: f32 = 300.0;

impl Workspace {
    /// The menu of the bar that is open, if one is.
    pub(super) fn bar_open(&self) -> Option<usize> {
        self.bar_open.filter(|_| self.menu.is_some())
    }

    /// The bar: its titles in a row. `None` when the setting is off.
    pub(super) fn menu_bar(&self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !st.menu_bar {
            return None;
        }
        let open = self.bar_open();
        // The letters Alt opens the menus with show while Alt is held, or
        // while a menu is open.
        let letters = self.alt_down || open.is_some();
        let mut row = div()
            .id("menu-bar")
            .flex_none()
            .h(px(HEIGHT))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .border_b_1()
            .border_color(st.c(|c| c.line))
            .text_size(st.px(|m| m.ui_text))
            .text_color(st.c(|c| c.muted));
        for (ix, menu) in MENUS.iter().enumerate() {
            let on = open == Some(ix);
            let bounds = self.bar_bounds.clone();
            let (first, rest) = menu.title.split_at(1);
            row = row.child(
                div()
                    .id(("menu-bar-title", ix))
                    .relative()
                    .h(px(22.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .rounded(px(6.0))
                    .when(on, |d| d.bg(st.c(|c| c.hover)).text_color(st.c(|c| c.text)))
                    .when(!on, |d| {
                        d.hover(|d| d.bg(st.hover_bg()).text_color(st.c(|c| c.text)))
                    })
                    .on_click(cx.listener(move |ws, _, window, cx| {
                        if ws.bar_open() == Some(ix) {
                            ws.close_menu(window, cx);
                        } else {
                            ws.open_bar_menu(ix, false, window, cx);
                        }
                    }))
                    .child(div().when(letters, |d| d.underline()).child(first))
                    .child(rest)
                    // Where the title is, for placing its menu under it and
                    // for the pointer moving along the bar while one is open.
                    .child(
                        canvas(
                            move |b, _, _| {
                                let mut all = bounds.get();
                                all[ix] = b;
                                bounds.set(all);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    ),
            );
        }
        Some(row.into_any_element())
    }

    /// Open menu `ix` under its title. `keyboard`: opened with a key, so its
    /// first row is picked.
    pub(super) fn open_bar_menu(
        &mut self,
        ix: usize,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = MENUS.get(ix) else { return };
        let reader = self.active_reader().cloned();
        let (doc, pdf, markdown) = match &reader {
            Some(r) => {
                let r = r.read(cx);
                (true, r.is_pdf(), r.is_markdown())
            }
            None => (false, false, false),
        };
        let items: Vec<MenuItem> = menu
            .rows
            .iter()
            .map(|row| match row {
                None => MenuItem::Separator,
                Some(e) => {
                    let action = e.action;
                    let off = match e.needs {
                        Needs::Nothing => false,
                        Needs::Doc => !doc,
                        Needs::Pdf => !pdf,
                        Needs::Markdown => !markdown,
                    };
                    // Once the menu has closed and the page has its focus
                    // back, so the action reaches it.
                    let item = MenuItem::new(e.label, move |window, cx| {
                        window.defer(cx, move |window, cx| {
                            window.dispatch_action(action(), cx);
                        });
                    })
                    .disabled(off);
                    if e.keys.is_empty() {
                        item
                    } else {
                        item.key(e.keys)
                    }
                }
            })
            .collect();
        let title = self.bar_bounds.get()[ix];
        let at = point(title.left(), title.bottom() + px(3.0));
        self.open_menu(at, false, MENU_WIDTH, items, window, cx);
        self.bar_open = Some(ix);
        if keyboard && let Some(m) = self.menu.as_mut() {
            m.hi = m.step(true);
        }
    }

    /// The title under `p`, if the pointer is on the bar.
    fn bar_title_at(&self, p: Point<Pixels>) -> Option<usize> {
        let all: [Bounds<Pixels>; COUNT] = self.bar_bounds.get();
        all.iter().position(|b| b.contains(&p))
    }

    /// The pointer moved while a menu is open: along the bar, the menu under
    /// it opens instead.
    pub(super) fn bar_hover(
        &mut self,
        p: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.bar_open() else { return };
        if let Some(ix) = self.bar_title_at(p).filter(|ix| *ix != open) {
            self.open_bar_menu(ix, false, window, cx);
        }
    }

    /// A click outside an open menu. On another title its menu opens; on the
    /// open one's title (or anywhere else) the menu closes. Whether it was
    /// dealt with here.
    pub(super) fn bar_press(
        &mut self,
        p: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = self.bar_open() else {
            return false;
        };
        match self.bar_title_at(p) {
            Some(ix) if ix != open => {
                self.open_bar_menu(ix, false, window, cx);
                true
            }
            _ => false,
        }
    }

    /// Left and Right with a bar menu open: the menu beside it.
    pub(super) fn bar_step(
        &mut self,
        right: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = self.bar_open() else {
            return false;
        };
        let next = if right {
            (open + 1) % COUNT
        } else {
            (open + COUNT - 1) % COUNT
        };
        self.open_bar_menu(next, true, window, cx);
        true
    }

    /// macOS: a key nothing in the window took is offered to the system's
    /// menu bar next, which would run the command that key has elsewhere:
    /// Ctrl+F in a note's text box would open Find, though the key is
    /// unbound there. So in a place where keys are typed, a Ctrl key that
    /// got this far ends here.
    pub(super) fn keep_from_os_menu(
        &mut self,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let m = &e.keystroke.modifiers;
        if !cfg!(target_os = "macos") || !m.control || m.platform {
            return;
        }
        let typing = window
            .context_stack()
            .iter()
            .any(|c| TYPING.iter().any(|name| c.contains(name)));
        if typing {
            cx.stop_propagation();
        }
    }

    /// Alt and a title's first letter opens that menu.
    pub(super) fn bar_key(
        &mut self,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !Style::get(cx).menu_bar {
            return;
        }
        let k = &e.keystroke;
        let m = &k.modifiers;
        // Ctrl+Alt is AltGr on many keyboards: that types a character.
        if !m.alt || m.control || m.shift || m.platform {
            return;
        }
        if let Some(ix) = MENUS.iter().position(|menu| menu.key == k.key) {
            self.open_bar_menu(ix, true, window, cx);
            cx.stop_propagation();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_opens_with_its_first_letter_and_they_differ() {
        let mut seen = std::collections::HashSet::new();
        for menu in MENUS {
            assert_eq!(menu.title[..1].to_lowercase(), menu.key, "{}", menu.title);
            assert!(
                seen.insert(menu.key),
                "two menus open with Alt+{}",
                menu.key
            );
        }
    }

    #[test]
    fn every_row_builds_its_action_and_no_menu_ends_on_a_separator() {
        for menu in MENUS {
            assert!(
                menu.rows.first().is_some_and(Option::is_some),
                "{}",
                menu.title
            );
            assert!(
                menu.rows.last().is_some_and(Option::is_some),
                "{}",
                menu.title
            );
            for row in menu.rows.iter().flatten() {
                assert!(!(row.action)().name().is_empty(), "{}", row.label);
            }
        }
    }

    #[test]
    fn the_system_menu_bar_has_every_command_once() {
        let menus = native_menus();
        let titles: Vec<String> = menus.iter().map(|m| m.name.to_string()).collect();
        assert_eq!(
            titles,
            [
                "Kitab", "File", "Edit", "View", "Go", "Tools", "Window", "Help"
            ]
        );
        let mut names = Vec::new();
        for menu in &menus {
            assert!(!menu.items.is_empty(), "{}", menu.name);
            for (ix, item) in menu.items.iter().enumerate() {
                match item {
                    OsItem::Action { action, .. } => names.push(action.name()),
                    OsItem::Separator => {
                        let first_or_last = ix == 0 || ix + 1 == menu.items.len();
                        assert!(!first_or_last, "{}: a separator at an end", menu.name);
                        assert!(
                            !matches!(menu.items[ix - 1], OsItem::Separator),
                            "{}: two separators together",
                            menu.name
                        );
                    }
                    _ => {}
                }
            }
        }
        let rows = MENUS.iter().flat_map(|m| m.rows.iter().flatten()).count();
        // The Kitab menu's four of its own and Window's two.
        assert_eq!(names.len(), rows + 6);
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "a command is in two menus");
    }

    /// The keys shown beside a row are the ones the cheat sheet lists for it
    /// (which a test there checks against the bound keys).
    #[test]
    fn shown_shortcuts_are_listed_shortcuts() {
        let listed: Vec<&str> = crate::shortcuts::GROUPS
            .iter()
            .flat_map(|g| g.rows.iter())
            .flat_map(|(_, keys)| keys.split(" / "))
            .collect();
        for row in MENUS.iter().flat_map(|m| m.rows.iter().flatten()) {
            if !row.keys.is_empty() {
                assert!(listed.contains(&row.keys), "{}: {}", row.label, row.keys);
            }
        }
    }
}
