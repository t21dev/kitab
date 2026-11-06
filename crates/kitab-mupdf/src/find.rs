// SPDX-License-Identifier: AGPL-3.0-or-later

//! Find on one page's text (PLAN.md §3.5).
//!
//! The page's characters, in reading order, are normalised one by one into a
//! searchable stream that remembers which source character each output
//! character came from. The query goes through the same normalisation, so
//! matching is a plain substring scan and every hit maps back to quads.
//!
//! Normalisation, always: compatibility decomposition (NFKD, so ligatures like
//! "ﬁ" become "fi" and composed and decomposed accents compare equal) and every
//! run of whitespace becomes one space. With `!match_case`: lower case. With
//! `ignore_diacritics`: drop combining diacritical marks (Latin, Greek and
//! Cyrillic accents, Arabic harakat, Hebrew points). Vowel signs of Indic
//! scripts are letters, not accents, and are kept.

use kitab_core::{FindOptions, RectPt};
use unicode_normalization::char::{decompose_compatible, is_combining_mark};

/// One character of page text with its box in page points.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceChar {
    pub ch: char,
    pub rect: RectPt,
    /// Index of the line the character is on (into `PageText::lines`).
    pub line: usize,
}

/// A page's text in reading order. Lines are separated by an implicit space.
#[derive(Debug, Default, Clone)]
pub(crate) struct PageText {
    pub chars: Vec<SourceChar>,
    /// Each line's text, for hit context.
    pub lines: Vec<String>,
}

impl PageText {
    /// Whether the page has any visible text at all.
    pub fn has_text(&self) -> bool {
        self.chars.iter().any(|c| !c.ch.is_whitespace())
    }
}

/// A hit before it is turned into a `FindHit`: merged boxes and its line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PageHit {
    pub rects: Vec<RectPt>,
    pub line: usize,
}

/// Unicode blocks of combining marks that are accents, not letters.
fn is_diacritic(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F   // Combining Diacritical Marks
        | 0x0483..=0x0489 // Cyrillic combining marks
        | 0x0591..=0x05C7 // Hebrew points and cantillation
        | 0x0610..=0x061A // Arabic signs
        | 0x064B..=0x065F // Arabic harakat
        | 0x0670          // Arabic superscript alef
        | 0x06D6..=0x06ED // Arabic Quranic marks
        | 0x1AB0..=0x1AFF // Combining Diacritical Marks Extended
        | 0x1DC0..=0x1DFF // Combining Diacritical Marks Supplement
        | 0x20D0..=0x20FF // Combining Diacritical Marks for Symbols
        | 0xFE20..=0xFE2F // Combining Half Marks
    )
}

/// Push the normalised form of `c` onto `out`, tagging each output char with `src`.
fn push_normalised(c: char, src: usize, opts: FindOptions, out: &mut Vec<(char, usize)>) {
    if c.is_whitespace() {
        if out.last().is_some_and(|&(p, _)| p != ' ') {
            out.push((' ', src));
        }
        return;
    }
    decompose_compatible(c, |d| {
        if opts.ignore_diacritics && is_diacritic(d) {
            return;
        }
        if d.is_whitespace() {
            if out.last().is_some_and(|&(p, _)| p != ' ') {
                out.push((' ', src));
            }
            return;
        }
        if opts.match_case {
            out.push((d, src));
        } else {
            for l in d.to_lowercase() {
                out.push((l, src));
            }
        }
    });
}

/// Normalise a query the way page text is normalised. Leading and trailing
/// whitespace is dropped.
pub(crate) fn normalise_query(query: &str, opts: FindOptions) -> Vec<char> {
    let mut out = Vec::new();
    for c in query.chars() {
        push_normalised(c, 0, opts, &mut out);
    }
    let mut chars: Vec<char> = out.into_iter().map(|(c, _)| c).collect();
    while chars.last() == Some(&' ') {
        chars.pop();
    }
    chars
}

/// Find every non-overlapping occurrence of `query` on the page, in reading order.
pub(crate) fn find_in_page(text: &PageText, query: &str, opts: FindOptions) -> Vec<PageHit> {
    let needle = normalise_query(query, opts);
    if needle.is_empty() {
        return Vec::new();
    }
    // The searchable stream. Line breaks count as a space, except before a
    // combining mark: MuPDF puts a reordered pre-base vowel sign (Bangla "ি"
    // drawn left of its consonant) on a "line" of its own, yet it belongs to
    // the character before it.
    let mut stream: Vec<(char, usize)> = Vec::with_capacity(text.chars.len() + text.lines.len());
    let mut last_line = None;
    for (i, sc) in text.chars.iter().enumerate() {
        if last_line.is_some_and(|l| l != sc.line) && !is_combining_mark(sc.ch) {
            push_normalised(' ', i, opts, &mut stream);
        }
        last_line = Some(sc.line);
        push_normalised(sc.ch, i, opts, &mut stream);
    }

    let mut hits = Vec::new();
    let n = needle.len();
    let mut i = 0;
    while i + n <= stream.len() {
        let matches = stream[i..i + n]
            .iter()
            .zip(&needle)
            .all(|(&(c, _), &q)| c == q);
        if !matches
            || !on_char_boundaries(&stream, i, n)
            || (opts.whole_word && !is_whole_word(&stream, i, n))
        {
            i += 1;
            continue;
        }
        let first = stream[i].1;
        let last = stream[i + n - 1].1;
        hits.push(hit_for(text, first, last));
        i += n;
    }
    hits
}

