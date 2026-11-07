// SPDX-License-Identifier: AGPL-3.0-or-later

//! Document work that runs on a worker thread: open, render, links, text.
//! Nothing here may run on the UI thread (AGENTS.md rule 2), and nothing here
//! trusts the file (rule 8): every MuPDF error becomes a value.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use kitab_core::{
    Bitmap, BookLayout, DocInfo, Format, Link, LinkTarget, OpenError, OutlineItem, RectPt,
    RenderRequest, SizePt,
};
use mupdf::pdf::{PdfDocument, PdfObject};
use mupdf::{
    Colorspace, DestinationKind, Device, Document, IRect, Matrix, MetadataName, Pixmap, Quad, Rect,
    TextPageFlags,
};
use unicode_normalization::UnicodeNormalization;

use crate::ffi;
use crate::find::{PageText, SourceChar};

/// More pages than this is treated as a hostile file, not a book.
pub const MAX_PAGES: u32 = 500_000;
/// Longest side of one rendered bitmap, in device pixels.
pub const MAX_SIDE_PX: u32 = 32_768;
/// Most pixels in one rendered bitmap: 64 Mpx, 256 MB of BGRA. Ask for tiles beyond that.
pub const MAX_PIXELS: u64 = 64 * 1024 * 1024;
/// Outline entries kept, and how deep the tree may go.
const MAX_OUTLINE_ITEMS: usize = 100_000;
const MAX_OUTLINE_DEPTH: usize = 64;
/// Size used for a page whose own box cannot be read: US Letter.
const FALLBACK_SIZE: SizePt = SizePt { w: 612.0, h: 792.0 };

/// The layout books open with until the reader knows its window
/// ([`BookLayout::DEFAULT`]; the fixture oracle counts pages at it).
pub const REFLOW_W: f32 = BookLayout::DEFAULT.width;
pub const REFLOW_H: f32 = BookLayout::DEFAULT.height;
pub const REFLOW_EM: f32 = BookLayout::DEFAULT.em;

/// A PDF keeps its PDF handle (passwords, permissions, repair state); every
/// other format is a plain MuPDF document.
pub(crate) enum Handle {
    Pdf(PdfDocument),
    Other(Document),
}

impl std::ops::Deref for Handle {
    type Target = Document;
    fn deref(&self) -> &Document {
        match self {
            Handle::Pdf(p) => p,
            Handle::Other(d) => d,
        }
    }
}

/// An open document, owned by one worker thread.
pub(crate) struct OpenDoc {
    pub pdf: Handle,
    pub page_count: u32,
    pub format: Format,
    /// Laid out by Kitab (a book), not paged by the file.
    pub reflowable: bool,
    pub right_to_left: bool,
}

fn failed(e: impl std::fmt::Display) -> OpenError {
    OpenError::Failed(e.to_string())
}

/// Whether the file starts like a PDF (`%PDF-` in its first KiB, as readers accept).
fn sniff_pdf(path: &Path) -> bool {
    let mut head = [0u8; 1024];
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let n = f.read(&mut head).unwrap_or(0);
    head[..n].windows(5).any(|w| w == b"%PDF-")
}

