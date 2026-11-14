// SPDX-License-Identifier: AGPL-3.0-or-later

//! Text selection in Markdown: drag across paragraphs, headings, list items,
//! table cells and code lines; double-click a word, triple-click a whole
//! paragraph or code line; Ctrl+C or the page menu copies.
//!
//! Every piece of text the view draws is a *unit* with a key in document
//! order: (top-level block, n-th text in it). While drawing, each unit
//! leaves its laid-out text here, so a pointer position maps to a unit and a
//! byte offset in it. A selection end is (block, unit, offset).

use gpui::{Bounds, Hsla, Pixels, Point, SharedString, TextLayout, TextRun, px};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Range;
use std::rc::Rc;

pub(super) type Key = (usize, u32);
/// A selection end: unit key and byte offset in its text.
pub(super) type End = (Key, usize);

pub(super) struct Unit {
    pub key: Key,
    pub layout: TextLayout,
}

#[derive(Default)]
pub(super) struct MdSelect {
    /// The units drawn in the latest frame (cleared as the view draws).
    pub units: Rc<RefCell<Vec<Unit>>>,
    /// Every unit's text seen so far, for copying units since scrolled away.
    pub texts: Rc<RefCell<BTreeMap<Key, SharedString>>>,
    pub anchor: Option<End>,
    pub head: Option<End>,
    pub dragging: bool,
}

impl MdSelect {
    /// (start, end) in document order, or `None` when nothing is selected.
    pub fn range(&self) -> Option<(End, End)> {
        let (a, h) = (self.anchor?, self.head?);
        if a == h {
            return None;
        }
        Some(if a <= h { (a, h) } else { (h, a) })
    }

    /// Forget everything (a new document, or the text changed).
    pub fn reset(&mut self) {
        self.anchor = None;
        self.head = None;
        self.dragging = false;
        self.texts.borrow_mut().clear();
    }

    /// The selection end nearest window point `p`: the unit under it, else
    /// the nearest one, and the character boundary nearest in it.
    pub fn end_at(&self, p: Point<Pixels>) -> Option<End> {
        let units = self.units.borrow();
        let mut best: Option<(f32, &Unit, Bounds<Pixels>)> = None;
        for u in units.iter() {
            let Some(b) = u.layout.kitab_bounds() else {
                continue;
            };
            let dy = if p.y < b.top() {
                f32::from(b.top() - p.y)
            } else if p.y > b.bottom() {
                f32::from(p.y - b.bottom())
            } else {
                0.0
            };
            let dx = if p.x < b.left() {
                f32::from(b.left() - p.x)
            } else if p.x > b.right() {
                f32::from(p.x - b.right())
            } else {
                0.0
            };
            // Rows first: a unit on the pointer's line beats a nearer one above.
            let d = dy * 1000.0 + dx;
            if best.as_ref().is_none_or(|(bd, _, _)| d < *bd) {
                best = Some((d, u, b));
            }
        }
        let (_, u, b) = best?;
        let len = u.layout.len();
        // Above or below the unit: its start or end; else the nearest boundary.
        let offset = if p.y < b.top() {
            0
        } else if p.y > b.bottom() {
            len
        } else {
            let at = Point {
                x: p.x.clamp(b.left(), b.right()),
                y: p.y.clamp(b.top(), b.bottom() - px(1.0)),
            };
            u.layout.index_for_position(at).unwrap_or_else(|i| i)
        };
        Some((u.key, offset.min(len)))
    }

    /// The word around `end` (letters, digits and joining marks; a run of
    /// anything else is its own "word").
    pub fn word_at(&self, end: End) -> Option<(End, End)> {
        let texts = self.texts.borrow();
        let text = texts.get(&end.0)?;
        let (a, b) = word_bounds(text, end.1);
        Some(((end.0, a), (end.0, b)))
    }

    /// The whole unit around `end`.
    pub fn unit_at(&self, end: End) -> Option<(End, End)> {
        let len = self.texts.borrow().get(&end.0)?.len();
        Some(((end.0, 0), (end.0, len)))
    }

    /// The selected text: units of one block joined by line breaks, blocks
    /// by a blank line.
    pub fn text(&self) -> Option<String> {
        let range = self.range()?;
        let ((k0, _), (k1, _)) = range;
        let texts = self.texts.borrow();
        let mut out = String::new();
        let mut last_block = None;
        for (key, text) in texts.range(k0..=k1) {
            let Some(r) = part_of(range, *key, text.len()) else {
                continue;
            };
            let Some(piece) = text.get(r) else {
                continue;
            };
            if let Some(b) = last_block {
                out.push_str(if b == key.0 { "\n" } else { "\n\n" });
            }
            out.push_str(piece);
            last_block = Some(key.0);
        }
        Some(out).filter(|t| !t.trim().is_empty())
    }
}

fn part_of(((k0, o0), (k1, o1)): (End, End), key: Key, len: usize) -> Option<Range<usize>> {
    if key < k0 || key > k1 {
        return None;
    }
    let a = if key == k0 { o0.min(len) } else { 0 };
    let b = if key == k1 { o1.min(len) } else { len };
    (a < b).then_some(a..b)
}

