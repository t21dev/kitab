// SPDX-License-Identifier: AGPL-3.0-or-later

//! Page points ↔ logical pixels on a page sheet, for find hits, links and text
//! selection. Page points come from the engine with the page's own /Rotate
//! applied; `rotation` is the reader's extra rotation, clockwise, the same
//! transform the renderer uses (rotate, then move back to the origin).

use kitab_core::{RectPt, SizePt};

/// A point in page points → pixels from the sheet's top-left.
pub fn point_to_view(x: f32, y: f32, page: SizePt, rotation: u16) -> (f32, f32) {
    match rotation % 360 {
        90 => (page.h - y, x),
        180 => (page.w - x, page.h - y),
        270 => (y, page.w - x),
        _ => (x, y),
    }
}

/// A rectangle in page points (of an unrotated page of size `page`) → a
/// rectangle in logical pixels on the sheet at `ppt` pixels per point.
pub fn rect_to_view(r: RectPt, page: SizePt, rotation: u16, ppt: f32) -> RectPt {
    let (ax, ay) = point_to_view(r.x, r.y, page, rotation);
    let (bx, by) = point_to_view(r.x + r.w, r.y + r.h, page, rotation);
    RectPt {
        x: ax.min(bx) * ppt,
        y: ay.min(by) * ppt,
        w: (ax - bx).abs() * ppt,
        h: (ay - by).abs() * ppt,
    }
}

/// Pixels from the sheet's top-left → page points.
pub fn view_to_point(u: f32, v: f32, page: SizePt, rotation: u16, ppt: f32) -> (f32, f32) {
    let (u, v) = (u / ppt.max(0.0001), v / ppt.max(0.0001));
    match rotation % 360 {
        90 => (v, page.h - u),
        180 => (page.w - u, page.h - v),
        270 => (page.w - v, u),
        _ => (u, v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: SizePt = SizePt { w: 600.0, h: 800.0 };

    #[test]
    fn unrotated_scales() {
        let r = rect_to_view(
            RectPt {
                x: 10.0,
                y: 20.0,
                w: 30.0,
                h: 40.0,
            },
            PAGE,
            0,
            2.0,
        );
        assert_eq!(
            r,
            RectPt {
                x: 20.0,
                y: 40.0,
                w: 60.0,
                h: 80.0
            }
        );
    }

    #[test]
    fn quarter_turn_moves_the_top_left_to_the_top_right() {
        // A box at the page's top-left ends up at the top-right after 90° clockwise.
        let r = rect_to_view(
            RectPt {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 50.0,
            },
            PAGE,
            90,
            1.0,
        );
        assert_eq!(
            r,
            RectPt {
                x: 750.0,
                y: 0.0,
                w: 50.0,
                h: 100.0
            }
        );
    }

    #[test]
    fn points_round_trip_at_every_rotation() {
        for rotation in [0, 90, 180, 270] {
            let r = rect_to_view(
                RectPt {
                    x: 123.0,
                    y: 456.0,
                    w: 0.0,
                    h: 0.0,
                },
                PAGE,
                rotation,
                1.5,
            );
            let (x, y) = view_to_point(r.x, r.y, PAGE, rotation, 1.5);
            assert!(
                (x - 123.0).abs() < 0.01 && (y - 456.0).abs() < 0.01,
                "{rotation}°"
            );
        }
    }
}
