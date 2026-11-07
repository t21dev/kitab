// SPDX-License-Identifier: AGPL-3.0-or-later

//! A page preview of Markdown (or plain text) for thumbnails: Kitab's own
//! small layout on a 612 × 816 pt sheet, drawn as glyph outlines with MuPDF's
//! built-in fonts. It stops at the foot of the sheet.

use super::Thumb;
use kitab_md::{Block, Inline, Inlines};
use kitab_theme::{Rgba, ThumbPalette, thumb_palette};
use mupdf::{ColorParams, Colorspace, Device, Font, Matrix, Path, Pixmap};

const PAGE_W: f32 = 612.0;
const PAGE_H: f32 = 816.0;
const MARGIN: f32 = 54.0;
const BOTTOM: f32 = PAGE_H - 40.0;
const BODY: f32 = 14.0;
const CODE: f32 = 11.0;

/// The page's width for a thumbnail `size` px tall.
pub(crate) fn page_width(size: u32) -> u32 {
    ((size as f32) * PAGE_W / PAGE_H).round().max(1.0) as u32
}

pub(crate) fn render(text: &str, plain: bool, size: u32) -> Result<Thumb, String> {
    let err = |e: mupdf::Error| e.to_string();
    let cs = Colorspace::device_rgb();
    let mut pixmap =
        Pixmap::new_with_w_h(&cs, page_width(size) as i32, size as i32, false).map_err(err)?;
    pixmap.clear_with(255).map_err(err)?;
    {
        let dev = Device::from_pixmap(&pixmap).map_err(err)?;
        let mut p = Painter {
            dev,
            cs,
            scale: size as f32 / PAGE_H,
            pal: thumb_palette(),
            body: Font::new("Times-Roman").map_err(err)?,
            bold: Font::new("Times-Bold").map_err(err)?,
            italic: Font::new("Times-Italic").map_err(err)?,
            mono: Font::new("Courier").map_err(err)?,
            y: MARGIN,
            full: false,
            colour: None,
        };
        p.fill(0.0, 0.0, PAGE_W, PAGE_H, p.pal.page)?;
        if plain {
            p.plain(text)?;
        } else {
            let doc = kitab_md::parse(text);
            for b in &doc.blocks {
                if p.full {
                    break;
                }
                p.block(b, MARGIN, PAGE_W - MARGIN)?;
            }
        }
        // The device closes on drop, before the pixmap is read.
    }
    Ok(Thumb { pixmap })
}

#[derive(Clone, Copy)]
enum Face {
    Body,
    Bold,
    Italic,
    Mono,
}

/// A word (with any trailing space) in one face and colour.
struct Word {
    text: String,
    face: Face,
    size: f32,
    colour: Rgba,
    /// A hard break before this word.
    newline: bool,
}

struct Painter {
    dev: Device,
    cs: Colorspace,
    scale: f32,
    pal: ThumbPalette,
    body: Font,
    bold: Font,
    italic: Font,
    mono: Font,
    /// Top of the next line, in page points.
    y: f32,
    /// The sheet is full: nothing more is drawn.
    full: bool,
    /// Text colour override (quotes draw muted).
    colour: Option<Rgba>,
}

type Res = Result<(), String>;

fn rgb(c: Rgba) -> [f32; 3] {
    [c.r, c.g, c.b]
}

impl Painter {
    fn font(&self, face: Face) -> &Font {
        match face {
            Face::Body => &self.body,
            Face::Bold => &self.bold,
            Face::Italic => &self.italic,
            Face::Mono => &self.mono,
        }
    }

    fn ink(&self) -> Rgba {
        self.colour.unwrap_or(self.pal.ink)
    }

    fn fill(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Rgba) -> Res {
        let err = |e: mupdf::Error| e.to_string();
        let mut path = Path::new().map_err(err)?;
        path.rect(x0, y0, x1, y1).map_err(err)?;
        self.dev
            .fill_path(
                &path,
                false,
                &Matrix::new_scale(self.scale, self.scale),
                &self.cs,
                &rgb(c),
                1.0,
                ColorParams::default(),
            )
            .map_err(err)
    }

