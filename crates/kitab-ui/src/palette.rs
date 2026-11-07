// SPDX-License-Identifier: AGPL-3.0-or-later

//! The command palette's commands (Ctrl+K / Ctrl+Shift+P, PLAN.md §3.10):
//! every action a reader would look for, by name, with its group, and a
//! small fuzzy matcher. The view is in `shell/palette_ui.rs`.

use crate::actions::*;
use gpui::Action;

/// One command: what it's called, its group, and the action it runs.
pub struct Command {
    pub label: &'static str,
    pub group: &'static str,
    /// Other words that find it ("open" finds Open a file, "print" finds
    /// nothing yet).
    pub keywords: &'static str,
    /// Only with a document open.
    pub doc: bool,
    pub action: fn() -> Box<dyn Action>,
}

macro_rules! cmd {
    ($group:expr, $label:expr, $kw:expr, $doc:expr, $action:expr) => {
        Command {
            label: $label,
            group: $group,
            keywords: $kw,
            doc: $doc,
            action: || Box::new($action),
        }
    };
}

pub const COMMANDS: &[Command] = &[
    // Files and tabs
    cmd!(
        "File",
        "Open a file…",
        "open browse pdf epub",
        false,
        OpenFile
    ),
    cmd!("File", "Close tab", "close", true, CloseTab),
    cmd!(
        "File",
        "Reopen closed tab",
        "undo close restore",
        false,
        ReopenClosedTab
    ),
    cmd!("File", "Next tab", "switch", false, NextTab),
    cmd!("File", "Previous tab", "switch", false, PrevTab),
    cmd!("File", "Home", "recent library start", false, ShowHome),
    cmd!(
        "File",
        "Save pages as a new PDF…",
        "extract export split",
        true,
        ExtractPages
    ),
    cmd!(
        "File",
        "Split view with the next tab",
        "side by side compare",
        true,
        ToggleSplit
    ),
    // Navigate
    cmd!("Go", "Go to page…", "jump number", true, GoToPage),
    cmd!("Go", "Next page", "forward down", true, NextPage),
    cmd!("Go", "Previous page", "back up", true, PrevPage),
    cmd!("Go", "First page", "start beginning top", true, FirstPage),
    cmd!("Go", "Last page", "end bottom", true, LastPage),
    cmd!("Go", "Back", "history previous place", true, HistoryBack),
    cmd!("Go", "Forward", "history next place", true, HistoryForward),
    // Find
    cmd!("Find", "Find in document", "search look", true, Find),
    cmd!("Find", "Next result", "search", true, FindNext),
    cmd!("Find", "Previous result", "search", true, FindPrev),
    // View
    cmd!("View", "Zoom in", "bigger larger magnify", true, ZoomIn),
    cmd!("View", "Zoom out", "smaller", true, ZoomOut),
    cmd!("View", "Fit page", "whole", true, FitPage),
    cmd!("View", "Fit width", "wide", true, FitWidth),
    cmd!("View", "Actual size", "100 one", true, ActualSize),
    cmd!("View", "Rotate clockwise", "turn right", true, RotateRight),
    cmd!(
        "View",
        "Rotate counter-clockwise",
        "turn left anticlockwise",
        true,
        RotateLeft
    ),
    cmd!(
        "View",
        "Sidebar",
        "contents outline toc pages panel",
        false,
        ToggleSidebar
    ),
    cmd!("View", "Hide or show the dock", "toolbar", true, ToggleDock),
    cmd!(
        "View",
        "Fold or unfold the file info",
        "size details chip",
        true,
        ToggleFileInfo
    ),
    cmd!(
        "View",
        "Minimal mode",
        "focus distraction full screen",
        false,
        ToggleMinimal
    ),
    cmd!(
        "View",
        "Next theme",
        "dark light colour color appearance",
        false,
        CycleTheme
    ),
    cmd!(
        "View",
        "Presentation mode",
        "slides present full screen slideshow",
        true,
        Present
    ),
    // Markup
    cmd!(
        "Mark up",
        "Draw and mark up",
        "pen annotate ink shapes markup",
        true,
        ToggleMarkup
    ),
    cmd!(
        "Mark up",
        "Highlight the selected text",
        "marker",
        true,
        HighlightSelection
    ),
    cmd!(
        "Mark up",
        "Bookmark this page",
        "favourite favorite remember",
        true,
        ToggleBookmark
    ),
    cmd!("Mark up", "Undo a mark", "", true, MarkupUndo),
    cmd!("Mark up", "Redo a mark", "", true, MarkupRedo),
    cmd!(
        "Mark up",
        "Save marks into the PDF",
        "write annotations",
        true,
        SaveMarks
    ),
    cmd!(
        "Markdown",
        "Edit beside the preview",
        "markdown editor write",
        true,
        ToggleEdit
    ),
    // App
    cmd!(
        "Kitab",
        "Settings",
        "preferences options",
        false,
        OpenSettings
    ),
    cmd!(
        "Kitab",
        "Keyboard shortcuts",
        "keys cheat sheet bindings",
        false,
        ShowShortcuts
    ),
    cmd!(
        "Kitab",
        "Edit keymap.json",
        "keys bindings rebind customise customize",
        false,
        OpenKeymap
    ),
    cmd!(
        "Kitab",
        "Check for updates",
        "version new release",
        false,
        CheckForUpdates
    ),
];

