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

/// The typeface a book is set in (Settings › Reading › Books).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BookFace {
    /// The book's own fonts.
    #[default]
    Book,
    Serif,
    Sans,
    Mono,
}

impl BookFace {
    /// MuPDF's built-in face for it (no font files needed).
    pub fn family(self) -> Option<&'static str> {
        match self {
            BookFace::Book => None,
            BookFace::Serif => Some("Times"),
            BookFace::Sans => Some("Helvetica"),
            BookFace::Mono => Some("Courier"),
        }
    }
}

/// The reader's typography for flowing books, applied over the book's own
/// styles. The default changes nothing, so a book looks as its maker set it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BookStyle {
    pub face: BookFace,
    /// Line height as a multiple of the text size.
    pub line_spacing: Option<f32>,
    /// Page margins in em.
    pub margin: Option<f32>,
}

impl BookStyle {
    pub const DEFAULT: BookStyle = BookStyle {
        face: BookFace::Book,
        line_spacing: None,
        margin: None,
    };
    pub const LINE_MIN: f32 = 1.0;
    pub const LINE_MAX: f32 = 2.5;
    pub const MARGIN_MAX: f32 = 6.0;

    pub fn is_valid(&self) -> bool {
        let line = self
            .line_spacing
            .is_none_or(|v| v.is_finite() && (Self::LINE_MIN..=Self::LINE_MAX).contains(&v));
        let margin = self
            .margin
            .is_none_or(|v| v.is_finite() && (0.0..=Self::MARGIN_MAX).contains(&v));
        line && margin
    }

    /// The style sheet laid over the book's own (empty for the default).
    pub fn css(&self) -> String {
        let mut css = String::new();
        if let Some(family) = self.face.family() {
            css.push_str(&format!("* {{ font-family: {family} !important }}\n"));
        }
        if let Some(line) = self.line_spacing {
            css.push_str(&format!(
                "p, li, div, blockquote, dd, td {{ line-height: {line:.2} !important }}\n"
            ));
        }
        if let Some(m) = self.margin {
            css.push_str(&format!("@page {{ margin: {m:.2}em }}\n"));
        }
        css
    }
}

/// How a reflowable book (EPUB, MOBI, FB2, TXT) is laid out on pages: page
/// size in points, the body text size in points, and the reader's typography.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BookLayout {
    pub width: f32,
    pub height: f32,
    pub em: f32,
    pub style: BookStyle,
}

impl BookLayout {
    /// Before the window size is known; also the fixture oracle's layout
    /// (`pages_at_400x600_11pt` in expected.json).
    pub const DEFAULT: BookLayout = BookLayout {
        width: 400.0,
        height: 600.0,
        em: 11.0,
        style: BookStyle::DEFAULT,
    };
    /// Text sizes Kitab offers for books, in points.
    pub const EM_MIN: f32 = 8.0;
    pub const EM_MAX: f32 = 32.0;

    /// Page sides from 100 to 10,000 pt and a text size Kitab offers; anything
    /// else is a caller bug or a hostile value, refused before MuPDF sees it.
    pub fn is_valid(&self) -> bool {
        let side = |v: f32| v.is_finite() && (100.0..=10_000.0).contains(&v);
        side(self.width)
            && side(self.height)
            && self.em.is_finite()
            && (Self::EM_MIN..=Self::EM_MAX).contains(&self.em)
            && self.style.is_valid()
    }
}

impl Default for BookLayout {
    fn default() -> Self {
        Self::DEFAULT
    }
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
    /// A book whose pages come from a [`BookLayout`] (EPUB, MOBI, FB2, TXT),
    /// not from the file.
    pub reflowable: bool,
    /// Pages advance right to left (an EPUB's `page-progression-direction`).
    pub right_to_left: bool,
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

/// One character of a page's text with its box in page points, for selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextChar {
    pub ch: char,
    pub rect: RectPt,
    /// The line it is on, counted from 0 in reading order.
    pub line: u32,
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

#[cfg(test)]
mod book_style_tests {
    use super::*;

    #[test]
    fn the_default_style_adds_nothing() {
        assert_eq!(BookStyle::DEFAULT.css(), "");
        assert!(BookLayout::DEFAULT.is_valid());
    }

    #[test]
    fn choices_become_css() {
        let s = BookStyle {
            face: BookFace::Sans,
            line_spacing: Some(1.6),
            margin: Some(2.0),
        };
        let css = s.css();
        assert!(css.contains("font-family: Helvetica !important"));
        assert!(css.contains("line-height: 1.60 !important"));
        assert!(css.contains("@page { margin: 2.00em }"));
    }

    #[test]
    fn out_of_range_styles_are_refused() {
        let bad = |style| BookLayout {
            style,
            ..BookLayout::DEFAULT
        };
        let line = |v| BookStyle {
            line_spacing: Some(v),
            ..BookStyle::DEFAULT
        };
        assert!(!bad(line(0.2)).is_valid());
        assert!(!bad(line(f32::NAN)).is_valid());
        assert!(
            !bad(BookStyle {
                margin: Some(99.0),
                ..BookStyle::DEFAULT
            })
            .is_valid()
        );
        assert!(bad(line(1.5)).is_valid());
    }
}
