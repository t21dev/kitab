// SPDX-License-Identifier: AGPL-3.0-or-later

//! Kitab Reader's GPUI views. The binary (`crates/kitab`) owns start-up and
//! hands in a document engine; nothing here touches MuPDF, the disk or SQLite
//! on the UI thread (AGENTS.md rule 2).

pub mod actions;
pub mod autoscroll;
pub mod cache;
pub mod edit_buffer;
pub mod engine;
pub mod icons;
pub mod kbd;
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

/// Windows: the exact place to put the window back, applied just before
/// it's first shown (after GPUI's own placement).
struct PendingPlace(Option<([i32; 4], bool)>);
impl gpui::Global for PendingPlace {}
impl gpui::Global for Unrevealed {}

/// How long a slow first page may keep the window hidden.
const REVEAL_AT_LATEST: std::time::Duration = std::time::Duration::from_millis(700);

pub(crate) fn hwnd_of(window: &gpui::Window) -> Option<isize> {
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
        // The saved place first, then show it a frame later at that size.
        let pending = cx.try_global::<PendingPlace>().and_then(|p| p.0);
        if let (Some((rect, maximized)), Some(hwnd)) = (pending, hwnd_of(window)) {
            cx.set_global(PendingPlace(None));
            if kitab_os::set_window_placement(hwnd, rect, maximized) {
                window.on_next_frame(reveal);
                window.refresh();
                return;
            }
        }
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
    if cx.has_global::<FirstPaint>()
        && let Some(f) = cx.global_mut::<FirstPaint>().0.take()
    {
        kitab_os::trace::mark("first paint: frame requested");
        window.on_next_frame(move |_, _| f());
    }
}
pub use icons::Assets;
pub use library::{
    CollectionInfo, Library, LibraryHit, NoLibrary, RecentItem, SessionTab, ShelfKind, StatsView,
};
pub use settings::Settings;
pub use style::Style;

/// The saved window place as GPUI bounds, if enough of it (a strip of the
/// title bar) is on a screen that's connected now: a monitor that was
/// unplugged mustn't leave the window off screen.
fn saved_bounds(
    p: settings::WindowPlace,
    cx: &App,
) -> Option<(WindowBounds, Option<gpui::DisplayId>)> {
    if !(p.x.is_finite() && p.y.is_finite() && p.w >= 320.0 && p.h >= 240.0) {
        return None;
    }
    let b = Bounds::new(gpui::point(px(p.x), px(p.y)), size(px(p.w), px(p.h)));
    let title = Bounds::new(b.origin, size(b.size.width, px(40.0)));
    // The screen it's on: GPUI opens the window on that display (it puts
    // bounds outside the display it was given back on the primary one).
    // The monitor under its centre (as GPUI decides), else one showing
    // enough of its title bar to grab.
    let displays = cx.displays();
    let display = displays
        .iter()
        .find(|d| d.bounds().contains(&b.center()))
        .or_else(|| {
            displays.iter().find(|d| {
                let o = d.bounds().intersect(&title);
                f32::from(o.size.width) >= 120.0 && f32::from(o.size.height) >= 20.0
            })
        })?;
    let bounds = if p.maximized {
        WindowBounds::Maximized(b)
    } else {
        WindowBounds::Windowed(b)
    };
    Some((bounds, Some(display.id())))
}

/// The window's place to save: its normal bounds (also while maximised or
/// full screen) and whether it's maximised.
pub(crate) fn window_place(window: &gpui::Window) -> settings::WindowPlace {
    let (b, maximized) = match window.window_bounds() {
        WindowBounds::Windowed(b) | WindowBounds::Fullscreen(b) => (b, false),
        WindowBounds::Maximized(b) => (b, true),
    };
    settings::WindowPlace {
        x: f32::from(b.origin.x),
        y: f32::from(b.origin.y),
        w: f32::from(b.size.width),
        h: f32::from(b.size.height),
        maximized,
        raw: hwnd_of(window)
            .and_then(kitab_os::window_placement)
            .map(|(r, _)| r),
    }
}

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
    // The interface size, before any window exists (gpui patch 10).
    gpui::kitab_set_ui_scale(f32::from(settings.ui_scale) / 100.0);
    cx.set_global(Style::from_settings(theme, &settings));
    cx.set_global(shell::SettingsSink(save_settings));
    actions::bind_default_keys(cx);
    shell::init_native_menu(cx);
    // Where the window was when Kitab last closed, if that place is still on
    // a screen; otherwise the default size, centred.
    let (bounds, display_id) = settings
        .window
        .and_then(|p| saved_bounds(p, cx))
        .unwrap_or_else(|| {
            (
                WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(layout::WINDOW_W), px(layout::WINDOW_H)),
                    cx,
                )),
                None,
            )
        });
    let options = WindowOptions {
        window_bounds: Some(bounds),
        display_id,
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
    cx.set_global(PendingPlace(
        settings
            .window
            .and_then(|p| p.raw.map(|r| (r, p.maximized))),
    ));
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
