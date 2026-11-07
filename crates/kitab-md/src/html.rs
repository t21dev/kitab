// SPDX-License-Identifier: AGPL-3.0-or-later

//! Raw HTML in Markdown, through an allow-list (PLAN.md §3.4). Nothing here
//! builds or runs HTML: tags become style changes, links, images or line
//! breaks, or are dropped. Elements that carry code or styling (`script`,
//! `style`, `iframe`, `object`, `embed`, `svg`, `math`, `template`, `noscript`)
//! are dropped with their content. `on*` attributes, `style`, and
//! `javascript:` / `data:` / `vbscript:` / `file:` targets never pass.

/// One piece of an HTML fragment, after the allow-list.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text(String),
    Open(Tag),
    Close(Tag),
    Break,
    Image { src: String, alt: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Bold,
    Italic,
    Strike,
    Code,
    Kbd,
    Sup,
    Sub,
    /// `<a href>` with a safe target, carried separately.
    Link,
    Details,
    Summary,
    /// Paragraph-like containers (`p`, `div`): a block boundary.
    Block,
}

/// Elements dropped together with everything inside them.
const SWALLOW: &[&str] = &[
    "script", "style", "iframe", "object", "embed", "svg", "math", "template", "noscript", "frame",
    "frameset", "applet", "canvas", "audio", "video", "form", "select", "textarea", "head",
    "title",
];

/// Whether a link or image target is allowed: http(s), mailto, a relative
/// path or an `#anchor`. Anything with another scheme is not.
pub fn safe_target(target: &str) -> bool {
    let t: String = target
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect::<String>()
        .to_ascii_lowercase();
    if t.is_empty() {
        return false;
    }
    match t.split_once(':') {
        // "C:/path" style drive letters are files: not allowed either.
        Some((scheme, _))
            if !scheme.contains('/') && !scheme.contains('#') && !scheme.contains('?') =>
        {
            matches!(scheme, "http" | "https" | "mailto")
        }
        _ => true,
    }
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = lower[at + name.len()..].trim_start();
        if before_ok && rest.starts_with('=') {
            let offset = tag.len() - rest.len() + 1;
            let value = tag[offset..].trim_start();
            return Some(match value.chars().next() {
                Some(q @ ('"' | '\'')) => value[1..].split(q).next().unwrap_or("").to_string(),
                _ => value
                    .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
                    .next()
                    .unwrap_or("")
                    .to_string(),
            });
        }
        from = at + name.len();
    }
    None
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find(';').filter(|e| *e <= 10);
        let decoded = end.and_then(|e| {
            let name = &rest[1..e];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ if name.starts_with("#x") || name.starts_with("#X") => {
                    u32::from_str_radix(&name[2..], 16)
                        .ok()
                        .and_then(char::from_u32)
                }
                _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            c.map(|c| (c, e))
        });
        match decoded {
            Some((c, e)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Turn an HTML fragment into allowed pieces. `link` gets the href of each
/// `<a>` that opens (or `None` when it isn't safe).
pub fn pieces(fragment: &str, swallowing: &mut Option<String>) -> Vec<(Piece, Option<String>)> {
    let mut out = Vec::new();
    let mut rest = fragment;
    while !rest.is_empty() {
        // Inside a swallowed element: skip to its closing tag.
        if let Some(name) = swallowing.clone() {
            let close = format!("</{name}");
            match rest.to_ascii_lowercase().find(&close) {
                Some(i) => {
                    rest = &rest[i..];
                    rest = rest.find('>').map_or("", |e| &rest[e + 1..]);
                    *swallowing = None;
                }
                None => return out,
            }
            continue;
        }
        let Some(lt) = rest.find('<') else {
            out.push((Piece::Text(decode_entities(rest)), None));
            break;
        };
        if lt > 0 {
            out.push((Piece::Text(decode_entities(&rest[..lt])), None));
        }
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |e| &rest[e + 3..]);
            continue;
        }
        let Some(gt) = rest.find('>') else {
            // Not a tag after all: show it.
            out.push((Piece::Text(decode_entities(rest)), None));
            break;
        };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if SWALLOW.contains(&name.as_str()) {
            let self_closing = tag.trim_end().ends_with('/');
            if !closing && !self_closing {
                *swallowing = Some(name);
            }
            continue;
        }
        let kind = match name.as_str() {
            "b" | "strong" => Some(Tag::Bold),
            "i" | "em" => Some(Tag::Italic),
            "s" | "del" | "strike" => Some(Tag::Strike),
            "code" | "tt" => Some(Tag::Code),
            "kbd" => Some(Tag::Kbd),
            "sup" => Some(Tag::Sup),
            "sub" => Some(Tag::Sub),
            "a" => Some(Tag::Link),
            "details" => Some(Tag::Details),
            "summary" => Some(Tag::Summary),
            "p" | "div" | "section" | "article" | "center" | "blockquote" => Some(Tag::Block),
            _ => None,
        };
        match name.as_str() {
            "br" => out.push((Piece::Break, None)),
            "img" if !closing => {
                let src = attr(tag, "src").unwrap_or_default();
                if safe_target(&src) {
                    let alt = decode_entities(&attr(tag, "alt").unwrap_or_default());
                    out.push((Piece::Image { src, alt }, None));
                }
            }
            _ => {
                if let Some(kind) = kind {
                    if closing {
                        out.push((Piece::Close(kind), None));
                    } else {
                        let href = (kind == Tag::Link)
                            .then(|| attr(tag, "href"))
                            .flatten()
                            .filter(|h| safe_target(h));
                        out.push((Piece::Open(kind), href));
                    }
                }
            }
        }
    }
    out
}