    /// The glyph for `c`, or `None` when the font lacks it.
    fn glyph(&self, face: Face, c: char) -> Option<i32> {
        match self.font(face).encode_character(c as i32) {
            Ok(g) if g > 0 => Some(g),
            _ => None,
        }
    }

    fn advance(&self, face: Face, c: char, size: f32) -> f32 {
        match self.glyph(face, c) {
            Some(g) => self.font(face).advance_glyph(g).unwrap_or(0.5) * size,
            None if c.is_whitespace() => 0.25 * size,
            None => 0.55 * size,
        }
    }

    fn measure(&self, face: Face, text: &str, size: f32) -> f32 {
        text.chars().map(|c| self.advance(face, c, size)).sum()
    }

    /// Draw `text` with its baseline at `base`; returns the advance.
    fn run(
        &self,
        face: Face,
        text: &str,
        x: f32,
        base: f32,
        size: f32,
        c: Rgba,
    ) -> Result<f32, String> {
        let err = |e: mupdf::Error| e.to_string();
        let mut pen = x;
        for ch in text.chars() {
            let adv = self.advance(face, ch, size);
            match self.glyph(face, ch) {
                Some(g) => {
                    let s = self.scale;
                    let ctm = Matrix::new(size * s, 0.0, 0.0, -size * s, pen * s, base * s);
                    if let Some(path) = self
                        .font(face)
                        .outline_glyph_with_ctm(g, &ctm)
                        .map_err(err)?
                    {
                        self.dev
                            .fill_path(
                                &path,
                                false,
                                &Matrix::IDENTITY,
                                &self.cs,
                                &rgb(c),
                                1.0,
                                ColorParams::default(),
                            )
                            .map_err(err)?;
                    }
                }
                // A script the built-in fonts lack: a grey bar per character.
                None if !ch.is_whitespace() => self.fill(
                    pen + 0.05 * size,
                    base - 0.5 * size,
                    pen + adv - 0.05 * size,
                    base - 0.1 * size,
                    self.pal.rule,
                )?,
                None => {}
            }
            pen += adv;
        }
        Ok(pen - x)
    }

    /// `text` cut to fit `width`, with an ellipsis when cut.
    fn fit(&self, face: Face, text: &str, size: f32, width: f32) -> String {
        if self.measure(face, text, size) <= width {
            return text.to_string();
        }
        let budget = width - self.measure(face, "…", size);
        let mut out = String::new();
        let mut w = 0.0;
        for c in text.chars() {
            w += self.advance(face, c, size);
            if w > budget {
                break;
            }
            out.push(c);
        }
        out.push('…');
        out
    }

    /// Reserve a line `height` tall; false (and the sheet is full) when it
    /// doesn't fit.
    fn line(&mut self, height: f32) -> bool {
        if self.full || self.y + height > BOTTOM {
            self.full = true;
            return false;
        }
        true
    }

    fn words(&self, inlines: &Inlines, face: Face, size: f32) -> Vec<Word> {
        let mut out = Vec::new();
        let mut newline = false;
        for i in inlines {
            match i {
                Inline::Text { text, style, link } => {
                    let (f, sz) = if style.code || style.kbd {
                        (Face::Mono, size * 0.9)
                    } else if style.bold {
                        (Face::Bold, size)
                    } else if style.italic {
                        (Face::Italic, size)
                    } else {
                        (face, size)
                    };
                    let colour = if link.is_some() {
                        self.pal.accent
                    } else {
                        self.ink()
                    };
                    for piece in text.split_inclusive(char::is_whitespace) {
                        out.push(Word {
                            text: piece.replace('\n', " "),
                            face: f,
                            size: sz,
                            colour,
                            newline: std::mem::take(&mut newline),
                        });
                    }
                }
                Inline::Image { alt, .. } => out.push(Word {
                    text: format!("[{alt}] "),
                    face: Face::Italic,
                    size,
                    colour: self.pal.muted,
                    newline: std::mem::take(&mut newline),
                }),
                Inline::Break => newline = true,
                Inline::FootnoteRef { number, .. } => out.push(Word {
                    text: format!("[{number}]"),
                    face,
                    size: size * 0.8,
                    colour: self.pal.accent,
                    newline: std::mem::take(&mut newline),
                }),
            }
        }
        out
    }

