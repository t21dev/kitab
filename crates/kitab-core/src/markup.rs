// SPDX-License-Identifier: AGPL-3.0-or-later

//! Markup a reader adds to a page (PLAN.md Phase 6): pen and highlighter
//! strokes, lines and arrows, rectangles and ellipses, text highlights,
//! underlines and strike-throughs, text boxes and notes. Coordinates are page
//! points with the origin at the top left, the same space as text boxes and
//! links, so a mark lines up with what MuPDF draws.

use crate::RectPt;

/// A point in page points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PointPt {
    pub x: f32,
    pub y: f32,
}

/// An sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MarkShape {
    /// Pen or highlighter: one or more strokes.
    Ink {
        strokes: Vec<Vec<PointPt>>,
    },
    /// A straight line, with an arrowhead at `to` for an arrow.
    Line {
        from: PointPt,
        to: PointPt,
        arrow: bool,
    },
    Rect {
        rect: RectPt,
    },
    Ellipse {
        rect: RectPt,
    },
    /// Text markup over these boxes (one per line of selected text).
    Highlight {
        rects: Vec<RectPt>,
    },
    Underline {
        rects: Vec<RectPt>,
    },
    Strike {
        rects: Vec<RectPt>,
    },
    /// Text written on the page.
    Text {
        rect: RectPt,
        text: String,
        size: f32,
    },
    /// A note pinned at a point, its text shown on demand.
    Note {
        at: PointPt,
        text: String,
    },
}

/// One mark on one page.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// 0-based.
    pub page: u32,
    pub shape: MarkShape,
    pub color: Rgb8,
    /// Stroke width in points (ignored by text markup and notes).
    pub width: f32,
    /// 0.05..=1 (a highlighter pen is about 0.4).
    pub opacity: f32,
}

/// An annotation already in a PDF (listed in the Marks panel).
#[derive(Debug, Clone, PartialEq)]
pub struct FileAnnot {
    pub page: u32,
    /// The PDF subtype: "Highlight", "Ink", "Text", "FreeText", …
    pub kind: String,
    /// Its text (a note's or a text box's contents), if any.
    pub text: Option<String>,
    pub rect: RectPt,
    pub color: Option<Rgb8>,
}

/// Limits a mark must fit: anything bigger is a bug or a hostile store row.
pub const MAX_STROKES: usize = 1_000;
pub const MAX_POINTS: usize = 20_000;
pub const MAX_RECTS: usize = 2_000;
pub const MAX_TEXT_CHARS: usize = 20_000;
pub const WIDTH_RANGE: std::ops::RangeInclusive<f32> = 0.25..=48.0;
pub const OPACITY_RANGE: std::ops::RangeInclusive<f32> = 0.05..=1.0;
/// Coordinates stay within this many points of the origin.
const MAX_COORD: f32 = 100_000.0;

fn ok_coord(v: f32) -> bool {
    v.is_finite() && v.abs() <= MAX_COORD
}

fn ok_point(p: &PointPt) -> bool {
    ok_coord(p.x) && ok_coord(p.y)
}

fn ok_rect(r: &RectPt) -> bool {
    ok_coord(r.x) && ok_coord(r.y) && ok_coord(r.w) && ok_coord(r.h) && r.w >= 0.0 && r.h >= 0.0
}

/// The rectangle spanning two corners, in either order.
pub fn rect_between(a: PointPt, b: PointPt) -> RectPt {
    RectPt {
        x: a.x.min(b.x),
        y: a.y.min(b.y),
        w: (a.x - b.x).abs(),
        h: (a.y - b.y).abs(),
    }
}

