// SPDX-License-Identifier: AGPL-3.0-or-later

//! Text selection on fixed-layout pages: drag to select (across pages too),
//! double-click a word, triple-click a line, Ctrl+C or the page menu copies.
//! A selection end is a character boundary on a page: `(page, index)` where
//! index is 0..=number of characters. Selection never animates (DESIGN.md §
//! Motion).

use super::Reader;
use super::geom::{rect_to_view, view_to_point};
use crate::style::Style;
use gpui::{
    AnyElement, AppContext, Context, IntoElement, MouseDownEvent, Pixels, Point, Styled, Window,
    div, px,
};
use kitab_core::{RectPt, TextChar};

/// Pages a copy may span (a drag from page 1 to 900 copies the first 300).
const MAX_COPY_PAGES: u32 = 300;

pub(super) type End = (u32, usize);

#[derive(Default)]
pub(super) struct SelectState {
    pub anchor: Option<End>,
    pub head: Option<End>,
    pub dragging: bool,
}

impl SelectState {
    /// (start, end) in document order, or `None` when nothing is selected.
    pub fn range(&self) -> Option<(End, End)> {
        let (a, h) = (self.anchor?, self.head?);
        if a == h {
            return None;
        }
        Some(if a <= h { (a, h) } else { (h, a) })
    }
}

/// The boundary nearest to (`x`, `y`) in page points: the line whose box is
/// closest vertically, then the gap between characters closest horizontally.
pub(super) fn boundary_at(chars: &[TextChar], x: f32, y: f32) -> usize {
    if chars.is_empty() {
        return 0;
    }
    // Lines as runs of characters, with their vertical extent.
    let mut best: Option<(f32, usize, usize)> = None; // (distance, start, end)
    let mut start = 0;
    while start < chars.len() {
        let line = chars[start].line;
        let mut end = start;
        let (mut top, mut bottom) = (f32::MAX, f32::MIN);
        while end < chars.len() && chars[end].line == line {
            top = top.min(chars[end].rect.y);
            bottom = bottom.max(chars[end].rect.y + chars[end].rect.h);
            end += 1;
        }
        let d = if y < top {
            top - y
        } else if y > bottom {
            y - bottom
        } else {
            0.0
        };
        if best.is_none_or(|(bd, ..)| d < bd) {
            best = Some((d, start, end));
        }
        start = end;
    }
    let Some((_, start, end)) = best else {
        return 0;
    };
    for (i, c) in chars[start..end].iter().enumerate() {
        if x < c.rect.x + c.rect.w / 2.0 {
            return start + i;
        }
    }
    end
}

/// The word around boundary `i` (letters, digits and joiners).
pub(super) fn word_at(chars: &[TextChar], i: usize) -> (usize, usize) {
    let is_word = |c: &TextChar| c.ch.is_alphanumeric() || matches!(c.ch, '\'' | '’' | '-' | '_');
    let i = i.min(chars.len().saturating_sub(1));
    if chars.get(i).is_none_or(|c| !is_word(c)) {
        return (i, (i + 1).min(chars.len()));
    }
    let mut a = i;
    while a > 0 && is_word(&chars[a - 1]) && chars[a - 1].line == chars[i].line {
        a -= 1;
    }
    let mut b = i;
    while b < chars.len() && is_word(&chars[b]) && chars[b].line == chars[i].line {
        b += 1;
    }
    (a, b)
}

/// The line around boundary `i`.
pub(super) fn line_at(chars: &[TextChar], i: usize) -> (usize, usize) {
    let i = i.min(chars.len().saturating_sub(1));
    let Some(line) = chars.get(i).map(|c| c.line) else {
        return (0, 0);
    };
    let a = chars[..i]
        .iter()
        .rposition(|c| c.line != line)
        .map_or(0, |p| p + 1);
    let b = chars[i..]
        .iter()
        .position(|c| c.line != line)
        .map_or(chars.len(), |p| i + p);
    (a, b)
}

/// Characters `a..b` as text, a newline where the line changes.
pub(super) fn text_of(chars: &[TextChar], a: usize, b: usize, out: &mut String) {
    let mut line = None;
    for c in &chars[a.min(chars.len())..b.min(chars.len())] {
        if line.is_some_and(|l| l != c.line) {
            // Keep a hyphen that ends a line, drop trailing spaces.
            while out.ends_with(' ') {
                out.pop();
            }
            out.push('\n');
        }
        line = Some(c.line);
        out.push(c.ch);
    }
}

