// SPDX-License-Identifier: AGPL-3.0-or-later

//! Page colour (Settings › Reading): PDFs, comics, DjVu and XPS drawn dark
//! for night reading, or in the theme's own page and text colours. Applied
//! to each rendered page off the UI thread; books and Markdown already take
//! the theme's colours.

use crate::settings::PageColour;

/// How a rendered page is recoloured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Tint {
    /// Lightness turned over, colours kept: white paper goes near black,
    /// black ink near white, a red stays red.
    Dark,
    /// White paper becomes `paper`, black ink becomes `ink`, the greys
    /// between in proportion; colours keep their hue.
    Theme { paper: [u8; 3], ink: [u8; 3] },
}

impl Tint {
    pub(crate) fn from_setting(c: PageColour, paper: [u8; 3], ink: [u8; 3]) -> Option<Tint> {
        match c {
            PageColour::Original => None,
            PageColour::Dark => Some(Tint::Dark),
            PageColour::Theme => Some(Tint::Theme { paper, ink }),
        }
    }

    /// Recolour BGRA pixels in place.
    pub(crate) fn apply(self, bgra: &mut [u8]) {
        for p in bgra.as_chunks_mut::<4>().0 {
            let (b, g, r) = (p[0] as i32, p[1] as i32, p[2] as i32);
            let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
            match self {
                Tint::Dark => {
                    // HSL lightness L becomes 1 - L, hue and saturation
                    // kept; then lifted off pure black, which glares less.
                    let shift = 255 - hi - lo;
                    let soft = |c: i32| (18 + (c + shift).clamp(0, 255) * 220 / 255) as u8;
                    p[0] = soft(b);
                    p[1] = soft(g);
                    p[2] = soft(r);
                }
                Tint::Theme { paper, ink } => {
                    // How light the pixel is (0 ink .. 255 paper), and how far
                    // each channel stands from its grey (its colour).
                    let l = (hi + lo) / 2;
                    let mix =
                        |k: usize| ink[k] as i32 + (paper[k] as i32 - ink[k] as i32) * l / 255;
                    let out = [mix(2) + (b - l), mix(1) + (g - l), mix(0) + (r - l)];
                    p[0] = out[0].clamp(0, 255) as u8;
                    p[1] = out[1].clamp(0, 255) as u8;
                    p[2] = out[2].clamp(0, 255) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(t: Tint, rgb: [u8; 3]) -> [u8; 3] {
        let mut p = [rgb[2], rgb[1], rgb[0], 255];
        t.apply(&mut p);
        [p[2], p[1], p[0]]
    }

    #[test]
    fn dark_turns_paper_dark_and_ink_light_and_keeps_colour() {
        let white = px(Tint::Dark, [255, 255, 255]);
        let black = px(Tint::Dark, [0, 0, 0]);
        assert!(white[0] < 40 && black[0] > 200, "{white:?} {black:?}");
        let red = px(Tint::Dark, [220, 30, 30]);
        assert!(red[0] > red[1] + 100, "red stays red: {red:?}");
    }

    #[test]
    fn theme_maps_paper_and_ink() {
        let t = Tint::Theme {
            paper: [250, 240, 220],
            ink: [40, 30, 20],
        };
        assert_eq!(px(t, [255, 255, 255]), [250, 240, 220]);
        assert_eq!(px(t, [0, 0, 0]), [40, 30, 20]);
        let mut alpha = [0, 0, 0, 128];
        t.apply(&mut alpha);
        assert_eq!(alpha[3], 128);
    }
}
