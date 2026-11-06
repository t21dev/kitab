// SPDX-License-Identifier: AGPL-3.0-or-later

//! Kitab Reader's GPUI views. The binary (`crates/kitab`) owns start-up and
//! hands in a document engine; nothing here touches MuPDF, the disk or SQLite
//! on the UI thread (AGENTS.md rule 2).

pub mod actions;
pub mod cache;
pub mod engine;
pub mod icons;
pub mod library;
pub mod menu;
pub mod motion;
pub mod reader;
pub mod settings;
pub mod settings_view;
pub mod shell;
pub mod shortcuts;
pub mod style;
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
pub struct FirstPaint(pub Option<Box<dyn FnOnce()>>);
impl gpui::Global for FirstPaint {}

pub(crate) fn fire_first_paint(window: &mut gpui::Window, cx: &mut gpui::App) {
    if cx.has_global::<FirstPaint>() {
        if let Some(f) = cx.global_mut::<FirstPaint>().0.take() {
            window.on_next_frame(move |_, _| f());
        }
    }
}
pub use icons::Assets;
pub use library::{Library, NoLibrary, RecentItem, SessionTab};
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
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        cx.new(|cx| shell::Workspace::new(engine, library, files, window, cx))
    })
    .is_ok()
}
