// SPDX-License-Identifier: AGPL-3.0-or-later

//! Write a reader's marks into a PDF as standard annotations (PLAN.md
//! Phase 6), so every PDF reader shows them: pen and highlighter as Ink,
//! lines and arrows as Line, shapes as Square and Circle, text markup as
//! Highlight, Underline and StrikeOut, text as FreeText and notes as Text.
//!
//! The source is copied and the copy saved incrementally (the changes are
//! appended), so the original bytes, digital signatures and history stay as
//! they were. The copy then replaces `out` in one rename.

use kitab_core::{Mark, MarkShape, PointPt, RectPt, Rgb8};
use mupdf::color::AnnotationColor;
use mupdf::pdf::{LineEndingStyle, PdfAnnotation, PdfAnnotationType, PdfDocument, PdfWriteOptions};
use mupdf::{Point, Quad, Rect};
use std::path::{Path, PathBuf};

fn p(pt: &PointPt) -> Point {
    Point::new(pt.x, pt.y)
}

fn r(rect: &RectPt) -> Rect {
    Rect::new(rect.x, rect.y, rect.x + rect.w, rect.y + rect.h)
}

fn colour(c: Rgb8) -> AnnotationColor {
    AnnotationColor::Rgb {
        red: f32::from(c.r) / 255.0,
        green: f32::from(c.g) / 255.0,
        blue: f32::from(c.b) / 255.0,
    }
}

fn subtype(shape: &MarkShape) -> PdfAnnotationType {
    match shape {
        MarkShape::Ink { .. } => PdfAnnotationType::Ink,
        MarkShape::Line { .. } => PdfAnnotationType::Line,
        MarkShape::Rect { .. } => PdfAnnotationType::Square,
        MarkShape::Ellipse { .. } => PdfAnnotationType::Circle,
        MarkShape::Highlight { .. } => PdfAnnotationType::Highlight,
        MarkShape::Underline { .. } => PdfAnnotationType::Underline,
        MarkShape::Strike { .. } => PdfAnnotationType::StrikeOut,
        MarkShape::Text { .. } => PdfAnnotationType::FreeText,
        MarkShape::Note { .. } => PdfAnnotationType::Text,
    }
}

fn fill(a: &mut PdfAnnotation, m: &Mark) -> Result<(), mupdf::Error> {
    let c = colour(m.color);
    match &m.shape {
        MarkShape::Ink { strokes } => {
            a.set_ink_list(strokes.iter().map(|s| s.iter().map(p).collect::<Vec<_>>()))?;
            a.set_border_width(m.width)?;
            a.set_color(c)?;
        }
        MarkShape::Line { from, to, arrow } => {
            a.set_line(p(from), p(to))?;
            if *arrow {
                a.set_line_ending_styles(LineEndingStyle::None, LineEndingStyle::OpenArrow)?;
            }
            a.set_border_width(m.width)?;
            a.set_color(c)?;
        }
        MarkShape::Rect { rect } | MarkShape::Ellipse { rect } => {
            a.set_rect(r(rect))?;
            a.set_border_width(m.width)?;
            a.set_color(c)?;
        }
        MarkShape::Highlight { rects }
        | MarkShape::Underline { rects }
        | MarkShape::Strike { rects } => {
            for rect in rects {
                a.add_quad_point(Quad::from(r(rect)))?;
            }
            a.set_color(c)?;
        }
        MarkShape::Text { rect, text, size } => {
            a.set_rect(r(rect))?;
            a.set_contents(text)?;
            a.set_default_appearance("Helv", *size, Some(c))?;
        }
        MarkShape::Note { at, text } => {
            a.set_rect(Rect::new(at.x, at.y, at.x + 18.0, at.y + 18.0))?;
            a.set_contents(text)?;
            a.set_icon_name("Note")?;
            a.set_color(c)?;
        }
    }
    a.set_opacity(m.opacity)?;
    a.update()?;
    Ok(())
}

