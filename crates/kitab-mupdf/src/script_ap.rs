// SPDX-License-Identifier: AGPL-3.0-or-later

//! Text in a form field in a script MuPDF's appearance writer can't draw.
//!
//! MuPDF writes a filled-in field's look itself (`pdf-appearance.c`), but
//! only for Latin, Greek, Cyrillic and Chinese, Japanese and Korean: every
//! other letter becomes "?", and nothing is joined, so Bangla conjuncts
//! (যুক্তাক্ষর), Devanagari, Arabic and Thai came out as question marks in
//! the saved PDF (issue #13). For those, Kitab draws the look itself:
//!
//! - the text is shaped with rustybuzz (HarfBuzz's rules), so letters join
//!   and reorder as they should;
//! - the font is the one Kitab already uses for that script when reading
//!   (`fonts.rs`: Nirmala UI, Kohinoor, Noto…), with only the glyphs used
//!   embedded (subsetter), as a Type 0 font any reader can draw;
//! - a ToUnicode map ties each glyph back to its letters, so the field's
//!   text can still be searched and copied in other readers.
//!
//! Text MuPDF can draw keeps MuPDF's own look, unchanged.

use crate::{ffi, fonts, forms};
use mupdf::pdf::{PdfDocument, PdfObject};
use mupdf_sys::*;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

/// Whether MuPDF's own writer draws every letter of `text`.
pub(crate) fn mupdf_draws(text: &str) -> bool {
    text.chars().all(|c| {
        matches!(
            ffi::script_of(c),
            UCDN_SCRIPT_COMMON
                | UCDN_SCRIPT_INHERITED
                | UCDN_SCRIPT_UNKNOWN
                | UCDN_SCRIPT_LATIN
                | UCDN_SCRIPT_GREEK
                | UCDN_SCRIPT_CYRILLIC
                | UCDN_SCRIPT_HAN
                | UCDN_SCRIPT_HIRAGANA
                | UCDN_SCRIPT_KATAKANA
                | UCDN_SCRIPT_HANGUL
                | UCDN_SCRIPT_BOPOMOFO
        )
    })
}

/// The most a font file may be (the big CJK collections are ~20 MB).
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

/// The system font for the script of `text`, with its index in a collection.
/// Read once per file per process.
fn font_for(text: &str) -> Option<(Arc<Vec<u8>>, u32)> {
    let script = text.chars().map(ffi::script_of).find(|s| {
        !matches!(
            *s,
            UCDN_SCRIPT_COMMON | UCDN_SCRIPT_INHERITED | UCDN_SCRIPT_UNKNOWN | UCDN_SCRIPT_LATIN
        )
    });
    // Latin only (a bold or italic text box): the system's sans.
    let need = script.map_or(fonts::Need::Latin, |s| fonts::need_for(s, 0));
    let (path, index) = fonts::file_for(need)?;
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<u8>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(bytes) = cache.lock().ok()?.get(&path) {
        return Some((bytes.clone(), index.max(0) as u32));
    }
    if std::fs::metadata(&path).ok()?.len() > MAX_FONT_BYTES {
        return None;
    }
    let bytes = Arc::new(std::fs::read(&path).ok()?);
    cache.lock().ok()?.insert(path, bytes.clone());
    Some((bytes, index.max(0) as u32))
}

/// One shaped glyph, in font units from the start of its line.
#[derive(Debug, Clone, Copy)]
struct Glyph {
    gid: u16,
    x: f32,
    y: f32,
    /// The byte where its cluster starts in the line's text.
    cluster: usize,
}

/// `text` shaped as one run: its glyphs and its width, in font units.
fn shape(face: &rustybuzz::Face, text: &str) -> (Vec<Glyph>, f32) {
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let out = rustybuzz::shape(face, &[], buffer);
    let mut pen = 0.0;
    let glyphs = out
        .glyph_infos()
        .iter()
        .zip(out.glyph_positions())
        .map(|(info, pos)| {
            let g = Glyph {
                gid: info.glyph_id as u16,
                x: pen + pos.x_offset as f32,
                y: pos.y_offset as f32,
                cluster: info.cluster as usize,
            };
            pen += pos.x_advance as f32;
            g
        })
        .collect();
    (glyphs, pen)
}

