// SPDX-License-Identifier: AGPL-3.0-or-later

//! pulldown-cmark events into the block tree.

use crate::html::{self, Piece, Tag as HtmlTag};
use crate::model::*;
use crate::slugify;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;

/// Containers nested deeper than this are flattened (a hostile file can nest
/// thousands of quotes; the tree and the UI stay shallow).
const MAX_DEPTH: usize = 32;

enum Frame {
    Root(Vec<Block>),
    Paragraph(Inlines),
    /// A paragraph the builder opened itself, for text outside one (inline
    /// HTML in an HTML block). Closed at the next block boundary.
    Implicit(Inlines),
    Heading(u8, Inlines),
    Quote(Vec<Block>),
    List(Option<u64>, Vec<Item>),
    /// A list item: its task state, its blocks, and loose text (tight lists).
    Item(Option<bool>, Vec<Block>, Inlines),
    Code(String, String),
    Table {
        aligns: Vec<Align>,
        head: Vec<Inlines>,
        rows: Vec<Vec<Inlines>>,
    },
    Row(Vec<Inlines>),
    Cell(Inlines),
    Footnote(String, Vec<Block>),
    Details {
        summary: Inlines,
        body: Vec<Block>,
        in_summary: bool,
    },
}

#[derive(Default)]
struct Styles {
    bold: u32,
    italic: u32,
    strike: u32,
    code: u32,
    kbd: u32,
    sup: u32,
    sub: u32,
}

impl Styles {
    fn span(&self) -> SpanStyle {
        SpanStyle {
            bold: self.bold > 0,
            italic: self.italic > 0,
            strike: self.strike > 0,
            code: self.code > 0,
            kbd: self.kbd > 0,
            sup: self.sup > 0,
            sub: self.sub > 0,
            math: false,
        }
    }

    fn counter(&mut self, tag: HtmlTag) -> Option<&mut u32> {
        Some(match tag {
            HtmlTag::Bold => &mut self.bold,
            HtmlTag::Italic => &mut self.italic,
            HtmlTag::Strike => &mut self.strike,
            HtmlTag::Code => &mut self.code,
            HtmlTag::Kbd => &mut self.kbd,
            HtmlTag::Sup => &mut self.sup,
            HtmlTag::Sub => &mut self.sub,
            _ => return None,
        })
    }
}

struct Builder {
    stack: Vec<Frame>,
    styles: Styles,
    links: Vec<Option<String>>,
    image: Option<(String, String)>,
    swallow: Option<String>,
    /// Whether each container start was really opened (false past MAX_DEPTH).
    opened: Vec<bool>,
    footnotes: HashMap<String, usize>,
    slugs: HashMap<String, usize>,
    outline: Vec<Heading>,
}

pub fn blocks(text: &str) -> MdDoc {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_MATH);
    let mut b = Builder {
        stack: vec![Frame::Root(Vec::new())],
        styles: Styles::default(),
        links: Vec::new(),
        image: None,
        swallow: None,
        opened: Vec::new(),
        footnotes: HashMap::new(),
        slugs: HashMap::new(),
        outline: Vec::new(),
    };
    for event in Parser::new_ext(text, options) {
        b.event(event);
    }
    // Close anything left open (an unclosed <details>, say).
    while b.stack.len() > 1 {
        b.close_top();
    }
    let blocks = match b.stack.pop() {
        Some(Frame::Root(blocks)) => blocks,
        _ => Vec::new(),
    };
    MdDoc {
        blocks,
        outline: b.outline,
        title: None,
        author: None,
    }
}

impl Builder {
    fn root_len(&self) -> usize {
        match self.stack.first() {
            Some(Frame::Root(b)) => b.len(),
            _ => 0,
        }
    }

