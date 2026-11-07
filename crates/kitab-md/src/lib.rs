// SPDX-License-Identifier: AGPL-3.0-or-later

//! Markdown for Kitab (PLAN.md §3.4): `text → normalise → frontmatter →
//! pulldown-cmark (GFM) → sanitise → block tree`. The UI draws the tree; no
//! HTML is ever produced or run. Raw HTML passes through an allow-list
//! (`details`, `summary`, `kbd`, `sub`, `sup`, `br`, `img`, `a` and inline
//! formatting); everything else is dropped, and `<script>`, `<style>`,
//! `<iframe>`, `<svg>` and friends lose their content too (AGENTS.md rule 11).

mod front;
mod highlight;
mod html;
mod model;
mod parse;
mod slug;

pub use highlight::highlight;
pub use model::*;
pub use slug::slugify;

/// Parse a Markdown document. Never fails: anything odd becomes text.
pub fn parse(source: &str) -> MdDoc {
    let text = normalise(source);
    let (front, body) = front::split(&text);
    let mut doc = parse::blocks(body);
    // Lines before the body (the frontmatter) count for the editor.
    let front_lines = text[..text.len() - body.len()].matches('\n').count();
    for l in &mut doc.block_lines {
        *l += front_lines;
    }
    match front {
        Some(front::Front::Valid(pairs)) => {
            doc.title = pairs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("title"))
                .map(|(_, v)| v.clone())
                .filter(|v| !v.is_empty());
            doc.author = pairs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("author"))
                .map(|(_, v)| v.clone())
                .filter(|v| !v.is_empty());
            doc.blocks.insert(0, Block::Frontmatter(pairs));
            doc.block_lines.insert(0, 0);
            doc.shift_outline(1);
        }
        Some(front::Front::Invalid(raw)) => {
            doc.blocks.insert(
                0,
                Block::Code {
                    lang: "yaml".into(),
                    tokens: highlight::highlight("yaml", &raw),
                    text: raw,
                },
            );
            doc.block_lines.insert(0, 0);
            doc.shift_outline(1);
        }
        None => {}
    }
    if doc.title.is_none() {
        doc.title = doc
            .outline
            .iter()
            .find(|h| h.level == 1)
            .map(|h| h.text.clone());
    }
    doc
}

/// CRLF to LF, and Unicode bullets at the start of a line ("• item") to
/// Markdown list markers, except inside code fences (md-viewer-pwa's
/// `normalizeMarkdown`).
pub fn normalise(source: &str) -> String {
    let text = source
        .strip_prefix('\u{feff}')
        .unwrap_or(source)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut out = String::with_capacity(text.len());
    let mut fence: Option<(char, usize)> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        let marker = trimmed.chars().next();
        let run = |c: char| trimmed.chars().take_while(|x| *x == c).count();
        if let Some(c @ ('`' | '~')) = marker {
            let n = run(c);
            if n >= 3 {
                match fence {
                    None => fence = Some((c, n)),
                    Some((fc, fnum)) if fc == c && n >= fnum && trimmed[n..].trim().is_empty() => {
                        fence = None
                    }
                    _ => {}
                }
                out.push_str(line);
                continue;
            }
        }
        if fence.is_none() {
            // md-viewer-pwa's set: bullets pasted from docs and social posts.
            if let Some(
                b @ ('•' | '◦' | '▪' | '▫' | '‣' | '⁃' | '●' | '○' | '■' | '□' | '►' | '▸' | '➤'
                | '➢' | '∙' | '·'),
            ) = marker
            {
                let rest = &trimmed[b.len_utf8()..];
                if rest.starts_with(' ') || rest.starts_with('\t') {
                    out.push_str(indent);
                    out.push('-');
                    out.push_str(rest);
                    continue;
                }
            }
        }
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests;