/// The lines `text` takes: one for a single-line field (line breaks become
/// spaces), else its paragraphs wrapped at spaces to fit `max` font units.
fn lines(face: &rustybuzz::Face, text: &str, multiline: bool, max: f32) -> Vec<String> {
    if !multiline {
        return vec![text.replace(['\r', '\n'], " ")];
    }
    let mut out = Vec::new();
    for para in text.split('\n').map(|p| p.trim_end_matches('\r')) {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && shape(face, &candidate).1 > max {
                out.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = candidate;
            }
        }
        out.push(line);
    }
    out
}

/// The fill colour in a default appearance string ("0 0 1 rg", "0 g",
/// "0 0 0 1 k"), as PDF operators. Black when it has none.
fn colour(da: &str) -> String {
    let words: Vec<&str> = da.split_whitespace().collect();
    let num = |s: &str| s.parse::<f32>().ok().filter(|v| v.is_finite());
    for (i, w) in words.iter().enumerate().rev() {
        let args = |n: usize| -> Option<Vec<f32>> {
            (i >= n).then(|| words[i - n..i].iter().filter_map(|s| num(s)).collect())
        };
        match *w {
            "rg" => {
                if let Some(v) = args(3).filter(|v| v.len() == 3) {
                    return format!("{} {} {} rg", v[0], v[1], v[2]);
                }
            }
            "g" => {
                if let Some(v) = args(1).filter(|v| v.len() == 1) {
                    return format!("{} g", v[0]);
                }
            }
            "k" => {
                if let Some(v) = args(4).filter(|v| v.len() == 4) {
                    return format!("{} {} {} {} k", v[0], v[1], v[2], v[3]);
                }
            }
            _ => {}
        }
    }
    "0 g".into()
}

/// A colour array (/MK's BG or BC: grey, RGB or CMYK) as a fill or stroke
/// operator, or `None` when there's none (transparent).
fn mk_colour(arr: Option<PdfObject>, stroke: bool) -> Option<String> {
    let arr = arr?;
    let n = arr.len().ok()?;
    let v: Vec<f32> = (0..n as i32)
        .filter_map(|i| arr.get_array(i).ok().flatten()?.as_float().ok())
        .collect();
    let op = |fill: &str, s: &str| {
        if stroke {
            s.to_string()
        } else {
            fill.to_string()
        }
    };
    match v.as_slice() {
        [g] => Some(format!("{g} {}", op("g", "G"))),
        [r, g, b] => Some(format!("{r} {g} {b} {}", op("rg", "RG"))),
        [c, m, y, k] => Some(format!("{c} {m} {y} {k} {}", op("k", "K"))),
        _ => None,
    }
}

/// The field's border width (/BS /W, else 1), as MuPDF reads it.
fn border_width(obj: &PdfObject) -> f32 {
    obj.get_dict("BS")
        .ok()
        .flatten()
        .and_then(|bs| bs.get_dict("W").ok().flatten())
        .and_then(|v| v.as_float().ok())
        .filter(|w| w.is_finite() && *w >= 0.0)
        .unwrap_or(1.0)
}

/// The field's box as MuPDF draws it under the text: /MK's background
/// fill and border colour, the border /BS /W wide (1 by default).
fn field_box(obj: &PdfObject, w: f32, h: f32) -> String {
    let mk = obj.get_dict("MK").ok().flatten();
    let get = |k: &str| mk.as_ref().and_then(|m| m.get_dict(k).ok().flatten());
    let mut out = String::new();
    if let Some(bg) = mk_colour(get("BG"), false) {
        let _ = writeln!(out, "q {bg} 0 0 {w:.2} {h:.2} re f Q");
    }
    if let Some(bc) = mk_colour(get("BC"), true) {
        let bw = border_width(obj);
        if bw > 0.0 {
            let _ = writeln!(
                out,
                "q {bc} {bw} w {0:.2} {0:.2} {1:.2} {2:.2} re S Q",
                bw / 2.0,
                (w - bw).max(0.0),
                (h - bw).max(0.0)
            );
        }
    }
    out
}

