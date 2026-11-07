// SPDX-License-Identifier: AGPL-3.0-or-later

//! Page tools (PLAN.md Phase 9): delete, rotate, move, compress, number,
//! watermark, and take the text or the pictures out. Each reads the PDF at
//! `src` and writes a new file; the original is never changed.

use crate::tools::{open_pdf, same_file, save_pdf};
use mupdf::pdf::{PageSelection, PdfDocument, PdfObject, PdfWriteOptions};
use mupdf::text_page::TextBlockType;
use mupdf::{ImageFormat, TextPageFlags};
use std::path::Path;

fn guard(src: &Path, out: &Path) -> Result<(), String> {
    if same_file(src, out) {
        return Err("choose a different file name than the original".into());
    }
    Ok(())
}

fn count(doc: &PdfDocument) -> Result<u32, String> {
    doc.page_count()
        .map(|n| n.max(0) as u32)
        .map_err(|e| e.to_string())
}

fn check(pages: &[u32], total: u32) -> Result<(), String> {
    if pages.is_empty() {
        return Err("no pages chosen".into());
    }
    if pages.iter().any(|p| *p >= total) {
        return Err("a page the document doesn't have".into());
    }
    Ok(())
}

/// Write the PDF without `pages`. Returns how many pages are left.
pub fn delete_pages(
    src: &Path,
    password: Option<&str>,
    pages: &[u32],
    out: &Path,
) -> Result<u32, String> {
    guard(src, out)?;
    let mut doc = open_pdf(src, password)?;
    let total = count(&doc)?;
    check(pages, total)?;
    let keep: Vec<usize> = (0..total)
        .filter(|p| !pages.contains(p))
        .map(|p| p as usize)
        .collect();
    if keep.is_empty() {
        return Err("that would delete every page".into());
    }
    let left = keep.len() as u32;
    doc.select_pages(PageSelection::Pages(keep))
        .map_err(|e| e.to_string())?;
    save_pdf(&doc, out)?;
    Ok(left)
}

/// The new page order when `pages` move to before page `before` (0-based
/// in the original; `total` puts them at the end).
pub fn moved_order(total: u32, pages: &[u32], before: u32) -> Vec<u32> {
    let moving: Vec<u32> = (0..total).filter(|p| pages.contains(p)).collect();
    let mut order = Vec::with_capacity(total as usize);
    for p in 0..total {
        if p == before {
            order.extend(&moving);
        }
        if !pages.contains(&p) {
            order.push(p);
        }
    }
    if before >= total {
        order.extend(&moving);
    }
    order
}

/// Write the PDF with `pages` moved to before page `before`.
pub fn move_pages(
    src: &Path,
    password: Option<&str>,
    pages: &[u32],
    before: u32,
    out: &Path,
) -> Result<(), String> {
    guard(src, out)?;
    let mut doc = open_pdf(src, password)?;
    let total = count(&doc)?;
    check(pages, total)?;
    let order: Vec<usize> = moved_order(total, pages, before)
        .into_iter()
        .map(|p| p as usize)
        .collect();
    doc.select_pages(PageSelection::Pages(order))
        .map_err(|e| e.to_string())?;
    save_pdf(&doc, out)
}

/// Write the PDF with `pages` turned by `degrees` (a multiple of 90).
pub fn rotate_pages(
    src: &Path,
    password: Option<&str>,
    pages: &[u32],
    degrees: i32,
    out: &Path,
) -> Result<(), String> {
    guard(src, out)?;
    if degrees % 90 != 0 {
        return Err("pages turn in quarter turns".into());
    }
    let doc = open_pdf(src, password)?;
    check(pages, count(&doc)?)?;
    let err = |e: mupdf::Error| e.to_string();
    for &p in pages {
        let mut page = doc.find_page(p as i32).map_err(err)?;
        let now = page
            .get_dict_inheritable("Rotate")
            .map_err(err)?
            .and_then(|r| r.as_int().ok())
            .unwrap_or(0);
        let turned = (now + degrees).rem_euclid(360);
        page.dict_put("Rotate", doc.new_int(turned).map_err(err)?)
            .map_err(err)?;
    }
    save_pdf(&doc, out)
}