/// Open `path` and describe it fully. Runs on a worker.
pub(crate) fn open(
    path: &Path,
    password: Option<&str>,
    layout: BookLayout,
) -> Result<(OpenDoc, DocInfo), OpenError> {
    if !layout.is_valid() {
        return Err(failed("bad book layout"));
    }
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => {}
        Ok(_) => return Err(OpenError::Unsupported),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(OpenError::NotFound),
        Err(e) => return Err(failed(e)),
    }
    let looks_like_pdf = sniff_pdf(path);
    let by_name = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(Format::from_extension);
    // Kindle KF8 (.azw3): MuPDF reads only the older MOBI records, which in a
    // KF8-only book is just the cover. Refuse rather than show one page.
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("azw3") {
        return Err(OpenError::Unsupported);
    }
    // Kitab reads documents, not image files (Markdown has its own reader).
    let format = match by_name {
        _ if looks_like_pdf => Format::Pdf,
        Some(Format::Image | Format::Markdown) | None => return Err(OpenError::Unsupported),
        Some(f) => f,
    };
    // MuPDF takes UTF-8 paths; on Windows it converts them back to UTF-16.
    let file = path
        .to_str()
        .ok_or_else(|| failed("path is not valid Unicode"))?;
    let mut doc = match Document::open(file) {
        Ok(doc) => doc,
        Err(_) if format != Format::Pdf => return Err(OpenError::Unsupported),
        Err(e) => return Err(failed(e)),
    };

    let mut reflowable = false;
    let mut right_to_left = false;
    let handle = if doc.is_pdf() {
        let mut pdf = PdfDocument::try_from(doc).map_err(|_| OpenError::Unsupported)?;
        if pdf.needs_password().map_err(failed)? {
            let Some(password) = password else {
                return Err(OpenError::NeedsPassword);
            };
            if !authenticate(&mut pdf, password) {
                return Err(OpenError::WrongPassword);
            }
        }
        Handle::Pdf(pdf)
    } else {
        // What MuPDF doesn't read from an EPUB's package document.
        let meta = (format == Format::Epub)
            .then(|| crate::epub::read(path))
            .flatten();
        right_to_left = meta.as_ref().is_some_and(|m| m.right_to_left);
        match meta.filter(|m| m.fixed_layout) {
            // Fixed layout: pages at the book's own viewport, like a PDF.
            Some(m) => fit_fixed_layout(&mut doc, &m, layout.em).map_err(failed)?,
            // Books flow onto pages; comics and XPS have their own.
            None if doc.is_reflowable().unwrap_or(false) => {
                doc.layout(layout.width, layout.height, layout.em)
                    .map_err(failed)?;
                reflowable = true;
            }
            None => {}
        }
        Handle::Other(doc)
    };

    let count = handle.page_count().map_err(failed)?;
    let page_count = u32::try_from(count).map_err(|_| failed("negative page count"))?;
    if page_count > MAX_PAGES {
        return Err(failed(format!("{page_count} pages is over the limit")));
    }
    if page_count == 0 {
        return Err(failed("the document has no pages"));
    }
    let format = if matches!(handle, Handle::Pdf(_)) {
        Format::Pdf
    } else {
        format
    };
    let mut info = describe(&handle, format, page_count, reflowable);
    info.right_to_left = right_to_left;
    Ok((
        OpenDoc {
            pdf: handle,
            page_count,
            format,
            reflowable,
            right_to_left,
        },
        info,
    ))
}

/// Try the password as typed, then its NFC and NFKC forms. MuPDF applies
/// SASLprep itself for AES-256; the extra forms help older handlers when the
/// text was pasted in a different normalisation.
fn authenticate(pdf: &mut PdfDocument, password: &str) -> bool {
    let mut tried = Vec::with_capacity(3);
    for candidate in [
        password.to_string(),
        password.nfc().collect::<String>(),
        password.nfkc().collect::<String>(),
    ] {
        if tried.contains(&candidate) {
            continue;
        }
        if pdf.authenticate(&candidate).unwrap_or(false) {
            return true;
        }
        tried.push(candidate);
    }
    false
}

