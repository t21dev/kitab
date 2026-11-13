// SPDX-License-Identifier: AGPL-3.0-or-later

//! A page's text as read aloud: its characters joined into running text
//! (a word broken over two lines with a hyphen is put back together, line
//! ends become spaces) and cut into sentences, each small enough for a voice
//! to start on at once. Every character spoken knows which character of the
//! page it came from, so the word being read can be shown on the page.

use crate::TextChar;

/// A sentence of a page, or part of a long one.
#[derive(Debug, Clone, PartialEq)]
pub struct Spoken {
    pub text: String,
    /// For each `char` of `text`, its index in the page's characters.
    pub map: Vec<usize>,
}

impl Spoken {
    /// The page's characters `a..b` this sentence covers.
    pub fn span(&self) -> (usize, usize) {
        match (self.map.first(), self.map.last()) {
            (Some(&a), Some(&b)) => (a, b + 1),
            _ => (0, 0),
        }
    }
}

/// Longest sentence handed to a voice; a longer one is cut at a space.
const MAX_CHARS: usize = 280;

/// Ends a sentence when followed by a space or the end of the page.
fn ends_sentence(c: char) -> bool {
    matches!(
        c,
        '.' | '!' | '?' | '…' | '।' | '॥' | '。' | '！' | '？' | '؟' | '۔'
    )
}

/// The page's sentences from character `from` on, in reading order. Ones
/// with nothing to say (page numbers' dots, lone symbols) are left out.
pub fn sentences(chars: &[TextChar], from: usize) -> Vec<Spoken> {
    // Running text, each char tagged with its page index.
    let mut run: Vec<(char, usize)> = Vec::with_capacity(chars.len());
    // Indexes into `run` where a new sentence must start: a new paragraph
    // or a change of text size (a title, then its authors).
    let mut breaks = std::collections::HashSet::new();
    let mut line = None;
    let mut last: Option<&TextChar> = None;
    for (i, c) in chars.iter().enumerate().skip(from) {
        if line.is_some_and(|l| l != c.line) {
            if let Some(p) = last
                && !c.ch.is_whitespace()
                && new_block(p, c)
            {
                breaks.insert(run.len() + 1);
            }
            // "exam-" + "ple" on the next line: "example".
            let broken = run.len() >= 2
                && run[run.len() - 1].0 == '-'
                && run[run.len() - 2].0.is_alphabetic()
                && c.ch.is_lowercase();
            if broken {
                run.pop();
            } else if run.last().is_some_and(|&(p, _)| !p.is_whitespace()) {
                run.push((' ', i.saturating_sub(1)));
            }
        }
        line = Some(c.line);
        if !c.ch.is_whitespace() {
            last = Some(c);
        }
        if c.ch.is_whitespace() {
            if run.last().is_some_and(|&(p, _)| !p.is_whitespace()) {
                run.push((' ', i));
            }
        } else if !c.ch.is_control() {
            run.push((c.ch, i));
        }
    }

    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < run.len() {
        let end_here = (ends_sentence(run[i].0)
            && run.get(i + 1).is_none_or(|&(n, _)| {
                n.is_whitespace() || ends_sentence(n) || n == '"' || n == '\u{201D}'
            })
            && !abbreviation(&run, i))
            || breaks.contains(&(i + 1));
        let too_long = i + 1 - start >= MAX_CHARS;
        if end_here || too_long || i + 1 == run.len() {
            let mut cut = i + 1;
            if too_long && !end_here {
                // Back to the last space, unless there is none.
                if let Some(sp) = run[start..=i].iter().rposition(|&(c, _)| c == ' ')
                    && sp > 0
                {
                    cut = start + sp;
                }
            }
            // Closing quotes and brackets stay with their sentence.
            while cut < run.len() && matches!(run[cut].0, '"' | '\u{201D}' | '\u{2019}' | ')' | ']')
            {
                cut += 1;
            }
            push(&run[start..cut], &mut out);
            start = cut;
            i = cut;
            continue;
        }
        i += 1;
    }
    out
}

/// The next line starts a new block: a gap above it (a new paragraph) or
/// text of another size. A jump up (the next column) does not.
fn new_block(prev: &TextChar, next: &TextChar) -> bool {
    let (a, b) = (prev.rect.h.max(0.1), next.rect.h.max(0.1));
    let gap = next.rect.y - (prev.rect.y + prev.rect.h);
    gap > 0.8 * a.min(b) || a.max(b) / a.min(b) > 1.25
}

