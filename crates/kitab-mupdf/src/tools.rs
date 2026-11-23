// SPDX-License-Identifier: AGPL-3.0-or-later

//! PDF tools that write new files (PLAN.md Phase 9): extract pages, merge,
//! and add, change or remove a password.
//! Each opens its own copy of the source, so the open document a reader shows
//! is never changed. Files are written next to the target and renamed, so a
//! failure never leaves half a PDF behind.

use mupdf::pdf::{Encryption, InsertPdfOptions, PdfDocument, PdfWriteOptions, Permission};
use std::path::{Path, PathBuf};

/// Open a PDF for a tool, with its password if it has one.
pub(crate) fn open_pdf(path: &Path, password: Option<&str>) -> Result<PdfDocument, String> {
    open(path, password)
}

fn open(path: &Path, password: Option<&str>) -> Result<PdfDocument, String> {
    let name = path.to_str().ok_or("the file name is not valid Unicode")?;
    let mut doc = PdfDocument::open(name).map_err(|e| format!("could not open the PDF: {e}"))?;
    if doc.needs_password().map_err(|e| e.to_string())? {
        let ok = match password {
            Some(p) => doc.authenticate(p).map_err(|e| e.to_string())?,
            None => false,
        };
        if !ok {
            return Err("this PDF needs its password".into());
        }
    }
    Ok(doc)
}

/// Save a tool's result next to `out`, then put it in place.
pub(crate) fn save_pdf(doc: &PdfDocument, out: &Path) -> Result<(), String> {
    save(doc, out)
}

pub(crate) fn save_with_options(
    doc: &PdfDocument,
    out: &Path,
    options: PdfWriteOptions,
) -> Result<(), String> {
    save_with(doc, out, options)
}

fn save(doc: &PdfDocument, out: &Path) -> Result<(), String> {
    save_with(doc, out, PdfWriteOptions::default())
}

fn save_with(doc: &PdfDocument, out: &Path, mut options: PdfWriteOptions) -> Result<(), String> {
    // Drop objects only the removed pages used, and keep streams compressed
    // (unless the caller asked for more).
    if options.garbage_level() == 0 {
        options.set_garbage(true);
    }
    options.set_compress(true);
    let tmp: PathBuf = out.with_extension("pdf.kitab-tmp");
    let tmp_name = tmp.to_str().ok_or("the file name is not valid Unicode")?;
    doc.save_with_options(tmp_name, options)
        .map_err(|e| format!("could not write the PDF: {e}"))?;
    std::fs::rename(&tmp, out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("could not save the PDF: {e}")
    })
}

/// Write `pages` (0-based, in this order and without repeats) of `src` to a new PDF.
pub fn extract_pages(
    src: &Path,
    password: Option<&str>,
    pages: &[u32],
    out: &Path,
) -> Result<(), String> {
    if pages.is_empty() {
        return Err("no pages to save".into());
    }
    if same_file(src, out) {
        return Err("choose a different file name than the original".into());
    }
    // In the order given ("3,1": page 3 first). select_pages alone would put
    // them back in document order.
    crate::cli_ops::arrange_pages(src, password, pages, out).map(|_| ())
}

/// Write every page of `inputs`, in order, to a new PDF. Returns the page count.
pub fn merge(inputs: &[(PathBuf, Option<String>)], out: &Path) -> Result<u32, String> {
    if inputs.len() < 2 {
        return Err("choose at least two PDFs".into());
    }
    if inputs.iter().any(|(p, _)| same_file(p, out)) {
        return Err("choose a file name that isn't one of the PDFs being merged".into());
    }
    let mut dst = PdfDocument::new();
    let mut total = 0u32;
    for (path, password) in inputs {
        let src = open(path, password.as_deref())?;
        let result = dst
            .insert_pdf(&src, InsertPdfOptions::default())
            .map_err(|e| format!("could not merge {}: {e}", file_name(path)))?;
        total += result.page_count as u32;
    }
    save(&dst, out)?;
    Ok(total)
}