fn word_bounds(text: &str, at: usize) -> (usize, usize) {
    let at = at.min(text.len());
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '\'';
    // The character under the pointer: after `at`, or before it at the end.
    let here = text[at..]
        .chars()
        .next()
        .or_else(|| text[..at].chars().next_back());
    let Some(here) = here else {
        return (at, at);
    };
    let kind = is_word(here);
    let same = |c: char| {
        is_word(c) == kind && !c.is_whitespace() || (c.is_whitespace() && here.is_whitespace())
    };
    let mut a = at;
    for (i, c) in text[..at].char_indices().rev() {
        if !same(c) {
            break;
        }
        a = i;
    }
    let mut b = at;
    for (i, c) in text[at..].char_indices() {
        if !same(c) {
            break;
        }
        b = at + i + c.len_utf8();
    }
    (a, b)
}

/// `runs` with `sel` (byte range of their text) given the selection colour.
pub(super) fn select_runs(
    runs: Vec<TextRun>,
    sel: Option<Range<usize>>,
    color: Hsla,
) -> Vec<TextRun> {
    let Some(sel) = sel else {
        return runs;
    };
    let mut out = Vec::with_capacity(runs.len() + 2);
    let mut at = 0;
    for run in runs {
        let (start, end) = (at, at + run.len);
        at = end;
        // Cut points inside this run, in order.
        let cuts = [
            start,
            sel.start.clamp(start, end),
            sel.end.clamp(start, end),
            end,
        ];
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if b <= a {
                continue;
            }
            let mut piece = run.clone();
            piece.len = b - a;
            if a >= sel.start && b <= sel.end {
                piece.background_color = Some(color);
            }
            out.push(piece);
        }
    }
    out
}

impl super::Reader {
    /// Left button down over Markdown: start a selection (Shift extends
    /// it), or select the word (double-click) or the unit (triple-click).
    pub(super) fn md_select_down(
        &mut self,
        e: &gpui::MouseDownEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(end) = self.md_sel.end_at(e.position) else {
            self.clear_selection(cx);
            return;
        };
        let s = &mut self.md_sel;
        let span = match e.click_count {
            2 => s.word_at(end),
            n if n >= 3 => s.unit_at(end),
            _ => None,
        };
        if let Some((a, b)) = span {
            s.anchor = Some(a);
            s.head = Some(b);
            s.dragging = false;
        } else {
            if !(e.modifiers.shift && s.anchor.is_some()) {
                s.anchor = Some(end);
            }
            s.head = Some(end);
            s.dragging = true;
        }
        cx.notify();
    }

    /// A code block's scroll bar thumb dragged to pointer x `x`.
    pub(super) fn md_hdrag_to(&mut self, g: super::HDrag, x: f32, cx: &mut gpui::Context<Self>) {
        let Some(md) = self.md.as_ref() else { return };
        let Some(scroll) = md.code_scroll.borrow().get(&g.key).cloned() else {
            return;
        };
        let max = f32::from(scroll.max_offset().width);
        let view = f32::from(scroll.bounds().size.width);
        if max < 1.0 || view < 1.0 {
            return;
        }
        // The thumb moves over the track; the text moves (view + max) / view
        // times as far.
        let track = (view - 24.0).max(1.0);
        let shown = (view / (view + max)).clamp(0.08, 1.0);
        let per_px = max / (track * (1.0 - shown)).max(1.0);
        let to = (g.from_offset + (x - g.from_x) * per_px).clamp(0.0, max);
        let mut o = scroll.offset();
        o.x = gpui::px(-to);
        scroll.set_offset(o);
        cx.notify();
    }

    /// The pointer moved while selecting.
    pub(super) fn md_select_move(&mut self, p: Point<Pixels>, cx: &mut gpui::Context<Self>) {
        if let Some(end) = self.md_sel.end_at(p)
            && self.md_sel.head != Some(end)
        {
            self.md_sel.head = Some(end);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_parts() {
        assert_eq!(word_bounds("hello brave world", 8), (6, 11));
        assert_eq!(word_bounds("hello brave world", 17), (12, 17));
        assert_eq!(word_bounds("a  b", 1), (1, 3));
        let r = (((0, 0), 3), ((2, 1), 4));
        assert_eq!(part_of(r, (0, 0), 10), Some(3..10));
        assert_eq!(part_of(r, (1, 5), 7), Some(0..7));
        assert_eq!(part_of(r, (2, 1), 9), Some(0..4));
        assert_eq!(part_of(r, (2, 2), 9), None);
    }

    #[test]
    fn copy_joins_lines_and_blocks() {
        let s = MdSelect::default();
        {
            let mut t = s.texts.borrow_mut();
            t.insert((0, 0), "Title".into());
            t.insert((1, 0), "fn a() {".into());
            t.insert((1, 1), "}".into());
        }
        let s = MdSelect {
            anchor: Some(((0, 0), 2)),
            head: Some(((1, 1), 1)),
            ..s
        };
        assert_eq!(s.text().as_deref(), Some("tle\n\nfn a() {\n}"));
    }
}
