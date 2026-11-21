// SPDX-License-Identifier: AGPL-3.0-or-later

//! A document's content as Markdown or plain text (Export as Markdown…,
//! `kitab markdown`, `kitab text`): every format MuPDF opens (PDF, EPUB,
//! MOBI, FB2, XPS, CBZ). Headings come from font size, bold and italic from
//! the fonts, lists from their bullets and numbers; pictures can be saved
//! beside the Markdown and linked.
//!
//! The work is done in two passes. The first reads each page once into a
//! small model of lines and styled runs (pictures are saved as they are met,
//! so no pixmap outlives its page). The second needs the whole document:
//! the body text size, the heading sizes and the running headers only show
//! once every page has been seen.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use mupdf::text_page::TextBlockType;
use mupdf::{Colorspace, Document, Image, ImageFormat, Pixmap, TextCharFlags, TextPageFlags};

/// What `to_markdown` writes.
#[derive(Debug, Clone, Default)]
pub struct MarkdownOptions {
    /// 0-based pages to include, in this order; `None` for all.
    pub pages: Option<Vec<u32>>,
    /// Save pictures and link them; `None` leaves pictures out.
    pub images: Option<ImageOut>,
    /// An HTML comment before each page: `<!-- page 3 -->`.
    pub page_markers: bool,
}

/// Where pictures go, and how the Markdown links them.
#[derive(Debug, Clone)]
pub struct ImageOut {
    /// The folder pictures are saved in (created if missing).
    pub dir: PathBuf,
    /// Put before each picture's file name in its link, usually the folder's
    /// name relative to the Markdown file ("report_images/").
    pub link_prefix: String,
}

/// A finished export.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exported {
    /// The Markdown or text.
    pub text: String,
    /// Pages read.
    pub pages: u32,
    /// Pictures saved.
    pub images: u32,
}

/// The document at `src` (opened with `password`) as Markdown.
pub fn to_markdown(
    src: &Path,
    password: Option<&str>,
    opts: &MarkdownOptions,
) -> Result<Exported, String> {
    let doc = open(src, password)?;
    let picked = pick_pages(&doc, opts.pages.as_deref())?;
    if let Some(out) = &opts.images {
        fs::create_dir_all(&out.dir)
            .map_err(|e| format!("couldn't make the picture folder: {e}"))?;
    }

    let mut sizes: HashMap<u16, usize> = HashMap::new();
    let mut images = 0u32;
    let mut read = Vec::with_capacity(picked.len());
    for &index in &picked {
        read.push(read_page(
            &doc,
            index,
            opts.images.as_ref(),
            &mut sizes,
            &mut images,
        )?);
    }

    let prefix = opts.images.as_ref().map_or("", |o| o.link_prefix.as_str());
    let text = render(&read, &sizes, opts.page_markers, prefix);
    Ok(Exported {
        text: one_newline(&text),
        pages: count(picked.len()),
        images,
    })
}

/// The document at `src` (opened with `password`) as plain text, pages
/// separated by a blank line; `pages` (0-based) limits it.
pub fn to_text(
    src: &Path,
    password: Option<&str>,
    pages: Option<&[u32]>,
) -> Result<Exported, String> {
    let doc = open(src, password)?;
    let picked = pick_pages(&doc, pages)?;
    let mut parts = Vec::with_capacity(picked.len());
    for &index in &picked {
        let page = doc
            .load_page(page_no(index)?)
            .map_err(|e| unreadable(index, e))?;
        let text = page
            .to_text_page(TextPageFlags::empty())
            .and_then(|t| t.to_text())
            .map_err(|e| unreadable(index, e))?;
        // MuPDF's text ends blocks with spaces and pages with a form feed;
        // neither is wanted in a .txt.
        let lines: Vec<&str> = text
            .split('\n')
            .map(|l| l.trim_end_matches(|c: char| c.is_whitespace() || c == '\u{c}'))
            .collect();
        let page_text = lines.join("\n");
        let page_text = page_text.trim_matches('\n');
        if !page_text.is_empty() {
            parts.push(page_text.to_string());
        }
    }
    Ok(Exported {
        text: one_newline(&parts.join("\n\n")),
        pages: count(picked.len()),
        images: 0,
    })
}

// ---------------------------------------------------------------------------
// Opening and choosing pages

/// Open any format MuPDF reads. Books are laid out at the reader's default
/// page size, so their pages match what the reader shows.
fn open(src: &Path, password: Option<&str>) -> Result<Document, String> {
    let file = src
        .to_str()
        .ok_or("this file's name has characters Kitab can't pass to MuPDF")?;
    let mut doc = Document::open(file)
        .map_err(|_| "this file couldn't be opened: it's damaged or not a document".to_string())?;
    if doc.needs_password().unwrap_or(false) {
        let Some(password) = password else {
            return Err("this PDF needs its password".into());
        };
        if !doc.authenticate(password).unwrap_or(false) {
            return Err("that password isn't right".into());
        }
    }
    if doc.is_reflowable().unwrap_or(false) {
        doc.layout(crate::REFLOW_W, crate::REFLOW_H, crate::REFLOW_EM)
            .map_err(|e| format!("couldn't lay out the book: {e}"))?;
    }
    Ok(doc)
}

/// The 0-based pages to read, checked against the document.
fn pick_pages(doc: &Document, pages: Option<&[u32]>) -> Result<Vec<u32>, String> {
    let total = doc
        .page_count()
        .ok()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or("couldn't count the document's pages")?;
    match pages {
        None => Ok((0..total).collect()),
        Some([]) => Err("no pages were chosen".into()),
        Some(list) => {
            if let Some(&bad) = list.iter().find(|&&p| p >= total) {
                let noun = if total == 1 { "page" } else { "pages" };
                return Err(format!(
                    "page {} doesn't exist; the document has {total} {noun}",
                    u64::from(bad) + 1
                ));
            }
            Ok(list.to_vec())
        }
    }
}

fn page_no(index: u32) -> Result<i32, String> {
    i32::try_from(index).map_err(|_| format!("page {} is out of range", u64::from(index) + 1))
}

fn unreadable(index: u32, e: impl std::fmt::Display) -> String {
    format!("couldn't read page {}: {e}", u64::from(index) + 1)
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Text that ends with exactly one newline.
fn one_newline(s: &str) -> String {
    let mut out = s.trim_end().to_string();
    out.push('\n');
    out
}

// ---------------------------------------------------------------------------
// Pass 1: pages into lines

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Style {
    bold: bool,
    italic: bool,
}

/// A run of text in one style.
type Span = (String, Style);

/// One line of text as MuPDF found it.
#[derive(Debug, Clone)]
struct Line {
    spans: Vec<Span>,
    /// The line's most common character size, in half points.
    size: u16,
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    /// Where each word sits, so tables can be found from the layout.
    words: Vec<Word>,
}

/// A word of a line: its characters in the line's text (counted in chars,
/// end excluded) and its left and right edges on the page.
#[derive(Debug, Clone, Copy)]
struct Word {
    start: usize,
    end: usize,
    x0: f32,
    x1: f32,
}

impl Line {
    fn text(&self) -> String {
        self.spans.iter().map(|(t, _)| t.as_str()).collect()
    }

    fn all_bold(&self) -> bool {
        self.spans
            .iter()
            .all(|(t, s)| s.bold || t.chars().all(|c| !c.is_alphanumeric()))
    }
}

#[derive(Debug)]
enum Item {
    Text(Vec<Line>),
    /// A saved picture's file name.
    Picture(String),
}

#[derive(Debug)]
struct PageData {
    /// The 0-based page number.
    index: u32,
    top: f32,
    bottom: f32,
    items: Vec<Item>,
}

/// Pictures smaller than this on either side are rules, bullets and specks.
const MIN_PICTURE_SIDE: u32 = 24;

/// Pictures with more pixels than this aren't decoded (a 20 000 × 20 000 scan
/// would need over a gigabyte).
const MAX_PICTURE_PIXELS: u64 = 100_000_000;

/// Read one page into lines and pictures, adding its character sizes to
/// `sizes` and saving its pictures (when `images` asks) as it goes.
fn read_page(
    doc: &Document,
    index: u32,
    images: Option<&ImageOut>,
    sizes: &mut HashMap<u16, usize>,
    saved: &mut u32,
) -> Result<PageData, String> {
    let page = doc
        .load_page(page_no(index)?)
        .map_err(|e| unreadable(index, e))?;
    let bounds = page.bounds().map_err(|e| unreadable(index, e))?;
    let flags = if images.is_some() {
        TextPageFlags::PRESERVE_IMAGES
    } else {
        TextPageFlags::empty()
    };
    let stext = page.to_text_page(flags).map_err(|e| unreadable(index, e))?;

    let mut items = Vec::new();
    let mut pictures_here = 0u32;
    for block in stext.blocks() {
        match block.r#type() {
            TextBlockType::Text => {
                let lines: Vec<Line> = block
                    .lines()
                    .filter_map(|line| read_line(&line, sizes))
                    .collect();
                if !lines.is_empty() {
                    items.push(Item::Text(lines));
                }
            }
            TextBlockType::Image => {
                let (Some(out), Some(image)) = (images, block.image()) else {
                    continue;
                };
                if image.width() < MIN_PICTURE_SIDE || image.height() < MIN_PICTURE_SIDE {
                    continue;
                }
                let name = format!("p{}-{}.png", u64::from(index) + 1, pictures_here + 1);
                // A picture MuPDF can't decode is left out rather than
                // failing the whole export; a folder we can't write to is
                // an error, since every picture would fail the same way.
                match save_picture(&image, &out.dir.join(&name)) {
                    Ok(true) => {
                        pictures_here += 1;
                        *saved += 1;
                        items.push(Item::Picture(name));
                    }
                    Ok(false) => {}
                    Err(e) => return Err(e),
                }
            }
            _ => {}
        }
    }
    Ok(PageData {
        index,
        top: bounds.y0,
        bottom: bounds.y1,
        items,
    })
}

