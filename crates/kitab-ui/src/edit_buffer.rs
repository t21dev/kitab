// SPDX-License-Identifier: AGPL-3.0-or-later

//! The text behind the Markdown editor: a string, a selection, line starts,
//! and undo/redo. No GPUI here, so it is tested on its own. Offsets are byte
//! offsets on character boundaries; the IME speaks UTF-16, so conversions live
//! here too.

use std::ops::Range;

/// Undo steps kept (each is a full snapshot; Markdown files are small).
const UNDO_MAX: usize = 200;

#[derive(Clone)]
struct Snapshot {
    text: String,
    anchor: usize,
    head: usize,
}

pub struct EditBuffer {
    text: String,
    /// Where the selection started; equals `head` when nothing is selected.
    pub anchor: usize,
    /// The caret.
    pub head: usize,
    /// Byte offset of each line's start.
    lines: Vec<usize>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Typing into the same run merges into one undo step.
    typing: bool,
    /// The column up/down movement tries to keep, in characters.
    goal_col: Option<usize>,
    /// IME composition in progress.
    pub marked: Option<Range<usize>>,
    /// Text differs from what was loaded or last saved.
    pub dirty: bool,
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    v
}

impl EditBuffer {
    pub fn new(text: String) -> Self {
        let lines = line_starts(&text);
        Self {
            text,
            anchor: 0,
            head: 0,
            lines,
            undo: Vec::new(),
            redo: Vec::new(),
            typing: false,
            goal_col: None,
            marked: None,
            dirty: false,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Line `ix` without its newline.
    pub fn line(&self, ix: usize) -> &str {
        let range = self.line_range(ix);
        &self.text[range]
    }

    /// Byte range of line `ix`, without its newline.
    pub fn line_range(&self, ix: usize) -> Range<usize> {
        let start = self.lines.get(ix).copied().unwrap_or(self.text.len());
        let end = self
            .lines
            .get(ix + 1)
            .map_or(self.text.len(), |n| n.saturating_sub(1));
        start..end.max(start)
    }

    /// The line an offset is on.
    pub fn line_of(&self, offset: usize) -> usize {
        match self.lines.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub fn has_selection(&self) -> bool {
        self.anchor != self.head
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection()]
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            anchor: self.anchor,
            head: self.head,
        }
    }

    fn remember(&mut self, typing: bool) {
        if !(typing && self.typing) {
            self.undo.push(self.snapshot());
            if self.undo.len() > UNDO_MAX {
                self.undo.remove(0);
            }
        }
        self.typing = typing;
        self.redo.clear();
    }

    fn clamp(&self, offset: usize) -> usize {
        let mut o = offset.min(self.text.len());
        while !self.text.is_char_boundary(o) {
            o -= 1;
        }
        o
    }

    /// Replace `range` (or the selection) with `with`; the caret lands after it.
    pub fn replace(&mut self, range: Option<Range<usize>>, with: &str, typing: bool) {
        let range = range.unwrap_or_else(|| self.selection());
        let (start, end) = (
            self.clamp(range.start),
            self.clamp(range.end.max(range.start)),
        );
        // A single typed character continues a typing run; anything else is its own step.
        let one_char = typing && with.chars().count() == 1 && start == end && !with.contains('\n');
        self.remember(one_char);
        self.text.replace_range(start..end, with);
        self.lines = line_starts(&self.text);
        self.head = start + with.len();
        self.anchor = self.head;
        self.goal_col = None;
        self.dirty = true;
    }

    pub fn insert(&mut self, s: &str) {
        self.replace(None, s, true);
    }

    pub fn backspace(&mut self, word: bool) {
        if !self.has_selection() {
            let to = if word {
                self.word_left(self.head)
            } else {
                self.prev_char(self.head)
            };
            self.anchor = to;
        }
        if self.has_selection() {
            self.replace(None, "", false);
        }
    }

    pub fn delete(&mut self, word: bool) {
        if !self.has_selection() {
            self.head = if word {
                self.word_right(self.head)
            } else {
                self.next_char(self.head)
            };
        }
        if self.has_selection() {
            self.replace(None, "", false);
        }
    }

    fn prev_char(&self, o: usize) -> usize {
        self.text[..o]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next_char(&self, o: usize) -> usize {
        self.text[o..]
            .chars()
            .next()
            .map_or(o, |c| o + c.len_utf8())
    }

    fn word_left(&self, o: usize) -> usize {
        let before = &self.text[..o];
        let trimmed = before.trim_end_matches(|c: char| !c.is_alphanumeric());
        let word_start = trimmed
            .char_indices()
            .rev()
            .find(|(_, c)| !c.is_alphanumeric())
            .map_or(0, |(i, c)| i + c.len_utf8());
        word_start.min(o)
    }

    fn word_right(&self, o: usize) -> usize {
        let after = &self.text[o..];
        let skip = after.len()
            - after
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .len();
        let rest = &after[skip..];
        let word = rest
            .find(|c: char| !c.is_alphanumeric())
            .unwrap_or(rest.len());
        o + skip + word
    }

    fn set_head(&mut self, to: usize, select: bool) {
        self.head = self.clamp(to);
        if !select {
            self.anchor = self.head;
        }
        self.typing = false;
    }

    pub fn left(&mut self, word: bool, select: bool) {
        let to = if !select && self.has_selection() {
            self.selection().start
        } else if word {
            self.word_left(self.head)
        } else {
            self.prev_char(self.head)
        };
        self.set_head(to, select);
        self.goal_col = None;
    }

    pub fn right(&mut self, word: bool, select: bool) {
        let to = if !select && self.has_selection() {
            self.selection().end
        } else if word {
            self.word_right(self.head)
        } else {
            self.next_char(self.head)
        };
        self.set_head(to, select);
        self.goal_col = None;
    }

    /// Characters from the line start to `offset`.
    fn col(&self, offset: usize) -> usize {
        let line = self.line_of(offset);
        self.text[self.lines[line]..offset].chars().count()
    }

    /// The offset `col` characters into line `line` (or its end).
    pub fn offset_at(&self, line: usize, col: usize) -> usize {
        let r = self.line_range(line.min(self.line_count() - 1));
        let s = &self.text[r.clone()];
        r.start + s.char_indices().nth(col).map_or(s.len(), |(i, _)| i)
    }

    pub fn vertical(&mut self, lines: isize, select: bool) {
        let col = match self.goal_col {
            Some(c) => c,
            None => self.col(self.head),
        };
        self.goal_col = Some(col);
        let line = self.line_of(self.head) as isize + lines;
        let to = if line < 0 {
            0
        } else if line as usize >= self.line_count() {
            self.text.len()
        } else {
            self.offset_at(line as usize, col)
        };
        let goal = self.goal_col;
        self.set_head(to, select);
        self.goal_col = goal;
    }

    pub fn home(&mut self, doc: bool, select: bool) {
        let to = if doc {
            0
        } else {
            let start = self.lines[self.line_of(self.head)];
            // First press goes to the indentation, the second to column 0.
            let indent = start
                + self.text[start..]
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .map(char::len_utf8)
                    .sum::<usize>();
            if self.head == indent { start } else { indent }
        };
        self.set_head(to, select);
        self.goal_col = None;
    }

    pub fn end(&mut self, doc: bool, select: bool) {
        let to = if doc {
            self.text.len()
        } else {
            self.line_range(self.line_of(self.head)).end
        };
        self.set_head(to, select);
        self.goal_col = None;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.head = self.text.len();
    }

    /// Put the caret at `offset` (a click), or extend the selection (shift-click, drag).
    pub fn click(&mut self, offset: usize, select: bool) {
        self.set_head(offset, select);
        self.goal_col = None;
    }

    /// Select the word around `offset` (double-click).
    pub fn select_word(&mut self, offset: usize) {
        let o = self.clamp(offset);
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let start = self.text[..o]
            .char_indices()
            .rev()
            .find(|(_, c)| !is_word(*c))
            .map_or(0, |(i, c)| i + c.len_utf8());
        let end = self.text[o..]
            .find(|c: char| !is_word(c))
            .map_or(self.text.len(), |i| o + i);
        self.anchor = start;
        self.head = end;
    }

    pub fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(s);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(s);
        true
    }

    fn restore(&mut self, s: Snapshot) {
        self.text = s.text;
        self.lines = line_starts(&self.text);
        self.anchor = s.anchor.min(self.text.len());
        self.head = s.head.min(self.text.len());
        self.typing = false;
        self.dirty = true;
    }

    // ── UTF-16 for the IME ──

    pub fn to_utf16(&self, offset: usize) -> usize {
        self.text[..self.clamp(offset)].encode_utf16().count()
    }

    pub fn from_utf16(&self, utf16: usize) -> usize {
        let mut count = 0;
        for (i, c) in self.text.char_indices() {
            if count >= utf16 {
                return i;
            }
            count += c.len_utf16();
        }
        self.text.len()
    }

    pub fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.from_utf16(r.start)..self.from_utf16(r.end)
    }

