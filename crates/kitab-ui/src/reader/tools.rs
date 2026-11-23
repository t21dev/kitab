// SPDX-License-Identifier: AGPL-3.0-or-later

//! What each toolbar tool puts on a bar for this tab (GitHub #25): its
//! buttons, their tooltips, whether they're lit, and what a click does. The
//! title bar and the dock draw the same list (crate::toolbar), each in its
//! own style, from this.

use super::Reader;
use crate::actions::*;
use crate::toolbar::Tool;
use gpui::{Action, App};
use kitab_core::ZoomMode;

/// What a click on a button does.
pub enum Click {
    /// An action, dispatched to the tab (or to the window for the
    /// workspace's own: print, password, open, export, present).
    Act(Box<dyn Action>),
    /// Read aloud's panel, at the button.
    SpeechMenu,
    /// More's menu, at the button.
    MoreMenu,
}

/// One button (or Zoom's percentage, `icon` None) of a tool.
pub struct ToolButton {
    /// Unique within the bar: the tool and the button's place in it.
    pub id: gpui::SharedString,
    pub icon: Option<&'static str>,
    /// Zoom's percentage (or a book's text size) when `icon` is None.
    pub text: String,
    pub tip: String,
    pub on: bool,
    /// Shown greyed out: there's nothing to do (Back with no history).
    pub enabled: bool,
    pub click: Click,
}

