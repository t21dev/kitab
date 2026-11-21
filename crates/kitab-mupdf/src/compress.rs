// SPDX-License-Identifier: AGPL-3.0-or-later

//! Make a PDF smaller (GitHub #13): pictures larger than the pages need are
//! scaled down and saved as JPEG, then the file is written out clean (unused
//! objects dropped, repeats merged, every stream and font compressed).
//!
//! Text, vector drawings and fonts are never changed, so they stay as sharp
//! as before. A picture is only replaced when its new copy is smaller, and
//! pictures Kitab can't safely re-encode (masks, CMYK, indexed colours, 16
//! bits, decode arrays) are left as they are. The source file is never
//! touched: the copy goes to `out`.

use crate::tools::{open_pdf, same_file, save_with_options};
use mupdf::pdf::{PdfDocument, PdfObject, PdfWriteOptions};
use std::path::Path;

/// How much smaller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strength {
    /// Only a clean rewrite: nothing about the pictures changes.
    Lossless,
    /// Pictures at most 150 dpi on the largest page, JPEG quality 75: still
    /// fine on screen and on most printers.
    Balanced,
    /// Pictures at most 96 dpi, JPEG quality 60: for sending by email.
    Smallest,
}

impl Strength {
    /// The resolution pictures are brought down to, and the JPEG quality.
    fn pictures(self) -> Option<(f32, u8)> {
        match self {
            Strength::Lossless => None,
            Strength::Balanced => Some((150.0, 75)),
            Strength::Smallest => Some((96.0, 60)),
        }
    }
}

/// What making the copy did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compressed {
    /// The source file's size, in bytes.
    pub before: u64,
    /// The copy's size, in bytes.
    pub after: u64,
    /// Pictures scaled down or re-encoded.
    pub pictures: u32,
}

/// Pictures with more pixels than this are left alone (a 20 000 × 20 000
/// scan would need 1.2 GB to decode).
const MAX_PIXELS: u64 = 200_000_000;

/// Write a smaller copy of the PDF at `src` (opened with `password`) to `out`.
pub fn compress_pdf(
    src: &Path,
    password: Option<&str>,
    strength: Strength,
    out: &Path,
) -> Result<Compressed, String> {
    if same_file(src, out) {
        return Err("choose a different file name than the original".into());
    }
    let before = std::fs::metadata(src)
        .map_err(|e| format!("could not read the PDF: {e}"))?
        .len();
    let mut doc = open_pdf(src, password)?;
    let mut pictures = 0;
    if let Some((dpi, quality)) = strength.pictures() {
        // No picture needs more pixels than its page's long side at `dpi`;
        // the largest page sets the limit for all of them.
        let long_pt = largest_page_side(&doc)?;
        let limit = (long_pt / 72.0 * dpi).round().max(64.0) as u32;
        pictures = shrink_pictures(&mut doc, limit, quality)?;
    }
    let mut options = PdfWriteOptions::default();
    options
        .set_garbage_level(4)
        .set_compress(true)
        .set_compress_fonts(true)
        .set_compress_images(true);
    save_with_options(&doc, out, options)?;
    let after = std::fs::metadata(out)
        .map_err(|e| format!("could not read the copy: {e}"))?
        .len();
    Ok(Compressed {
        before,
        after,
        pictures,
    })
}

/// The longest side of the largest page, in points (at least A4's).
fn largest_page_side(doc: &PdfDocument) -> Result<f32, String> {
    let count = doc.page_count().map_err(|e| e.to_string())?;
    let mut long: f32 = 842.0;
    for i in 0..count {
        if let Ok(b) = doc.load_page(i).and_then(|p| p.bounds()) {
            long = long.max((b.x1 - b.x0).abs()).max((b.y1 - b.y0).abs());
        }
    }
    Ok(long)
}

/// Scale every picture whose long side is over `limit` pixels down to it,
/// as JPEG at `quality`. Returns how many were replaced.
fn shrink_pictures(doc: &mut PdfDocument, limit: u32, quality: u8) -> Result<u32, String> {
    let objects = doc.xref_len().map_err(|e| e.to_string())?;
    let mut replaced = 0;
    for num in 1..objects as i32 {
        let Ok(mut obj) = doc.new_indirect(num, 0) else {
            continue;
        };
        // One picture that can't be read is skipped, not the whole file.
        if let Ok(true) = shrink_one(doc, &mut obj, limit, quality) {
            replaced += 1;
        }
    }
    Ok(replaced)
}

