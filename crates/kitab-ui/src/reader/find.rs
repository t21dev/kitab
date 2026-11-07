// SPDX-License-Identifier: AGPL-3.0-or-later

//! The find bar (PLAN.md §3.5, DESIGN.md § Find bar). Ctrl+F or `/` opens it
//! under the header; typing searches after a short pause, page by page from
//! the current page onwards, so the first hit shows before the whole document
//! is read. Every hit is highlighted; the current one is the accent. A new
//! query bumps the generation, and batches for an older one are dropped
//! (AGENTS.md rule 3).

use super::Reader;
use super::geom::rect_to_view;
use crate::actions::*;
use crate::icons::icon;
use crate::motion;
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, AppContext, Context, ElementId, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, ListOffset, MouseButton, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, Window, div, prelude::FluentBuilder, px, svg,
};
use kitab_core::{FindOptions, RectPt};
use std::collections::BTreeMap;
use std::time::Duration;

/// Pause after the last keystroke before searching.
const DEBOUNCE: Duration = Duration::from_millis(140);
/// Pages searched per background job; results show after each batch.
const BATCH: u32 = 6;
/// Hits kept at most (AGENTS.md rule 5); the count then reads "10000+".
const MAX_HITS: usize = 10_000;
/// Characters a query may have.
const MAX_QUERY: usize = 256;

pub(super) struct FindState {
    pub open: bool,
    /// Bumped on every opening, so the slide-in plays again.
    pub serial: u64,
    /// Closing: the bar plays its exit, then goes.
    pub closing: bool,
    pub query: String,
    /// The whole query is selected: typing replaces it (after Ctrl+F on an open bar).
    pub all_selected: bool,
    pub focus: FocusHandle,
    pub options: FindOptions,
    /// Hits by page, each a list of boxes in page points, in reading order.
    pub hits: BTreeMap<u32, Vec<Vec<RectPt>>>,
    pub total: usize,
    pub current: Option<(u32, usize)>,
    /// Pages searched so far for this query.
    pub scanned: u32,
    pub done: bool,
    pub generation: u64,
}

impl FindState {
    pub fn new(focus: FocusHandle) -> Self {
        Self {
            open: false,
            serial: 0,
            closing: false,
            query: String::new(),
            all_selected: false,
            focus,
            options: FindOptions {
                match_case: false,
                whole_word: false,
                ignore_diacritics: true,
            },
            hits: BTreeMap::new(),
            total: 0,
            current: None,
            scanned: 0,
            done: true,
            generation: 0,
        }
    }

    /// The current hit's position in document order, 1-based.
    fn current_number(&self) -> Option<usize> {
        let (page, ix) = self.current?;
        let before: usize = self.hits.range(..page).map(|(_, v)| v.len()).sum();
        Some(before + ix + 1)
    }

    /// The hit after (or before) `from` in document order, wrapping around.
    fn step(&self, from: Option<(u32, usize)>, forward: bool) -> Option<(u32, usize)> {
        let flat: Vec<(u32, usize)> = self
            .hits
            .iter()
            .flat_map(|(p, v)| (0..v.len()).map(move |i| (*p, i)))
            .collect();
        if flat.is_empty() {
            return None;
        }
        let at = from.and_then(|f| flat.iter().position(|h| *h == f));
        let next = match (at, forward) {
            (Some(i), true) => (i + 1) % flat.len(),
            (Some(i), false) => (i + flat.len() - 1) % flat.len(),
            (None, true) => 0,
            (None, false) => flat.len() - 1,
        };
        flat.get(next).copied()
    }
}