impl Reader {
    /// The buttons `tool` puts on a bar for this document; none when it
    /// doesn't apply here (Password outside a PDF, Two pages in a book).
    pub fn tool_buttons(&self, tool: Tool, cx: &App) -> Vec<ToolButton> {
        let b = |k: usize, icon: &'static str, tip: &str, on: bool, click: Click| ToolButton {
            id: format!("tool-{tool:?}-{k}").into(),
            icon: Some(icon),
            text: String::new(),
            tip: tip.to_string(),
            on,
            enabled: true,
            click,
        };
        let act = |a: Box<dyn Action>| Click::Act(a);
        let mode = self.zoom_mode();
        let paged = !self.is_book() && !self.is_markdown();
        match tool {
            Tool::Sidebar => vec![b(
                0,
                "sidebar",
                "Sidebar  Ctrl+B",
                false,
                act(Box::new(ToggleSidebar)),
            )],
            Tool::History => vec![
                ToolButton {
                    enabled: self.can_go_back(),
                    ..b(
                        0,
                        "arrow-left",
                        "Back  Alt+Left",
                        false,
                        act(Box::new(HistoryBack)),
                    )
                },
                ToolButton {
                    enabled: self.can_go_forward(),
                    ..b(
                        1,
                        "arrow-right",
                        "Forward  Alt+Right",
                        false,
                        act(Box::new(HistoryForward)),
                    )
                },
            ],
            Tool::Find => vec![b(
                0,
                "search",
                "Find  Ctrl+F",
                self.find.open,
                act(Box::new(Find)),
            )],
            Tool::Bookmark if self.can_bookmark() => {
                let marked = self.is_bookmarked(self.current_page() as u32);
                let tip = if marked {
                    "Remove bookmark  Ctrl+D"
                } else {
                    "Bookmark this page  Ctrl+D"
                };
                vec![b(0, "bookmark", tip, marked, act(Box::new(ToggleBookmark)))]
            }
            Tool::Zoom => {
                // A book shows its text size: zoom there changes the text.
                let text = match self.book_text() {
                    Some(pt) => format!("{pt:.0} pt"),
                    None => format!("{} %", self.zoom_percent()),
                };
                vec![
                    b(
                        0,
                        "minus",
                        "Zoom out  Ctrl+−",
                        false,
                        act(Box::new(ZoomOut)),
                    ),
                    ToolButton {
                        icon: None,
                        text,
                        ..b(
                            1,
                            "plus",
                            "Actual size  Ctrl+1",
                            false,
                            act(Box::new(ActualSize)),
                        )
                    },
                    b(2, "plus", "Zoom in  Ctrl+=", false, act(Box::new(ZoomIn))),
                ]
            }
            Tool::Fit => vec![
                b(
                    0,
                    "fit-width",
                    "Fit width  Ctrl+2",
                    mode == ZoomMode::FitWidth,
                    act(Box::new(FitWidth)),
                ),
                b(
                    1,
                    "fit-page",
                    "Fit page  Ctrl+0",
                    mode == ZoomMode::FitPage,
                    act(Box::new(FitPage)),
                ),
            ],
            Tool::Hand if paged => vec![b(
                0,
                "hand",
                "Hand tool: drag to move the page (or hold Space)",
                self.hand_on(),
                act(Box::new(ToggleHand)),
            )],
            Tool::TwoPages if paged => vec![b(
                0,
                "pages",
                "Two pages  Ctrl+7",
                self.spread() == super::Spread::Two,
                act(Box::new(TwoPages)),
            )],
            Tool::BookView if paged => vec![b(
                0,
                "book",
                "Book view  Ctrl+8",
                self.spread() == super::Spread::Book,
                act(Box::new(BookView)),
            )],
            Tool::RotateLeft if !self.is_markdown() => {
                vec![b(
                    0,
                    "rotate-left",
                    "Rotate left  Ctrl+Shift+−",
                    false,
                    act(Box::new(RotateLeft)),
                )]
            }
            Tool::Rotate if !self.is_markdown() => {
                vec![b(
                    0,
                    "rotate",
                    "Rotate right  Ctrl+Shift+=",
                    false,
                    act(Box::new(RotateRight)),
                )]
            }
            Tool::Focus => vec![b(
                0,
                "focus",
                "Focus mode  F11",
                crate::style::Style::get(cx).minimal,
                act(Box::new(ToggleMinimal)),
            )],
            Tool::Markup if self.can_markup(cx) => {
                vec![b(
                    0,
                    "pen",
                    "Draw and mark up  Ctrl+Shift+D",
                    self.markup_on(),
                    act(Box::new(ToggleMarkup)),
                )]
            }
            Tool::ReadAloud if self.can_read_aloud() => {
                // The speaker opens read aloud's panel; Stop joins it while reading.
                let reading = self.reading_aloud();
                let glyph = if reading == Some(false) {
                    "pause"
                } else {
                    "speaker"
                };
                let mut v = vec![b(
                    0,
                    glyph,
                    "Read aloud  Ctrl+Shift+U",
                    reading.is_some(),
                    Click::SpeechMenu,
                )];
                if reading.is_some() {
                    v.push(b(
                        1,
                        "stop",
                        "Stop reading",
                        false,
                        act(Box::new(StopReading)),
                    ));
                }
                v
            }
            Tool::Present if self.can_present() => {
                vec![b(0, "play", "Present  F5", false, act(Box::new(Present)))]
            }
            Tool::Password if self.is_pdf() => {
                vec![b(0, "lock", "Password", false, act(Box::new(PdfPassword)))]
            }
            Tool::Open => vec![b(
                0,
                "folder-open",
                "Open file…  Ctrl+O",
                false,
                act(Box::new(OpenFile)),
            )],
            Tool::Print => vec![b(0, "print", "Print  Ctrl+P", false, act(Box::new(Print)))],
            Tool::Export if !self.is_markdown() => {
                vec![b(
                    0,
                    "download",
                    "Export as Markdown",
                    false,
                    act(Box::new(ExportMarkdown)),
                )]
            }
            Tool::Export => vec![b(
                0,
                "download",
                "Save as PDF",
                false,
                act(Box::new(SaveAsPdf)),
            )],
            Tool::More => vec![b(0, "more-vertical", "More", false, Click::MoreMenu)],
            _ => Vec::new(),
        }
    }
}