/// Write a smaller copy: unused and duplicate objects dropped, streams,
/// fonts and images compressed. Returns (bytes before, bytes after).
pub fn compress(src: &Path, password: Option<&str>, out: &Path) -> Result<(u64, u64), String> {
    guard(src, out)?;
    let before = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    let doc = open_pdf(src, password)?;
    let mut options = PdfWriteOptions::default();
    options
        .set_garbage_level(4)
        .set_compress(true)
        .set_compress_fonts(true)
        .set_compress_images(true)
        .set_clean(true);
    crate::tools::save_with_options(&doc, out, options)?;
    let after = std::fs::metadata(out).map(|m| m.len()).unwrap_or(0);
    Ok((before, after))
}

/// A page's visible box and its /Rotate.
fn page_box(page: &PdfObject) -> Result<([f32; 4], i32), String> {
    let err = |e: mupdf::Error| e.to_string();
    let b = match page.get_dict_inheritable("CropBox").map_err(err)? {
        Some(b) => b,
        None => page
            .get_dict_inheritable("MediaBox")
            .map_err(err)?
            .ok_or("a page without a size")?,
    };
    let mut v = [0.0f32; 4];
    for (i, slot) in v.iter_mut().enumerate() {
        *slot = b
            .get_array(i as i32)
            .map_err(err)?
            .and_then(|n| n.as_float().ok())
            .unwrap_or(0.0);
    }
    let r = [
        v[0].min(v[2]),
        v[1].min(v[3]),
        v[0].max(v[2]),
        v[1].max(v[3]),
    ];
    let rot = page
        .get_dict_inheritable("Rotate")
        .map_err(err)?
        .and_then(|r| r.as_int().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    Ok((r, rot))
}

/// Text that reads upright at the bottom centre (`watermark` false) or
/// across the middle at 45° (`watermark` true) of a page shown with
/// `rotate`, as a text matrix and start point.
pub fn placement(b: [f32; 4], rotate: i32, text_w: f32, size: f32, watermark: bool) -> [f32; 6] {
    let (cx, cy) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
    // The page is shown turned clockwise by `rotate`: text meant to look
    // level runs at +rotate in the file.
    let angle = (rotate as f32 + if watermark { 45.0 } else { 0.0 }).to_radians();
    let (d, up) = ((angle.cos(), angle.sin()), (-angle.sin(), angle.cos()));
    let anchor = if watermark {
        (cx, cy)
    } else {
        // 24 pt above the edge that's shown at the bottom.
        match rotate {
            90 => (b[2] - 24.0, cy),
            180 => (cx, b[3] - 24.0),
            270 => (b[0] + 24.0, cy),
            _ => (cx, b[1] + 24.0),
        }
    };
    let lift = if watermark { size / 3.0 } else { 0.0 };
    let x = anchor.0 - d.0 * text_w / 2.0 - up.0 * lift;
    let y = anchor.1 - d.1 * text_w / 2.0 - up.1 * lift;
    [d.0, d.1, up.0, up.1, x, y]
}

/// A PDF literal string for Helvetica (WinAnsi): outside ASCII becomes '?'.
fn literal(s: &str) -> String {
    let mut out = String::from("(");
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            ' '..='~' => out.push(c),
            _ => out.push('?'),
        }
    }
    out.push(')');
    out
}

/// Draw `content` on top of a page, its graphics state kept apart from the
/// page's own, with Kitab's font and transparency in the resources.
fn stamp(doc: &mut PdfDocument, index: u32, content: &str) -> Result<(), String> {
    let err = |e: mupdf::Error| e.to_string();
    let mut page = doc.find_page(index as i32).map_err(err)?;
    // Fill each dictionary, then put it in place: a dictionary put into
    // another is copied, so changes made after the put would be lost.
    let mut res = match page.get_dict_inheritable("Resources").map_err(err)? {
        Some(r) => r,
        None => doc.new_dict().map_err(err)?,
    };
    let entry =
        |res: &PdfObject, key: &str, name: &str, value: &str| -> Result<PdfObject, String> {
            let mut d = match res.get_dict(key).map_err(err)? {
                Some(d) => d,
                None => doc.new_dict().map_err(err)?,
            };
            d.dict_put(name, doc.new_object_from_str(value).map_err(err)?)
                .map_err(err)?;
            Ok(d)
        };
    let fonts = entry(
        &res,
        "Font",
        "KitabHelv",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    )?;
    let states = entry(&res, "ExtGState", "KitabGS", "<< /ca 0.2 /CA 0.2 >>")?;
    res.dict_put("Font", fonts).map_err(err)?;
    res.dict_put("ExtGState", states).map_err(err)?;
    page.dict_put("Resources", res).map_err(err)?;
    let stream = |doc: &mut PdfDocument, text: &str| -> Result<PdfObject, String> {
        let mut buf = mupdf::Buffer::with_capacity(text.len());
        std::io::Write::write_all(&mut buf, text.as_bytes()).map_err(|e| e.to_string())?;
        doc.add_stream(&buf, None, false).map_err(err)
    };
    let pre = stream(doc, "q\n")?;
    let post = stream(doc, &format!("\nQ\n{content}"))?;
    let mut contents = doc.new_array().map_err(err)?;
    contents.array_push(pre).map_err(err)?;
    if let Some(old) = page.get_dict("Contents").map_err(err)? {
        if old.is_array().map_err(err)? {
            for i in 0..old.len().map_err(err)? {
                if let Some(o) = old.get_array(i as i32).map_err(err)? {
                    contents.array_push(o).map_err(err)?;
                }
            }
        } else {
            contents.array_push(old).map_err(err)?;
        }
    }
    contents.array_push(post).map_err(err)?;
    page.dict_put("Contents", contents).map_err(err)
}

