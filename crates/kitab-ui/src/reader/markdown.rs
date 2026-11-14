// SPDX-License-Identifier: AGPL-3.0-or-later

//! Markdown in the reader (PLAN.md §3.4, DESIGN.md § Reading area). A
//! document is a list of top-level blocks in the same virtualised list that
//! holds PDF pages, so tabs, the outline, the dock, history and session
//! restore work unchanged. Text reflows in a centred column on the app's own
//! surface; zoom scales the type.
//!
//! Text selection: md_select.rs.

use super::{Reader, ReaderEvent};
use crate::icons::icon;
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, App, Context, Font, FontStyle, FontWeight, Hsla, InteractiveElement,
    InteractiveText, IntoElement, ListOffset, ParentElement, SharedString,
    StatefulInteractiveElement, StrikethroughStyle, Styled, StyledText, TextRun, UnderlineStyle,
    WeakEntity, div, img, prelude::FluentBuilder, px, relative, svg,
};
use kitab_md::{Align, Block, Inline, Item, MdDoc, SpanStyle, Token, TokenKind, plain_text};
use std::cell::Cell;
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Reading column width at 100 %.
const COLUMN: f32 = 720.0;
/// Body text size at 100 %.
const BODY: f32 = 16.0;
pub(crate) const MONO_FONT: &str = "Consolas";
const MATH_FONT: &str = "Cambria Math";
/// Nesting drawn before deeper blocks are drawn flat.
const MAX_DRAW_DEPTH: usize = 12;

pub(super) struct MdState {
    pub doc: Arc<MdDoc>,
    /// The file's text as loaded or last saved (what Edit starts from).
    pub source: Arc<str>,
    /// Source line of each block (see `MdDoc::block_lines`).
    pub block_lines: Vec<usize>,
    /// The editor, while editing.
    pub edit: Option<super::editor::Editor>,
    pub dir: PathBuf,
    /// `<details>` the reader has opened, by block and position.
    pub open_details: HashSet<u64>,
    /// The file's modified time when last read (live reload, `live.rs`).
    pub mtime: Option<std::time::SystemTime>,
    /// Drawn Mermaid diagrams (`mermaid.rs`).
    pub diagrams: super::mermaid::Diagrams,
    /// Each wide code block's sideways scroll, by block and position.
    pub code_scroll:
        std::rc::Rc<std::cell::RefCell<std::collections::HashMap<u64, gpui::ScrollHandle>>>,
}

/// What a block needs while it draws.
struct Draw<'a> {
    st: &'a Style,
    scale: f32,
    dir: &'a Path,
    reader: WeakEntity<Reader>,
    /// Find query, lower case, for highlighting.
    query: Option<String>,
    block: usize,
    seq: Cell<u64>,
    open: &'a HashSet<u64>,
    diagrams: &'a super::mermaid::Diagrams,
    /// Text selection: the selected range, where units report in, and the
    /// next unit's number in this block.
    sel: Option<(super::md_select::End, super::md_select::End)>,
    units: std::rc::Rc<std::cell::RefCell<Vec<super::md_select::Unit>>>,
    texts: std::rc::Rc<
        std::cell::RefCell<std::collections::BTreeMap<super::md_select::Key, SharedString>>,
    >,
    unit: Cell<u32>,
    code_scroll:
        std::rc::Rc<std::cell::RefCell<std::collections::HashMap<u64, gpui::ScrollHandle>>>,
    /// The code block whose scroll bar is being dragged.
    hdrag: Option<u64>,
}

impl Draw<'_> {
    /// The next text unit in this block: its key, and its selected part.
    fn unit(&self, len: usize) -> (super::md_select::Key, Option<Range<usize>>) {
        let n = self.unit.get();
        self.unit.set(n + 1);
        let key = (self.block, n);
        let part = self.sel.and_then(|((k0, o0), (k1, o1))| {
            if key < k0 || key > k1 {
                return None;
            }
            let a = if key == k0 { o0.min(len) } else { 0 };
            let b = if key == k1 { o1.min(len) } else { len };
            (a < b).then_some(a..b)
        });
        (key, part)
    }

    /// A drawn unit reports its laid-out text, for hit-testing and copying.
    fn report(&self, key: super::md_select::Key, styled: &StyledText, text: SharedString) {
        self.units.borrow_mut().push(super::md_select::Unit {
            key,
            layout: styled.layout().clone(),
        });
        self.texts.borrow_mut().insert(key, text);
    }

    fn sel_color(&self) -> Hsla {
        self.st.c(|c| c.accent).opacity(0.3)
    }

    fn id(&self, name: &'static str) -> gpui::ElementId {
        let n = self.seq.get();
        self.seq.set(n + 1);
        gpui::ElementId::NamedInteger(name.into(), ((self.block as u64) << 20) | n)
    }
    fn px(&self, v: f32) -> gpui::Pixels {
        px(v * self.scale)
    }
}

/// Base look of a run of text in a block.
#[derive(Clone, Copy)]
struct Base {
    size: f32,
    bold: bool,
    italic: bool,
    color: Hsla,
}

fn sup_char(c: char) -> char {
    match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        c => c,
    }
}

fn sub_char(c: char) -> char {
    match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        c => c,
    }
}

fn font(family: &'static str, bold: bool, italic: bool) -> Font {
    let mut f = gpui::font(family);
    if bold {
        f.weight = FontWeight::BOLD;
    }
    if italic {
        f.style = FontStyle::Italic;
    }
    f
}

/// Byte ranges of `query` in `text`, ignoring case (ASCII and simple Unicode).
fn matches(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let lower = text.to_lowercase();
    // Lower-casing can change lengths; only highlight when it didn't.
    if lower.len() != text.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = lower[from..].find(query) {
        out.push(from + i..from + i + query.len());
        from += i + query.len().max(1);
    }
    out
}

impl Reader {
    /// Text scale for Markdown (zoom changes the type size). A file without
    /// a zoom of its own, or put back to the default size (Ctrl+2), reads at
    /// the Markdown text size from Settings; Actual size (Ctrl+0) is 100 %.
    pub(super) fn md_scale(&self) -> f32 {
        match self.zoom {
            kitab_core::ZoomMode::Custom(z) => z,
            kitab_core::ZoomMode::ActualSize => 1.0,
            kitab_core::ZoomMode::FitWidth | kitab_core::ZoomMode::FitPage => self.md_base,
        }
    }