    /// Add a finished block to the nearest container.
    fn emit(&mut self, block: Block) {
        self.close_implicit();
        for frame in self.stack.iter_mut().rev() {
            match frame {
                Frame::Root(blocks)
                | Frame::Quote(blocks)
                | Frame::Footnote(_, blocks)
                | Frame::Details { body: blocks, .. } => {
                    blocks.push(block);
                    return;
                }
                Frame::Item(_, blocks, loose) => {
                    if !loose.is_empty() {
                        blocks.push(Block::Paragraph(std::mem::take(loose)));
                    }
                    blocks.push(block);
                    return;
                }
                _ => {}
            }
        }
    }

    fn close_implicit(&mut self) {
        if let Some(Frame::Implicit(_)) = self.stack.last() {
            if let Some(Frame::Implicit(inl)) = self.stack.pop() {
                if has_content(&inl) {
                    self.emit(Block::Paragraph(inl));
                }
            }
        }
    }

    /// Where inline content goes now.
    fn inlines(&mut self) -> &mut Inlines {
        let needs_implicit = !matches!(
            self.stack.last(),
            Some(
                Frame::Paragraph(_)
                    | Frame::Implicit(_)
                    | Frame::Heading(..)
                    | Frame::Item(..)
                    | Frame::Cell(_)
                    | Frame::Details {
                        in_summary: true,
                        ..
                    }
            )
        );
        if needs_implicit {
            self.stack.push(Frame::Implicit(Vec::new()));
        }
        match self.stack.last_mut() {
            Some(
                Frame::Paragraph(i)
                | Frame::Implicit(i)
                | Frame::Heading(_, i)
                | Frame::Item(_, _, i)
                | Frame::Cell(i),
            ) => i,
            Some(Frame::Details { summary, .. }) => summary,
            _ => unreachable!("an inline frame was just ensured"),
        }
    }

    fn push_inline(&mut self, inline: Inline) {
        self.inlines().push(inline);
    }

    fn push_text(&mut self, text: &str, style: SpanStyle) {
        if text.is_empty() {
            return;
        }
        if let Some((_, alt)) = self.image.as_mut() {
            alt.push_str(text);
            return;
        }
        let link = self.links.last().cloned().flatten();
        if link.is_none() && !style.code && !style.math {
            // Bare web addresses become links (GFM autolinks).
            let mut rest = text;
            while let Some(i) = find_url(rest) {
                self.push_run(&rest[..i], style, None);
                let len = url_len(&rest[i..]);
                let url = &rest[i..i + len];
                self.push_run(url, style, Some(url.to_string()));
                rest = &rest[i + len..];
            }
            self.push_run(rest, style, None);
        } else {
            self.push_run(text, style, link);
        }
    }

    fn push_run(&mut self, text: &str, style: SpanStyle, link: Option<String>) {
        if text.is_empty() {
            return;
        }
        let inl = self.inlines();
        if let Some(Inline::Text {
            text: last,
            style: s,
            link: l,
        }) = inl.last_mut()
        {
            if *s == style && *l == link {
                last.push_str(text);
                return;
            }
        }
        inl.push(Inline::Text {
            text: text.to_string(),
            style,
            link,
        });
    }

