// SPDX-License-Identifier: AGPL-3.0-or-later

//! Display math laid out with GPUI elements (DESIGN.md § Markdown): the tree
//! from `kitab_md::math` drawn in Cambria Math. Fractions stack over a rule,
//! roots carry an overline, scripts sit small beside their base, big
//! operators take their limits above and below, and matrices get brackets
//! drawn as rules so they grow with their rows. Rows centre on the math axis.

use crate::style::Style;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};
use kitab_md::math::Node;

/// Scripts and limits are this much smaller than their base.
const SCRIPT: f32 = 0.7;

/// Relations and binary operators get space around them.
fn spaced(op: &str) -> bool {
    matches!(
        op,
        "=" | "+"
            | "−"
            | "<"
            | ">"
            | "≤"
            | "≥"
            | "≠"
            | "≈"
            | "≡"
            | "∼"
            | "≃"
            | "≅"
            | "∝"
            | "→"
            | "←"
            | "↔"
            | "⇒"
            | "⇐"
            | "⇔"
            | "↦"
            | "×"
            | "÷"
            | "·"
            | "±"
            | "∓"
            | "∈"
            | "∉"
            | "⊂"
            | "⊆"
            | "⊃"
            | "⊇"
            | "∪"
            | "∩"
            | "∧"
            | "∨"
            | "∘"
            | "⊕"
            | "⊗"
            | "∣"
    )
}

fn glyph(st: &Style, text: String, size: f32, italic: bool, bold: bool) -> AnyElement {
    div()
        .flex_none()
        .font_family(crate::style::math_font())
        .text_size(px(size))
        .line_height(px(size * 1.25))
        .when(italic, |d| d.italic())
        .when(!italic, |d| d.not_italic())
        .when(bold, |d| d.font_weight(FontWeight::BOLD))
        .text_color(st.c(|c| c.text))
        .child(text)
        .into_any_element()
}

/// A display formula, centred, at `size` px.
pub(super) fn display(st: &Style, node: &Node, size: f32) -> AnyElement {
    // A formula wider than the column scrolls sideways in its own box
    // instead of spilling out of both sides.
    div()
        .id(gpui::ElementId::Name(format!("math-{:p}", node).into()))
        .w_full()
        .overflow_x_scroll()
        // The wheel scrolls the page; Shift+wheel or a sideways swipe this box.
        .map(|mut el| {
            el.style().restrict_scroll_to_axis = Some(true);
            el
        })
        .child(
            div()
                .min_w_full()
                .flex()
                .justify_center()
                .py(px(size * 0.4))
                .child(el(st, node, size, false)),
        )
        .into_any_element()
}

fn row(items: Vec<AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .children(items)
        .into_any_element()
}

