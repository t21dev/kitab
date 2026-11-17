// SPDX-License-Identifier: AGPL-3.0-or-later

//! A PDF's own page labels (the catalog's `/PageLabels` number tree, PDF
//! 32000-1 §12.4.2): "i", "ii", "1", "A-1"… Read through MuPDF's checked
//! object calls, so a broken tree gives no labels rather than an error.

use mupdf::pdf::{PdfDocument, PdfObject};

/// How deep a number tree may go, and how many ranges it may have.
const MAX_DEPTH: usize = 32;
const MAX_RANGES: usize = 100_000;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Range {
    /// The first page (0-based) the range applies to.
    pub start: u32,
    /// `D`, `R`, `r`, `A`, `a`, or none (the prefix alone).
    pub style: Option<u8>,
    pub prefix: String,
    /// The number of the range's first page (`/St`, 1 by default).
    pub first: u32,
}

/// Every page's label, or `None` when the file has none, or when they are
/// only the page numbers ("1", "2"…), which say nothing new.
pub(crate) fn page_labels(pdf: &PdfDocument, count: u32) -> Option<Vec<String>> {
    let root = pdf.catalog().ok()?.get_dict("PageLabels").ok()??;
    let mut ranges = Vec::new();
    collect(&root, 0, &mut ranges);
    labels(ranges, count)
}

fn collect(node: &PdfObject, depth: usize, out: &mut Vec<Range>) {
    if depth > MAX_DEPTH || out.len() >= MAX_RANGES {
        return;
    }
    if let Ok(Some(nums)) = node.get_dict("Nums") {
        let n = nums.len().unwrap_or(0);
        let mut i = 0;
        while i + 1 < n && out.len() < MAX_RANGES {
            let start = nums.get_array(i as i32).ok().flatten();
            let dict = nums.get_array(i as i32 + 1).ok().flatten();
            if let (Some(start), Some(dict)) = (start, dict)
                && let Ok(start) = start.as_int()
                && let Ok(start) = u32::try_from(start)
            {
                out.push(range(start, &dict));
            }
            i += 2;
        }
    }
    if let Ok(Some(kids)) = node.get_dict("Kids") {
        for k in 0..kids.len().unwrap_or(0) {
            if let Ok(Some(kid)) = kids.get_array(k as i32) {
                collect(&kid, depth + 1, out);
            }
        }
    }
}

fn range(start: u32, dict: &PdfObject) -> Range {
    let style = dict
        .get_dict("S")
        .ok()
        .flatten()
        .and_then(|s| s.as_name().ok())
        .and_then(|n| match n.as_slice() {
            [c @ (b'D' | b'R' | b'r' | b'A' | b'a')] => Some(*c),
            _ => None,
        });
    let prefix = dict
        .get_dict("P")
        .ok()
        .flatten()
        .and_then(|p| p.as_bytes().ok())
        .map(|b| text_string(&b))
        .unwrap_or_default();
    let first = dict
        .get_dict("St")
        .ok()
        .flatten()
        .and_then(|s| s.as_int().ok())
        .and_then(|s| u32::try_from(s).ok())
        .filter(|s| *s >= 1)
        .unwrap_or(1);
    Range {
        start,
        style,
        prefix,
        first,
    }
}

/// The labels for `count` pages from the ranges (in any order).
pub(crate) fn labels(mut ranges: Vec<Range>, count: u32) -> Option<Vec<String>> {
    ranges.sort_by_key(|r| r.start);
    ranges.dedup_by_key(|r| r.start);
    if ranges.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(count as usize);
    let mut ix = 0;
    for page in 0..count {
        while ix + 1 < ranges.len() && ranges[ix + 1].start <= page {
            ix += 1;
        }
        let r = &ranges[ix];
        if r.start > page {
            // Pages before the first range (a malformed tree): their numbers.
            out.push((page + 1).to_string());
            continue;
        }
        let n = r.first.saturating_add(page - r.start);
        let number = match r.style {
            Some(b'D') => n.to_string(),
            Some(b'R') => roman(n).to_uppercase(),
            Some(b'r') => roman(n),
            Some(b'A') => letters(n).to_uppercase(),
            Some(b'a') => letters(n),
            _ => String::new(),
        };
        out.push(format!("{}{number}", r.prefix));
    }
    let plain = out
        .iter()
        .enumerate()
        .all(|(i, l)| *l == (i + 1).to_string());
    (!plain).then_some(out)
}

/// 1 → "i", 4 → "iv", 1994 → "mcmxciv". Past 4999, the number itself.
fn roman(mut n: u32) -> String {
    if n == 0 || n >= 5000 {
        return n.to_string();
    }
    const TABLE: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut s = String::new();
    for (v, r) in TABLE {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

/// The spec's letters: a…z, then aa…zz, then aaa…
fn letters(n: u32) -> String {
    if n == 0 {
        return String::new();
    }
    let letter = (b'a' + ((n - 1) % 26) as u8) as char;
    let times = ((n - 1) / 26 + 1).min(64) as usize;
    std::iter::repeat_n(letter, times).collect()
}

/// A PDF text string: UTF-16 (with a byte-order mark), UTF-8 (with its
/// mark), or PDFDocEncoding, read as Latin-1 (the same for the letters
/// prefixes use).
fn text_string(b: &[u8]) -> String {
    if let Some(rest) = b.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = b.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    b.iter().map(|&c| c as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(start: u32, style: Option<u8>, prefix: &str, first: u32) -> Range {
        Range {
            start,
            style,
            prefix: prefix.into(),
            first,
        }
    }

    #[test]
    fn front_matter_then_numbers_then_appendix() {
        let ranges = vec![
            r(0, Some(b'r'), "", 1),
            r(3, Some(b'D'), "", 1),
            r(6, Some(b'A'), "App. ", 1),
        ];
        let l = labels(ranges, 8).unwrap();
        assert_eq!(l, ["i", "ii", "iii", "1", "2", "3", "App. A", "App. B"]);
    }

    #[test]
    fn plain_numbers_are_no_labels() {
        assert_eq!(labels(vec![r(0, Some(b'D'), "", 1)], 5), None);
        assert_eq!(labels(vec![], 5), None);
    }

    #[test]
    fn styles_and_starts() {
        assert_eq!(roman(1994), "mcmxciv");
        assert_eq!(letters(1), "a");
        assert_eq!(letters(27), "aa");
        assert_eq!(letters(53), "aaa");
        let l = labels(vec![r(0, None, "Cover", 1), r(1, Some(b'D'), "", 10)], 3).unwrap();
        assert_eq!(l, ["Cover", "10", "11"]);
        assert_eq!(text_string(&[0xFE, 0xFF, 0, b'A', 0, b'-']), "A-");
    }
}