/// Helvetica's average width per character, as a share of the size.
const HELV_AVG: f32 = 0.52;

/// Write the PDF with page numbers at the bottom centre, starting at `first`.
pub fn number_pages(
    src: &Path,
    password: Option<&str>,
    first: u32,
    out: &Path,
) -> Result<u32, String> {
    guard(src, out)?;
    let mut doc = open_pdf(src, password)?;
    let total = count(&doc)?;
    for i in 0..total {
        let page = doc.find_page(i as i32).map_err(|e| e.to_string())?;
        let (b, rot) = page_box(&page)?;
        let label = (first + i).to_string();
        let size = 10.0;
        let m = placement(b, rot, label.len() as f32 * size * 0.556, size, false);
        let content = format!(
            "BT /KitabHelv {size} Tf 0.35 g {:.4} {:.4} {:.4} {:.4} {:.2} {:.2} Tm {} Tj ET\n",
            m[0],
            m[1],
            m[2],
            m[3],
            m[4],
            m[5],
            literal(&label)
        );
        stamp(&mut doc, i, &content)?;
    }
    save_pdf(&doc, out)?;
    Ok(total)
}

/// Write the PDF with `text` across every page, light and at 45°.
pub fn watermark(
    src: &Path,
    password: Option<&str>,
    text: &str,
    out: &Path,
) -> Result<u32, String> {
    guard(src, out)?;
    let text = text.trim();
    if text.is_empty() {
        return Err("the watermark has no text".into());
    }
    let mut doc = open_pdf(src, password)?;
    let total = count(&doc)?;
    let chars = text.chars().count().max(1) as f32;
    for i in 0..total {
        let page = doc.find_page(i as i32).map_err(|e| e.to_string())?;
        let (b, rot) = page_box(&page)?;
        let diag = ((b[2] - b[0]).powi(2) + (b[3] - b[1]).powi(2)).sqrt();
        let size = (diag * 0.7 / (chars * HELV_AVG)).clamp(14.0, 140.0);
        let m = placement(b, rot, chars * size * HELV_AVG, size, true);
        let content = format!(
            "/KitabGS gs BT /KitabHelv {size:.1} Tf 0.45 g {:.4} {:.4} {:.4} {:.4} {:.2} {:.2} Tm {} Tj ET\n",
            m[0],
            m[1],
            m[2],
            m[3],
            m[4],
            m[5],
            literal(text)
        );
        stamp(&mut doc, i, &content)?;
    }
    save_pdf(&doc, out)?;
    Ok(total)
}

/// The text of every page, pages separated by a form feed, to `out`.
/// Returns the page count.
pub fn extract_text(src: &Path, password: Option<&str>, out: &Path) -> Result<u32, String> {
    guard(src, out)?;
    let doc = open_pdf(src, password)?;
    let total = count(&doc)?;
    let mut text = String::new();
    for i in 0..total {
        let page = doc.load_page(i as i32).map_err(|e| e.to_string())?;
        let stext = page
            .to_text_page(TextPageFlags::empty())
            .map_err(|e| e.to_string())?;
        if i > 0 {
            text.push('\u{c}');
        }
        for block in stext.blocks() {
            for line in block.lines() {
                text.extend(line.chars().filter_map(|c| c.char()));
                text.push('\n');
            }
            text.push('\n');
        }
    }
    std::fs::write(out, text).map_err(|e| format!("could not write: {e}"))?;
    Ok(total)
}

