// SPDX-License-Identifier: AGPL-3.0-or-later

//! Engine operations only the `kitab` command line needs: pages from one
//! PDF put into another, and a rendered page saved as a PNG. Like the other
//! tools they read the source and write a new file next to the target, then
//! rename it, so the original is never changed and a failure leaves nothing.

use crate::tools::{open_pdf, same_file, save_pdf};
use kitab_core::Bitmap;
use mupdf::pdf::{InsertPdfOptions, InsertPosition, PageSelection};
use std::path::{Path, PathBuf};

/// Write `src` with pages of `from` put in before page `before` (0-based;
/// the page count puts them at the end). `from_pages` (0-based, in this
/// order) picks the pages; `None` takes all of them. Returns (pages put in,
/// pages in the new file).
pub fn insert_pages(
    src: &Path,
    password: Option<&str>,
    from: &Path,
    from_password: Option<&str>,
    from_pages: Option<&[u32]>,
    before: u32,
    out: &Path,
) -> Result<(u32, u32), String> {
    if same_file(src, out) || same_file(from, out) {
        return Err("choose a different file name than the originals".into());
    }
    let mut doc = open_pdf(src, password)?;
    let other = open_pdf(from, from_password)?;
    let total = doc.page_count().map_err(|e| e.to_string())?.max(0) as u32;
    if before > total {
        return Err(format!(
            "page {} is outside 1–{}",
            before + 1,
            total.saturating_add(1)
        ));
    }
    let source_pages = match from_pages {
        Some([]) => return Err("no pages chosen".into()),
        Some(pages) => PageSelection::Pages(pages.iter().map(|p| *p as usize).collect()),
        None => PageSelection::All,
    };
    let target = if before == total {
        InsertPosition::Append
    } else {
        InsertPosition::Before(before as usize)
    };
    let done = doc
        .insert_pdf(
            &other,
            InsertPdfOptions {
                source_pages,
                target,
                ..InsertPdfOptions::default()
            },
        )
        .map_err(|e| format!("could not insert the pages: {e}"))?;
    let now = doc.page_count().map_err(|e| e.to_string())?.max(0) as u32;
    save_pdf(&doc, out)?;
    Ok((done.page_count as u32, now))
}

/// Write a PDF of `src`'s pages `order` (0-based, without repeats), in
/// exactly that order: the others are dropped, then each page is moved into
/// place. Returns the page count.
///
/// `PdfDocument::select_pages` (behind `tools::extract_pages` and
/// `pages::move_pages`) keeps pages in their original order, so "keep 3,1"
/// or a move came out unchanged; this does what the command asks. A moved
/// page keeps its object, so outline entries and links still find it.
pub fn arrange_pages(
    src: &Path,
    password: Option<&str>,
    order: &[u32],
    out: &Path,
) -> Result<u32, String> {
    if order.is_empty() {
        return Err("no pages chosen".into());
    }
    if same_file(src, out) {
        return Err("choose a different file name than the original".into());
    }
    let mut doc = open_pdf(src, password)?;
    let total = doc.page_count().map_err(|e| e.to_string())?.max(0) as u32;
    let mut seen = std::collections::HashSet::new();
    if order.iter().any(|p| *p >= total || !seen.insert(*p)) {
        return Err("a page the document doesn't have, or one chosen twice".into());
    }
    let mut kept: Vec<u32> = order.to_vec();
    kept.sort_unstable();
    if kept.len() < total as usize {
        doc.select_pages(PageSelection::Pages(
            kept.iter().map(|p| *p as usize).collect(),
        ))
        .map_err(|e| format!("could not select those pages: {e}"))?;
    }
    // `kept` is now the document's order; move each page to its place.
    for (i, want) in order.iter().enumerate() {
        let Some(j) = kept.iter().position(|p| p == want) else {
            continue;
        };
        if j != i {
            doc.move_page(j, i)
                .map_err(|e| format!("could not move a page: {e}"))?;
            let p = kept.remove(j);
            kept.insert(i, p);
        }
    }
    save_pdf(&doc, out)?;
    Ok(order.len() as u32)
}