    /// Follow a link in a Markdown document.
    fn md_follow(&mut self, target: &str, cx: &mut Context<Self>) {
        let Some(md) = self.md.as_ref() else { return };
        if let Some(label) = target.strip_prefix("fn:") {
            let at = md
                .doc
                .blocks
                .iter()
                .position(|b| matches!(b, Block::Footnote { label: l, .. } if l == label));
            if let Some(at) = at {
                self.jump(at, cx);
            }
            return;
        }
        if target.starts_with('#') {
            if let Some(at) = md.doc.anchor(target) {
                self.jump(at, cx);
            }
            return;
        }
        let lower = target.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:")
        {
            cx.open_url(target);
            return;
        }
        // A relative path: open it in Kitab (another tab).
        let path = target.split('#').next().unwrap_or("").replace("%20", " ");
        if !path.is_empty() {
            cx.emit(ReaderEvent::OpenPath(md.dir.join(path)));
        }
    }

    /// Text of block `ix`, for Copy.
    pub(super) fn md_block_text(&self, ix: usize) -> Option<String> {
        let md = self.md.as_ref()?;
        md.doc.blocks.get(ix).map(plain_text)
    }

    pub(super) fn md_element(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let st = Style::get(cx).clone();
        let scale = self.md_scale();
        let Some(md) = self.md.as_ref() else {
            return div().into_any_element();
        };
        let Some(block) = md.doc.blocks.get(ix) else {
            // An empty file still gets a page, saying so.
            return div()
                .w_full()
                .pt(px(120.0))
                .flex()
                .justify_center()
                .text_color(st.c(|c| c.faint))
                .child("This Markdown file is empty.")
                .into_any_element();
        };
        // Diagrams in this block are asked for before it draws.
        if let Block::Mermaid(src) = block {
            let src = src.clone();
            self.diagram(&src, &st, cx);
        }
        let Some(md) = self.md.as_ref() else {
            return div().into_any_element();
        };
        let Some(block) = md.doc.blocks.get(ix) else {
            return div().into_any_element();
        };
        let query = (self.find.open && !self.find.closing && !self.find.query.trim().is_empty())
            .then(|| self.find.query.trim().to_lowercase());
        let d = Draw {
            st: &st,
            scale,
            dir: &md.dir,
            reader: cx.weak_entity(),
            query,
            block: ix,
            seq: Cell::new(0),
            open: &md.open_details,
            diagrams: &md.diagrams,
            sel: self.md_sel.range(),
            units: self.md_sel.units.clone(),
            texts: self.md_sel.texts.clone(),
            unit: Cell::new(0),
            code_scroll: md.code_scroll.clone(),
            hdrag: self.md_hdrag.map(|g| g.key),
        };
        let el = block_el(&d, block, 0, cx);
        let last = ix + 1 == md.doc.blocks.len();
        // Read aloud: the block being read is shaded, with a rule beside it.
        let reading = self.speech_block() == Some(ix);
        let column = div()
            .w_full()
            .max_w(px(COLUMN * scale.max(0.5)))
            .when(reading, |d| {
                d.bg(st.c(|c| c.accent).opacity(0.10))
                    .border_l_2()
                    .border_color(st.c(|c| c.accent))
                    .rounded(px(4.0))
                    .ml(px(-10.0))
                    .pl(px(8.0))
            })
            .child(el);
        div()
            .w_full()
            .flex()
            .justify_center()
            .px(px(28.0))
            .pt(px(if ix == 0 { 40.0 + self.top_inset } else { 0.0 }))
            .pb(px(if last { 120.0 } else { 0.0 }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|r, e: &gpui::MouseDownEvent, window, cx| {
                    // Keys (Ctrl+C, Ctrl+D, …) go to this tab after a click.
                    window.focus(&r.focus);
                    r.md_select_down(e, cx)
                }),
            )
            .child(column)
            .into_any_element()
    }
}

