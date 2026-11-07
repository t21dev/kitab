// SPDX-License-Identifier: AGPL-3.0-or-later

//! Heading anchors, as GitHub makes them (github-slugger): lower case, letters,
//! digits, `-` and `_` kept, spaces become `-`, everything else dropped.
//! Duplicates get `-1`, `-2` from the parser.

pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.trim().chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            out.extend(c.to_lowercase());
        } else if c == ' ' {
            out.push('-');
        }
    }
    out
}
