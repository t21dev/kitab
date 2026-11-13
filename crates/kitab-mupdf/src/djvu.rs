// SPDX-License-Identifier: AGPL-3.0-or-later

//! DjVu (scanned books and documents), read with djvu-rs, a decoder written
//! from the public DjVu v3 specification. The pages draw through MuPDF (see
//! `source.rs`); what is read here directly: the text layer (find, selection,
//! copy, read aloud), the bookmarks (the contents) and the title.

use djvu_rs::DjVuDocument;
use djvu_rs::Rotation;
use djvu_rs::text::{TextZone, TextZoneKind};
use kitab_core::{OutlineItem, RectPt, SizePt};

use crate::find::{PageText, SourceChar};

/// A page's size in points (72 a inch), as it shows: turned when the page
/// says it is.
pub(crate) fn page_size(doc: &DjVuDocument, index: usize) -> Option<SizePt> {
    let page = doc.page(index).ok()?;
    let dpi = f32::from(page.dpi().max(1));
    let (w, h) = (
        f32::from(page.width()) * 72.0 / dpi,
        f32::from(page.height()) * 72.0 / dpi,
    );
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some(match page.rotation() {
        Rotation::Ccw90 | Rotation::Cw90 => SizePt { w: h, h: w },
        _ => SizePt { w, h },
    })
}

/// The page's text with a box per character, lines in reading order. Pages
/// without a text layer (not OCRed) have none.
pub(crate) fn page_text(doc: &DjVuDocument, index: usize) -> PageText {
    let mut text = PageText::default();
    let Ok(page) = doc.page(index) else {
        return text;
    };
    let Ok(Some(layer)) = page.text_layer() else {
        return text;
    };
    let scale = 72.0 / f32::from(page.dpi().max(1));
    let mut lines = Vec::new();
    for zone in &layer.zones {
        collect_lines(zone, &mut lines);
    }
    // A page whose layer has words but no line zones: one line of words.
    if lines.is_empty() {
        let mut words = Vec::new();
        for zone in &layer.zones {
            collect(zone, TextZoneKind::Word, &mut words);
        }
        if !words.is_empty() {
            lines.push(words);
        }
    }
    for words in lines {
        let index = text.lines.len();
        let mut line = String::new();
        let mut prev: Option<RectPt> = None;
        for word in words {
            let r = rect(&word.rect, scale);
            if let Some(p) = prev {
                // The space between words, spanning the gap.
                let x = p.x + p.w;
                text.chars.push(SourceChar {
                    ch: ' ',
                    rect: RectPt {
                        x,
                        y: p.y.min(r.y),
                        w: (r.x - x).max(0.0),
                        h: p.h.max(r.h),
                    },
                    line: index,
                });
                line.push(' ');
            }
            push_word(&mut text, &mut line, word, r, scale, index);
            prev = Some(r);
        }
        text.lines.push(line);
    }
    text
}

/// A word's characters: their own boxes when the layer has them, else the
/// word's box shared out evenly.
fn push_word(
    text: &mut PageText,
    line: &mut String,
    word: &TextZone,
    r: RectPt,
    scale: f32,
    index: usize,
) {
    let mut chars = Vec::new();
    collect(word, TextZoneKind::Character, &mut chars);
    let own: Vec<(char, RectPt)> = chars
        .iter()
        .filter_map(|z| Some((z.text.trim().chars().next()?, rect(&z.rect, scale))))
        .collect();
    let letters: Vec<char> = word.text.trim().chars().collect();
    if !own.is_empty() && own.len() == letters.len() {
        for (ch, rect) in own {
            text.chars.push(SourceChar {
                ch,
                rect,
                line: index,
            });
            line.push(ch);
        }
        return;
    }
    let n = letters.len().max(1) as f32;
    for (i, ch) in letters.into_iter().enumerate() {
        text.chars.push(SourceChar {
            ch,
            rect: RectPt {
                x: r.x + r.w * i as f32 / n,
                y: r.y,
                w: r.w / n,
                h: r.h,
            },
            line: index,
        });
        line.push(ch);
    }
}

fn rect(r: &djvu_rs::text::Rect, scale: f32) -> RectPt {
    RectPt {
        x: r.x as f32 * scale,
        y: r.y as f32 * scale,
        w: r.width as f32 * scale,
        h: r.height as f32 * scale,
    }
}

/// Each line zone's words, in order.
fn collect_lines<'a>(zone: &'a TextZone, out: &mut Vec<Vec<&'a TextZone>>) {
    if zone.kind == TextZoneKind::Line {
        let mut words = Vec::new();
        collect(zone, TextZoneKind::Word, &mut words);
        if words.is_empty() && !zone.text.trim().is_empty() {
            // A line without word zones: the line is one word.
            words.push(zone);
        }
        if !words.is_empty() {
            out.push(words);
        }
        return;
    }
    for child in &zone.children {
        collect_lines(child, out);
    }
}

fn collect<'a>(zone: &'a TextZone, kind: TextZoneKind, out: &mut Vec<&'a TextZone>) {
    if zone.kind == kind {
        out.push(zone);
        return;
    }
    for child in &zone.children {
        collect(child, kind, out);
    }
}

/// The bookmarks as Kitab's contents. Targets are `#3` (a page number,
/// from 1) or `#name` (a page's file name in the document).
pub(crate) fn outline(doc: &DjVuDocument, page_count: u32) -> Vec<OutlineItem> {
    let names: Vec<String> = doc
        .component_directory()
        .map(|d| {
            d.into_iter()
                .filter(|e| e.kind == 'P')
                .map(|e| e.id)
                .collect()
        })
        .unwrap_or_default();
    let mut budget = 100_000usize;
    convert(doc.bookmarks(), &names, page_count, 0, &mut budget)
}

fn convert(
    items: &[djvu_rs::djvu_document::DjVuBookmark],
    names: &[String],
    page_count: u32,
    depth: usize,
    budget: &mut usize,
) -> Vec<OutlineItem> {
    let mut out = Vec::new();
    if depth > 64 {
        return out;
    }
    for b in items {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let (page, uri) = target(&b.url, names, page_count);
        out.push(OutlineItem {
            title: b.title.trim().to_string(),
            page,
            uri,
            open: false,
            children: convert(&b.children, names, page_count, depth + 1, budget),
        });
    }
    out
}

fn target(url: &str, names: &[String], page_count: u32) -> (Option<u32>, Option<String>) {
    let Some(rest) = url.strip_prefix('#') else {
        let external = url.contains("://") || url.starts_with("mailto:");
        return (None, external.then(|| url.to_string()));
    };
    let page = match rest.trim().parse::<u32>() {
        Ok(n) if n >= 1 => Some(n - 1),
        _ => names
            .iter()
            .position(|n| n == rest)
            .and_then(|i| u32::try_from(i).ok()),
    };
    (page.filter(|&p| p < page_count), None)
}

/// The title the document gives itself, if any.
pub(crate) fn title(doc: &DjVuDocument) -> Option<String> {
    let t = doc.metadata().ok()??.title?;
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmark_targets() {
        let names = vec!["p0001.djvu".to_string(), "p0002.djvu".to_string()];
        assert_eq!(target("#2", &names, 5), (Some(1), None));
        assert_eq!(target("#p0002.djvu", &names, 5), (Some(1), None));
        assert_eq!(target("#9", &names, 5), (None, None));
        assert_eq!(target("#0", &names, 5), (None, None));
        assert_eq!(
            target("https://example.org", &names, 5),
            (None, Some("https://example.org".into()))
        );
    }
}
