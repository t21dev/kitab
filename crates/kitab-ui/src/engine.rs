// SPDX-License-Identifier: AGPL-3.0-or-later

//! What the UI needs from a document engine. `kitab-mupdf` implements it in the
//! app; tests implement it with fakes. Every method is called from a background
//! task, never from the UI thread (AGENTS.md rule 2).

use kitab_core::{
    Bitmap, BookLayout, DocInfo, FindHit, FindOptions, Link, OpenError, RenderRequest, TextChar,
};
use std::path::{Path, PathBuf};

/// An open document inside the engine.
pub type DocHandle = u64;

/// Asked on the render worker whether a queued page is still worth drawing.
pub type Wanted = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;

pub trait DocEngine: Send + Sync + 'static {
    fn open(&self, path: &Path, password: Option<&str>) -> Result<(DocHandle, DocInfo), OpenError>;
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
        out: &Path,
    ) -> Result<usize, String> {
        let _ = (src, password, marks, out);
        Err("this engine can't save marks".into())
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
    fn close(&self, doc: DocHandle);
}
