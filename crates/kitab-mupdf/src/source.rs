// SPDX-License-Identifier: AGPL-3.0-or-later

//! Documents MuPDF can't read itself, drawn through MuPDF anyway: comic
//! books in RAR and 7z (`.cbr`, `.cb7`) and DjVu. A small MuPDF document
//! handler hands MuPDF their pages, so rendering, thumbnails, tiles, zoom,
//! printing and saving pages work exactly as for every other format.
//!
//! - A comic page is its picture file, given to MuPDF without a copy and
//!   decoded by MuPDF when drawn (WebP, which MuPDF can't decode, by Kitab).
//! - A DjVu page is rendered by djvu-rs at the size it's drawn at.
//!
//! Each file is read once per process: the workers and render helpers that
//! open the same file share it (by reference count) for as long as any has
//! it open.
//!
//! The handler's callbacks run inside MuPDF. They call only MuPDF functions
//! that can't throw, or mupdf-sys's wrappers that catch and return the error,
//! except `fz_new_document_of_size`/`fz_new_page_of_size`/
//! `fz_new_buffer_from_shared_data`, which throw only when out of memory and
//! are called with no Rust value that needs dropping alive.

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::{Arc, LazyLock, Mutex, Once, Weak};

use djvu_rs::DjVuDocument;
use mupdf::Document;
use mupdf_sys::{
    FZ_ERROR_ARGUMENT, FZ_ERROR_UNSUPPORTED, fz_archive, fz_box_type, fz_context, fz_cookie,
    fz_default_color_params, fz_device, fz_device_rgb, fz_document, fz_document_handler,
    fz_document_recognize_state_free_fn, fz_drop_buffer, fz_drop_image, fz_drop_pixmap, fz_image,
    fz_image_resolution, fz_matrix, fz_new_buffer_from_shared_data, fz_new_document_of_size,
    fz_new_page_of_size, fz_page, fz_pixmap_samples, fz_pixmap_stride, fz_rect,
    fz_register_document_handler, fz_stream, fz_strlcpy, fz_throw, mupdf_drop_error, mupdf_error_t,
    mupdf_fill_image, mupdf_new_image_from_buffer, mupdf_new_image_from_pixmap, mupdf_new_pixmap,
};

use crate::comic::Comic;

// MuPDF's error codes as `fz_throw` takes them: bindgen makes the enum `i32`
// on Windows and `u32` elsewhere.
#[allow(clippy::unnecessary_cast)]
const UNSUPPORTED: c_int = FZ_ERROR_UNSUPPORTED as c_int;
#[allow(clippy::unnecessary_cast)]
const ARGUMENT: c_int = FZ_ERROR_ARGUMENT as c_int;

/// A file read for MuPDF to draw.
pub(crate) enum Source {
    Comic(Comic),
    /// Boxed: a parsed DjVu document is much larger than a page list.
    Djvu(Box<DjVuDocument>),
}

impl Source {
    fn page_count(&self) -> usize {
        match self {
            Source::Comic(c) => c.pages.len(),
            Source::Djvu(d) => d.page_count(),
        }
    }
}

// ─── reading each file once ─────────────────────────────────────────────

#[derive(Hash, PartialEq, Eq)]
struct Key {
    path: PathBuf,
    len: u64,
    modified: Option<std::time::SystemTime>,
}

static OPEN: LazyLock<Mutex<HashMap<Key, Weak<Source>>>> = LazyLock::new(Default::default);
/// One file is read at a time, so two workers opening the same comic read it
/// once (the second finds it in `OPEN`).
static READING: Mutex<()> = Mutex::new(());

/// The file at `path`, read (or shared with whoever has it open already).
pub(crate) fn read(path: &Path, djvu: bool) -> Result<Arc<Source>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let key = Key {
        path: path.to_path_buf(),
        len: meta.len(),
        modified: meta.modified().ok(),
    };
    let _one = READING.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(s) = lock_open().get(&key).and_then(Weak::upgrade) {
        return Ok(s);
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let source = Arc::new(parse(&bytes, djvu, false)?);
    let mut open = lock_open();
    open.retain(|_, w| w.strong_count() > 0);
    open.insert(key, Arc::downgrade(&source));
    Ok(source)
}

