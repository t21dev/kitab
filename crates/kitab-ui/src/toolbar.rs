// SPDX-License-Identifier: AGPL-3.0-or-later

//! The title bar's and the dock's buttons, in order (Settings › Appearance ›
//! Toolbar, DESIGN.md § Toolbar, GitHub #25). Each place holds an ordered list
//! of tools; a tool is in one place at most, and More (⋯) is always in one.
//! Settings files from before this kept a switch per button (`DockItems`,
//! `TitleItems`): [`Toolbar::from_items`] reads those in today's order, and
//! [`Toolbar::to_items`] still writes them, so an older Kitab keeps working.

use crate::settings::{DockItems, TitleItems};
use serde::{Deserialize, Serialize};

/// One toolbar entry. Most are one button; Back and forward, Zoom (− % +)
/// and Fit (width, page) are a group of buttons that move together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    Sidebar,
    History,
    Find,
    Bookmark,
    Zoom,
    Fit,
    Hand,
    TwoPages,
    BookView,
    RotateLeft,
    Rotate,
    Focus,
    Markup,
    ReadAloud,
    Present,
    Password,
    Open,
    Print,
    Export,
    More,
}

/// Where the add menu lists a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolGroup {
    Navigate,
    Zoom,
    Layout,
    Tools,
    File,
}

impl ToolGroup {
    pub const ALL: [ToolGroup; 5] = [
        ToolGroup::Navigate,
        ToolGroup::Zoom,
        ToolGroup::Layout,
        ToolGroup::Tools,
        ToolGroup::File,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ToolGroup::Navigate => "Navigate",
            ToolGroup::Zoom => "Zoom",
            ToolGroup::Layout => "Layout",
            ToolGroup::Tools => "Tools",
            ToolGroup::File => "File",
        }
    }
}

impl Tool {
    /// Every tool, in the add menu's order.
    pub const ALL: [Tool; 20] = [
        Tool::Sidebar,
        Tool::History,
        Tool::Find,
        Tool::Bookmark,
        Tool::Zoom,
        Tool::Fit,
        Tool::Hand,
        Tool::TwoPages,
        Tool::BookView,
        Tool::RotateLeft,
        Tool::Rotate,
        Tool::Focus,
        Tool::Markup,
        Tool::ReadAloud,
        Tool::Present,
        Tool::Password,
        Tool::Open,
        Tool::Print,
        Tool::Export,
        Tool::More,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Sidebar => "Sidebar",
            Tool::History => "Back and forward",
            Tool::Find => "Find",
            Tool::Bookmark => "Bookmark",
            Tool::Zoom => "Zoom",
            Tool::Fit => "Fit width and fit page",
            Tool::Hand => "Hand tool",
            Tool::TwoPages => "Two pages",
            Tool::BookView => "Book view",
            Tool::RotateLeft => "Rotate left",
            Tool::Rotate => "Rotate right",
            Tool::Focus => "Focus mode",
            Tool::Markup => "Draw and mark up",
            Tool::ReadAloud => "Read aloud",
            Tool::Present => "Present",
            Tool::Password => "Password",
            Tool::Open => "Open file",
            Tool::Print => "Print",
            Tool::Export => "Export as Markdown",
            Tool::More => "More",
        }
    }

    pub fn group(self) -> ToolGroup {
        match self {
            Tool::Sidebar | Tool::History | Tool::Find | Tool::Bookmark => ToolGroup::Navigate,
            Tool::Zoom | Tool::Fit | Tool::Hand => ToolGroup::Zoom,
            Tool::TwoPages | Tool::BookView | Tool::RotateLeft | Tool::Rotate | Tool::Focus => {
                ToolGroup::Layout
            }
            Tool::Markup | Tool::ReadAloud | Tool::Present | Tool::Password => ToolGroup::Tools,
            Tool::Open | Tool::Print | Tool::Export | Tool::More => ToolGroup::File,
        }
    }

    /// The icon that stands for it (in menus, and its first button).
    pub fn icon(self) -> &'static str {
        match self {
            Tool::Sidebar => "sidebar",
            Tool::History => "arrow-left",
            Tool::Find => "search",
            Tool::Bookmark => "bookmark",
            Tool::Zoom => "plus",
            Tool::Fit => "fit-width",
            Tool::Hand => "hand",
            Tool::TwoPages => "pages",
            Tool::BookView => "book",
            Tool::RotateLeft => "rotate-left",
            Tool::Rotate => "rotate",
            Tool::Focus => "focus",
            Tool::Markup => "pen",
            Tool::ReadAloud => "speaker",
            Tool::Present => "play",
            Tool::Password => "lock",
            Tool::Open => "folder-open",
            Tool::Print => "print",
            Tool::Export => "download",
            Tool::More => "more-vertical",
        }
    }

    /// Buttons it puts on a bar (Zoom's percentage counts as one), for the
    /// title bar's width sums.
    pub fn width(self) -> usize {
        match self {
            Tool::History | Tool::Fit => 2,
            Tool::Zoom => 2,
            _ => 1,
        }
    }

    /// Where it shows, for a tool that a bar leaves out in some documents
    /// (Settings › Appearance › Toolbar says so, GitHub #32). Must agree with
    /// `Reader::tool_buttons`.
    pub fn only_in(self) -> Option<&'static str> {
        match self {
            Tool::Hand => Some("Shows for PDFs, comics, XPS and DjVu"),
            Tool::Bookmark | Tool::Markup => {
                Some("Shows for PDFs, comics, XPS and DjVu, not books")
            }
            Tool::TwoPages | Tool::BookView | Tool::Present | Tool::RotateLeft | Tool::Rotate => {
                Some("Shows for all but Markdown")
            }
            Tool::Password => Some("Shows for PDFs"),
            _ => None,
        }
    }

    /// The compact dock (and a narrow window) leaves these out first.
    pub fn trimmed_when_compact(self) -> bool {
        matches!(
            self,
            Tool::Sidebar
                | Tool::History
                | Tool::Fit
                | Tool::Hand
                | Tool::Rotate
                | Tool::RotateLeft
        )
    }
}