fn union(rects: impl IntoIterator<Item = RectPt>) -> Option<RectPt> {
    let mut it = rects.into_iter();
    let first = it.next()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x + first.w, first.y + first.h);
    for r in it {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x + r.w);
        y1 = y1.max(r.y + r.h);
    }
    Some(RectPt {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

/// Distance from `p` to the segment `a`–`b`.
fn segment_distance(p: PointPt, a: PointPt, b: PointPt) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (cx, cy) = (a.x + t * dx, a.y + t * dy);
    ((p.x - cx).powi(2) + (p.y - cy).powi(2)).sqrt()
}

fn in_rect(p: PointPt, r: &RectPt, pad: f32) -> bool {
    p.x >= r.x - pad && p.x <= r.x + r.w + pad && p.y >= r.y - pad && p.y <= r.y + r.h + pad
}

impl Mark {
    /// Within the limits, finite, and with something to draw.
    pub fn is_valid(&self) -> bool {
        if !WIDTH_RANGE.contains(&self.width) || !OPACITY_RANGE.contains(&self.opacity) {
            return false;
        }
        match &self.shape {
            MarkShape::Ink { strokes } => {
                !strokes.is_empty()
                    && strokes.len() <= MAX_STROKES
                    && strokes.iter().map(Vec::len).sum::<usize>() <= MAX_POINTS
                    && strokes
                        .iter()
                        .all(|s| !s.is_empty() && s.iter().all(ok_point))
            }
            MarkShape::Line { from, to, .. } => ok_point(from) && ok_point(to),
            MarkShape::Rect { rect } | MarkShape::Ellipse { rect } => {
                ok_rect(rect) && (rect.w > 0.0 || rect.h > 0.0)
            }
            MarkShape::Highlight { rects }
            | MarkShape::Underline { rects }
            | MarkShape::Strike { rects } => {
                !rects.is_empty() && rects.len() <= MAX_RECTS && rects.iter().all(ok_rect)
            }
            MarkShape::Text { rect, text, size } => {
                ok_rect(rect)
                    && text.chars().count() <= MAX_TEXT_CHARS
                    && size.is_finite()
                    && (4.0..=144.0).contains(size)
            }
            MarkShape::Note { at, text } => ok_point(at) && text.chars().count() <= MAX_TEXT_CHARS,
        }
    }

    /// The area the mark covers, stroke included.
    pub fn bounds(&self) -> RectPt {
        let pad = self.width / 2.0;
        let grow = |r: RectPt, by: f32| RectPt {
            x: r.x - by,
            y: r.y - by,
            w: r.w + 2.0 * by,
            h: r.h + 2.0 * by,
        };
        let point_box = |p: &PointPt| RectPt {
            x: p.x,
            y: p.y,
            w: 0.0,
            h: 0.0,
        };
        match &self.shape {
            MarkShape::Ink { strokes } => grow(
                union(strokes.iter().flatten().map(point_box)).unwrap_or_default(),
                pad,
            ),
            MarkShape::Line { from, to, arrow } => {
                // An arrowhead reaches out about four widths.
                let head = if *arrow { 4.0 * self.width } else { 0.0 };
                grow(rect_between(*from, *to), pad + head)
            }
            MarkShape::Rect { rect } | MarkShape::Ellipse { rect } => grow(*rect, pad),
            MarkShape::Highlight { rects }
            | MarkShape::Underline { rects }
            | MarkShape::Strike { rects } => union(rects.iter().copied()).unwrap_or_default(),
            MarkShape::Text { rect, .. } => *rect,
            MarkShape::Note { at, .. } => RectPt {
                x: at.x,
                y: at.y,
                w: 18.0,
                h: 18.0,
            },
        }
    }

    /// Whether a click at `p` (the eraser, or selecting) touches the mark,
    /// within `tolerance` points of its stroke or inside its area.
    pub fn hits(&self, p: PointPt, tolerance: f32) -> bool {
        let reach = tolerance + self.width / 2.0;
        match &self.shape {
            MarkShape::Ink { strokes } => strokes.iter().any(|s| match s.as_slice() {
                [only] => segment_distance(p, *only, *only) <= reach,
                pts => pts
                    .windows(2)
                    .any(|w| segment_distance(p, w[0], w[1]) <= reach),
            }),
            MarkShape::Line { from, to, .. } => segment_distance(p, *from, *to) <= reach,
            MarkShape::Rect { rect } => {
                // The outline, not the inside: a frame round a paragraph
                // shouldn't swallow clicks on the text.
                let corners = [
                    PointPt {
                        x: rect.x,
                        y: rect.y,
                    },
                    PointPt {
                        x: rect.x + rect.w,
                        y: rect.y,
                    },
                    PointPt {
                        x: rect.x + rect.w,
                        y: rect.y + rect.h,
                    },
                    PointPt {
                        x: rect.x,
                        y: rect.y + rect.h,
                    },
                ];
                (0..4).any(|i| segment_distance(p, corners[i], corners[(i + 1) % 4]) <= reach)
            }
            MarkShape::Ellipse { rect } => {
                let (rx, ry) = (rect.w / 2.0, rect.h / 2.0);
                if rx <= 0.0 || ry <= 0.0 {
                    return in_rect(p, rect, reach);
                }
                let (cx, cy) = (rect.x + rx, rect.y + ry);
                let d = (((p.x - cx) / rx).powi(2) + ((p.y - cy) / ry).powi(2)).sqrt();
                // Distance to the outline, approximately, in points.
                (d - 1.0).abs() * rx.min(ry) <= reach
            }
            MarkShape::Highlight { rects }
            | MarkShape::Underline { rects }
            | MarkShape::Strike { rects } => rects.iter().any(|r| in_rect(p, r, tolerance)),
            MarkShape::Text { rect, .. } => in_rect(p, rect, tolerance),
            MarkShape::Note { .. } => in_rect(p, &self.bounds(), tolerance),
        }
    }

    /// A short name for lists and the store: "ink", "line", "arrow", …
    pub fn kind(&self) -> &'static str {
        match &self.shape {
            MarkShape::Ink { .. } => "ink",
            MarkShape::Line { arrow: false, .. } => "line",
            MarkShape::Line { arrow: true, .. } => "arrow",
            MarkShape::Rect { .. } => "rect",
            MarkShape::Ellipse { .. } => "ellipse",
            MarkShape::Highlight { .. } => "highlight",
            MarkShape::Underline { .. } => "underline",
            MarkShape::Strike { .. } => "strike",
            MarkShape::Text { .. } => "text",
            MarkShape::Note { .. } => "note",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32) -> PointPt {
        PointPt { x, y }
    }

    fn mark(shape: MarkShape) -> Mark {
        Mark {
            page: 0,
            shape,
            color: Rgb8 {
                r: 200,
                g: 40,
                b: 40,
            },
            width: 2.0,
            opacity: 1.0,
        }
    }

    #[test]
    fn validity_checks_limits_and_numbers() {
        let ok = mark(MarkShape::Line {
            from: pt(0.0, 0.0),
            to: pt(10.0, 10.0),
            arrow: true,
        });
        assert!(ok.is_valid());
        let mut bad = ok.clone();
        bad.width = f32::NAN;
        assert!(!bad.is_valid());
        bad = ok.clone();
        bad.opacity = 0.0;
        assert!(!bad.is_valid());
        assert!(!mark(MarkShape::Ink { strokes: vec![] }).is_valid());
        assert!(
            !mark(MarkShape::Ink {
                strokes: vec![vec![pt(f32::INFINITY, 0.0)]]
            })
            .is_valid()
        );
        assert!(!mark(MarkShape::Highlight { rects: vec![] }).is_valid());
        let r = RectPt {
            x: 0.0,
            y: 0.0,
            w: -1.0,
            h: 5.0,
        };
        assert!(!mark(MarkShape::Rect { rect: r }).is_valid());
        assert!(
            mark(MarkShape::Note {
                at: pt(5.0, 5.0),
                text: "hi".into()
            })
            .is_valid()
        );
    }

    #[test]
    fn hits_follow_the_stroke_not_the_box() {
        let line = mark(MarkShape::Line {
            from: pt(0.0, 0.0),
            to: pt(100.0, 0.0),
            arrow: false,
        });
        assert!(line.hits(pt(50.0, 2.0), 2.0));
        assert!(!line.hits(pt(50.0, 10.0), 2.0));
        let rect = mark(MarkShape::Rect {
            rect: RectPt {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 50.0,
            },
        });
        assert!(rect.hits(pt(0.5, 25.0), 2.0), "on the left edge");
        assert!(!rect.hits(pt(50.0, 25.0), 2.0), "the middle is free");
        let ellipse = mark(MarkShape::Ellipse {
            rect: RectPt {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
        });
        assert!(ellipse.hits(pt(100.0, 50.0), 2.0));
        assert!(!ellipse.hits(pt(50.0, 50.0), 2.0));
        let ink = mark(MarkShape::Ink {
            strokes: vec![vec![pt(0.0, 0.0), pt(10.0, 10.0)]],
        });
        assert!(ink.hits(pt(5.0, 5.0), 1.0));
        let dot = mark(MarkShape::Ink {
            strokes: vec![vec![pt(3.0, 3.0)]],
        });
        assert!(dot.hits(pt(3.5, 3.0), 1.0));
    }

    #[test]
    fn bounds_cover_the_stroke_and_arrowhead() {
        let arrow = mark(MarkShape::Line {
            from: pt(10.0, 10.0),
            to: pt(20.0, 10.0),
            arrow: true,
        });
        let b = arrow.bounds();
        assert!(b.x <= 10.0 - 8.0 && b.x + b.w >= 20.0 + 8.0);
        let r = rect_between(pt(5.0, 9.0), pt(1.0, 2.0));
        assert_eq!(
            r,
            RectPt {
                x: 1.0,
                y: 2.0,
                w: 4.0,
                h: 7.0
            }
        );
    }
}
