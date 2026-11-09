// SPDX-License-Identifier: AGPL-3.0-or-later

//! What the UI needs from a document engine. `kitab-mupdf` implements it in the
//! app; tests implement it with fakes. Every method is called from a background
//! task, never from the UI thread (AGENTS.md rule 2).

use kitab_core::{
    Bitmap, BookLayout, DocInfo, FindHit, FindOptions, Link, OpenError, RenderRequest,
    SignatureCheck, TextChar,
};

/// A signature to add (Sign…): where it goes and what it says. The key's
/// password travels with the request only (AGENTS.md rule 13).
#[derive(Clone)]
pub struct SignRequest {
    /// The `.pfx`/`.p12` file and its password.
    pub key_file: PathBuf,
    pub password: String,
    pub page: u32,
    /// The box in page points from the top-left: x0, y0, x1, y1.
    pub rect: [f32; 4],
    pub reason: Option<String>,
    pub location: Option<String>,
}

impl std::fmt::Debug for SignRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignRequest")
            .field("key_file", &self.key_file)
            .field("page", &self.page)
            .finish_non_exhaustive()
    }
}

/// A page tool (Page tools…): each writes a new file.
#[derive(Debug, Clone, PartialEq)]
pub enum PageOp {
    Delete(Vec<u32>),
    /// Pages, and the page (0-based) they go before; the page count puts
    /// them at the end.
    Move(Vec<u32>, u32),
    /// Pages and degrees clockwise.
    Rotate(Vec<u32>, i32),
    Compress,
    /// Page numbers, starting at this one.
    Number(u32),
    Watermark(String),
    /// The text, as a .txt file.
    Text,
    /// The pictures, as PNG files in the folder `out`.
    Images,
}

/// A certificate the user trusts for signatures (Settings › Signing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedCert {
    pub subject: String,
    pub fingerprint: String,
    pub path: PathBuf,
}
use std::path::{Path, PathBuf};

/// An open document inside the engine.
pub type DocHandle = u64;

/// Asked on the render worker whether a queued page is still worth drawing.
pub type Wanted = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;

