// SPDX-License-Identifier: AGPL-3.0-or-later

//! Spike A (PLAN.md §12 Phase 0): open a GPUI window, render pages of a PDF
//! with MuPDF off the UI thread, show them as GPUI images, and scroll.
//!
//! Throwaway code. It prints internal trace marks to stderr:
//!   application_new, run_callback, window_opened, page1_rendered,
//!   first_page_painted, all_rendered
//! measured from process start. These are not the black-box S1/S2 numbers
//! (PLAN.md §6.1); they say whether the stack is in the right ballpark.
//!
//! Usage: spike-a <file.pdf> [pages=40] [--theme <id>] [--zoom <f>] [--splash] [--exit-after-first-paint]

use gpui::{
    App, Application, Bounds, Context, ImageSource, RenderImage, SharedString, Window,
    WindowBounds, WindowOptions, div, img, prelude::*, px, size,
};
use image::{Frame, RgbaImage};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

#[cfg(windows)]
mod splash;

static START: OnceLock<Instant> = OnceLock::new();
/// Theme id from `--theme <id>` (default "dark"), for captures in each theme.
static THEME: OnceLock<String> = OnceLock::new();
/// Display zoom from `--zoom <f>` (default 1.0: one point = one logical pixel).
static ZOOM: OnceLock<f32> = OnceLock::new();

fn mark(what: &str) {
    if let Some(start) = START.get() {
        eprintln!(
            "[spike-a] {what}: {:.1} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}

/// A rendered page in BGRA, as GPUI wants it.
struct PageImage {
    image: Arc<RenderImage>,
    width_pt: f32,
    height_pt: f32,
}

/// Render one page at `scale` (device pixels per point) to BGRA on white paper,
/// top-down: what GPUI's image and a GDI DIB both take. Returns (w, h, pixels, w pt, h pt).
fn render_bgra(
    doc: &mupdf::Document,
    index: i32,
    scale: f32,
) -> Result<(u32, u32, Vec<u8>, f32, f32), String> {
    let page = doc.load_page(index).map_err(|e| e.to_string())?;
    let bounds = page.bounds().map_err(|e| e.to_string())?;
    let matrix = mupdf::Matrix::new_scale(scale, scale);
    let pixmap = page
        .to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), true, false)
        .map_err(|e| e.to_string())?;
    let (w, h, n) = (pixmap.width(), pixmap.height(), pixmap.n() as usize);
    let stride = pixmap.stride() as usize;
    if n != 4 {
        return Err(format!("expected RGBA pixmap, got {n} channels"));
    }
    let samples = pixmap.samples();
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h as usize {
        let line = samples
            .get(row * stride..row * stride + w as usize * 4)
            .ok_or("pixmap shorter than its size")?;
        for px in line.as_chunks::<4>().0 {
            // MuPDF's alpha is premultiplied and the page background is
            // transparent: composite onto white paper, or black text lands
            // on the dark desk and the page looks empty.
            let paper = 255 - px[3];
            bgra.extend_from_slice(&[px[2] + paper, px[1] + paper, px[0] + paper, 255]);
        }
    }
    Ok((w, h, bgra, bounds.x1 - bounds.x0, bounds.y1 - bounds.y0))
}

/// Render one page for GPUI. Errors are strings: the spike only reports them.
fn render_page(doc: &mupdf::Document, index: i32, scale: f32) -> Result<PageImage, String> {
    let (w, h, bgra, width_pt, height_pt) = render_bgra(doc, index, scale)?;
    let buffer = RgbaImage::from_raw(w, h, bgra).ok_or("pixel buffer size mismatch")?;
    Ok(PageImage {
        image: Arc::new(RenderImage::new([Frame::new(buffer)])),
        width_pt,
        height_pt,
    })
}

struct Reader {
    title: SharedString,
    pages: Vec<PageImage>,
    error: Option<String>,
    painted: bool,
    exit_after_first_paint: bool,
}

impl Render for Reader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = kitab_theme::builtin(THEME.get().map_or("dark", String::as_str)).ok();
        let tok = |f: fn(&kitab_theme::Colors) -> kitab_theme::Rgba| {
            let c = theme.as_ref().map(|t| f(&t.colors));
            c.map(|c| gpui::Rgba {
                r: c.r,
                g: c.g,
                b: c.b,
                a: c.a,
            })
            .unwrap_or_default()
        };
        if !self.painted && !self.pages.is_empty() {
            self.painted = true;
            // The frame being built now is the first with a page in it.
            window.on_next_frame(|_, _| {
                mark("first_page_painted");
                // GPUI now shows the page itself: hand over from the GDI splash.
                #[cfg(windows)]
                splash::close();
            });
            if self.exit_after_first_paint {
                window.on_next_frame(|_, cx| cx.quit());
            }
        }
        let _ = cx;
        let zoom = *ZOOM.get().unwrap_or(&1.0);
        div()
            .id("pages")
            .size_full()
            .overflow_y_scroll()
            .bg(tok(|c| c.desk))
            .text_color(tok(|c| c.text))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(22.0))
            .py(px(22.0))
            .children(self.error.clone().map(|e| div().child(e)))
            .children(self.pages.iter().map(|p| {
                // flex_none: in a scrolling flex column, children may shrink
                // to fit the viewport; pages must keep their own size.
                img(ImageSource::Render(p.image.clone()))
                    .flex_none()
                    .w(px(p.width_pt * zoom))
                    .h(px(p.height_pt * zoom))
            }))
            .child(div().text_color(tok(|c| c.faint)).child(self.title.clone()))
    }
}