fn block_el(d: &Draw, block: &Block, depth: usize, cx: &mut Context<Reader>) -> AnyElement {
    let st = d.st;
    let body = Base {
        size: BODY,
        bold: false,
        italic: false,
        color: st.c(|c| c.read),
    };
    match block {
        Block::Heading { level, content, .. } => {
            let (size, top) = match level {
                1 => (2.0, 28.0),
                2 => (1.55, 26.0),
                3 => (1.28, 22.0),
                4 => (1.12, 18.0),
                5 => (1.0, 16.0),
                _ => (0.92, 16.0),
            };
            let base = Base {
                size: BODY * size,
                bold: true,
                italic: false,
                color: if *level >= 6 {
                    st.c(|c| c.muted)
                } else {
                    st.c(|c| c.text)
                },
            };
            div()
                .pt(d.px(top))
                .pb(d.px(10.0))
                .when(*level <= 2, |el| {
                    el.border_b_1()
                        .border_color(st.c(|c| c.line))
                        .mb(d.px(12.0))
                })
                .child(inline_el(d, content, base, cx))
                .into_any_element()
        }
        Block::Paragraph(content) => div()
            // Inside list items paragraphs sit close (tight lists).
            .pb(d.px(if depth > 0 { 4.0 } else { 14.0 }))
            .child(inline_el(d, content, body, cx))
            .into_any_element(),
        Block::List { start, items } => list_el(d, *start, items, depth, cx),
        Block::Quote(blocks) => div()
            .mb(d.px(14.0))
            .pl(d.px(16.0))
            .border_l_4()
            .border_color(st.c(|c| c.accent).opacity(0.5))
            .italic()
            .text_color(st.c(|c| c.muted))
            .children(blocks_el(d, blocks, depth + 1, cx))
            .into_any_element(),
        Block::Code { lang, text, tokens } => code_el(d, lang, text, tokens, cx),
        Block::Mermaid(src) => mermaid_el(d, src),
        Block::Math(src) => match kitab_md::math::parse(src) {
            Ok(node) => super::math_view::display(d.st, &node, BODY * d.scale * 1.15),
            Err(e) => source_box(d, "Math", &format!("Can't read this TeX: {e}"), src),
        },
        Block::Table { aligns, head, rows } => table_el(d, aligns, head, rows, cx),
        Block::Rule => div()
            .my(d.px(18.0))
            .h(px(1.0))
            .bg(st.c(|c| c.line))
            .into_any_element(),
        Block::Details {
            summary,
            body: blocks,
        } => details_el(d, summary, blocks, depth, cx),
        Block::Footnote {
            number,
            body: blocks,
            ..
        } => div()
            .flex()
            .gap(d.px(10.0))
            .mt(d.px(4.0))
            .text_size(d.px(BODY * 0.85))
            .text_color(st.c(|c| c.muted))
            .child(div().flex_none().child(format!("{number}.")))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .children(blocks_el(d, blocks, depth + 1, cx)),
            )
            .into_any_element(),
        Block::Frontmatter(pairs) => {
            let mut grid = div().flex().flex_col().gap(d.px(4.0)).text_size(d.px(13.0));
            for (k, v) in pairs {
                grid = grid.child(
                    div()
                        .flex()
                        .gap(d.px(16.0))
                        .child(
                            div()
                                .w(d.px(120.0))
                                .flex_none()
                                .text_color(st.c(|c| c.faint))
                                .child(k.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(st.c(|c| c.text))
                                .child(v.clone()),
                        ),
                );
            }
            div()
                .mb(d.px(20.0))
                .p(d.px(14.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(st.c(|c| c.line))
                .bg(st.c(|c| c.panel))
                .child(grid)
                .into_any_element()
        }
    }
}

fn blocks_el(
    d: &Draw,
    blocks: &[Block],
    depth: usize,
    cx: &mut Context<Reader>,
) -> Vec<AnyElement> {
    blocks
        .iter()
        .map(|b| {
            // Past the limit, nesting is drawn flat (hostile files).
            let depth = depth.min(MAX_DRAW_DEPTH);
            block_el(d, b, depth, cx)
        })
        .collect()
}

fn list_el(
    d: &Draw,
    start: Option<u64>,
    items: &[Item],
    depth: usize,
    cx: &mut Context<Reader>,
) -> AnyElement {
    let st = d.st;
    let mut col = div()
        .flex()
        .flex_col()
        .gap(d.px(2.0))
        .mb(d.px(if depth > 0 { 2.0 } else { 12.0 }));
    let indent = if depth >= MAX_DRAW_DEPTH { 0.0 } else { 26.0 };
    for (i, item) in items.iter().enumerate() {
        let marker: AnyElement = match (item.task, start) {
            (Some(done), _) => div()
                .mt(d.px(5.0))
                .size(d.px(15.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .border_1()
                .border_color(if done {
                    st.c(|c| c.accent)
                } else {
                    st.c(|c| c.ring)
                })
                .when(done, |m| m.bg(st.c(|c| c.accent)))
                .when(done, |m| {
                    m.child(
                        svg()
                            .path(icon("check"))
                            .size(d.px(11.0))
                            .text_color(st.c(|c| c.bg)),
                    )
                })
                .into_any_element(),
            (None, Some(n)) => div()
                .text_color(st.c(|c| c.muted))
                .text_size(d.px(BODY))
                .child(format!("{}.", n + i as u64))
                .into_any_element(),
            (None, None) => div()
                .text_color(st.c(|c| c.muted))
                .text_size(d.px(BODY))
                .child(match depth % 3 {
                    0 => "•",
                    1 => "◦",
                    _ => "▪",
                })
                .into_any_element(),
        };
        col = col.child(
            div()
                .flex()
                .child(
                    div()
                        .w(d.px(indent))
                        .flex_none()
                        .flex()
                        .justify_end()
                        .pr(d.px(8.0))
                        .child(marker),
                )
                .child(div().flex_1().min_w_0().children(blocks_el(
                    d,
                    &item.blocks,
                    depth + 1,
                    cx,
                ))),
        );
    }
    col.into_any_element()
}

/// Theme colour for a kind of code token.
pub(crate) fn token_color(st: &Style, kind: TokenKind) -> Hsla {
    if let Some(p) = st.code_palette() {
        let c = match kind {
            TokenKind::Plain => p.plain,
            TokenKind::Keyword => p.keyword,
            TokenKind::String => p.string,
            TokenKind::Comment => p.comment,
            TokenKind::Number => p.number,
            TokenKind::Function => p.function,
            TokenKind::Type => p.type_,
            TokenKind::Tag => p.tag,
            TokenKind::Punctuation => p.punctuation,
        };
        return crate::style::hsla(c);
    }
    match kind {
        TokenKind::Plain => st.c(|c| c.text),
        TokenKind::Keyword => st.c(|c| c.accent),
        TokenKind::String => st.c(|c| c.ok),
        TokenKind::Comment => st.c(|c| c.faint),
        TokenKind::Number => st.c(|c| c.warn),
        TokenKind::Function => st.c(|c| c.read),
        TokenKind::Type => st.c(|c| c.warn).opacity(0.85),
        TokenKind::Tag => st.c(|c| c.bad).opacity(0.85),
        TokenKind::Punctuation => st.c(|c| c.muted),
    }
}

/// One line of code with its colours (token offsets are into the whole block).
fn code_line(d: &Draw, line: &str, at: usize, tokens: &[Token]) -> StyledText {
    let st = d.st;
    let shown = line.replace('\t', "    ");
    let (key, part) = d.unit(shown.len());
    let finish = |runs: Vec<TextRun>| {
        let text = SharedString::from(shown.clone());
        let styled = if runs.is_empty() {
            StyledText::new(text.clone())
        } else {
            StyledText::new(text.clone()).with_runs(super::md_select::select_runs(
                runs,
                part.clone(),
                d.sel_color(),
            ))
        };
        d.report(key, &styled, text);
        styled
    };
    let plain_run = |len: usize| TextRun {
        len,
        font: font(MONO_FONT, false, false),
        color: token_color(st, TokenKind::Plain),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    if tokens.is_empty() || shown.len() != line.len() {
        // No colours, or tabs moved the offsets: one plain run (a run only
        // when part of it is selected).
        return finish(if part.is_some() && !shown.is_empty() {
            vec![plain_run(shown.len())]
        } else {
            Vec::new()
        });
    }
    let end = at + line.len();
    let plain = font(MONO_FONT, false, false);
    let run = |len: usize, kind: TokenKind| TextRun {
        len,
        font: match kind {
            TokenKind::Comment => font(MONO_FONT, false, true),
            TokenKind::Function => font(MONO_FONT, true, false),
            _ => plain.clone(),
        },
        color: token_color(st, kind),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut runs = Vec::new();
    let mut pos = at;
    for t in tokens.iter().filter(|t| t.end > at && t.start < end) {
        let (s, e) = (t.start.max(pos), t.end.min(end));
        if s > pos {
            runs.push(run(s - pos, TokenKind::Plain));
        }
        if e > s {
            runs.push(run(e - s, t.kind));
            pos = e;
        }
    }
    if end > pos {
        runs.push(run(end - pos, TokenKind::Plain));
    }
    // Runs must cover the line exactly and end on character boundaries.
    let mut b = 0;
    let ok = runs.iter().all(|r| {
        b += r.len;
        line.is_char_boundary(b)
    }) && b == line.len();
    if ok && !runs.is_empty() {
        finish(runs)
    } else if part.is_some() && !shown.is_empty() {
        finish(vec![plain_run(shown.len())])
    } else {
        finish(Vec::new())
    }
}

fn code_el(
    d: &Draw,
    lang: &str,
    text: &str,
    tokens: &[Token],
    cx: &mut Context<Reader>,
) -> AnyElement {
    let st = d.st;
    let copy_text = text.to_string();
    // Lines wider than the column scroll sideways, with a bar to show it.
    let code_key = ((d.block as u64) << 20) | d.seq.get();
    let scroll = d
        .code_scroll
        .borrow_mut()
        .entry(code_key)
        .or_default()
        .clone();
    let bar = code_hbar(code_key, &scroll, d.hdrag == Some(code_key), st, cx);
    div()
        .mb(d.px(16.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(st.c(|c| c.line))
        .bg(st
            .code_palette()
            .map_or(st.c(|c| c.panel), |p| crate::style::hsla(p.background)))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(d.px(12.0))
                .h(d.px(30.0))
                .border_b_1()
                .border_color(st.c(|c| c.line))
                .text_size(d.px(12.0))
                .text_color(st.c(|c| c.faint))
                .child(div().min_w_0().truncate().child(if lang.is_empty() {
                    "text".to_string()
                } else {
                    lang.to_string()
                }))
                .child({
                    let key = copy_key(&copy_text);
                    let done = copied_recently(key);
                    div()
                        .id(d.id("md-copy"))
                        .group("md-copy")
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(d.px(24.0))
                        .rounded(px(5.0))
                        .cursor_pointer()
                        .text_color(if done {
                            st.c(|c| c.accent)
                        } else {
                            st.c(|c| c.faint)
                        })
                        .hover(|b| b.bg(st.hover_bg()).text_color(st.c(|c| c.text)))
                        .tip(if done { "Copied" } else { "Copy code" })
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                copy_text.clone(),
                            ));
                            // A check for a moment, then back to the copy icon.
                            if let Ok(mut c) = COPIED.lock() {
                                *c = Some((key, std::time::Instant::now()));
                            }
                            cx.refresh_windows();
                            let timer = cx.background_executor().timer(COPIED_FOR);
                            cx.spawn(async move |cx| {
                                timer.await;
                                let _ = cx.update(|cx| cx.refresh_windows());
                            })
                            .detach();
                        })
                        .child(
                            gpui::svg()
                                .path(crate::icons::icon(if done { "check" } else { "copy" }))
                                .size(d.px(14.0))
                                .text_color(if done {
                                    st.c(|c| c.accent)
                                } else {
                                    st.c(|c| c.faint)
                                })
                                .when(!done, |s| {
                                    s.group_hover("md-copy", |s| s.text_color(st.c(|c| c.text)))
                                }),
                        )
                }),
        )
        .child(
            div()
                .id(d.id("md-code"))
                .overflow_x_scroll()
                .track_scroll(&scroll)
                // A plain wheel scrolls the page, not the block sideways;
                // Shift+wheel (or a sideways swipe) scrolls the block.
                .map(|mut el| {
                    el.style().restrict_scroll_to_axis = Some(true);
                    el
                })
                .on_scroll_wheel({
                    let scroll = scroll.clone();
                    move |e: &gpui::ScrollWheelEvent, window, cx| {
                        if !e.modifiers.shift {
                            return;
                        }
                        let delta = e.delta.pixel_delta(window.line_height());
                        if delta.x.abs() > px(0.0) {
                            return;
                        }
                        let max = scroll.max_offset().width;
                        let mut at = scroll.offset();
                        at.x = (at.x + delta.y).clamp(-max, px(0.0));
                        scroll.set_offset(at);
                        cx.stop_propagation();
                        window.refresh();
                    }
                })
                .p(d.px(14.0))
                // The lines' box is as wide as the longest line (not the
                // column), so there is something to scroll to.
                .flex()
                .child(
                    div()
                        .flex_none()
                        .min_w_full()
                        .whitespace_nowrap()
                        .font_family(MONO_FONT)
                        .text_size(d.px(BODY * 0.85))
                        .line_height(relative(1.5))
                        .text_color(token_color(st, TokenKind::Plain))
                        .children({
                            let mut at = 0;
                            text.split('\n')
                                .map(|line| {
                                    let el = div()
                                        .min_h(d.px(BODY * 0.85 * 1.5))
                                        .cursor(gpui::CursorStyle::IBeam)
                                        .child(code_line(d, line, at, tokens));
                                    at += line.len() + 1;
                                    el
                                })
                                .collect::<Vec<_>>()
                        }),
                ),
        )
        .children(bar)
        .into_any_element()
}

/// The sideways scroll bar under a code block, when its lines are wider
/// than the column: drag the thumb, or click the track to jump.
fn code_hbar(
    key: u64,
    scroll: &gpui::ScrollHandle,
    held: bool,
    st: &Style,
    cx: &mut Context<Reader>,
) -> Option<AnyElement> {
    let max = f32::from(scroll.max_offset().width);
    let view = f32::from(scroll.bounds().size.width);
    if view < 1.0 {
        // Not measured yet (it just came on screen): look again next frame.
        cx.notify();
        return None;
    }
    if max < 1.0 {
        return None;
    }
    let shown = (view / (view + max)).clamp(0.08, 1.0);
    let at = (-f32::from(scroll.offset().x) / max).clamp(0.0, 1.0);
    let thumb = div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(relative(at * (1.0 - shown)))
        .w(relative(shown))
        .rounded_full()
        .bg(st.c(|c| c.muted).opacity(if held { 0.6 } else { 0.35 }))
        .hover(|d| d.bg(st.c(|c| c.muted).opacity(0.6)));
    let track = div()
        .id(gpui::ElementId::NamedInteger("md-code-bar".into(), key))
        .relative()
        .mx(px(12.0))
        .mb(px(6.0))
        .h(px(6.0))
        .cursor_default()
        .on_mouse_down(gpui::MouseButton::Left, {
            let scroll = scroll.clone();
            cx.listener(move |r, e: &gpui::MouseDownEvent, _, cx| {
                cx.stop_propagation();
                // Off the thumb: jump so the thumb centres on the pointer.
                let b = scroll.bounds();
                let track_w = f32::from(b.size.width) - 24.0;
                let x = f32::from(e.position.x - b.left()) - 12.0;
                let thumb_w = track_w * shown;
                let left = track_w * at * (1.0 - shown);
                if x < left || x > left + thumb_w {
                    let frac = ((x - thumb_w / 2.0) / (track_w - thumb_w).max(1.0)).clamp(0.0, 1.0);
                    let mut o = scroll.offset();
                    o.x = px(-frac * max);
                    scroll.set_offset(o);
                }
                r.md_hdrag = Some(super::HDrag {
                    key,
                    from_x: f32::from(e.position.x),
                    from_offset: -f32::from(scroll.offset().x),
                });
                cx.notify();
            })
        })
        .child(thumb);
    Some(track.into_any_element())
}

/// A Mermaid diagram: the drawing when ready, a quiet frame meanwhile, the
/// source and the reason when it can't be drawn.
fn mermaid_el(d: &Draw, src: &str) -> AnyElement {
    let k = super::mermaid::key(src, d.st, d.scale);
    match d.diagrams.map.get(&k) {
        Some(super::mermaid::Diagram::Ready { image, w, h }) => {
            // Shown at its size, or shrunk to the text column keeping its shape.
            let column = COLUMN * d.scale.max(0.5);
            let (full_w, full_h) = (w * d.scale, h * d.scale);
            let w = full_w.min(column);
            let h = full_h * w / full_w.max(1.0);
            div()
                .mb(d.px(16.0))
                .w_full()
                .flex()
                .justify_center()
                .child(
                    img(gpui::ImageSource::Render(image.clone()))
                        .w(px(w))
                        .h(px(h)),
                )
                .into_any_element()
        }
        Some(super::mermaid::Diagram::Failed(e)) => {
            source_box(d, "Mermaid diagram", &format!("Can't draw this: {e}"), src)
        }
        _ => div()
            .mb(d.px(16.0))
            .h(d.px(120.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(d.st.c(|c| c.line))
            .into_any_element(),
    }
}

fn source_box(d: &Draw, title: &'static str, note: &str, src: &str) -> AnyElement {
    let st = d.st;
    div()
        .mb(d.px(16.0))
        .p(d.px(14.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(st.c(|c| c.line))
        .flex()
        .flex_col()
        .gap(d.px(8.0))
        .child(
            div()
                .flex()
                .gap(d.px(8.0))
                .text_size(d.px(12.0))
                .child(
                    div()
                        .flex_none()
                        .text_color(st.c(|c| c.accent))
                        .child(title),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(st.c(|c| c.faint))
                        .child(note.to_string()),
                ),
        )
        .child(
            div()
                .font_family(if title == "Math" {
                    MATH_FONT
                } else {
                    MONO_FONT
                })
                .text_size(d.px(BODY * if title == "Math" { 1.0 } else { 0.85 }))
                .text_color(st.c(|c| c.text))
                .when(title == "Math", |el| el.flex().justify_center().italic())
                .children(src.split('\n').map(|l| div().child(l.to_string()))),
        )
        .into_any_element()
}

/// How a table's columns share the reading column. `longest` is each
/// column's longest cell and `word` its longest unbreakable run (a URL, a
/// path), both in characters; `avail` is how many characters fit across.
/// Columns get room by how much they hold, and never less than their
/// longest word. `None` when even that doesn't fit: the table scrolls.
/// The least room a column takes, in characters: its longest word (a long
/// URL or path may still break, so only up to 16 count), a little padding.
fn col_need(word: usize) -> f32 {
    word.clamp(6, 16) as f32 + 4.0
}

fn column_shares(longest: &[usize], word: &[usize], avail: f32) -> Option<Vec<f32>> {
    let n = longest.len();
    if n == 0 || avail <= 0.0 {
        return None;
    }
    // Room each column needs at least (its longest word, a few characters
    // at minimum) and what it would like (its longest cell, within reason).
    let pad = 4.0;
    let need: Vec<f32> = word.iter().map(|&w| col_need(w)).collect();
    let want: Vec<f32> = longest
        .iter()
        .zip(&need)
        .map(|(&l, &m)| ((l.min(70) as f32) + pad).max(m))
        .collect();
    let need_sum: f32 = need.iter().sum();
    if need_sum > avail {
        return None;
    }
    let want_sum: f32 = want.iter().sum();
    let widths: Vec<f32> = if want_sum <= avail {
        // Everything fits on one line: spread the spare room by size.
        want.iter().map(|w| w / want_sum * avail).collect()
    } else {
        // Each column gets what it needs, and the rest goes by how much
        // more it would like.
        let spare = avail - need_sum;
        let extra: Vec<f32> = want.iter().zip(&need).map(|(w, m)| w - m).collect();
        let extra_sum: f32 = extra.iter().sum::<f32>().max(0.001);
        need.iter()
            .zip(&extra)
            .map(|(m, e)| m + spare * e / extra_sum)
            .collect()
    };
    Some(widths.iter().map(|w| w / avail).collect())
}

fn table_el(
    d: &Draw,
    aligns: &[Align],
    head: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    cx: &mut Context<Reader>,
) -> AnyElement {
    let st = d.st;
    let cols = head
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0))
        .max(1);
    let base = Base {
        size: BODY * 0.92,
        bold: false,
        italic: false,
        color: st.c(|c| c.read),
    };
    let cell = |content: &[Inline], col: usize, header: bool, cx: &mut Context<Reader>| {
        let align = aligns.get(col).copied().unwrap_or(Align::None);
        div()
            .px(d.px(10.0))
            .py(d.px(6.0))
            .border_b_1()
            .border_color(st.c(|c| c.line))
            .when(header, |c| c.bg(st.c(|c| c.panel)))
            // Text wraps inside its cell; nothing spills into the next one.
            .overflow_hidden()
            .flex()
            .when(align == Align::Center, |c| c.justify_center())
            .when(align == Align::Right, |c| c.justify_end())
            .child(div().min_w_0().max_w_full().child(inline_el(
                d,
                content,
                Base {
                    bold: header,
                    ..base
                },
                cx,
            )))
    };
    // How much each column holds, in characters: its longest cell and its
    // longest word (which can't wrap).
    let mut longest = vec![0usize; cols];
    let mut word = vec![0usize; cols];
    for row in std::iter::once(head).chain(rows.iter().map(Vec::as_slice)) {
        for (i, c) in row.iter().enumerate().take(cols) {
            let text = kitab_md::inline_text(c);
            longest[i] = longest[i].max(text.chars().count());
            let w = text
                .split_whitespace()
                .map(|w| w.chars().count())
                .max()
                .unwrap_or(0);
            word[i] = word[i].max(w);
        }
    }
    // About how many characters fit across the reading column.
    // Bold header text and wide letters run wider than the average: err
    // on the generous side, so a short word never breaks mid-word.
    let char_w = BODY * 0.92 * 0.62 * d.scale;
    let avail = (COLUMN * d.scale - cols as f32 * 20.0 * d.scale) / char_w;
    let shares = column_shares(&longest, &word, avail);
    // Too wide even so: columns as wide as their text (within reason), and
    // the table scrolls sideways in its own box.
    let fixed: Vec<f32> = longest
        .iter()
        .zip(&word)
        .map(|(&l, &w)| {
            let chars = l.min(48).max(w.min(60)) as f32;
            (chars * char_w + 20.0 * d.scale).clamp(90.0 * d.scale, 440.0 * d.scale)
        })
        .collect();
    let wide = shares.is_none();
    // Each column's least width in pixels: a narrower window scrolls the
    // table rather than breaking short words.
    let mins: Vec<f32> = word
        .iter()
        .map(|&w| col_need(w) * char_w + 20.0 * d.scale)
        .collect();
    let min_total: f32 = mins.iter().sum();
    let sized = |c: gpui::Div, i: usize| match &shares {
        // Its share of the row to start from; it gives way when the row is
        // tight, but never below what its longest word needs.
        Some(s) => c
            .flex_basis(gpui::relative(s.get(i).copied().unwrap_or(0.0)))
            .flex_grow()
            .flex_shrink()
            .min_w(px(mins.get(i).copied().unwrap_or(60.0))),
        None => c.w(px(fixed.get(i).copied().unwrap_or(160.0))).flex_none(),
    };
    let mut table = div()
        .flex()
        .flex_col()
        .when(wide, |t| t.w(px(fixed.iter().sum::<f32>())))
        .when(!wide, |t| t.w_full().min_w(px(min_total)));
    let mut head_row = div().flex().w_full();
    for i in 0..cols {
        let empty = Vec::new();
        head_row = head_row.child(sized(cell(head.get(i).unwrap_or(&empty), i, true, cx), i));
    }
    table = table.child(head_row);
    for (r, row) in rows.iter().enumerate() {
        let mut line = div().flex().w_full();
        for i in 0..cols {
            let empty = Vec::new();
            let c = cell(row.get(i).unwrap_or(&empty), i, false, cx);
            // Alternate rows get a faint tint (md-viewer-pwa).
            line = line.child(sized(
                c.when(r % 2 == 1, |c| c.bg(st.c(|c| c.hover).opacity(0.5))),
                i,
            ));
        }
        table = table.child(line);
    }
    let grid = table;
    div()
        .id(d.id("md-table"))
        .mb(d.px(16.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(st.c(|c| c.line))
        .overflow_x_scroll()
        // The wheel scrolls the page; Shift+wheel or a sideways swipe this box.
        .map(|mut el| {
            el.style().restrict_scroll_to_axis = Some(true);
            el
        })
        .child(grid)
        .into_any_element()
}

fn details_el(
    d: &Draw,
    summary: &[Inline],
    blocks: &[Block],
    depth: usize,
    cx: &mut Context<Reader>,
) -> AnyElement {
    let st = d.st;
    let key = ((d.block as u64) << 20) | d.seq.get();
    let open = d.open.contains(&key);
    let reader = d.reader.clone();
    let base = Base {
        size: BODY,
        bold: true,
        italic: false,
        color: st.c(|c| c.text),
    };
    div()
        .mb(d.px(14.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(st.c(|c| c.line))
        .child(
            div()
                .id(d.id("md-details"))
                .flex()
                .items_center()
                .gap(d.px(8.0))
                .px(d.px(12.0))
                .py(d.px(8.0))
                .cursor_pointer()
                .hover(|h| h.bg(st.hover_bg()))
                .on_click(move |_, _, cx| {
                    let _ = reader.update(cx, |r, cx| {
                        if let Some(md) = r.md.as_mut()
                            && !md.open_details.remove(&key)
                        {
                            md.open_details.insert(key);
                        }
                        cx.notify();
                    });
                })
                .child(
                    svg()
                        .path(icon(if open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        }))
                        .size(d.px(14.0))
                        .text_color(st.c(|c| c.muted)),
                )
                // A long summary wraps beside its arrow.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(inline_el(d, summary, base, cx)),
                ),
        )
        .when(open, |el| {
            el.child(div().px(d.px(14.0)).pt(d.px(6.0)).children(blocks_el(
                d,
                blocks,
                depth + 1,
                cx,
            )))
        })
        .into_any_element()
}

/// A run of inline content as wrapped text, with links clickable. Images
/// split the run and show on their own line.
fn inline_el(d: &Draw, inlines: &[Inline], base: Base, cx: &mut Context<Reader>) -> AnyElement {
    let mut parts: Vec<AnyElement> = Vec::new();
    let mut start = 0;
    for (i, inl) in inlines.iter().enumerate() {
        if let Inline::Image { src, alt } = inl {
            if i > start {
                parts.push(text_el(d, &inlines[start..i], base, cx));
            }
            parts.push(image_el(d, src, alt));
            start = i + 1;
        }
    }
    if start < inlines.len() {
        parts.push(text_el(d, &inlines[start..], base, cx));
    }
    if parts.len() == 1 {
        parts.pop().unwrap_or_else(|| div().into_any_element())
    } else {
        div()
            .flex()
            .flex_col()
            .gap(d.px(8.0))
            .children(parts)
            .into_any_element()
    }
}

fn image_el(d: &Draw, src: &str, alt: &str) -> AnyElement {
    let st = d.st;
    let lower = src.to_ascii_lowercase();
    let placeholder = |text: String| {
        div()
            .my(d.px(6.0))
            .px(d.px(12.0))
            .py(d.px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(st.c(|c| c.line))
            .text_size(d.px(13.0))
            .text_color(st.c(|c| c.faint))
            .child(text)
            .into_any_element()
    };
    if lower.starts_with("http://") || lower.starts_with("https://") {
        let host = src.split('/').nth(2).unwrap_or(src);
        return placeholder(format!("Remote image from {host} is blocked. {alt}"));
    }
    let clean = src
        .split(['?', '#'])
        .next()
        .unwrap_or(src)
        .replace("%20", " ");
    let path = d.dir.join(&clean);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let media = match ext.as_str() {
        "mp4" | "webm" | "ogv" | "mov" | "m4v" => Some("Video"),
        "mp3" | "wav" | "ogg" | "oga" | "m4a" | "aac" | "flac" => Some("Audio"),
        _ => None,
    };
    if let Some(kind) = media {
        // Kitab doesn't play media: offer the system's player (md-viewer-pwa
        // shows a player here).
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let exists = path.is_file();
        return div()
            .id(d.id("md-media"))
            .my(d.px(6.0))
            .px(d.px(12.0))
            .py(d.px(10.0))
            .flex()
            .items_center()
            .gap(d.px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(st.c(|c| c.line))
            .text_size(d.px(13.0))
            .text_color(st.c(|c| c.muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(format!("{kind}: {name}")),
            )
            .when(exists, |el| {
                el.child(
                    div()
                        .flex_none()
                        .text_color(st.c(|c| c.accent))
                        .cursor_pointer()
                        .child("Open")
                        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx: &mut App| {
                            cx.open_with_system(&path)
                        }),
                )
            })
            .when(!exists, |el| el.child("(not found)"))
            .into_any_element();
    }
    if path.is_file() {
        img(path).max_w_full().rounded(px(6.0)).into_any_element()
    } else {
        placeholder(format!(
            "Image not found: {}",
            if alt.is_empty() { src } else { alt }
        ))
    }
}

fn text_el(d: &Draw, inlines: &[Inline], base: Base, _cx: &mut Context<Reader>) -> AnyElement {
    let st = d.st;
    let mut text = String::new();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut links: Vec<(Range<usize>, String)> = Vec::new();
    let highlight = st.c(|c| c.highlight);
    let mut push = |text: &mut String, piece: &str, style: SpanStyle, link: Option<&String>| {
        if piece.is_empty() {
            return;
        }
        // Inline math as one line of Unicode (x², α ≤ β); TeX that can't be
        // read stays as its source, marked.
        let mut bad_math = false;
        let piece: String = if style.math {
            match kitab_md::math::parse(piece) {
                Ok(node) => kitab_md::math::to_line(&node),
                Err(_) => {
                    bad_math = true;
                    format!("${piece}$ ⚠")
                }
            }
        } else if style.sup {
            piece.chars().map(sup_char).collect()
        } else if style.sub {
            piece.chars().map(sub_char).collect()
        } else {
            piece.to_string()
        };
        let mono = style.code || style.kbd;
        let family = if mono {
            MONO_FONT
        } else if style.math {
            MATH_FONT
        } else {
            st.md_face.family()
        };
        let color = if bad_math {
            st.c(|c| c.bad)
        } else if link.is_some() {
            st.c(|c| c.accent)
        } else if mono {
            st.c(|c| c.text)
        } else {
            base.color
        };
        let bg = (mono).then(|| st.c(|c| c.hover));
        let font = font(
            family,
            base.bold || style.bold,
            base.italic || style.italic || style.math,
        );
        let at = text.len();
        // Find hits get the highlight colour.
        let hits = d
            .query
            .as_deref()
            .map(|q| matches(&piece, q))
            .unwrap_or_default();
        let mut cursor = 0;
        let add = |len: usize, hit: bool, runs: &mut Vec<TextRun>| {
            if len == 0 {
                return;
            }
            runs.push(TextRun {
                len,
                font: font.clone(),
                color,
                background_color: if hit { Some(highlight) } else { bg },
                underline: link.map(|_| UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(st.c(|c| c.accent).opacity(0.5)),
                    wavy: false,
                }),
                strikethrough: style.strike.then(|| StrikethroughStyle {
                    thickness: px(1.0),
                    color: Some(color),
                }),
            });
        };
        for h in hits {
            add(h.start - cursor, false, &mut runs);
            add(h.end - h.start, true, &mut runs);
            cursor = h.end;
        }
        add(piece.len() - cursor, false, &mut runs);
        text.push_str(&piece);
        if let Some(l) = link {
            links.push((at..text.len(), l.clone()));
        }
    };
    for inl in inlines {
        match inl {
            Inline::Text {
                text: t,
                style,
                link,
            } => push(&mut text, t, *style, link.as_ref()),
            Inline::Break => push(&mut text, "\n", SpanStyle::default(), None),
            Inline::FootnoteRef { label, number } => {
                let target = format!("fn:{label}");
                let sup = SpanStyle {
                    sup: true,
                    ..SpanStyle::default()
                };
                push(&mut text, &number.to_string(), sup, Some(&target));
            }
            Inline::Image { .. } => {}
        }
    }
    if text.is_empty() {
        return div().into_any_element();
    }
    let rtl = starts_right_to_left(&text);
    let (key, part) = d.unit(text.len());
    let runs = super::md_select::select_runs(runs, part, d.sel_color());
    let text = SharedString::from(text);
    let styled = StyledText::new(text.clone()).with_runs(runs);
    d.report(key, &styled, text);
    let container = div()
        .cursor(gpui::CursorStyle::IBeam)
        .text_size(d.px(base.size))
        .line_height(relative(1.6))
        .font_family(d.st.md_face.family())
        // A paragraph whose first strong letter is Arabic or Hebrew reads from the right.
        .when(rtl, |c| c.text_right());
    if links.is_empty() {
        return container.child(styled).into_any_element();
    }
    let ranges: Vec<Range<usize>> = links.iter().map(|(r, _)| r.clone()).collect();
    let targets: Vec<String> = links.into_iter().map(|(_, t)| t).collect();
    let reader = d.reader.clone();
    container
        .child(InteractiveText::new(d.id("md-text"), styled).on_click(
            ranges,
            move |i, _, cx: &mut App| {
                if let Some(t) = targets.get(i).cloned() {
                    let _ = reader.update(cx, |r, cx| r.md_follow(&t, cx));
                }
            },
        ))
        .into_any_element()
}

/// Source lines of a parsed document's blocks.
pub(super) fn block_lines(_text: &str, doc: &MdDoc) -> Vec<usize> {
    doc.block_lines.clone()
}

impl Reader {
    /// A re-parse (after an edit) replaces the preview, keeping its place.
    pub(super) fn show_parsed(&mut self, doc: MdDoc, lines: Vec<usize>, cx: &mut Context<Self>) {
        // The text changed: a selection in the old text means nothing now.
        self.md_sel.reset();
        let Some(md) = self.md.as_mut() else { return };
        let top = self.list.logical_scroll_top();
        let count = doc.blocks.len().max(1);
        if let Some(info) = self.info.as_mut() {
            info.outline = outline(&doc);
            info.page_count = count as u32;
            info.title = doc.title.clone().or(info.title.take());
        }
        md.doc = Arc::new(doc);
        md.block_lines = lines;
        self.list.reset(count);
        self.list.scroll_to(ListOffset {
            item_ix: top.item_ix.min(count - 1),
            offset_in_item: top.offset_in_item,
        });
        cx.emit(ReaderEvent::PageChanged);
        cx.notify();
    }

    /// "Edit" over a Markdown document, top right (hidden while editing).
    pub(super) fn edit_button(&self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.md.is_none() || self.is_editing() || self.status != super::Status::Open {
            return None;
        }
        Some(
            div()
                .absolute()
                .top(px(14.0 + self.top_inset))
                .right(px(22.0))
                .child(
                    div()
                        .id("md-edit")
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .h(px(30.0))
                        .px(px(12.0))
                        .rounded(px(9.0))
                        .bg(st.c(|c| c.raised))
                        .shadow(st.float())
                        .border_1()
                        .border_color(st.c(|c| c.line))
                        .text_size(px(12.5))
                        .text_color(st.c(|c| c.text))
                        .cursor_pointer()
                        .hover(|b| b.bg(st.raised_hover()))
                        .tip("Edit this Markdown  Ctrl+E")
                        .on_click(cx.listener(|r, _, window, cx| r.toggle_edit(window, cx)))
                        .child(
                            svg()
                                .path(icon("edit"))
                                .size(px(14.0))
                                .text_color(st.c(|c| c.accent)),
                        )
                        .child("Edit"),
                )
                .into_any_element(),
        )
    }
}

/// Reveal block `ix` near the top of the view (find, anchors).
pub(super) fn reveal_block(r: &mut Reader, ix: usize) {
    r.list.scroll_to(ListOffset {
        item_ix: ix,
        offset_in_item: px(0.0),
    });
}

/// The outline as `OutlineItem`s, nested by heading level.
pub(super) fn outline(doc: &MdDoc) -> Vec<kitab_core::OutlineItem> {
    fn build(hs: &[kitab_md::Heading], i: &mut usize, parent: u8) -> Vec<kitab_core::OutlineItem> {
        let mut out = Vec::new();
        while let Some(h) = hs.get(*i).filter(|h| h.level > parent) {
            *i += 1;
            let children = build(hs, i, h.level);
            out.push(kitab_core::OutlineItem {
                title: h.text.clone(),
                page: Some(h.block as u32),
                uri: None,
                open: true,
                children,
            });
        }
        out
    }
    let mut i = 0;
    let mut out = Vec::new();
    // A heading shallower than the first one starts a new run at the top.
    while i < doc.outline.len() {
        out.extend(build(&doc.outline, &mut i, 0));
    }
    out
}

/// Whether the first letter with a direction is right-to-left (Hebrew,
/// Arabic, Syriac, Thaana, N'Ko and their presentation forms).
fn starts_right_to_left(text: &str) -> bool {
    for c in text.chars() {
        let u = c as u32;
        let rtl = matches!(u, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF);
        if rtl {
            return true;
        }
        if c.is_alphabetic() {
            return false;
        }
    }
    false
}

/// How long a code block's Copy says "Copied".
const COPIED_FOR: std::time::Duration = std::time::Duration::from_millis(1500);
/// The code block copied last (by its text's hash) and when.
static COPIED: std::sync::Mutex<Option<(u64, std::time::Instant)>> = std::sync::Mutex::new(None);

fn copy_key(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

fn copied_recently(key: u64) -> bool {
    COPIED
        .lock()
        .ok()
        .and_then(|c| *c)
        .is_some_and(|(k, at)| k == key && at.elapsed() < COPIED_FOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_columns_get_room_by_what_they_hold() {
        // A short label column and a long text column: the text gets most.
        let s = column_shares(&[14, 120], &[10, 18], 90.0).expect("fits");
        assert!((s.iter().sum::<f32>() - 1.0).abs() < 0.001);
        assert!(s[1] > 0.7, "{s:?}");
        // The label column still fits its longest word.
        assert!(s[0] * 90.0 >= 13.0, "{s:?}");
        // Short cells that fit share the width by size.
        let s = column_shares(&[5, 10], &[5, 10], 90.0).expect("fits");
        assert!(s[1] > s[0]);
        // Long unbreakable words in many columns: it scrolls instead.
        assert!(column_shares(&[60; 5], &[40; 5], 90.0).is_none());
    }
}
