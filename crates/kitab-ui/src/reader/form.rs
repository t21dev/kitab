// SPDX-License-Identifier: AGPL-3.0-or-later

//! Filling in a PDF's form: a click ticks a checkbox, picks a radio button
//! or opens a list's choices; a text field opens a box to type in, right
//! over the field, at its size. Each value goes into the open document at
//! once, so the page draws it as every other reader will (the engine draws
//! the field's own appearance), and into the file on Save (Ctrl+S), after
//! the original bytes, so signatures stay valid.
//!
//! Fields are fetched per page once it has drawn, like links, and kept for
//! pages near the current one only (AGENTS.md rule 5).

use super::Reader;
use super::geom::rect_to_view;
use crate::style::Style;
use crate::text_field::{FieldEvent, TabOut, TextField};
use gpui::{
    AnyElement, AppContext, Context, CursorStyle, Entity, InteractiveElement, IntoElement,
    MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled, Subscription,
    Window, div, prelude::FluentBuilder, px,
};
use kitab_core::{FieldEdit, FieldKind, FieldValue, FormField};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// Pages kept either side of the current one when the map is trimmed.
const KEEP: u32 = 12;
const TRIM_AT: usize = 40;

#[derive(Default)]
pub(super) struct FormState {
    pub fields: HashMap<u32, Rc<Vec<FormField>>>,
    pub pending: HashSet<u32>,
    /// Filled in since the last save, in order (the newest per widget wins).
    pub edits: Vec<FieldEdit>,
    /// The text field being typed in.
    pub typing: Option<Typing>,
    /// A list field showing its choices.
    pub choosing: Option<FormField>,
    /// Any field seen yet: the document has a form.
    pub any: bool,
}

pub(super) struct Typing {
    pub field: FormField,
    pub input: Entity<TextField>,
    _subs: [Subscription; 2],
}

/// Text size for a field in points: its own, or one that fits its box.
fn text_size(f: &FormField) -> f32 {
    match f.kind {
        FieldKind::Text { font_size, .. } if font_size > 0.0 => font_size,
        FieldKind::Text {
            multiline: true, ..
        } => 10.0,
        _ => (f.rect.h * 0.62).clamp(6.0, 14.0),
    }
}

/// Fields in reading order: top to bottom, then left to right on a line.
fn reading_order(fields: &[FormField]) -> Vec<&FormField> {
    let mut v: Vec<&FormField> = fields.iter().collect();
    v.sort_by(|a, b| {
        let same_line = (a.rect.y - b.rect.y).abs() < a.rect.h.min(b.rect.h) * 0.5;
        if same_line {
            a.rect.x.total_cmp(&b.rect.x)
        } else {
            a.rect.y.total_cmp(&b.rect.y)
        }
    });
    v
}

impl Reader {
    /// Whether this document's form can be filled in here.
    fn can_fill(&self) -> bool {
        self.is_pdf() && self.info.as_ref().is_some_and(|i| i.permissions.fill_forms)
    }