fn non_empty(s: Result<String, mupdf::Error>) -> Option<String> {
    let s = s.ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn describe(handle: &Handle, format: Format, page_count: u32, reflowable: bool) -> DocInfo {
    let page_sizes = (0..page_count).map(|i| page_size(handle, i)).collect();
    let (outline, permissions, encryption, repaired) = match handle {
        Handle::Pdf(pdf) => (
            load_outline(pdf, page_count),
            ffi::permissions(pdf).unwrap_or_default(),
            non_empty(pdf.metadata(MetadataName::Encryption)).filter(|e| e != "None"),
            ffi::was_repaired(pdf).unwrap_or(false),
        ),
        Handle::Other(doc) => {
            let items = doc.outlines().unwrap_or_default();
            let mut budget = MAX_OUTLINE_ITEMS;
            (
                convert_outline(items, &[], page_count, 0, &mut budget),
                Default::default(),
                None,
                false,
            )
        }
    };
    DocInfo {
        format,
        title: non_empty(handle.metadata(MetadataName::Title)),
        author: non_empty(handle.metadata(MetadataName::Author)),
        page_count,
        page_sizes,
        outline,
        permissions,
        encryption,
        repaired,
        reflowable,
        right_to_left: false,
    }
}

/// Lay a fixed-layout EPUB out at its viewport. MuPDF adds its own page
/// margins, so a viewport-sized page spills onto a second one: grow the page
/// until each spine document fits on one (at most half again as tall).
fn fit_fixed_layout(
    doc: &mut Document,
    meta: &crate::epub::EpubMeta,
    em: f32,
) -> Result<(), mupdf::Error> {
    let (w, h) = meta.viewport.unwrap_or((600.0, 800.0));
    let target = i32::try_from(meta.spine_items.max(1)).unwrap_or(i32::MAX);
    for step in 0..=10 {
        let grow = 1.0 + 0.05 * step as f32;
        doc.layout(w + 40.0, h * grow + 40.0, em)?;
        if doc.page_count()? <= target {
            break;
        }
    }
    Ok(())
}

/// Lay a book out again and describe it anew. Runs on the document's worker.
pub(crate) fn relayout(doc: &mut OpenDoc, layout: BookLayout) -> Result<DocInfo, String> {
    if !doc.reflowable || !layout.is_valid() {
        return Err("only a book can be laid out again, at a valid layout".into());
    }
    let Handle::Other(d) = &mut doc.pdf else {
        return Err("only a book can be laid out again".into());
    };
    d.layout(layout.width, layout.height, layout.em)
        .map_err(|e| e.to_string())?;
    let count = doc.pdf.page_count().map_err(|e| e.to_string())?;
    let page_count = u32::try_from(count).map_err(|e| e.to_string())?;
    if page_count == 0 || page_count > MAX_PAGES {
        return Err(format!("{page_count} pages after layout"));
    }
    doc.page_count = page_count;
    let mut info = describe(&doc.pdf, doc.format, page_count, true);
    info.right_to_left = doc.right_to_left;
    Ok(info)
}

/// A page's own size with its /Rotate applied (MuPDF's page bounds).
fn page_size(doc: &Document, index: u32) -> SizePt {
    let bounds = i32::try_from(index)
        .ok()
        .and_then(|i| doc.load_page(i).ok())
        .and_then(|p| p.bounds().ok());
    match bounds {
        Some(r) => {
            let (w, h) = (r.x1 - r.x0, r.y1 - r.y0);
            if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
                SizePt { w, h }
            } else {
                FALLBACK_SIZE
            }
        }
        None => FALLBACK_SIZE,
    }
}

/// Open state of one outline node, read from the PDF's /Count, in the same
/// order MuPDF walks /First and /Next.
struct OpenNode {
    open: bool,
    children: Vec<OpenNode>,
}

fn outline_open_flags(pdf: &PdfDocument) -> Vec<OpenNode> {
    let first = pdf
        .catalog()
        .ok()
        .and_then(|c| c.get_dict("Outlines").ok().flatten())
        .and_then(|o| o.get_dict("First").ok().flatten());
    let mut seen = HashSet::new();
    let mut budget = MAX_OUTLINE_ITEMS;
    match first {
        Some(first) => walk_open_flags(first, 0, &mut seen, &mut budget),
        None => Vec::new(),
    }
}

fn walk_open_flags(
    first: PdfObject,
    depth: usize,
    seen: &mut HashSet<i32>,
    budget: &mut usize,
) -> Vec<OpenNode> {
    let mut out = Vec::new();
    let mut node = Some(first);
    while let Some(obj) = node.take() {
        if *budget == 0 || !obj.is_dict().unwrap_or(false) {
            break;
        }
        let num = obj.as_indirect().unwrap_or(0);
        if num > 0 && !seen.insert(num) {
            break; // a cycle
        }
        *budget -= 1;
        let count = obj
            .get_dict("Count")
            .ok()
            .flatten()
            .and_then(|c| c.as_int().ok())
            .unwrap_or(0);
        let children = match obj.get_dict("First").ok().flatten() {
            Some(child) if depth < MAX_OUTLINE_DEPTH => {
                walk_open_flags(child, depth + 1, seen, budget)
            }
            _ => Vec::new(),
        };
        out.push(OpenNode {
            open: count > 0,
            children,
        });
        node = obj.get_dict("Next").ok().flatten();
    }
    out
}

