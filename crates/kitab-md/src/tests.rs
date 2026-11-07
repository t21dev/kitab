// SPDX-License-Identifier: AGPL-3.0-or-later

// Tests may unwrap (AGENTS.md rule 9).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/md")
        .join(name);
    std::fs::read_to_string(p).expect("fixture")
}

fn expected(name: &str) -> serde_json::Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/expected.json");
    let all: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(p).expect("expected.json")).expect("json");
    all["md"][name].clone()
}

fn all_text(doc: &MdDoc) -> String {
    doc.blocks
        .iter()
        .map(plain_text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Heading and Mermaid counts match expected.json for every fixture
/// (AGENTS.md rule 21).
#[test]
fn every_fixture_matches_expected_counts() {
    for name in [
        "01-gfm-kitchen-sink.md",
        "02-mermaid.md",
        "03-code-highlighting.md",
        "04-frontmatter.md",
        "05-frontmatter-invalid.md",
        "06-math.md",
        "07-unicode-rtl.md",
        "08-links-and-anchors.md",
        "09-hostile.md",
        "10-large.md",
    ] {
        let doc = parse(&fixture(name));
        let want = expected(name);
        assert_eq!(
            doc.outline.len() as u64,
            want["headings"].as_u64().unwrap(),
            "{name}: headings"
        );
        assert_eq!(
            doc.mermaid_count() as u64,
            want["mermaid_blocks"].as_u64().unwrap(),
            "{name}: mermaid blocks"
        );
    }
}

#[test]
fn kitchen_sink_has_every_block_kind() {
    let doc = parse(&fixture("01-gfm-kitchen-sink.md"));
    let has = |f: &dyn Fn(&Block) -> bool| doc.blocks.iter().any(f);
    assert!(has(&|b| matches!(b, Block::Table { .. })));
    assert!(has(
        &|b| matches!(b, Block::Code { lang, .. } if lang == "rust")
    ));
    assert!(has(&|b| matches!(b, Block::Rule)));
    assert!(has(&|b| matches!(b, Block::Details { .. })));
    assert!(has(&|b| matches!(b, Block::Footnote { .. })));
    assert!(has(&|b| matches!(b, Block::Quote(_))));
    let tasks = doc.blocks.iter().find_map(|b| match b {
        Block::List { items, .. } if items.iter().any(|i| i.task.is_some()) => Some(items),
        _ => None,
    });
    let tasks = tasks.expect("a task list");
    assert_eq!(tasks.iter().filter(|i| i.task == Some(true)).count(), 2);
    // Bare URLs become links.
    let text_links: Vec<_> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(i) => Some(i),
            _ => None,
        })
        .flatten()
        .filter_map(|i| match i {
            Inline::Text { link: Some(l), .. } => Some(l.clone()),
            _ => None,
        })
        .collect();
    assert!(
        text_links
            .iter()
            .any(|l| l == "https://kitab.example.invalid/docs")
    );
    // <kbd>, <sub> and <sup> survive as styles.
    let styles: Vec<SpanStyle> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(i) => Some(i),
            _ => None,
        })
        .flatten()
        .filter_map(|i| match i {
            Inline::Text { style, .. } => Some(*style),
            _ => None,
        })
        .collect();
    assert!(styles.iter().any(|s| s.kbd));
    assert!(styles.iter().any(|s| s.sub));
    assert!(styles.iter().any(|s| s.sup));
}

#[test]
fn frontmatter_valid_and_invalid() {
    let doc = parse(&fixture("04-frontmatter.md"));
    assert_eq!(doc.title.as_deref(), Some("Frontmatter sample"));
    assert_eq!(doc.author.as_deref(), Some("Kitab test suite"));
    match &doc.blocks[0] {
        Block::Frontmatter(pairs) => {
            assert!(
                pairs
                    .iter()
                    .any(|(k, v)| k == "tags" && v == "markdown, metadata")
            );
        }
        other => panic!("frontmatter block expected, got {other:?}"),
    }
    let doc = parse(&fixture("05-frontmatter-invalid.md"));
    assert!(matches!(&doc.blocks[0], Block::Code { lang, .. } if lang == "yaml"));
    // The title falls back to the first heading, then the file name (UI).
    assert_eq!(doc.title.as_deref(), Some("Invalid frontmatter"));
    // Outline entries point at the right blocks after the frontmatter block.
    assert!(matches!(
        doc.blocks[doc.outline[0].block],
        Block::Heading { .. }
    ));
}