/// A PDF hex string of `s` in UTF-16BE (for the ToUnicode map).
fn utf16_hex(s: &str) -> String {
    s.encode_utf16().map(|u| format!("{u:04X}")).collect()
}

fn err(e: impl std::fmt::Display) -> String {
    format!("could not draw the field's text: {e}")
}

/// How the text sits in its box.
struct Look {
    w: f32,
    h: f32,
    /// The text size; 0 picks one as MuPDF does.
    size: f32,
    /// The fill colour, as PDF operators ("0 0 1 rg").
    colour: String,
    /// 0 left, 1 centred, 2 right.
    q: i64,
    multiline: bool,
    /// Room between the box's edge and the text.
    pad: f32,
    /// Drawn under the text (a field's background and border).
    under: String,
    /// Bold: the letters also outlined, a little, in their own colour.
    bold: bool,
    /// Italic: the letters slanted, as fonts without an italic are.
    italic: bool,
}

/// The rectangle of annotation `obj`: its width and height.
fn rect_size(obj: &PdfObject) -> Result<(f32, f32), String> {
    let rect = obj
        .get_dict("Rect")
        .map_err(err)?
        .ok_or_else(|| err("no rectangle"))?;
    let at = |i: i32| -> f32 {
        rect.get_array(i)
            .ok()
            .flatten()
            .and_then(|v| v.as_float().ok())
            .unwrap_or(0.0)
    };
    Ok(((at(2) - at(0)).abs(), (at(3) - at(1)).abs()))
}

/// Draw text widget `obj`'s value `text` ourselves when MuPDF can't (it has
/// already drawn its own look, with "?"). Returns whether it did. MuPDF won't
/// draw over it later: the widget is marked clean, and the form's
/// NeedAppearances (which asks every reader to redraw every field) is turned
/// off once every field has a look of its own (`settle_form`).
pub(crate) fn draw_widget(
    doc: &mut PdfDocument,
    obj: &mut PdfObject,
    text: &str,
) -> Result<bool, String> {
    if text.is_empty() || mupdf_draws(text) {
        return Ok(false);
    }
    let (w, h) = rect_size(obj)?;
    let da = forms::inheritable(obj, "DA")
        .and_then(|d| d.as_string().ok())
        .unwrap_or_default();
    let flags = forms::inheritable(obj, "Ff")
        .and_then(|v| v.as_int().ok())
        .unwrap_or(0);
    let look = Look {
        w,
        h,
        size: forms::font_size(obj),
        colour: colour(&da),
        q: forms::inheritable(obj, "Q")
            .and_then(|v| v.as_int().ok())
            .unwrap_or(0)
            .into(),
        multiline: flags & (1 << 12) != 0,
        // Twice the border, as MuPDF leaves for its own text.
        pad: 2.0 * border_width(obj),
        under: field_box(obj, w, h),
        bold: false,
        italic: false,
    };
    draw(doc, obj, text, &look)
}

/// Draw text box (FreeText) `obj` ourselves when MuPDF can't: text in a
/// script it doesn't draw, or bold or italic (its text boxes are plain).
/// Wrapped in its rectangle, `size` points, in colour `rgb` (0–1).
pub(crate) fn draw_text_box(
    doc: &mut PdfDocument,
    obj: &mut PdfObject,
    text: &str,
    size: f32,
    rgb: (f32, f32, f32),
    style: kitab_core::TextStyle,
) -> Result<bool, String> {
    if text.is_empty() || (mupdf_draws(text) && !style.bold && !style.italic) {
        return Ok(false);
    }
    let (w, h) = rect_size(obj)?;
    let look = Look {
        w,
        h,
        size,
        colour: format!(
            "{} {} {} rg {} {} {} RG",
            rgb.0, rgb.1, rgb.2, rgb.0, rgb.1, rgb.2
        ),
        q: i64::from(style.align.quadding()),
        multiline: true,
        pad: 2.0,
        under: String::new(),
        bold: style.bold,
        italic: style.italic,
    };
    draw(doc, obj, text, &look)
}