/// The most pictures one PDF is made from.
pub const MAX_IMAGES: usize = 2_000;
/// A picture's page is at most this long on its longer side, in points (A4's
/// long side): a phone photo at 72 dpi would otherwise make a page over a
/// metre tall. The picture keeps all its pixels; only the page is smaller.
const MAX_PAGE_PT: f32 = 842.0;

/// The rotation a JPEG's EXIF data asks for (1 upright, 3 upside down, 6 a
/// quarter turn clockwise, 8 anticlockwise; the mirrored ones read as their
/// unmirrored turn). 1 when there's none.
fn jpeg_orientation(data: &[u8]) -> u8 {
    // Walk the markers up to the first APP1 "Exif" segment.
    let mut i = 2;
    if data.get(..2) != Some(&[0xFF, 0xD8]) {
        return 1;
    }
    while i + 4 <= data.len() && data[i] == 0xFF {
        let marker = data[i + 1];
        let len = usize::from(u16::from_be_bytes([data[i + 2], data[i + 3]]));
        if marker == 0xDA || len < 2 {
            break;
        }
        let seg = data.get(i + 4..i + 2 + len).unwrap_or(&[]);
        if marker == 0xE1 && seg.starts_with(b"Exif\0\0") {
            let tiff = &seg[6..];
            let le = tiff.starts_with(b"II");
            let u16_at = |o: usize| -> Option<u16> {
                let b = tiff.get(o..o + 2)?;
                Some(if le {
                    u16::from_le_bytes([b[0], b[1]])
                } else {
                    u16::from_be_bytes([b[0], b[1]])
                })
            };
            let u32_at = |o: usize| -> Option<u32> {
                let b = tiff.get(o..o + 4)?;
                let b = [b[0], b[1], b[2], b[3]];
                Some(if le {
                    u32::from_le_bytes(b)
                } else {
                    u32::from_be_bytes(b)
                })
            };
            let Some(ifd) = u32_at(4).map(|o| o as usize) else {
                return 1;
            };
            let count = usize::from(u16_at(ifd).unwrap_or(0)).min(256);
            for k in 0..count {
                let entry = ifd + 2 + k * 12;
                if u16_at(entry) == Some(0x0112) {
                    return match u16_at(entry + 8).unwrap_or(1) {
                        3 | 4 => 3,
                        5 | 6 => 6,
                        7 | 8 => 8,
                        _ => 1,
                    };
                }
            }
            return 1;
        }
        i += 2 + len;
    }
    1
}

/// A picture as a MuPDF image: MuPDF decodes most formats itself; WebP is
/// decoded by Kitab, as for comic books.
fn picture(path: &Path, data: &[u8]) -> Result<mupdf::Image, String> {
    let webp = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("webp"));
    if !webp {
        return mupdf::Image::from_bytes(data).map_err(|e| e.to_string());
    }
    let (w, h, rgba) = crate::source::webp_pixels(data).ok_or("not a picture Kitab can read")?;
    let (wi, hi) = (
        i32::try_from(w).map_err(|e| e.to_string())?,
        i32::try_from(h).map_err(|e| e.to_string())?,
    );
    let mut pix = mupdf::Pixmap::new_with_w_h(&mupdf::Colorspace::device_rgb(), wi, hi, false)
        .map_err(|e| e.to_string())?;
    let samples = pix.samples_mut();
    for (dst, src) in samples
        .as_chunks_mut::<3>()
        .0
        .iter_mut()
        .zip(rgba.as_chunks::<4>().0)
    {
        dst.copy_from_slice(&src[..3]);
    }
    mupdf::Image::from_pixmap(&pix).map_err(|e| e.to_string())
}

