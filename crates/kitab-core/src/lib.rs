// SPDX-License-Identifier: AGPL-3.0-or-later

//! The document model shared by every format (PLAN.md §3.3).
//!
//! Phase 0 holds only the format and layout enums. The `Document` trait,
//! locators and the text model land with Phase 1.

/// A file format Kitab can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Pdf,
    Xps,
    Epub,
    Mobi,
    Fb2,
    Cbz,
    Image,
    Txt,
    Markdown,
}

/// How a format is laid out on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutKind {
    /// Pages with their own fixed size (PDF, CBZ, fixed-layout EPUB).
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
            Format::Pdf | Format::Xps | Format::Cbz | Format::Image => LayoutKind::Fixed,
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
    fn layouts() {
        assert_eq!(Format::Pdf.default_layout(), LayoutKind::Fixed);
        assert_eq!(Format::Epub.default_layout(), LayoutKind::Reflow);
        assert_eq!(Format::Markdown.default_layout(), LayoutKind::Flow);
    }
}