/// Draw `text` into annotation `obj` as its normal appearance, shaped and
/// with its font embedded, then mark `obj` clean.
fn draw(
    doc: &mut PdfDocument,
    obj: &mut PdfObject,
    text: &str,
    look: &Look,
) -> Result<bool, String> {
    let Some((bytes, index)) = font_for(text) else {
        return Ok(false);
    };
    let face = rustybuzz::Face::from_slice(&bytes, index).ok_or_else(|| err("unreadable font"))?;
    let upem = (face.units_per_em() as f32).max(1.0);
    let (w, h, q, multiline, pad) = (look.w, look.h, look.q, look.multiline, look.pad);
    if w < 1.0 || h < 1.0 {
        return Ok(false);
    }
    let (ascent, descent) = (f32::from(face.ascender()), f32::from(face.descender()));

    // The size: the field's own, else (0, "auto") as MuPDF picks it: 12 in
    // a multi-line field, else the largest that fits.
    let mut size = look.size;
    let lines = if size > 0.0 {
        lines(&face, text, multiline, (w - 2.0 * pad) / size * upem)
    } else if multiline {
        size = 12.0;
        lines(&face, text, true, (w - 2.0 * pad) / size * upem)
    } else {
        let line = lines(&face, text, false, 0.0);
        let fit_h = (h - 2.0 * pad) / ((ascent - descent) / upem);
        let width = shape(&face, &line[0]).1 / upem;
        size = fit_h.min((w - 2.0 * pad) / width.max(0.01)).max(4.0);
        line
    };
    let scale = size / upem;
    let leading = size * 1.15;

    // Lay the glyphs out, and note which letters each one stands for.
    let mut remap = subsetter::GlyphRemapper::new();
    let mut to_unicode: Vec<(u16, String)> = Vec::new();
    let mut seen_gids: HashMap<u16, ()> = HashMap::new();
    let mut widths: Vec<(u16, f32)> = Vec::new();
    let mut ops = String::new();
    let first_base = if multiline {
        h - pad - ascent * scale
    } else {
        (h - (ascent - descent) * scale) / 2.0 - descent * scale
    };
    for (n, line) in lines.iter().enumerate() {
        let (glyphs, width) = shape(&face, line);
        let width = width * scale;
        let x0 = match q {
            1 => (w - width) / 2.0,
            2 => w - pad - width,
            _ => pad,
        };
        let base = first_base - n as f32 * leading;
        let mut starts: Vec<usize> = glyphs.iter().map(|g| g.cluster).collect();
        starts.sort_unstable();
        starts.dedup();
        // Glyph by glyph, a cluster (the glyphs of one or more letters, as
        // shaping joined them) at a time. A cluster of one glyph tells its
        // letters through the font's ToUnicode map; one of several (a vowel
        // sign drawn on both sides of its consonant), or a glyph already
        // standing for other letters, carries them as ActualText.
        let mut i = 0;
        while i < glyphs.len() {
            let cluster = glyphs[i].cluster;
            let mut j = i + 1;
            while j < glyphs.len() && glyphs[j].cluster == cluster {
                j += 1;
            }
            let end = starts
                .iter()
                .find(|&&s| s > cluster)
                .copied()
                .unwrap_or(line.len());
            let letters = line.get(cluster..end).unwrap_or("");
            let mut news = Vec::with_capacity(j - i);
            for g in &glyphs[i..j] {
                let new = remap.remap(g.gid);
                if seen_gids.insert(new, ()).is_none() {
                    let adv = face
                        .glyph_hor_advance(rustybuzz::ttf_parser::GlyphId(g.gid))
                        .map_or(0.0, f32::from);
                    widths.push((new, adv * 1000.0 / upem));
                }
                news.push(new);
            }
            let mapped = |gid: u16| to_unicode.iter().find(|(g, _)| *g == gid).map(|(_, t)| t);
            let simple = news.len() == 1
                && !letters.is_empty()
                && mapped(news[0]).is_none_or(|t| t == letters);
            if simple && mapped(news[0]).is_none() {
                to_unicode.push((news[0], letters.to_string()));
            }
            if !simple && !letters.is_empty() {
                let _ = writeln!(
                    ops,
                    "/Span << /ActualText <FEFF{}> >> BDC",
                    utf16_hex(letters)
                );
            }
            // Italic: slanted about 12°, as a font without an italic is.
            let skew = if look.italic { 0.21 } else { 0.0 };
            for (g, new) in glyphs[i..j].iter().zip(&news) {
                let _ = writeln!(
                    ops,
                    "1 0 {skew} 1 {:.3} {:.3} Tm <{new:04X}> Tj",
                    x0 + g.x * scale,
                    base + g.y * scale
                );
            }
            if !simple && !letters.is_empty() {
                ops.push_str("EMC\n");
            }
            i = j;
        }
    }
    let program = subsetter::subset(&bytes, index, &remap).map_err(err)?;
    let cff = face.tables().cff.is_some();

    // The font: Type 0 over a CID font, only the glyphs used.
    let name = format!("KITABF+Kitab{}", if cff { "CFF" } else { "TT" });
    let mut buf = mupdf::Buffer::with_capacity(program.len());
    std::io::Write::write_all(&mut buf, &program).map_err(err)?;
    let ff_dict = doc
        .new_object_from_str(&if cff {
            "<< /Subtype /OpenType >>".to_string()
        } else {
            format!("<< /Length1 {} >>", program.len())
        })
        .map_err(err)?;
    let ff = doc.add_stream(&buf, Some(&ff_dict), false).map_err(err)?;
    let ff = ff.as_indirect().map_err(err)?;

    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def /CMapType 2 def\n1 begincodespacerange <0000> <FFFF> endcodespacerange\n",
    );
    for chunk in to_unicode.chunks(100) {
        let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
        for (gid, t) in chunk {
            let _ = writeln!(cmap, "<{gid:04X}> <{}>", utf16_hex(t));
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap CMapName currentdict /CMap defineresource pop end end\n");
    let mut buf = mupdf::Buffer::with_capacity(cmap.len());
    std::io::Write::write_all(&mut buf, cmap.as_bytes()).map_err(err)?;
    let tu_dict = doc.new_object_from_str("<< >>").map_err(err)?;
    let tu = doc.add_stream(&buf, Some(&tu_dict), false).map_err(err)?;
    let tu = tu.as_indirect().map_err(err)?;

    let bbox = face.global_bounding_box();
    let k = 1000.0 / upem;
    let descriptor = doc
        .new_object_from_str(&format!(
            "<< /Type /FontDescriptor /FontName /{name} /Flags 4 /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV 80 /{} {ff} 0 R >>",
            (f32::from(bbox.x_min) * k).round(),
            (f32::from(bbox.y_min) * k).round(),
            (f32::from(bbox.x_max) * k).round(),
            (f32::from(bbox.y_max) * k).round(),
            (ascent * k).round(),
            (descent * k).round(),
            (ascent * k).round(),
            if cff { "FontFile3" } else { "FontFile2" },
        ))
        .map_err(err)?;
    let descriptor = doc.add_object(&descriptor).map_err(err)?;
    let descriptor = descriptor.as_indirect().map_err(err)?;
    let mut w_array = String::new();
    for (gid, adv) in &widths {
        let _ = write!(w_array, "{gid} [{}] ", adv.round());
    }
    let cid = doc
        .new_object_from_str(&format!(
            "<< /Type /Font /Subtype /{} /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /DW 0 /W [{w_array}]{} >>",
            if cff { "CIDFontType0" } else { "CIDFontType2" },
            if cff { "" } else { " /CIDToGIDMap /Identity" },
        ))
        .map_err(err)?;
    let cid = doc.add_object(&cid).map_err(err)?;
    let cid = cid.as_indirect().map_err(err)?;
    let font = doc
        .new_object_from_str(&format!(
            "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {tu} 0 R >>"
        ))
        .map_err(err)?;
    let font = doc.add_object(&font).map_err(err)?;
    let font = font.as_indirect().map_err(err)?;

    // The look: clipped to the field, as MuPDF's own (/Tx marks it as a
    // field's variable text).
    let content = format!(
        "{}/Tx BMC q {pad:.2} {pad:.2} {:.2} {:.2} re W n BT /KitabF {size:.2} Tf {}{}\n{ops}ET Q EMC\n",
        look.under,
        (w - 2.0 * pad).max(0.0),
        (h - 2.0 * pad).max(0.0),
        look.colour,
        // Bold: fill and outline the letters (render mode 2), the outline
        // about 3 % of the size, in the text's colour.
        if look.bold {
            format!(" 2 Tr {:.3} w 1 j", size * 0.03)
        } else {
            String::new()
        },
    );
    let mut buf = mupdf::Buffer::with_capacity(content.len());
    std::io::Write::write_all(&mut buf, content.as_bytes()).map_err(err)?;
    let ap_dict = doc
        .new_object_from_str(&format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 {w:.2} {h:.2}] /Resources << /Font << /KitabF {font} 0 R >> >> >>"
        ))
        .map_err(err)?;
    let ap = doc.add_stream(&buf, Some(&ap_dict), false).map_err(err)?;
    let ap = ap.as_indirect().map_err(err)?;
    let ap_ref = doc
        .new_object_from_str(&format!("<< /N {ap} 0 R >>"))
        .map_err(err)?;
    obj.dict_put("AP", ap_ref).map_err(err)?;
    ffi::clean_obj(obj);
    Ok(true)
}