impl Reader {
    pub fn has_selection(&self) -> bool {
        if self.md.is_some() {
            return self.md_sel.range().is_some();
        }
        self.select.range().is_some()
    }

    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if self.md_sel.anchor.take().is_some() {
            self.md_sel.head = None;
            cx.notify();
        }
        if self.select.anchor.take().is_some() {
            self.select.head = None;
            cx.notify();
        }
    }

    /// The page and boundary under window point `p`, if the page's characters are in.
    pub(super) fn end_at(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) -> Option<End> {
        let top = self.list.logical_scroll_top().item_ix;
        let last = self.page_count().saturating_sub(1);
        // The visible page whose sheet is nearest vertically.
        let mut best: Option<(f32, usize, gpui::Bounds<Pixels>)> = None;
        for ix in top..=(top + 8).min(last) {
            let Some(b) = self.sheet_bounds(ix) else {
                continue;
            };
            let (y, t, bt) = (f32::from(p.y), f32::from(b.top()), f32::from(b.bottom()));
            let (x, l, r) = (f32::from(p.x), f32::from(b.left()), f32::from(b.right()));
            let off = |v: f32, lo: f32, hi: f32| {
                if v < lo {
                    lo - v
                } else if v > hi {
                    v - hi
                } else {
                    0.0
                }
            };
            // Pages side by side are level: the one across settles it.
            let d = off(y, t, bt) + off(x, l, r);
            if best.is_none_or(|(bd, ..)| d < bd) {
                best = Some((d, ix, b));
            }
        }
        let (_, ix, b) = best?;
        let page = ix as u32;
        let Some(chars) = self.layers.chars.get(&page).cloned() else {
            self.ensure_chars(page, cx);
            return None;
        };
        let (x, y) = view_to_point(
            f32::from(p.x - b.origin.x),
            f32::from(p.y - b.origin.y),
            self.raw_page_size(ix),
            self.rotation,
            self.px_per_pt(ix),
        );
        Some((page, boundary_at(&chars, x, y)))
    }

    /// Left button down on page `ix`: start a selection (or a word or line).
    pub(super) fn select_down(
        &mut self,
        ix: usize,
        e: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus);
        self.ensure_chars(ix as u32, cx);
        let Some(end) = self.end_at(e.position, cx) else {
            self.clear_selection(cx);
            return;
        };
        if e.modifiers.shift && self.select.anchor.is_some() {
            self.select.head = Some(end);
            self.select.dragging = true;
            cx.notify();
            return;
        }
        let chars = self.layers.chars.get(&end.0).cloned().unwrap_or_default();
        let span = match e.click_count {
            2 => Some(word_at(&chars, end.1)),
            n if n >= 3 => Some(line_at(&chars, end.1)),
            _ => None,
        };
        match span {
            Some((a, b)) => {
                self.select.anchor = Some((end.0, a));
                self.select.head = Some((end.0, b));
                self.select.dragging = false;
            }
            None => {
                self.select.anchor = Some(end);
                self.select.head = Some(end);
                self.select.dragging = true;
            }
        }
        cx.notify();
    }

    /// The pointer moved with the button down: extend, and scroll near the edges.
    pub(super) fn select_move(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        if !self.select.dragging {
            return;
        }
        let vb = self.list.viewport_bounds();
        if p.y < vb.top() + px(24.0) {
            self.list.scroll_by(px(-24.0));
        } else if p.y > vb.bottom() - px(24.0) {
            self.list.scroll_by(px(24.0));
        }
        if let Some(end) = self.end_at(p, cx)
            && self.select.head != Some(end)
        {
            self.select.head = Some(end);
            cx.notify();
        }
    }

    pub(super) fn select_up(&mut self) {
        self.select.dragging = false;
    }

    /// The selected text, from pages whose characters are in (all of them,
    /// after a drag over each).
    fn selected_text_cached(&self) -> Option<String> {
        if self.md.is_some() {
            return self.md_sel.text();
        }
        let ((p0, i0), (p1, i1)) = self.select.range()?;
        let mut out = String::new();
        for page in p0..=p1.min(p0 + MAX_COPY_PAGES) {
            let chars = self.layers.chars.get(&page)?;
            let a = if page == p0 { i0 } else { 0 };
            let b = if page == p1 { i1 } else { chars.len() };
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            text_of(chars, a, b, &mut out);
        }
        Some(out.trim().to_string()).filter(|t| !t.is_empty())
    }

    /// Ctrl+C: copy the selection. Pages the drag skipped over are read off
    /// the UI thread first. Not allowed when the document forbids copying.
    pub fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if !self.may_copy() {
            return;
        }
        if let Some(text) = self.selected_text_cached() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            return;
        }
        if self.md.is_some() {
            return;
        }
        let (Some(((p0, i0), (p1, i1))), Some(doc)) = (self.select.range(), self.doc) else {
            return;
        };
        let engine = self.engine.clone();
        cx.spawn(async move |_, cx| {
            let text = cx
                .background_spawn(async move {
                    let mut out = String::new();
                    for page in p0..=p1.min(p0 + MAX_COPY_PAGES) {
                        let chars = engine.page_chars(doc, page);
                        let a = if page == p0 { i0 } else { 0 };
                        let b = if page == p1 { i1 } else { chars.len() };
                        if !out.is_empty() && !out.ends_with('\n') {
                            out.push('\n');
                        }
                        text_of(&chars, a, b, &mut out);
                    }
                    out.trim().to_string()
                })
                .await;
            if !text.is_empty() {
                let _ =
                    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)));
            }
        })
        .detach();
    }

    /// Whether the document's owner allows copying text (PLAN.md §3.12).
    pub fn may_copy(&self) -> bool {
        self.info.as_ref().is_none_or(|i| i.permissions.copy)
    }

    /// "Find" for the selection: put the selected words in the find bar.
    pub fn find_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.selected_text_cached() else {
            return;
        };
        let query: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        self.find.query = query.chars().take(256).collect();
        self.open_find(window, cx);
        self.find.all_selected = true;
        self.search(cx);
    }

    /// Characters selected (0 with nothing selected, or text not read yet).
    pub fn selected_text_len(&self) -> usize {
        self.selected_text_cached().map_or(0, |t| t.chars().count())
    }

    /// A short preview of the selection for menus ("Find “lantern”").
    pub fn selection_preview(&self) -> Option<String> {
        let text = self.selected_text_cached()?;
        let one: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        Some(if one.chars().count() > 24 {
            format!("{}…", one.chars().take(23).collect::<String>().trim_end())
        } else {
            one
        })
    }

    /// Selection boxes on page `ix`, one per line run.
    pub(super) fn selection_marks(&self, ix: usize, ppt: f32, st: &Style) -> Vec<AnyElement> {
        let Some(((p0, i0), (p1, i1))) = self.select.range() else {
            return Vec::new();
        };
        let page = ix as u32;
        if page < p0 || page > p1 {
            return Vec::new();
        }
        let Some(chars) = self.layers.chars.get(&page) else {
            return Vec::new();
        };
        let a = if page == p0 { i0 } else { 0 };
        let b = if page == p1 { i1 } else { chars.len() };
        let size = self.raw_page_size(ix);
        let color = st.c(|c| c.accent).opacity(0.3);
        let mut out = Vec::new();
        let mut run: Option<(u32, RectPt)> = None;
        let flush = |r: RectPt, out: &mut Vec<AnyElement>| {
            let v = rect_to_view(r, size, self.rotation, ppt);
            out.push(
                div()
                    .absolute()
                    .left(px(v.x))
                    .top(px(v.y))
                    .w(px(v.w))
                    .h(px(v.h))
                    .bg(color)
                    .into_any_element(),
            );
        };
        for c in &chars[a.min(chars.len())..b.min(chars.len())] {
            run = match run {
                Some((line, r)) if line == c.line => Some((line, union(r, c.rect))),
                Some((_, r)) => {
                    flush(r, &mut out);
                    Some((c.line, c.rect))
                }
                None => Some((c.line, c.rect)),
            };
        }
        if let Some((_, r)) = run {
            flush(r, &mut out);
        }
        out
    }
}

