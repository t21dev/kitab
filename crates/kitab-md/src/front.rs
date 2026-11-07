// SPDX-License-Identifier: AGPL-3.0-or-later

//! YAML frontmatter, read leniently: top-level `key: value` lines, with
//! indented lists (`- a`) and maps (`k: v`) folded into one value. Anything
//! that doesn't read cleanly (unbalanced brackets, a line without a key) makes
//! the block invalid, and the UI shows it as a code block (md-viewer-pwa).

pub enum Front {
    Valid(Vec<(String, String)>),
    Invalid(String),
}

/// Split `---` frontmatter from the body. `None` when there is none.
pub fn split(text: &str) -> (Option<Front>, &str) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return (None, text);
    };
    let (raw, body) = match rest.find("\n---\n") {
        Some(i) => (&rest[..i], &rest[i + 5..]),
        None => match rest.strip_suffix("\n---") {
            Some(raw) => (raw, ""),
            None => return (None, text),
        },
    };
    (Some(read(raw)), body)
}

fn balanced(v: &str) -> bool {
    let mut depth: i32 = 0;
    for c in v.chars() {
        match c {
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0 && v.matches('"').count() % 2 == 0
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    let quoted = v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')));
    if quoted {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

fn read(raw: &str) -> Front {
    let invalid = || Front::Invalid(raw.to_string());
    let mut pairs: Vec<(String, String)> = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if indented {
            // A nested list item or map entry belongs to the last key.
            let Some((_, value)) = pairs.last_mut() else {
                return invalid();
            };
            let item = line.trim().trim_start_matches("- ").trim();
            if !balanced(item) || item.matches(": ").count() > 1 {
                return invalid();
            }
            if !value.is_empty() {
                value.push_str(", ");
            }
            value.push_str(&unquote(item));
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return invalid();
        };
        let key = key.trim();
        let key_ok = !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '));
        if !key_ok || !balanced(value) {
            return invalid();
        }
        let value = value.trim();
        let value = value
            .strip_prefix('[')
            .and_then(|v| v.strip_suffix(']'))
            .map(|v| v.split(',').map(unquote).collect::<Vec<_>>().join(", "))
            .unwrap_or_else(|| unquote(value));
        pairs.push((key.to_string(), value));
    }
    if pairs.is_empty() {
        invalid()
    } else {
        Front::Valid(pairs)
    }
}