/// The form asks readers to draw every field again (NeedAppearances), which
/// would draw over Kitab's look with MuPDF's "?". Once every field on every
/// page has a look of its own (MuPDF draws the missing ones as each page
/// loads), turn the request off. Skipped for very long documents, which keep
/// the request (their other readers redraw; MuPDF in Kitab too).
pub(crate) fn settle_form(doc: &mut PdfDocument, page_count: u32, max_pages: u32) {
    let need = doc
        .catalog()
        .ok()
        .and_then(|c| c.get_dict("AcroForm").ok().flatten())
        .and_then(|f| f.get_dict("NeedAppearances").ok().flatten())
        .and_then(|v| v.as_bool().ok())
        .unwrap_or(false);
    if !need || page_count > max_pages {
        return;
    }
    for i in 0..page_count {
        if let Ok(mut p) = doc.load_pdf_page(i as i32) {
            let _ = p.update();
        }
    }
    if let Some(mut form) = doc
        .catalog()
        .ok()
        .and_then(|c| c.get_dict("AcroForm").ok().flatten())
    {
        let _ = form.dict_put("NeedAppearances", PdfObject::new_bool(false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_what_mupdf_draws() {
        assert!(mupdf_draws("Hello, Ünïcode 123"));
        assert!(mupdf_draws("Привет, Γειά, 你好, こんにちは, 안녕"));
        assert!(!mupdf_draws("আমার সোনার বাংলা"));
        assert!(!mupdf_draws("नमस्ते"));
        assert!(!mupdf_draws("مرحبا"));
        assert!(!mupdf_draws("สวัสดี"));
    }

    #[test]
    fn colours_come_from_the_default_appearance() {
        assert_eq!(colour("/Helv 9 Tf 0 0 1 rg"), "0 0 1 rg");
        assert_eq!(colour("/Helv 0 Tf 0.5 g"), "0.5 g");
        assert_eq!(colour("/Helv 0 Tf"), "0 g");
        assert_eq!(colour("0 0 0 1 k /Helv 9 Tf"), "0 0 0 1 k");
    }

    #[test]
    fn utf16_for_tounicode() {
        assert_eq!(utf16_hex("ক্ষ"), "099509CD09B7");
        assert_eq!(utf16_hex("😀"), "D83DDE00");
    }
}
