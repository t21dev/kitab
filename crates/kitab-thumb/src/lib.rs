// SPDX-License-Identifier: AGPL-3.0-or-later

//! File-manager thumbnails (PLAN.md §3.9 "Thumbnails").
//!
//! Documents MuPDF reads (PDF now, EPUB, XPS, CBZ… with Phase 4) show their
//! first page. Markdown and plain text show a small page preview Kitab lays
//! out itself with MuPDF's built-in fonts (no HTML engine, no system fonts):
//! headings, paragraphs, lists, quotes, code boxes and tables. Characters the
//! built-in fonts lack are drawn as grey bars, so a page in any script still
//! reads as a page.
//!
//! Every system calls the same code through `kitab --thumbnail`, which runs in
//! its own process (a hostile file can only take that process down) and exits
//! before any window code runs:
//!
//! * `kitab --thumbnail -s SIZE IN OUT.png`: Linux thumbnailers (freedesktop
//!   `.thumbnailer` files), and anyone else wanting a PNG.
//! * `kitab --thumbnail -s SIZE --stdin EXT --bgra`: the Windows thumbnail
//!   provider pipes the file in and reads `width, height` (little-endian
//!   `u32`s) and opaque BGRA rows back.

mod markdown;

use kitab_core::Format;
use mupdf::{Colorspace, Document, ImageFormat, Matrix, Pixmap};
use std::io::{Read, Write};
use std::path::Path;

/// File managers ask for 16–1024 px; anything else is clamped.
pub const MIN_SIZE: u32 = 16;
pub const MAX_SIZE: u32 = 1024;
/// The most a thumbnail request may pipe in (Windows hands over a stream).
pub const MAX_BYTES: u64 = 1 << 30;
/// Markdown and text previews read only the start of the file.
const TEXT_HEAD: usize = 64 * 1024;

/// A rendered thumbnail: opaque RGB.
pub struct Thumb {
    pixmap: Pixmap,
}

impl Thumb {
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    pub fn to_png(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        self.pixmap
            .write_to(&mut out, ImageFormat::PNG)
            .map_err(|e| e.to_string())?;
        Ok(out)
    }

    /// Top-down BGRA rows with alpha 255 (what a Windows DIB section takes).
    pub fn to_bgra(&self) -> Vec<u8> {
        let (w, h) = (self.width() as usize, self.height() as usize);
        let n = self.pixmap.n() as usize;
        let stride = self.pixmap.stride().unsigned_abs();
        let samples = self.pixmap.samples();
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let row = &samples[y * stride..];
            for x in 0..w {
                let p = &row[x * n..x * n + 3];
                out.extend_from_slice(&[p[2], p[1], p[0], 255]);
            }
        }
        out
    }

    /// Whether any pixel differs from the first one (a test helper: a blank
    /// thumbnail means nothing was drawn).
    pub fn has_content(&self) -> bool {
        let bgra = self.to_bgra();
        let first = &bgra[..4];
        bgra.as_chunks::<4>().0.iter().any(|p| p != first)
    }
}