pub(super) fn union(a: RectPt, b: RectPt) -> RectPt {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    RectPt {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "ab cd" on line 0 and "ef" on line 1, 10 pt wide characters.
    fn sample() -> Vec<TextChar> {
        let mut v = Vec::new();
        for (i, ch) in "ab cd".chars().enumerate() {
            v.push(TextChar {
                ch,
                rect: RectPt {
                    x: i as f32 * 10.0,
                    y: 0.0,
                    w: 10.0,
                    h: 12.0,
                },
                line: 0,
            });
        }
        for (i, ch) in "ef".chars().enumerate() {
            v.push(TextChar {
                ch,
                rect: RectPt {
                    x: i as f32 * 10.0,
                    y: 20.0,
                    w: 10.0,
                    h: 12.0,
                },
                line: 1,
            });
        }
        v
    }

    #[test]
    fn boundary_picks_the_nearest_gap() {
        let c = sample();
        assert_eq!(boundary_at(&c, 2.0, 5.0), 0);
        assert_eq!(boundary_at(&c, 8.0, 5.0), 1);
        assert_eq!(boundary_at(&c, 500.0, 5.0), 5, "past the end of line 0");
        assert_eq!(boundary_at(&c, 12.0, 25.0), 6, "on line 1");
        assert_eq!(
            boundary_at(&c, 0.0, 300.0),
            5,
            "below everything: line 1, start"
        );
    }

    #[test]
    fn words_and_lines() {
        let c = sample();
        assert_eq!(word_at(&c, 4), (3, 5));
        assert_eq!(word_at(&c, 0), (0, 2));
        assert_eq!(line_at(&c, 1), (0, 5));
        assert_eq!(line_at(&c, 6), (5, 7));
    }

    #[test]
    fn text_breaks_lines() {
        let c = sample();
        let mut s = String::new();
        text_of(&c, 3, 7, &mut s);
        assert_eq!(s, "cd\nef");
    }
}
