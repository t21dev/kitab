// SPDX-License-Identifier: AGPL-3.0-or-later

//! Ink colours for markup. Marks are part of the document (they are saved into
//! the PDF), so they keep their colour in every app theme, like ink on paper.

/// Pen, line and shape colours: name and sRGB.
pub const PEN_COLOURS: [(&str, (u8, u8, u8)); 6] = [
    ("Black", (0x1F, 0x1D, 0x1A)),
    ("Red", (0xD0, 0x34, 0x2C)),
    ("Blue", (0x1F, 0x6F, 0xD1)),
    ("Green", (0x2E, 0x8B, 0x57)),
    ("Amber", (0xE0, 0x9A, 0x10)),
    ("Purple", (0x7B, 0x4F, 0xD6)),
];

/// Highlighter colours (drawn translucent).
pub const HIGHLIGHTER_COLOURS: [(&str, (u8, u8, u8)); 5] = [
    ("Yellow", (0xFF, 0xE0, 0x3D)),
    ("Green", (0x7E, 0xE0, 0x81)),
    ("Cyan", (0x5C, 0xD6, 0xF0)),
    ("Pink", (0xFF, 0x8A, 0xC2)),
    ("Orange", (0xFF, 0xAE, 0x4D)),
];

/// Stroke widths in points: pen and shapes, then highlighter.
pub const PEN_WIDTHS: [f32; 3] = [1.0, 2.0, 4.0];
pub const HIGHLIGHTER_WIDTHS: [f32; 3] = [8.0, 12.0, 18.0];
/// The highlighter's opacity.
pub const HIGHLIGHTER_OPACITY: f32 = 0.38;

/// The colour picker's grid: 12 hues in 6 shades (dark to light, even steps
/// in OKLCH so each row reads as one lightness), then a row of greys from
/// black to white.
pub const PICKER_HUES: usize = 12;
pub const PICKER_SHADES: usize = 6;

pub fn picker_grid() -> Vec<(u8, u8, u8)> {
    use crate::Rgba;
    let to_u8 = |c: Rgba| {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        (q(c.r), q(c.g), q(c.b))
    };
    let mut out = Vec::with_capacity(PICKER_HUES * (PICKER_SHADES + 1));
    for shade in 0..PICKER_SHADES {
        let l = 0.32 + 0.11 * shade as f32;
        // Pale shades can't hold much chroma; dark ones can.
        let c = 0.17 - 0.018 * shade as f32;
        for hue in 0..PICKER_HUES {
            let h = hue as f32 / PICKER_HUES as f32 * std::f32::consts::TAU;
            out.push(to_u8(Rgba::from_oklch(l, c, h)));
        }
    }
    for g in 0..PICKER_HUES {
        let v = (g as f32 / (PICKER_HUES - 1) as f32 * 255.0).round() as u8;
        out.push((v, v, v));
    }
    out
}

/// `#RGB` or `#RRGGBB` (the '#' optional) as a colour.
pub fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim().trim_start_matches('#');
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |h: &str| u8::from_str_radix(h, 16).ok();
    match s.len() {
        6 => Some((byte(&s[0..2])?, byte(&s[2..4])?, byte(&s[4..6])?)),
        3 => {
            let d = |i: usize| byte(&s[i..i + 1]).map(|v| v * 17);
            Some((d(0)?, d(1)?, d(2)?))
        }
        _ => None,
    }
}

/// Six upper-case hex digits, no '#'.
pub fn hex_of((r, g, b): (u8, u8, u8)) -> String {
    format!("{r:02X}{g:02X}{b:02X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_codes_parse_in_long_and_short_form() {
        assert_eq!(parse_hex("#2E8B57"), Some((0x2E, 0x8B, 0x57)));
        assert_eq!(parse_hex("2e8b57"), Some((0x2E, 0x8B, 0x57)));
        assert_eq!(parse_hex("#F0A"), Some((0xFF, 0x00, 0xAA)));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("#GG0000"), None);
        assert_eq!(parse_hex(""), None);
        assert_eq!(hex_of((0x2E, 0x8B, 0x57)), "2E8B57");
    }

    #[test]
    fn the_picker_grid_is_full_distinct_and_has_black_and_white() {
        let g = picker_grid();
        assert_eq!(g.len(), PICKER_HUES * (PICKER_SHADES + 1));
        assert!(g.contains(&(0, 0, 0)) && g.contains(&(255, 255, 255)));
        let mut uniq = g.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert!(
            uniq.len() >= g.len() - 2,
            "colours repeat: {}",
            g.len() - uniq.len()
        );
    }
}