/// One line as styled runs, trimmed; `None` when it holds no text.
fn read_line(line: &mupdf::TextLine<'_>, sizes: &mut HashMap<u16, usize>) -> Option<Line> {
    let mut spans: Vec<Span> = Vec::new();
    let mut local: Vec<(u16, usize)> = Vec::new();
    let mut words: Vec<Word> = Vec::new();
    // Whether the last character was a space, so the next one starts a word.
    let mut after_space = true;
    let mut at = 0usize;
    for ch in line.chars() {
        let Some(c) = ch.char() else { continue };
        let style = char_style(&ch);
        if c.is_whitespace() {
            after_space = true;
        } else {
            let key = size_key(ch.size());
            match local.iter_mut().find(|(k, _)| *k == key) {
                Some((_, n)) => *n += 1,
                None => local.push((key, 1)),
            }
            let q = ch.quad();
            let (x0, x1) = (q.ul.x.min(q.ll.x), q.ur.x.max(q.lr.x));
            // A wide gap with no space character between still parts two
            // words: some PDFs place table cells without writing a space.
            match words.last_mut() {
                Some(w) if !after_space && x0 - w.x1 <= ch.size() * 0.3 => {
                    w.end = at + 1;
                    w.x1 = w.x1.max(x1);
                }
                _ => words.push(Word {
                    start: at,
                    end: at + 1,
                    x0,
                    x1,
                }),
            }
            after_space = false;
        }
        at += 1;
        match spans.last_mut() {
            Some((text, s)) if *s == style => text.push(c),
            _ => spans.push((c.to_string(), style)),
        }
    }
    let (size, _) = local.iter().copied().max_by_key(|&(_, n)| n)?;
    for (key, n) in local {
        *sizes.entry(key).or_insert(0) += n;
    }
    // Word positions count from the trimmed text's first character.
    let lead = spans
        .iter()
        .flat_map(|(t, _)| t.chars())
        .take_while(|c| c.is_whitespace())
        .count();
    for w in &mut words {
        w.start = w.start.saturating_sub(lead);
        w.end = w.end.saturating_sub(lead);
    }
    trim_spans(&mut spans);
    if spans.is_empty() {
        return None;
    }
    let b = line.bounds();
    Some(Line {
        spans,
        size,
        x0: b.x0,
        x1: b.x1,
        y0: b.y0,
        y1: b.y1,
        words,
    })
}

/// Sizes are compared in half points, so 11.02 and 10.98 count as one size.
fn size_key(size: f32) -> u16 {
    let k = (size * 2.0).round();
    if k.is_finite() && k > 0.0 {
        k.min(f32::from(u16::MAX)) as u16
    } else {
        0
    }
}

/// Bold and italic from MuPDF's flags and the font's name and flags. Names
/// are the most reliable clue in PDFs ("Minion-SemiboldIt"); the flags catch
/// fake bold and fonts with plain names.
fn char_style(ch: &mupdf::TextChar<'_>) -> Style {
    let mut style = Style {
        bold: ch.flags().contains(TextCharFlags::BOLD),
        italic: false,
    };
    if let Some(font) = ch.font() {
        let (bold, italic) = name_style(font.name());
        style.bold |= bold || font.is_bold();
        style.italic |= italic || font.is_italic();
    }
    style
}

/// Bold and italic as a font's name tells them.
fn name_style(name: &str) -> (bool, bool) {
    let lower = name.to_ascii_lowercase();
    let bold = [
        "bold",
        "black",
        "heavy",
        "semibold",
        "demibold",
        "extrabold",
    ]
    .iter()
    .any(|w| lower.contains(w));
    let italic = ["italic", "oblique"].iter().any(|w| lower.contains(w))
        || lower.ends_with("-it")
        || lower.ends_with("boldit");
    (bold, italic)
}

/// Drop leading and trailing whitespace, and runs left empty by it.
fn trim_spans(spans: &mut Vec<Span>) {
    if let Some((t, _)) = spans.first_mut() {
        *t = t.trim_start().to_string();
    }
    if let Some((t, _)) = spans.last_mut() {
        *t = t.trim_end().to_string();
    }
    spans.retain(|(t, _)| !t.is_empty());
    if let Some((t, _)) = spans.first_mut() {
        *t = t.trim_start().to_string();
    }
}

/// Save a picture as PNG. `Ok(false)` when it can't be decoded or is too big.
fn save_picture(image: &Image, path: &Path) -> Result<bool, String> {
    if u64::from(image.width()) * u64::from(image.height()) > MAX_PICTURE_PIXELS {
        return Ok(false);
    }
    let Ok(pixmap) = image.to_pixmap() else {
        return Ok(false);
    };
    let mut png = Vec::new();
    if pixmap.write_to(&mut png, ImageFormat::PNG).is_err() {
        // MuPDF writes PNG only from grey and RGB; CMYK scans need turning
        // into RGB first.
        png.clear();
        let Some(rgb) = cmyk_to_rgb(&pixmap) else {
            return Ok(false);
        };
        if rgb.write_to(&mut png, ImageFormat::PNG).is_err() {
            return Ok(false);
        }
    }
    fs::write(path, png).map_err(|e| format!("couldn't save a picture: {e}"))?;
    Ok(true)
}

/// A CMYK pixmap as RGB (the simple formula; colour management isn't worth
/// it for an exported picture). `None` for anything that isn't CMYK.
fn cmyk_to_rgb(src: &Pixmap) -> Option<Pixmap> {
    let n = usize::from(src.n());
    let alpha = src.alpha();
    if n != 4 + usize::from(alpha) {
        return None;
    }
    let w = src.width();
    let h = src.height();
    let mut dst = Pixmap::new_with_w_h(
        &Colorspace::device_rgb(),
        i32::try_from(w).ok()?,
        i32::try_from(h).ok()?,
        alpha,
    )
    .ok()?;
    let src_stride = usize::try_from(src.stride()).ok()?;
    let dst_stride = usize::try_from(dst.stride()).ok()?;
    let dn = 3 + usize::from(alpha);
    let (w, h) = (usize::try_from(w).ok()?, usize::try_from(h).ok()?);
    let from = src.samples();
    let to = dst.samples_mut();
    for y in 0..h {
        for x in 0..w {
            let s = from.get(y * src_stride + x * n..y * src_stride + x * n + n)?;
            let d = y * dst_stride + x * dn;
            let k = u16::from(s[3]);
            for (i, &c) in s[..3].iter().enumerate() {
                *to.get_mut(d + i)? = 255 - (u16::from(c) + k).min(255) as u8;
            }
            if alpha {
                *to.get_mut(d + 3)? = s[4];
            }
        }
    }
    Some(dst)
}

// ---------------------------------------------------------------------------
// Pass 2: lines into Markdown

/// A heading's characters are at least this much bigger than body text.
const HEADING_RATIO: f32 = 1.15;

/// Running headers and page numbers live in this share of the page's height
/// at the top and bottom.
const MARGIN_BAND: f32 = 0.12;

/// One finished piece of Markdown.
struct Chunk {
    text: String,
    /// Bullets and `1.` items: consecutive ones are kept on adjacent lines
    /// so they read as one list.
    list: bool,
    /// A plain paragraph, which may continue the one before it.
    para: bool,
}