/// A PDF with one page for each picture in `inputs`, in order: each page the
/// picture's own size (from its resolution, at most A4's long side), photos
/// turned upright as their camera recorded. Returns the page count.
pub fn images_to_pdf(inputs: &[PathBuf], out: &Path) -> Result<u32, String> {
    if inputs.is_empty() {
        return Err("choose at least one picture".into());
    }
    if inputs.len() > MAX_IMAGES {
        return Err(format!("choose at most {MAX_IMAGES} pictures"));
    }
    if inputs.iter().any(|p| same_file(p, out)) {
        return Err("choose a file name that isn't one of the pictures".into());
    }
    let mut doc = PdfDocument::new();
    for path in inputs {
        let fail = |e: String| format!("could not add {}: {e}", file_name(path));
        let data = std::fs::read(path).map_err(|e| fail(e.to_string()))?;
        let image = picture(path, &data).map_err(fail)?;
        let (px_w, px_h) = (image.width() as f32, image.height() as f32);
        if px_w < 1.0 || px_h < 1.0 {
            return Err(fail("it has no pixels".into()));
        }
        let (xres, yres) = image.resolution();
        let dpi = |r: i32| if r > 0 { r as f32 } else { 96.0 };
        let (mut w, mut h) = (px_w * 72.0 / dpi(xres), px_h * 72.0 / dpi(yres));
        let long = w.max(h);
        if long > MAX_PAGE_PT {
            let k = MAX_PAGE_PT / long;
            (w, h) = (w * k, h * k);
        }
        let turn = jpeg_orientation(&data);
        // The page (turned pictures swap its sides) and where the picture
        // goes on it (`cm` maps the picture's unit square onto the page).
        let ((pw, ph), cm) = match turn {
            3 => ((w, h), format!("{} 0 0 {} {w} {h}", -w, -h)),
            6 => ((h, w), format!("0 {} {h} 0 0 {w}", -w)),
            8 => ((h, w), format!("0 {w} {} 0 {h} 0", -h)),
            _ => ((w, h), format!("{w} 0 0 {h} 0 0")),
        };
        let img = doc.add_image(&image).map_err(|e| fail(e.to_string()))?;
        let img = img.as_indirect().map_err(|e| fail(e.to_string()))?;
        let content = format!("q {cm} cm /Im0 Do Q\n");
        let mut buf = mupdf::Buffer::with_capacity(content.len());
        std::io::Write::write_all(&mut buf, content.as_bytes()).map_err(|e| fail(e.to_string()))?;
        let dict = doc
            .new_object_from_str("<< >>")
            .map_err(|e| fail(e.to_string()))?;
        let contents = doc
            .add_stream(&buf, Some(&dict), false)
            .map_err(|e| fail(e.to_string()))?
            .as_indirect()
            .map_err(|e| fail(e.to_string()))?;
        let page = doc
            .new_object_from_str(&format!(
                "<< /Type /Page /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im0 {img} 0 R >> >> /Contents {contents} 0 R >>"
            ))
            .map_err(|e| fail(e.to_string()))?;
        let page = doc.add_object(&page).map_err(|e| fail(e.to_string()))?;
        // At the end: the count, not -1, which the wrapper refuses.
        let at = doc.page_count().map_err(|e| fail(e.to_string()))?;
        doc.insert_page(at, &page)
            .map_err(|e| fail(e.to_string()))?;
    }
    save(&doc, out)?;
    Ok(inputs.len() as u32)
}

/// A picture drawn with its longer side `size` px, upright as its camera
/// recorded, decoded as `images_to_pdf` decodes it (Make a PDF from
/// pictures shows these in its list).
pub fn picture_thumbnail(path: &Path, size: u32) -> Result<mupdf::Pixmap, String> {
    let err = |e: mupdf::Error| e.to_string();
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let image = picture(path, &data)?;
    let (iw, ih) = (image.width() as f32, image.height() as f32);
    if iw < 1.0 || ih < 1.0 {
        return Err("it has no pixels".into());
    }
    let turn = jpeg_orientation(&data);
    let (w, h) = if matches!(turn, 6 | 8) {
        (ih, iw)
    } else {
        (iw, ih)
    };
    let k = size.max(1) as f32 / w.max(h);
    let (tw, th) = ((w * k).round().max(1.0), (h * k).round().max(1.0));
    let mut pix = mupdf::Pixmap::new_with_w_h(
        &mupdf::Colorspace::device_rgb(),
        tw as i32,
        th as i32,
        false,
    )
    .map_err(err)?;
    pix.clear_with(255).map_err(err)?;
    {
        let dev = mupdf::Device::from_pixmap(&pix).map_err(err)?;
        // The picture's unit square (its first row at the top) onto the
        // thumbnail, turned as the camera said.
        let ctm = match turn {
            3 => mupdf::Matrix::new(-tw, 0.0, 0.0, -th, tw, th),
            6 => mupdf::Matrix::new(0.0, th, -tw, 0.0, tw, 0.0),
            8 => mupdf::Matrix::new(0.0, -th, tw, 0.0, 0.0, th),
            _ => mupdf::Matrix::new(tw, 0.0, 0.0, th, 0.0, 0.0),
        };
        dev.fill_image(&image, &ctm, 1.0, mupdf::ColorParams::default())
            .map_err(err)?;
    }
    Ok(pix)
}

