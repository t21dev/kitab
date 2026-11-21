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
        MarkShape::Rect { .. } | MarkShape::Cover { .. } => PdfAnnotationType::Square,
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
        // A square filled with its own colour, so every reader shows a box.
        MarkShape::Cover { rect } => {
            a.set_rect(r(rect))?;
            a.set_border_width(0.0)?;
            a.set_color(c)?;
            a.set_interior_color(c)?;
        }
        MarkShape::Highlight { rects }
        | MarkShape::Underline { rects }
        | MarkShape::Strike { rects } => {
            for rect in rects {
                a.add_quad_point(Quad::from(r(rect)))?;
            }
            a.set_color(c)?;
        }
        MarkShape::Text {
            rect,
            text,
            size,
            style,
        } => {
            a.set_rect(r(rect))?;
            a.set_contents(text)?;
            a.set_default_appearance("Helv", *size, Some(c))?;
            // Alignment is standard (/Q), and MuPDF's own look follows it.
            a.object()
                .dict_put("Q", mupdf::pdf::PdfObject::new_int(style.align.quadding())?)?;
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
    // Form fields first, the newest value of each, by widget (deleting
    // annotations below doesn't move a widget among the page's widgets).
    let fields: Vec<kitab_core::FieldEdit> = edits
        .iter()
        .filter_map(|e| match e {
            kitab_core::AnnotEdit::Field(f) => Some(f.clone()),
            kitab_core::AnnotEdit::Delete { .. } => None,
        })
        .collect();
    if !fields.is_empty() && !crate::doc::can_fill_forms(&doc) {
        return Err("this PDF doesn't allow filling in its form".into());
    }
    for f in kitab_core::latest_edits(&fields) {
        crate::forms::apply(&mut doc, u32::try_from(pages).unwrap_or(0), &f)
            .map_err(|e| format!("could not fill in a field: {e}"))?;
        written += 1;
    }
    // Deletions next, page by page, highest index first so the others keep
    // their places (new marks are appended after).
    let mut gone: Vec<(u32, u32)> = edits
        .iter()
        .filter_map(|e| match e {
            kitab_core::AnnotEdit::Delete { page, index } => Some((*page, *index)),
            kitab_core::AnnotEdit::Field(_) => None,
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
            // Text MuPDF draws as "?" (Bangla, Hindi, Arabic, Thai…):
            // Kitab draws it (script_ap.rs).
            // and bold or italic text, which MuPDF's text boxes don't have.
            if let MarkShape::Text {
                text, size, style, ..
            } = &m.shape
            {
                let c = m.color;
                let rgb = (
                    f32::from(c.r) / 255.0,
                    f32::from(c.g) / 255.0,
                    f32::from(c.b) / 255.0,
                );
                crate::script_ap::draw_text_box(
                    &mut doc,
                    &mut annot.object(),
                    text,
                    *size,
                    rgb,
                    *style,
                )
                .map_err(|e| format!("could not add a {}: {e}", m.kind()))?;
            }
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
            MarkShape::Cover {
                rect: RectPt { y: 520.0, ..rect },
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
                style: kitab_core::TextStyle::default(),
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

    /// A text box sized with the screen's font (narrower than the system
    /// font Kitab saves with) grows to fit, top-left kept, instead of its
    /// last word wrapping out of sight and its underline being clipped
    /// (found running the app: "kitab text box" saved as "kitab text b").
    #[cfg(windows)]
    #[test]
    fn a_tight_text_box_grows_to_fit_its_text() {
        let src = fixture("01-text-outline.pdf");
        let out = temp("tight-box.pdf");
        let (x, y, w, h) = (100.0, 500.0, 60.0, 12.0);
        let mark = Mark {
            page: 0,
            shape: MarkShape::Text {
                rect: RectPt { x, y, w, h },
                text: "kitab text box".into(),
                size: 14.0,
                style: kitab_core::TextStyle {
                    underline: true,
                    ..Default::default()
                },
            },
            color: Rgb8 { r: 0, g: 0, b: 0 },
            width: 1.0,
            opacity: 1.0,
        };
        write_marks(&src, None, &[mark], &[], &out).expect("writes");
        let doc = PdfDocument::open(out.to_str().expect("utf-8")).expect("reopens");
        let page = doc.load_pdf_page(0).expect("page");
        let annot = page.annotations().next().expect("the box");
        let o = annot.object();
        let rect = o.get_dict("Rect").expect("rect").expect("rect");
        let at = |i| {
            rect.get_array(i)
                .expect("n")
                .expect("n")
                .as_float()
                .expect("f")
        };
        let (rw, rh) = ((at(2) - at(0)).abs(), (at(3) - at(1)).abs());
        assert!(rw > w + 20.0 && rh > h, "grew to {rw} × {rh}");
        // The left edge stays put.
        assert!((at(0).min(at(2)) - x).abs() < 0.5, "left {}", at(0));
        let ap = o
            .get_dict("AP")
            .ok()
            .flatten()
            .and_then(|ap| ap.get_dict("N").ok().flatten())
            .and_then(|n| n.read_stream().ok())
            .map(|s| String::from_utf8_lossy(&s).into_owned())
            .unwrap_or_default();
        // One line of text, so one underline.
        assert_eq!(ap.matches(" l S").count(), 1, "{ap}");
        let _ = std::fs::remove_file(&out);
    }

    /// Bold, italic and centred text boxes (issue #13): Kitab draws the bold
    /// and italic ones (MuPDF's are plain), alignment is the standard /Q,
    /// and a plain box keeps MuPDF's own look.
    #[cfg(windows)]
    #[test]
    fn styled_text_boxes_are_drawn_and_aligned() {
        let src = fixture("01-text-outline.pdf");
        let out = temp("styled-box.pdf");
        let boxed = |y: f32, text: &str, style: kitab_core::TextStyle| Mark {
            page: 0,
            shape: MarkShape::Text {
                rect: RectPt {
                    x: 72.0,
                    y,
                    w: 300.0,
                    h: 30.0,
                },
                text: text.into(),
                size: 16.0,
                style,
            },
            color: Rgb8 {
                r: 160,
                g: 20,
                b: 20,
            },
            width: 1.0,
            opacity: 1.0,
        };
        let styled = kitab_core::TextStyle {
            bold: true,
            italic: true,
            align: kitab_core::TextAlign::Centre,
            ..Default::default()
        };
        let plain_right = kitab_core::TextStyle {
            align: kitab_core::TextAlign::Right,
            ..Default::default()
        };
        let lined = kitab_core::TextStyle {
            underline: true,
            strike: true,
            valign: kitab_core::TextVAlign::Bottom,
            ..Default::default()
        };
        let marks = [
            boxed(560.0, "Bold and slanted", styled),
            boxed(620.0, "Plain on the right", plain_right),
            boxed(680.0, "Underlined and struck", lined),
        ];
        write_marks(&src, None, &marks, &[], &out).expect("writes");
        let doc = PdfDocument::open(out.to_str().expect("utf-8")).expect("reopens");
        let page = doc.load_pdf_page(0).expect("page");
        let looks: Vec<(i64, String)> = page
            .annotations()
            .map(|a| {
                let o = a.object();
                let q = o
                    .get_dict("Q")
                    .ok()
                    .flatten()
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(0);
                let ap = o
                    .get_dict("AP")
                    .ok()
                    .flatten()
                    .and_then(|ap| ap.get_dict("N").ok().flatten())
                    .and_then(|n| n.read_stream().ok())
                    .map(|s| String::from_utf8_lossy(&s).into_owned())
                    .unwrap_or_default();
                (i64::from(q), ap)
            })
            .collect();
        assert_eq!(looks.len(), 3);
        let (q0, ap0) = &looks[0];
        assert_eq!(*q0, 1, "centred");
        assert!(
            ap0.contains("/KitabF") && ap0.contains("2 Tr") && ap0.contains(" 0.21 "),
            "{ap0}"
        );
        let (q1, ap1) = &looks[1];
        assert_eq!(*q1, 2, "right");
        assert!(!ap1.contains("/KitabF"), "a plain box keeps MuPDF's look");
        // Underline and strikethrough: Kitab draws the box, two lines a line.
        let (_, ap2) = &looks[2];
        assert!(ap2.contains("/KitabF"), "{ap2}");
        assert_eq!(ap2.matches(" l S").count(), 2, "{ap2}");
        let stext = doc
            .load_page(0)
            .expect("page")
            .to_display_list(true)
            .expect("list")
            .to_text_page(mupdf::TextPageFlags::empty())
            .expect("text");
        let mut read = String::new();
        for block in stext.blocks() {
            for line in block.lines() {
                read.extend(line.chars().filter_map(|c| c.char()));
                read.push(' ');
            }
        }
        assert!(
            read.contains("Bold and slanted")
                && read.contains("Plain on the right")
                && read.contains("Underlined and struck"),
            "{read}"
        );
        if let Ok(dir) = std::env::var("KITAB_SHOT") {
            let pix = doc
                .load_page(0)
                .expect("page")
                .to_pixmap(
                    &mupdf::Matrix::new_scale(3.0, 3.0),
                    &mupdf::Colorspace::device_rgb(),
                    false,
                    true,
                )
                .expect("draws");
            pix.save_as(&format!("{dir}/styled-box.png"), mupdf::ImageFormat::PNG)
                .expect("png");
        }
        let _ = std::fs::remove_file(&out);
    }

    /// A text box in Bangla (issue #13): drawn by Kitab with its letters
    /// joined, and read back as the same words, not MuPDF's "?".
    #[cfg(windows)]
    #[test]
    fn a_bangla_text_box_is_drawn_and_read_back() {
        let src = fixture("01-text-outline.pdf");
        let out = temp("bangla-box.pdf");
        let text = "আমার সোনার বাংলা, আমি তোমায় ভালোবাসি";
        let mark = Mark {
            page: 0,
            shape: MarkShape::Text {
                rect: RectPt {
                    x: 72.0,
                    y: 600.0,
                    w: 220.0,
                    h: 60.0,
                },
                text: text.into(),
                size: 14.0,
                style: kitab_core::TextStyle::default(),
            },
            color: Rgb8 {
                r: 20,
                g: 40,
                b: 160,
            },
            width: 1.0,
            opacity: 1.0,
        };
        write_marks(&src, None, &[mark], &[], &out).expect("writes");
        let doc = PdfDocument::open(out.to_str().expect("utf-8")).expect("reopens");
        let page = doc.load_pdf_page(0).expect("page");
        let ours = page.annotations().any(|a| {
            a.object()
                .get_dict("AP")
                .ok()
                .flatten()
                .and_then(|ap| ap.get_dict("N").ok().flatten())
                .and_then(|n| n.read_stream().ok())
                .is_some_and(|s| String::from_utf8_lossy(&s).contains("/KitabF"))
        });
        assert!(ours, "Kitab draws the text box");
        let stext = doc
            .load_page(0)
            .expect("page")
            .to_display_list(true)
            .expect("list")
            .to_text_page(mupdf::TextPageFlags::empty())
            .expect("text");
        let mut read = String::new();
        for block in stext.blocks() {
            for line in block.lines() {
                read.extend(line.chars().filter_map(|c| c.char()));
                read.push(' ');
            }
        }
        for word in ["সোনার", "বাংলা", "তোমায়", "ভালোবাসি"]
        {
            assert!(read.contains(word), "{word} in {read}");
        }
        assert!(!read.contains(char::REPLACEMENT_CHARACTER), "{read}");
        if let Ok(dir) = std::env::var("KITAB_SHOT") {
            let pix = doc
                .load_page(0)
                .expect("page")
                .to_pixmap(
                    &mupdf::Matrix::new_scale(3.0, 3.0),
                    &mupdf::Colorspace::device_rgb(),
                    false,
                    true,
                )
                .expect("draws");
            pix.save_as(&format!("{dir}/bangla-box.png"), mupdf::ImageFormat::PNG)
                .expect("png");
        }
        let _ = std::fs::remove_file(&out);
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
    fn form_fields_are_saved_into_the_file_after_the_original_bytes() {
        use kitab_core::{AnnotEdit, FieldEdit, FieldValue};
        let copy = temp("form.pdf");
        std::fs::copy(fixture("11-form-two-pages.pdf"), &copy).expect("copy");
        let fill = |index, value| {
            AnnotEdit::Field(FieldEdit {
                page: 0,
                index,
                value,
            })
        };
        let edits = [
            fill(0, FieldValue::Text("first".into())),
            fill(1, FieldValue::Text("line one\nline two".into())),
            // A later value of the same field wins.
            fill(0, FieldValue::Text("1 Main St".into())),
        ];
        assert_eq!(
            write_marks(&copy, None, &[], &edits, &copy).expect("saves"),
            2
        );
        let before = std::fs::read(fixture("11-form-two-pages.pdf")).expect("src");
        let after = std::fs::read(&copy).expect("out");
        assert!(after.starts_with(&before), "saved incrementally");
        let doc = PdfDocument::open(copy.to_str().expect("utf-8")).expect("opens");
        let f = crate::forms::fields(&doc, 1).expect("fields");
        assert_eq!(f[0].value, "1 Main St", "the shared field, on page two");
        let f = crate::forms::fields(&doc, 0).expect("fields");
        assert_eq!(f[1].value, "line one\nline two");
        // A read-only field is refused, and nothing is written.
        let locked = [fill(2, FieldValue::Text("x".into()))];
        assert!(write_marks(&copy, None, &[], &locked, &copy).is_err());
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