/// Read a file's bytes: a DjVu document, or a comic's pages (all of them, or
/// only the first for a thumbnail).
fn parse(bytes: &[u8], djvu: bool, first_only: bool) -> Result<Source, String> {
    if djvu {
        let doc = DjVuDocument::parse(bytes).map_err(|e| format!("not a DjVu file ({e})"))?;
        if doc.page_count() == 0 {
            return Err("the document has no pages".into());
        }
        Ok(Source::Djvu(Box::new(doc)))
    } else {
        Ok(Source::Comic(crate::comic::load(bytes, first_only)?))
    }
}

fn lock_open() -> std::sync::MutexGuard<'static, HashMap<Key, Weak<Source>>> {
    OPEN.lock().unwrap_or_else(|p| p.into_inner())
}

// ─── opening through MuPDF ──────────────────────────────────────────────

/// The handler's private name: never a real file's extension or type.
const MAGIC: &CStr = c"application/x-kitab-page-source";

thread_local! {
    /// The source `open` is handing MuPDF on this thread, for the handler's
    /// open callback to pick up.
    static PENDING: RefCell<Option<Arc<Source>>> = const { RefCell::new(None) };
}

/// Add the handler to MuPDF (once). Call before any document opens on another
/// thread: MuPDF's handler list is shared, not locked.
pub fn register() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let names: &'static mut [*const c_char; 2] =
            Box::leak(Box::new([MAGIC.as_ptr(), std::ptr::null()]));
        let types: &'static mut [*const c_char; 2] =
            Box::leak(Box::new([MAGIC.as_ptr(), std::ptr::null()]));
        let handler: &'static fz_document_handler = Box::leak(Box::new(fz_document_handler {
            recognize: None,
            open: Some(open_doc),
            extensions: names.as_mut_ptr(),
            mimetypes: types.as_mut_ptr(),
            recognize_content: Some(recognize),
            wants_dir: 0,
            wants_file: 0,
            fin: None,
        }));
        if let Some(ctx) = crate::ffi::context_ptr() {
            // SAFETY: a valid context; the handler lives for the process.
            // Throws only past 32 handlers (MuPDF registers about a dozen).
            unsafe { fz_register_document_handler(ctx, handler) };
        }
    });
}

/// Open `path` (a RAR or 7z comic, or DjVu) as a MuPDF document, with the
/// file it reads (for the text and contents Kitab takes from it directly).
pub(crate) fn open(path: &Path, djvu: bool) -> Result<(Document, Arc<Source>), String> {
    let source = read(path, djvu)?;
    Ok((open_source(source.clone())?, source))
}

fn open_source(source: Arc<Source>) -> Result<Document, String> {
    register();
    PENDING.with(|p| *p.borrow_mut() = Some(source));
    let doc = Document::from_bytes(&[], MAGIC.to_str().unwrap_or_default());
    PENDING.with(|p| p.borrow_mut().take());
    doc.map_err(|e| e.to_string())
}

/// Whether a file with this extension and these first bytes is read by
/// Kitab's own page source (a comic in RAR or 7z, whatever its extension
/// says, or DjVu) rather than by MuPDF.
pub fn reads_itself(ext: &str, head: &[u8]) -> bool {
    use kitab_core::Format;
    match Format::from_extension(ext) {
        Some(Format::Djvu) => true,
        Some(Format::Cbz | Format::Cbr | Format::Cb7) => matches!(
            crate::comic::sniff(head),
            crate::comic::Kind::Rar | crate::comic::Kind::SevenZ
        ),
        _ => false,
    }
}

/// For a thumbnail: a file [`reads_itself`] says is Kitab's, as a MuPDF
/// document with just its first page (a comic is unpacked no further).
pub fn open_first_page(bytes: &[u8], ext: &str) -> Result<Document, String> {
    let djvu = kitab_core::Format::from_extension(ext) == Some(kitab_core::Format::Djvu);
    open_source(Arc::new(parse(bytes, djvu, true)?))
}

#[repr(C)]
struct SrcDoc {
    base: fz_document,
    /// From `Arc::into_raw`; released when MuPDF drops the document.
    source: *const Source,
    /// Pictures Kitab drew for this document (DjVu pages, WebP), newest
    /// last: tiles of one page at one zoom reuse one.
    drawn: *mut Mutex<Vec<Drawn>>,
}

