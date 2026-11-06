// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tooltips: every icon button has one, with its shortcut (AGENTS.md rule 18).

use crate::style::Style;
use gpui::{
    AnyView, App, AppContext, Context, Div, IntoElement, ParentElement, Render, SharedString,
    Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};

pub struct TooltipView {
    text: SharedString,
}

impl Render for TooltipView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let st = Style::get(cx).clone();
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(7.0))
            .bg(st.c(|c| c.raised))
            .shadow(st.float())
            .text_size(px(12.0))
            .text_color(st.c(|c| c.text))
            .child(self.text.clone())
    }
}

pub trait Tip: Sized {
    fn tip(self, text: impl Into<SharedString>) -> Self;
}

impl Tip for Stateful<Div> {
    fn tip(self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.tooltip(move |_: &mut Window, cx: &mut App| -> AnyView {
            cx.new(|_| TooltipView { text: text.clone() }).into()
        })
    }
}
