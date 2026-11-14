// SPDX-License-Identifier: AGPL-3.0-or-later

//! The command palette (Ctrl+K / Ctrl+Shift+P, DESIGN.md § Command palette):
//! a field over a list of commands with their keys. Typing also finds open
//! tabs, recent files and the document's headings. Enter runs the chosen
//! one on the page that was focused before; Up and Down choose; Esc closes.

use super::Workspace;
use crate::actions::{CloseOverlay, OVERLAY};
use crate::palette::{COMMANDS, matches, score};
use crate::shortcuts;
use crate::style::Style;
use crate::text_field::{FieldEvent, TextField, new_field};
use gpui::{
    AnyElement, Context, ElementId, Entity, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};

/// One row of the palette.
#[derive(Clone)]
pub(super) enum Hit {
    /// An index into COMMANDS.
    Command(usize),
    /// An open tab (its index then), by title.
    Tab(usize, String),
    /// A file from Recent.
    Recent(std::path::PathBuf, String),
    /// A heading in the document showing: its page (or Markdown block).
    Heading(u32, String),
}

/// At most this many tabs, recent files and headings each (commands first).
const EXTRA_MAX: usize = 6;

pub(super) struct Palette {
    field: Entity<TextField>,
    /// Best first.
    hits: Vec<Hit>,
    selected: usize,
    scroll: ScrollHandle,
    /// Per opening, so its entrance plays each time.
    serial: u64,
    _sub: Subscription,
}

