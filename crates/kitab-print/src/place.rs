// SPDX-License-Identifier: AGPL-3.0-or-later

//! Where a page lands on the paper: turned to match it (auto-rotate), sized
//! by the scaling choice, centred in the printable area.

use crate::{MAX_DPI, Paper};

/// How a page is sized on the paper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scale {
    /// As large as fits.
    #[default]
    Fit,
    /// Its own size; anything past the printable area is cut.
    Actual,
    /// Its own size, unless that doesn't fit: then as large as fits.
    Shrink,
}

/// A page on the paper.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Turn the page a quarter clockwise first (a landscape page on portrait
    /// paper, or the other way round).
    pub rotate: bool,
    /// The drawn page in device pixels: left, top, width, height.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Pixels per point to render the page at (the device's, capped).
    pub render_scale: f32,
}

/// Place a page of `w_pt` × `h_pt` points on `paper`.
pub fn place(w_pt: f32, h_pt: f32, paper: Paper, scale: Scale, auto_rotate: bool) -> Placement {
    let w_pt = w_pt.max(1.0);
    let h_pt = h_pt.max(1.0);
    let landscape_page = w_pt > h_pt;
    let landscape_paper = paper.width_px > paper.height_px;
    let rotate = auto_rotate && landscape_page != landscape_paper;
    let (pw, ph) = if rotate { (h_pt, w_pt) } else { (w_pt, h_pt) };
    // Device pixels per point at the page's own size.
    let (kx, ky) = (paper.dpi_x / 72.0, paper.dpi_y / 72.0);
    let natural = (pw * kx, ph * ky);
    let fit = (paper.width_px / natural.0).min(paper.height_px / natural.1);
    let factor = match scale {
        Scale::Fit => fit,
        Scale::Actual => 1.0,
        Scale::Shrink => fit.min(1.0),
    };
    let (w, h) = (natural.0 * factor, natural.1 * factor);
    let x = (paper.width_px - w) / 2.0;
    let y = (paper.height_px - h) / 2.0;
    // Render at the device's resolution for the drawn size, but no finer
    // than MAX_DPI.
    let device_ppt = (w / pw).max(h / ph);
    let render_scale = device_ppt.min(MAX_DPI / 72.0 * factor.max(0.01)).max(0.1);
    Placement {
        rotate,
        x,
        y,
        w,
        h,
        render_scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A4 paper at 600 dpi, all of it printable.
    const A4_600: Paper = Paper {
        width_px: 8.27 * 600.0,
        height_px: 11.69 * 600.0,
        dpi_x: 600.0,
        dpi_y: 600.0,
    };
    const A4: (f32, f32) = (595.0, 842.0);

    #[test]
    fn a_portrait_page_fills_portrait_paper() {
        let p = place(A4.0, A4.1, A4_600, Scale::Fit, true);
        assert!(!p.rotate);
        assert!((p.w - A4_600.width_px).abs() < 30.0, "{p:?}");
        // Centred.
        assert!((p.x - (A4_600.width_px - p.w) / 2.0).abs() < 0.01);
        // Rendered at no more than 300 dpi.
        assert!(p.render_scale <= MAX_DPI / 72.0 + 0.01, "{p:?}");
    }

    #[test]
    fn a_landscape_page_turns_on_portrait_paper() {
        let p = place(A4.1, A4.0, A4_600, Scale::Fit, true);
        assert!(p.rotate);
        assert!(p.h > p.w, "drawn upright on the paper: {p:?}");
        let kept = place(A4.1, A4.0, A4_600, Scale::Fit, false);
        assert!(!kept.rotate);
        assert!(kept.w > kept.h);
    }

    #[test]
    fn actual_size_and_shrink() {
        // An A5 page on A4: actual keeps its size, shrink too, fit grows it.
        let a5 = (420.0, 595.0);
        let actual = place(a5.0, a5.1, A4_600, Scale::Actual, true);
        assert!((actual.w - 420.0 / 72.0 * 600.0).abs() < 0.5);
        assert_eq!(place(a5.0, a5.1, A4_600, Scale::Shrink, true), actual);
        assert!(place(a5.0, a5.1, A4_600, Scale::Fit, true).w > actual.w);
        // An A3 page on A4: shrink makes it fit, actual doesn't.
        let a3 = (842.0, 1191.0);
        let shrunk = place(a3.0, a3.1, A4_600, Scale::Shrink, true);
        assert!(shrunk.w <= A4_600.width_px + 0.5 && shrunk.h <= A4_600.height_px + 0.5);
        assert!(place(a3.0, a3.1, A4_600, Scale::Actual, true).w > A4_600.width_px);
    }

    #[test]
    fn nonsense_sizes_stay_finite() {
        let p = place(0.0, f32::NAN.max(0.0), A4_600, Scale::Fit, true);
        assert!(p.w.is_finite() && p.h.is_finite() && p.render_scale > 0.0);
    }
}
