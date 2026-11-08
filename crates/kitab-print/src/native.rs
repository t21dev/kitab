// SPDX-License-Identifier: AGPL-3.0-or-later

//! The system's own print dialog on macOS and Linux, given the document as a
//! PDF (vector pages, written by MuPDF). The dialog picks the printer, the
//! copies, the pages and the paper, and prints.
//!
//! macOS: PDFKit's print operation (`NSPrintOperation`), shown as a sheet on
//! the window. It must run on the main thread.
//!
//! Linux: the XDG print portal (`org.freedesktop.portal.Print`), the same
//! dialog GNOME and KDE apps show, and the one that works inside Flatpak.
//! It blocks, so it runs off the UI thread. Without a portal Kitab falls back
//! to its own sheet and CUPS.

/// Whether this system has a native print dialog for PDFs.
pub const AVAILABLE: bool = cfg!(any(target_os = "macos", target_os = "linux"));

/// How the dialog ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Printed,
    Cancelled,
}

/// Why it couldn't print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// No dialog here (Linux without a print portal): use Kitab's sheet.
    Unavailable,
    Failed(String),
}

#[cfg(target_os = "macos")]
pub use mac::print_with_dialog;

#[cfg(target_os = "macos")]
mod mac {
    use super::{Failure, Outcome};
    use objc::runtime::{NO, Object, YES};
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::CString;
    use std::path::Path;

    #[link(name = "PDFKit", kind = "framework")]
    unsafe extern "C" {}
    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {}

    /// kPDFPrintPageScaleDownToFit: big pages shrink to the paper, small
    /// ones print at their size.
    const SCALE_DOWN_TO_FIT: isize = 2;
    // NSPrintPanelOptions: copies, page range, paper size, orientation,
    // scaling, preview.
    const PANEL_OPTIONS: usize = 1 | 2 | 4 | 8 | 16 | (1 << 17);

    fn ns_string(s: &str) -> *mut Object {
        let c = CString::new(s.replace('\0', "")).unwrap_or_default();
        // SAFETY: a valid NUL-terminated string; the result is autoreleased.
        unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
    }

    /// Show the print panel for `pdf` as a sheet on the window whose content
    /// view is `ns_view` (0: a free-standing panel). Main thread only. The
    /// sheet runs on its own; this returns once it is up.
    pub fn print_with_dialog(ns_view: isize, pdf: &Path, title: &str) -> Result<Outcome, Failure> {
        let path = ns_string(&pdf.to_string_lossy());
        // SAFETY: plain AppKit/PDFKit messages on the main thread, each
        // receiver checked for nil before use. The operation keeps its own
        // reference to the document while it runs.
        unsafe {
            let url: *mut Object = msg_send![class!(NSURL), fileURLWithPath: path];
            if url.is_null() {
                return Err(Failure::Failed("the pages couldn't be found".into()));
            }
            let doc: *mut Object = msg_send![class!(PDFDocument), alloc];
            let doc: *mut Object = msg_send![doc, initWithURL: url];
            if doc.is_null() {
                return Err(Failure::Failed("the pages couldn't be read".into()));
            }
            let info: *mut Object = msg_send![class!(NSPrintInfo), sharedPrintInfo];
            let op: *mut Object = msg_send![doc,
                printOperationForPrintInfo: info
                scalingMode: SCALE_DOWN_TO_FIT
                autoRotate: YES];
            let _: () = msg_send![doc, release];
            if op.is_null() {
                return Err(Failure::Failed("macOS couldn't start printing".into()));
            }
            let _: () = msg_send![op, setJobTitle: ns_string(title)];
            let _: () = msg_send![op, setShowsPrintPanel: YES];
            let _: () = msg_send![op, setShowsProgressPanel: YES];
            let panel: *mut Object = msg_send![op, printPanel];
            if !panel.is_null() {
                let _: () = msg_send![panel, setOptions: PANEL_OPTIONS];
            }
            let window: *mut Object = if ns_view == 0 {
                std::ptr::null_mut()
            } else {
                msg_send![ns_view as *mut Object, window]
            };
            if window.is_null() {
                let ok: objc::runtime::BOOL = msg_send![op, runOperation];
                return Ok(if ok == NO {
                    Outcome::Cancelled
                } else {
                    Outcome::Printed
                });
            }
            let nil: *mut Object = std::ptr::null_mut();
            let none: objc::runtime::Sel = sel!(self);
            let _: () = msg_send![op,
                runOperationModalForWindow: window
                delegate: nil
                didRunSelector: none
                contextInfo: nil];
        }
        // The sheet reports nothing back without a delegate; macOS shows its
        // own progress.
        Ok(Outcome::Printed)
    }
}

#[cfg(target_os = "linux")]
pub use linux::print_with_dialog;

#[cfg(target_os = "linux")]
mod linux {
    use super::{Failure, Outcome};
    use ashpd::desktop::ResponseError;
    use ashpd::desktop::print::PrintProxy;
    use std::os::fd::AsFd;
    use std::path::Path;

    /// The portal's print dialog, then the pages to the printer. Blocks
    /// until the dialog closes: call it off the UI thread.
    pub fn print_with_dialog(_owner: isize, pdf: &Path, title: &str) -> Result<Outcome, Failure> {
        futures_lite::future::block_on(async {
            let proxy = PrintProxy::new().await.map_err(|_| Failure::Unavailable)?;
            let prepared = proxy
                .prepare_print(
                    None,
                    title,
                    Default::default(),
                    Default::default(),
                    None,
                    true,
                )
                .await
                .map_err(|_| Failure::Unavailable)?;
            let prepared = match prepared.response() {
                Ok(p) => p,
                Err(ashpd::Error::Response(ResponseError::Cancelled)) => {
                    return Ok(Outcome::Cancelled);
                }
                Err(e) => return Err(Failure::Failed(e.to_string())),
            };
            let file = std::fs::File::open(pdf).map_err(|e| Failure::Failed(e.to_string()))?;
            proxy
                .print(None, title, &file.as_fd(), Some(prepared.token), true)
                .await
                .map_err(|e| Failure::Failed(e.to_string()))?
                .response()
                .map_err(|e| Failure::Failed(e.to_string()))?;
            Ok(Outcome::Printed)
        })
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn print_with_dialog(
    _owner: isize,
    _pdf: &std::path::Path,
    _title: &str,
) -> Result<Outcome, Failure> {
    Err(Failure::Unavailable)
}
