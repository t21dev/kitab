// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types shared by the engine, the store and the UI (PLAN.md §3.3).
//! Plain data only: no MuPDF, no GPUI, no SQLite here.

use crate::Format;

/// A size in PDF points (1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SizePt {
    pub w: f32,
    pub h: f32,
}

/// A rectangle in points, page space (origin top-left).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RectPt {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// What the document's owner allows (PLAN.md §3.12). All true for unencrypted files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    pub print: bool,
    pub copy: bool,
    pub annotate: bool,
    pub fill_forms: bool,
    pub modify: bool,
    pub assemble: bool,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            print: true,
            copy: true,
            annotate: true,
            fill_forms: true,
            modify: true,
            assemble: true,
        }
    }
}

/// One outline (bookmarks) entry.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineItem {
    pub title: String,
    /// Target page (0-based), if the entry points into this document.
    pub page: Option<u32>,
    /// External link, if the entry points outside.
    pub uri: Option<String>,
    /// Whether the entry starts open (PDF /Count > 0).
    pub open: bool,
    pub children: Vec<OutlineItem>,
}

/// Everything known about an open document that the UI needs up front.
#[derive(Debug, Clone, PartialEq)]
pub struct DocInfo {
    pub format: Format,
    pub title: Option<String>,
    pub author: Option<String>,
    pub page_count: u32,
    /// Each page's own size, rotation applied (true page sizes, PLAN.md §3.3).
    pub page_sizes: Vec<SizePt>,
    pub outline: Vec<OutlineItem>,
    pub permissions: Permissions,
    /// The security handler, e.g. "AES 256-bit", when the file is encrypted.
    pub encryption: Option<String>,
    /// MuPDF repaired a damaged file while opening it.
    pub repaired: bool,
}

/// Why a document could not be opened. Every variant is a placeholder in the UI,
/// never a panic (AGENTS.md rule 8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    NotFound,
    /// Encrypted; ask for the password.
    NeedsPassword,
    /// A password was given and it was wrong.
    WrongPassword,
    /// Not a format Kitab reads, or not a document at all.
    Unsupported,
    /// The engine failed; the message is for logs, not for document text.
    Failed(String),
}

/// A link on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub rect: RectPt,
    pub target: LinkTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    /// 0-based page, with an optional point on it.
    Page {
        page: u32,
        y: Option<f32>,
    },
    Uri(String),
}

/// A request to render (part of) a page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderRequest {
    pub page: u32,
    /// Device pixels per point.
    pub scale: f32,
    /// Extra rotation in degrees (0, 90, 180, 270), on top of the page's own.
    pub rotation: u16,
    /// Region of the rendered page in device pixels; `None` renders the whole page.
    pub tile: Option<TilePx>,
    /// Bumped by the caller when the view changes; results for older generations are dropped.
    pub generation: u64,
}

/// A rectangle in device pixels of a rendered page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TilePx {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// A rendered bitmap: BGRA, premultiplied, top-down, composited on white paper.
#[derive(Debug, Clone, PartialEq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl Bitmap {
    pub fn byte_size(&self) -> usize {
        self.bgra.len()
    }
}

/// Find options (PLAN.md §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FindOptions {
    pub match_case: bool,
    pub whole_word: bool,
    /// Ignore accents (diacritic-insensitive). On by default in the UI.
    pub ignore_diacritics: bool,
}

/// One find hit: its page and the quads that cover it, in page points.
#[derive(Debug, Clone, PartialEq)]
pub struct FindHit {
    pub page: u32,
    pub quads: Vec<RectPt>,
    /// The line the hit is on, for the results list.
    pub context: String,
}

/// A reading position that survives re-layout (PLAN.md §3.3). Fixed-layout only in Phase 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Locator {
    /// A point on a fixed page, in points from its top-left.
    Fixed { page: u32, x: f32, y: f32 },
}

/// How the page area fits pages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZoomMode {
    FitPage,
    FitWidth,
    ActualSize,
    /// A fixed zoom factor (1.0 = 100 %).
    Custom(f32),
}

/// Saved reading state for one document (the store keeps it, the UI restores it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReadingState {
    pub locator: Locator,
    pub zoom: ZoomMode,
    pub rotation: u16,
    /// 0.0..=1.0, for the library's progress bars.
    pub progress: f32,
}
