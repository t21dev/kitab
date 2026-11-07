// SPDX-License-Identifier: AGPL-3.0-or-later

//! The Marks panel's data: Kitab's marks and the annotations already in the
//! file, one row each, in page order, with the words under text markup; and
//! export to Markdown or CSV (PLAN.md Phase 6, H4).

use super::*;
use kitab_core::{MarkShape, RectPt};

/// Where a row comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkSource {
    /// Drawn in Kitab: in a PDF not saved into the file yet, elsewhere kept in Kitab.
    Kitab(u64),
    /// Already in the file: its page and place among the page's annotations.
    File { page: u32, index: u32 },
}

/// One row of the Marks panel.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkRow {
    pub page: u32,
    /// "Highlight", "Note", "Pen", …
    pub kind: String,
    pub colour: Option<(u8, u8, u8)>,
    /// A note's or text box's text, or the words a highlight covers.
    pub text: Option<String>,
    pub source: MarkSource,
}

/// A friendly name for a mark or a PDF annotation subtype.
fn label_for(kind: &str) -> &'static str {
    match kind {
        "ink" | "Ink" => "Pen",
        "line" | "Line" => "Line",
        "arrow" => "Arrow",
        "rect" | "Square" => "Rectangle",
        "ellipse" | "Circle" => "Ellipse",
        "highlight" | "Highlight" => "Highlight",
        "underline" | "Underline" | "Squiggly" => "Underline",
        "strike" | "StrikeOut" => "Strike-through",
        "text" | "FreeText" => "Text",
        "note" | "Text" => "Note",
        "Stamp" => "Stamp",
        "Polygon" | "PolyLine" => "Shape",
        "Caret" => "Insert",
        "FileAttachment" => "Attachment",
        _ => "Mark",
    }
}