/// How well `query` matches `text`: `None` when some query letter is
/// missing, else higher is better. Letters must appear in order; a run of
/// letters, a word start and an early start all score more.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let mut qi = 0;
    let mut total = 0;
    let mut last: Option<usize> = None;
    for (i, &c) in t.iter().enumerate() {
        if qi < q.len() && c == q[qi] {
            let word_start = i == 0 || !t[i - 1].is_alphanumeric();
            total += 1;
            if word_start {
                total += 8;
            }
            if last.is_some_and(|l| l + 1 == i) {
                total += 5;
            }
            if i < 3 {
                total += 3;
            }
            last = Some(i);
            qi += 1;
        }
    }
    (qi == q.len()).then_some(total - (t.len() as i32) / 8)
}

/// The commands for `query`, best first. `has_doc` hides commands that need
/// a document when none is open. Indexes into COMMANDS.
pub fn matches(query: &str, has_doc: bool) -> Vec<usize> {
    let mut hits: Vec<(i32, usize)> = COMMANDS
        .iter()
        .enumerate()
        .filter(|(_, c)| has_doc || !c.doc)
        .filter_map(|(i, c)| {
            let by_label = score(query, c.label);
            // Keywords find a command, but rank below a label match.
            let by_word = c
                .keywords
                .split_whitespace()
                .filter_map(|w| score(query, w))
                .max()
                .map(|s| s - 20);
            let best = by_label.max(by_word)?;
            Some((best, i))
        })
        .collect();
    // Stable: equal scores keep the list's order (grouped, common first).
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.into_iter().map(|(_, i)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(query: &str, doc: bool) -> Vec<&'static str> {
        matches(query, doc)
            .into_iter()
            .map(|i| COMMANDS[i].label)
            .collect()
    }

    #[test]
    fn letters_in_order_match_and_word_starts_win() {
        assert!(score("zi", "Zoom in").is_some());
        assert!(score("iz", "Zoom in").is_none());
        assert!(score("zi", "Zoom in") > score("zi", "Fit size"));
        assert_eq!(labels("zoom in", true)[0], "Zoom in");
        assert_eq!(labels("gtp", true)[0], "Go to page…");
    }

    #[test]
    fn keywords_find_commands() {
        assert!(labels("toc", true).contains(&"Sidebar"));
        assert!(labels("dark", false).contains(&"Next theme"));
    }

    #[test]
    fn home_hides_document_commands() {
        let home = labels("", false);
        assert!(home.contains(&"Open a file…"));
        assert!(!home.contains(&"Zoom in"));
        assert_eq!(labels("", true).len(), COMMANDS.len());
    }

    #[test]
    fn every_command_builds_its_action() {
        for c in COMMANDS {
            let a = (c.action)();
            assert!(!a.name().is_empty(), "{}", c.label);
        }
    }
}
