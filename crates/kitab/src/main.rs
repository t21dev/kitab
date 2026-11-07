// SPDX-License-Identifier: AGPL-3.0-or-later

//! Kitab Reader entry point. The start-up path is sacred (AGENTS.md rule 1):
//! with a file to open, the first frame (page 1, painted with GDI) is up before
//! GPUI has created its GPU device; GPUI then takes over.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adapters;
mod print_cli;

use gpui::{AppContext, Application};
use kitab_ui::{DocEngine, FirstPaint, Library};
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    // Print without a window (Explorer's Print verb, PLAN.md §3.8).
    if std::env::args_os().nth(1).is_some_and(|a| a == "--print") {
        let args: Option<Vec<String>> = std::env::args_os()
            .skip(2)
            .map(|a| a.into_string().ok())
            .collect();
        std::process::exit(args.map_or(1, |a| print_cli::run(&a)));
    }
    // File-manager thumbnails (PLAN.md §3.9): a separate short-lived process
    // the shell starts; no settings, no instance check, no window.
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == "--thumbnail")
    {
        // `args()` would panic on a path that isn't Unicode; refuse it instead.
        let args: Option<Vec<String>> = std::env::args_os()
            .skip(2)
            .map(|a| a.into_string().ok())
            .collect();
        let code = match args {
            Some(args) => kitab_thumb::cli(
                &args,
                &mut std::io::stdin().lock(),
                &mut std::io::stdout().lock(),
            ),
            None => 1,
        };
        std::process::exit(code);
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for arg in std::env::args_os().skip(1) {
        let s = arg.to_string_lossy();
        if s == "--version" {
            println!("Kitab Reader {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        if !s.starts_with("--") {
            files.push(PathBuf::from(arg));
        }
    }

    // settings.json is read before the first frame because the theme and density
    // are needed to draw it (AGENTS.md rule 1); it is a few hundred bytes.
    // One Kitab per user: a second launch (a double-clicked file) hands its
    // files to the running one, which opens them as tabs, and exits.
    let listener = match kitab_os::instance::claim(&files) {
        kitab_os::instance::Claim::Forwarded => return,
        kitab_os::instance::Claim::Primary(listener) => listener,
    };

    let settings_path = kitab_os::data_dir().map(|d| d.join("settings.json"));
    let settings = settings_path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| {
            kitab_ui::Settings::from_json(&text).unwrap_or_else(|e| {
                log::warn!("settings.json is not valid, using defaults: {e}");
                kitab_ui::Settings::default()
            })
        })
        .unwrap_or_default();
    let save_settings: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |json: String| {
        if let Some(path) = &settings_path {
            if let Err(e) = write_atomically(path, &json) {
                log::warn!("could not save settings: {e}");
            }
        }
    });
    let theme =
        match kitab_theme::builtin(&settings.theme).or_else(|_| kitab_theme::builtin("dark")) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Kitab: built-in theme failed to load: {e}");
                std::process::exit(1);
            }
        };
    let engine: Arc<dyn DocEngine> = Arc::new(adapters::MupdfEngine::new());
    // The library database opens on its own io thread, off the start-up path.
    let library: Arc<dyn Library> =
        adapters::StoreLibrary::start(kitab_os::data_dir().map(|d| d.join("library.sqlite3")));

    #[cfg(windows)]
    if let Some(first) = files.first().cloned() {
        first_frame::show(first, engine.clone(), &theme, settings.density);
    }

    Application::new()
        .with_assets(kitab_ui::Assets)
        .run(move |cx| {
            #[cfg(windows)]
            cx.set_global(FirstPaint(Some(Box::new(kitab_os::splash::close))));
            #[cfg(not(windows))]
            cx.set_global(FirstPaint(None));
            // Files from later launches arrive on a background thread.
            let (tx, rx) = smol::channel::unbounded::<Vec<PathBuf>>();
            listener.run(move |paths| {
                let _ = tx.send_blocking(paths);
            });
            cx.spawn(async move |cx| {
                while let Ok(paths) = rx.recv().await {
                    let _ = cx.update(|cx| kitab_ui::open_paths(cx, paths));
                }
            })
            .detach();
            let on_quit = library.clone();
            cx.on_app_quit(move |cx| {
                let library = on_quit.clone();
                // Off the UI thread; the quit waits for it (bounded by GPUI's quit timeout).
                cx.background_spawn(async move { library.end_session() })
            })
            .detach();
            if !kitab_ui::open_main_window(
                cx,
                engine,
                library,
                theme,
                settings,
                save_settings,
                files,
            ) {
                eprintln!("Kitab: could not open a window");
                cx.quit();
            }
        });
}

