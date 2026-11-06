// SPDX-License-Identifier: AGPL-3.0-or-later

//! Page ranges as people type them: "1-3, 7, 10-" (1-based, inclusive; an open
//! end means "to the last page"). The result is 0-based, in the order typed,
//! without repeats.

/// Parse `text` for a document of `count` pages.
pub fn parse_page_ranges(text: &str, count: u32) -> Result<Vec<u32>, String> {
    let mut out: Vec<u32> = Vec::new();
    let push = |p: u32, out: &mut Vec<u32>| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    let num = |s: &str| -> Result<u32, String> {
        let s = s.trim();
        let n: u32 = s
            .parse()
            .map_err(|_| format!("\"{s}\" isn't a page number"))?;
        if n == 0 || n > count {
            return Err(format!("page {n} is outside 1–{count}"));
        }
        Ok(n)
    };
    for part in text
        .split([',', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        match part.split_once(['-', '–']) {
            Some((a, b)) => {
                let start = if a.trim().is_empty() { 1 } else { num(a)? };
                let end = if b.trim().is_empty() { count } else { num(b)? };
                if start <= end {
                    (start..=end).for_each(|p| push(p - 1, &mut out));
                } else {
                    // "9-5" counts down: the pages in that order.
                    (end..=start).rev().for_each(|p| push(p - 1, &mut out));
                }
            }
            None => push(num(part)? - 1, &mut out),
        }
    }
    if out.is_empty() {
        return Err("type the pages to save, such as 1-3, 7".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::parse_page_ranges as p;

    #[test]
    fn ranges_lists_and_open_ends() {
        assert_eq!(p("1-3, 7", 10), Ok(vec![0, 1, 2, 6]));
        assert_eq!(p("8-", 10), Ok(vec![7, 8, 9]));
        assert_eq!(p("-2", 10), Ok(vec![0, 1]));
        assert_eq!(p("3, 1-3", 10), Ok(vec![2, 0, 1]));
        assert_eq!(p("5-3", 10), Ok(vec![4, 3, 2]));
        assert_eq!(p(" 2 – 4 ", 10), Ok(vec![1, 2, 3]));
    }

    #[test]
    fn bad_input_says_why() {
        assert!(p("", 10).is_err());
        assert!(p("0", 10).is_err());
        assert!(p("11", 10).is_err());
        assert!(p("a-b", 10).is_err());
    }
}