fn load_outline(pdf: &PdfDocument, page_count: u32) -> Vec<OutlineItem> {
    // A damaged outline is not a reason to refuse the document.
    let items = pdf.outlines().unwrap_or_default();
    let flags = outline_open_flags(pdf);
    let mut budget = MAX_OUTLINE_ITEMS;
    convert_outline(items, &flags, page_count, 0, &mut budget)
}

fn convert_outline(
    items: Vec<mupdf::Outline>,
    flags: &[OpenNode],
    page_count: u32,
    depth: usize,
    budget: &mut usize,
) -> Vec<OutlineItem> {
    let mut out = Vec::with_capacity(items.len().min(*budget));
    for (i, item) in items.into_iter().enumerate() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let flag = flags.get(i);
        let page = item
            .dest
            .map(|d| d.loc.page_number)
            .filter(|&p| p < page_count);
        let uri = match page {
            Some(_) => None,
            None => item.uri.filter(|u| !u.is_empty() && !u.starts_with('#')),
        };
        let children = if depth < MAX_OUTLINE_DEPTH {
            let child_flags = flag.map(|f| f.children.as_slice()).unwrap_or(&[]);
            convert_outline(item.down, child_flags, page_count, depth + 1, budget)
        } else {
            Vec::new()
        };
        out.push(OutlineItem {
            title: item.title,
            page,
            uri,
            open: flag.is_some_and(|f| f.open),
            children,
        });
    }
    out
}

fn page_index(doc: &OpenDoc, page: u32) -> Result<i32, String> {
    if page >= doc.page_count {
        return Err(format!("page {page} is out of range"));
    }
    i32::try_from(page).map_err(|e| e.to_string())
}

/// Render one page, or one tile of it, to BGRA on white paper. Runs on a worker.
pub(crate) fn render(doc: &OpenDoc, req: &RenderRequest) -> Result<Bitmap, String> {
    if !(req.scale.is_finite() && req.scale > 0.0) {
        return Err(format!("bad scale {}", req.scale));
    }
    if req.rotation % 90 != 0 {
        return Err(format!("rotation {} is not a multiple of 90", req.rotation));
    }
    let page = doc
        .pdf
        .load_page(page_index(doc, req.page)?)
        .map_err(|e| e.to_string())?;
    let bounds = page.bounds().map_err(|e| e.to_string())?;

    let mut ctm = Matrix::new_scale(req.scale, req.scale);
    ctm.concat(Matrix::new_rotate(f32::from(req.rotation % 360)));
    let placed = bounds.transform(&ctm);
    ctm.concat(Matrix::new_translate(-placed.x0, -placed.y0));
    let full = Rect::new(0.0, 0.0, placed.x1 - placed.x0, placed.y1 - placed.y0).round();

    let area = match req.tile {
        None => full,
        Some(t) => {
            let x0 = i32::try_from(t.x).map_err(|e| e.to_string())?;
            let y0 = i32::try_from(t.y).map_err(|e| e.to_string())?;
            let x1 = x0.saturating_add(i32::try_from(t.w).map_err(|e| e.to_string())?);
            let y1 = y0.saturating_add(i32::try_from(t.h).map_err(|e| e.to_string())?);
            IRect::new(x0, y0, x1, y1).intersect(&full)
        }
    };
    let (w, h) = (area.width(), area.height());
    if w <= 0 || h <= 0 {
        return Err("the requested area is empty".to_string());
    }
    let (wu, hu) = (w.unsigned_abs(), h.unsigned_abs());
    if wu > MAX_SIDE_PX || hu > MAX_SIDE_PX || u64::from(wu) * u64::from(hu) > MAX_PIXELS {
        return Err(format!(
            "{wu} x {hu} px is over the bitmap limit; render tiles"
        ));
    }

    // Draw straight onto white paper: no alpha, so no compositing afterwards,
    // and blend modes and soft masks behave as they would on a printed page.
    let mut pixmap = Pixmap::new(&Colorspace::device_rgb(), area.x0, area.y0, w, h, false)
        .map_err(|e| e.to_string())?;
    pixmap.clear_with(255).map_err(|e| e.to_string())?;
    {
        let device = Device::from_pixmap(&pixmap).map_err(|e| e.to_string())?;
        page.run(&device, &ctm).map_err(|e| e.to_string())?;
        // Dropping the device closes it and flushes pending drawing.
    }
    to_bgra(&pixmap)
}