fn render(pages: &[PageData], sizes: &HashMap<u16, usize>, markers: bool, prefix: &str) -> String {
    // The body size is the one most characters are set in; ties go to the
    // smaller size, which is the safer guess (fewer headings).
    let body = sizes
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
        .map_or(0, |(&k, _)| k);
    let running = running_headers(pages, body);

    // Keep every page's surviving blocks, then find the heading sizes.
    let kept: Vec<Vec<&Item>> = pages
        .iter()
        .map(|p| {
            p.items
                .iter()
                .filter(|item| match item {
                    Item::Text(lines) => {
                        let lines = surviving(p, lines, &running);
                        !lines.is_empty() && !is_page_number_block(p, &lines, body)
                    }
                    Item::Picture(_) => true,
                })
                .collect()
        })
        .collect();

    // Tables are found per page, before headings: a table's lines are
    // neither headings nor paragraphs.
    let tables: Vec<Vec<Table>> = pages
        .iter()
        .zip(&kept)
        .map(|(p, items)| find_tables(&placed_lines(p, items, &running)))
        .collect();

    let mut heading_sizes: Vec<u16> = Vec::new();
    for ((p, items), tables) in pages.iter().zip(&kept).zip(&tables) {
        let mut at = 0usize;
        for item in items {
            if let Item::Text(lines) = item {
                let lines = surviving(p, lines, &running);
                let mut outside = Vec::with_capacity(lines.len());
                for line in lines {
                    if table_at(tables, at).is_none() {
                        outside.push(line);
                    }
                    at += 1;
                }
                for run in runs(&outside, body) {
                    if let Some(size) = run.heading
                        && !heading_sizes.contains(&size)
                    {
                        heading_sizes.push(size);
                    }
                }
            }
        }
    }
    let levels = heading_levels(heading_sizes);

    let mut chunks: Vec<Chunk> = Vec::new();
    for ((p, items), tables) in pages.iter().zip(&kept).zip(&tables) {
        let mut at = 0usize;
        if markers {
            chunks.push(Chunk {
                text: format!("<!-- page {} -->", u64::from(p.index) + 1),
                list: false,
                para: false,
            });
        }
        for item in items {
            match item {
                Item::Picture(name) => chunks.push(Chunk {
                    text: picture_link(prefix, name),
                    list: false,
                    para: false,
                }),
                Item::Text(lines) => {
                    let lines = surviving(p, lines, &running);
                    // A one-line block keeps its run-in heading test even
                    // when a table took the rest of its neighbours.
                    let single = lines.len() == 1;
                    let mut stretch: Vec<&Line> = Vec::new();
                    for line in lines {
                        match table_at(tables, at) {
                            Some(table) => {
                                render_block(&stretch, single, body, &levels, &mut chunks);
                                stretch.clear();
                                if table.first == at {
                                    chunks.push(Chunk {
                                        text: table.text.clone(),
                                        list: false,
                                        para: false,
                                    });
                                }
                            }
                            None => stretch.push(line),
                        }
                        at += 1;
                    }
                    render_block(&stretch, single, body, &levels, &mut chunks);
                }
            }
        }
    }

    let mut out = String::new();
    let mut prev: Option<&Chunk> = None;
    for chunk in &chunks {
        match prev {
            None => {}
            Some(p) if continues(p, chunk) => {
                // MuPDF splits a paragraph where a page ends or a picture
                // floats beside it; a lowercase start after an unfinished
                // sentence is the same paragraph going on.
                let hyphen =
                    out.ends_with('-') && out.chars().rev().nth(1).is_some_and(char::is_alphabetic);
                if hyphen {
                    out.pop();
                } else {
                    out.push(' ');
                }
            }
            Some(p) if p.list && chunk.list => out.push('\n'),
            Some(_) => out.push_str("\n\n"),
        }
        out.push_str(&chunk.text);
        prev = Some(chunk);
    }
    out
}

/// Whether paragraph `next` carries on paragraph `prev`.
fn continues(prev: &Chunk, next: &Chunk) -> bool {
    prev.para
        && next.para
        && next.text.starts_with(char::is_lowercase)
        && !prev
            .text
            .ends_with(['.', '!', '?', ':', ';', '"', '\u{201d}', ')'])
}

/// `![](prefix + name)`, in angle brackets when the link has spaces.
fn picture_link(prefix: &str, name: &str) -> String {
    let link = format!("{prefix}{name}");
    if link.contains(' ') {
        format!("![](<{link}>)")
    } else {
        format!("![]({link})")
    }
}

fn surviving<'a>(p: &PageData, lines: &'a [Line], running: &HashSet<HeaderKey>) -> Vec<&'a Line> {
    lines
        .iter()
        .filter(|l| !running.contains(&header_key(p, l)))
        .collect()
}

/// A line's place in the top or bottom margin and its text without page
/// numbers at either end; `None` outside the margins.
type HeaderKey = Option<(bool, i32, String)>;

fn header_key(p: &PageData, line: &Line) -> HeaderKey {
    let h = p.bottom - p.top;
    if h <= 0.0 {
        return None;
    }
    let at_top = line.y1 <= p.top + h * MARGIN_BAND;
    let at_bottom = line.y0 >= p.bottom - h * MARGIN_BAND;
    if !at_top && !at_bottom {
        return None;
    }
    let text = line.text();
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut a = 0;
    let mut b = words.len();
    while a < b && is_number_word(words[a]) {
        a += 1;
    }
    while b > a && is_number_word(words[b - 1]) {
        b -= 1;
    }
    let core = words[a..b].join(" ");
    if core.is_empty() {
        return None;
    }
    Some((at_top, (line.y0 / 4.0).round() as i32, core))
}

/// Lines repeated at the same place in the margin on more than half the
/// pages (four pages at least, so a short document keeps its title). Only
/// lines no bigger than body text count, so chapter titles that happen to
/// start every page are never taken for running headers.
fn running_headers(pages: &[PageData], body: u16) -> HashSet<HeaderKey> {
    let mut seen: HashMap<HeaderKey, usize> = HashMap::new();
    if pages.len() < 4 {
        return HashSet::new();
    }
    for p in pages {
        let mut here = HashSet::new();
        for item in &p.items {
            if let Item::Text(lines) = item {
                for line in lines {
                    if line.size > body.saturating_add(1) {
                        continue;
                    }
                    let key = header_key(p, line);
                    if key.is_some() {
                        here.insert(key);
                    }
                }
            }
        }
        for key in here {
            *seen.entry(key).or_insert(0) += 1;
        }
    }
    seen.into_iter()
        .filter(|(_, n)| n * 2 > pages.len())
        .map(|(k, _)| k)
        .collect()
}

/// A block that is only a page number, in the top or bottom margin.
fn is_page_number_block(p: &PageData, lines: &[&Line], body: u16) -> bool {
    let h = p.bottom - p.top;
    let y0 = lines.iter().map(|l| l.y0).fold(f32::MAX, f32::min);
    let y1 = lines.iter().map(|l| l.y1).fold(f32::MIN, f32::max);
    let in_margin = h > 0.0 && (y1 <= p.top + h * MARGIN_BAND || y0 >= p.bottom - h * MARGIN_BAND);
    // Heading-sized text is never a page number ("Page 1" as a title).
    let small = lines.iter().all(|l| l.size <= body.saturating_add(1));
    let text: Vec<String> = lines.iter().map(|l| l.text()).collect();
    in_margin && small && is_page_number(&text.join(" "))
}

/// "12", "xii", "Page 3", "3 of 10", "- 4 -", "5 / 9".
fn is_page_number(text: &str) -> bool {
    let lower = text.to_lowercase();
    let mut numbers = 0;
    for word in lower.split(|c: char| c.is_whitespace() || matches!(c, '/' | '|')) {
        let word =
            word.trim_matches(|c| matches!(c, '-' | '–' | '—' | '.' | '(' | ')' | '[' | ']'));
        match word {
            "" | "page" | "pg" | "p" | "of" => {}
            w if is_number_word(w) => numbers += 1,
            _ => return false,
        }
    }
    (1..=2).contains(&numbers)
}

/// Digits (up to four) or a well-formed roman numeral.
fn is_number_word(w: &str) -> bool {
    (!w.is_empty() && w.len() <= 4 && w.chars().all(|c| c.is_ascii_digit())) || is_roman(w)
}

/// A roman numeral as a page would print it: all one case, written the
/// standard way and under 400 (front matter is never longer), so words like
/// "mix" (1009) and "did" don't pass.
fn is_roman(w: &str) -> bool {
    if w.is_empty() || w.len() > 9 {
        return false;
    }
    let lower = w.to_ascii_lowercase();
    if w != lower && w != w.to_ascii_uppercase() {
        return false;
    }
    let value = |c: char| match c {
        'i' => Some(1),
        'v' => Some(5),
        'x' => Some(10),
        'l' => Some(50),
        'c' => Some(100),
        'd' => Some(500),
        'm' => Some(1000),
        _ => None,
    };
    let digits: Option<Vec<i64>> = lower.chars().map(value).collect();
    let Some(digits) = digits else { return false };
    let mut total = 0i64;
    for (i, &d) in digits.iter().enumerate() {
        if digits.get(i + 1).is_some_and(|&next| next > d) {
            total -= d;
        } else {
            total += d;
        }
    }
    total > 0 && total < 400 && u32::try_from(total).is_ok_and(|t| to_roman(t) == lower)
}