    /// Pop the top frame and emit what it built.
    fn close_top(&mut self) {
        let Some(frame) = self.stack.pop() else {
            return;
        };
        match frame {
            Frame::Root(blocks) => self.stack.push(Frame::Root(blocks)),
            Frame::Paragraph(inl) | Frame::Implicit(inl) => {
                if has_content(&inl) {
                    self.emit(Block::Paragraph(inl));
                }
            }
            Frame::Heading(level, content) => {
                let text = inline_text(&content);
                let base = slugify(&text);
                let n = self.slugs.entry(base.clone()).or_insert(0);
                let slug = if *n == 0 {
                    base.clone()
                } else {
                    format!("{base}-{n}")
                };
                *n += 1;
                // A heading inside a quote or list points at its container.
                let block = self.root_len();
                self.outline.push(Heading {
                    level,
                    text,
                    slug: slug.clone(),
                    block,
                });
                self.emit(Block::Heading {
                    level,
                    content,
                    slug,
                });
            }
            Frame::Quote(blocks) => self.emit(Block::Quote(blocks)),
            Frame::List(start, items) => self.emit(Block::List { start, items }),
            Frame::Item(task, mut blocks, loose) => {
                if has_content(&loose) {
                    blocks.push(Block::Paragraph(loose));
                }
                if let Some(Frame::List(_, items)) = self.stack.last_mut() {
                    items.push(Item { task, blocks });
                } else {
                    for b in blocks {
                        self.emit(b);
                    }
                }
            }
            Frame::Code(lang, mut text) => {
                if text.ends_with('\n') {
                    text.pop();
                }
                self.emit(match lang.to_ascii_lowercase().as_str() {
                    "mermaid" => Block::Mermaid(text),
                    "math" | "latex" | "tex" => Block::Math(text),
                    _ => Block::Code { lang, text },
                });
            }
            Frame::Cell(inl) => {
                if let Some(Frame::Row(cells)) = self.stack.last_mut() {
                    cells.push(inl);
                }
            }
            Frame::Row(cells) => {
                if let Some(Frame::Table { head, rows, .. }) = self.stack.last_mut() {
                    if head.is_empty() {
                        *head = cells;
                    } else {
                        rows.push(cells);
                    }
                }
            }
            Frame::Table { aligns, head, rows } => self.emit(Block::Table { aligns, head, rows }),
            Frame::Footnote(label, body) => {
                let number = self.footnote_number(&label);
                self.emit(Block::Footnote {
                    label,
                    number,
                    body,
                });
            }
            Frame::Details { summary, body, .. } => self.emit(Block::Details { summary, body }),
        }
    }

    fn footnote_number(&mut self, label: &str) -> usize {
        let next = self.footnotes.len() + 1;
        *self.footnotes.entry(label.to_string()).or_insert(next)
    }

    /// Close frames until one matching `is` has been closed.
    fn close_until(&mut self, is: impl Fn(&Frame) -> bool) {
        while self.stack.len() > 1 {
            let done = self.stack.last().is_some_and(&is);
            self.close_top();
            if done {
                return;
            }
        }
    }

    fn depth(&self) -> usize {
        self.stack.len()
    }

    fn event(&mut self, event: Event) {
        // Inside a dropped element (<script>, <svg>, …) split over several
        // inline HTML events, its text is dropped as well.
        if self.swallow.is_some()
            && matches!(
                event,
                Event::Text(_) | Event::Code(_) | Event::InlineMath(_) | Event::SoftBreak
            )
        {
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => {
                if let Some(Frame::Code(_, text)) = self.stack.last_mut() {
                    text.push_str(&t);
                } else {
                    let style = self.styles.span();
                    self.push_text(&t, style);
                }
            }
            Event::Code(t) => {
                let mut style = self.styles.span();
                style.code = true;
                self.push_text(&t, style);
            }
            Event::InlineMath(t) => {
                let mut style = self.styles.span();
                style.math = true;
                self.push_text(&t, style);
            }
            Event::DisplayMath(t) => {
                // Split the paragraph around it: math gets its own block.
                if let Some(Frame::Paragraph(inl) | Frame::Implicit(inl)) = self.stack.last_mut() {
                    let before = std::mem::take(inl);
                    if has_content(&before) {
                        self.emit_keep_frame(Block::Paragraph(before));
                    }
                }
                self.emit_keep_frame(Block::Math(t.trim().to_string()));
            }
            Event::Html(t) | Event::InlineHtml(t) => self.html(&t),
            Event::FootnoteReference(label) => {
                let number = self.footnote_number(&label);
                self.push_inline(Inline::FootnoteRef {
                    label: label.to_string(),
                    number,
                });
            }
            Event::SoftBreak => {
                let style = self.styles.span();
                self.push_text(" ", style);
            }
            Event::HardBreak => self.push_inline(Inline::Break),
            Event::Rule => self.emit(Block::Rule),
            Event::TaskListMarker(done) => {
                if let Some(Frame::Item(task, ..)) = self.stack.last_mut() {
                    *task = Some(done);
                }
            }
        }
    }