/// Convert an RGB pixmap (n = 3, no alpha) to opaque BGRA, top-down.
fn to_bgra(pixmap: &Pixmap) -> Result<Bitmap, String> {
    let (w, h, n) = (pixmap.width(), pixmap.height(), usize::from(pixmap.n()));
    if n != 3 || pixmap.alpha() {
        return Err(format!("expected an RGB pixmap, got {n} channels"));
    }
    let stride = usize::try_from(pixmap.stride()).map_err(|e| e.to_string())?;
    let row_bytes = w as usize * 3;
    let samples = pixmap.samples();
    let mut bgra = Vec::with_capacity(w as usize * h as usize * 4);
    for row in 0..h as usize {
        let line = samples
            .get(row * stride..row * stride + row_bytes)
            .ok_or("pixmap shorter than its size")?;
        for px in line.chunks_exact(3) {
            bgra.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    Ok(Bitmap {
        width: w,
        height: h,
        bgra,
    })
}

fn rect_of(r: Rect) -> RectPt {
    RectPt {
        x: r.x0,
        y: r.y0,
        w: r.x1 - r.x0,
        h: r.y1 - r.y0,
    }
}

fn dest_y(kind: DestinationKind) -> Option<f32> {
    match kind {
        DestinationKind::XYZ { top, .. }
        | DestinationKind::FitH { top }
        | DestinationKind::FitBH { top } => top,
        DestinationKind::FitR { top, .. } => Some(top),
        _ => None,
    }
    .filter(|y| y.is_finite())
}

/// The page's links, in page points. Runs on a worker.
pub(crate) fn links(doc: &OpenDoc, page: u32) -> Result<Vec<Link>, String> {
    let page = doc
        .pdf
        .load_page(page_index(doc, page)?)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for link in page.links().map_err(|e| e.to_string())? {
        let target = match link.dest {
            Some(d) if d.loc.page_number < doc.page_count => LinkTarget::Page {
                page: d.loc.page_number,
                y: dest_y(d.kind),
            },
            Some(_) => continue,
            None if !link.uri.is_empty() && !link.uri.starts_with('#') => LinkTarget::Uri(link.uri),
            None => continue,
        };
        out.push(Link {
            rect: rect_of(link.bounds),
            target,
        });
    }
    Ok(out)
}

fn quad_rect(q: &Quad) -> RectPt {
    let xs = [q.ul.x, q.ur.x, q.ll.x, q.lr.x];
    let ys = [q.ul.y, q.ur.y, q.ll.y, q.lr.y];
    let x0 = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let x1 = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let y0 = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let y1 = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    RectPt {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// The page's text in reading order, with a box per character. Runs on a worker.
pub(crate) fn page_text(doc: &OpenDoc, page: u32) -> Result<PageText, String> {
    let page = doc
        .pdf
        .load_page(page_index(doc, page)?)
        .map_err(|e| e.to_string())?;
    let stext = page
        .to_text_page(TextPageFlags::empty())
        .map_err(|e| e.to_string())?;
    let mut text = PageText::default();
    for block in stext.blocks() {
        for line in block.lines() {
            let index = text.lines.len();
            let mut line_text = String::new();
            for ch in line.chars() {
                let Some(c) = ch.char() else { continue };
                line_text.push(c);
                text.chars.push(SourceChar {
                    ch: c,
                    rect: quad_rect(&ch.quad()),
                    line: index,
                });
            }
            text.lines.push(line_text);
        }
    }
    Ok(text)
}

/// The share of pixels (0.0..=1.0) that are not paper white. A rendered page
/// with text or images has some; a blank render has none.
pub fn ink_ratio(bitmap: &Bitmap) -> f64 {
    let total = bitmap.bgra.len() / 4;
    if total == 0 {
        return 0.0;
    }
    let inked = bitmap
        .bgra
        .chunks_exact(4)
        .filter(|p| p[0] < 250 || p[1] < 250 || p[2] < 250)
        .count();
    inked as f64 / total as f64
}