    pub fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.to_utf16(r.start)..self.to_utf16(r.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_merges_into_one_undo_step() {
        let mut b = EditBuffer::new(String::new());
        for c in ["h", "i", " ", "x"] {
            b.insert(c);
        }
        assert_eq!(b.text(), "hi x");
        assert!(b.undo());
        assert_eq!(b.text(), "");
        assert!(b.redo());
        assert_eq!(b.text(), "hi x");
    }

    #[test]
    fn lines_and_vertical_movement_keep_the_column() {
        let mut b = EditBuffer::new("abcdef\nab\nabcdef".into());
        assert_eq!(b.line_count(), 3);
        b.click(5, false);
        b.vertical(1, false);
        assert_eq!(b.head, 9, "end of the short line");
        b.vertical(1, false);
        assert_eq!(b.head, 10 + 5, "back to column 5");
    }

    #[test]
    fn words_selection_and_unicode() {
        let mut b = EditBuffer::new("কিতাব reader".into());
        b.end(false, false);
        b.backspace(true);
        assert_eq!(b.text(), "কিতাব ");
        b.home(false, true);
        assert_eq!(b.selected_text(), "কিতাব ");
        b.insert("x");
        assert_eq!(b.text(), "x");
        // UTF-16 round trip with a character outside the BMP.
        let b = EditBuffer::new("a😀b".into());
        assert_eq!(b.to_utf16(5), 3);
        assert_eq!(b.from_utf16(3), 5);
    }

    #[test]
    fn home_toggles_between_indent_and_column_zero() {
        let mut b = EditBuffer::new("    code".into());
        b.end(false, false);
        b.home(false, false);
        assert_eq!(b.head, 4);
        b.home(false, false);
        assert_eq!(b.head, 0);
    }
}