/// The text under `rects`: characters whose centre falls inside, lines joined by spaces.
pub(super) fn quote(chars: &[kitab_core::TextChar], rects: &[RectPt]) -> Option<String> {
    let inside = |c: &kitab_core::TextChar| {
        let (x, y) = (c.rect.x + c.rect.w / 2.0, c.rect.y + c.rect.h / 2.0);
        rects
            .iter()
            .any(|r| x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h)
    };
    let mut out = String::new();
    let mut line = None;
    for c in chars.iter().filter(|c| inside(c)) {
        if line.is_some_and(|l| l != c.line) && !out.ends_with(' ') {
            out.push(' ');
        }
        line = Some(c.line);
        out.push(c.ch);
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    (!out.is_empty()).then_some(out)
}

impl Reader {
    /// Load the file's own annotations for the panel (once; again after a save).
    pub fn load_file_annots(&mut self, cx: &mut Context<Self>) {
        if self.file_annots.is_some() || !self.is_pdf() {
            return;
        }
        let Some(doc) = self.doc else { return };
        self.file_annots = Some(Vec::new());
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let annots = cx
                .background_spawn(async move { engine.annotations(doc) })
                .await;
            let _ = this.update(cx, |r, cx| {
                // Text markup quotes need the pages' characters.
                let pages: std::collections::BTreeSet<u32> = annots
                    .iter()
                    .filter(|a| {
                        matches!(
                            a.kind.as_str(),
                            "Highlight" | "Underline" | "StrikeOut" | "Squiggly"
                        )
                    })
                    .map(|a| a.page)
                    .collect();
                for p in pages {
                    r.ensure_chars(p, cx);
                }
                r.file_annots = Some(annots);
                cx.notify();
            });
        })
        .detach();
    }

    /// Every row for the panel, in page order (Kitab's marks after the file's on a page).
    pub fn mark_rows(&mut self, cx: &mut Context<Self>) -> Vec<MarkRow> {
        let mut rows = Vec::new();
        let mut want_chars = Vec::new();
        for a in self.file_annots.clone().unwrap_or_default() {
            let text = a.text.clone().or_else(|| {
                let markup = matches!(
                    a.kind.as_str(),
                    "Highlight" | "Underline" | "StrikeOut" | "Squiggly"
                );
                markup
                    .then(|| {
                        self.layers
                            .chars
                            .get(&a.page)
                            .and_then(|c| quote(c, &[a.rect]))
                    })
                    .flatten()
            });
            rows.push(MarkRow {
                page: a.page,
                kind: label_for(&a.kind).to_string(),
                colour: a.color.map(|c| (c.r, c.g, c.b)),
                text,
                source: MarkSource::File {
                    page: a.page,
                    index: a.index,
                },
            });
        }
        for p in &self.markup.marks {
            let m = &p.mark;
            let text = match &m.shape {
                MarkShape::Note { text, .. } | MarkShape::Text { text, .. } => Some(text.clone()),
                MarkShape::Highlight { rects }
                | MarkShape::Underline { rects }
                | MarkShape::Strike { rects } => match self.layers.chars.get(&m.page) {
                    Some(chars) => quote(chars, rects),
                    None => {
                        want_chars.push(m.page);
                        None
                    }
                },
                _ => None,
            };
            rows.push(MarkRow {
                page: m.page,
                kind: label_for(m.kind()).to_string(),
                colour: Some((m.color.r, m.color.g, m.color.b)),
                text,
                source: MarkSource::Kitab(p.local),
            });
        }
        for page in want_chars {
            self.ensure_chars(page, cx);
        }
        rows.sort_by_key(|r| r.page);
        rows
    }

    /// Delete one of Kitab's marks from the panel (undo brings it back).
    pub fn delete_mark(&mut self, local: u64, cx: &mut Context<Self>) {
        if let Some(mark) = self.unplace_mark(local, cx) {
            self.push_removed(mark);
        }
    }

    /// The marks as Markdown, for export.
    pub fn marks_markdown(&mut self, cx: &mut Context<Self>) -> String {
        let title = self.title();
        let rows = self.mark_rows(cx);
        let mut out = format!("# Marks in {title}\n\n");
        if rows.is_empty() {
            out.push_str("No marks.\n");
        }
        for r in rows {
            let text = r
                .text
                .map(|t| format!(": {}", t.replace('\n', " ")))
                .unwrap_or_default();
            out.push_str(&format!("- Page {} · {}{}\n", r.page + 1, r.kind, text));
        }
        out
    }

    /// The marks as CSV (page, kind, colour, text), for export.
    pub fn marks_csv(&mut self, cx: &mut Context<Self>) -> String {
        let field = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut out = String::from("page,kind,colour,text,saved_in_file\n");
        for r in self.mark_rows(cx) {
            let colour = r
                .colour
                .map(|(r, g, b)| format!("{r:02X}{g:02X}{b:02X}"))
                .unwrap_or_default();
            let saved = matches!(r.source, MarkSource::File { .. });
            out.push_str(&format!(
                "{},{},{},{},{}\n",
                r.page + 1,
                field(&r.kind),
                field(&colour),
                field(r.text.as_deref().unwrap_or("")),
                saved
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitab_core::TextChar;

    fn ch(c: char, x: f32, line: u32) -> TextChar {
        TextChar {
            ch: c,
            rect: RectPt {
                x,
                y: line as f32 * 20.0,
                w: 8.0,
                h: 12.0,
            },
            line,
        }
    }

    #[test]
    fn quotes_take_the_characters_under_the_boxes() {
        let chars: Vec<TextChar> = "ab cd"
            .chars()
            .enumerate()
            .map(|(i, c)| ch(c, i as f32 * 8.0, 0))
            .chain(
                "ef".chars()
                    .enumerate()
                    .map(|(i, c)| ch(c, i as f32 * 8.0, 1)),
            )
            .collect();
        let line0 = RectPt {
            x: 20.0,
            y: 0.0,
            w: 30.0,
            h: 12.0,
        };
        assert_eq!(quote(&chars, &[line0]).as_deref(), Some("cd"));
        let both = [
            line0,
            RectPt {
                x: 0.0,
                y: 20.0,
                w: 20.0,
                h: 12.0,
            },
        ];
        assert_eq!(quote(&chars, &both).as_deref(), Some("cd ef"));
        assert_eq!(
            quote(
                &chars,
                &[RectPt {
                    x: 500.0,
                    y: 0.0,
                    w: 5.0,
                    h: 5.0
                }]
            ),
            None
        );
    }

    #[test]
    fn kinds_have_friendly_names() {
        assert_eq!(label_for("ink"), "Pen");
        assert_eq!(label_for("Square"), "Rectangle");
        assert_eq!(label_for("Text"), "Note");
        assert_eq!(label_for("FreeText"), "Text");
        assert_eq!(label_for("Weird"), "Mark");
    }
}