impl Reader {
    /// Ctrl+F or `/`: open the bar, or select the query in an open one.
    pub fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.status != super::Status::Open {
            return;
        }
        let f = &mut self.find;
        if !f.open || f.closing {
            f.open = true;
            f.closing = false;
            f.serial += 1;
        }
        f.all_selected = !f.query.is_empty();
        window.focus(&f.focus.clone());
        if !self.find.query.is_empty() && self.find.hits.is_empty() && self.find.done {
            self.search(cx);
        }
        cx.notify();
    }

    /// Esc: close the bar and clear the highlights. The query is kept for next time.
    pub fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find.open {
            return;
        }
        self.find.generation += 1;
        self.find.hits.clear();
        self.find.total = 0;
        self.find.current = None;
        self.find.done = true;
        window.focus(&self.focus);
        if crate::style::Style::get(cx).animations {
            self.find.closing = true;
            let serial = self.find.serial;
            let timer = cx.background_executor().timer(motion::EXIT_FAST);
            cx.spawn(async move |this, cx| {
                timer.await;
                let _ = this.update(cx, |r, cx| {
                    if r.find.closing && r.find.serial == serial {
                        r.find.open = false;
                        r.find.closing = false;
                        cx.notify();
                    }
                });
            })
            .detach();
        } else {
            self.find.open = false;
        }
        cx.notify();
    }

    /// F3 / Enter (forward) and Shift+F3 / Shift+Enter.
    pub fn find_step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find.open {
            // F3 with the bar closed reopens it on the last query.
            self.open_find(window, cx);
            return;
        }
        if let Some(next) = self.find.step(self.find.current, forward) {
            self.find.current = Some(next);
            self.reveal_hit(cx);
        }
        cx.notify();
    }

    fn toggle_option(&mut self, f: fn(&mut FindOptions), cx: &mut Context<Self>) {
        f(&mut self.find.options);
        self.search(cx);
    }

    /// Search with the current query and options, after a short pause.
    pub(super) fn search(&mut self, cx: &mut Context<Self>) {
        let f = &mut self.find;
        f.generation += 1;
        f.hits.clear();
        f.total = 0;
        f.current = None;
        f.scanned = 0;
        let query = f.query.trim().to_string();
        let Some(doc) = self.doc.filter(|_| !query.is_empty()) else {
            f.done = true;
            cx.notify();
            return;
        };
        f.done = false;
        let (generation, options) = (f.generation, f.options);
        let count = self.page_count() as u32;
        let start = self.current_page() as u32;
        let engine = self.engine.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            // From the current page to the end, then from the start.
            let order: Vec<u32> = (start..count).chain(0..start).collect();
            for batch in order.chunks(BATCH as usize) {
                let stale = this
                    .update(cx, |r, _| r.find.generation != generation)
                    .unwrap_or(true);
                if stale {
                    return;
                }
                let (engine, pages, query) = (engine.clone(), batch.to_vec(), query.clone());
                let found = cx
                    .background_spawn(async move {
                        pages
                            .into_iter()
                            .map(|p| (p, engine.find_page(doc, p, &query, options)))
                            .collect::<Vec<_>>()
                    })
                    .await;
                let keep_going = this
                    .update(cx, |r, cx| {
                        if r.find.generation != generation {
                            return false;
                        }
                        r.find.scanned += found.len() as u32;
                        let mut first = None;
                        for (page, hits) in found {
                            if hits.is_empty() || r.find.total >= MAX_HITS {
                                continue;
                            }
                            r.find.total += hits.len();
                            first.get_or_insert((page, 0));
                            r.find
                                .hits
                                .insert(page, hits.into_iter().map(|h| h.quads).collect());
                        }
                        if r.find.current.is_none() {
                            if let Some(first) = first {
                                r.find.current = Some(first);
                                r.reveal_hit(cx);
                            }
                        }
                        cx.notify();
                        r.find.total < MAX_HITS
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
            let _ = this.update(cx, |r, cx| {
                if r.find.generation == generation {
                    r.find.done = true;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Scroll so the current hit is in view, about a third of the way down,
    /// unless it is already visible.
    fn reveal_hit(&mut self, cx: &mut Context<Self>) {
        let Some((page, ix)) = self.find.current else {
            return;
        };
        let Some(first) = self.find.hits.get(&page).and_then(|h| h.get(ix)?.first()) else {
            return;
        };
        let ppt = self.px_per_pt(page as usize);
        let view = rect_to_view(
            *first,
            self.raw_page_size(page as usize),
            self.rotation,
            ppt,
        );
        let top_pad = self.sheet_top_pad(page as usize);
        if let Some(sheet) = self.sheet_bounds(page as usize) {
            let vb = self.list.viewport_bounds();
            let y = f32::from(sheet.origin.y) + view.y;
            let (top, bottom) = (f32::from(vb.origin.y), f32::from(vb.bottom()));
            // Leave room for the find bar at the top.
            if y > top + 64.0 && y + view.h < bottom - 72.0 {
                return;
            }
        }
        let offset = (top_pad + view.y - self.viewport.1 * 0.33).max(0.0);
        self.list.scroll_to(ListOffset {
            item_ix: page as usize,
            offset_in_item: px(offset),
        });
        cx.notify();
    }

    fn on_key_find(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        let f = &mut self.find;
        let changed = match k.key.as_str() {
            // Bound keys arrive as actions; these never edit the query.
            "enter" | "escape" | "tab" | "f3" => return,
            "backspace" => {
                if f.all_selected || k.modifiers.control {
                    f.query.clear();
                } else {
                    f.query.pop();
                }
                true
            }
            "a" if k.modifiers.control => {
                f.all_selected = !f.query.is_empty();
                cx.notify();
                cx.stop_propagation();
                return;
            }
            "v" if k.modifiers.control => {
                let pasted = cx
                    .read_from_clipboard()
                    .and_then(|c| c.text())
                    .unwrap_or_default();
                let line = pasted.lines().next().unwrap_or("").to_string();
                if f.all_selected {
                    f.query.clear();
                }
                f.query.push_str(&line);
                !line.is_empty()
            }
            _ => match k.key_char.as_ref() {
                Some(ch)
                    if !k.modifiers.control
                        && !k.modifiers.alt
                        && ch.chars().all(|c| !c.is_control()) =>
                {
                    if f.all_selected {
                        f.query.clear();
                    }
                    f.query.push_str(ch);
                    true
                }
                _ => false,
            },
        };
        if changed {
            f.all_selected = false;
            if f.query.chars().count() > MAX_QUERY {
                f.query = f.query.chars().take(MAX_QUERY).collect();
            }
            self.search(cx);
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Highlights for page `ix`, positioned on its sheet.
    pub(super) fn find_marks(&self, ix: usize, ppt: f32, st: &Style) -> Vec<AnyElement> {
        if !self.find.open || self.find.closing {
            return Vec::new();
        }
        let Some(hits) = self.find.hits.get(&(ix as u32)) else {
            return Vec::new();
        };
        let size = self.raw_page_size(ix);
        let mut out = Vec::new();
        for (h, quads) in hits.iter().enumerate() {
            let current = self.find.current == Some((ix as u32, h));
            let color = if current {
                st.c(|c| c.accent).opacity(0.45)
            } else {
                st.c(|c| c.highlight)
            };
            for q in quads {
                let r = rect_to_view(*q, size, self.rotation, ppt);
                // A little air around the glyphs, as a marker would leave.
                out.push(
                    div()
                        .absolute()
                        .left(px(r.x - 1.0))
                        .top(px(r.y - 1.0))
                        .w(px(r.w + 2.0))
                        .h(px(r.h + 2.0))
                        .rounded(px(2.0))
                        .bg(color)
                        .into_any_element(),
                );
            }
        }
        out
    }

    /// The bar under the header, right-aligned over the pages.
    pub(super) fn find_bar(
        &self,
        st: &Style,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let f = &self.find;
        if !f.open {
            return None;
        }
        let focused = f.focus.is_focused(window);
        let empty = f.query.is_empty();
        let count: SharedString = if empty {
            "".into()
        } else if !f.done && f.total == 0 {
            "Searching…".into()
        } else if f.total == 0 {
            "No results".into()
        } else {
            let total = if f.total >= MAX_HITS {
                format!("{MAX_HITS}+")
            } else if f.done {
                format!("{}", f.total)
            } else {
                format!("{}…", f.total)
            };
            match f.current_number() {
                Some(n) => format!("{n} of {total}").into(),
                None => total.into(),
            }
        };
        let none = !empty && f.done && f.total == 0;
        let caret = div()
            .w(px(1.0))
            .h(px(15.0))
            .bg(st.c(|c| c.accent))
            .flex_none();
        let field = div()
            .id("find-field")
            .key_context(FIND_BAR)
            .track_focus(&f.focus)
            .on_key_down(cx.listener(Self::on_key_find))
            .on_action(cx.listener(|r, _: &FindNext, window, cx| r.find_step(true, window, cx)))
            .on_action(cx.listener(|r, _: &FindPrev, window, cx| r.find_step(false, window, cx)))
            .on_action(cx.listener(|r, _: &CloseFind, window, cx| r.close_find(window, cx)))
            .on_action(cx.listener(|r, _: &ToggleMatchCase, _, cx| {
                r.toggle_option(|o| o.match_case = !o.match_case, cx)
            }))
            .on_action(cx.listener(|r, _: &ToggleWholeWord, _, cx| {
                r.toggle_option(|o| o.whole_word = !o.whole_word, cx)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|r, _, window, cx| {
                    r.find.all_selected = false;
                    window.focus(&r.find.focus.clone());
                    cx.notify();
                }),
            )
            .flex_1()
            .min_w_0()
            .h(px(30.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(st.px(|m| m.control_radius))
            .bg(st.c(|c| c.hover))
            .shadow(st.well())
            .border_1()
            .border_color(if none {
                st.c(|c| c.bad)
            } else if focused {
                st.c(|c| c.accent)
            } else {
                st.c(|c| c.line)
            })
            .cursor_text()
            .child(
                svg()
                    .path(icon("search"))
                    .size(px(14.0))
                    .flex_none()
                    .text_color(st.c(|c| c.faint)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .text_size(st.px(|m| m.ui_text))
                    .whitespace_nowrap()
                    .map(|d| {
                        if empty {
                            d.when(focused, |d| d.child(caret)).child(
                                div()
                                    .text_color(st.c(|c| c.faint))
                                    .child("Find in document"),
                            )
                        } else if f.all_selected {
                            d.child(
                                div()
                                    .px(px(1.0))
                                    .rounded(px(3.0))
                                    .bg(st.c(|c| c.accent).opacity(0.3))
                                    .text_color(st.c(|c| c.text))
                                    .child(f.query.clone()),
                            )
                        } else {
                            d.child(div().text_color(st.c(|c| c.text)).child(f.query.clone()))
                                .when(focused, |d| d.child(caret))
                        }
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(12.0))
                    .text_color(if none {
                        st.c(|c| c.bad)
                    } else {
                        st.c(|c| c.faint)
                    })
                    .child(count),
            );

        let has_hits = f.total > 0;
        let bar = div()
            .id("find-bar")
            .w(px(440.0))
            .p(px(6.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(12.0))
            .bg(st.c(|c| c.raised))
            .shadow(st.float())
            .border_1()
            .border_color(st.c(|c| c.line))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(field)
            .child(
                bar_button("find-case", "match-case", f.options.match_case, false, st)
                    .tip("Match case (Alt+C)")
                    .on_click(cx.listener(|r, _, _, cx| {
                        r.toggle_option(|o| o.match_case = !o.match_case, cx)
                    })),
            )
            .child(
                bar_button("find-word", "whole-word", f.options.whole_word, false, st)
                    .tip("Whole word (Alt+W)")
                    .on_click(cx.listener(|r, _, _, cx| {
                        r.toggle_option(|o| o.whole_word = !o.whole_word, cx)
                    })),
            )
            .child(
                div()
                    .mx(px(2.0))
                    .w(px(1.0))
                    .h(px(18.0))
                    .bg(st.c(|c| c.line)),
            )
            .child(
                bar_button("find-prev", "arrow-up", false, !has_hits, st)
                    .tip("Previous (Shift+F3)")
                    .on_click(cx.listener(|r, _, window, cx| r.find_step(false, window, cx))),
            )
            .child(
                bar_button("find-next", "arrow-down", false, !has_hits, st)
                    .tip("Next (F3)")
                    .on_click(cx.listener(|r, _, window, cx| r.find_step(true, window, cx))),
            )
            .child(
                bar_button("find-close", "close", false, false, st)
                    .tip("Close (Esc)")
                    .on_click(cx.listener(|r, _, window, cx| r.close_find(window, cx))),
            );
        let bar = if f.closing {
            motion::exit(
                bar,
                ElementId::NamedInteger("find-out".into(), f.serial),
                motion::From::Above(8.0),
                motion::EXIT_FAST,
            )
        } else {
            motion::enter(
                bar,
                ElementId::NamedInteger("find-in".into(), f.serial),
                st.animations,
                motion::From::Above(8.0),
                motion::FAST,
                Duration::ZERO,
            )
        };
        Some(
            div()
                .absolute()
                .top(px(10.0))
                .right(px(18.0))
                .child(bar)
                .into_any_element(),
        )
    }
}

fn bar_button(
    id: &'static str,
    name: &str,
    on: bool,
    disabled: bool,
    st: &Style,
) -> gpui::Stateful<gpui::Div> {
    let fg = if disabled {
        st.c(|c| c.ring)
    } else if on {
        st.c(|c| c.accent)
    } else {
        st.c(|c| c.muted)
    };
    div()
        .id(id)
        .size(px(30.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(st.px(|m| m.control_radius))
        .when(on, |d| d.bg(st.c(|c| c.hover)).shadow(st.well()))
        .when(!on && !disabled, |d| d.hover(|d| d.bg(st.c(|c| c.hover))))
        .child(svg().path(icon(name)).size(px(16.0)).text_color(fg))
}