    /// Lay out words between `left` and `right`, wrapping at spaces.
    fn flow(&mut self, words: &[Word], left: f32, right: f32, size: f32) -> Res {
        let lh = size * 1.45;
        let mut x = left;
        let mut started = false;
        for w in words {
            let width = self.measure(w.face, w.text.trim_end(), w.size);
            if !started || w.newline || (x > left && x + width > right) {
                if started {
                    self.y += lh;
                }
                if !self.line(lh) {
                    return Ok(());
                }
                started = true;
                x = left;
                if w.text.trim().is_empty() {
                    continue;
                }
            }
            let text = if width > right - left {
                self.fit(w.face, &w.text, w.size, right - x)
            } else {
                w.text.clone()
            };
            x += self.run(w.face, &text, x, self.y + size * 0.8, w.size, w.colour)?;
        }
        if started {
            self.y += lh;
        }
        Ok(())
    }

    fn block(&mut self, b: &Block, left: f32, right: f32) -> Res {
        if self.full {
            return Ok(());
        }
        match b {
            Block::Heading { level, content, .. } => {
                let size = match level {
                    1 => 30.0,
                    2 => 23.0,
                    3 => 19.0,
                    4 => 16.0,
                    _ => 15.0,
                };
                if self.y > MARGIN {
                    self.y += size * 0.45;
                }
                let words = self.words(content, Face::Bold, size);
                self.flow(&words, left, right, size)?;
                if *level <= 2 && !self.full {
                    self.fill(left, self.y + 2.0, right, self.y + 3.0, self.pal.rule)?;
                    self.y += 10.0;
                } else {
                    self.y += 4.0;
                }
            }
            Block::Paragraph(content) => {
                let words = self.words(content, Face::Body, BODY);
                self.flow(&words, left, right, BODY)?;
                self.y += 8.0;
            }
            Block::List { start, items } => {
                for (n, item) in items.iter().enumerate() {
                    if self.full {
                        break;
                    }
                    let base = self.y + BODY * 0.8;
                    if self.line(BODY * 1.45) {
                        match (item.task, start) {
                            (Some(done), _) => {
                                let (x0, y0) = (left + 2.0, base - 10.0);
                                self.fill(x0, y0, x0 + 10.0, y0 + 10.0, self.pal.muted)?;
                                if !done {
                                    self.fill(
                                        x0 + 1.5,
                                        y0 + 1.5,
                                        x0 + 8.5,
                                        y0 + 8.5,
                                        self.pal.page,
                                    )?;
                                }
                            }
                            (None, Some(first)) => {
                                let label = format!("{}.", first + n as u64);
                                self.run(Face::Body, &label, left, base, BODY, self.pal.muted)?;
                            }
                            (None, None) => {
                                self.run(Face::Body, "•", left + 4.0, base, BODY, self.pal.muted)?;
                            }
                        }
                    }
                    for inner in &item.blocks {
                        self.block(inner, left + 20.0, right)?;
                        // Items sit closer than paragraphs.
                        if matches!(inner, Block::Paragraph(_)) {
                            self.y -= 5.0;
                        }
                    }
                }
                self.y += 6.0;
            }
            Block::Quote(blocks) => {
                let top = self.y;
                let saved = self.colour.replace(self.pal.muted);
                for inner in blocks {
                    self.block(inner, left + 16.0, right)?;
                }
                self.colour = saved;
                let bottom = self.y.min(BOTTOM) - 6.0;
                if bottom > top {
                    self.fill(left, top, left + 3.0, bottom, self.pal.accent)?;
                }
            }
            Block::Code { text, .. } => self.code(text, left, right)?,
            Block::Mermaid(text) | Block::Math(text) => self.code(text, left, right)?,
            Block::Table { head, rows, .. } => self.table(head, rows, left, right)?,
            Block::Rule => {
                if self.line(16.0) {
                    self.fill(left, self.y + 8.0, right, self.y + 9.0, self.pal.rule)?;
                    self.y += 16.0;
                }
            }
            Block::Details { summary, .. } => {
                let mut words = vec![Word {
                    text: "› ".into(),
                    face: Face::Bold,
                    size: BODY,
                    colour: self.pal.muted,
                    newline: false,
                }];
                words.extend(self.words(summary, Face::Body, BODY));
                self.flow(&words, left, right, BODY)?;
                self.y += 6.0;
            }
            Block::Footnote { number, body, .. } => {
                let saved = self.colour.replace(self.pal.muted);
                let base = self.y + 11.0 * 0.8;
                if self.line(11.0 * 1.45) {
                    self.run(
                        Face::Body,
                        &format!("{number}."),
                        left,
                        base,
                        11.0,
                        self.pal.muted,
                    )?;
                }
                for inner in body {
                    self.block(inner, left + 18.0, right)?;
                }
                self.colour = saved;
            }
            Block::Frontmatter(pairs) => {
                let lines: Vec<String> = pairs
                    .iter()
                    .take(6)
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect();
                self.boxed(&lines, Face::Body, left, right)?;
            }
        }
        Ok(())
    }