    /// Emit a block while a paragraph frame stays open around it.
    fn emit_keep_frame(&mut self, block: Block) {
        let top = self.stack.pop();
        self.emit(block);
        if let Some(top) = top {
            self.stack.push(top);
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                self.close_implicit();
                self.stack.push(Frame::Paragraph(Vec::new()));
            }
            Tag::Heading { level, .. } => {
                self.close_implicit();
                self.stack.push(Frame::Heading(level as u8, Vec::new()));
            }
            Tag::BlockQuote(_) => {
                self.close_implicit();
                let open = self.depth() < MAX_DEPTH;
                self.opened.push(open);
                if open {
                    self.stack.push(Frame::Quote(Vec::new()));
                }
            }
            Tag::List(start) => {
                self.close_implicit();
                let open = self.depth() < MAX_DEPTH;
                self.opened.push(open);
                if open {
                    self.stack.push(Frame::List(start, Vec::new()));
                }
            }
            Tag::Item => {
                let open = matches!(self.stack.last(), Some(Frame::List(..)));
                self.opened.push(open);
                if open {
                    self.stack.push(Frame::Item(None, Vec::new(), Vec::new()));
                }
            }
            Tag::CodeBlock(kind) => {
                self.close_implicit();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                self.stack.push(Frame::Code(lang, String::new()));
            }
            Tag::HtmlBlock => self.close_implicit(),
            Tag::FootnoteDefinition(label) => {
                self.close_implicit();
                self.stack
                    .push(Frame::Footnote(label.to_string(), Vec::new()));
            }
            Tag::Table(aligns) => {
                self.close_implicit();
                self.stack.push(Frame::Table {
                    aligns: aligns
                        .iter()
                        .map(|a| match a {
                            Alignment::None => Align::None,
                            Alignment::Left => Align::Left,
                            Alignment::Center => Align::Center,
                            Alignment::Right => Align::Right,
                        })
                        .collect(),
                    head: Vec::new(),
                    rows: Vec::new(),
                });
            }
            Tag::TableHead | Tag::TableRow => self.stack.push(Frame::Row(Vec::new())),
            Tag::TableCell => self.stack.push(Frame::Cell(Vec::new())),
            Tag::Emphasis => self.styles.italic += 1,
            Tag::Strong => self.styles.bold += 1,
            Tag::Strikethrough => self.styles.strike += 1,
            Tag::Superscript => self.styles.sup += 1,
            Tag::Subscript => self.styles.sub += 1,
            Tag::Link { dest_url, .. } => {
                let dest = dest_url.to_string();
                self.links.push(html::safe_target(&dest).then_some(dest));
            }
            Tag::Image { dest_url, .. } => {
                self.image = Some((dest_url.to_string(), String::new()));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.close_until(|f| matches!(f, Frame::Paragraph(_))),
            TagEnd::Heading(_) => self.close_until(|f| matches!(f, Frame::Heading(..))),
            TagEnd::BlockQuote(_) => {
                if self.opened.pop().unwrap_or(false) {
                    self.close_until(|f| matches!(f, Frame::Quote(_)));
                }
            }
            TagEnd::List(_) => {
                if self.opened.pop().unwrap_or(false) {
                    self.close_until(|f| matches!(f, Frame::List(..)));
                }
            }
            TagEnd::Item => {
                if self.opened.pop().unwrap_or(false) {
                    self.close_until(|f| matches!(f, Frame::Item(..)));
                }
            }
            TagEnd::CodeBlock => self.close_until(|f| matches!(f, Frame::Code(..))),
            TagEnd::HtmlBlock => self.close_implicit(),
            TagEnd::FootnoteDefinition => self.close_until(|f| matches!(f, Frame::Footnote(..))),
            TagEnd::Table => self.close_until(|f| matches!(f, Frame::Table { .. })),
            TagEnd::TableHead | TagEnd::TableRow => {
                self.close_until(|f| matches!(f, Frame::Row(_)))
            }
            TagEnd::TableCell => self.close_until(|f| matches!(f, Frame::Cell(_))),
            TagEnd::Emphasis => self.styles.italic = self.styles.italic.saturating_sub(1),
            TagEnd::Strong => self.styles.bold = self.styles.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.styles.strike = self.styles.strike.saturating_sub(1),
            TagEnd::Superscript => self.styles.sup = self.styles.sup.saturating_sub(1),
            TagEnd::Subscript => self.styles.sub = self.styles.sub.saturating_sub(1),
            TagEnd::Link => {
                self.links.pop();
            }
            TagEnd::Image => {
                if let Some((src, alt)) = self.image.take() {
                    if html::safe_target(&src) {
                        self.push_inline(Inline::Image { src, alt });
                    } else {
                        let style = self.styles.span();
                        self.push_text(&alt, style);
                    }
                }
            }
            _ => {}
        }
    }