fn to_roman(mut n: u32) -> String {
    const TABLE: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (v, s) in TABLE {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

/// Consecutive lines of a block that are all heading-sized (at one size) or
/// all body.
struct Run<'a> {
    heading: Option<u16>,
    lines: Vec<&'a Line>,
}

/// A heading is a few short lines; anything longer is running text set
/// large (a pull quote, a children's book) and stays a paragraph.
const MAX_HEADING_LINES: usize = 3;
const MAX_HEADING_CHARS: usize = 200;

fn runs<'a>(lines: &[&'a Line], body: u16) -> Vec<Run<'a>> {
    let class = |l: &Line| {
        let big = body > 0 && f32::from(l.size) >= f32::from(body) * HEADING_RATIO;
        big.then_some(l.size)
    };
    let mut out: Vec<Run<'a>> = Vec::new();
    for &line in lines {
        let c = class(line);
        match out.last_mut() {
            Some(run) if run.heading == c => run.lines.push(line),
            _ => out.push(Run {
                heading: c,
                lines: vec![line],
            }),
        }
    }
    for run in &mut out {
        if run.heading.is_some() {
            let text: String = run.lines.iter().map(|l| l.text()).collect();
            let letters = text.chars().filter(|c| c.is_alphanumeric()).count();
            if run.lines.len() > MAX_HEADING_LINES
                || text.chars().count() > MAX_HEADING_CHARS
                || letters < 2
            {
                run.heading = None;
            }
        }
    }
    // Downgraded runs may now sit next to body runs; merge them back so a
    // paragraph with a drop cap stays one paragraph.
    let mut merged: Vec<Run<'a>> = Vec::new();
    for run in out {
        match merged.last_mut() {
            Some(last) if last.heading.is_none() && run.heading.is_none() => {
                last.lines.extend(run.lines)
            }
            _ => merged.push(run),
        }
    }
    merged
}

/// Heading sizes, largest first, as `#` levels: sizes within 5 % of each
/// other share a level, and everything past the third is `###`.
fn heading_levels(mut sizes: Vec<u16>) -> HashMap<u16, usize> {
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    let mut levels = HashMap::new();
    let mut level = 0usize;
    let mut anchor = 0u16;
    for size in sizes {
        if level == 0 || f32::from(size) < f32::from(anchor) * 0.95 {
            level += 1;
            anchor = size;
        }
        levels.insert(size, level.min(3));
    }
    levels
}

/// A short line, all bold at body size, alone in its block.
fn run_in_heading(line: &Line, body: u16) -> bool {
    let text = line.text();
    line.all_bold()
        && text.chars().count() <= 80
        && line.size.abs_diff(body) <= 1
        && text.chars().any(char::is_alphanumeric)
        && list_marker(&text).is_none()
}

/// What a paragraph-like piece of a block is.
enum Kind {
    Paragraph,
    Bullet,
    /// A numbered item; the marker as printed ("1.", "a)", "(iv)").
    Numbered(String),
}

fn render_block(
    lines: &[&Line],
    single_line_block: bool,
    body: u16,
    levels: &HashMap<u16, usize>,
    out: &mut Vec<Chunk>,
) {
    for run in runs(lines, body) {
        if let Some(size) = run.heading {
            let level = levels.get(&size).copied().unwrap_or(3);
            let spans = join_lines(&run.lines);
            let text: String = spans.iter().map(|(t, _)| t.as_str()).collect();
            out.push(Chunk {
                text: format!("{} {}", "#".repeat(level), escape_heading(&text)),
                list: false,
                para: false,
            });
            continue;
        }
        // A short line on its own, all bold at body size: a run-in heading.
        if single_line_block
            && let [line] = run.lines.as_slice()
            && run_in_heading(line, body)
        {
            out.push(Chunk {
                text: format!("#### {}", escape_heading(&line.text())),
                list: false,
                para: false,
            });
            continue;
        }
        for (kind, spans) in split_items(&run.lines) {
            let inline = render_inline(&spans);
            if inline.is_empty() {
                continue;
            }
            let chunk = match kind {
                Kind::Paragraph => Chunk {
                    text: escape_line_start(&inline),
                    list: false,
                    para: true,
                },
                Kind::Bullet => Chunk {
                    text: format!("- {inline}"),
                    list: true,
                    para: false,
                },
                Kind::Numbered(marker) => Chunk {
                    para: false,
                    list: marker.ends_with('.') && marker.starts_with(|c: char| c.is_ascii_digit()),
                    text: format!("{marker} {inline}"),
                },
            };
            out.push(chunk);
        }
    }
}

/// A block's body lines as paragraphs and list items: a line that starts
/// with a bullet or a number starts a new item, and the lines after it
/// (wrapped text) belong to it.
fn split_items(lines: &[&Line]) -> Vec<(Kind, Vec<Span>)> {
    let mut items: Vec<(Kind, Vec<&Line>, Vec<Line>)> = Vec::new();
    for &line in lines {
        let text = line.text();
        match list_marker(&text) {
            Some((kind, marker_chars)) => {
                let mut rest = line.clone();
                drop_prefix(&mut rest.spans, marker_chars);
                items.push((kind, Vec::new(), vec![rest]));
            }
            None => match items.last_mut() {
                Some((_, _, owned)) => owned.push(line.clone()),
                None => items.push((Kind::Paragraph, vec![line], Vec::new())),
            },
        }
    }
    items
        .into_iter()
        .map(|(kind, borrowed, owned)| {
            let mut all: Vec<&Line> = borrowed;
            all.extend(owned.iter());
            (kind, join_lines(&all))
        })
        .collect()
}

const BULLETS: [char; 8] = ['•', '◦', '▪', '‣', '●', '–', '-', '*'];

/// The list marker a line starts with, and how many characters it takes
/// (with the space after it).
fn list_marker(text: &str) -> Option<(Kind, usize)> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if BULLETS.contains(&first) && chars.next().is_some_and(char::is_whitespace) {
        return Some((Kind::Bullet, 2));
    }
    let word = text.split_whitespace().next()?;
    if word.len() == text.len() {
        return None;
    }
    let numbered = {
        let b = word.as_bytes();
        let n = b.len();
        let digits = (2..=4).contains(&n)
            && matches!(b[n - 1], b'.' | b')')
            && b[..n - 1].iter().all(u8::is_ascii_digit);
        let letter = n == 2 && b[0].is_ascii_alphabetic() && b[1] == b')';
        let paren = (3..=7).contains(&n)
            && b[0] == b'('
            && b[n - 1] == b')'
            && b[1..n - 1].iter().all(u8::is_ascii_alphanumeric);
        digits || letter || paren
    };
    numbered.then(|| (Kind::Numbered(word.to_string()), word.chars().count() + 1))
}

/// Remove the first `n` characters from the runs, then leading spaces.
fn drop_prefix(spans: &mut Vec<Span>, mut n: usize) {
    for (text, _) in spans.iter_mut() {
        if n == 0 {
            break;
        }
        let len = text.chars().count();
        if len <= n {
            n -= len;
            text.clear();
        } else {
            *text = text.chars().skip(n).collect();
            n = 0;
        }
    }
    spans.retain(|(t, _)| !t.is_empty());
    if let Some((t, _)) = spans.first_mut() {
        *t = t.trim_start().to_string();
    }
    spans.retain(|(t, _)| !t.is_empty());
}

/// Lines joined into one run list: a space between lines, or nothing when a
/// word was hyphenated across them ("exam-" + "ple").
fn join_lines(lines: &[&Line]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for line in lines {
        let Some((first_text, first_style)) = line.spans.first() else {
            continue;
        };
        if let Some((last_text, last_style)) = out.last_mut() {
            let mut tail = last_text.chars().rev();
            let end = tail.next();
            let before = tail.next();
            let next_lower = first_text.chars().next().is_some_and(char::is_lowercase);
            if end == Some('\u{ad}')
                || (end == Some('-') && before.is_some_and(char::is_alphabetic) && next_lower)
            {
                last_text.pop();
            } else if *last_style == *first_style {
                last_text.push(' ');
            } else {
                out.push((" ".into(), Style::default()));
            }
        }
        for (text, style) in &line.spans {
            match out.last_mut() {
                Some((t, s)) if s == style => t.push_str(text),
                _ => out.push((text.clone(), *style)),
            }
        }
    }
    out
}

