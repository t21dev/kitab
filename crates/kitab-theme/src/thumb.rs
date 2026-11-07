// SPDX-License-Identifier: AGPL-3.0-or-later

//! Colours of the file-manager thumbnail Kitab draws for text documents
//! (Markdown). It's a sheet of paper whatever the app theme is, like a page
//! thumbnail, so it has one fixed palette.

use crate::Rgba;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThumbPalette {
    pub page: Rgba,
    pub ink: Rgba,
    pub muted: Rgba,
    pub code_bg: Rgba,
    pub rule: Rgba,
    pub accent: Rgba,
}

fn hex(h: u32) -> Rgba {
    Rgba {
        r: ((h >> 16) & 0xFF) as f32 / 255.0,
        g: ((h >> 8) & 0xFF) as f32 / 255.0,
        b: (h & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

/// The paper palette (the Paper theme's page and ink, Kitab's amber).
pub fn thumb_palette() -> ThumbPalette {
    ThumbPalette {
        page: hex(0xFFFFFF),
        ink: hex(0x1F1D1A),
        muted: hex(0x6B665E),
        code_bg: hex(0xF3F0E9),
        rule: hex(0xDDD8CE),
        accent: hex(0xD99A2A),
    }
}
