// SPDX-License-Identifier: AGPL-3.0-or-later

//! Colour parsing, WCAG contrast and OKLCH lightness adjustment.

use std::fmt;

/// An sRGB colour with straight (non-premultiplied) alpha, each channel 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Rgba {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: 1.0,
        }
    }

    /// Parse `#RGB`, `#RRGGBB`, `#RRGGBBAA`, `rgb(r,g,b)` or `rgba(r,g,b,a)`.
    pub fn parse(s: &str) -> Option<Rgba> {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix('#') {
            return parse_hex(hex);
        }
        let inner = s
            .strip_prefix("rgba(")
            .or_else(|| s.strip_prefix("rgb("))?
            .strip_suffix(')')?;
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() != 3 && parts.len() != 4 {
            return None;
        }
        let channel = |p: &str| -> Option<f32> {
            let v: f32 = p.parse().ok()?;
            (0.0..=255.0).contains(&v).then_some(v / 255.0)
        };
        let a = match parts.get(3) {
            Some(p) => {
                let v: f32 = p.parse().ok()?;
                if !(0.0..=1.0).contains(&v) {
                    return None;
                }
                v
            }
            None => 1.0,
        };
        Some(Rgba {
            r: channel(parts[0])?,
            g: channel(parts[1])?,
            b: channel(parts[2])?,
            a,
        })
    }

    /// Composite this colour over an opaque background.
    pub fn over(self, bg: Rgba) -> Rgba {
        let mix = |f: f32, b: f32| f * self.a + b * (1.0 - self.a);
        Rgba {
            r: mix(self.r, bg.r),
            g: mix(self.g, bg.g),
            b: mix(self.b, bg.b),
            a: 1.0,
        }
    }

    /// WCAG 2.x relative luminance of the opaque colour.
    pub fn luminance(self) -> f32 {
        0.2126 * srgb_to_linear(self.r)
            + 0.7152 * srgb_to_linear(self.g)
            + 0.0722 * srgb_to_linear(self.b)
    }

    /// OKLCH lightness, chroma and hue (radians).
    pub fn to_oklch(self) -> (f32, f32, f32) {
        let (l, a, b) = linear_to_oklab(
            srgb_to_linear(self.r),
            srgb_to_linear(self.g),
            srgb_to_linear(self.b),
        );
        (l, (a * a + b * b).sqrt(), b.atan2(a))
    }

    /// Build an opaque colour from OKLCH, reducing chroma until it fits sRGB.
    pub fn from_oklch(l: f32, c: f32, h: f32) -> Rgba {
        let mut chroma = c;
        loop {
            let (r, g, b) = oklab_to_linear(l, chroma * h.cos(), chroma * h.sin());
            let in_gamut = [r, g, b].iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v));
            if in_gamut || chroma <= 1e-4 {
                return Rgba {
                    r: linear_to_srgb(r.clamp(0.0, 1.0)),
                    g: linear_to_srgb(g.clamp(0.0, 1.0)),
                    b: linear_to_srgb(b.clamp(0.0, 1.0)),
                    a: 1.0,
                };
            }
            chroma *= 0.95;
        }
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let (r, g, b) = (q(self.r), q(self.g), q(self.b));
        if self.a >= 1.0 {
            write!(f, "#{r:02X}{g:02X}{b:02X}")
        } else {
            write!(f, "#{r:02X}{g:02X}{b:02X}{:02X}", q(self.a))
        }
    }
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    let nib = |i: usize| {
        u8::from_str_radix(hex.get(i..i + 1)?, 16)
            .ok()
            .map(|v| v * 17)
    };
    let (r, g, b, a) = match hex.len() {
        3 => (nib(0)?, nib(1)?, nib(2)?, 255),
        6 => (byte(0)?, byte(2)?, byte(4)?, 255),
        8 => (byte(0)?, byte(2)?, byte(4)?, byte(6)?),
        _ => return None,
    };
    Some(Rgba {
        a: a as f32 / 255.0,
        ..Rgba::rgb(r, g, b)
    })
}