#[test]
fn math_blocks_and_dollar_amounts() {
    let doc = parse(&fixture("06-math.md"));
    let maths = doc
        .blocks
        .iter()
        .filter(|b| matches!(b, Block::Math(_)))
        .count();
    assert_eq!(maths, 3, "two $$ blocks and one ```math fence");
    let text = all_text(&doc);
    assert!(
        text.contains("costs $12 and the other costs $15"),
        "dollars stay text"
    );
}

#[test]
fn hostile_input_is_inert() {
    let doc = parse(&fixture("09-hostile.md"));
    let text = all_text(&doc);
    assert!(
        !text.contains("pwned"),
        "script and handlers dropped: {text}"
    );
    assert!(!text.contains("display: none"), "style dropped");
    assert!(!text.contains("alert"), "svg script dropped: {text}");
    // No link with a dangerous scheme survives anywhere.
    fn links(blocks: &[Block], out: &mut Vec<String>) {
        for b in blocks {
            match b {
                Block::Paragraph(i) | Block::Heading { content: i, .. } => {
                    for x in i {
                        if let Inline::Text { link: Some(l), .. } = x {
                            out.push(l.clone());
                        }
                        if let Inline::Image { src, .. } = x {
                            out.push(src.clone());
                        }
                    }
                }
                Block::Quote(b)
                | Block::Details { body: b, .. }
                | Block::Footnote { body: b, .. } => links(b, out),
                Block::List { items, .. } => {
                    for i in items {
                        links(&i.blocks, out)
                    }
                }
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    links(&doc.blocks, &mut found);
    for l in &found {
        let l = l.to_ascii_lowercase();
        assert!(
            !l.starts_with("javascript:") && !l.starts_with("data:"),
            "{l}"
        );
    }
    // The safe text still renders, and the unclosed fence becomes code.
    assert!(text.contains("must not run"));
    assert!(matches!(doc.blocks.last(), Some(Block::Code { .. })));
}

#[test]
fn deep_nesting_stays_shallow() {
    let deep = format!("{}deep\n\n{}item\n", "> ".repeat(500), "- ".repeat(500));
    let doc = parse(&deep);
    fn depth(blocks: &[Block]) -> usize {
        blocks
            .iter()
            .map(|b| match b {
                Block::Quote(b) => 1 + depth(b),
                Block::List { items, .. } => {
                    1 + items.iter().map(|i| depth(&i.blocks)).max().unwrap_or(0)
                }
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }
    assert!(depth(&doc.blocks) <= 40, "depth {}", depth(&doc.blocks));
    assert!(all_text(&doc).contains("deep"));
}

#[test]
fn anchors_follow_github_slugs() {
    let doc = parse(&fixture("08-links-and-anchors.md"));
    assert!(doc.anchor("#section-a").is_some());
    assert!(doc.anchor("#what-about-punctuation-and-émojis").is_some());
    assert_eq!(
        slugify("What about punctuation, and émojis? 🚀"),
        "what-about-punctuation-and-émojis-"
    );
}

#[test]
fn unicode_bullets_become_lists_outside_fences() {
    let doc = parse("• one\n• two\n\n```\n• not a list\n```\n");
    assert!(matches!(&doc.blocks[0], Block::List { items, .. } if items.len() == 2));
    assert!(matches!(&doc.blocks[1], Block::Code { text, .. } if text == "• not a list"));
}

#[test]
fn large_document_parses_quickly() {
    let src = fixture("10-large.md");
    let started = std::time::Instant::now();
    let doc = parse(&src);
    let ms = started.elapsed().as_millis();
    assert_eq!(doc.outline.len(), 401);
    assert!(all_text(&doc).contains("kitab-md-needle-400"));
    // Generous for debug builds and slow CI; release is far faster.
    assert!(ms < 3000, "{ms} ms");
}
