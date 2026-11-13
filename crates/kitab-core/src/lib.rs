// SPDX-License-Identifier: AGPL-3.0-or-later

//! The document model shared by every format (PLAN.md §3.3).
//!
//! Plain data shared by the engine (`kitab-mupdf`), the store (`kitab-store`)
//! and the UI (`kitab-ui`): formats, page sizes, outline, permissions, render
//! requests, locators and reading state.

mod form;
mod legible;
mod markup;
mod model;
pub mod notes;
mod ranges;
mod signature;
pub mod spoken;
pub use form::*;
pub use legible::legible;
pub use markup::*;
pub use model::*;
pub use ranges::parse_page_ranges;
pub use signature::*;

/// A file format Kitab can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Pdf,
    Xps,
    Epub,
    Mobi,
    Fb2,
    Cbz,
    /// A comic book in a RAR archive (`.cbr`).
    Cbr,
    /// A comic book in a 7z archive (`.cb7`).
    Cb7,
    /// A scanned book or document (`.djvu`, `.djv`).
    Djvu,
    Image,
    Txt,
    Markdown,
}

/// How a format is laid out on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutKind {
    /// Pages with their own fixed size (PDF, comics, DjVu, fixed-layout EPUB).
    Fixed,
    /// Text reflowed into pages for the window size (EPUB, MOBI, FB2, TXT).
    Reflow,
    /// One continuous flow (Markdown).
    Flow,
}

impl Format {
    /// The layout a format uses by default. Fixed-layout EPUB is decided per
    /// document, not here.
    pub fn default_layout(self) -> LayoutKind {
        match self {
            Format::Pdf
            | Format::Xps
            | Format::Cbz
            | Format::Cbr
            | Format::Cb7
            | Format::Djvu
            | Format::Image => LayoutKind::Fixed,
            Format::Epub | Format::Mobi | Format::Fb2 | Format::Txt => LayoutKind::Reflow,
            Format::Markdown => LayoutKind::Flow,
        }
    }

    /// Guess the format from a file extension (without the dot, any case).
    /// Content sniffing comes later; this is only a hint.
    pub fn from_extension(ext: &str) -> Option<Format> {
        let ext = ext.to_ascii_lowercase();
        Some(match ext.as_str() {
            "pdf" => Format::Pdf,
            "xps" | "oxps" => Format::Xps,
            "epub" => Format::Epub,
            "mobi" | "azw" | "azw3" => Format::Mobi,
            "fb2" => Format::Fb2,
            "cbz" => Format::Cbz,
            "cbr" => Format::Cbr,
            "cb7" => Format::Cb7,
            "djvu" | "djv" => Format::Djvu,
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tif" | "tiff" | "webp" => Format::Image,
            "txt" => Format::Txt,
            "md" | "markdown" | "mdown" | "mkd" => Format::Markdown,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_is_case_insensitive() {
        assert_eq!(Format::from_extension("PDF"), Some(Format::Pdf));
        assert_eq!(Format::from_extension("Md"), Some(Format::Markdown));
        assert_eq!(Format::from_extension("docx"), None);
    }

    #[test]
    fn comics_and_djvu() {
        assert_eq!(Format::from_extension("CBR"), Some(Format::Cbr));
        assert_eq!(Format::from_extension("cb7"), Some(Format::Cb7));
        assert_eq!(Format::from_extension("djv"), Some(Format::Djvu));
        assert_eq!(Format::from_extension("djvu"), Some(Format::Djvu));
        // .rar and .7z are archives, not comics: Kitab doesn't claim them.
        assert_eq!(Format::from_extension("rar"), None);
        assert_eq!(Format::from_extension("7z"), None);
        for f in [Format::Cbr, Format::Cb7, Format::Djvu] {
            assert_eq!(f.default_layout(), LayoutKind::Fixed);
        }
    }

    #[test]
    fn layouts() {
        assert_eq!(Format::Pdf.default_layout(), LayoutKind::Fixed);
        assert_eq!(Format::Epub.default_layout(), LayoutKind::Reflow);
        assert_eq!(Format::Markdown.default_layout(), LayoutKind::Flow);
    }
}