/// The longest password Kitab writes, in bytes (MuPDF holds 128).
pub const MAX_PASSWORD_BYTES: usize = 100;

/// Write a copy of the PDF at `src` (opened with `password`) to `out`, locked
/// with `new_password` (AES-256; the same password opens it and allows
/// everything), or with no password at all when `None`.
///
/// A PDF whose owner restricts it (opened with its user password only) is
/// refused: taking its password off, or putting a new one on, would lift
/// restrictions the owner set.
pub fn set_password(
    src: &Path,
    password: Option<&str>,
    new_password: Option<&str>,
    out: &Path,
) -> Result<(), String> {
    if same_file(src, out) {
        return Err("choose a different file name than the original".into());
    }
    if let Some(pw) = new_password {
        if pw.is_empty() {
            return Err("the password is empty".into());
        }
        if pw.len() > MAX_PASSWORD_BYTES {
            return Err(format!(
                "the password is longer than {MAX_PASSWORD_BYTES} bytes"
            ));
        }
    }
    let doc = open(src, password)?;
    let allowed = crate::ffi::permissions(&doc).unwrap_or_default();
    let full = allowed.print
        && allowed.copy
        && allowed.annotate
        && allowed.fill_forms
        && allowed.modify
        && allowed.assemble;
    if !full {
        return Err(
            "this PDF's owner restricts it; open it with the owner password to change its password"
                .into(),
        );
    }
    let mut options = PdfWriteOptions::default();
    match new_password {
        Some(pw) => {
            options
                .set_encryption(Encryption::Aes256)
                .set_user_password(pw)
                .set_owner_password(pw)
                .set_permissions(Permission::all());
        }
        None => {
            options.set_encryption(Encryption::None);
        }
    }
    save_with(&doc, out, options)
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pdf")
            .join(name)
    }

    fn page_count(p: &Path) -> usize {
        let name = p.to_str().expect("utf-8 temp path");
        PdfDocument::open(name)
            .expect("output opens")
            .page_count()
            .expect("count") as usize
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kitab-tools-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    fn needs_password(p: &Path) -> bool {
        let name = p.to_str().expect("utf-8 temp path");
        PdfDocument::open(name)
            .expect("output opens")
            .needs_password()
            .expect("check")
    }

    #[test]
    fn a_password_goes_on_and_comes_off() {
        let plain = fixture("01-text-outline.pdf");
        let locked = temp("locked.pdf");
        set_password(&plain, None, Some("sésame 42"), &locked).expect("locks");
        assert!(needs_password(&locked));
        let name = locked.to_str().expect("utf-8");
        let mut doc = PdfDocument::open(name).expect("opens");
        assert!(!doc.authenticate("wrong").expect("auth"));
        assert!(doc.authenticate("sésame 42").expect("auth"));
        assert_eq!(
            doc.page_count().expect("count") as usize,
            page_count(&plain)
        );
        drop(doc);

        // Changed, then taken off again.
        let changed = temp("changed.pdf");
        set_password(&locked, Some("sésame 42"), Some("new"), &changed).expect("changes");
        let mut doc = PdfDocument::open(changed.to_str().expect("utf-8")).expect("opens");
        assert!(doc.authenticate("new").expect("auth"));
        drop(doc);
        let open = temp("unlocked.pdf");
        set_password(&changed, Some("new"), None, &open).expect("unlocks");
        assert!(!needs_password(&open));
        assert_eq!(page_count(&open), page_count(&plain));
        for p in [locked, changed, open] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn password_mistakes_are_errors() {
        let plain = fixture("01-text-outline.pdf");
        let out = temp("never.pdf");
        assert!(set_password(&plain, None, Some(""), &out).is_err());
        assert!(set_password(&plain, None, Some(&"x".repeat(101)), &out).is_err());
        assert!(set_password(&plain, None, Some("a"), &plain).is_err());
        // The encrypted fixture without its password.
        let locked = fixture("04a-encrypted-password-kitab.pdf");
        assert!(set_password(&locked, None, None, &out).is_err());
        // An owner-restricted PDF keeps its restrictions.
        let restricted = fixture("04b-owner-restricted.pdf");
        let e = set_password(&restricted, None, None, &out).expect_err("refused");
        assert!(e.contains("owner"), "{e}");
        assert!(!out.exists());
    }

    #[test]
    fn extracts_the_chosen_pages() {
        let out = temp("extract.pdf");
        extract_pages(&fixture("01-text-outline.pdf"), None, &[0, 4, 9], &out).expect("extracts");
        assert_eq!(page_count(&out), 3);
        let _ = std::fs::remove_file(&out);
    }

    /// A `w` × `h` PNG, filled with grey `shade`, at `path`.
    fn png(path: &Path, w: i32, h: i32, shade: u8) {
        let mut pix = mupdf::Pixmap::new_with_w_h(&mupdf::Colorspace::device_rgb(), w, h, false)
            .expect("pixmap");
        pix.samples_mut().fill(shade);
        pix.save_as(path.to_str().expect("utf-8"), mupdf::ImageFormat::PNG)
            .expect("png");
    }

    #[test]
    fn pictures_become_pages_in_order_at_their_own_shape() {
        let (a, b) = (temp("one.png"), temp("two.png"));
        png(&a, 400, 300, 200);
        png(&b, 300, 600, 90);
        let out = temp("pictures.pdf");
        assert_eq!(
            images_to_pdf(&[a.clone(), b.clone()], &out).expect("makes"),
            2
        );
        let doc = PdfDocument::open(out.to_str().expect("utf-8")).expect("opens");
        assert_eq!(doc.page_count().expect("count"), 2);
        let size = |i: i32| {
            let r = doc.load_page(i).expect("page").bounds().expect("bounds");
            (r.x1 - r.x0, r.y1 - r.y0)
        };
        // Landscape then portrait, 96 dpi: 300 × 225 pt and 225 × 450 pt.
        let (w0, h0) = size(0);
        let (w1, h1) = size(1);
        assert!(
            (w0 - 300.0).abs() < 1.0 && (h0 - 225.0).abs() < 1.0,
            "{w0} × {h0}"
        );
        assert!(
            (w1 - 225.0).abs() < 1.0 && (h1 - 450.0).abs() < 1.0,
            "{w1} × {h1}"
        );
        // The picture fills the page: its middle has the picture's grey.
        let pix = doc
            .load_page(1)
            .expect("page")
            .to_pixmap(
                &mupdf::Matrix::IDENTITY,
                &mupdf::Colorspace::device_rgb(),
                false,
                true,
            )
            .expect("draws");
        let (pw, ph) = (pix.width() as usize, pix.height() as usize);
        let mid = (ph / 2 * pw + pw / 2) * 3;
        assert!((i32::from(pix.samples()[mid]) - 90).abs() < 8);
        for p in [a, b, out] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn pictures_to_pdf_refuses_nothing_and_its_own_inputs() {
        let a = temp("self.png");
        png(&a, 10, 10, 0);
        assert!(images_to_pdf(&[], &temp("none.pdf")).is_err());
        assert!(images_to_pdf(std::slice::from_ref(&a), &a).is_err());
        assert!(images_to_pdf(&[temp("missing.png")], &temp("x.pdf")).is_err());
        let _ = std::fs::remove_file(a);
    }

    #[test]
    fn exif_turns_are_read() {
        // A JPEG start, then an APP1 Exif segment with one IFD entry:
        // Orientation (0x0112) = `o`, big- or little-endian.
        let jpeg = |o: u16, le: bool| -> Vec<u8> {
            let w16 = |v: u16| if le { v.to_le_bytes() } else { v.to_be_bytes() };
            let w32 = |v: u32| if le { v.to_le_bytes() } else { v.to_be_bytes() };
            let mut tiff = Vec::new();
            tiff.extend_from_slice(if le { b"II" } else { b"MM" });
            tiff.extend_from_slice(&w16(42));
            tiff.extend_from_slice(&w32(8));
            tiff.extend_from_slice(&w16(1));
            tiff.extend_from_slice(&w16(0x0112));
            tiff.extend_from_slice(&w16(3));
            tiff.extend_from_slice(&w32(1));
            tiff.extend_from_slice(&w16(o));
            tiff.extend_from_slice(&[0, 0]);
            tiff.extend_from_slice(&w32(0));
            let mut seg = b"Exif\0\0".to_vec();
            seg.extend_from_slice(&tiff);
            let mut d = vec![0xFF, 0xD8, 0xFF, 0xE1];
            d.extend_from_slice(&((seg.len() + 2) as u16).to_be_bytes());
            d.extend_from_slice(&seg);
            d.extend_from_slice(&[0xFF, 0xDA, 0, 2]);
            d
        };
        assert_eq!(jpeg_orientation(&jpeg(6, false)), 6);
        assert_eq!(jpeg_orientation(&jpeg(8, true)), 8);
        assert_eq!(jpeg_orientation(&jpeg(3, true)), 3);
        assert_eq!(jpeg_orientation(&jpeg(1, false)), 1);
        assert_eq!(jpeg_orientation(b"\x89PNG"), 1);
        assert_eq!(jpeg_orientation(&[0xFF, 0xD8, 0xFF]), 1);
    }

    #[test]
    fn merges_in_order() {
        let a = fixture("01-text-outline.pdf");
        let a_pages = page_count(&a);
        let out = temp("merge.pdf");
        let n = merge(&[(a.clone(), None), (a.clone(), None)], &out).expect("merges");
        assert_eq!(n as usize, a_pages * 2);
        assert_eq!(page_count(&out), a_pages * 2);
        let _ = std::fs::remove_file(&out);
    }

    /// Page counts and the password come from expected.json (AGENTS.md rule 21).
    fn expected(file: &str) -> serde_json::Value {
        let text = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/expected.json"),
        )
        .expect("expected.json");
        let all: serde_json::Value = serde_json::from_str(&text).expect("parses");
        all.pointer(&format!("/pdf/{file}"))
            .cloned()
            .unwrap_or_else(|| {
                // expected.json groups by format; fall back to a search.
                fn find(v: &serde_json::Value, key: &str) -> Option<serde_json::Value> {
                    match v {
                        serde_json::Value::Object(m) => m
                            .get(key)
                            .cloned()
                            .or_else(|| m.values().find_map(|c| find(c, key))),
                        _ => None,
                    }
                }
                find(&all, file).expect("fixture in expected.json")
            })
    }

    #[test]
    fn extracts_from_a_password_protected_pdf() {
        let name = "04a-encrypted-password-kitab.pdf";
        let want = expected(name);
        assert_eq!(want["unlocks_with_kitab"], true);
        let pages = want["pages"].as_u64().expect("pages") as u32;
        let src = fixture(name);
        let out = temp("encrypted-extract.pdf");
        assert!(
            extract_pages(&src, None, &[0], &out).is_err(),
            "needs the password"
        );
        extract_pages(&src, Some("kitab"), &[0, pages - 1], &out)
            .expect("extracts with the password");
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn refuses_to_overwrite_the_source() {
        let a = fixture("01-text-outline.pdf");
        assert!(extract_pages(&a, None, &[0], &a).is_err());
        assert!(merge(&[(a.clone(), None)], &temp("one.pdf")).is_err());
    }
}
