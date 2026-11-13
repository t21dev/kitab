// SPDX-License-Identifier: AGPL-3.0-or-later

//! Whether a document's metadata text (title, author) reads as text.
//!
//! Many Bangla (and other Indic) PDFs were typed with legacy "ASCII fonts"
//! such as SutonnyMJ (Bijoy): the file stores Latin bytes that only look
//! like Bangla in that font. Shown in our own font, "ঢাকা" becomes "XvKv"
//! and the title turns into a run of daggers, diaereses and backticks.
//! There's no font to fix that with, so such metadata is dropped and the
//! file name used instead.

/// Symbols that real titles almost never use but legacy Indic font
/// encodings are full of (Windows-1252 and Latin-1 punctuation).
const TELLS: &[char] = &[
    '†', '‡', 'ˆ', '‰', '‹', '›', 'Œ', 'œ', 'Š', 'š', 'Ÿ', '˜', '¦', '¨', 'ª', '¬', '¯', '±', '²',
    '³', '´', 'µ', '¶', '¸', '¹', 'º', '¼', '½', '¾', '¿', '¤', '÷', '`', '~', '^', '|',
];

/// `text` trimmed, or `None` when it is empty or doesn't read as text
/// (legacy-font mojibake, replacement or control characters).
pub fn legible(text: &str) -> Option<&str> {
    let text = text.trim();
    if text.is_empty() || text.chars().any(|c| c == '\u{FFFD}' || c.is_control()) {
        return None;
    }
    let shown = text.chars().filter(|c| !c.is_whitespace()).count();
    let tells = text.chars().filter(|c| TELLS.contains(c)).count();
    // Two or more tells making up a tenth of the text: an ordinary title
    // has none, or one ("Part ½").
    (tells < 2 || tells * 10 < shown).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::legible;

    #[test]
    fn bijoy_title_is_not_legible() {
        assert_eq!(legible("XvKv (†g‡q‡`i Rb¨), iv‡R›`ªcyi"), None);
        assert_eq!(legible("evsjv‡`k"), None);
    }

    #[test]
    fn real_titles_are_legible() {
        for t in [
            "রামু অবকাঠামোগত উন্নয়ন",
            "Les Misérables — Tome I",
            "Recipes for ½ the price",
            "“Quoted” title: 2024 – 2025",
            "東京の地図",
            "C++ ~ a tour",
        ] {
            assert_eq!(legible(t), Some(t), "{t}");
        }
    }

    #[test]
    fn blank_and_broken_are_not_legible() {
        assert_eq!(legible("  "), None);
        assert_eq!(legible("Bad \u{FFFD}\u{FFFD} name"), None);
        assert_eq!(legible("  Spaced  "), Some("Spaced"));
    }
}
