// SPDX-License-Identifier: AGPL-3.0-or-later

//! Every command is a GPUI action, bound in the default keymap (PLAN.md §3.10,
//! AGENTS.md rule 19). keymap.json overrides arrive with Phase 2.

use gpui::{App, KeyBinding, actions};

actions!(
    kitab,
    [
        // Files and tabs
        OpenFile,
        CloseTab,
        NextTab,
        PrevTab,
        ShowHome,
        // Navigate
        NextPage,
        PrevPage,
        FirstPage,
        LastPage,
        ScrollDown,
        ScrollUp,
        ScreenDown,
        ScreenUp,
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
        // Password prompt
        SubmitPassword,
        DismissPrompt,
    ]
);

/// Context names used in key bindings.
pub const READER: &str = "Reader";
pub const WORKSPACE: &str = "Workspace";
pub const PROMPT: &str = "PasswordPrompt";

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
}