/// Styled runs as Markdown: `**bold**`, `*italic*`, `***both***`. Spaces stay
/// outside the markers (CommonMark ignores `** bold**`), and runs that are
/// only spaces or punctuation aren't marked.
fn render_inline(spans: &[Span]) -> String {
    let mut merged: Vec<Span> = Vec::new();
    for (text, style) in spans {
        match merged.last_mut() {
            Some((t, s)) if s == style => t.push_str(text),
            _ => merged.push((text.clone(), *style)),
        }
    }
    let mut out = String::new();
    for (text, style) in &merged {
        let marker = match (style.bold, style.italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            (false, false) => "",
        };
        if marker.is_empty() || !text.chars().any(char::is_alphanumeric) {
            out.push_str(&escape_inline(text));
            continue;
        }
        let core = text.trim();
        let start = text.len() - text.trim_start().len();
        let lead = &text[..start];
        let trail = &text[start + core.len()..];
        out.push_str(lead);
        out.push_str(marker);
        out.push_str(&escape_inline(core));
        out.push_str(marker);
        out.push_str(trail);
    }
    out.trim().to_string()
}

/// Backslash the characters Markdown would read as markup inside a line.
fn escape_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.char_indices() {
        // `&` only matters where it would start an HTML entity: "&amp;" in
        // the source must stay "&amp;" on screen, not turn into "&".
        if matches!(c, '*' | '_' | '`' | '[' | ']' | '\\' | '<')
            || (c == '&' && starts_entity(&s[i + 1..]))
        {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Whether text after an `&` reads as an entity: `amp;`, `#38;`, `#x26;`.
fn starts_entity(rest: &str) -> bool {
    let Some((name, _)) = rest.split_once(';') else {
        return false;
    };
    if let Some(num) = name.strip_prefix('#') {
        return match num.strip_prefix(['x', 'X']) {
            Some(hex) => !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()),
            None => !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()),
        };
    }
    name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Backslash what would make a paragraph's first characters a heading, a
/// quote, a list item, a rule or a table row. `s` has been through
/// `escape_inline`.
fn escape_line_start(s: &str) -> String {
    if s.starts_with(['#', '>', '+', '-', '=', '|']) {
        return format!("\\{s}");
    }
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits) && s[digits..].starts_with(['.', ')']) {
        return format!("{}\\{}", &s[..digits], &s[digits..]);
    }
    s.to_string()
}

/// A heading's text: inline-escaped, with a closing `#` protected (it would
/// be read as the heading's closing sequence).
fn escape_heading(text: &str) -> String {
    let mut s = escape_inline(text.trim());
    if s.ends_with('#') {
        s.insert(s.len() - 1, '\\');
    }
    s
}

// ---------------------------------------------------------------------------
// Tables
//
// No format tells us where its tables are once MuPDF has laid the page out,
// so they are found from the geometry: rows of short cells parted by wide
// gaps, whose edges line up into the same columns from row to row. The
// tests are strict on purpose; a table missed reads as one paragraph per
// row, while prose taken for a table is mangled.

/// Inside one line, a gap this many times the text size parts two cells
/// (a word space is about a quarter of the size).
const CELL_GAP_IN_LINE: f32 = 1.5;

/// Between two lines MuPDF already split at one height, a smaller gap is
/// enough: it judged them separate, and only a superscript or a drop cap
/// sits closer than this.
const CELL_GAP_BETWEEN_LINES: f32 = 0.5;

/// How far apart, in points, edges can be and still line up into a column.
const COLUMN_TOLERANCE: f32 = 3.0;

/// Table cells are short; longer runs of text are prose.
const MAX_CELL_CHARS: usize = 80;

/// A line of a page in reading order. `group` changes at every picture, so
/// a table never spans one; `item` is the block the line came from.
struct Placed<'a> {
    group: usize,
    item: usize,
    line: &'a Line,
}

/// A word and the placed line it belongs to.
type PlacedWord = (usize, Word);

/// A cell: its words, in order.
type Cell = Vec<PlacedWord>;

/// Lines side by side at one height, in reading order, cut into cells at
/// wide gaps.
struct Row {
    /// The first and last placed lines in it.
    first: usize,
    last: usize,
    group: usize,
    y0: f32,
    y1: f32,
    cells: Vec<Cell>,
}

/// A column's left and right edges, over the rows that fill every column.
struct Column {
    lo: f32,
    hi: f32,
}

/// A found table: the placed lines it covers and its Markdown.
struct Table {
    first: usize,
    last: usize,
    text: String,
}

/// The table that placed line `at` belongs to.
fn table_at(tables: &[Table], at: usize) -> Option<&Table> {
    tables.iter().find(|t| (t.first..=t.last).contains(&at))
}

/// A page's surviving lines in reading order.
fn placed_lines<'a>(
    p: &PageData,
    items: &[&'a Item],
    running: &HashSet<HeaderKey>,
) -> Vec<Placed<'a>> {
    let mut out = Vec::new();
    let mut group = 0;
    for (n, &item) in items.iter().enumerate() {
        match item {
            Item::Text(lines) => {
                for line in surviving(p, lines, running) {
                    out.push(Placed {
                        group,
                        item: n,
                        line,
                    });
                }
            }
            Item::Picture(_) => group += 1,
        }
    }
    out
}

/// The tables among a page's lines. Runs of consecutive rows that each have
/// two or more short cells are tried as tables; the row just above a run
/// may join it as its header.
fn find_tables(placed: &[Placed]) -> Vec<Table> {
    let rows = visual_rows(placed);
    let mut out = Vec::new();
    let mut i = 0;
    // Rows before this already belong to a table.
    let mut free_from = 0;
    while i < rows.len() {
        if !is_seed(&rows[i], placed) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < rows.len() && is_seed(&rows[j], placed) && adjacent(&rows[j - 1], &rows[j]) {
            j += 1;
        }
        match build_table(&rows, i, j, free_from, placed) {
            Some(table) => {
                out.push(table);
                free_from = j;
                i = j;
            }
            // A run that fails may hold a table further down (a caption
            // line above it with a gap in it, say); try again one row on.
            None => i += 1,
        }
    }
    out
}

/// Lines grouped into rows: a line joins the row before it when it sits at
/// the same height, to the right of it.
fn visual_rows(placed: &[Placed]) -> Vec<Row> {
    let mut bounds: Vec<(usize, usize)> = Vec::new();
    for (i, pl) in placed.iter().enumerate() {
        match bounds.last_mut() {
            Some((_, last))
                if placed[*last].group == pl.group && side_by_side(placed[*last].line, pl.line) =>
            {
                *last = i;
            }
            _ => bounds.push((i, i)),
        }
    }
    bounds
        .into_iter()
        .map(|(first, last)| {
            let lines = &placed[first..=last];
            let size = lines.iter().map(|p| p.line.size).max().unwrap_or(0);
            let size = f32::from(size) / 2.0;
            let mut cells: Vec<Cell> = Vec::new();
            let mut prev: Option<PlacedWord> = None;
            for (at, pl) in (first..=last).zip(lines) {
                for &w in &pl.line.words {
                    let new_cell = prev.is_none_or(|(prev_at, pw)| {
                        let k = if prev_at == at {
                            CELL_GAP_IN_LINE
                        } else {
                            CELL_GAP_BETWEEN_LINES
                        };
                        w.x0 - pw.x1 >= size * k
                    });
                    match cells.last_mut() {
                        Some(cell) if !new_cell => cell.push((at, w)),
                        _ => cells.push(vec![(at, w)]),
                    }
                    prev = Some((at, w));
                }
            }
            Row {
                first,
                last,
                group: placed[first].group,
                y0: lines.iter().map(|p| p.line.y0).fold(f32::MAX, f32::min),
                y1: lines.iter().map(|p| p.line.y1).fold(f32::MIN, f32::max),
                cells,
            }
        })
        .collect()
}

/// Line `b` is at the same height as line `a` (they overlap by half the
/// smaller one) and starts to its right.
fn side_by_side(a: &Line, b: &Line) -> bool {
    let overlap = a.y1.min(b.y1) - a.y0.max(b.y0);
    let h = (a.y1 - a.y0).min(b.y1 - b.y0);
    h > 0.0 && overlap >= h * 0.5 && b.x0 >= a.x1 - 1.0
}

/// Row `b` comes just below row `a`, with no more than a row and a half of
/// space between them.
fn adjacent(a: &Row, b: &Row) -> bool {
    let h = (a.y1 - a.y0).max(b.y1 - b.y0);
    a.group == b.group && b.y0 > a.y0 && b.y0 - a.y1 <= h * 1.5
}

/// A row that could be part of a table: two cells or more, all short.
fn is_seed(row: &Row, placed: &[Placed]) -> bool {
    row.cells.len() >= 2 && row.cells.iter().all(|c| is_short(c, placed))
}

fn is_short(cell: &[PlacedWord], placed: &[Placed]) -> bool {
    cell_text(cell, placed).chars().count() <= MAX_CELL_CHARS
}

