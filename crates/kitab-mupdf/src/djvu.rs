// SPDX-License-Identifier: AGPL-3.0-or-later

//! DjVu (scanned books and documents), read with djvu-rs, a decoder written
//! from the public DjVu v3 specification. The pages draw through MuPDF (see
//! `source.rs`); what is read here directly: the text layer (find, selection,
//! copy, read aloud), the links, the bookmarks (the contents) and the title.

use djvu_rs::DjVuDocument;
use djvu_rs::Rotation;
use djvu_rs::annotation::Shape;
use djvu_rs::text::{TextZone, TextZoneKind};
use kitab_core::{Link, LinkTarget, OutlineItem, RectPt, SizePt};

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
    let names = page_names(doc);
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
        let (page, uri) = target(&b.url, names, page_count, None);
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

/// The page names (`#name` targets), in page order.
fn page_names(doc: &DjVuDocument) -> Vec<String> {
    doc.component_directory()
        .map(|d| {
            d.into_iter()
                .filter(|e| e.kind == 'P')
                .map(|e| e.id)
                .collect()
        })
        .unwrap_or_default()
}

/// Where a DjVu link goes: `#3` (a page number, from 1), `#+1` / `#-1`
/// (pages on from `from`, the page the link is on), `#name` (a page's file
/// name), or a web or mail address. Anything else (relative files, scripts)
/// goes nowhere.
fn target(
    url: &str,
    names: &[String],
    page_count: u32,
    from: Option<u32>,
) -> (Option<u32>, Option<String>) {
    let url = url.trim();
    let Some(rest) = url.strip_prefix('#') else {
        let lower = url.to_ascii_lowercase();
        let external = ["http://", "https://", "mailto:"]
            .iter()
            .any(|s| lower.starts_with(s));
        return (None, external.then(|| url.to_string()));
    };
    let rest = rest.trim();
    let page = match rest.as_bytes().first() {
        // Rust's parse takes a leading '+', so the relative forms come first.
        Some(b'+') | Some(b'-') => from.and_then(|from| {
            let n = rest[1..].parse::<u32>().ok()?;
            if rest.starts_with('+') {
                from.checked_add(n)
            } else {
                from.checked_sub(n)
            }
        }),
        Some(b'0'..=b'9') => rest.parse::<u32>().ok().and_then(|n| n.checked_sub(1)),
        _ => names
            .iter()
            .position(|n| n == rest)
            .and_then(|i| u32::try_from(i).ok()),
    };
    (page.filter(|&p| p < page_count), None)
}

/// The page's hyperlinks (its `maparea`s with an address), in page points.
pub(crate) fn page_links(doc: &DjVuDocument, index: usize, page_count: u32) -> Vec<Link> {
    let Ok(page) = doc.page(index) else {
        return Vec::new();
    };
    let Ok(areas) = page.hyperlinks() else {
        return Vec::new();
    };
    if areas.is_empty() {
        return Vec::new();
    }
    let names = page_names(doc);
    let scale = 72.0 / f32::from(page.dpi().max(1));
    let height = f32::from(page.height());
    let from = u32::try_from(index).ok();
    let mut out = Vec::new();
    for area in areas.iter().take(10_000) {
        let target = match target(&area.url, &names, page_count, from) {
            (Some(page), _) => LinkTarget::Page { page, y: None },
            (None, Some(uri)) => LinkTarget::Uri(uri),
            (None, None) => continue,
        };
        let Some((x, y, w, h)) = bounds(&area.shape) else {
            continue;
        };
        // DjVu counts up from the bottom of the page; Kitab down from the top.
        out.push(Link {
            rect: RectPt {
                x: x * scale,
                y: (height - y - h).max(0.0) * scale,
                w: w * scale,
                h: h * scale,
            },
            target,
        });
    }
    out
}

/// A map area's bounding box: x, y (from the bottom), width, height in pixels.
fn bounds(shape: &Shape) -> Option<(f32, f32, f32, f32)> {
    let (x0, y0, x1, y1) = match shape {
        Shape::Rect(r) | Shape::Oval(r) | Shape::Text(r) => (
            r.x as f32,
            r.y as f32,
            r.x as f32 + r.width as f32,
            r.y as f32 + r.height as f32,
        ),
        Shape::Poly(points) => {
            let xs = points.iter().map(|p| p.0 as f32);
            let ys = points.iter().map(|p| p.1 as f32);
            (
                xs.clone().fold(f32::INFINITY, f32::min),
                ys.clone().fold(f32::INFINITY, f32::min),
                xs.fold(f32::NEG_INFINITY, f32::max),
                ys.fold(f32::NEG_INFINITY, f32::max),
            )
        }
        Shape::Line(ax, ay, bx, by) => (
            *ax.min(bx) as f32,
            *ay.min(by) as f32,
            *ax.max(bx) as f32,
            *ay.max(by) as f32,
        ),
    };
    let (w, h) = (x1 - x0, y1 - y0);
    (x0.is_finite() && y0.is_finite() && w > 0.0 && h > 0.0).then_some((x0, y0, w, h))
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
        assert_eq!(target("#2", &names, 5, None), (Some(1), None));
        assert_eq!(target("#p0002.djvu", &names, 5, None), (Some(1), None));
        assert_eq!(target("#9", &names, 5, None), (None, None));
        assert_eq!(target("#0", &names, 5, None), (None, None));
        assert_eq!(
            target("https://example.org", &names, 5, None),
            (None, Some("https://example.org".into()))
        );
        // Relative pages, from the page the link is on.
        assert_eq!(target("#+1", &names, 5, Some(2)), (Some(3), None));
        assert_eq!(target("#-2", &names, 5, Some(2)), (Some(0), None));
        assert_eq!(target("#-3", &names, 5, Some(2)), (None, None));
        assert_eq!(target("#+3", &names, 5, Some(2)), (None, None));
        assert_eq!(target("#+1", &names, 5, None), (None, None));
        // Nothing that could run or reach the disk.
        assert_eq!(target("javascript:x()", &names, 5, Some(0)), (None, None));
        assert_eq!(
            target("file:///etc/passwd", &names, 5, Some(0)),
            (None, None)
        );
        assert_eq!(target("other.djvu", &names, 5, Some(0)), (None, None));
    }

    #[test]
    fn link_shapes_have_boxes() {
        use djvu_rs::annotation::Rect;
        let r = Rect {
            x: 10,
            y: 20,
            width: 30,
            height: 40,
        };
        assert_eq!(bounds(&Shape::Rect(r)), Some((10.0, 20.0, 30.0, 40.0)));
        assert_eq!(
            bounds(&Shape::Poly(vec![(5, 9), (15, 1), (8, 4)])),
            Some((5.0, 1.0, 10.0, 8.0))
        );
        assert_eq!(bounds(&Shape::Line(9, 9, 1, 1)), Some((1.0, 1.0, 8.0, 8.0)));
        assert_eq!(bounds(&Shape::Poly(Vec::new())), None);
    }
}
