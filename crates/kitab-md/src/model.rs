// SPDX-License-Identifier: AGPL-3.0-or-later

//! The block tree the UI draws. Plain data: no GPUI here.

/// How a run of text is styled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub kbd: bool,
    pub sup: bool,
    pub sub: bool,
    /// Inline TeX, shown as styled source until math rendering lands.
    pub math: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text {
        text: String,
        style: SpanStyle,
        /// A safe link target (http, https, mailto, a relative path or `#anchor`).
        link: Option<String>,
    },
    /// An image: a path relative to the document or an http(s) address
    /// (remote images stay blocked until allowed).
    Image { src: String, alt: String },
    /// A hard line break.
    Break,
    /// A reference to a footnote, by label; drawn as its number.
    FootnoteRef { label: String, number: usize },
}

pub type Inlines = Vec<Inline>;

/// What a run of code is, for colouring from the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Keyword,
    String,
    Comment,
    Number,
    Function,
    Type,
    Tag,
    Punctuation,
}

/// A coloured run of a code block: byte range and kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// `Some(done)` for a task list item.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading {
        level: u8,
        content: Inlines,
        slug: String,
    },
    Paragraph(Inlines),
    List {
        /// The first number of an ordered list; `None` for bullets.
        start: Option<u64>,
        items: Vec<Item>,
    },
    Quote(Vec<Block>),
    Code {
        lang: String,
        text: String,
        /// Syntax colours, empty for an unknown language.
        tokens: Vec<Token>,
    },
    /// A ```mermaid fence (drawn as a diagram later; its source for now).
    Mermaid(String),
    /// Display math: `$$…$$` or a ```math fence.
    Math(String),
    Table {
        aligns: Vec<Align>,
        head: Vec<Inlines>,
        rows: Vec<Vec<Inlines>>,
    },
    Rule,
    /// `<details><summary>…</summary>…</details>`.
    Details {
        summary: Inlines,
        body: Vec<Block>,
    },
    Footnote {
        label: String,
        number: usize,
        body: Vec<Block>,
    },
    /// Valid YAML-ish frontmatter as key/value pairs.
    Frontmatter(Vec<(String, String)>),
}

/// A heading, for the outline and `#anchor` links.
#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub slug: String,
    /// Index of the top-level block the heading is.
    pub block: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MdDoc {
    pub blocks: Vec<Block>,
    pub outline: Vec<Heading>,
    pub title: Option<String>,
    pub author: Option<String>,
}

impl MdDoc {
    pub(crate) fn shift_outline(&mut self, by: usize) {
        for h in &mut self.outline {
            h.block += by;
        }
    }

    /// The block a `#anchor` points at. Trailing hyphens are ignored, so
    /// links written without GitHub's trailing `-` (from a trailing emoji) work.
    pub fn anchor(&self, fragment: &str) -> Option<usize> {
        let want = crate::slugify(fragment.trim_start_matches('#'));
        let want = want.trim_end_matches('-');
        self.outline
            .iter()
            .find(|h| h.slug.trim_end_matches('-') == want)
            .map(|h| h.block)
    }

    /// How many fenced Mermaid diagrams the document has, at any depth.
    pub fn mermaid_count(&self) -> usize {
        fn count(blocks: &[Block]) -> usize {
            blocks
                .iter()
                .map(|b| match b {
                    Block::Mermaid(_) => 1,
                    Block::Quote(b)
                    | Block::Details { body: b, .. }
                    | Block::Footnote { body: b, .. } => count(b),
                    Block::List { items, .. } => items.iter().map(|i| count(&i.blocks)).sum(),
                    _ => 0,
                })
                .sum()
        }
        count(&self.blocks)
    }
}

/// A block's text, for find and copying. Lines and cells are separated by
/// newlines and tabs.
pub fn plain_text(block: &Block) -> String {
    let mut out = String::new();
    push_block(block, &mut out);
    out
}

pub fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    push_inlines(inlines, &mut out);
    out
}

fn push_inlines(inlines: &[Inline], out: &mut String) {
    for i in inlines {
        match i {
            Inline::Text { text, .. } => out.push_str(text),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::Break => out.push('\n'),
            Inline::FootnoteRef { number, .. } => out.push_str(&format!("[{number}]")),
        }
    }
}

fn push_block(block: &Block, out: &mut String) {
    let nl = |out: &mut String| {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
    };
    match block {
        Block::Heading { content, .. } | Block::Paragraph(content) => push_inlines(content, out),
        Block::List { items, .. } => {
            for item in items {
                nl(out);
                for b in &item.blocks {
                    nl(out);
                    push_block(b, out);
                }
            }
        }
        Block::Quote(blocks) | Block::Footnote { body: blocks, .. } => {
            for b in blocks {
                nl(out);
                push_block(b, out);
            }
        }
        Block::Details { summary, body } => {
            push_inlines(summary, out);
            for b in body {
                nl(out);
                push_block(b, out);
            }
        }
        Block::Code { text, .. } | Block::Mermaid(text) | Block::Math(text) => out.push_str(text),
        Block::Table { head, rows, .. } => {
            for (i, cell) in head.iter().enumerate() {
                if i > 0 {
                    out.push('\t');
                }
                push_inlines(cell, out);
            }
            for row in rows {
                out.push('\n');
                for (i, cell) in row.iter().enumerate() {
                    if i > 0 {
                        out.push('\t');
                    }
                    push_inlines(cell, out);
                }
            }
        }
        Block::Rule => {}
        Block::Frontmatter(pairs) => {
            for (k, v) in pairs {
                nl(out);
                out.push_str(k);
                out.push_str(": ");
                out.push_str(v);
            }
        }
    }
}