struct Drawn {
    page: usize,
    width: u32,
    image: *mut fz_image,
}

/// How many drawn pictures a document keeps (a page being tiled, and the
/// one next to it).
const KEEP_DRAWN: usize = 2;

#[repr(C)]
struct SrcPage {
    base: fz_page,
    index: usize,
    /// Size in points.
    w: f32,
    h: f32,
    /// A comic page MuPDF decodes itself; null when Kitab draws it.
    image: *mut fz_image,
    /// Pixels across at full size, for pages Kitab draws.
    native_w: u32,
}

unsafe extern "C" fn recognize(
    _ctx: *mut fz_context,
    _handler: *const fz_document_handler,
    _stream: *mut fz_stream,
    _dir: *mut fz_archive,
    _state: *mut *mut c_void,
    _free: *mut fz_document_recognize_state_free_fn,
) -> c_int {
    // Only ever this handler's own documents: those `open` is opening now.
    if PENDING.with(|p| p.borrow().is_some()) {
        100
    } else {
        0
    }
}

unsafe extern "C" fn open_doc(
    ctx: *mut fz_context,
    _handler: *const fz_document_handler,
    _stream: *mut fz_stream,
    _accel: *mut fz_stream,
    _dir: *mut fz_archive,
    _state: *mut c_void,
) -> *mut fz_document {
    if !PENDING.with(|p| p.borrow().is_some()) {
        // SAFETY: inside MuPDF's open, which catches; nothing to drop here.
        unsafe { fz_throw(ctx, UNSUPPORTED, c"no page source to open".as_ptr()) }
    }
    // SAFETY: allocates a zeroed document with refs = 1; throws only when
    // out of memory, with no Rust value alive to skip.
    let doc =
        unsafe { fz_new_document_of_size(ctx, size_of::<SrcDoc>() as c_int) }.cast::<SrcDoc>();
    let source = PENDING.with(|p| p.borrow().clone());
    // SAFETY: `doc` is a fresh SrcDoc-sized allocation MuPDF owns.
    unsafe {
        (*doc).source = source.map_or(std::ptr::null(), Arc::into_raw);
        (*doc).drawn = Box::into_raw(Box::new(Mutex::new(Vec::new())));
        (*doc).base.drop_document = Some(drop_doc);
        (*doc).base.count_pages = Some(count_pages);
        (*doc).base.load_page = Some(load_page);
        (*doc).base.lookup_metadata = Some(lookup_metadata);
    }
    doc.cast()
}

/// The document's source. Valid while the document is.
unsafe fn source<'a>(doc: *mut fz_document) -> Option<&'a Source> {
    // SAFETY: every document this handler made is a SrcDoc.
    unsafe { (*doc.cast::<SrcDoc>()).source.as_ref() }
}

unsafe extern "C" fn drop_doc(ctx: *mut fz_context, doc: *mut fz_document) {
    // SAFETY: a SrcDoc this handler made; MuPDF frees the struct afterwards.
    unsafe {
        let doc = doc.cast::<SrcDoc>();
        if !(*doc).drawn.is_null() {
            let drawn = Box::from_raw((*doc).drawn);
            for d in drawn.into_inner().unwrap_or_else(|p| p.into_inner()) {
                fz_drop_image(ctx, d.image);
            }
            (*doc).drawn = null_mut();
        }
        if !(*doc).source.is_null() {
            drop(Arc::from_raw((*doc).source));
            (*doc).source = std::ptr::null();
        }
    }
}

unsafe extern "C" fn count_pages(
    _ctx: *mut fz_context,
    doc: *mut fz_document,
    _chapter: c_int,
) -> c_int {
    // SAFETY: see `source`.
    unsafe { source(doc) }.map_or(0, |s| c_int::try_from(s.page_count()).unwrap_or(c_int::MAX))
}

