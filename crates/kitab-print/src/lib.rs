// SPDX-License-Identifier: AGPL-3.0-or-later

//! Printing (PLAN.md §3.8). GPUI has no printing, so each OS gets its own
//! path; this crate holds the shared, testable part (where a page lands on
//! the paper) and the native part. One of the crates allowed `unsafe`
//! (AGENTS.md rule 10), for the OS calls only.
//!
//! Windows: the standard print dialog (`PrintDlgEx`) picks the printer and
//! copies, then each page, drawn by MuPDF at the printer's resolution, goes
//! to the printer's GDI device.
//!
//! macOS and Linux: CUPS. Kitab's own sheet lists the printers (`lpstat`) and
//! takes the copies; the chosen pages are written to a PDF (vector pages, by
//! MuPDF) and sent with `lp`, which scales and turns them for the paper.

pub mod cups;
mod place;
pub use place::{Placement, Scale, place};

/// A printer the system knows (macOS and Linux; Windows uses its dialog).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterInfo {
    pub name: String,
    pub is_default: bool,
}

/// Whether printing goes through the system's own dialog (Windows), or
/// through Kitab's sheet with a printer list (macOS and Linux).
pub const SYSTEM_DIALOG: bool = cfg!(windows);

/// The printers to choose from in Kitab's sheet (macOS and Linux).
pub fn printers() -> Result<Vec<PrinterInfo>, String> {
    #[cfg(not(windows))]
    {
        cups::run::printers()
    }
    #[cfg(windows)]
    {
        Ok(Vec::new())
    }
}

/// Send a PDF to a printer through CUPS (macOS and Linux).
pub fn print_pdf(
    printer: &str,
    pdf: &std::path::Path,
    title: &str,
    copies: u32,
    scale: Scale,
    auto_rotate: bool,
) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        cups::run::print_pdf(printer, pdf, title, copies, scale, auto_rotate)
    }
    #[cfg(windows)]
    {
        let _ = (printer, pdf, title, copies, scale, auto_rotate);
        Err("Windows prints through its own dialog".into())
    }
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::Printer;

/// What a printer offers a page: its printable area in device pixels, and
/// its resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paper {
    pub width_px: f32,
    pub height_px: f32,
    pub dpi_x: f32,
    pub dpi_y: f32,
}

/// A page to print: its size in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub w: f32,
    pub h: f32,
}

/// One print job's choices.
#[derive(Debug, Clone)]
pub struct Job<'a> {
    /// Its name in the print queue.
    pub title: &'a str,
    /// Send the driver's output to this file instead (Print to PDF).
    pub to_file: Option<&'a std::path::Path>,
    /// (page number, size) in print order.
    pub pages: &'a [(u32, PageSize)],
    pub scale: Scale,
    pub auto_rotate: bool,
}

/// Print `job` on `printer`. `render(page, scale, rotation)` draws a page as
/// BGRA pixels; `progress` hears how many pages are done. Returns how many
/// pages went out; on an error the job is thrown away.
pub fn print_pages(
    printer: &mut Printer,
    job: &Job,
    mut render: impl FnMut(u32, f32, u16) -> Result<(Vec<u8>, u32, u32), String>,
    mut progress: impl FnMut(usize),
) -> Result<usize, String> {
    if job.pages.is_empty() {
        return Err("there are no pages to print".into());
    }
    let paper = printer.paper();
    printer.begin(job.title, job.to_file)?;
    for (n, &(page, size)) in job.pages.iter().enumerate() {
        let at = place(size.w, size.h, paper, job.scale, job.auto_rotate);
        let drawn = render(page, at.render_scale, if at.rotate { 90 } else { 0 })
            .and_then(|(px, w, h)| printer.page(&px, w, h, at));
        if let Err(e) = drawn {
            printer.abort();
            return Err(format!("page {}: {e}", page + 1));
        }
        progress(n + 1);
    }
    printer.end()?;
    Ok(job.pages.len())
}

/// The most detail a page is drawn at. Printers report 600 or 1200 dpi;
/// text and images look the same at 300, at a quarter of the memory.
pub const MAX_DPI: f32 = 300.0;

#[cfg(not(windows))]
mod other {
    use super::Paper;

    /// Printing here comes in a later version.
    pub struct Printer;

    impl Printer {
        pub fn choose(_owner: isize, _copies: u32) -> Result<Option<Printer>, String> {
            Err("Printing on this system comes in a later version of Kitab.".into())
        }
        pub fn named(_name: &str) -> Result<Printer, String> {
            Err("Printing on this system comes in a later version of Kitab.".into())
        }
        pub fn default_printer() -> Result<Printer, String> {
            Err("Printing on this system comes in a later version of Kitab.".into())
        }
        pub fn paper(&self) -> Paper {
            Paper {
                width_px: 0.0,
                height_px: 0.0,
                dpi_x: 72.0,
                dpi_y: 72.0,
            }
        }
        pub fn begin(
            &mut self,
            _title: &str,
            _to_file: Option<&std::path::Path>,
        ) -> Result<(), String> {
            Err("not supported".into())
        }
        pub fn page(
            &mut self,
            _bgra: &[u8],
            _w: u32,
            _h: u32,
            _at: super::Placement,
        ) -> Result<(), String> {
            Err("not supported".into())
        }
        pub fn end(&mut self) -> Result<(), String> {
            Err("not supported".into())
        }
        pub fn abort(&mut self) {}
    }
}
#[cfg(not(windows))]
pub use other::Printer;