/// Write through a temporary file and rename, so a crash never leaves half a file.
fn write_atomically(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(windows)]
mod first_frame {
    //! The GDI first frame: the window's skeleton in theme colours and page 1
    //! where the GPU window will draw it (layout from `kitab_ui::layout`).

    use kitab_core::RenderRequest;
    use kitab_os::splash::{self, Fill, Frame, Page};
    use kitab_theme::{Density, Rgba, Theme};
    use kitab_ui::{DocEngine, layout};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn rgb(c: Rgba) -> (u8, u8, u8) {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        (q(c.r), q(c.g), q(c.b))
    }

    pub fn show(file: PathBuf, engine: Arc<dyn DocEngine>, theme: &Theme, density: Density) {
        splash::set_dpi_aware();
        let colors = theme.colors.clone();
        let dark = theme.appearance != kitab_theme::Appearance::Light;
        splash::show(move |scale| {
            let (doc, info) = engine.open(&file, None).ok()?;
            let m = density.metrics();
            let (w, h) = (layout::WINDOW_W, layout::WINDOW_H);
            // The tab bar only: the dock floats over the page (no header row).
            let top = m.title_bar;
            let area_x = layout::SIDEBAR_W + 1.0;
            let area_w = w - area_x;
            let size = info.page_sizes.first().copied()?;
            // Fit width, as the reader opens (PLAN.md Â§3.3).
            let ppt = (area_w - 2.0 * layout::PAGE_GAP) / size.w.max(1.0);
            let req = RenderRequest {
                page: 0,
                scale: ppt * scale,
                rotation: 0,
                tile: None,
                generation: 0,
            };
            let bmp = engine.render(doc, req).ok().flatten();
            engine.close(doc);
            let bmp = bmp?;
            let page_w = bmp.width as f32 / scale;
            let fills = vec![
                Fill {
                    x: 0.0,
                    y: 0.0,
                    w,
                    h,
                    rgb: rgb(colors.bg),
                },
                Fill {
                    x: 0.0,
                    y: m.title_bar - 1.0,
                    w,
                    h: 1.0,
                    rgb: rgb(colors.line),
                },
                Fill {
                    x: 0.0,
                    y: top - 1.0,
                    w,
                    h: 1.0,
                    rgb: rgb(colors.line),
                },
                Fill {
                    x: 0.0,
                    y: top,
                    w: layout::SIDEBAR_W,
                    h: h - top,
                    rgb: rgb(colors.panel),
                },
                Fill {
                    x: layout::SIDEBAR_W,
                    y: top,
                    w: 1.0,
                    h: h - top,
                    rgb: rgb(colors.line),
                },
                Fill {
                    x: area_x,
                    y: top,
                    w: area_w,
                    h: h - top,
                    rgb: rgb(colors.desk),
                },
            ];
            Some(Frame {
                client_w: w,
                client_h: h,
                fills,
                page: Page {
                    width: bmp.width,
                    height: bmp.height,
                    bgra: bmp.bgra,
                },
                page_x: area_x + (area_w - page_w) / 2.0,
                page_y: top + layout::PAGE_GAP,
                dark_title_bar: dark,
            })
        });
    }
}