/// Every picture on the pages, as PNG files in `dir` ("page 3 - 1.png").
/// Returns how many were written.
pub fn extract_images(src: &Path, password: Option<&str>, dir: &Path) -> Result<u32, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let doc = open_pdf(src, password)?;
    let total = count(&doc)?;
    let mut written = 0u32;
    for i in 0..total {
        let page = doc.load_page(i as i32).map_err(|e| e.to_string())?;
        let stext = page
            .to_text_page(TextPageFlags::PRESERVE_IMAGES)
            .map_err(|e| e.to_string())?;
        let mut n = 0;
        for block in stext.blocks() {
            if block.r#type() != TextBlockType::Image {
                continue;
            }
            let Some(image) = block.image() else { continue };
            let Ok(pixmap) = image.to_pixmap() else {
                continue;
            };
            n += 1;
            let path = dir.join(format!("page {} - {n}.png", i + 1));
            let name = path
                .to_str()
                .ok_or("the folder's name is not valid Unicode")?;
            pixmap
                .save_as(name, ImageFormat::PNG)
                .map_err(|e| format!("could not write {}: {e}", path.display()))?;
            written += 1;
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pdf")
            .join(name)
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kitab-pages-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }

    fn pages_of(p: &Path) -> u32 {
        count(&open_pdf(p, None).unwrap()).unwrap()
    }

    #[test]
    fn moving_pages_keeps_the_rest_in_order() {
        assert_eq!(moved_order(5, &[3], 0), [3, 0, 1, 2, 4]);
        assert_eq!(moved_order(5, &[0, 1], 5), [2, 3, 4, 0, 1]);
        assert_eq!(moved_order(5, &[1, 3], 2), [0, 1, 3, 2, 4]);
        assert_eq!(moved_order(3, &[2], 2), [0, 1, 2]);
    }

    #[test]
    fn text_sits_level_on_turned_pages() {
        let b = [0.0, 0.0, 600.0, 800.0];
        let m = placement(b, 0, 10.0, 10.0, false);
        assert!((m[0] - 1.0).abs() < 1e-4 && (m[5] - 24.0).abs() < 1e-3);
        let m = placement(b, 90, 10.0, 10.0, false);
        assert!(m[1] > 0.99 && (m[4] - 576.0).abs() < 1e-3, "{m:?}");
        let m = placement(b, 270, 10.0, 10.0, false);
        assert!(m[1] < -0.99 && (m[4] - 24.0).abs() < 1e-3);
    }

    #[test]
    fn each_tool_writes_a_working_pdf() {
        let src = fixture("01-text-outline.pdf");
        let total = pages_of(&src);
        let out = temp("deleted.pdf");
        assert_eq!(delete_pages(&src, None, &[0, 2], &out).unwrap(), total - 2);
        assert_eq!(pages_of(&out), total - 2);
        assert!(delete_pages(&src, None, &(0..total).collect::<Vec<_>>(), &out).is_err());

        let out = temp("moved.pdf");
        move_pages(&src, None, &[total - 1], 0, &out).unwrap();
        assert_eq!(pages_of(&out), total);

        let out = temp("turned.pdf");
        rotate_pages(&src, None, &[1], 90, &out).unwrap();
        let doc = open_pdf(&out, None).unwrap();
        let r = doc
            .find_page(1)
            .unwrap()
            .get_dict_inheritable("Rotate")
            .unwrap();
        assert_eq!(r.and_then(|r| r.as_int().ok()), Some(90));

        let out = temp("small.pdf");
        let (before, after) = compress(&src, None, &out).unwrap();
        assert!(before > 0 && after > 0 && pages_of(&out) == total);

        let out = temp("numbered.pdf");
        assert_eq!(number_pages(&src, None, 1, &out).unwrap(), total);
        let out2 = temp("numbered.txt");
        extract_text(&out, None, &out2).unwrap();
        let text = std::fs::read_to_string(&out2).unwrap();
        assert!(
            text.contains(&format!("\n{total}\n")),
            "page numbers in the text"
        );

        let out = temp("marked.pdf");
        watermark(&fixture("02-mixed-page-sizes.pdf"), None, "DRAFT", &out).unwrap();
        extract_text(&out, None, &out2).unwrap();
        assert!(std::fs::read_to_string(&out2).unwrap().contains("DRAFT"));

        let dir = temp("images");
        let n = extract_images(&fixture("10-scanned-and-images.pdf"), None, &dir).unwrap();
        assert!(n >= 1, "10 has pictures");
        assert!(guard(&src, &src).is_err());
    }
}