unsafe extern "C" fn lookup_metadata(
    _ctx: *mut fz_context,
    doc: *mut fz_document,
    key: *const c_char,
    buf: *mut c_char,
    size: usize,
) -> c_int {
    // SAFETY: MuPDF passes a valid key, and `buf` of `size` bytes (or null).
    let key = unsafe { CStr::from_ptr(key) };
    let Some(src) = (unsafe { source(doc) }) else {
        return -1;
    };
    let value: Option<std::ffi::CString> = match (key.to_bytes(), src) {
        (b"format", Source::Comic(_)) => Some(c"Comic book".into()),
        (b"format", Source::Djvu(_)) => Some(c"DjVu".into()),
        (b"info:Title", Source::Djvu(d)) => {
            crate::djvu::title(d).and_then(|t| std::ffi::CString::new(t).ok())
        }
        _ => None,
    };
    let Some(value) = value else { return -1 };
    if !buf.is_null() && size > 0 {
        // SAFETY: copies at most `size` bytes, terminated; never throws.
        unsafe { fz_strlcpy(buf, value.as_ptr(), size) };
    }
    c_int::try_from(value.as_bytes().len() + 1).unwrap_or(-1)
}

unsafe extern "C" fn load_page(
    ctx: *mut fz_context,
    doc: *mut fz_document,
    _chapter: c_int,
    number: c_int,
) -> *mut fz_page {
    // SAFETY: see `source`.
    let count = unsafe { source(doc) }.map_or(0, Source::page_count);
    let index = match usize::try_from(number) {
        Ok(i) if i < count => i,
        // SAFETY: inside MuPDF's load_page, which catches; nothing to drop.
        _ => unsafe { fz_throw(ctx, ARGUMENT, c"page out of range".as_ptr()) },
    };
    // SAFETY: a zeroed page holding a reference to `doc`; throws only when
    // out of memory, with no Rust value alive to skip.
    let page =
        unsafe { fz_new_page_of_size(ctx, size_of::<SrcPage>() as c_int, doc) }.cast::<SrcPage>();
    // SAFETY: `page` is a fresh SrcPage-sized allocation; the source outlives it.
    unsafe {
        (*page).base.drop_page = Some(drop_page);
        (*page).base.bound_page = Some(bound_page);
        (*page).base.run_page_contents = Some(run_page);
        (*page).index = index;
        let (w, h, image, native_w) = match source(doc) {
            Some(Source::Comic(c)) => comic_page(ctx, &c.pages[index].data),
            Some(Source::Djvu(d)) => {
                let size = crate::djvu::page_size(d, index).unwrap_or(FALLBACK);
                let native = d.page(index).map_or(1, |p| u32::from(p.width().max(1)));
                (size.w, size.h, null_mut(), native)
            }
            None => (FALLBACK.w, FALLBACK.h, null_mut(), 1),
        };
        (*page).w = w;
        (*page).h = h;
        (*page).image = image;
        (*page).native_w = native_w;
    }
    page.cast()
}

const FALLBACK: kitab_core::SizePt = kitab_core::SizePt { w: 612.0, h: 792.0 };

/// A comic page: MuPDF's image of the picture (no copy, decoded when drawn)
/// and its size in points; or, for WebP, just the size (Kitab draws it).
///
/// SAFETY: `data` outlives the image (the document holds the source, and a
/// cached image is never decoded again after its document is gone).
unsafe fn comic_page(ctx: *mut fz_context, data: &[u8]) -> (f32, f32, *mut fz_image, u32) {
    if is_webp(data) {
        let (w, h) = webp_size(data).unwrap_or((612, 792));
        return (w as f32 * 0.75, h as f32 * 0.75, null_mut(), w);
    }
    // SAFETY: wraps the bytes without copying; throws only when out of memory
    // (no Rust value alive to skip).
    let buf = unsafe { fz_new_buffer_from_shared_data(ctx, data.as_ptr(), data.len()) };
    let mut err: *mut mupdf_error_t = null_mut();
    // SAFETY: the wrapper catches and reports through `err`.
    let image = unsafe { mupdf_new_image_from_buffer(ctx, buf, &raw mut err) };
    // SAFETY: the image keeps its own reference to the buffer.
    unsafe { fz_drop_buffer(ctx, buf) };
    if !err.is_null() || image.is_null() {
        // SAFETY: dropping what the wrapper made.
        unsafe { mupdf_drop_error(err) };
        return (FALLBACK.w, FALLBACK.h, null_mut(), 0);
    }
    let (mut xres, mut yres) = (0, 0);
    // SAFETY: a valid image; never throws.
    unsafe { fz_image_resolution(image, &raw mut xres, &raw mut yres) };
    // SAFETY: a valid image.
    let (w, h) = unsafe { ((*image).w, (*image).h) };
    let (xres, yres) = (
        if xres > 0 { xres } else { 96 },
        if yres > 0 { yres } else { 96 },
    );
    (
        w as f32 * 72.0 / xres as f32,
        h as f32 * 72.0 / yres as f32,
        image,
        u32::try_from(w).unwrap_or(0),
    )
}