/// Openings of the palette so far.
static OPENED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Workspace {
    pub(super) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.palette.as_ref() {
            // Asked for again while it was leaving: it stays.
            if self.modal_exit == Some(("palette", p.serial)) {
                self.modal_exit = None;
                p.field.read(cx).focus(window);
                cx.notify();
                return;
            }
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
                    let hits = ws.palette_hits(&q, cx);
                    if let Some(p) = ws.palette.as_mut() {
                        p.hits = hits;
                        p.selected = 0;
                        p.scroll.scroll_to_item(0);
                    }
                    cx.notify();
                }
                FieldEvent::Submit => ws.run_palette(None, window, cx),
                FieldEvent::Cancel => ws.close_palette(window, cx),
            },
        );
        let hits = self.palette_hits("", cx);
        self.palette = Some(Palette {
            field,
            hits,
            selected: 0,
            scroll: ScrollHandle::new(),
            serial: OPENED.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            _sub: sub,
        });
        cx.notify();
    }

    pub(super) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_none() {
            return;
        }
        self.exit_modal("palette", window, cx, |ws, window, cx| {
            if ws.palette.take().is_some() {
                ws.refocus(window, cx);
                cx.notify();
            }
        });
    }

    /// Commands for `q`, then (once something is typed) the open tabs,
    /// recent files and headings it matches, each best first.
    fn palette_hits(&self, q: &str, cx: &Context<Self>) -> Vec<Hit> {
        let mut hits: Vec<Hit> = matches(q, self.active.is_some())
            .into_iter()
            .map(Hit::Command)
            .collect();
        if q.trim().is_empty() {
            return hits;
        }
        let best = |mut found: Vec<(i32, Hit)>| {
            found.sort_by_key(|h| std::cmp::Reverse(h.0));
            found.into_iter().take(EXTRA_MAX).map(|h| h.1)
        };
        let tabs: Vec<(i32, Hit)> = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let title = t.read(cx).title().to_string();
                Some((score(q, &title)?, Hit::Tab(i, title)))
            })
            .collect();
        let open: Vec<std::path::PathBuf> =
            self.tabs.iter().map(|t| t.read(cx).path.clone()).collect();
        let recent: Vec<(i32, Hit)> = self
            .recent
            .iter()
            .filter(|r| !r.missing && !open.contains(&r.path))
            .filter_map(|r| {
                Some((
                    score(q, &r.title)?,
                    Hit::Recent(r.path.clone(), r.title.clone()),
                ))
            })
            .collect();
        let mut headings = Vec::new();
        if let Some(r) = self.active_reader() {
            fn walk(items: &[kitab_core::OutlineItem], q: &str, out: &mut Vec<(i32, Hit)>) {
                for it in items {
                    if out.len() > 2000 {
                        return;
                    }
                    if let (Some(page), Some(s)) = (it.page, score(q, &it.title)) {
                        out.push((s, Hit::Heading(page, it.title.clone())));
                    }
                    walk(&it.children, q, out);
                }
            }
            walk(r.read(cx).outline(), q, &mut headings);
        }
        // A tab or heading named exactly as typed beats a command that
        // merely contains the letters.
        let mut extra: Vec<Hit> = best(tabs)
            .chain(best(headings))
            .chain(best(recent))
            .collect();
        let lead = extra
            .iter()
            .position(|h| match h {
                Hit::Tab(_, t) | Hit::Recent(_, t) | Hit::Heading(_, t) => {
                    t.to_lowercase().starts_with(&q.trim().to_lowercase())
                }
                Hit::Command(_) => false,
            })
            .map(|i| extra.remove(i));
        hits.extend(extra);
        if let Some(lead) = lead {
            hits.insert(0, lead);
        }
        hits
    }

    /// Run the chosen row (or `pick`), on whatever had focus before.
    fn run_palette(&mut self, pick: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        let Some(hit) = p.hits.get(pick.unwrap_or(p.selected)).cloned() else {
            return;
        };
        let ix = match hit {
            Hit::Command(ix) => ix,
            Hit::Tab(i, _) => {
                self.close_palette(window, cx);
                self.activate(i, window, cx);
                return;
            }
            Hit::Recent(path, _) => {
                self.close_palette(window, cx);
                self.open_tab(path, window, cx);
                return;
            }
            Hit::Heading(page, _) => {
                self.close_palette(window, cx);
                if let Some(r) = self.active_reader().cloned() {
                    r.update(cx, |r, cx| r.jump_to(page, cx));
                }
                return;
            }
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
                    .child("Nothing by that name"),
            );
        }
        for (row, hit) in p.hits.iter().enumerate() {
            let on = row == p.selected;
            let (group, label, ix): (&str, SharedString, Option<usize>) = match hit {
                Hit::Command(ix) => (COMMANDS[*ix].group, COMMANDS[*ix].label.into(), Some(*ix)),
                Hit::Tab(_, t) => ("Tab", t.clone().into(), None),
                Hit::Recent(_, t) => ("Recent", t.clone().into(), None),
                Hit::Heading(_, t) => ("Contents", t.clone().into(), None),
            };
            let Some(ix) = ix else {
                list = list.child(
                    div()
                        .id(ElementId::NamedInteger("palette-other".into(), row as u64))
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .h(st.px(|m| m.list_row))
                        .px(px(10.0))
                        .rounded(st.px(|m| m.control_radius))
                        .when(on, |d| d.bg(st.c(|c| c.hover)))
                        .cursor_pointer()
                        .on_click(cx.listener(move |ws, _, window, cx| {
                            ws.run_palette(Some(row), window, cx)
                        }))
                        .child(
                            div()
                                .w(px(64.0))
                                .flex_none()
                                .text_size(px(11.5))
                                .text_color(st.c(|c| c.accent))
                                .child(group.to_string()),
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
                                .child(label),
                        ),
                );
                continue;
            };
            let _ = (group, label);
            let c = &COMMANDS[ix];
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
        // It opens from a key, often many times a day: a quick fade and a
        // 4 px drop, short enough never to be in the way.
        let serial = self.palette.as_ref().map_or(0, |p| p.serial);
        self.modal_serials.insert("palette", serial);
        let leaving = self.modal_exit == Some(("palette", serial));
        let (panel, scrim) = if leaving {
            (
                crate::motion::exit(
                    div().child(panel),
                    ElementId::NamedInteger("palette-out".into(), serial),
                    crate::motion::From::Above(4.0),
                    crate::motion::EXIT_FAST,
                ),
                crate::motion::exit(
                    div().absolute().inset_0().bg(st.c(|c| c.scrim)),
                    ElementId::NamedInteger("palette-scrim-out".into(), serial),
                    crate::motion::From::Fade,
                    crate::motion::EXIT_FAST,
                ),
            )
        } else {
            (
                crate::motion::enter(
                    div().child(panel),
                    ElementId::NamedInteger("palette-in".into(), serial),
                    st.animations,
                    crate::motion::From::Above(4.0),
                    crate::motion::FAST,
                    std::time::Duration::ZERO,
                ),
                crate::motion::enter(
                    div().absolute().inset_0().bg(st.c(|c| c.scrim)),
                    ElementId::NamedInteger("palette-scrim".into(), serial),
                    st.animations,
                    crate::motion::From::Fade,
                    crate::motion::FAST,
                    std::time::Duration::ZERO,
                ),
            )
        };
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
                .child(scrim)
                .child(panel)
                .into_any_element(),
        )
    }
}
