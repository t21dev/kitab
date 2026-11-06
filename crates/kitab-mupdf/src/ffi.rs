// SPDX-License-Identifier: AGPL-3.0-or-later

//! The few raw MuPDF calls the safe `mupdf` crate does not offer.
//!
//! The crate keeps its `fz_context` and `fz_document` pointers `pub(crate)`.
//! It derives `Debug` for both wrappers, which prints exactly
//! `Context { inner: 0xâ€¦ }` and `Document { inner: 0xâ€¦ }`. We read the pointer
//! back from that output, strictly: anything else (a crate upgrade that changes
//! the struct) parses to `None` and the caller falls back to a safe default.
//! `debug_format_is_understood` in the tests fails loudly if that happens.
//!
//! Only functions that cannot throw (no `fz_try` needed) are called here: a
//! MuPDF exception would `longjmp` across Rust frames.

#![allow(unsafe_code)]

use mupdf::{Context, Document};
use mupdf_sys::{
    fz_context, fz_document, fz_has_permission, fz_permission, fz_shrink_store, pdf_specifics,
    pdf_was_repaired,
};

/// Parse `"<name> { inner: 0x<hex> }"` into the pointer value.
fn parse_inner(debug: &str, name: &str) -> Option<usize> {
    let rest = debug.strip_prefix(name)?.strip_prefix(" { inner: 0x")?;
    let hex = rest.strip_suffix(" }")?;
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = usize::from_str_radix(hex, 16).ok()?;
    (value != 0).then_some(value)
}

/// This thread's MuPDF context (the crate's thread-local clone of the base context).
fn context_ptr() -> Option<*mut fz_context> {
    let ctx = Context::get();
    parse_inner(&format!("{ctx:?}"), "Context").map(|p| p as *mut fz_context)
}

fn document_ptr(doc: &Document) -> Option<*mut fz_document> {
    parse_inner(&format!("{doc:?}"), "Document").map(|p| p as *mut fz_document)
}

/// What the document allows, by MuPDF's own rules (an owner-password unlock
/// grants everything). `None` if the pointers could not be read.
pub(crate) fn permissions(doc: &Document) -> Option<kitab_core::Permissions> {
    let ctx = context_ptr()?;
    let raw = document_ptr(doc)?;
    let has = |p: fz_permission| {
        // SAFETY: `ctx` is this thread's live context, `raw` is the live document
        // borrowed through `doc` for the whole call, and `fz_has_permission` only
        // reads the security handler's flags; it never throws.
        unsafe { fz_has_permission(ctx, raw, p) != 0 }
    };
    Some(kitab_core::Permissions {
        print: has(mupdf_sys::FZ_PERMISSION_PRINT),
        copy: has(mupdf_sys::FZ_PERMISSION_COPY),
        annotate: has(mupdf_sys::FZ_PERMISSION_ANNOTATE),
        fill_forms: has(mupdf_sys::FZ_PERMISSION_FORM),
        modify: has(mupdf_sys::FZ_PERMISSION_EDIT),
        assemble: has(mupdf_sys::FZ_PERMISSION_ASSEMBLE),
    })
}

/// Whether MuPDF had to repair the file's cross-reference table to open it.
pub(crate) fn was_repaired(doc: &Document) -> Option<bool> {
    let ctx = context_ptr()?;
    let raw = document_ptr(doc)?;
    // SAFETY: as above; `pdf_specifics` returns the same object viewed as a
    // `pdf_document`, or null for other formats, and does not throw.
    let pdf = unsafe { pdf_specifics(ctx, raw) };
    if pdf.is_null() {
        return Some(false);
    }
    // SAFETY: `pdf` is non-null and owned by `doc`; `pdf_was_repaired` reads one field.
    Some(unsafe { pdf_was_repaired(ctx, pdf) } != 0)
}

/// Evict MuPDF's resource store (fonts, images, decoded objects) down to
/// `percent` % of its maximum. The store is shared by every context cloned
/// from the base one, so one call covers all workers. Returns false if the
/// context pointer could not be read.
pub(crate) fn shrink_store(percent: u32) -> bool {
    let Some(ctx) = context_ptr() else {
        return false;
    };
    // SAFETY: `ctx` is this thread's live context. `fz_shrink_store` takes the
    // store's own lock, drops unreferenced items and never throws.
    unsafe { fz_shrink_store(ctx, percent.min(100)) };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_the_exact_shape() {
        assert_eq!(
            parse_inner("Context { inner: 0x1f00 }", "Context"),
            Some(0x1f00)
        );
        assert_eq!(parse_inner("Context { inner: 0x0 }", "Context"), None);
        assert_eq!(
            parse_inner("Context { inner: 0x1f00, x: 1 }", "Context"),
            None
        );
        assert_eq!(parse_inner("Document { inner: 0x1f00 }", "Context"), None);
        assert_eq!(parse_inner("Context { inner: 1f00 }", "Context"), None);
    }

    /// Fails if a `mupdf` upgrade changes the `Debug` shape we rely on.
    #[test]
    fn debug_format_is_understood() {
        assert!(context_ptr().is_some(), "{:?}", Context::get());
    }
}