/// Write `marks` into a copy of the PDF at `src`, apply `edits` to the
/// annotations it already has, and put it at `out` (which may be `src`
/// itself). Returns how many changes were made. Nothing is changed if any
/// mark is invalid or a page or annotation doesn't exist.
pub fn write_marks(
    src: &Path,
    password: Option<&str>,
    marks: &[Mark],
    edits: &[kitab_core::AnnotEdit],
    out: &Path,
) -> Result<usize, String> {
    if marks.is_empty() && edits.is_empty() {
        return Err("there are no marks to save".into());
    }
    if marks.iter().any(|m| !m.is_valid()) {
        return Err("a mark is damaged".into());
    }
    // Work on a copy beside the target, so a failure leaves both files alone
    // and the rename at the end is on one volume.
    let tmp: PathBuf = out.with_extension("pdf.kitab-tmp");
    std::fs::copy(src, &tmp).map_err(|e| format!("could not copy the PDF: {e}"))?;
    let result = annotate(&tmp, password, marks, edits);
    match result {
        Ok(n) => std::fs::rename(&tmp, out).map(|_| n).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("could not save the PDF: {e}")
        }),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn annotate(
    tmp: &Path,
    password: Option<&str>,
    marks: &[Mark],
    edits: &[kitab_core::AnnotEdit],
) -> Result<usize, String> {
    let name = tmp.to_str().ok_or("the file name is not valid Unicode")?;
    let mut doc = PdfDocument::open(name).map_err(|e| format!("could not open the PDF: {e}"))?;
    if doc.needs_password().map_err(|e| e.to_string())? {
        let ok = password.is_some_and(|pw| doc.authenticate(pw).unwrap_or(false));
        if !ok {
            return Err("this PDF needs its password".into());
        }
    }
    let pages = doc.page_count().map_err(|e| e.to_string())?;
    if marks.iter().any(|m| i64::from(m.page) >= i64::from(pages)) {
        return Err("a mark is on a page the PDF doesn't have".into());
    }
    if edits
        .iter()
        .any(|e| i64::from(e.page()) >= i64::from(pages))
    {
        return Err("an annotation is on a page the PDF doesn't have".into());
    }
    let mut written = 0;
    // Deletions first, page by page, highest index first so the others keep
    // their places (new marks are appended after).
    let mut gone: Vec<(u32, u32)> = edits
        .iter()
        .map(|e| match e {
            kitab_core::AnnotEdit::Delete { page, index } => (*page, *index),
        })
        .collect();
    gone.sort_unstable();
    gone.dedup();
    for group in gone.chunk_by(|a, b| a.0 == b.0) {
        let index = i32::try_from(group[0].0).map_err(|e| e.to_string())?;
        let mut page = doc.load_pdf_page(index).map_err(|e| e.to_string())?;
        let mut annots: Vec<Option<_>> = page.annotations().map(Some).collect();
        for &(_, n) in group.iter().rev() {
            let annot = annots
                .get_mut(n as usize)
                .and_then(Option::take)
                .ok_or("an annotation to delete isn't in the PDF any more")?;
            page.delete_annotation(annot)
                .map_err(|e| format!("could not delete an annotation: {e}"))?;
            written += 1;
        }
    }
    // One page at a time, in order.
    let mut order: Vec<&Mark> = marks.iter().collect();
    order.sort_by_key(|m| m.page);
    for group in order.chunk_by(|a, b| a.page == b.page) {
        let index = i32::try_from(group[0].page).map_err(|e| e.to_string())?;
        let mut page = doc.load_pdf_page(index).map_err(|e| e.to_string())?;
        for m in group {
            let mut annot = page
                .create_annotation(subtype(&m.shape))
                .map_err(|e| format!("could not add a {}: {e}", m.kind()))?;
            fill(&mut annot, m).map_err(|e| format!("could not add a {}: {e}", m.kind()))?;
            written += 1;
        }
    }
    let mut options = PdfWriteOptions::default();
    options.set_incremental(true);
    doc.save_with_options(name, options)
        .map_err(|e| format!("could not write the PDF: {e}"))?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pdf")
            .join(name)
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kitab-marks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    fn every_shape(page: u32) -> Vec<Mark> {
        let pt = |x, y| PointPt { x, y };
        let rect = RectPt {
            x: 100.0,
            y: 120.0,
            w: 160.0,
            h: 60.0,
        };
        [
            MarkShape::Ink {
                strokes: vec![vec![pt(80.0, 300.0), pt(120.0, 330.0), pt(160.0, 300.0)]],
            },
            MarkShape::Line {
                from: pt(80.0, 400.0),
                to: pt(300.0, 400.0),
                arrow: false,
            },
            MarkShape::Line {
                from: pt(80.0, 450.0),
                to: pt(300.0, 480.0),
                arrow: true,
            },
            MarkShape::Rect { rect },
            MarkShape::Ellipse {
                rect: RectPt { y: 200.0, ..rect },
            },
            MarkShape::Highlight {
                rects: vec![RectPt {
                    x: 72.0,
                    y: 90.0,
                    w: 200.0,
                    h: 14.0,
                }],
            },
            MarkShape::Underline {
                rects: vec![RectPt {
                    x: 72.0,
                    y: 110.0,
                    w: 200.0,
                    h: 14.0,
                }],
            },
            MarkShape::Strike {
                rects: vec![RectPt {
                    x: 72.0,
                    y: 130.0,
                    w: 200.0,
                    h: 14.0,
                }],
            },
            MarkShape::Text {
                rect: RectPt {
                    x: 300.0,
                    y: 600.0,
                    w: 200.0,
                    h: 40.0,
                },
                text: "Kitab text".into(),
                size: 14.0,
            },
            MarkShape::Note {
                at: pt(500.0, 100.0),
                text: "A note".into(),
            },
        ]
        .into_iter()
        .map(|shape| Mark {
            page,
            shape,
            color: Rgb8 {
                r: 200,
                g: 40,
                b: 40,
            },
            width: 2.0,
            opacity: 0.8,
        })
        .collect()
    }

    fn annots(path: &Path, password: Option<&str>, page: i32) -> Vec<(PdfAnnotationType, Rect)> {
        let mut doc = PdfDocument::open(path.to_str().expect("utf-8")).expect("opens");
        if let Some(pw) = password {
            assert!(doc.authenticate(pw).expect("auth"));
        }
        let page = doc.load_pdf_page(page).expect("page");
        page.annotations()
            .map(|a| (a.r#type().expect("type"), a.rect().expect("rect")))
            .collect()
    }

    #[test]
    fn every_mark_becomes_its_annotation_and_the_original_bytes_stay() {
        let src = fixture("01-text-outline.pdf");
        let out = temp("annotated.pdf");
        let marks = every_shape(1);
        assert_eq!(
            write_marks(&src, None, &marks, &[], &out).expect("writes"),
            marks.len()
        );
        let found = annots(&out, None, 1);
        let kinds: Vec<_> = found.iter().map(|(t, _)| *t).collect();
        assert_eq!(
            kinds,
            marks.iter().map(|m| subtype(&m.shape)).collect::<Vec<_>>()
        );
        // The square lands where it was drawn, in page points.
        let (_, sq) = found[3];
        assert!(
            (sq.x0 - 100.0).abs() < 3.0 && (sq.y0 - 120.0).abs() < 3.0,
            "{sq:?}"
        );
        // Incremental: the original file is a prefix of the saved one.
        let before = std::fs::read(&src).expect("src");
        let after = std::fs::read(&out).expect("out");
        assert!(after.len() > before.len() && after.starts_with(&before));
        // Other pages untouched.
        assert!(annots(&out, None, 0).is_empty());
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn a_locked_pdf_keeps_its_password() {
        let src = fixture("04a-encrypted-password-kitab.pdf");
        let out = temp("locked.pdf");
        assert!(write_marks(&src, None, &every_shape(0), &[], &out).is_err());
        assert!(!out.exists(), "nothing written without the password");
        write_marks(&src, Some("kitab"), &every_shape(0)[..2], &[], &out).expect("writes");
        let doc = PdfDocument::open(out.to_str().expect("utf-8")).expect("opens");
        assert!(doc.needs_password().expect("check"));
        assert_eq!(annots(&out, Some("kitab"), 0).len(), 2);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn bad_marks_change_nothing() {
        let src = fixture("01-text-outline.pdf");
        let out = temp("bad.pdf");
        let mut marks = every_shape(0);
        marks[0].width = f32::NAN;
        assert!(write_marks(&src, None, &marks, &[], &out).is_err());
        let mut far = every_shape(0);
        far[0].page = 10_000;
        assert!(write_marks(&src, None, &far, &[], &out).is_err());
        assert!(write_marks(&src, None, &[], &[], &out).is_err());
        assert!(!out.exists());
        assert!(!out.with_extension("pdf.kitab-tmp").exists());
    }

    #[test]
    fn saving_over_the_source_works() {
        let copy = temp("in-place.pdf");
        std::fs::copy(fixture("01-text-outline.pdf"), &copy).expect("copy");
        write_marks(&copy, None, &every_shape(2)[..3], &[], &copy).expect("writes in place");
        assert_eq!(annots(&copy, None, 2).len(), 3);
        let _ = std::fs::remove_file(&copy);
    }

    #[test]
    fn annotations_already_in_the_pdf_can_be_deleted() {
        use kitab_core::AnnotEdit;
        let copy = temp("delete.pdf");
        std::fs::copy(fixture("01-text-outline.pdf"), &copy).expect("copy");
        write_marks(&copy, None, &every_shape(1)[..4], &[], &copy).expect("writes");
        let before = annots(&copy, None, 1);
        assert_eq!(before.len(), 4);
        // Delete the first and the third; the others keep their order.
        let edits = [
            AnnotEdit::Delete { page: 1, index: 0 },
            AnnotEdit::Delete { page: 1, index: 2 },
        ];
        assert_eq!(
            write_marks(&copy, None, &[], &edits, &copy).expect("deletes"),
            2
        );
        let after = annots(&copy, None, 1);
        assert_eq!(after, vec![before[1], before[3]]);
        // An index the page doesn't have changes nothing.
        let missing = [AnnotEdit::Delete { page: 1, index: 40 }];
        assert!(write_marks(&copy, None, &[], &missing, &copy).is_err());
        assert_eq!(annots(&copy, None, 1).len(), 2);
        let _ = std::fs::remove_file(&copy);
    }
}