fn main() {
    START.get_or_init(Instant::now);
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(i) = args.iter().position(|a| a == "--zoom") {
        args.remove(i);
        if i < args.len() {
            let z = args.remove(i).parse::<f32>().unwrap_or(1.0).clamp(0.1, 8.0);
            ZOOM.get_or_init(|| z);
        }
    }
    if let Some(i) = args.iter().position(|a| a == "--theme") {
        args.remove(i);
        if i < args.len() {
            THEME.get_or_init(|| args.remove(i));
        }
    }
    let Some(path) = args.iter().find(|a| !a.starts_with("--")).cloned() else {
        eprintln!("usage: spike-a <file.pdf> [pages] [--exit-after-first-paint]");
        std::process::exit(2);
    };
    let max_pages: i32 = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(40);
    let exit_after_first_paint = args.iter().any(|a| a == "--exit-after-first-paint");

    // Window first (ADR 0002 step 3): with --splash, a GDI window shows page 1 while
    // GPUI sets up its GPU device and fonts on this thread.
    #[cfg(windows)]
    if args.iter().any(|a| a == "--splash") {
        splash::set_dpi_aware();
        let theme_id = THEME.get().map_or("dark", String::as_str).to_string();
        let desk = kitab_theme::builtin(&theme_id)
            .map(|t| t.colors.desk)
            .map(|c| {
                (
                    (c.r * 255.0) as u8,
                    (c.g * 255.0) as u8,
                    (c.b * 255.0) as u8,
                )
            })
            .unwrap_or((15, 15, 16));
        let dark = !matches!(theme_id.as_str(), "light" | "paper");
        let zoom = *ZOOM.get().unwrap_or(&1.0);
        let file = path.clone();
        splash::show(
            move |scale| {
                let doc = mupdf::Document::open(file.as_str()).ok()?;
                let (width, height, bgra, ..) = render_bgra(&doc, 0, scale * zoom).ok()?;
                Some(splash::Page {
                    width,
                    height,
                    bgra,
                })
            },
            desk,
            1000.0,
            dark,
        );
    }

    // Memory isolation (diagnostics): --splash-only keeps just the GDI splash
    // (Win32 + GDI + MuPDF, no GPUI); --no-mupdf runs GPUI without opening the file.
    if args.iter().any(|a| a == "--splash-only") {
        loop {
            std::thread::park();
        }
    }
    let no_mupdf = args.iter().any(|a| a == "--no-mupdf");

    let app = Application::new();
    mark("application_new");
    app.run(move |cx: &mut App| {
        mark("run_callback");
        let bounds = Bounds::centered(None, size(px(1000.0), px(1000.0)), cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| Reader {
                    title: SharedString::from(path.clone()),
                    pages: Vec::new(),
                    error: None,
                    painted: false,
                    exit_after_first_paint,
                })
            },
        );
        let Ok(window) = window else {
            eprintln!("could not open a window");
            cx.quit();
            return;
        };
        mark("window_opened");
        let scale = window
            .update(cx, |_, window, _| window.scale_factor())
            .unwrap_or(1.0);

        // Render on a plain thread: MuPDF never runs on the UI thread.
        let (tx, rx) = std::sync::mpsc::channel::<Result<PageImage, String>>();
        std::thread::spawn(move || {
            if no_mupdf {
                return;
            }
            let doc = match mupdf::Document::open(path.as_str()) {
                Ok(d) => d,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    return;
                }
            };
            let count = doc.page_count().unwrap_or(0).min(max_pages);
            for i in 0..count {
                let page = render_page(&doc, i, scale * ZOOM.get().copied().unwrap_or(1.0));
                if i == 0 {
                    mark("page1_rendered");
                }
                if tx.send(page).is_err() {
                    return;
                }
            }
            mark("all_rendered");
        });

        // Drain results onto the UI thread without blocking it.
        cx.spawn(async move |cx| {
            let rx = Arc::new(std::sync::Mutex::new(rx));
            loop {
                let rx2 = rx.clone();
                let got = cx
                    .background_spawn(
                        async move { rx2.lock().map_err(|_| ())?.recv().map_err(|_| ()) },
                    )
                    .await;
                let Ok(result) = got else { break };
                let ok = window.update(cx, |reader, _, cx| {
                    match result {
                        Ok(p) => reader.pages.push(p),
                        Err(e) => reader.error = Some(e),
                    }
                    cx.notify();
                });
                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();
    });
}
