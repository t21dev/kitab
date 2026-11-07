// SPDX-License-Identifier: AGPL-3.0-or-later

//! The command palette (Ctrl+K / Ctrl+Shift+P, DESIGN.md § Command palette):
//! a field over a list of commands with their keys. Enter runs the chosen
//! one on the page that was focused before; Up and Down choose; Esc closes.

use super::Workspace;
use crate::actions::{CloseOverlay, OVERLAY};
use crate::palette::{COMMANDS, matches};
use crate::shortcuts;
use crate::style::Style;
use crate::text_field::{FieldEvent, TextField, new_field};
use gpui::{
    AnyElement, Context, ElementId, Entity, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, ScrollHandle, StatefulInteractiveElement, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};

pub(super) struct Palette {
    field: Entity<TextField>,
    /// Indexes into COMMANDS, best first.
    hits: Vec<usize>,
    selected: usize,
    scroll: ScrollHandle,
    _sub: Subscription,
}

impl Workspace {
    pub(super) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            return self.close_palette(window, cx);
        }
        self.menu = None;
        self.menu_closing = None;
        let field = new_field("", false, "Type a command", window, cx);
        let sub = cx.subscribe_in(
            &field,
            window,
            |ws, field, e: &FieldEvent, window, cx| match e {
                FieldEvent::Changed => {
                    let q = field.read(cx).text();
                    let doc = ws.active.is_some();
                    if let Some(p) = ws.palette.as_mut() {
                        p.hits = matches(&q, doc);
                        p.selected = 0;
                        p.scroll.scroll_to_item(0);
                    }
                    cx.notify();
                }
                FieldEvent::Submit => ws.run_palette(None, window, cx),
                FieldEvent::Cancel => ws.close_palette(window, cx),
            },
        );
        self.palette = Some(Palette {
            field,
            hits: matches("", self.active.is_some()),
            selected: 0,
            scroll: ScrollHandle::new(),
            _sub: sub,
        });
        cx.notify();
    }

    pub(super) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.take().is_some() {
            self.refocus(window, cx);
            cx.notify();
        }
    }

    /// Run the chosen command (or `pick`), on whatever had focus before.
    fn run_palette(&mut self, pick: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        let Some(&ix) = p.hits.get(pick.unwrap_or(p.selected)) else {
            return;
        };
        let action = (COMMANDS[ix].action)();
        self.close_palette(window, cx);
        // After the page has its focus back, so the action reaches it.
        cx.defer_in(window, move |_, window, cx| {
            window.dispatch_action(action, cx);
        });
    }

    fn palette_key(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.palette.as_mut() else {
            return;
        };
        let n = p.hits.len();
        if n == 0 {
            return;
        }
        let step = match e.keystroke.key.as_str() {
            "down" => 1,
            "up" => n - 1,
            "pagedown" => 8.min(n - 1),
            "pageup" => n - 8.min(n - 1),
            _ => return,
        };
        p.selected = (p.selected + step) % n;
        p.scroll.scroll_to_item(p.selected);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn palette_view(
        &mut self,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = self.palette.as_ref()?;
        let bindings = cx.key_bindings();
        let bindings = bindings.borrow();
        let mut list = div()
            .id("palette-list")
            .flex()
            .flex_col()
            .max_h(px(360.0))
            .overflow_y_scroll()
            .track_scroll(&p.scroll)
            .p(px(6.0));
        if p.hits.is_empty() {
            list = list.child(
                div()
                    .px(px(12.0))
                    .py(px(14.0))
                    .text_size(st.px(|m| m.ui_text))
                    .text_color(st.c(|c| c.faint))
                    .child("No command by that name"),
            );
        }
        for (row, &ix) in p.hits.iter().enumerate() {
            let c = &COMMANDS[ix];
            let on = row == p.selected;
            let action = (c.action)();
            // keymap.json's binding when it set one (it comes last); else the
            // first default, the main one (Ctrl+W before Ctrl+F4).
            let user = cx
                .try_global::<crate::keymap::UserBound>()
                .is_some_and(|u| u.0.contains(action.name()));
            let mut all = bindings.bindings_for_action(&*action);
            let pick = if user { all.next_back() } else { all.next() };
            let keys = pick.map(|b| {
                b.keystrokes()
                    .iter()
                    .map(|k| shortcuts::pretty(&k.inner().unparse()))
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            let mut caps = div().flex().flex_none().items_center().gap(px(3.0));
            if let Some(keys) = keys.as_deref() {
                for cap in shortcuts::caps(keys) {
                    caps = caps.child(
                        div()
                            .min_w(px(20.0))
                            .h(px(20.0))
                            .px(px(5.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .bg(st.c(|c| c.raised))
                            .shadow(st.elev())
                            .text_size(px(11.0))
                            .text_color(st.c(|c| c.muted))
                            .child(cap.to_string()),
                    );
                }
            }
            list = list.child(
                div()
                    .id(ElementId::NamedInteger("palette-row".into(), ix as u64))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .h(st.px(|m| m.list_row))
                    .px(px(10.0))
                    .rounded(st.px(|m| m.control_radius))
                    .when(on, |d| d.bg(st.c(|c| c.hover)))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |ws, _, window, cx| ws.run_palette(Some(row), window, cx)),
                    )
                    .child(
                        div()
                            .w(px(64.0))
                            .flex_none()
                            .text_size(px(11.5))
                            .text_color(st.c(|c| c.faint))
                            .child(c.group),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(st.px(|m| m.ui_text))
                            .text_color(if on {
                                st.c(|c| c.text)
                            } else {
                                st.c(|c| c.muted)
                            })
                            .child(c.label),
                    )
                    .child(caps),
            );
        }
        let panel = div()
            .id("palette")
            .w(px(560.0))
            .flex()
            .flex_col()
            .rounded(px(14.0))
            .bg(st.c(|c| c.raised))
            .shadow(st.modal())
            .border_1()
            .border_color(st.c(|c| c.line))
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .capture_key_down(cx.listener(Self::palette_key))
            .child(
                div()
                    .px(px(14.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(st.c(|c| c.line))
                    .child(p.field.clone()),
            )
            .child(list)
            .child(
                div()
                    .px(px(14.0))
                    .py(px(7.0))
                    .border_t_1()
                    .border_color(st.c(|c| c.line))
                    .text_size(px(11.5))
                    .text_color(st.c(|c| c.faint))
                    .font_weight(FontWeight::NORMAL)
                    .child("↑ ↓ to choose · Enter to run · Esc to close"),
            );
        // No motion: the palette opens from a key, often many times a day.
        Some(
            div()
                .id("palette-layer")
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .flex_col()
                .items_center()
                .pt(px(72.0))
                .key_context(OVERLAY)
                .on_action(
                    cx.listener(|ws, _: &CloseOverlay, window, cx| ws.close_palette(window, cx)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, window, cx| ws.close_palette(window, cx)),
                )
                .child(div().absolute().inset_0().bg(st.c(|c| c.scrim)))
                .child(panel)
                .into_any_element(),
        )
    }
}