/// Which bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bar {
    Title,
    Dock,
}

impl Bar {
    pub fn other(self) -> Bar {
        match self {
            Bar::Title => Bar::Dock,
            Bar::Dock => Bar::Title,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Bar::Title => "title bar",
            Bar::Dock => "dock",
        }
    }
}

/// Where the dock's buttons sit along it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DockAlign {
    Start,
    #[default]
    Centre,
    End,
}

/// The two bars' buttons, in order, and where the dock's sit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Toolbar {
    #[serde(deserialize_with = "known_tools")]
    pub title: Vec<Tool>,
    #[serde(deserialize_with = "known_tools")]
    pub dock: Vec<Tool>,
    pub dock_align: DockAlign,
}

/// A list of tools, skipping names this Kitab doesn't know (a newer one
/// wrote them): one unknown button mustn't cost every other setting.
fn known_tools<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Tool>, D::Error> {
    let raw: Vec<serde_json::Value> = Vec::deserialize(d)?;
    Ok(raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

impl Default for Toolbar {
    /// What Kitab shipped with: Find, Draw, Password, Open file and More up
    /// top; the dock keeps paging, zoom and fit, read aloud and rotate.
    fn default() -> Self {
        Toolbar::from_items(DockItems::default(), TitleItems::default())
    }
}

impl Toolbar {
    /// The lists the old per-button switches stood for, in the order the
    /// bars drew them.
    pub fn from_items(d: DockItems, t: TitleItems) -> Toolbar {
        let title_order = [
            (t.sidebar, Tool::Sidebar),
            (t.history, Tool::History),
            (t.zoom, Tool::Zoom),
            (t.fit, Tool::Fit),
            (t.find, Tool::Find),
            (t.bookmark, Tool::Bookmark),
            (t.markup, Tool::Markup),
            (t.rotate, Tool::Rotate),
            (t.print, Tool::Print),
            (t.password, Tool::Password),
            (t.minimal, Tool::Focus),
            (t.read_aloud, Tool::ReadAloud),
            (t.open, Tool::Open),
            (t.more, Tool::More),
        ];
        let title: Vec<Tool> = title_order
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, tool)| *tool)
            .collect();
        // A button moved up top kept its dock switch on; it isn't in both.
        let dock_order = [
            (d.sidebar, Tool::Sidebar),
            (d.history, Tool::History),
            (d.zoom, Tool::Zoom),
            (d.fit, Tool::Fit),
            // The hand tool sat in the fit group.
            (d.fit, Tool::Hand),
            (d.find, Tool::Find),
            (d.read_aloud, Tool::ReadAloud),
            (d.bookmark, Tool::Bookmark),
            (d.markup, Tool::Markup),
            (d.rotate, Tool::Rotate),
            (d.print, Tool::Print),
            (d.password, Tool::Password),
            (d.minimal, Tool::Focus),
            (!t.more, Tool::More),
        ];
        let dock = dock_order
            .iter()
            .filter(|(on, tool)| *on && !title.contains(tool))
            .map(|(_, tool)| *tool)
            .collect();
        Toolbar {
            title,
            dock,
            dock_align: DockAlign::Centre,
        }
        .normalized()
    }

    /// The old switches for these lists, written beside them so an older
    /// Kitab shows the same buttons (in its own order).
    pub fn to_items(&self) -> (DockItems, TitleItems) {
        let has = |list: &[Tool], tool| list.contains(&tool);
        let (t, d) = (&self.title, &self.dock);
        let title = TitleItems {
            sidebar: has(t, Tool::Sidebar),
            history: has(t, Tool::History),
            zoom: has(t, Tool::Zoom),
            fit: has(t, Tool::Fit),
            find: has(t, Tool::Find),
            markup: has(t, Tool::Markup),
            rotate: has(t, Tool::Rotate),
            bookmark: has(t, Tool::Bookmark),
            minimal: has(t, Tool::Focus),
            print: has(t, Tool::Print),
            password: has(t, Tool::Password),
            read_aloud: has(t, Tool::ReadAloud),
            more: has(t, Tool::More),
            open: has(t, Tool::Open),
        };
        // Old Kitab hides a dock button that's up top only by its title
        // switch, so a title bar button stays on in the dock list too.
        let either = |tool| has(d, tool) || has(t, tool);
        let dock = DockItems {
            sidebar: either(Tool::Sidebar),
            history: either(Tool::History),
            zoom: either(Tool::Zoom),
            fit: either(Tool::Fit),
            find: either(Tool::Find),
            markup: either(Tool::Markup),
            rotate: either(Tool::Rotate),
            bookmark: either(Tool::Bookmark),
            read_aloud: either(Tool::ReadAloud),
            minimal: either(Tool::Focus),
            print: either(Tool::Print),
            password: either(Tool::Password),
        };
        (dock, title)
    }

    /// Each tool once (the first place wins), and More somewhere.
    pub fn normalized(mut self) -> Toolbar {
        let mut seen = std::collections::HashSet::new();
        self.title.retain(|t| seen.insert(*t));
        self.dock.retain(|t| seen.insert(*t));
        if !seen.contains(&Tool::More) {
            self.title.push(Tool::More);
        }
        self
    }

    pub fn list(&self, bar: Bar) -> &[Tool] {
        match bar {
            Bar::Title => &self.title,
            Bar::Dock => &self.dock,
        }
    }

    fn list_mut(&mut self, bar: Bar) -> &mut Vec<Tool> {
        match bar {
            Bar::Title => &mut self.title,
            Bar::Dock => &mut self.dock,
        }
    }

    /// Where `tool` is, if anywhere.
    pub fn place(&self, tool: Tool) -> Option<(Bar, usize)> {
        [Bar::Title, Bar::Dock].into_iter().find_map(|bar| {
            self.list(bar)
                .iter()
                .position(|t| *t == tool)
                .map(|i| (bar, i))
        })
    }

    /// Put `tool` into `bar` before `before` (at the end when `None`, or
    /// when `before` isn't there), taking it from wherever it was.
    pub fn put(&mut self, tool: Tool, bar: Bar, before: Option<Tool>) {
        if before == Some(tool) {
            return;
        }
        self.title.retain(|t| *t != tool);
        self.dock.retain(|t| *t != tool);
        let list = self.list_mut(bar);
        match before.and_then(|b| list.iter().position(|t| *t == b)) {
            Some(i) => list.insert(i, tool),
            None => list.push(tool),
        }
    }

    /// Move `tool` one place earlier (`by` −1) or later (+1) along its bar.
    pub fn shift(&mut self, tool: Tool, by: isize) {
        let Some((bar, i)) = self.place(tool) else {
            return;
        };
        let list = self.list_mut(bar);
        let j = i as isize + by;
        if j >= 0 && (j as usize) < list.len() {
            list.swap(i, j as usize);
        }
    }

    /// Take `tool` off its bar. More stays: it's the way to everything else.
    pub fn remove(&mut self, tool: Tool) {
        if tool != Tool::More {
            self.title.retain(|t| *t != tool);
            self.dock.retain(|t| *t != tool);
        }
    }

    /// Tools not on `bar`, in the add menu's order.
    pub fn missing_from(&self, bar: Bar) -> impl Iterator<Item = Tool> + '_ {
        Tool::ALL
            .into_iter()
            .filter(move |t| !self.list(bar).contains(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_what_kitab_had() {
        let t = Toolbar::default();
        assert_eq!(
            t.title,
            [
                Tool::Find,
                Tool::Markup,
                Tool::Password,
                Tool::Open,
                Tool::More
            ]
        );
        assert_eq!(
            t.dock,
            [
                Tool::Sidebar,
                Tool::History,
                Tool::Zoom,
                Tool::Fit,
                Tool::Hand,
                Tool::ReadAloud,
                Tool::Rotate
            ]
        );
        assert_eq!(t.dock_align, DockAlign::Centre);
    }

    #[test]
    fn old_switches_come_back_the_same() {
        let t = Toolbar::default();
        let (d, ti) = t.to_items();
        assert_eq!(Toolbar::from_items(d, ti), t);
    }

    #[test]
    fn a_button_moves_along_and_across() {
        let mut t = Toolbar::default();
        t.put(Tool::TwoPages, Bar::Dock, Some(Tool::Zoom));
        assert_eq!(t.place(Tool::TwoPages), Some((Bar::Dock, 2)));
        t.put(Tool::Find, Bar::Dock, None);
        assert_eq!(t.place(Tool::Find), Some((Bar::Dock, t.dock.len() - 1)));
        assert!(!t.title.contains(&Tool::Find));
        t.shift(Tool::Find, -1);
        assert_eq!(t.place(Tool::Find), Some((Bar::Dock, t.dock.len() - 2)));
        t.shift(Tool::Sidebar, -1);
        assert_eq!(
            t.place(Tool::Sidebar),
            Some((Bar::Dock, 0)),
            "already first"
        );
        t.remove(Tool::Find);
        assert_eq!(t.place(Tool::Find), None);
        t.remove(Tool::More);
        assert!(t.place(Tool::More).is_some(), "More always stays");
    }

    #[test]
    fn a_damaged_file_still_has_more_and_no_doubles() {
        let t = Toolbar {
            title: vec![Tool::Find, Tool::Find],
            dock: vec![Tool::Find, Tool::Zoom],
            dock_align: DockAlign::End,
        }
        .normalized();
        assert_eq!(t.title, [Tool::Find, Tool::More]);
        assert_eq!(t.dock, [Tool::Zoom]);
    }

    #[test]
    fn the_add_menu_lists_what_isnt_there() {
        let t = Toolbar::default();
        let missing: Vec<Tool> = t.missing_from(Bar::Title).collect();
        assert!(missing.contains(&Tool::TwoPages));
        assert!(
            missing.contains(&Tool::Zoom),
            "it's in the dock, so it can move up"
        );
        assert!(!missing.contains(&Tool::Find));
    }

    #[test]
    fn unknown_buttons_are_skipped_not_fatal() {
        let t: Toolbar =
            serde_json::from_str(r#"{"title":["find","teleport","more"],"dock_align":"end"}"#)
                .unwrap();
        assert_eq!(t.title, [Tool::Find, Tool::More]);
        assert_eq!(t.dock_align, DockAlign::End);
    }

    #[test]
    fn it_reads_and_writes_as_words() {
        let json = serde_json::to_string(&Toolbar::default()).unwrap();
        assert!(json.contains("\"read-aloud\"") && json.contains("\"centre\""));
        let back: Toolbar = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Toolbar::default());
    }
}
