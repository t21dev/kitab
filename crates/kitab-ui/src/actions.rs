// SPDX-License-Identifier: AGPL-3.0-or-later

//! Every command is a GPUI action, bound in the default keymap (PLAN.md §3.10,
//! AGENTS.md rule 19), and listed in the command palette (`palette.rs`).
//! keymap.json adds to and overrides them (`keymap.rs`).

use gpui::{Action, App, KeyBinding, NoAction, actions};

actions!(
    kitab,
    [
        // Files and tabs
        OpenFile,
        // Make a PDF from pictures (one page each).
        ImagesToPdf,
        CloseTab,
        NextTab,
        PrevTab,
        MoveTabLeft,
        PinTab,
        MoveTabRight,
        ShowHome,
        ReopenClosedTab,
        SelectTab1,
        SelectTab2,
        SelectTab3,
        SelectTab4,
        SelectTab5,
        SelectTab6,
        SelectTab7,
        SelectTab8,
        SelectLastTab,
        // Del, twice: the file to the Recycle Bin / Trash, and on to the
        // next one in its folder (GitHub #17).
        MoveToTrash,
        // The same from the command palette and File menu, asked in a dialog.
        MoveToTrashAsk,
        // Navigate
        NextPage,
        PrevPage,
        // The arrow keys: next/previous, swapped in a right-to-left book.
        PageRight,
        PageLeft,
        // Markup
        ToggleMarkup,
        HighlightSelection,
        ToggleBookmark,
        // Read aloud: start, pause and carry on; stop.
        ReadAloud,
        StopReading,
        ToggleDock,
        // Markdown's typeface: System, Serif, Mono, round.
        CycleMdFace,
        ToggleFileInfo,
        MarkupUndo,
        MarkupRedo,
        SaveMarks,
        SaveCopyWithMarks,
        SinglePage,
        TwoPages,
        BookView,
        FirstPage,
        LastPage,
        ScrollDown,
        ScrollUp,
        ScreenDown,
        ScreenUp,
        // Space: a tap moves down a screen (on release); held, dragging
        // moves the page, as in Acrobat (issue #14).
        SpaceHand,
        // The hand tool: dragging moves the page instead of selecting.
        ToggleHand,
        GoToPage,
        HistoryBack,
        HistoryForward,
        // Select and copy
        Copy,
        ClearSelection,
        // Markdown
        ToggleEdit,
        // Find
        Find,
        FindNext,
        FindPrev,
        CloseFind,
        ToggleMatchCase,
        ToggleWholeWord,
        // Zoom and view
        ZoomIn,
        ZoomOut,
        FitPage,
        FitWidth,
        ActualSize,
        RotateLeft,
        RotateRight,
        // Read
        ToggleSidebar,
        CycleTheme,
        ToggleMinimal,
        // App
        OpenSettings,
        ShowShortcuts,
        CommandPalette,
        CheckForUpdates,
        OpenKeymap,
        OpenGuide,
        OpenRepo,
        ReportIssue,
        // Presentation mode
        Present,
        // Printing
        Print,
        // A PDF's password
        PdfPassword,
        // Signing
        SignPdf,
        // Page tools
        PageTools,
        // PDF tools
        ExtractPages,
        // Make a smaller copy of a PDF (pictures scaled down).
        CompressPdf,
        // The document's content as Markdown (pictures beside it) or text.
        ExportMarkdown,
        ExportText,
        // A Markdown document as a PDF (laid out on A4 as for printing).
        SaveAsPdf,
        ToggleSplit,
        CloseOverlay,
        // Keyboard focus: the next or previous button or control
        FocusNext,
        FocusPrev,
        // Page field
        ConfirmPage,
        CancelPage,
        // Password prompt
        SubmitPassword,
        DismissPrompt,
    ]
);

/// Context names used in key bindings.
pub const READER: &str = "Reader";
pub const WORKSPACE: &str = "Workspace";
pub const PROMPT: &str = "PasswordPrompt";
pub const PAGE_FIELD: &str = "PageField";
pub const FIND_BAR: &str = "FindBar";
/// The Markdown editor: typing keys are text here, not reader shortcuts.
pub const MD_EDITOR: &str = "MdEditor";
/// The markup colour picker's hex field.
pub const MARKUP_HEX: &str = "MarkupHex";
/// Settings and open menus: Escape closes them.
pub const OVERLAY: &str = "Overlay";

