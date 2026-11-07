// SPDX-License-Identifier: AGPL-3.0-or-later

//! Code colours for Markdown code blocks (Settings › Reading › Code colours).
//! "Kitab" draws from the app theme's own tokens; the others are the
//! well-known editor palettes, each in a light and a dark variant where the
//! original has one (the variant follows the app theme's appearance).

use crate::Rgba;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodeTheme {
    /// The app theme's tokens (keyword = accent, string = ok, …).
    #[default]
    Kitab,
    GitHub,
    One,
    Solarized,
    Dracula,
    Monokai,
    Nord,
}

/// Colours for one code block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CodePalette {
    pub background: Rgba,
    pub plain: Rgba,
    pub keyword: Rgba,
    pub string: Rgba,
    pub comment: Rgba,
    pub number: Rgba,
    pub function: Rgba,
    pub type_: Rgba,
    pub tag: Rgba,
    pub punctuation: Rgba,
}

fn hex(h: u32) -> Rgba {
    Rgba {
        r: ((h >> 16) & 0xFF) as f32 / 255.0,
        g: ((h >> 8) & 0xFF) as f32 / 255.0,
        b: (h & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

#[allow(clippy::too_many_arguments)]
fn pal(
    bg: u32,
    plain: u32,
    kw: u32,
    s: u32,
    c: u32,
    n: u32,
    f: u32,
    t: u32,
    tag: u32,
    p: u32,
) -> CodePalette {
    CodePalette {
        background: hex(bg),
        plain: hex(plain),
        keyword: hex(kw),
        string: hex(s),
        comment: hex(c),
        number: hex(n),
        function: hex(f),
        type_: hex(t),
        tag: hex(tag),
        punctuation: hex(p),
    }
}

impl CodeTheme {
    pub const ALL: [CodeTheme; 7] = [
        Self::Kitab,
        Self::GitHub,
        Self::One,
        Self::Solarized,
        Self::Dracula,
        Self::Monokai,
        Self::Nord,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Kitab => "kitab",
            Self::GitHub => "github",
            Self::One => "one",
            Self::Solarized => "solarized",
            Self::Dracula => "dracula",
            Self::Monokai => "monokai",
            Self::Nord => "nord",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Kitab => "Kitab",
            Self::GitHub => "GitHub",
            Self::One => "One",
            Self::Solarized => "Solarized",
            Self::Dracula => "Dracula",
            Self::Monokai => "Monokai",
            Self::Nord => "Nord",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Kitab => "Follows the theme",
            Self::GitHub | Self::One | Self::Solarized => "Light and dark",
            Self::Dracula | Self::Monokai | Self::Nord => "Dark",
        }
    }

    /// The palette for a light (`dark == false`) or dark app theme; `None`
    /// for Kitab, which uses the theme's tokens.
    pub fn palette(self, dark: bool) -> Option<CodePalette> {
        Some(match (self, dark) {
            (Self::Kitab, _) => return None,
            (Self::GitHub, false) => pal(
                0xF6F8FA, 0x1F2328, 0xCF222E, 0x0A3069, 0x6E7781, 0x0550AE, 0x8250DF, 0x953800,
                0x116329, 0x1F2328,
            ),
            (Self::GitHub, true) => pal(
                0x161B22, 0xE6EDF3, 0xFF7B72, 0xA5D6FF, 0x8B949E, 0x79C0FF, 0xD2A8FF, 0xFFA657,
                0x7EE787, 0xC9D1D9,
            ),
            (Self::One, false) => pal(
                0xFAFAFA, 0x383A42, 0xA626A4, 0x50A14F, 0xA0A1A7, 0x986801, 0x4078F2, 0xC18401,
                0xE45649, 0x383A42,
            ),
            (Self::One, true) => pal(
                0x282C34, 0xABB2BF, 0xC678DD, 0x98C379, 0x7F848E, 0xD19A66, 0x61AFEF, 0xE5C07B,
                0xE06C75, 0xABB2BF,
            ),
            (Self::Solarized, false) => pal(
                0xFDF6E3, 0x586E75, 0x859900, 0x2AA198, 0x93A1A1, 0xD33682, 0x268BD2, 0xB58900,
                0xCB4B16, 0x657B83,
            ),
            (Self::Solarized, true) => pal(
                0x002B36, 0x93A1A1, 0x859900, 0x2AA198, 0x586E75, 0xD33682, 0x268BD2, 0xB58900,
                0xCB4B16, 0x839496,
            ),
            (Self::Dracula, _) => pal(
                0x282A36, 0xF8F8F2, 0xFF79C6, 0xF1FA8C, 0x6272A4, 0xBD93F9, 0x50FA7B, 0x8BE9FD,
                0xFFB86C, 0xF8F8F2,
            ),
            (Self::Monokai, _) => pal(
                0x272822, 0xF8F8F2, 0xF92672, 0xE6DB74, 0x75715E, 0xAE81FF, 0xA6E22E, 0x66D9EF,
                0xFD971F, 0xF8F8F2,
            ),
            (Self::Nord, _) => pal(
                0x2E3440, 0xD8DEE9, 0x81A1C1, 0xA3BE8C, 0x616E88, 0xB48EAD, 0x88C0D0, 0x8FBCBB,
                0xD08770, 0xECEFF4,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_kitab_follows_the_theme() {
        for t in CodeTheme::ALL {
            assert_eq!(CodeTheme::from_id(t.id()), Some(t));
        }
        assert!(CodeTheme::Kitab.palette(true).is_none());
        // Light and dark variants differ where the original has both.
        assert_ne!(
            CodeTheme::GitHub.palette(false),
            CodeTheme::GitHub.palette(true)
        );
        assert_eq!(
            CodeTheme::Dracula.palette(false),
            CodeTheme::Dracula.palette(true)
        );
    }
}