/// Where the document comes from: a path (Linux, the CLI) or bytes (Windows
/// pipes the stream in).
pub enum Source<'a> {
    Path(&'a Path),
    Bytes(&'a [u8]),
}

/// Render a thumbnail whose longer side is `size` px. `ext` names the format
/// (without the dot); it decides between a page render and a text preview.
pub fn render(source: Source<'_>, ext: &str, size: u32) -> Result<Thumb, String> {
    kitab_mupdf::fonts::install();
    let size = size.clamp(MIN_SIZE, MAX_SIZE);
    match Format::from_extension(ext) {
        Some(Format::Markdown) => markdown::render(&text_head(&source)?, false, size),
        Some(Format::Txt) => markdown::render(&text_head(&source)?, true, size),
        // Kitab doesn't view image files (the user's decision): no thumbnails.
        Some(Format::Image) => Err("Kitab doesn't open image files".into()),
        Some(_) => first_page(&source, ext, size),
        None => Err(format!("Kitab doesn't read .{ext} files")),
    }
}

/// The start of a text file, cut at a line end so no block is half read.
fn text_head(source: &Source<'_>) -> Result<String, String> {
    let bytes: Vec<u8> = match source {
        Source::Bytes(b) => b[..b.len().min(TEXT_HEAD)].to_vec(),
        Source::Path(p) => {
            let mut buf = Vec::new();
            std::fs::File::open(p)
                .and_then(|f| f.take(TEXT_HEAD as u64).read_to_end(&mut buf))
                .map_err(|e| e.to_string())?;
            buf
        }
    };
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if bytes.len() == TEXT_HEAD
        && let Some(end) = text.rfind('\n')
    {
        text.truncate(end);
    }
    Ok(text)
}

fn first_page(source: &Source<'_>, ext: &str, size: u32) -> Result<Thumb, String> {
    let err = |e: mupdf::Error| e.to_string();
    // Comics in RAR or 7z and DjVu: Kitab's own reader, unpacking only the
    // first page.
    let head: Vec<u8> = match source {
        Source::Bytes(b) => b[..b.len().min(512)].to_vec(),
        Source::Path(p) => {
            let mut head = Vec::new();
            std::fs::File::open(p)
                .and_then(|f| f.take(512).read_to_end(&mut head))
                .map_err(|e| e.to_string())?;
            head
        }
    };
    let doc = if kitab_mupdf::reads_itself(ext, &head) {
        let bytes = match source {
            Source::Bytes(b) => std::borrow::Cow::Borrowed(*b),
            Source::Path(p) => {
                std::borrow::Cow::Owned(std::fs::read(p).map_err(|e| e.to_string())?)
            }
        };
        kitab_mupdf::open_first_page(&bytes, ext)?
    } else {
        match source {
            Source::Path(p) => {
                let p = p.to_str().ok_or("path is not valid Unicode")?;
                Document::open(p).map_err(err)?
            }
            Source::Bytes(b) => Document::from_bytes(b, &format!("file.{ext}")).map_err(err)?,
        }
    };
    // A locked document shows its file icon: never guess or prompt.
    if doc.needs_password().map_err(err)? {
        return Err("the document is password-protected".into());
    }
    if doc.page_count().map_err(err)? < 1 {
        return Err("the document has no pages".into());
    }
    let page = doc.load_page(0).map_err(err)?;
    let b = page.bounds().map_err(err)?;
    let (w, h) = (b.x1 - b.x0, b.y1 - b.y0);
    if !(w.is_finite() && h.is_finite() && w >= 1.0 && h >= 1.0) {
        return Err("the first page has no size".into());
    }
    // The longer side fits `size`; MuPDF keeps at least one pixel on the other.
    let scale = size as f32 / w.max(h);
    let pixmap = page
        .to_pixmap(
            &Matrix::new_scale(scale, scale),
            &Colorspace::device_rgb(),
            false,
            true,
        )
        .map_err(err)?;
    Ok(Thumb { pixmap })
}

/// `kitab --thumbnail …` (arguments after `--thumbnail`). Returns the exit
/// code; errors go to stderr without paths (AGENTS.md rule 13).
pub fn cli(args: &[String], stdin: &mut dyn Read, stdout: &mut dyn Write) -> i32 {
    match run_cli(args, stdin, stdout) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("kitab --thumbnail: {e}");
            1
        }
    }
}

fn run_cli(args: &[String], stdin: &mut dyn Read, stdout: &mut dyn Write) -> Result<(), String> {
    let mut size = 256u32;
    let mut stdin_ext: Option<String> = None;
    let mut bgra = false;
    let mut files: Vec<&String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-s" | "--size" => {
                size = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("-s takes a size in pixels")?;
            }
            "--stdin" => stdin_ext = Some(it.next().ok_or("--stdin takes an extension")?.clone()),
            "--bgra" => bgra = true,
            _ => files.push(a),
        }
    }
    let thumb = match (&stdin_ext, files.as_slice()) {
        (Some(ext), []) => {
            let mut bytes = Vec::new();
            stdin
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err("the file is too large for a thumbnail".into());
            }
            render(Source::Bytes(&bytes), ext.trim_start_matches('.'), size)?
        }
        (None, [input, _out]) => {
            let path = Path::new(input.as_str());
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            render(Source::Path(path), ext, size)?
        }
        _ => {
            return Err(
                "usage: kitab --thumbnail -s SIZE IN OUT.png | -s SIZE --stdin EXT --bgra".into(),
            );
        }
    };
    if bgra {
        stdout
            .write_all(&thumb.width().to_le_bytes())
            .and_then(|_| stdout.write_all(&thumb.height().to_le_bytes()))
            .and_then(|_| stdout.write_all(&thumb.to_bgra()))
            .and_then(|_| stdout.flush())
            .map_err(|e| e.to_string())
    } else {
        let png = thumb.to_png()?;
        match files.as_slice() {
            [_, out] => std::fs::write(out.as_str(), png).map_err(|e| e.to_string()),
            _ => stdout.write_all(&png).map_err(|e| e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests;