/// A default binding, with the Mac's keys on a Mac: Cmd where Windows and
/// Linux use Ctrl ("ctrl-o" is Cmd+O). Tab switching keeps Ctrl, as in
/// Safari and Finder.
fn key<A: Action>(keys: &str, action: A, context: Option<&str>) -> KeyBinding {
    if cfg!(target_os = "macos") {
        KeyBinding::new(&crate::shortcuts::mac_keys(keys), action, context)
    } else {
        KeyBinding::new(keys, action, context)
    }
}

pub fn bind_default_keys(cx: &mut App) {
    let r = Some(READER);
    let w = Some(WORKSPACE);
    cx.bind_keys([
        // Files and tabs
        key("ctrl-o", OpenFile, w),
        key("ctrl-w", CloseTab, w),
        key("ctrl-tab", NextTab, w),
        key("ctrl-shift-tab", PrevTab, w),
        key("ctrl-shift-h", ShowHome, w),
        key("ctrl-b", ToggleSidebar, w),
        key("ctrl-.", ToggleDock, w),
        key("ctrl-i", ToggleFileInfo, w),
        key("f12", ToggleSidebar, w),
        key("ctrl-shift-t", ReopenClosedTab, w),
        key("ctrl-f4", CloseTab, w),
        key("ctrl-pagedown", NextTab, w),
        key("ctrl-pageup", PrevTab, w),
        key("ctrl-shift-pageup", MoveTabLeft, w),
        key("ctrl-shift-pagedown", MoveTabRight, w),
        // Alt+digit, because Ctrl+0/1/2 are SumatraPDF's zoom keys.
        key("alt-1", SelectTab1, w),
        key("alt-2", SelectTab2, w),
        key("alt-3", SelectTab3, w),
        key("alt-4", SelectTab4, w),
        key("alt-5", SelectTab5, w),
        key("alt-6", SelectTab6, w),
        key("alt-7", SelectTab7, w),
        key("alt-8", SelectTab8, w),
        key("alt-9", SelectLastTab, w),
        key("ctrl-,", OpenSettings, w),
        key("ctrl-/", ShowShortcuts, w),
        key("ctrl-k", CommandPalette, w),
        key("ctrl-shift-p", CommandPalette, w),
        key("ctrl-shift-e", ExtractPages, w),
        key("ctrl-\\", ToggleSplit, w),
        key("escape", CloseOverlay, Some(OVERLAY)),
        key("tab", FocusNext, w),
        key("shift-tab", FocusPrev, w),
        key("ctrl-shift-l", CycleTheme, w),
        key("f11", ToggleMinimal, w),
        key("f5", Present, w),
        key("ctrl-p", Print, w),
        // Navigate (SumatraPDF-compatible where Sumatra has a binding)
        key("ctrl-shift-d", ToggleMarkup, r),
        key("h", HighlightSelection, r),
        key("ctrl-d", ToggleBookmark, r),
        key("ctrl-shift-u", ReadAloud, r),
        key("ctrl-z", MarkupUndo, r),
        key("ctrl-y", MarkupRedo, r),
        key("ctrl-shift-z", MarkupRedo, r),
        key("ctrl-s", SaveMarks, r),
        key("delete", MoveToTrash, r),
        key("right", PageRight, r),
        key("left", PageLeft, r),
        key("n", NextPage, r),
        key("p", PrevPage, r),
        key("home", FirstPage, r),
        key("end", LastPage, r),
        key("j", ScrollDown, r),
        key("k", ScrollUp, r),
        key("down", ScrollDown, r),
        key("up", ScrollUp, r),
        key("pagedown", ScreenDown, r),
        key("pageup", ScreenUp, r),
        key("space", SpaceHand, r),
        key("shift-space", ScreenUp, r),
        key("ctrl-g", GoToPage, r),
        key("alt-left", HistoryBack, r),
        key("alt-right", HistoryForward, r),
        key("enter", ConfirmPage, Some(PAGE_FIELD)),
        key("escape", CancelPage, Some(PAGE_FIELD)),
        // Markdown
        key("ctrl-e", ToggleEdit, r),
        // Select and copy
        key("ctrl-c", Copy, r),
        key("escape", ClearSelection, r),
        // Find
        key("ctrl-f", Find, r),
        key("/", Find, r),
        key("f3", FindNext, r),
        key("shift-f3", FindPrev, r),
        key("enter", FindNext, Some(FIND_BAR)),
        key("shift-enter", FindPrev, Some(FIND_BAR)),
        key("escape", CloseFind, Some(FIND_BAR)),
        key("alt-c", ToggleMatchCase, Some(FIND_BAR)),
        key("alt-w", ToggleWholeWord, Some(FIND_BAR)),
        // Zoom and view
        key("ctrl-=", ZoomIn, r),
        key("ctrl-+", ZoomIn, r),
        key("ctrl--", ZoomOut, r),
        key("ctrl-0", FitPage, r),
        key("ctrl-1", ActualSize, r),
        key("ctrl-2", FitWidth, r),
        key("ctrl-6", SinglePage, r),
        key("ctrl-7", TwoPages, r),
        key("ctrl-8", BookView, r),
        key("ctrl-shift--", RotateLeft, r),
        key("ctrl-shift-=", RotateRight, r),
        // Password prompt
        key("enter", SubmitPassword, Some(PROMPT)),
        key("escape", DismissPrompt, Some(PROMPT)),
    ]);
    // Inside the find field these keys type (or move nothing): unbind the
    // reader's page keys there.
    let typing = Some(FIND_BAR);
    cx.bind_keys(
        [
            "n",
            "p",
            "j",
            "k",
            "/",
            "space",
            "shift-space",
            "left",
            "right",
            "home",
            "end",
        ]
        .map(|k| key(k, NoAction, typing)),
    );
    // In the Markdown editor the reader's page keys move the caret instead,
    // Ctrl+C copies the editor's selection and Esc ends editing.
    let editor = Some(MD_EDITOR);
    cx.bind_keys(
        [
            "n",
            "p",
            "j",
            "k",
            "/",
            "space",
            "shift-space",
            "left",
            "right",
            "up",
            "down",
            "home",
            "end",
            "pageup",
            "pagedown",
            "ctrl-c",
            "escape",
            "alt-left",
            "alt-right",
            "ctrl-g",
            // The editor's own undo, redo and save, not the markup's.
            "ctrl-z",
            "ctrl-y",
            "ctrl-shift-z",
            "ctrl-s",
            "ctrl-shift-d",
            "tab",
            "shift-tab",
        ]
        .map(|k| key(k, NoAction, editor)),
    );
    // Typing a hex colour: every key goes to the field, none to the reader.
    let hex = Some(MARKUP_HEX);
    let mut keys: Vec<String> = ('a'..='z').chain('0'..='9').map(String::from).collect();
    keys.extend(('a'..='f').map(|c| format!("shift-{c}")));
    keys.extend(
        [
            "space",
            "backspace",
            // Typing, Del deletes a letter, not the file.
            "delete",
            "enter",
            "escape",
            "left",
            "right",
            "home",
            "end",
            "/",
            "-",
            "ctrl-z",
            "ctrl-y",
            "ctrl-shift-z",
            "ctrl-s",
            "ctrl-shift-d",
            "ctrl-c",
            "ctrl-f",
        ]
        .map(String::from),
    );
    cx.bind_keys(keys.iter().map(|k| key(k, NoAction, hex)));
    // The find field and the Markdown editor type every key too (their own
    // lists above missed "h", which highlighted instead of typing, #16).
    // Enter, Escape and Ctrl+F keep their find and editor actions.
    cx.bind_keys(
        keys.iter()
            .filter(|k| !matches!(k.as_str(), "enter" | "escape" | "ctrl-f"))
            .map(|k| key(k, NoAction, Some(FIND_BAR))),
    );
    cx.bind_keys(
        keys.iter()
            .filter(|k| !matches!(k.as_str(), "ctrl-f"))
            .map(|k| key(k, NoAction, Some(MD_EDITOR))),
    );
    // A text field (notes, text boxes) takes every key it types with.
    let field = Some(crate::text_field::TEXT_FIELD);
    cx.bind_keys(keys.iter().map(|k| key(k, NoAction, field)));
    // A form's text box: Tab goes to the form's next field, not the next control.
    let form = Some(crate::text_field::TAB_OUT);
    cx.bind_keys(["tab", "shift-tab"].map(|k| key(k, NoAction, form)));
    // The password prompt too: plain letters are reader shortcuts (j, k, n,
    // p, h), which ate them. Enter and Escape keep their prompt actions.
    cx.bind_keys(
        keys.iter()
            .filter(|k| !matches!(k.as_str(), "enter" | "escape"))
            .map(|k| key(k, NoAction, Some(PROMPT))),
    );
    cx.bind_keys(
        [
            "up",
            "down",
            "pageup",
            "pagedown",
            "delete",
            "ctrl-a",
            "ctrl-x",
            "ctrl-v",
            "ctrl-enter",
        ]
        .map(|k| key(k, NoAction, field)),
    );
}