    fn code(&mut self, text: &str, left: f32, right: f32) -> Res {
        let lines: Vec<String> = text.lines().map(|l| l.replace('\t', "    ")).collect();
        self.boxed(&lines, Face::Mono, left, right)
    }

    /// Lines of small text in a tinted box (code, frontmatter).
    fn boxed(&mut self, lines: &[String], face: Face, left: f32, right: f32) -> Res {
        let lh = CODE * 1.4;
        let pad = 8.0;
        if !self.line(lh + 2.0 * pad) {
            return Ok(());
        }
        let room = ((BOTTOM - self.y - 2.0 * pad) / lh).floor().max(1.0) as usize;
        let shown = lines.len().clamp(1, room);
        let height = shown as f32 * lh + 2.0 * pad;
        self.fill(left, self.y, right, self.y + height, self.pal.code_bg)?;
        let ink = self.ink();
        for (i, l) in lines.iter().take(shown).enumerate() {
            let t = self.fit(face, l, CODE, right - left - 2.0 * pad);
            let base = self.y + pad + i as f32 * lh + CODE * 0.85;
            self.run(face, &t, left + pad, base, CODE, ink)?;
        }
        self.y += height + 10.0;
        if shown < lines.len() {
            self.full = true;
        }
        Ok(())
    }

    fn table(&mut self, head: &[Inlines], rows: &[Vec<Inlines>], left: f32, right: f32) -> Res {
        let cols = head.len().max(1);
        let cw = (right - left) / cols as f32;
        let size = 12.0;
        let rh = 22.0;
        let row_line = |p: &mut Painter, cells: &[Inlines], face: Face| -> Res {
            if !p.line(rh) {
                return Ok(());
            }
            let ink = p.ink();
            for (i, cell) in cells.iter().take(cols).enumerate() {
                let x = left + i as f32 * cw + 4.0;
                let t = p.fit(face, &kitab_md::inline_text(cell), size, cw - 8.0);
                p.run(face, &t, x, p.y + 15.0, size, ink)?;
            }
            p.fill(left, p.y + rh - 1.0, right, p.y + rh, p.pal.rule)?;
            p.y += rh;
            Ok(())
        };
        row_line(self, head, Face::Bold)?;
        for r in rows {
            if self.full {
                break;
            }
            row_line(self, r, Face::Body)?;
        }
        self.y += 10.0;
        Ok(())
    }

    /// Plain text: lines as they are, wrapped, in the body face.
    fn plain(&mut self, text: &str) -> Res {
        for line in text.lines() {
            if self.full {
                break;
            }
            if line.trim().is_empty() {
                self.y += BODY * 0.8;
                continue;
            }
            let ink = self.ink();
            let words: Vec<Word> = line
                .split_inclusive(char::is_whitespace)
                .map(|w| Word {
                    text: w.to_string(),
                    face: Face::Body,
                    size: BODY,
                    colour: ink,
                    newline: false,
                })
                .collect();
            self.flow(&words, MARGIN, PAGE_W - MARGIN, BODY)?;
        }
        Ok(())
    }
}