unsafe extern "C" fn drop_page(ctx: *mut fz_context, page: *mut fz_page) {
    // SAFETY: a SrcPage this handler made; MuPDF frees the struct afterwards.
    unsafe {
        let page = page.cast::<SrcPage>();
        if !(*page).image.is_null() {
            fz_drop_image(ctx, (*page).image);
            (*page).image = null_mut();
        }
    }
}

unsafe extern "C" fn bound_page(
    _ctx: *mut fz_context,
    page: *mut fz_page,
    _box: fz_box_type,
) -> fz_rect {
    // SAFETY: a SrcPage this handler made.
    let page = unsafe { &*page.cast::<SrcPage>() };
    fz_rect {
        x0: 0.0,
        y0: 0.0,
        x1: page.w,
        y1: page.h,
    }
}

unsafe extern "C" fn run_page(
    ctx: *mut fz_context,
    page: *mut fz_page,
    dev: *mut fz_device,
    ctm: fz_matrix,
    _cookie: *mut fz_cookie,
) {
    // SAFETY: a SrcPage this handler made, on a live document.
    let (p, doc) = unsafe { (&*page.cast::<SrcPage>(), (*page).doc) };
    if !p.image.is_null() {
        // SAFETY: valid pointers; the wrapper catches.
        unsafe { fill(ctx, dev, p.image, ctm, p.w, p.h) };
        return;
    }
    // Pixels across the page is drawn at; the full size when the page is
    // drawn unscaled (a PDF being written), never more than the full size.
    let identity = ctm.a == 1.0 && ctm.b == 0.0 && ctm.c == 0.0 && ctm.d == 1.0;
    let expansion = (ctm.a * ctm.d - ctm.b * ctm.c).abs().sqrt();
    let width = if identity {
        p.native_w
    } else {
        ((p.w * expansion).ceil() as u32).clamp(1, p.native_w.max(1))
    };
    // SAFETY: a SrcDoc's fields, alive while the document is.
    let (src, drawn) = unsafe {
        let d = doc.cast::<SrcDoc>();
        (source(doc), (*d).drawn.as_ref())
    };
    let (Some(src), Some(drawn)) = (src, drawn) else {
        return;
    };
    let mut drawn = drawn.lock().unwrap_or_else(|e| e.into_inner());
    let image = match drawn
        .iter()
        .position(|d| d.page == p.index && d.width == width)
    {
        Some(i) => drawn[i].image,
        None => {
            // SAFETY: valid context; returns null on any failure.
            let image = unsafe { draw(ctx, src, p.index, width) };
            if image.is_null() {
                return;
            }
            if drawn.len() >= KEEP_DRAWN {
                let old = drawn.remove(0);
                // SAFETY: an image this document made; never throws.
                unsafe { fz_drop_image(ctx, old.image) };
            }
            drawn.push(Drawn {
                page: p.index,
                width,
                image,
            });
            image
        }
    };
    // SAFETY: valid pointers; the wrapper catches.
    unsafe { fill(ctx, dev, image, ctm, p.w, p.h) };
}

/// Draw `image` over the page's `w` × `h` points.
unsafe fn fill(
    ctx: *mut fz_context,
    dev: *mut fz_device,
    image: *mut fz_image,
    ctm: fz_matrix,
    w: f32,
    h: f32,
) {
    let m = fz_matrix {
        a: w * ctm.a,
        b: w * ctm.b,
        c: h * ctm.c,
        d: h * ctm.d,
        e: ctm.e,
        f: ctm.f,
    };
    let mut err: *mut mupdf_error_t = null_mut();
    // SAFETY: valid pointers; the wrapper catches and reports through `err`.
    unsafe {
        mupdf_fill_image(
            ctx,
            dev,
            image,
            m,
            1.0,
            fz_default_color_params,
            &raw mut err,
        );
        mupdf_drop_error(err);
    }
}