/// The table made of `rows[start..end]` (and maybe the row above, as its
/// header), when its cells line up into columns.
fn build_table(
    rows: &[Row],
    start: usize,
    end: usize,
    free_from: usize,
    placed: &[Placed],
) -> Option<Table> {
    let seeds = rows.get(start..end)?;
    if seeds.len() < 2 {
        return None;
    }
    // Columns come from the rows that fill all of them; at least two must,
    // so the line-up is seen and not assumed.
    let n = seeds.iter().map(|r| r.cells.len()).max()?;
    let full: Vec<&Row> = seeds.iter().filter(|r| r.cells.len() == n).collect();
    if full.len() < 2 {
        return None;
    }
    let cols = columns(&full)?;
    let mut grid: Vec<Vec<Cell>> = seeds
        .iter()
        .map(|r| place(r, &cols))
        .collect::<Option<_>>()?;
    let (top, bottom) = (seeds.first()?, seeds.last()?);
    let mut first = top.first;
    if start > free_from
        && let Some(head) = rows.get(start - 1)
        && is_header(head, top, placed)
        && let Some(cells) = place(head, &cols)
        && cells.iter().filter(|c| !c.is_empty()).count() >= 2
        && cells.iter().all(|c| is_short(c, placed))
    {
        grid.insert(0, cells);
        first = head.first;
    }
    let texts: Vec<Vec<String>> = grid
        .iter()
        .map(|r| r.iter().map(|c| cell_text(c, placed)).collect())
        .collect();
    if looks_like_prose(&texts) || looks_like_list(&texts) {
        return None;
    }
    Some(Table {
        first,
        last: bottom.last,
        text: table_markdown(&grid, placed),
    })
}

/// Each column's edges, when its cells line up on the left, the right
/// (numbers) or the centre, and no two columns overlap.
fn columns(full: &[&Row]) -> Option<Vec<Column>> {
    let n = full.first()?.cells.len();
    let mut cols = Vec::with_capacity(n);
    for k in 0..n {
        let xs: Vec<(f32, f32)> = full
            .iter()
            .filter_map(|r| r.cells.get(k))
            .map(|c| cell_x(c))
            .collect();
        let lined_up = spread(xs.iter().map(|x| x.0)) <= COLUMN_TOLERANCE
            || spread(xs.iter().map(|x| x.1)) <= COLUMN_TOLERANCE
            || spread(xs.iter().map(|x| (x.0 + x.1) / 2.0)) <= COLUMN_TOLERANCE;
        if !lined_up {
            return None;
        }
        cols.push(Column {
            lo: xs.iter().map(|x| x.0).fold(f32::MAX, f32::min),
            hi: xs.iter().map(|x| x.1).fold(f32::MIN, f32::max),
        });
    }
    if cols.windows(2).any(|w| w[0].hi >= w[1].lo) {
        return None;
    }
    Some(cols)
}

/// The largest value less the smallest.
fn spread(values: impl Iterator<Item = f32>) -> f32 {
    let (lo, hi) = values.fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    hi - lo
}

/// A cell's left and right edges.
fn cell_x(cell: &[PlacedWord]) -> (f32, f32) {
    let x0 = cell.iter().map(|(_, w)| w.x0).fold(f32::MAX, f32::min);
    let x1 = cell.iter().map(|(_, w)| w.x1).fold(f32::MIN, f32::max);
    (x0, x1)
}

/// A row's words sorted into the table's columns. Cells are first cut again
/// where a word starts at a column's left edge (a header MuPDF read as one
/// run of text, "Format Pages"), then each piece goes to the column it
/// overlaps most. `None` when two pieces land in one column, or out of order.
fn place(row: &Row, cols: &[Column]) -> Option<Vec<Cell>> {
    let mut pieces: Vec<Cell> = Vec::new();
    for cell in &row.cells {
        let mut piece: Cell = Vec::new();
        for &(at, w) in cell {
            let at_edge = cols
                .iter()
                .skip(1)
                .any(|c| (w.x0 - c.lo).abs() <= COLUMN_TOLERANCE);
            if at_edge && !piece.is_empty() {
                pieces.push(std::mem::take(&mut piece));
            }
            piece.push((at, w));
        }
        if !piece.is_empty() {
            pieces.push(piece);
        }
    }
    let mut out: Vec<Cell> = vec![Vec::new(); cols.len()];
    let mut next = 0usize;
    for piece in pieces {
        let (x0, x1) = cell_x(&piece);
        // Overlap, or minus the distance when they don't touch.
        let (k, _) = cols
            .iter()
            .enumerate()
            .map(|(k, c)| (k, x1.min(c.hi) - x0.max(c.lo)))
            .max_by(|a, b| a.1.total_cmp(&b.1))?;
        if k < next {
            return None;
        }
        *out.get_mut(k)? = piece;
        next = k + 1;
    }
    Some(out)
}

/// The row just above a table can be its header when it sits close above
/// and is set bold or starts its own block (a paragraph's last line never
/// does).
fn is_header(head: &Row, top: &Row, placed: &[Placed]) -> bool {
    if !adjacent(head, top) {
        return false;
    }
    let lines = placed.get(head.first..=head.last).unwrap_or_default();
    let bold = lines.iter().all(|p| p.line.all_bold());
    let own_block = head
        .first
        .checked_sub(1)
        .and_then(|i| placed.get(i))
        .is_none_or(|before| Some(before.item) != lines.first().map(|p| p.item));
    bold || own_block
}

/// A column whose cells run on from row to row (no closing punctuation,
/// then a lowercase start) is wrapped prose: two columns of text that
/// MuPDF read across the page, not a table.
fn looks_like_prose(rows: &[Vec<String>]) -> bool {
    let n = rows.first().map_or(0, Vec::len);
    (0..n).any(|k| {
        let col: Vec<&str> = rows
            .iter()
            .filter_map(|r| r.get(k))
            .map(String::as_str)
            .filter(|t| !t.is_empty())
            .collect();
        let pairs = col.len().saturating_sub(1);
        let run_on = col
            .windows(2)
            .filter(|w| {
                !w[0].ends_with(['.', '!', '?', ':', ';']) && w[1].starts_with(char::is_lowercase)
            })
            .count();
        let chars: usize = col.iter().map(|t| t.chars().count()).sum();
        pairs > 0 && run_on * 2 >= pairs && chars >= 20 * col.len()
    })
}

/// A first column of bullets or item numbers is a list with a hanging
/// indent, not a table.
fn looks_like_list(rows: &[Vec<String>]) -> bool {
    rows.iter().filter_map(|r| r.first()).all(|t| {
        let mut chars = t.chars();
        let bullet =
            matches!((chars.next(), chars.next()), (Some(c), None) if BULLETS.contains(&c));
        bullet || matches!(list_marker(&format!("{t} x")), Some((Kind::Numbered(_), _)))
    })
}

/// A cell's styled runs: each line's words as one slice of the line, with a
/// space between lines.
fn cell_spans(cell: &[PlacedWord], placed: &[Placed]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut i = 0;
    while let Some(&(at, w)) = cell.get(i) {
        let mut end = w.end;
        i += 1;
        while let Some(&(next_at, next)) = cell.get(i)
            && next_at == at
        {
            end = next.end;
            i += 1;
        }
        let Some(pl) = placed.get(at) else { continue };
        if !out.is_empty() {
            out.push((" ".into(), Style::default()));
        }
        out.extend(slice_spans(&pl.line.spans, w.start, end));
    }
    out
}

fn cell_text(cell: &[PlacedWord], placed: &[Placed]) -> String {
    cell_spans(cell, placed)
        .iter()
        .map(|(t, _)| t.as_str())
        .collect()
}

/// Characters `start..end` of a line's runs, keeping their styles.
fn slice_spans(spans: &[Span], start: usize, end: usize) -> Vec<Span> {
    let mut out = Vec::new();
    let mut pos = 0;
    for (text, style) in spans {
        let len = text.chars().count();
        let (a, b) = (start.max(pos), end.min(pos + len));
        if a < b {
            out.push((text.chars().skip(a - pos).take(b - a).collect(), *style));
        }
        pos += len;
    }
    out
}