/// A hit must not cut a source character in half ("cafe" is not in "café"
/// unless accents are ignored), nor stop before an accent mark.
fn on_char_boundaries(stream: &[(char, usize)], start: usize, len: usize) -> bool {
    let first_src = stream[start].1;
    let last_src = stream[start + len - 1].1;
    let split_start = start > 0 && stream[start - 1].1 == first_src && stream[start - 1].0 != ' ';
    let split_end = stream
        .get(start + len)
        .is_some_and(|&(c, src)| (src == last_src && c != ' ') || is_diacritic(c));
    !split_start && !split_end
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_whole_word(stream: &[(char, usize)], start: usize, len: usize) -> bool {
    let before = start.checked_sub(1).map(|j| stream[j].0);
    let after = stream.get(start + len).map(|&(c, _)| c);
    // Only a word character at the very edge of the hit needs a boundary.
    let head_is_word = is_word_char(stream[start].0);
    let tail_is_word = is_word_char(stream[start + len - 1].0);
    let ok_before = !head_is_word || !before.is_some_and(is_word_char);
    let ok_after = !tail_is_word || !after.is_some_and(is_word_char);
    ok_before && ok_after
}

/// Merge the boxes of source chars `first..=last` into one box per line.
fn hit_for(text: &PageText, first: usize, last: usize) -> PageHit {
    let mut rects: Vec<RectPt> = Vec::new();
    let mut current_line = None;
    for sc in &text.chars[first..=last] {
        match (current_line, rects.last_mut()) {
            (Some(l), Some(r)) if l == sc.line => *r = union(*r, sc.rect),
            _ => {
                rects.push(sc.rect);
                current_line = Some(sc.line);
            }
        }
    }
    PageHit {
        rects,
        line: text.chars[first].line,
    }
}

fn union(a: RectPt, b: RectPt) -> RectPt {
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

    fn page(lines: &[&str]) -> PageText {
        let mut t = PageText::default();
        for (li, line) in lines.iter().enumerate() {
            for (ci, ch) in line.chars().enumerate() {
                t.chars.push(SourceChar {
                    ch,
                    rect: RectPt {
                        x: ci as f32 * 10.0,
                        y: li as f32 * 20.0,
                        w: 10.0,
                        h: 12.0,
                    },
                    line: li,
                });
            }
            t.lines.push(line.to_string());
        }
        t
    }

    const CASELESS: FindOptions = FindOptions {
        match_case: false,
        whole_word: false,
        ignore_diacritics: false,
    };

    #[test]
    fn case_and_whole_word() {
        let t = page(&["The Lantern and the lanterns.", "a lantern"]);
        assert_eq!(find_in_page(&t, "lantern", CASELESS).len(), 3);
        let word = FindOptions {
            whole_word: true,
            ..CASELESS
        };
        assert_eq!(find_in_page(&t, "lantern", word).len(), 2);
        let cased = FindOptions {
            match_case: true,
            ..CASELESS
        };
        assert_eq!(find_in_page(&t, "Lantern", cased).len(), 1);
    }

    #[test]
    fn diacritics_and_ligatures() {
        let t = page(&["café naïve ﬁnal"]);
        let loose = FindOptions {
            ignore_diacritics: true,
            ..CASELESS
        };
        assert_eq!(find_in_page(&t, "cafe", loose).len(), 1);
        assert_eq!(find_in_page(&t, "cafe", CASELESS).len(), 0);
        // Composed query, composed text, exact mode.
        assert_eq!(find_in_page(&t, "café", CASELESS).len(), 1);
        // Decomposed query against composed text.
        assert_eq!(find_in_page(&t, "cafe\u{301}", CASELESS).len(), 1);
        assert_eq!(find_in_page(&t, "final", CASELESS).len(), 1);
        assert_eq!(find_in_page(&t, "NAIVE", loose).len(), 1);
    }

    #[test]
    fn spans_lines_and_merges_boxes() {
        let t = page(&["kitab-", "needle"]);
        assert!(find_in_page(&t, "kitab-needle", CASELESS).is_empty());
        let hits = find_in_page(&t, "kitab- needle", CASELESS);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rects.len(), 2);
        assert_eq!(hits[0].rects[0].w, 60.0);
    }

    #[test]
    fn bangla_vowel_signs_are_kept() {
        let t = page(&["কিতাব"]);
        let loose = FindOptions {
            ignore_diacritics: true,
            ..CASELESS
        };
        assert_eq!(find_in_page(&t, "কিতাব", loose).len(), 1);
        assert_eq!(find_in_page(&t, "কতব", loose).len(), 0);
        // MuPDF puts a pre-base vowel sign on its own line.
        let t = page(&["ক", "িতাব"]);
        assert_eq!(find_in_page(&t, "কিতাব", loose).len(), 1);
        // A suffix vowel sign does not hide the word.
        let t = page(&["কিতাবে"]);
        assert_eq!(find_in_page(&t, "কিতাব", loose).len(), 1);
    }

    #[test]
    fn empty_query_finds_nothing() {
        let t = page(&["abc"]);
        assert!(find_in_page(&t, "  ", CASELESS).is_empty());
    }
}
