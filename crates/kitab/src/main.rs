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
    // Uninstalling: take Kitab's file types and thumbnails off this account
    // (the Windows uninstaller runs this), then stop.
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == "--unregister")
    {
        kitab_os::assoc::unregister();
        return;
    }
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

    kitab_os::trace::mark("main");
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
    kitab_os::trace::mark("settings and theme");
    let engine: Arc<dyn DocEngine> = Arc::new(adapters::MupdfEngine::new());
    kitab_os::trace::mark("engine");
    // The library database opens on its own io thread, off the start-up path.
    let library: Arc<dyn Library> =
        adapters::StoreLibrary::start(kitab_os::data_dir().map(|d| d.join("library.sqlite3")));

    // Files from later launches arrive on a background thread (Windows,
    // Linux) or as "open" events from the system (macOS's Finder).
    let (tx, rx) = smol::channel::unbounded::<Vec<PathBuf>>();
    let app = Application::new().with_assets(kitab_ui::Assets);
    {
        let tx = tx.clone();
        app.on_open_urls(move |urls| {
            let paths: Vec<PathBuf> = urls.iter().filter_map(|u| path_of_url(u)).collect();
            if !paths.is_empty() {
                let _ = tx.send_blocking(paths);
            }
        });
    }
    kitab_os::trace::mark("gpui application");
    app.run(move |cx| {
        kitab_os::trace::mark("gpui running");
        cx.set_global(FirstPaint(None));
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
        kitab_os::trace::mark("opening window");
        if !kitab_ui::open_main_window(cx, engine, library, theme, settings, save_settings, files) {
            eprintln!("Kitab: could not open a window");
            cx.quit();
        }
    });
}

/// A `file://` URL (macOS hands opened files over as these) as a path.
fn path_of_url(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // Undo %-escapes ("%20" for a space), byte by byte, then read as UTF-8.
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let path = PathBuf::from(String::from_utf8(out).ok()?);
    path.is_absolute().then_some(path)
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

#[cfg(test)]
mod url_tests {
    use super::path_of_url;

    #[test]
    fn file_urls_become_paths() {
        #[cfg(not(windows))]
        assert_eq!(
            path_of_url("file:///Users/me/My%20Books/a.pdf"),
            Some(std::path::PathBuf::from("/Users/me/My Books/a.pdf"))
        );
        assert_eq!(path_of_url("https://example.com/a.pdf"), None);
        assert_eq!(path_of_url("file://relative.pdf"), None);
        assert_eq!(path_of_url("file:///bad%zz"), None);
    }
}