/// A GitHub table. The first row is always the header: GFM requires one,
/// and a table's first row is its header far more often than not.
fn table_markdown(grid: &[Vec<Cell>], placed: &[Placed]) -> String {
    let n = grid.first().map_or(0, Vec::len);
    let mut out = String::new();
    for (r, row) in grid.iter().enumerate() {
        if r == 1 {
            out.push('|');
            out.push_str(&" --- |".repeat(n));
            out.push('\n');
        }
        out.push('|');
        for cell in row {
            let text = render_inline(&cell_spans(cell, placed))
                .replace('\n', " ")
                .replace('|', "\\|");
            out.push(' ');
            if !text.is_empty() {
                out.push_str(&text);
                out.push(' ');
            }
            out.push('|');
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(rel: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(rel)
    }

    fn plain(t: &str) -> Span {
        (t.into(), Style::default())
    }
    fn bold(t: &str) -> Span {
        (
            t.into(),
            Style {
                bold: true,
                italic: false,
            },
        )
    }
    fn italic(t: &str) -> Span {
        (
            t.into(),
            Style {
                bold: false,
                italic: true,
            },
        )
    }

    /// A one-page PDF drawing `content`, with Helvetica as /F1 and
    /// Helvetica-Bold as /F2, saved in the temp folder.
    fn text_pdf(name: &str, content: &str) -> PathBuf {
        use mupdf::pdf::PdfDocument;
        let mut doc = PdfDocument::new();
        let mut buf = mupdf::Buffer::with_capacity(content.len());
        std::io::Write::write_all(&mut buf, content.as_bytes()).unwrap();
        let dict = doc.new_object_from_str("<< >>").unwrap();
        let contents = doc
            .add_stream(&buf, Some(&dict), false)
            .unwrap()
            .as_indirect()
            .unwrap();
        let fonts = "/F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> \
                     /F2 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>";
        let page = doc
            .new_object_from_str(&format!(
                "<< /Type /Page /MediaBox [0 0 612 792] /Resources << /Font << {fonts} >> >> /Contents {contents} 0 R >>"
            ))
            .unwrap();
        let page = doc.add_object(&page).unwrap();
        doc.insert_page(0, &page).unwrap();
        let path =
            std::env::temp_dir().join(format!("kitab-export-{name}-{}.pdf", std::process::id()));
        crate::tools::save_pdf(&doc, &path).unwrap();
        path
    }

    /// `s` in 10 pt type, starting at `x`, `y`.
    fn text_at(font: &str, x: f32, y: f32, s: &str) -> String {
        format!("BT /{font} 10 Tf {x:.2} {y:.2} Td ({s}) Tj ET\n")
    }

    fn markdown_of(path: &Path) -> String {
        let md = to_markdown(path, None, &MarkdownOptions::default()).unwrap();
        let _ = fs::remove_file(path);
        md.text
    }

    #[test]
    fn grid_becomes_a_table() {
        // Helvetica digits are 0.556 em and the point 0.278 em, so prices
        // can be set flush right at x = 300 like a real price column.
        let right = |s: &str, bold: bool| {
            let em: f32 = s
                .chars()
                .map(|c| match c {
                    '.' => 0.278,
                    _ if bold => 0.0,
                    _ => 0.556,
                })
                .sum();
            let w = if bold { 24.46 } else { em * 10.0 };
            300.0 - w
        };
        let mut c = text_at(
            "F1",
            72.0,
            740.0,
            "Fruit and what each one costs this week.",
        );
        c += &text_at("F2", 72.0, 700.0, "Name");
        c += &text_at("F2", 180.0, 700.0, "Qty");
        c += &text_at("F2", right("Price", true), 700.0, "Price");
        let rows = [
            ("Apple", "3", "1.50"),
            ("Kiwi|Gold", "12", "12.00"),
            ("Pear", "", "3.25"),
        ];
        for (i, (name, qty, price)) in rows.iter().enumerate() {
            let y = 680.0 - 20.0 * i as f32;
            c += &text_at("F1", 72.0, y, name);
            if !qty.is_empty() {
                c += &text_at("F1", 180.0, y, qty);
            }
            c += &text_at("F1", right(price, false), y, price);
        }
        c += &text_at("F1", 72.0, 600.0, "Prices are per kilo and change often.");
        let md = markdown_of(&text_pdf("grid", &c));
        let table = "| **Name** | **Qty** | **Price** |\n\
                     | --- | --- | --- |\n\
                     | Apple | 3 | 1.50 |\n\
                     | Kiwi\\|Gold | 12 | 12.00 |\n\
                     | Pear | | 3.25 |";
        assert!(md.contains(table), "{md}");
        assert!(
            md.starts_with("Fruit and what each one costs this week.\n\n|"),
            "{md}"
        );
        assert!(
            md.ends_with("|\n\nPrices are per kilo and change often.\n"),
            "{md}"
        );
    }

    #[test]
    fn two_column_prose_is_not_a_table() {
        // The hard case: each line of the left column written just before
        // the line beside it, so MuPDF reads straight across the page.
        let left = [
            "the reader opens the book and finds that",
            "every page is set in two narrow columns",
            "which run down the sheet side by side and",
            "carry on from the foot of the left one to",
            "the head of the right one without a break.",
        ];
        let right = [
            "so a careless program that reads straight",
            "across the page would mix the two halves",
            "into nonsense and call the result a table",
            "of cells when it is really just running",
            "text that happens to share its baselines.",
        ];
        let mut c = String::new();
        for (i, (l, r)) in left.iter().zip(right).enumerate() {
            let y = 700.0 - 14.0 * i as f32;
            c += &text_at("F1", 72.0, y, l);
            c += &text_at("F1", 330.0, y, r);
        }
        let md = markdown_of(&text_pdf("prose", &c));
        assert!(!md.contains("| ---"), "{md}");
        assert!(!md.lines().any(|l| l.starts_with('|')), "{md}");
        assert!(
            md.contains("finds that") && md.contains("baselines."),
            "{md}"
        );
    }

    #[test]
    fn label_value_and_hanging_lists_are_not_tables() {
        let mut c = text_at("F1", 72.0, 740.0, "Thank you for your order.");
        c += &text_at("F1", 72.0, 720.0, "Total due:");
        c += &text_at("F1", 400.0, 720.0, "42.00");
        c += &text_at("F1", 72.0, 700.0, "Please pay within thirty days.");
        // Bullets with a wide hanging indent line up like a two-column grid.
        for (i, item) in ["Bread", "Milk", "Eggs"].iter().enumerate() {
            let y = 660.0 - 14.0 * i as f32;
            c += &text_at("F1", 72.0, y, "\\267");
            c += &text_at("F1", 100.0, y, item);
        }
        let md = markdown_of(&text_pdf("label", &c));
        assert!(!md.lines().any(|l| l.starts_with('|')), "{md}");
        assert!(md.contains("Total due:") && md.contains("42.00"), "{md}");
        assert!(md.contains("Eggs"), "{md}");
    }

    #[test]
    fn epub_table_becomes_a_table() {
        let md = to_markdown(
            &fixture("epub/03-footnotes-images.epub"),
            None,
            &MarkdownOptions::default(),
        )
        .unwrap();
        let table = "| **Format** | **Pages** |\n| --- | --- |\n| PDF | 40 |\n| EPUB | 5 |";
        assert!(md.text.contains(table), "{}", md.text);
        assert!(!md.text.contains("#### Format"), "{}", md.text);
    }

    #[test]
    fn pipes_and_ampersands_escaped_only_where_they_matter() {
        assert_eq!(escape_line_start("| a | b"), "\\| a | b");
        assert_eq!(escape_line_start("a | b"), "a | b");
        assert_eq!(
            escape_inline("&amp; & &#38; &#x26; &x AT&T"),
            "\\&amp; & \\&#38; \\&#x26; &x AT&T"
        );
        assert_eq!(
            render_inline(&[plain("Fish &amp; chips")]),
            "Fish \\&amp; chips"
        );
    }

    #[test]
    fn inline_styles_keep_spaces_outside_markers() {
        let spans = vec![
            plain("a "),
            bold("bold "),
            plain("and "),
            italic("it"),
            plain("."),
        ];
        assert_eq!(render_inline(&spans), "a **bold** and *it*.");
        let both = vec![(
            "both".to_string(),
            Style {
                bold: true,
                italic: true,
            },
        )];
        assert_eq!(render_inline(&both), "***both***");
        // Punctuation and spaces alone are never marked.
        assert_eq!(render_inline(&[plain("x"), bold(", "), plain("y")]), "x, y");
    }

    #[test]
    fn escaping() {
        assert_eq!(
            escape_inline("2*3 = a_b [x] `c` <d> \\"),
            "2\\*3 = a\\_b \\[x\\] \\`c\\` \\<d> \\\\"
        );
        assert_eq!(escape_line_start("# not a heading"), "\\# not a heading");
        assert_eq!(escape_line_start("> nope"), "\\> nope");
        assert_eq!(escape_line_start("- dash"), "\\- dash");
        assert_eq!(escape_line_start("2024. was a year"), "2024\\. was a year");
        assert_eq!(escape_line_start("plain"), "plain");
        assert_eq!(
            render_inline(&[plain("*star* and #hash")]),
            "\\*star\\* and #hash"
        );
        assert_eq!(escape_heading("C#"), "C\\#");
    }

    #[test]
    fn list_markers() {
        assert!(matches!(list_marker("• item"), Some((Kind::Bullet, 2))));
        assert!(matches!(list_marker("- item"), Some((Kind::Bullet, 2))));
        assert!(list_marker("-item").is_none());
        assert!(matches!(list_marker("12. twelve"), Some((Kind::Numbered(m), 4)) if m == "12."));
        assert!(matches!(list_marker("a) first"), Some((Kind::Numbered(m), _)) if m == "a)"));
        assert!(matches!(list_marker("(iv) fourth"), Some((Kind::Numbered(m), _)) if m == "(iv)"));
        assert!(list_marker("2024. was").is_none());
        assert!(list_marker("Hello world").is_none());
    }

    #[test]
    fn hyphenation_is_joined() {
        let line = |t: &str| Line {
            spans: vec![plain(t)],
            size: 22,
            x0: 0.0,
            x1: 0.0,
            y0: 0.0,
            y1: 0.0,
            words: Vec::new(),
        };
        let (a, b, c) = (line("an exam-"), line("ple of well-"), line("Known"));
        let spans = join_lines(&[&a, &b, &c]);
        assert_eq!(render_inline(&spans), "an example of well- Known");
    }

    #[test]
    fn page_numbers_and_romans() {
        for yes in [
            "12", "xii", "XIV", "Page 3", "3 of 10", "- 4 -", "5 / 9", "page iv",
        ] {
            assert!(is_page_number(yes), "{yes}");
        }
        for no in ["mix", "Did", "Chapter 3", "In 1999 we", "", "Page"] {
            assert!(!is_page_number(no), "{no}");
        }
    }

    #[test]
    fn running_headers_and_page_numbers_are_dropped() {
        let line = |t: &str, size: u16, y: f32| Line {
            spans: vec![plain(t)],
            size,
            x0: 0.0,
            x1: 0.0,
            y0: y,
            y1: y + 12.0,
            words: Vec::new(),
        };
        let pages: Vec<PageData> = (0..5u32)
            .map(|i| PageData {
                index: i,
                top: 0.0,
                bottom: 800.0,
                items: vec![
                    Item::Text(vec![line("The Book of Things", 20, 20.0)]),
                    Item::Text(vec![line(&format!("Body text on page {i}."), 22, 300.0)]),
                    Item::Text(vec![line(&format!("{}", i + 1), 20, 770.0)]),
                ],
            })
            .collect();
        let mut sizes = HashMap::new();
        sizes.insert(22u16, 1000usize);
        let md = render(&pages, &sizes, false, "");
        assert!(!md.contains("Book of Things"), "{md}");
        assert!(md.contains("Body text on page 3."));
        assert!(!md.lines().any(|l| l.trim() == "3"), "{md}");
        // Paragraphs split by a page break are joined back.
        let pages: Vec<PageData> = (0..2u32)
            .map(|i| PageData {
                index: i,
                top: 0.0,
                bottom: 800.0,
                items: vec![Item::Text(vec![line(
                    if i == 0 {
                        "a sentence that goes"
                    } else {
                        "on and on."
                    },
                    22,
                    300.0,
                )])],
            })
            .collect();
        assert_eq!(
            render(&pages, &sizes, false, ""),
            "a sentence that goes on and on."
        );
    }

    #[test]
    fn heading_levels_cluster() {
        let levels = heading_levels(vec![28, 48, 36, 47, 30, 60]);
        assert_eq!(levels[&60], 1);
        assert_eq!(levels[&48], 2);
        assert_eq!(levels[&47], 2);
        assert_eq!(levels[&36], 3);
        assert_eq!(levels[&28], 3);
    }

    #[test]
    fn text_outline_headings_and_paragraphs() {
        let md = to_markdown(
            &fixture("pdf/01-text-outline.pdf"),
            None,
            &MarkdownOptions::default(),
        )
        .expect("exports");
        assert_eq!(md.pages, 40);
        assert!(md.text.ends_with('\n') && !md.text.ends_with("\n\n"));
        assert!(
            md.text
                .lines()
                .any(|l| l.starts_with("# Chapter 1") || l.starts_with("## Chapter 1")),
            "{}",
            &md.text[..md.text.len().min(2000)]
        );
        for n in [1, 17, 40] {
            let needle = format!("kitab-needle-{n:02}");
            let line = md
                .text
                .lines()
                .find(|l| l.contains(&needle))
                .expect("needle present");
            assert!(!line.starts_with('#'), "body became a heading: {line}");
        }
        // Body text is a paragraph, never a heading.
        let headings = md.text.lines().filter(|l| l.starts_with('#')).count();
        let paragraphs = md
            .text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .count();
        assert!(paragraphs > headings);
    }

    #[test]
    fn page_selection_markers_and_range() {
        let src = fixture("pdf/01-text-outline.pdf");
        let opts = MarkdownOptions {
            pages: Some(vec![16, 2]),
            images: None,
            page_markers: true,
        };
        let md = to_markdown(&src, None, &opts).expect("exports");
        assert_eq!(md.pages, 2);
        let p17 = md.text.find("<!-- page 17 -->").expect("marker 17");
        let p3 = md.text.find("<!-- page 3 -->").expect("marker 3");
        assert!(p17 < p3);
        assert!(md.text.contains("kitab-needle-17"));
        assert!(!md.text.contains("kitab-needle-01"));

        let bad = MarkdownOptions {
            pages: Some(vec![40]),
            ..Default::default()
        };
        assert_eq!(
            to_markdown(&src, None, &bad).unwrap_err(),
            "page 41 doesn't exist; the document has 40 pages"
        );
        assert!(to_text(&src, None, Some(&[99])).is_err());
    }

    #[test]
    fn pictures_saved_and_linked_or_left_out() {
        let src = fixture("pdf/10-scanned-and-images.pdf");
        let dir = std::env::temp_dir().join(format!("kitab-export-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let opts = MarkdownOptions {
            pages: None,
            images: Some(ImageOut {
                dir: dir.clone(),
                link_prefix: "pics/".into(),
            }),
            page_markers: false,
        };
        let md = to_markdown(&src, None, &opts).expect("exports");
        assert!(md.images >= 3, "{md:?}");
        let links: Vec<&str> = md
            .text
            .lines()
            .filter(|l| l.starts_with("![](pics/p"))
            .collect();
        assert_eq!(links.len(), md.images as usize);
        for link in links {
            let name = link.trim_start_matches("![](pics/").trim_end_matches(')');
            let bytes = fs::read(dir.join(name)).expect("picture saved");
            assert!(bytes.starts_with(b"\x89PNG"), "{name} is a PNG");
        }
        let without = to_markdown(&src, None, &MarkdownOptions::default()).expect("exports");
        assert_eq!(without.images, 0);
        assert!(!without.text.contains("!["));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn epub_converts_with_needles() {
        let md = to_markdown(
            &fixture("epub/01-epub3-basic.epub"),
            None,
            &MarkdownOptions::default(),
        )
        .expect("exports");
        for n in 1..=5 {
            assert!(
                md.text.contains(&format!("kitab-epub-needle-{n}")),
                "needle {n}"
            );
        }
        assert!(md.text.lines().any(|l| l.starts_with('#')), "has headings");
    }

    #[test]
    fn plain_text_has_needles() {
        let src = fixture("pdf/01-text-outline.pdf");
        let txt = to_text(&src, None, None).expect("exports");
        assert_eq!(txt.pages, 40);
        assert!(txt.text.contains("kitab-needle-01") && txt.text.contains("kitab-needle-40"));
        assert!(!txt.text.contains('\u{c}'));
        assert!(txt.text.lines().all(|l| l == l.trim_end()));
        assert!(txt.text.ends_with('\n') && !txt.text.ends_with("\n\n"));
        let one = to_text(&src, None, Some(&[16])).expect("exports");
        assert!(one.text.contains("kitab-needle-17") && !one.text.contains("kitab-needle-16"));
    }

    #[test]
    fn password_and_other_formats() {
        let locked = fixture("pdf/04a-encrypted-password-kitab.pdf");
        assert_eq!(
            to_text(&locked, None, None).unwrap_err(),
            "this PDF needs its password"
        );
        assert!(to_text(&locked, Some("wrong"), None).is_err());
        assert!(to_text(&locked, Some("kitab"), None).is_ok());
        // The XPS fixture is drawings only: no text, and no error either.
        to_markdown(
            &fixture("other/04-vector.xps"),
            None,
            &MarkdownOptions::default(),
        )
        .expect("xps");
        for f in ["other/02-book.fb2", "pdf/07-unicode-rtl.pdf"] {
            let md = to_markdown(&fixture(f), None, &MarkdownOptions::default()).expect(f);
            assert!(md.text.trim().len() > 10, "{f}");
        }
        assert!(
            to_markdown(
                &fixture("pdf/05c-not-a-pdf.pdf"),
                None,
                &MarkdownOptions::default()
            )
            .is_err()
        );
    }
}