/// A page Kitab draws (DjVu, or a WebP comic page) as a MuPDF image `width`
/// pixels across. Null when it can't be drawn.
unsafe fn draw(ctx: *mut fz_context, src: &Source, index: usize, width: u32) -> *mut fz_image {
    let pixels = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match src {
        Source::Djvu(d) => djvu_pixels(d, index, width),
        Source::Comic(c) => webp_pixels(&c.pages[index].data),
    }));
    let Ok(Some((w, h, rgba))) = pixels else {
        return null_mut();
    };
    let (Ok(wi), Ok(hi)) = (c_int::try_from(w), c_int::try_from(h)) else {
        return null_mut();
    };
    let mut err: *mut mupdf_error_t = null_mut();
    // SAFETY: the wrapper catches and reports through `err`.
    let pix =
        unsafe { mupdf_new_pixmap(ctx, fz_device_rgb(ctx), 0, 0, wi, hi, false, &raw mut err) };
    if !err.is_null() || pix.is_null() {
        // SAFETY: dropping what the wrapper made.
        unsafe { mupdf_drop_error(err) };
        return null_mut();
    }
    // SAFETY: a w × h RGB pixmap: `stride` bytes a row, h rows.
    unsafe {
        let stride = usize::try_from(fz_pixmap_stride(ctx, pix)).unwrap_or(0);
        let samples = fz_pixmap_samples(ctx, pix);
        let row = w as usize * 4;
        for (y, src_row) in rgba.chunks_exact(row).take(h as usize).enumerate() {
            let dst = std::slice::from_raw_parts_mut(samples.add(y * stride), w as usize * 3);
            let src = src_row.as_chunks::<4>().0;
            for (d, s) in dst.as_chunks_mut::<3>().0.iter_mut().zip(src) {
                d.copy_from_slice(&s[..3]);
            }
        }
    }
    // SAFETY: the wrapper catches; the image keeps its own pixmap reference.
    let image = unsafe { mupdf_new_image_from_pixmap(ctx, pix, &raw mut err) };
    // SAFETY: dropping our reference and any error.
    unsafe {
        fz_drop_pixmap(ctx, pix);
        if !err.is_null() {
            mupdf_drop_error(err);
            return null_mut();
        }
    }
    image
}

/// A DjVu page `width` pixels across (in its own orientation), RGBA.
fn djvu_pixels(doc: &DjVuDocument, index: usize, width: u32) -> Option<(u32, u32, Vec<u8>)> {
    use djvu_rs::djvu_render::{RenderOptions, render_pixmap};
    let page = doc.page(index).ok()?;
    let (pw, ph) = (
        u32::from(page.width().max(1)),
        u32::from(page.height().max(1)),
    );
    // `width` is across the page as shown; a turned page is drawn from its height.
    let turned = matches!(
        page.rotation(),
        djvu_rs::Rotation::Ccw90 | djvu_rs::Rotation::Cw90
    );
    let scale = f64::from(width) / f64::from(if turned { ph } else { pw });
    let opts = RenderOptions {
        width: ((f64::from(pw) * scale).round() as u32).clamp(1, pw),
        height: ((f64::from(ph) * scale).round() as u32).clamp(1, ph),
        ..Default::default()
    };
    let pix = render_pixmap(page, &opts).ok()?;
    // Keep djvu-rs's decoded layers in bounds: this page's, little else.
    doc.enforce_cache_budget(48 << 20, &[index]);
    Some((pix.width, pix.height, pix.data))
}

fn is_webp(data: &[u8]) -> bool {
    data.len() > 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP"
}

fn webp_size(data: &[u8]) -> Option<(u32, u32)> {
    let d = image_webp::WebPDecoder::new(std::io::Cursor::new(data)).ok()?;
    Some(d.dimensions())
}

fn webp_pixels(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(data)).ok()?;
    let (w, h) = d.dimensions();
    let has_alpha = d.has_alpha();
    let mut buf = vec![0u8; d.output_buffer_size()?];
    d.read_image(&mut buf).ok()?;
    let rgba = if has_alpha {
        // Over white paper.
        buf.as_chunks_mut::<4>().0.iter_mut().for_each(|p| {
            let a = u16::from(p[3]);
            for c in &mut p[..3] {
                *c = ((u16::from(*c) * a + 255 * (255 - a)) / 255) as u8;
            }
        });
        buf
    } else {
        buf.as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect()
    };
    Some((w, h, rgba))
}