fn name_of(obj: &PdfObject, key: &str) -> Option<String> {
    let v = obj.get_dict(key).ok()??;
    if !v.is_name().ok()? {
        return None;
    }
    Some(String::from_utf8_lossy(&v.as_name().ok()?).into_owned())
}

fn int_of(obj: &PdfObject, key: &str) -> Option<i32> {
    let v = obj.get_dict(key).ok()??;
    if !v.is_int().ok()? {
        return None;
    }
    v.as_int().ok()
}

/// The number of colour components of a picture Kitab can re-encode: 1
/// (grey) or 3 (RGB), from a device colour space or an ICC profile.
fn components(obj: &PdfObject) -> Option<u8> {
    let cs = obj.get_dict("ColorSpace").ok()??;
    if cs.is_name().ok()? {
        return match cs.as_name().ok()?.as_slice() {
            b"DeviceGray" => Some(1),
            b"DeviceRGB" => Some(3),
            _ => None,
        };
    }
    if cs.is_array().ok()? && cs.len().ok()? == 2 {
        let kind = cs.get_array(0).ok()??;
        if kind.is_name().ok()? && kind.as_name().ok()?.as_slice() == b"ICCBased" {
            let profile = cs.get_array(1).ok()??;
            return match int_of(&profile, "N")? {
                1 => Some(1),
                3 => Some(3),
                _ => None,
            };
        }
    }
    None
}

/// Shrink one object if it is a picture worth shrinking. `Ok(true)` when it
/// was replaced.
fn shrink_one(
    doc: &mut PdfDocument,
    obj: &mut PdfObject,
    limit: u32,
    quality: u8,
) -> Result<bool, mupdf::Error> {
    if !obj.is_stream()? || name_of(obj, "Subtype").as_deref() != Some("Image") {
        return Ok(false);
    }
    // What a JPEG can't carry the same way: stencil masks, colour-key
    // masks, decode arrays, and anything but 8-bit grey or RGB.
    let image_mask = obj
        .get_dict("ImageMask")?
        .is_some_and(|v| v.as_bool().unwrap_or(false));
    if image_mask
        || obj.get_dict("Mask")?.is_some()
        || obj.get_dict("Decode")?.is_some()
        || int_of(obj, "BitsPerComponent") != Some(8)
    {
        return Ok(false);
    }
    let Some(n) = components(obj) else {
        return Ok(false);
    };
    let (Some(w), Some(h)) = (int_of(obj, "Width"), int_of(obj, "Height")) else {
        return Ok(false);
    };
    let (w, h) = (w.max(0) as u32, h.max(0) as u32);
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > MAX_PIXELS || w.max(h) <= limit {
        return Ok(false);
    }
    let raw_len = obj.read_raw_stream()?.len();
    let pixels = obj.read_stream()?;
    let Some(jpeg) = scaled_jpeg(&pixels, w, h, n, limit, quality) else {
        return Ok(false);
    };
    if jpeg.bytes.len() >= raw_len {
        return Ok(false);
    }
    let mut buf = mupdf::Buffer::with_capacity(jpeg.bytes.len());
    std::io::Write::write_all(&mut buf, &jpeg.bytes)
        .map_err(|e| mupdf::Error::InvalidArgument(e.to_string()))?;
    obj.write_raw_stream_buffer(&buf)?;
    obj.dict_put("Filter", doc.new_name("DCTDecode")?)?;
    obj.dict_delete("DecodeParms")?;
    obj.dict_put("Width", PdfObject::new_int(jpeg.w as i32)?)?;
    obj.dict_put("Height", PdfObject::new_int(jpeg.h as i32)?)?;
    Ok(true)
}

struct Jpeg {
    bytes: Vec<u8>,
    w: u32,
    h: u32,
}

