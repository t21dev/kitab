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
//! macOS and Linux: the document is written to a PDF (vector pages, by
//! MuPDF) and handed to the system's print dialog (`native`). On Linux
//! without a print portal, Kitab's own sheet lists the printers (`lpstat`)
//! and sends the pages with `lp`, which scales and turns them for the paper.

pub mod cups;
pub mod native;

/// A new temporary file for the pages to print. Older ones (a sheet on macOS
/// reads its file after the call returns) are removed here, after an hour.
pub fn temp_pdf() -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    if let Ok(list) = std::fs::read_dir(&dir) {
        for e in list.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age.as_secs() > 3600);
            if old && e.file_name().to_string_lossy().starts_with("kitab-print-") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    dir.join(format!("kitab-print-{}-{stamp}.pdf", std::process::id()))
}
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

/// Which pages the print dialog asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Chosen {
    All,
    /// The page on screen.
    Current,
    /// Inclusive ranges of 1-based page numbers, as typed in the dialog.
    Ranges(Vec<(u32, u32)>),
}

impl Chosen {
    /// The 0-based pages to print, in order, for a document of `count`
    /// pages showing `current` (0-based). Pages past the end are dropped.
    pub fn pages(&self, count: u32, current: u32) -> Vec<u32> {
        match self {
            Chosen::All => (0..count).collect(),
            Chosen::Current => vec![current.min(count.saturating_sub(1))],
            Chosen::Ranges(r) => r
                .iter()
                .flat_map(|&(a, b)| a.max(1)..=b.min(count))
                .map(|p| p - 1)
                .collect(),
        }
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
        pub fn choose_pages(
            _owner: isize,
            _page_count: u32,
            _current: u32,
        ) -> Result<Option<(Printer, super::Chosen)>, String> {
            Err("Printing on this system comes in a later version of Kitab.".into())
        }
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

#[cfg(test)]
mod chosen_tests {
    use super::Chosen;

    #[test]
    fn dialog_choices_become_pages() {
        assert_eq!(Chosen::All.pages(3, 1), vec![0, 1, 2]);
        assert_eq!(Chosen::Current.pages(3, 1), vec![1]);
        assert_eq!(Chosen::Current.pages(3, 9), vec![2]);
        // "1-2, 5, 9-12" in a 10-page document: past the end is dropped.
        let r = Chosen::Ranges(vec![(1, 2), (5, 5), (9, 12)]);
        assert_eq!(r.pages(10, 0), vec![0, 1, 4, 8, 9]);
        assert!(Chosen::Ranges(vec![(11, 12)]).pages(10, 0).is_empty());
    }
}
