// SPDX-License-Identifier: AGPL-3.0-or-later

//! A small text field: one line or several, with the platform's input
//! methods (Bangla, Arabic, CJK composition), clipboard, undo and redo.
//! Used for notes and text boxes on pages. It owns an [`EditBuffer`] and
//! implements GPUI's `EntityInputHandler`, like the Markdown editor does.
//!
//! Events: [`FieldEvent::Changed`] on every edit, [`FieldEvent::Submit`] on
//! Enter (Ctrl+Enter in a multi-line field) and [`FieldEvent::Cancel`] on Esc.

use crate::edit_buffer::EditBuffer;
use crate::style::Style;
use gpui::{
    App, AppContext, Bounds, ClipboardItem, Context, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, Font, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Pixels, Render, SharedString, Styled, TextRun,
    UTF16Selection, Window, canvas, div, point, px, size,
};
use std::ops::Range;

/// The key context; app-wide keys are unbound inside it (actions.rs).
pub const TEXT_FIELD: &str = "TextField";

const FONT_SIZE: f32 = 13.0;
const LINE_H: f32 = 19.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldEvent {
    Changed,
    Submit,
    Cancel,
}

pub struct TextField {
    buf: EditBuffer,
    focus: FocusHandle,
    multiline: bool,
    placeholder: SharedString,
}

impl EventEmitter<FieldEvent> for TextField {}

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Tabs as spaces for display (shaping has no tab stops).
fn display(text: &str) -> SharedString {
    text.replace('\t', "    ").into()
}

fn run(len: usize, color: gpui::Hsla, font: &Font) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

impl TextField {
    pub fn new(text: &str, multiline: bool, placeholder: &str, cx: &mut Context<Self>) -> Self {
        let mut buf = EditBuffer::new(text.to_string());
        buf.end(true, false);
        Self {
            buf,
            focus: cx.focus_handle(),
            multiline,
            placeholder: placeholder.to_string().into(),
        }
    }