    fn html(&mut self, fragment: &str) {
        let mut swallow = self.swallow.take();
        let pieces = html::pieces(fragment, &mut swallow);
        self.swallow = swallow;
        for (piece, href) in pieces {
            match piece {
                Piece::Text(t) => {
                    // Whitespace between block tags isn't content.
                    let in_inline = !matches!(
                        self.stack.last(),
                        Some(
                            Frame::Root(_)
                                | Frame::Quote(_)
                                | Frame::Footnote(..)
                                | Frame::Details {
                                    in_summary: false,
                                    ..
                                }
                        )
                    );
                    if in_inline || !t.trim().is_empty() {
                        let style = self.styles.span();
                        self.push_text(&t, style);
                    }
                }
                Piece::Break => self.push_inline(Inline::Break),
                Piece::Image { src, alt } => self.push_inline(Inline::Image { src, alt }),
                Piece::Open(HtmlTag::Link) => self.links.push(href),
                Piece::Close(HtmlTag::Link) => {
                    self.links.pop();
                }
                Piece::Open(HtmlTag::Details) => {
                    self.close_implicit();
                    if self.depth() < MAX_DEPTH {
                        self.stack.push(Frame::Details {
                            summary: Vec::new(),
                            body: Vec::new(),
                            in_summary: false,
                        });
                    }
                }
                Piece::Close(HtmlTag::Details) => {
                    if self
                        .stack
                        .iter()
                        .any(|f| matches!(f, Frame::Details { .. }))
                    {
                        self.close_until(|f| matches!(f, Frame::Details { .. }));
                    }
                }
                Piece::Open(HtmlTag::Summary) | Piece::Close(HtmlTag::Summary) => {
                    let open = matches!(piece, Piece::Open(_));
                    if !open {
                        self.close_implicit();
                    }
                    if let Some(Frame::Details { in_summary, .. }) = self.stack.last_mut() {
                        *in_summary = open;
                    }
                }
                Piece::Open(HtmlTag::Block) | Piece::Close(HtmlTag::Block) => self.close_implicit(),
                Piece::Open(tag) => {
                    if let Some(n) = self.styles.counter(tag) {
                        *n += 1;
                    }
                }
                Piece::Close(tag) => {
                    if let Some(n) = self.styles.counter(tag) {
                        *n = n.saturating_sub(1);
                    }
                }
            }
        }
    }
}

fn has_content(inl: &Inlines) -> bool {
    inl.iter().any(|i| match i {
        Inline::Text { text, .. } => !text.trim().is_empty(),
        _ => true,
    })
}

/// Where the next bare `http://` or `https://` address starts.
fn find_url(text: &str) -> Option<usize> {
    let a = text.find("https://");
    let b = text.find("http://");
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Length of the address at the start of `text`: up to whitespace or `<`,
/// without trailing punctuation.
fn url_len(text: &str) -> usize {
    let end = text
        .find(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '"')
        .unwrap_or(text.len());
    let url = &text[..end];
    url.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '\'', '*', '_'])
        .len()
}
