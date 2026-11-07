// SPDX-License-Identifier: AGPL-3.0-or-later

//! Kitab Reader's GPUI views. The binary (`crates/kitab`) owns start-up and
//! hands in a document engine; nothing here touches MuPDF, the disk or SQLite
//! on the UI thread (AGENTS.md rule 2).

pub mod actions;
pub mod cache;
pub mod edit_buffer;
pub mod engine;
pub mod icons;
pub mod keymap;
pub mod library;
pub mod menu;
pub mod motion;
pub mod palette;
pub mod reader;
pub mod settings;
pub mod settings_view;
pub mod shell;
pub mod shortcuts;
pub mod style;
pub mod text_field;
pub mod tooltip;

use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowOptions, px, size,
};
use std::path::PathBuf;
use std::sync::Arc;

pub use engine::{DocEngine, DocHandle};

/// Window and layout sizes the first frame (kitab-os splash) must match exactly.
pub mod layout {
    pub const WINDOW_W: f32 = 1180.0;
    pub const WINDOW_H: f32 = 860.0;
    pub const SIDEBAR_W: f32 = 260.0;
    /// Gap around and between page sheets (DESIGN.md: 22 px).
    pub const PAGE_GAP: f32 = 22.0;
}

/// Runs once, on the frame after GPUI first shows a page or a placeholder: the
/// app uses it to close the GDI first frame (ADR 0002).
/// The main window, so files handed over by a second launch open in it.
struct MainWindow(gpui::WindowHandle<shell::Workspace>);
impl gpui::Global for MainWindow {}

/// Open `paths` as tabs in the main window and bring it to the front (a
/// second Kitab handed them over). An empty list only brings it forward.
pub fn open_paths(cx: &mut App, paths: Vec<PathBuf>) {
    let Some(handle) = cx.try_global::<MainWindow>().map(|w| w.0) else {
        return;
    };
    let _ = handle.update(cx, |ws, window, cx| {
        for p in paths {
            ws.open_tab(p, window, cx);
        }
        window.activate_window();
    });
}

pub struct FirstPaint(pub Option<Box<dyn FnOnce()>>);
impl gpui::Global for FirstPaint {}

/// The main window opens cloaked (Windows: shown and drawing, but kept off
/// the screen by DWM) and appears once its first page (or Home) is drawn,
/// so nothing half-built ever shows: no empty frame, no stand-in window.
/// `true` until it has appeared.
struct Unrevealed(bool);
impl gpui::Global for Unrevealed {}

/// How long a slow first page may keep the window hidden.
const REVEAL_AT_LATEST: std::time::Duration = std::time::Duration::from_millis(700);

fn hwnd_of(window: &gpui::Window) -> Option<isize> {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(RawWindowHandle::Win32(h)) =
            HasWindowHandle::window_handle(window).map(|h| h.as_raw())
        {
            return Some(h.hwnd.get());
        }
    }
    let _ = window;
    None
}

fn reveal(window: &mut gpui::Window, cx: &mut gpui::App) {
    if cx.try_global::<Unrevealed>().is_some_and(|u| u.0) {
        cx.set_global(Unrevealed(false));
        motion::set_on_screen();
        kitab_os::trace::mark("window shown");
        if let Some(hwnd) = hwnd_of(window) {
            kitab_os::cloak_window(hwnd, false);
        }
    }
}

pub(crate) fn fire_first_paint(window: &mut gpui::Window, cx: &mut gpui::App) {
    // The frame being built has the page: show the window once it's drawn.
    if cx.try_global::<Unrevealed>().is_some_and(|u| u.0) {
        window.on_next_frame(reveal);
    }
    if cx.has_global::<FirstPaint>() {
        if let Some(f) = cx.global_mut::<FirstPaint>().0.take() {
            kitab_os::trace::mark("first paint: frame requested");
            window.on_next_frame(move |_, _| f());
        }
    }
}
pub use icons::Assets;
pub use library::{
    CollectionInfo, Library, LibraryHit, NoLibrary, RecentItem, SessionTab, ShelfKind, StatsView,
};
pub use settings::Settings;
pub use style::Style;

/// Register globals and key bindings, then open the main window on `files`.
/// `theme` is the one `settings.theme` names; `save_settings` writes settings.json.
/// Returns false if the window could not be created.
pub fn open_main_window(
    cx: &mut App,
    engine: Arc<dyn DocEngine>,
    library: Arc<dyn Library>,
    theme: kitab_theme::Theme,
    settings: Settings,
    save_settings: Arc<dyn Fn(String) + Send + Sync>,
    files: Vec<PathBuf>,
) -> bool {
    cx.set_global(Style::from_settings(theme, &settings));
    cx.set_global(shell::SettingsSink(save_settings));
    actions::bind_default_keys(cx);
    let bounds = Bounds::centered(None, size(px(layout::WINDOW_W), px(layout::WINDOW_H)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Kitab".into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        window_background: WindowBackgroundAppearance::Opaque,
        app_id: Some("dev.t21.kitab".into()),
        // Small enough for half a laptop screen; below it the title bar,
        // Home's header and Settings can't fit their controls.
        window_min_size: Some(gpui::size(px(640.0), px(460.0))),
        // Shown below, cloaked; `reveal` uncloaks it once the first page is
        // drawn (or after REVEAL_AT_LATEST), so it never appears empty.
        show: false,
        ..Default::default()
    };
    cx.set_global(Unrevealed(true));
    match cx.open_window(options, |window, cx| {
        cx.new(|cx| shell::Workspace::new(engine, library, files, window, cx))
    }) {
        Ok(handle) => {
            cx.set_global(MainWindow(handle));
            // Cloak, then show: it draws while off screen. Where there's no
            // cloaking (macOS, Linux) it simply shows now.
            let _ = handle.update(cx, |_, window, cx| {
                match hwnd_of(window) {
                    Some(hwnd) => kitab_os::cloak_window(hwnd, true),
                    None => {
                        cx.set_global(Unrevealed(false));
                        motion::set_on_screen();
                    }
                }
                window.activate_window();
            });
            // A slow first page doesn't keep the window hidden for long.
            cx.spawn(async move |cx| {
                cx.background_executor().timer(REVEAL_AT_LATEST).await;
                let _ = handle.update(cx, |_, window, cx| reveal(window, cx));
            })
            .detach();
            true
        }
        Err(_) => false,
    }
}