    pub fn text(&self) -> String {
        self.buf.text().to_string()
    }

    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus);
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(FieldEvent::Changed);
        cx.notify();
    }

    fn on_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        let (ctrl, shift) = (
            k.modifiers.control || k.modifiers.platform,
            k.modifiers.shift,
        );
        let b = &mut self.buf;
        match k.key.as_str() {
            "enter" if self.multiline && !ctrl => {
                b.insert("\n");
                self.changed(cx);
            }
            "enter" => cx.emit(FieldEvent::Submit),
            "escape" => cx.emit(FieldEvent::Cancel),
            "backspace" => {
                b.backspace(ctrl);
                self.changed(cx);
            }
            "delete" => {
                b.delete(ctrl);
                self.changed(cx);
            }
            "left" => b.left(ctrl, shift),
            "right" => b.right(ctrl, shift),
            "up" if self.multiline => b.vertical(-1, shift),
            "down" if self.multiline => b.vertical(1, shift),
            "home" => b.home(ctrl, shift),
            "end" => b.end(ctrl, shift),
            "a" if ctrl => b.select_all(),
            "c" if ctrl => {
                if b.has_selection() {
                    cx.write_to_clipboard(ClipboardItem::new_string(b.selected_text().to_string()));
                }
            }
            "x" if ctrl => {
                if b.has_selection() {
                    cx.write_to_clipboard(ClipboardItem::new_string(b.selected_text().to_string()));
                    b.replace(None, "", false);
                    self.changed(cx);
                }
            }
            "v" if ctrl => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    let text = if self.multiline {
                        text
                    } else {
                        text.replace(['\r', '\n'], " ")
                    };
                    self.buf.replace(None, &text, false);
                    self.changed(cx);
                }
            }
            "z" if ctrl && shift => {
                if b.redo() {
                    self.changed(cx);
                }
            }
            "z" if ctrl => {
                if b.undo() {
                    self.changed(cx);
                }
            }
            "y" if ctrl => {
                if b.redo() {
                    self.changed(cx);
                }
            }
            _ => return,
        }
        let _ = window;
        cx.stop_propagation();
        cx.notify();
    }

    /// The caret's x on its line, in pixels.
    fn caret_x(&self, line: usize, at: usize, window: &mut Window) -> f32 {
        let text = self.buf.line(line);
        let font = window.text_style().font();
        let shaped = window.text_system().shape_line(
            display(text),
            px(FONT_SIZE),
            &[run(text.len(), gpui::black(), &font)],
            None,
        );
        let col = (at - self.buf.line_range(line).start).min(text.len());
        f32::from(shaped.x_for_index(col))
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let st = Style::get(cx).clone();
        let lines = self.buf.line_count().max(1);
        let focused = self.focus.is_focused(window);
        let empty = self.buf.text().is_empty();
        let entity = cx.entity();
        let text_colour = st.c(|c| c.text);
        let faint = st.c(|c| c.faint);
        let accent = st.c(|c| c.accent);
        let select_bg = st.c(|c| c.highlight);
        let rows: Vec<String> = (0..lines).map(|i| self.buf.line(i).to_string()).collect();
        let head_line = self.buf.line_of(self.buf.head);
        let head = self.buf.head;
        let sel = self.buf.selection();
        let ranges: Vec<Range<usize>> = (0..lines).map(|i| self.buf.line_range(i)).collect();
        let placeholder = self.placeholder.clone();
        // The app's own UI font, as the window's text style has it.
        let font = window.text_style().font();
        let caret_x = self.caret_x(head_line, head, window);
        // Selection: x from and to on each line it touches.
        let sel_spans: Vec<(usize, f32, f32)> = if sel.is_empty() {
            Vec::new()
        } else {
            (0..lines)
                .filter_map(|i| {
                    let r = &ranges[i];
                    let (a, b) = (sel.start.max(r.start), sel.end.min(r.end));
                    (a < b || (a == b && sel.start < r.start && sel.end > r.end))
                        .then(|| (i, self.caret_x(i, a, window), self.caret_x(i, b, window)))
                })
                .collect()
        };
        div()
            .key_context(TEXT_FIELD)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|f, e: &KeyDownEvent, window, cx| f.on_key(e, window, cx)))
            .w_full()
            .h(px(lines as f32 * LINE_H + 4.0))
            .cursor_text()
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds: Bounds<Pixels>, _, window, cx| {
                        window.handle_input(
                            &entity.read(cx).focus.clone(),
                            ElementInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                        let origin = bounds.origin + point(px(0.0), px(2.0));
                        for (i, x0, x1) in &sel_spans {
                            window.paint_quad(gpui::fill(
                                Bounds::new(
                                    origin + point(px(*x0), px(*i as f32 * LINE_H)),
                                    size(px((x1 - x0).max(4.0)), px(LINE_H)),
                                ),
                                select_bg,
                            ));
                        }
                        if empty {
                            let shaped = window.text_system().shape_line(
                                placeholder.clone(),
                                px(FONT_SIZE),
                                &[run(placeholder.len(), faint, &font)],
                                None,
                            );
                            let _ = shaped.paint(origin, px(LINE_H), window, cx);
                        }
                        for (i, row) in rows.iter().enumerate() {
                            if row.is_empty() {
                                continue;
                            }
                            let shaped = window.text_system().shape_line(
                                display(row),
                                px(FONT_SIZE),
                                &[run(row.len(), text_colour, &font)],
                                None,
                            );
                            let _ = shaped.paint(
                                origin + point(px(0.0), px(i as f32 * LINE_H)),
                                px(LINE_H),
                                window,
                                cx,
                            );
                        }
                        if focused {
                            window.paint_quad(gpui::fill(
                                Bounds::new(
                                    origin
                                        + point(px(caret_x), px(head_line as f32 * LINE_H + 2.0)),
                                    size(px(1.0), px(LINE_H - 4.0)),
                                ),
                                accent,
                            ));
                        }
                    },
                )
                .size_full(),
            )
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let b = &self.buf;
        let r = b.range_from_utf16(&range);
        *adjusted = Some(b.range_to_utf16(&r));
        Some(b.text()[r].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let b = &self.buf;
        Some(UTF16Selection {
            range: b.range_to_utf16(&b.selection()),
            reversed: b.head < b.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.buf.marked.as_ref().map(|m| self.buf.range_to_utf16(m))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.buf.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Control characters arrive as keys (Enter, Backspace) instead.
        if text.chars().any(|c| c.is_control()) {
            return;
        }
        let b = &mut self.buf;
        let range = range
            .map(|r| b.range_from_utf16(&r))
            .or_else(|| b.marked.clone());
        b.replace(range, text, true);
        b.marked = None;
        self.changed(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let b = &mut self.buf;
        let range = range
            .map(|r| b.range_from_utf16(&r))
            .or_else(|| b.marked.clone())
            .unwrap_or_else(|| b.selection());
        let start = range.start;
        b.replace(Some(range), new_text, false);
        b.marked = (!new_text.is_empty()).then(|| start..start + new_text.len());
        if let Some(sel) = new_selected {
            let base = b.to_utf16(start);
            let s = b.from_utf16(base + sel.start);
            let t = b.from_utf16(base + sel.end);
            b.click(s, false);
            b.click(t, true);
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let start = self.buf.from_utf16(range.start);
        let line = self.buf.line_of(start);
        let x = self.caret_x(line, start, window);
        Some(Bounds::new(
            point(
                element_bounds.origin.x + px(x),
                element_bounds.origin.y + px(2.0 + line as f32 * LINE_H),
            ),
            size(px(2.0), px(LINE_H)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

/// A field as an entity, focused, ready to type in.
pub fn new_field(
    text: &str,
    multiline: bool,
    placeholder: &str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<TextField> {
    let field = cx.new(|cx| TextField::new(text, multiline, placeholder, cx));
    field.read(cx).focus(window);
    field
}