/// `pixels` (8-bit, `n` components, `w` × `h`) scaled so the long side is
/// `limit`, as a JPEG. `None` if the data doesn't match its size.
fn scaled_jpeg(pixels: &[u8], w: u32, h: u32, n: u8, limit: u32, quality: u8) -> Option<Jpeg> {
    use image::{ImageEncoder, imageops::FilterType};
    let k = limit as f32 / w.max(h) as f32;
    let (nw, nh) = (
        ((w as f32 * k).round() as u32).max(1),
        ((h as f32 * k).round() as u32).max(1),
    );
    let need = w as usize * h as usize * n as usize;
    let data = pixels.get(..need)?.to_vec();
    let mut bytes = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality);
    if n == 1 {
        let img = image::GrayImage::from_raw(w, h, data)?;
        let small = image::imageops::resize(&img, nw, nh, FilterType::Triangle);
        encoder
            .write_image(&small, nw, nh, image::ExtendedColorType::L8)
            .ok()?;
    } else {
        let img = image::RgbImage::from_raw(w, h, data)?;
        let small = image::imageops::resize(&img, nw, nh, FilterType::Triangle);
        encoder
            .write_image(&small, nw, nh, image::ExtendedColorType::Rgb8)
            .ok()?;
    }
    Some(Jpeg {
        bytes,
        w: nw,
        h: nh,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kitab-compress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    /// A 3000 × 2000 photo-like picture (smooth gradients and noise) saved
    /// as a lossless PNG, so the PDF made from it is large.
    fn big_picture(path: &Path) {
        let img = image::RgbImage::from_fn(3000, 2000, |x, y| {
            let noise = ((x.wrapping_mul(7919) ^ y.wrapping_mul(104_729)) % 23) as u8;
            image::Rgb([
                (x % 256) as u8 ^ noise,
                (y % 256) as u8,
                ((x + y) % 256) as u8 ^ noise,
            ])
        });
        img.save_with_format(path, image::ImageFormat::Png)
            .expect("png");
    }

    #[test]
    fn a_picture_pdf_gets_smaller_and_keeps_its_pages() {
        let png = temp("photo.png");
        big_picture(&png);
        let src = temp("photo.pdf");
        crate::tools::images_to_pdf(std::slice::from_ref(&png), &src).expect("makes the PDF");

        let out = temp("photo-smaller.pdf");
        let done = compress_pdf(&src, None, Strength::Smallest, &out).expect("compresses");
        assert_eq!(done.pictures, 1, "{done:?}");
        assert!(done.after * 4 < done.before, "{done:?}");

        // Still one page, the same size, its picture now a JPEG.
        let doc = PdfDocument::open(out.to_str().expect("utf-8")).expect("opens");
        assert_eq!(doc.page_count().expect("count"), 1);
        let before = PdfDocument::open(src.to_str().expect("utf-8")).expect("opens");
        let size = |d: &PdfDocument| d.load_page(0).expect("page").bounds().expect("bounds");
        let (a, b) = (size(&before), size(&doc));
        assert!((a.x1 - b.x1).abs() < 0.5 && (a.y1 - b.y1).abs() < 0.5);
        let mut jpegs = 0;
        for num in 1..doc.xref_len().expect("len") as i32 {
            let o = doc.new_indirect(num, 0).expect("obj");
            if name_of(&o, "Filter").as_deref() == Some("DCTDecode") {
                jpegs += 1;
                assert!(int_of(&o, "Width").expect("w") <= 1200);
            }
        }
        assert_eq!(jpegs, 1);
        for p in [png, src, out] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn lossless_keeps_every_picture_and_text_pdfs_stay_whole() {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pdf/01-text-outline.pdf");
        let out = temp("text-clean.pdf");
        let done = compress_pdf(&src, None, Strength::Lossless, &out).expect("rewrites");
        assert_eq!(done.pictures, 0);
        let a = PdfDocument::open(src.to_str().expect("utf-8")).expect("opens");
        let b = PdfDocument::open(out.to_str().expect("utf-8")).expect("opens");
        assert_eq!(a.page_count().expect("n"), b.page_count().expect("n"));
        let _ = std::fs::remove_file(out);
    }

    #[test]
    fn the_original_cant_be_overwritten() {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pdf/01-text-outline.pdf");
        assert!(compress_pdf(&src, None, Strength::Balanced, &src).is_err());
    }
}
