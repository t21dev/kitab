// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows printing: the standard print dialog (`PrintDlgEx`) picks the
//! printer and copies, then GDI takes one bitmap per page
//! (`StartDoc`/`StartPage`/`StretchDIBits`). Runs on a worker thread, never
//! the UI thread: the dialog is modal to Kitab's window from there.

use crate::{Chosen, Paper, Placement};
use std::path::Path;
use windows::Win32::Foundation::{HGLOBAL, HWND};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDCW, DIB_RGB_COLORS, DeleteDC, GetDeviceCaps,
    HALFTONE, HDC, HORZRES, LOGPIXELSX, LOGPIXELSY, SRCCOPY, SetStretchBltMode, StretchDIBits,
    VERTRES,
};
use windows::Win32::Storage::Xps::{AbortDoc, DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::Controls::Dialogs::{
    PD_CURRENTPAGE, PD_NOCURRENTPAGE, PD_NOPAGENUMS, PD_NOSELECTION, PD_PAGENUMS, PD_RESULT_PRINT,
    PD_RETURNDC, PD_USEDEVMODECOPIESANDCOLLATE, PRINTDLGEXW, PRINTPAGERANGE, PrintDlgExW,
};
use windows::core::{HSTRING, PCWSTR};

/// `START_PAGE_GENERAL`: open the dialog on its General page.
const START_PAGE_GENERAL: u32 = 0xFFFF_FFFF;

/// A printer chosen in the dialog, with its device context.
pub struct Printer {
    dc: HDC,
    started: bool,
}

// SAFETY: the device context is used from one thread at a time (the print
// job's worker); `Printer` is moved to it, never shared.
unsafe impl Send for Printer {}

/// The most ranges the dialog takes ("1-3, 7, 10-12" is three).
const MAX_RANGES: usize = 64;

impl Printer {
    /// Windows' own print dialog, everything in it: the printer and its
    /// preferences, copies, and All / Current page / Pages for a document
    /// of `page_count` pages showing `current` (1-based). `None` when the
    /// reader cancels.
    pub fn choose_pages(
        owner: isize,
        page_count: u32,
        current: u32,
    ) -> Result<Option<(Printer, Chosen)>, String> {
        // SAFETY: COM for the dialog on this thread; a second call on the
        // same thread is harmless (it reports S_FALSE).
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let last = page_count.max(1);
        let mut ranges = vec![PRINTPAGERANGE::default(); MAX_RANGES];
        ranges[0] = PRINTPAGERANGE {
            nFromPage: current.clamp(1, last),
            nToPage: current.clamp(1, last),
        };
        let mut pd = PRINTDLGEXW {
            lStructSize: std::mem::size_of::<PRINTDLGEXW>() as u32,
            hwndOwner: HWND(owner as *mut core::ffi::c_void),
            Flags: PD_RETURNDC | PD_NOSELECTION | PD_USEDEVMODECOPIESANDCOLLATE,
            nPageRanges: 0,
            nMaxPageRanges: MAX_RANGES as u32,
            lpPageRanges: ranges.as_mut_ptr(),
            nMinPage: 1,
            nMaxPage: last,
            nCopies: 1,
            nStartPage: START_PAGE_GENERAL,
            ..Default::default()
        };
        // SAFETY: `pd` is a fully initialised PRINTDLGEXW of the size it
        // states, and `ranges` outlives the call with room for
        // `nMaxPageRanges` entries; the dialog fills in handles freed below.
        unsafe { PrintDlgExW(&mut pd) }.map_err(|e| format!("the print dialog failed: {e}"))?;
        free(pd.hDevMode);
        free(pd.hDevNames);
        if pd.dwResultAction != PD_RESULT_PRINT {
            if !pd.hDC.is_invalid() {
                // SAFETY: a DC the dialog created and nobody else holds.
                let _ = unsafe { DeleteDC(pd.hDC) };
            }
            return Ok(None);
        }
        if pd.hDC.is_invalid() {
            return Err("the printer gave no device to draw on".into());
        }
        let chosen = if pd.Flags.contains(PD_PAGENUMS) {
            let n = (pd.nPageRanges as usize).min(MAX_RANGES);
            Chosen::Ranges(
                ranges[..n]
                    .iter()
                    .map(|r| (r.nFromPage.min(r.nToPage), r.nFromPage.max(r.nToPage)))
                    .collect(),
            )
        } else if pd.Flags.contains(PD_CURRENTPAGE) {
            Chosen::Current
        } else {
            Chosen::All
        };
        Ok(Some((
            Printer {
                dc: pd.hDC,
                started: false,
            },
            chosen,
        )))
    }

    /// Show the print dialog over `owner` (a window handle, or 0). `None`
    /// when the reader cancels.
    pub fn choose(owner: isize, copies: u32) -> Result<Option<Printer>, String> {
        // SAFETY: COM for the dialog on this thread; a second call on the
        // same thread is harmless (it reports S_FALSE).
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let mut pd = PRINTDLGEXW {
            lStructSize: std::mem::size_of::<PRINTDLGEXW>() as u32,
            hwndOwner: HWND(owner as *mut core::ffi::c_void),
            Flags: PD_RETURNDC
                | PD_NOPAGENUMS
                | PD_NOSELECTION
                | PD_NOCURRENTPAGE
                | PD_USEDEVMODECOPIESANDCOLLATE,
            nCopies: copies.max(1),
            nStartPage: START_PAGE_GENERAL,
            ..Default::default()
        };
        // SAFETY: `pd` is a fully initialised PRINTDLGEXW of the size it
        // states; the dialog fills in the handles we free below.
        unsafe { PrintDlgExW(&mut pd) }.map_err(|e| format!("the print dialog failed: {e}"))?;
        free(pd.hDevMode);
        free(pd.hDevNames);
        if pd.dwResultAction != PD_RESULT_PRINT {
            if !pd.hDC.is_invalid() {
                // SAFETY: a DC the dialog created and nobody else holds.
                let _ = unsafe { DeleteDC(pd.hDC) };
            }
            return Ok(None);
        }
        if pd.hDC.is_invalid() {
            return Err("the printer gave no device to draw on".into());
        }
        Ok(Some(Printer {
            dc: pd.hDC,
            started: false,
        }))
    }

    /// A printer by name, without the dialog (tests, and `kitab --print`).
    pub fn named(name: &str) -> Result<Printer, String> {
        // SAFETY: plain strings in; a null DC is reported as an error.
        let dc = unsafe {
            CreateDCW(
                &HSTRING::from("WINSPOOL"),
                &HSTRING::from(name),
                PCWSTR::null(),
                None,
            )
        };
        if dc.is_invalid() {
            return Err(format!("no printer called {name}"));
        }
        Ok(Printer { dc, started: false })
    }

    /// The printer Windows prints to by default.
    pub fn default_printer() -> Result<Printer, String> {
        let mut len = 0u32;
        // SAFETY: the first call asks for the buffer size only.
        let _ = unsafe { windows::Win32::Graphics::Printing::GetDefaultPrinterW(None, &mut len) };
        if len == 0 {
            return Err("there is no default printer".into());
        }
        let mut buf = vec![0u16; len as usize];
        // SAFETY: `buf` holds `len` UTF-16 units, as the call was told.
        let ok = unsafe {
            windows::Win32::Graphics::Printing::GetDefaultPrinterW(
                Some(windows::core::PWSTR(buf.as_mut_ptr())),
                &mut len,
            )
        };
        if !ok.as_bool() {
            return Err("there is no default printer".into());
        }
        let name = String::from_utf16_lossy(&buf[..(len as usize).saturating_sub(1)]);
        Self::named(&name)
    }

    /// The printable area and resolution.
    pub fn paper(&self) -> Paper {
        // SAFETY: device-capability reads on a live DC.
        let cap = |i| unsafe { GetDeviceCaps(Some(self.dc), i) } as f32;
        Paper {
            width_px: cap(HORZRES),
            height_px: cap(VERTRES),
            dpi_x: cap(LOGPIXELSX).max(72.0),
            dpi_y: cap(LOGPIXELSY).max(72.0),
        }
    }

    /// Start the job, named `title` in the print queue. `to_file` sends a
    /// driver's output to a file instead (Microsoft Print to PDF in tests).
    pub fn begin(&mut self, title: &str, to_file: Option<&Path>) -> Result<(), String> {
        let name = HSTRING::from(title);
        let out = to_file.map(|p| HSTRING::from(p.as_os_str()));
        let info = DOCINFOW {
            cbSize: std::mem::size_of::<DOCINFOW>() as i32,
            lpszDocName: PCWSTR(name.as_ptr()),
            lpszOutput: out.as_ref().map_or(PCWSTR::null(), |o| PCWSTR(o.as_ptr())),
            ..Default::default()
        };
        // SAFETY: the strings outlive the call; a job id <= 0 is an error.
        let job = unsafe { StartDocW(self.dc, &info) };
        if job <= 0 {
            return Err("the printer didn't start the job".into());
        }
        self.started = true;
        Ok(())
    }

    /// Print one page: `bgra` is `w` × `h` pixels, drawn at `at`.
    pub fn page(&mut self, bgra: &[u8], w: u32, h: u32, at: Placement) -> Result<(), String> {
        if bgra.len() < (w as usize) * (h as usize) * 4 || w == 0 || h == 0 {
            return Err("a page image is the wrong size".into());
        }
        let header = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            // Negative: rows run top to bottom, as MuPDF draws them.
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let info = BITMAPINFO {
            bmiHeader: header,
            ..Default::default()
        };
        // SAFETY: page bracket and one blit on a live DC; `bgra` holds
        // w*h*4 bytes (checked above) as the header describes.
        unsafe {
            if StartPage(self.dc) <= 0 {
                return Err("the printer refused a page".into());
            }
            SetStretchBltMode(self.dc, HALFTONE);
            let drawn = StretchDIBits(
                self.dc,
                at.x.round() as i32,
                at.y.round() as i32,
                at.w.round() as i32,
                at.h.round() as i32,
                0,
                0,
                w as i32,
                h as i32,
                Some(bgra.as_ptr().cast()),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
            if EndPage(self.dc) <= 0 || drawn == 0 {
                return Err("the printer couldn't take a page".into());
            }
        }
        Ok(())
    }

    /// Finish the job: the printer gets it now.
    pub fn end(&mut self) -> Result<(), String> {
        self.started = false;
        // SAFETY: ends the job started in `begin` on this DC.
        if unsafe { EndDoc(self.dc) } <= 0 {
            return Err("the printer didn't finish the job".into());
        }
        Ok(())
    }

    /// Throw the job away (cancelled, or a page failed).
    pub fn abort(&mut self) {
        if self.started {
            self.started = false;
            // SAFETY: aborts the job started on this DC.
            let _ = unsafe { AbortDoc(self.dc) };
        }
    }
}

impl Drop for Printer {
    fn drop(&mut self) {
        self.abort();
        // SAFETY: the DC is ours (from the dialog or CreateDC) and dropped once.
        let _ = unsafe { DeleteDC(self.dc) };
    }
}

fn free(h: HGLOBAL) {
    if !h.is_invalid() {
        // SAFETY: memory the dialog allocated for us to free.
        let _ = unsafe { windows::Win32::Foundation::GlobalFree(Some(h)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Scale, place};

    /// J4 (PLAN.md §7.3): print three pages, one landscape, to Microsoft
    /// Print to PDF and count the pages it wrote. Run by hand: it needs that
    /// printer, which Windows 10 and 11 install by default.
    #[test]
    #[ignore = "needs the Microsoft Print to PDF printer; run by hand"]
    fn print_to_pdf_writes_every_page() {
        let out = std::env::temp_dir().join(format!("kitab-print-{}.pdf", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let mut p = Printer::named("Microsoft Print to PDF").expect("printer");
        let paper = p.paper();
        assert!(paper.width_px > 0.0 && paper.dpi_x >= 72.0);
        p.begin("Kitab print test", Some(&out)).expect("begin");
        for (w, h) in [(595.0, 842.0), (842.0, 595.0), (420.0, 595.0)] {
            let at = place(w, h, paper, Scale::Fit, true);
            let (pw, ph) = ((w * 0.5) as u32, (h * 0.5) as u32);
            let mut px = vec![255u8; (pw * ph * 4) as usize];
            // A dark band so the page has ink.
            for row in 0..ph / 10 {
                for col in 0..pw {
                    let i = ((row * pw + col) * 4) as usize;
                    px[i..i + 3].copy_from_slice(&[40, 40, 40]);
                }
            }
            p.page(&px, pw, ph, at).expect("page");
        }
        p.end().expect("end");
        drop(p);
        let bytes = std::fs::read(&out).expect("the PDF was written");
        assert!(bytes.starts_with(b"%PDF"));
        // The page tree's count: "/Count 3", however the driver spaces it.
        let text = String::from_utf8_lossy(&bytes);
        let count = text
            .split("/Count")
            .skip(1)
            .filter_map(|rest| {
                rest.trim_start()
                    .split(|c: char| !c.is_ascii_digit())
                    .next()
            })
            .filter_map(|n| n.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        if std::env::var_os("KITAB_KEEP_PRINT").is_some() {
            eprintln!("kept {}", out.display());
        } else {
            let _ = std::fs::remove_file(&out);
        }
        assert_eq!(count, 3, "pages in the printed PDF");
    }
}