/// WCAG contrast ratio between a foreground and an opaque background.
/// A translucent foreground is composited over the background first.
pub fn contrast_ratio(fg: Rgba, bg: Rgba) -> f32 {
    let fg = fg.over(bg);
    let (a, b) = (fg.luminance(), bg.luminance());
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

/// Keep the hue and chroma of `accent` and move its OKLCH lightness away from
/// `bg` until it reaches `target` contrast (DESIGN.md § Accent). Returns the
/// colour unchanged (made opaque) when it already passes.
pub fn accent_for_contrast(accent: Rgba, bg: Rgba, target: f32) -> Rgba {
    let accent = Rgba { a: 1.0, ..accent };
    if contrast_ratio(accent, bg) >= target {
        return accent;
    }
    let (l0, c, h) = accent.to_oklch();
    // Darken on light backgrounds, lighten on dark ones.
    let darken = bg.luminance() > 0.18;
    let (mut lo, mut hi) = if darken { (0.0, l0) } else { (l0, 1.0) };
    // Bisect for the passing lightness closest to the original.
    for _ in 0..32 {
        let mid = (lo + hi) / 2.0;
        let passes = contrast_ratio(Rgba::from_oklch(mid, c, h), bg) >= target;
        if darken == passes {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Rgba::from_oklch(if darken { lo } else { hi }, c, h)
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

// OKLab matrices from Björn Ottosson's reference implementation.
fn linear_to_oklab(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    (
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    )
}

fn oklab_to_linear(l: f32, a: f32, b: f32) -> (f32, f32, f32) {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    (
        4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_,
        -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_rgba() {
        assert_eq!(Rgba::parse("#fff"), Some(Rgba::rgb(255, 255, 255)));
        assert_eq!(Rgba::parse("#141415"), Some(Rgba::rgb(0x14, 0x14, 0x15)));
        let c = Rgba::parse("rgba(230,180,80,.30)").unwrap();
        assert!((c.a - 0.30).abs() < 1e-6);
        assert_eq!(Rgba::parse("#12345"), None);
        assert_eq!(Rgba::parse("rgba(300,0,0,1)"), None);
        assert_eq!(Rgba::parse("red"), None);
    }

    #[test]
    fn known_contrast_values() {
        let black = Rgba::rgb(0, 0, 0);
        let white = Rgba::rgb(255, 255, 255);
        assert!((contrast_ratio(black, white) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 1e-6);
        // #767676 on white is the classic 4.54:1.
        let grey = Rgba::rgb(0x76, 0x76, 0x76);
        assert!((contrast_ratio(grey, white) - 4.54).abs() < 0.01);
    }

    #[test]
    fn oklch_round_trip() {
        for c in [
            Rgba::rgb(0xE6, 0xB4, 0x50),
            Rgba::rgb(0x88, 0xC0, 0xD0),
            Rgba::rgb(0x12, 0x34, 0x56),
        ] {
            let (l, ch, h) = c.to_oklch();
            assert_eq!(Rgba::from_oklch(l, ch, h).to_string(), c.to_string());
        }
    }

    #[test]
    fn accent_adjustment_reaches_target_and_keeps_passing_colours() {
        let white = Rgba::rgb(255, 255, 255);
        let dark = Rgba::rgb(0x14, 0x14, 0x15);
        let yellow = Rgba::rgb(0xFF, 0xD7, 0x00);
        let on_white = accent_for_contrast(yellow, white, 4.5);
        assert!(contrast_ratio(on_white, white) >= 4.5);
        let navy = Rgba::rgb(0x10, 0x20, 0x60);
        let on_dark = accent_for_contrast(navy, dark, 3.0);
        assert!(contrast_ratio(on_dark, dark) >= 3.0);
        assert_eq!(accent_for_contrast(yellow, dark, 3.0), yellow);
    }
}