pub trait DocEngine: Send + Sync + 'static {
    fn open(&self, path: &Path, password: Option<&str>) -> Result<(DocHandle, DocInfo), OpenError>;
    /// [`DocEngine::open`], with a book laid out in `style` from the start
    /// (so opening in the theme's colours doesn't mean opening twice).
    fn open_styled(
        &self,
        path: &Path,
        password: Option<&str>,
        style: kitab_core::BookStyle,
    ) -> Result<(DocHandle, DocInfo), OpenError> {
        let _ = style;
        self.open(path, password)
    }
    /// `Ok(None)` when the request's generation is stale.
    fn render(&self, doc: DocHandle, req: RenderRequest) -> Result<Option<Bitmap>, String>;
    /// [`DocEngine::render`], skipped (`Ok(None)`) if `wanted` returns false
    /// when the job starts: the page left the screen while it waited.
    fn render_when(
        &self,
        doc: DocHandle,
        req: RenderRequest,
        wanted: Wanted,
    ) -> Result<Option<Bitmap>, String> {
        if wanted() {
            self.render(doc, req)
        } else {
            Ok(None)
        }
    }
    fn set_generation(&self, doc: DocHandle, generation: u64);
    fn links(&self, doc: DocHandle, page: u32) -> Vec<Link>;
    /// Calls `on_hit` for each hit in page order; stops when it returns false.
    fn find(
        &self,
        doc: DocHandle,
        query: &str,
        options: FindOptions,
        on_hit: &mut dyn FnMut(FindHit) -> bool,
    );
    /// Every hit on one page, in reading order. The find bar searches page by
    /// page so it can stop early and show hits as they arrive.
    fn find_page(
        &self,
        doc: DocHandle,
        page: u32,
        query: &str,
        options: FindOptions,
    ) -> Vec<FindHit>;
    /// A page's characters with their boxes, for text selection.
    fn page_chars(&self, doc: DocHandle, page: u32) -> Vec<TextChar>;
    /// A page's text in reading order, lines separated by newlines.
    fn page_text(&self, doc: DocHandle, page: u32) -> Option<String>;
    /// Write `pages` (0-based) of the PDF at `src` to a new PDF at `out`.
    fn extract_pages(
        &self,
        src: &Path,
        password: Option<&str>,
        pages: &[u32],
        out: &Path,
    ) -> Result<(), String>;
    /// Run a page tool on the PDF at `src`, writing `out`. Returns what it
    /// did, in words for a toast ("Deleted 2 pages").
    fn page_tool(
        &self,
        src: &Path,
        password: Option<&str>,
        op: &PageOp,
        out: &Path,
    ) -> Result<String, String> {
        let _ = (src, password, op, out);
        Err("this engine has no page tools".into())
    }
    /// Verify every signature in the PDF at `src` against the trusted
    /// certificates. Empty when it has none.
    fn signatures(
        &self,
        src: &Path,
        password: Option<&str>,
    ) -> Result<Vec<SignatureCheck>, String> {
        let _ = (src, password);
        Ok(Vec::new())
    }
    /// Who a key file signs as: (name, full subject). Checks the password.
    fn signer_of(&self, key_file: &Path, password: &str) -> Result<(String, String), String> {
        let _ = (key_file, password);
        Err("this engine can't sign".into())
    }
    /// Sign the PDF at `src` into a new file at `out` (an incremental
    /// update, so earlier signatures stay valid).
    fn sign_pdf(&self, src: &Path, req: &SignRequest, out: &Path) -> Result<(), String> {
        let _ = (src, req, out);
        Err("this engine can't sign".into())
    }
    /// The certificates trusted for signatures.
    fn trusted_certs(&self) -> Vec<TrustedCert> {
        Vec::new()
    }
    /// Trust the certificates in a PEM or DER file; their subjects.
    fn trust_cert(&self, file: &Path) -> Result<Vec<String>, String> {
        let _ = file;
        Err("this engine has no trust list".into())
    }
    /// Stop trusting one certificate.
    fn untrust_cert(&self, path: &Path) -> Result<(), String> {
        let _ = path;
        Err("this engine has no trust list".into())
    }
    /// Write a copy of the PDF at `src` to `out` locked with `new_password`,
    /// or with no password when `None`.
    fn set_password(
        &self,
        src: &Path,
        password: Option<&str>,
        new_password: Option<&str>,
        out: &Path,
    ) -> Result<(), String> {
        let _ = (src, password, new_password, out);
        Err("this engine can't change passwords".into())
    }
    /// Write all pages of `inputs` (path, password), in order, to `out`. Returns the page count.
    fn merge(&self, inputs: &[(PathBuf, Option<String>)], out: &Path) -> Result<u32, String>;
    /// Write `marks` into the PDF at `src` as annotations and put it at `out`
    /// (which may be `src`; close the document first on Windows). Returns how
    /// many were written.
    fn write_marks(
        &self,
        src: &Path,
        password: Option<&str>,
        marks: &[kitab_core::Mark],
        edits: &[kitab_core::AnnotEdit],
        out: &Path,
    ) -> Result<usize, String> {
        let _ = (src, password, marks, edits, out);
        Err("this engine can't save marks".into())
    }
    /// Write `pages` of an open document to a new PDF at `out` (vector pages
    /// as laid out now): what macOS and Linux print through CUPS.
    fn write_pdf(&self, doc: DocHandle, pages: &[u32], out: &Path) -> Result<u32, String> {
        let _ = (doc, pages, out);
        Err("this engine can't write PDFs".into())
    }
    /// A page's form fields (PDF only), in file order.
    fn form_fields(&self, doc: DocHandle, page: u32) -> Vec<kitab_core::FormField> {
        let _ = (doc, page);
        Vec::new()
    }
    /// Fill in a field in the open document (Save writes it into the file).
    /// Returns the pages to draw again.
    fn set_field(&self, doc: DocHandle, edit: kitab_core::FieldEdit) -> Result<Vec<u32>, String> {
        let _ = (doc, edit);
        Err("this engine can't fill in forms".into())
    }
    /// The annotations already in the file (PDF only).
    fn annotations(&self, doc: DocHandle) -> Vec<kitab_core::FileAnnot> {
        let _ = doc;
        Vec::new()
    }
    /// Lay a reflowable book out again (new page size or text size) and
    /// describe it anew: page count, sizes and contents change.
    fn relayout(&self, doc: DocHandle, layout: BookLayout) -> Result<DocInfo, String> {
        let _ = (doc, layout);
        Err("this engine can't lay books out".into())
    }
    /// Drop cached engine data (decoded fonts and images, parsed pages) that
    /// no open page is using (Settings › Advanced › Clear cached pages).
    fn shrink_caches(&self) {}
    fn close(&self, doc: DocHandle);
}