/// Save a rendered page (BGRA on white, as [`crate::Engine::render`] gives
/// it) to `out` as an RGB PNG.
pub fn write_png(bitmap: &Bitmap, out: &Path) -> Result<(), String> {
    let (w, h) = (bitmap.width as usize, bitmap.height as usize);
    if w == 0 || h == 0 || bitmap.bgra.len() < w * h * 4 {
        return Err("the page drew nothing".into());
    }
    let wi = i32::try_from(bitmap.width).map_err(|e| e.to_string())?;
    let hi = i32::try_from(bitmap.height).map_err(|e| e.to_string())?;
    let mut pix = mupdf::Pixmap::new_with_w_h(&mupdf::Colorspace::device_rgb(), wi, hi, false)
        .map_err(|e| e.to_string())?;
    let stride = pix.stride().unsigned_abs();
    let samples = pix.samples_mut();
    for y in 0..h {
        let src = &bitmap.bgra[y * w * 4..(y + 1) * w * 4];
        let row = &mut samples[y * stride..y * stride + w * 3];
        for (dst, px) in row
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .zip(src.as_chunks::<4>().0)
        {
            // Opaque, so premultiplied and straight colour are the same.
            *dst = [px[2], px[1], px[0]];
        }
    }
    let tmp: PathBuf = out.with_extension("png.kitab-tmp");
    let name = tmp.to_str().ok_or("the file name is not valid Unicode")?;
    pix.save_as(name, mupdf::ImageFormat::PNG)
        .map_err(|e| format!("could not write the PNG: {e}"))?;
    std::fs::rename(&tmp, out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("could not save the PNG: {e}")
    })
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
        let dir = std::env::temp_dir().join(format!("kitab-cli-ops-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    #[test]
    fn pages_from_another_pdf_go_in_where_asked() {
        let src = fixture("01-text-outline.pdf");
        let from = fixture("10-scanned-and-images.pdf");
        let out = temp("inserted.pdf");
        let (added, total) =
            insert_pages(&src, None, &from, None, Some(&[1]), 0, &out).expect("inserts");
        assert_eq!((added, total), (1, 41));
        let (added, total) =
            insert_pages(&src, None, &from, None, None, 40, &out).expect("appends");
        assert_eq!(total, 40 + added);
        assert!(insert_pages(&src, None, &from, None, None, 42, &out).is_err());
        assert!(insert_pages(&src, None, &from, None, None, 0, &src).is_err());
        let _ = std::fs::remove_file(out);
    }
    /// The first needle on each page of `path`, page by page.
    fn needles(path: &Path) -> Vec<String> {
        let doc = mupdf::pdf::PdfDocument::open(path.to_str().expect("utf-8")).expect("opens");
        let n = doc.page_count().expect("count");
        (0..n)
            .map(|i| {
                let text = doc
                    .load_page(i)
                    .expect("page")
                    .to_text_page(mupdf::TextPageFlags::empty())
                    .expect("text")
                    .to_text()
                    .expect("text");
                let at = text.find("kitab-needle-").expect("needle");
                text[at..at + 15].to_string()
            })
            .collect()
    }

    #[test]
    fn pages_come_out_in_the_order_asked() {
        let src = fixture("01-text-outline.pdf");
        let out = temp("arranged.pdf");
        assert_eq!(
            arrange_pages(&src, None, &[16, 0, 2], &out).expect("arranges"),
            3
        );
        assert_eq!(
            needles(&out),
            ["kitab-needle-17", "kitab-needle-01", "kitab-needle-03"]
        );
        assert!(arrange_pages(&src, None, &[1, 1], &out).is_err());
        assert!(arrange_pages(&src, None, &[40], &out).is_err());
        assert!(arrange_pages(&src, None, &[0], &src).is_err());
        let _ = std::fs::remove_file(out);
    }
}