fn el(st: &Style, n: &Node, size: f32, bold: bool) -> AnyElement {
    match n {
        Node::Row(v) => row(v.iter().map(|c| el(st, c, size, bold)).collect()),
        Node::Ident(t) => glyph(st, t.clone(), size, !bold, bold),
        Node::Num(t) => glyph(st, t.clone(), size, false, bold),
        Node::Op(t) => {
            let g = glyph(st, t.clone(), size, false, bold);
            if spaced(t) {
                div()
                    .flex_none()
                    .px(px(size * 0.22))
                    .child(g)
                    .into_any_element()
            } else {
                g
            }
        }
        Node::Func(t) => div()
            .flex_none()
            .pr(px(size * 0.17))
            .child(glyph(st, t.clone(), size, false, bold))
            .into_any_element(),
        Node::Text(t) => div()
            .flex_none()
            .font_family(crate::style::math_text_font())
            .text_size(px(size))
            .text_color(st.c(|c| c.text))
            .child(t.clone())
            .into_any_element(),
        Node::Space(em) => div()
            .flex_none()
            .when(*em >= 0.0, |d| d.w(px(size * em)))
            .when(*em < 0.0, |d| d.ml(px(size * em)))
            .into_any_element(),
        Node::Bold(b) => el(st, b, size, true),
        Node::Frac(a, b) => {
            let inner = size * 0.9;
            div()
                .flex()
                .flex_none()
                .flex_col()
                .items_center()
                .px(px(size * 0.12))
                .child(el(st, a, inner, bold))
                .child(
                    div()
                        .w_full()
                        .h(px((size * 0.05).max(1.0)))
                        .my(px(size * 0.08))
                        .bg(st.c(|c| c.text)),
                )
                .child(el(st, b, inner, bold))
                .into_any_element()
        }
        Node::Sqrt(index, body) => {
            let radical = glyph(st, "√".into(), size * 1.25, false, false);
            div()
                .flex()
                .flex_none()
                .items_end()
                .children(index.as_ref().map(|i| {
                    div()
                        .flex_none()
                        .mr(px(-size * 0.35))
                        .mb(px(size * 0.55))
                        .child(el(st, i, size * 0.55, bold))
                }))
                .child(radical)
                .child(
                    div()
                        .flex_none()
                        .border_t(px((size * 0.05).max(1.0)))
                        .border_color(st.c(|c| c.text))
                        .pt(px(size * 0.06))
                        .px(px(size * 0.06))
                        .child(el(st, body, size, bold)),
                )
                .into_any_element()
        }
        Node::Scripts { base, sub, sup } => {
            let small = size * SCRIPT;
            // A column beside the base: superscript above, subscript below;
            // an empty half keeps the other where it belongs.
            let gap = || div().h(px(small * 0.75));
            let column = div()
                .flex()
                .flex_none()
                .flex_col()
                .pl(px(size * 0.04))
                .child(match sup {
                    Some(s) => el(st, s, small, bold),
                    None => gap().into_any_element(),
                })
                .child(match sub {
                    Some(s) => el(st, s, small, bold),
                    None => gap().into_any_element(),
                });
            row(vec![el(st, base, size, bold), column.into_any_element()])
        }
        Node::BigOp { op, sub, sup } => {
            let small = size * SCRIPT;
            let word = op.chars().all(|c| c.is_ascii_alphabetic() || c == ' ');
            let integral = op.starts_with(['∫', '∬', '∭', '∮']);
            let symbol = if word {
                glyph(st, op.clone(), size, false, bold)
            } else {
                glyph(st, op.clone(), size * 1.6, false, bold)
            };
            if integral {
                // Integrals carry their limits at the side.
                let column = div()
                    .flex()
                    .flex_none()
                    .flex_col()
                    .justify_between()
                    .h(px(size * 2.0))
                    .child(
                        sup.as_ref()
                            .map_or_else(|| div().into_any_element(), |s| el(st, s, small, bold)),
                    )
                    .child(
                        sub.as_ref()
                            .map_or_else(|| div().into_any_element(), |s| el(st, s, small, bold)),
                    );
                return div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .pr(px(size * 0.15))
                    .child(symbol)
                    .child(column)
                    .into_any_element();
            }
            div()
                .flex()
                .flex_none()
                .flex_col()
                .items_center()
                .px(px(size * 0.12))
                .children(sup.as_ref().map(|s| el(st, s, small, bold)))
                .child(symbol)
                .children(sub.as_ref().map(|s| el(st, s, small, bold)))
                .into_any_element()
        }
        Node::Fenced { open, close, body } => {
            let tall = matches!(**body, Node::Matrix { .. } | Node::Frac(..))
                || matches!(&**body, Node::Row(v) if v.iter().any(|n| matches!(n, Node::Frac(..) | Node::Matrix { .. } | Node::BigOp { .. })));
            if tall {
                bracketed(st, open, close, el(st, body, size, bold), size)
            } else {
                row(vec![
                    glyph(st, open.clone(), size, false, false),
                    el(st, body, size, bold),
                    glyph(st, close.clone(), size, false, false),
                ])
            }
        }
        Node::Matrix {
            rows,
            open,
            close,
            left,
        } => {
            let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
            let mut grid = div().flex().flex_none().gap(px(size * 0.9));
            for c in 0..cols {
                let mut column = div()
                    .flex()
                    .flex_col()
                    .gap(px(size * 0.25))
                    .when(!*left, |d| d.items_center());
                for r in rows {
                    column =
                        column.child(div().flex().min_h(px(size * 1.25)).items_center().child(
                            r.get(c).map_or_else(
                                || div().into_any_element(),
                                |cell| el(st, cell, size, bold),
                            ),
                        ));
                }
                grid = grid.child(column);
            }
            let grid = grid.px(px(size * 0.2)).into_any_element();
            if open.is_empty() && close.is_empty() {
                grid
            } else {
                bracketed(st, open, close, grid, size)
            }
        }
        Node::Accent { body, mark } => div()
            .relative()
            .flex_none()
            .child(el(st, body, size, bold))
            .child(
                div()
                    .absolute()
                    .top(px(-size * 0.55))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(glyph(
                        st,
                        accent_mark(*mark).into(),
                        size * 0.8,
                        false,
                        false,
                    )),
            )
            .into_any_element(),
    }
}

/// The spacing form of a combining accent, drawn above its base.
fn accent_mark(mark: char) -> &'static str {
    match mark {
        '\u{302}' => "ˆ",
        '\u{305}' => "‾",
        '\u{20D7}' => "→",
        '\u{307}' => "˙",
        '\u{308}' => "¨",
        '\u{303}' => "˜",
        '\u{30C}' => "ˇ",
        '\u{306}' => "˘",
        '\u{301}' => "´",
        '\u{300}' => "`",
        _ => "",
    }
}

/// Brackets drawn as rules beside `inner`, so they span its full height.
fn bracketed(st: &Style, open: &str, close: &str, inner: AnyElement, size: f32) -> AnyElement {
    let w = (size * 0.06).max(1.0);
    let side = |which: &str, left: bool| -> AnyElement {
        let d = div().flex_none().w(px(size * 0.3));
        let c = st.c(|c| c.text);
        match which {
            "(" | ")" => {
                let d = d.border_color(c);
                // A parenthesis: the side and both ends of an ellipse.
                let d = d.w(px(size * 0.4));
                if left {
                    d.border_l(px(w))
                        .rounded_tl(px(size * 1.2))
                        .rounded_bl(px(size * 1.2))
                } else {
                    d.border_r(px(w))
                        .rounded_tr(px(size * 1.2))
                        .rounded_br(px(size * 1.2))
                }
                .into_any_element()
            }
            "[" | "]" => {
                let d = d.border_color(c).border_t(px(w)).border_b(px(w));
                if left {
                    d.border_l(px(w))
                } else {
                    d.border_r(px(w))
                }
                .into_any_element()
            }
            "|" | "‖" => {
                let d = d.w(px(size * 0.12)).border_color(c);
                let d = if left {
                    d.border_l(px(w))
                } else {
                    d.border_r(px(w))
                };
                if which == "‖" {
                    d.border_r(px(w)).w(px(size * 0.2)).into_any_element()
                } else {
                    d.into_any_element()
                }
            }
            "" => div().into_any_element(),
            // { and anything else: the glyph, as tall as the font allows.
            other => div()
                .flex()
                .flex_none()
                .items_center()
                .child(glyph(st, other.to_string(), size * 2.2, false, false))
                .into_any_element(),
        }
    };
    div()
        .flex()
        .flex_none()
        .py(px(size * 0.1))
        .child(side(open, true))
        .child(div().flex().items_center().child(inner))
        .child(side(close, false))
        .into_any_element()
}
