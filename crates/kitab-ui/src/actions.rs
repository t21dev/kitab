// SPDX-License-Identifier: AGPL-3.0-or-later

//! Every command is a GPUI action, bound in the default keymap (PLAN.md §3.10,
//! AGENTS.md rule 19). keymap.json overrides arrive with Phase 2.

use gpui::{App, KeyBinding, NoAction, actions};

actions!(
    kitab,
    [
        // Files and tabs
        OpenFile,
        CloseTab,
        NextTab,
        PrevTab,
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
        // Navigate
        NextPage,
        PrevPage,
        FirstPage,
        LastPage,
        ScrollDown,
        ScrollUp,
        ScreenDown,
        ScreenUp,
        GoToPage,
        HistoryBack,
        HistoryForward,
        // Select and copy
        Copy,
        ClearSelection,
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
        // App
        OpenSettings,
        ShowShortcuts,
        // PDF tools
        ExtractPages,
        ToggleSplit,
        CloseOverlay,
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
/// Settings and open menus: Escape closes them.
pub const OVERLAY: &str = "Overlay";

pub fn bind_default_keys(cx: &mut App) {
    let r = Some(READER);
    let w = Some(WORKSPACE);
    cx.bind_keys([
        // Files and tabs
        KeyBinding::new("ctrl-o", OpenFile, w),
        KeyBinding::new("ctrl-w", CloseTab, w),
        KeyBinding::new("ctrl-tab", NextTab, w),
        KeyBinding::new("ctrl-shift-tab", PrevTab, w),
        KeyBinding::new("ctrl-shift-h", ShowHome, w),
        KeyBinding::new("ctrl-b", ToggleSidebar, w),
        KeyBinding::new("f12", ToggleSidebar, w),
        KeyBinding::new("ctrl-shift-t", ReopenClosedTab, w),
        KeyBinding::new("ctrl-f4", CloseTab, w),
        KeyBinding::new("ctrl-pagedown", NextTab, w),
        KeyBinding::new("ctrl-pageup", PrevTab, w),
        // Alt+digit, because Ctrl+0/1/2 are SumatraPDF's zoom keys.
        KeyBinding::new("alt-1", SelectTab1, w),
        KeyBinding::new("alt-2", SelectTab2, w),
        KeyBinding::new("alt-3", SelectTab3, w),
        KeyBinding::new("alt-4", SelectTab4, w),
        KeyBinding::new("alt-5", SelectTab5, w),
        KeyBinding::new("alt-6", SelectTab6, w),
        KeyBinding::new("alt-7", SelectTab7, w),
        KeyBinding::new("alt-8", SelectTab8, w),
        KeyBinding::new("alt-9", SelectLastTab, w),
        KeyBinding::new("ctrl-,", OpenSettings, w),
        KeyBinding::new("ctrl-/", ShowShortcuts, w),
        KeyBinding::new("ctrl-shift-e", ExtractPages, w),
        KeyBinding::new("ctrl-\\", ToggleSplit, w),
        KeyBinding::new("escape", CloseOverlay, Some(OVERLAY)),
        KeyBinding::new("ctrl-shift-l", CycleTheme, w),
        // Navigate (SumatraPDF-compatible where Sumatra has a binding)
        KeyBinding::new("right", NextPage, r),
        KeyBinding::new("left", PrevPage, r),
        KeyBinding::new("n", NextPage, r),
        KeyBinding::new("p", PrevPage, r),
        KeyBinding::new("home", FirstPage, r),
        KeyBinding::new("end", LastPage, r),
        KeyBinding::new("j", ScrollDown, r),
        KeyBinding::new("k", ScrollUp, r),
        KeyBinding::new("down", ScrollDown, r),
        KeyBinding::new("up", ScrollUp, r),
        KeyBinding::new("pagedown", ScreenDown, r),
        KeyBinding::new("pageup", ScreenUp, r),
        KeyBinding::new("space", ScreenDown, r),
        KeyBinding::new("shift-space", ScreenUp, r),
        KeyBinding::new("ctrl-g", GoToPage, r),
        KeyBinding::new("alt-left", HistoryBack, r),
        KeyBinding::new("alt-right", HistoryForward, r),
        KeyBinding::new("enter", ConfirmPage, Some(PAGE_FIELD)),
        KeyBinding::new("escape", CancelPage, Some(PAGE_FIELD)),
        // Select and copy
        KeyBinding::new("ctrl-c", Copy, r),
        KeyBinding::new("escape", ClearSelection, r),
        // Find
        KeyBinding::new("ctrl-f", Find, r),
        KeyBinding::new("/", Find, r),
        KeyBinding::new("f3", FindNext, r),
        KeyBinding::new("shift-f3", FindPrev, r),
        KeyBinding::new("enter", FindNext, Some(FIND_BAR)),
        KeyBinding::new("shift-enter", FindPrev, Some(FIND_BAR)),
        KeyBinding::new("escape", CloseFind, Some(FIND_BAR)),
        KeyBinding::new("alt-c", ToggleMatchCase, Some(FIND_BAR)),
        KeyBinding::new("alt-w", ToggleWholeWord, Some(FIND_BAR)),
        // Zoom and view
        KeyBinding::new("ctrl-=", ZoomIn, r),
        KeyBinding::new("ctrl-+", ZoomIn, r),
        KeyBinding::new("ctrl--", ZoomOut, r),
        KeyBinding::new("ctrl-0", FitPage, r),
        KeyBinding::new("ctrl-1", ActualSize, r),
        KeyBinding::new("ctrl-2", FitWidth, r),
        KeyBinding::new("ctrl-shift--", RotateLeft, r),
        KeyBinding::new("ctrl-shift-=", RotateRight, r),
        // Password prompt
        KeyBinding::new("enter", SubmitPassword, Some(PROMPT)),
        KeyBinding::new("escape", DismissPrompt, Some(PROMPT)),
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
        .map(|key| KeyBinding::new(key, NoAction, typing)),
    );
}