/// The full stop at `i` belongs to an initial ("R.") or an abbreviation
/// followed by more of the sentence ("e.g. the", "et al. show").
fn abbreviation(run: &[(char, usize)], i: usize) -> bool {
    if run[i].0 != '.' {
        return false;
    }
    let initial =
        i >= 1 && run[i - 1].0.is_uppercase() && (i == 1 || !run[i - 2].0.is_alphanumeric());
    let goes_on = run[i + 1..]
        .iter()
        .find(|&&(c, _)| !c.is_whitespace())
        .is_some_and(|&(c, _)| c.is_lowercase());
    initial || goes_on
}

fn push(part: &[(char, usize)], out: &mut Vec<Spoken>) {
    let a = part.iter().position(|&(c, _)| !c.is_whitespace());
    let b = part.iter().rposition(|&(c, _)| !c.is_whitespace());
    let (Some(a), Some(b)) = (a, b) else { return };
    let part = &part[a..=b];
    if !part.iter().any(|&(c, _)| c.is_alphanumeric()) {
        return;
    }
    out.push(Spoken {
        text: part.iter().map(|&(c, _)| c).collect(),
        map: part.iter().map(|&(_, i)| i).collect(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RectPt;

    fn page(lines: &[&str]) -> Vec<TextChar> {
        let mut v = Vec::new();
        for (l, text) in lines.iter().enumerate() {
            for (x, ch) in text.chars().enumerate() {
                v.push(TextChar {
                    ch,
                    rect: RectPt {
                        x: x as f32 * 6.0,
                        y: l as f32 * 12.0,
                        w: 6.0,
                        h: 12.0,
                    },
                    line: l as u32,
                });
            }
        }
        v
    }

    fn texts(chars: &[TextChar], from: usize) -> Vec<String> {
        sentences(chars, from).into_iter().map(|s| s.text).collect()
    }

    #[test]
    fn lines_join_and_sentences_split() {
        let p = page(&["The first one. And the", "second one! A third?"]);
        assert_eq!(
            texts(&p, 0),
            ["The first one.", "And the second one!", "A third?"]
        );
    }

    #[test]
    fn hyphenated_words_are_put_back() {
        let p = page(&["a well-known exam-", "ple of text. Self-", "Made stays."]);
        assert_eq!(
            texts(&p, 0),
            ["a well-known example of text.", "Self- Made stays."]
        );
    }

    #[test]
    fn numbers_and_bangla_dari() {
        let p = page(&["It costs 3.50 today. রামু উন্নয়ন। শেষ"]);
        assert_eq!(texts(&p, 0), ["It costs 3.50 today.", "রামু উন্নয়ন।", "শেষ"]);
    }

    #[test]
    fn map_points_at_page_characters() {
        let p = page(&["ab. Cd", "ef."]);
        let s = sentences(&p, 0);
        assert_eq!(s[1].text, "Cd ef.");
        // "cd" are 4 and 5, the line break maps to "d", "ef." to 6..9.
        assert_eq!(s[1].map, vec![4, 5, 5, 6, 7, 8]);
        assert_eq!(s[1].span(), (4, 9));
    }

    #[test]
    fn starts_mid_page_and_skips_empty_bits() {
        let p = page(&["One. Two. . . 7", "Eight."]);
        assert_eq!(texts(&p, 5), ["Two.", "7 Eight."]);
    }

    #[test]
    fn initials_and_abbreviations_dont_end_sentences() {
        let p = page(&["Written by Mohammad R. Haghighat, e.g. here. Next one."]);
        assert_eq!(
            texts(&p, 0),
            ["Written by Mohammad R. Haghighat, e.g. here.", "Next one."]
        );
    }

    #[test]
    fn a_title_and_a_new_paragraph_are_their_own_sentences() {
        // A 24 pt title, 12 pt authors, then a paragraph after a gap.
        let mut v = Vec::new();
        let mut put = |text: &str, line: u32, y: f32, h: f32| {
            for (x, ch) in text.chars().enumerate() {
                v.push(TextChar {
                    ch,
                    rect: RectPt {
                        x: x as f32 * 6.0,
                        y,
                        w: 6.0,
                        h,
                    },
                    line,
                });
            }
        };
        put("Big Title", 0, 0.0, 24.0);
        put("Ann Author", 1, 26.0, 12.0);
        put("First para", 2, 40.0, 12.0);
        put("goes on", 3, 52.0, 12.0);
        put("Second para", 4, 80.0, 12.0);
        assert_eq!(
            texts(&v, 0),
            ["Big Title", "Ann Author First para goes on", "Second para"]
        );
    }

    #[test]
    fn long_runs_are_cut_at_a_space() {
        let words = "word ".repeat(100);
        let p = page(&[words.trim_end()]);
        let s = texts(&p, 0);
        assert!(s.len() >= 2);
        assert!(s.iter().all(|t| t.chars().count() <= MAX_CHARS));
        assert!(
            s.iter()
                .all(|t| t.starts_with("word") && t.ends_with("word"))
        );
    }
}