    /// Fetch page `ix`'s fields once (after its bitmap is in).
    pub(super) fn ensure_fields(&mut self, ix: u32, cx: &mut Context<Self>) {
        let Some(doc) = self.doc else { return };
        if !self.can_fill() || self.form.fields.contains_key(&ix) || !self.form.pending.insert(ix) {
            return;
        }
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let fields = cx
                .background_spawn(async move { engine.form_fields(doc, ix) })
                .await;
            let _ = this.update(cx, |r, cx| {
                r.form.pending.remove(&ix);
                r.form.any |= !fields.is_empty();
                r.form.fields.insert(ix, Rc::new(fields));
                if r.form.fields.len() > TRIM_AT {
                    let around = r.current_page() as u32;
                    r.form.fields.retain(|p, _| p.abs_diff(around) <= KEEP);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Unsaved fields, one per widget.
    pub fn unsaved_fields(&self) -> usize {
        kitab_core::latest_edits(&self.form.edits).len()
    }

    /// Whether a click should go to the form (not a markup tool or Sign…).
    fn form_takes_clicks(&self) -> bool {
        !self.placing && !(self.markup.open && self.markup.tool.is_some())
    }

    /// Fill in a field: into the open document now, the page drawn again.
    fn set_field(&mut self, field: &FormField, value: FieldValue, cx: &mut Context<Self>) {
        let Some(doc) = self.doc else { return };
        let edit = FieldEdit {
            page: field.page,
            index: field.index,
            value,
        };
        if !edit.is_valid() {
            return;
        }
        // Counted at once (a save right after this includes it); taken back
        // if the document refuses it.
        self.form.edits.push(edit.clone());
        let engine = self.engine.clone();
        let sent = edit.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { engine.set_field(doc, sent) })
                .await;
            let _ = this.update(cx, |r, cx| match result {
                Ok(pages) => {
                    // Its pages draw anew, and their fields are read again.
                    for p in &pages {
                        r.form.fields.remove(p);
                    }
                    r.redraw_pages(&pages, cx);
                }
                // Saved and opened again meanwhile: the file has it.
                Err(_) if r.doc != Some(doc) => {}
                Err(e) => {
                    log::warn!("filling in a field failed: {e}");
                    if let Some(i) = r.form.edits.iter().rposition(|x| *x == edit) {
                        r.form.edits.remove(i);
                    }
                    r.markup.note = Some(format!("Couldn't fill in the field: {e}").into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Fill the fields in again after the document was opened anew (a save
    /// that failed reopens it): what was typed is still to be saved.
    pub(super) fn reapply_fields(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.doc else { return };
        let edits = kitab_core::latest_edits(&self.form.edits);
        if edits.is_empty() {
            return;
        }
        self.form.fields.clear();
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let pages = cx
                .background_spawn(async move {
                    let mut pages: Vec<u32> = Vec::new();
                    for e in edits {
                        if let Ok(p) = engine.set_field(doc, e) {
                            pages.extend(p);
                        }
                    }
                    pages.sort_unstable();
                    pages.dedup();
                    pages
                })
                .await;
            let _ = this.update(cx, |r, cx| {
                for p in &pages {
                    r.form.fields.remove(p);
                }
                r.redraw_pages(&pages, cx);
            });
        })
        .detach();
    }

    /// Draw these pages again (a field changed): the old picture stays up
    /// until the new one is in, so nothing flashes.
    pub(super) fn redraw_pages(&mut self, pages: &[u32], cx: &mut Context<Self>) {
        for &p in pages {
            self.cache.mark_stale(p);
        }
        self.generation += 1;
        if let Some(doc) = self.doc {
            let (engine, generation) = (self.engine.clone(), self.generation);
            cx.background_spawn(async move { engine.set_generation(doc, generation) })
                .detach();
        }
        self.pending.clear();
        cx.notify();
    }

    /// A click on a field.
    fn press_field(&mut self, field: FormField, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_typing(cx);
        self.form.choosing = None;
        match &field.kind {
            FieldKind::Check => self.set_field(&field, FieldValue::Checked(!field.on), cx),
            FieldKind::Radio if !field.on => self.set_field(&field, FieldValue::Checked(true), cx),
            FieldKind::Radio => {}
            FieldKind::Text { .. } => self.start_typing(field, window, cx),
            FieldKind::Choice { .. } => {
                self.form.choosing = Some(field);
                cx.notify();
            }
            FieldKind::Signature | FieldKind::Button => {}
        }
    }

    fn start_typing(&mut self, field: FormField, window: &mut Window, cx: &mut Context<Self>) {
        let FieldKind::Text {
            multiline,
            password,
            ..
        } = field.kind
        else {
            return;
        };
        let size = text_size(&field) * self.px_per_pt(field.page as usize);
        let value = field.value.clone();
        let input = cx.new(|cx| {
            let mut t = TextField::new(&value, multiline, "", cx);
            t.set_font_size(size);
            t.set_tab_out(true);
            t.set_on_paper(true);
            t.set_secret(password);
            t
        });
        input.read(cx).focus(window);
        // The click that opened it is still being handled: focus it again
        // once it's drawn (the page would take focus back otherwise).
        let again = input.clone();
        cx.on_next_frame(window, move |_, window, cx| again.read(cx).focus(window));
        let events = cx.subscribe(&input, |r, _, ev: &FieldEvent, cx| match ev {
            FieldEvent::Submit => r.finish_typing(cx),
            FieldEvent::Cancel => {
                r.form.typing = None;
                cx.notify();
            }
            FieldEvent::Changed => cx.notify(),
        });
        let tabs = cx.subscribe_in(&input, window, |r, _, ev: &TabOut, window, cx| {
            r.tab_to_field(ev.back, window, cx)
        });
        self.form.typing = Some(Typing {
            field,
            input,
            _subs: [events, tabs],
        });
        cx.notify();
    }

    /// Keep what was typed (Enter, Tab, a click elsewhere).
    pub(super) fn finish_typing(&mut self, cx: &mut Context<Self>) {
        let Some(t) = self.form.typing.take() else {
            return;
        };
        let mut text = t.input.read(cx).text();
        if let FieldKind::Text {
            max_len: Some(n), ..
        } = t.field.kind
        {
            text = text.chars().take(n as usize).collect();
        }
        if text != t.field.value {
            self.set_field(&t.field, FieldValue::Text(text), cx);
        }
        cx.notify();
    }

    /// Tab: keep the value and type in the next text field on the page.
    fn tab_to_field(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(current) = self.form.typing.as_ref().map(|t| t.field.clone()) else {
            return;
        };
        self.finish_typing(cx);
        let Some(fields) = self.form.fields.get(&current.page).cloned() else {
            return;
        };
        let texts: Vec<&FormField> = reading_order(&fields)
            .into_iter()
            .filter(|f| f.fillable() && matches!(f.kind, FieldKind::Text { .. }))
            .collect();
        let at = texts.iter().position(|f| f.index == current.index);
        let next = match (at, back) {
            (Some(i), false) => texts.get(i + 1),
            (Some(i), true) => i.checked_sub(1).and_then(|i| texts.get(i)),
            (None, _) => texts.first(),
        };
        // The field's value may have just changed: start from what was typed.
        if let Some(f) = next {
            let mut f = (*f).clone();
            if let Some(e) = self
                .form
                .edits
                .iter()
                .rev()
                .find(|e| (e.page, e.index) == (f.page, f.index))
                && let FieldValue::Text(t) = &e.value
            {
                f.value = t.clone();
            }
            self.start_typing(f, window, cx);
        }
    }

    /// The fields on page `ix`: a light tint over each one that can be
    /// filled in, the box being typed in, and a list's choices.
    pub(super) fn form_layer(
        &self,
        ix: usize,
        ppt: f32,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let page = ix as u32;
        let Some(fields) = self.form.fields.get(&page).cloned() else {
            return Vec::new();
        };
        if !self.form_takes_clicks() {
            return Vec::new();
        }
        let size = self.raw_page_size(ix);
        let accent = st.c(|c| c.accent);
        let typing = self.form.typing.as_ref().filter(|t| t.field.page == page);
        let mut out: Vec<AnyElement> = fields
            .iter()
            .filter(|f| f.fillable())
            .filter(|f| typing.is_none_or(|t| t.field.index != f.index))
            .map(|f| {
                let r = rect_to_view(f.rect, size, self.rotation, ppt);
                let field = f.clone();
                let text = matches!(f.kind, FieldKind::Text { .. });
                div()
                    .id(("form-field", ix * 10_000 + f.index as usize))
                    .absolute()
                    .left(px(r.x))
                    .top(px(r.y))
                    .w(px(r.w))
                    .h(px(r.h))
                    .rounded(px(2.0))
                    .bg(accent.opacity(0.07))
                    .hover(|d| d.bg(accent.opacity(0.16)))
                    .cursor(if text {
                        CursorStyle::IBeam
                    } else {
                        CursorStyle::PointingHand
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |r, _, window, cx| {
                            cx.stop_propagation();
                            r.press_field(field.clone(), window, cx)
                        }),
                    )
                    .into_any_element()
            })
            .collect();
        if let Some(t) = typing {
            let r = rect_to_view(t.field.rect, size, self.rotation, ppt);
            let multiline = matches!(
                t.field.kind,
                FieldKind::Text {
                    multiline: true,
                    ..
                }
            );
            let line = text_size(&t.field) * ppt * 19.0 / 13.0;
            // One line sits in the middle of its box; several start at the top.
            let pad_y = if multiline {
                2.0
            } else {
                ((r.h - line) / 2.0).max(0.0)
            };
            out.push(
                div()
                    .id("form-typing")
                    .absolute()
                    .left(px(r.x - 1.0))
                    .top(px(r.y - 1.0))
                    .w(px(r.w + 2.0))
                    .min_h(px(r.h + 2.0))
                    .px(px(3.0))
                    .pt(px(pad_y))
                    .rounded(px(2.0))
                    .bg(gpui::white())
                    .border_1()
                    .border_color(accent)
                    .shadow(st.float())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(t.input.clone())
                    .into_any_element(),
            );
        }
        if let Some(c) = self.form.choosing.as_ref().filter(|c| c.page == page)
            && let FieldKind::Choice { options, .. } = &c.kind
        {
            let r = rect_to_view(c.rect, size, self.rotation, ppt);
            let list = self.choices(c, options, r, st, cx);
            out.push(crate::motion::enter(
                div().absolute().size_full().child(list),
                gpui::ElementId::NamedInteger(
                    "form-choices-in".into(),
                    u64::from(c.page) << 32 | u64::from(c.index),
                ),
                st.animations,
                crate::motion::From::Above(4.0),
                crate::motion::FAST,
                std::time::Duration::ZERO,
            ));
        }
        out
    }

    /// A list field's choices, under the field.
    fn choices(
        &self,
        field: &FormField,
        options: &[String],
        r: kitab_core::RectPt,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = options.iter().enumerate().map(|(i, o)| {
            let chosen = *o == field.value;
            let (f, value) = (field.clone(), o.clone());
            div()
                .id(("form-choice", i))
                .px(px(10.0))
                .py(px(5.0))
                .rounded(px(6.0))
                .text_size(px(13.0))
                .text_color(st.c(|c| c.text))
                .when(chosen, |d| d.bg(st.c(|c| c.accent).opacity(0.18)))
                .hover(|d| d.bg(st.hover_bg()))
                .cursor_pointer()
                .child(SharedString::from(o.clone()))
                .on_click(cx.listener(move |r, _, _, cx| {
                    r.form.choosing = None;
                    if value != f.value {
                        r.set_field(&f, FieldValue::Choice(value.clone()), cx);
                    }
                    cx.notify();
                }))
        });
        div()
            .id("form-choices")
            .absolute()
            .left(px(r.x))
            .top(px(r.y + r.h + 2.0))
            .min_w(px(r.w.max(140.0)))
            .max_h(px(260.0))
            .overflow_y_scroll()
            .p(px(4.0))
            .flex()
            .flex_col()
            .rounded(px(10.0))
            .bg(st.c(|c| c.raised))
            .border_1()
            .border_color(st.c(|c| c.line))
            .shadow(st.float())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when(options.is_empty(), |d| {
                d.child(
                    div()
                        .px(px(10.0))
                        .py(px(5.0))
                        .text_size(px(12.0))
                        .text_color(st.c(|c| c.muted))
                        .child("This list has no choices"),
                )
            })
            .children(rows)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitab_core::RectPt;

    fn field(index: u32, x: f32, y: f32) -> FormField {
        FormField {
            page: 0,
            index,
            name: format!("f{index}"),
            kind: FieldKind::Text {
                multiline: false,
                max_len: None,
                password: false,
                font_size: 0.0,
            },
            rect: RectPt {
                x,
                y,
                w: 100.0,
                h: 20.0,
            },
            value: String::new(),
            on: false,
            read_only: false,
        }
    }

    #[test]
    fn tab_goes_by_lines_then_left_to_right() {
        // File order is often not reading order.
        let fields = [
            field(0, 300.0, 100.0),
            field(1, 72.0, 200.0),
            field(2, 72.0, 102.0),
        ];
        let order: Vec<u32> = reading_order(&fields).iter().map(|f| f.index).collect();
        assert_eq!(order, [2, 0, 1]);
    }

    #[test]
    fn text_fits_the_box_unless_the_field_says() {
        let mut f = field(0, 0.0, 0.0);
        assert!((text_size(&f) - 12.4).abs() < 0.01);
        f.kind = FieldKind::Text {
            multiline: false,
            max_len: None,
            password: false,
            font_size: 9.0,
        };
        assert_eq!(text_size(&f), 9.0);
    }
}
