// SPDX-License-Identifier: AGPL-3.0-or-later

//! Footnotes as popovers (PLAN.md Phase 4, MANIFEST `epub/03`): whether a link
//! is a note mark, and the note's text cut from the page it points at.

use crate::{RectPt, TextChar};

/// The characters whose centres fall inside `rect`: a link's own text.
pub fn text_in(chars: &[TextChar], rect: RectPt) -> String {
    chars
        .iter()
        .filter(|c| {
            let (cx, cy) = (c.rect.x + c.rect.w / 2.0, c.rect.y + c.rect.h / 2.0);
            cx >= rect.x && cx <= rect.x + rect.w && cy >= rect.y && cy <= rect.y + rect.h
        })
        .map(|c| c.ch)
        .collect()
}

/// A link whose own text is a note mark: a number, a letter, `*`, `†`, `‡`,
/// `§`, a short roman numeral, optionally in brackets.
pub fn is_note_mark(text: &str) -> bool {
    let t = text
        .trim()
        .trim_matches(|c: char| matches!(c, '[' | ']' | '(' | ')' | '{' | '}'))
        .trim();
    if t.is_empty() || t.chars().count() > 4 {
        return false;
    }
    let all = |f: fn(char) -> bool| t.chars().all(f);
    all(|c| c.is_ascii_digit())
        || all(|c| matches!(c, '*' | '†' | '‡' | '§' | '¶'))
        || all(|c| "ivxlcIVXLC".contains(c))
        || (t.chars().count() == 1 && t.chars().all(char::is_alphabetic))
        // Superscript digits, as some books write them.
        || all(|c| "⁰¹²³⁴⁵⁶⁷⁸⁹".contains(c))
}

/// The text of the note that starts at `y` (points from the page's top) on a
/// page with these characters: its lines up to a blank gap, the next note's
/// mark, or `max` characters. `None` for `y` starts at the top. With the
/// link's `mark` ("1"), a line opening with that mark near `y` is where the
/// note starts: link targets often land a line or two above their note.
pub fn note_text(chars: &[TextChar], y: Option<f32>, mark: Option<&str>, max: usize) -> String {
    // The page's lines, in reading order, with their top, bottom and text.
    let mut lines: Vec<(f32, f32, String)> = Vec::new();
    let mut current: Option<u32> = None;
    for c in chars {
        if current != Some(c.line) {
            current = Some(c.line);
            lines.push((c.rect.y, c.rect.y + c.rect.h, String::new()));
        }
        if let Some(l) = lines.last_mut() {
            l.0 = l.0.min(c.rect.y);
            l.1 = l.1.max(c.rect.y + c.rect.h);
            l.2.push(c.ch);
        }
    }
    let from = y.unwrap_or(f32::MIN);
    let Some(mut start) = lines.iter().position(|l| l.1 > from + 1.0) else {
        return String::new();
    };
    // The note's own line: within a few lines of the target, opening with
    // the mark ("1.", "1 ", "[1]", "¹").
    if let Some(mark) = mark.map(|m| {
        m.trim()
            .trim_matches(|c: char| matches!(c, '[' | ']' | '(' | ')'))
            .to_string()
    }) {
        let opens = |t: &str| {
            let t = t.trim().trim_start_matches(['[', '(']);
            t.strip_prefix(mark.as_str())
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric()))
        };
        if let Some(found) = lines[start..].iter().take(4).position(|l| opens(&l.2)) {
            start += found;
        }
    }
    let mut out = String::new();
    let mut last_bottom: Option<f32> = None;
    for (i, (top, bottom, text)) in lines[start..].iter().enumerate() {
        let text = text.trim();
        if let Some(prev) = last_bottom {
            let height = (bottom - top).max(1.0);
            // A gap taller than a line ends the note, and so does the next
            // note's own mark at the start of a line.
            if top - prev > height * 0.9 || (i > 0 && starts_with_mark(text)) {
                break;
            }
        }
        if text.is_empty() {
            continue;
        }
        if !out.is_empty() {
            // A word broken across lines joins without a space.
            if out.ends_with('-') {
                out.pop();
            } else {
                out.push(' ');
            }
        }
        out.push_str(text);
        last_bottom = Some(*bottom);
        if out.chars().count() >= max {
            let cut: String = out.chars().take(max).collect();
            return format!("{}…", cut.trim_end());
        }
    }
    out
}

/// "2. The second note" or "[3] Text": a line that opens with a note mark.
fn starts_with_mark(line: &str) -> bool {
    let head: String = line
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '.')
        .collect();
    let dotted = line[head.len()..].starts_with('.') || head.ends_with(']');
    dotted && is_note_mark(&head)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines of text 14 pt apart, `gap` extra points before line `gap_at`.
    fn page(lines: &[&str], gap_at: usize, gap: f32) -> Vec<TextChar> {
        let mut out = Vec::new();
        let mut y = 100.0;
        for (n, l) in lines.iter().enumerate() {
            if n == gap_at {
                y += gap;
            }
            for (i, ch) in l.chars().enumerate() {
                out.push(TextChar {
                    ch,
                    rect: RectPt {
                        x: 50.0 + i as f32 * 6.0,
                        y,
                        w: 6.0,
                        h: 12.0,
                    },
                    line: n as u32,
                });
            }
            y += 14.0;
        }
        out
    }

    #[test]
    fn note_marks() {
        for yes in ["1", "12", "[3]", "(4)", "*", "†", "iv", "a", "²"] {
            assert!(is_note_mark(yes), "{yes}");
        }
        for no in ["", "Chapter 2", "see here", "12345", "page"] {
            assert!(!is_note_mark(no), "{no}");
        }
    }

    #[test]
    fn a_note_runs_to_the_next_mark_or_a_gap() {
        let chars = page(
            &[
                "Body text above the notes.",
                "1. The first footnote, shown",
                "as a popover.",
                "2. The second footnote.",
            ],
            9,
            0.0,
        );
        // y at the first note's line (100 + 14).
        assert_eq!(
            note_text(&chars, Some(114.0), None, 500),
            "1. The first footnote, shown as a popover."
        );
        assert_eq!(
            note_text(&chars, Some(142.0), None, 500),
            "2. The second footnote."
        );

        let gapped = page(&["Note one, line one", "line two", "Next section"], 2, 20.0);
        assert_eq!(
            note_text(&gapped, None, None, 500),
            "Note one, line one line two"
        );
    }

    #[test]
    fn long_notes_are_cut_and_hyphens_join() {
        let chars = page(&["A hyphen-", "ated word and more words"], 9, 0.0);
        assert_eq!(
            note_text(&chars, None, None, 500),
            "A hyphenated word and more words"
        );
        assert_eq!(note_text(&chars, None, None, 8), "A hyphen…");
        assert_eq!(note_text(&chars, Some(900.0), None, 500), "");
    }
}
