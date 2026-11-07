// SPDX-License-Identifier: AGPL-3.0-or-later

//! Editing Markdown in place (PLAN.md §3.4, Phase 5): Edit (Ctrl+E) splits the
//! tab into the source and a live preview. Lines are a virtualised list, so a
//! 1 MB file stays fast. Typing, IMEs (Bangla, CJK…), selection, clipboard,
//! undo/redo and Ctrl+S save; the preview re-parses off the UI thread a moment
//! after you stop typing. Saving writes next to the file and renames, so a
//! failed save never leaves half a file.

use super::{Reader, ReaderEvent};
use crate::actions::*;
use crate::edit_buffer::EditBuffer;
use crate::icons::icon;
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, App, AppContext, Bounds, Context, ElementInputHandler, EntityInputHandler,
    FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, Pixels, Point, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, TextRun, UTF16Selection, UniformListScrollHandle, Window,
    canvas, div, point, prelude::FluentBuilder, px, size, svg, uniform_list,
};
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

const FONT: &str = "Consolas";
const FONT_SIZE: f32 = 14.0;
const LINE_H: f32 = 22.0;
const GUTTER: f32 = 52.0;
const PAD: f32 = 12.0;
/// Pause after the last keystroke before the preview re-parses.
const REPARSE_AFTER: Duration = Duration::from_millis(180);

pub(super) struct Editor {
    pub buf: EditBuffer,
    pub focus: FocusHandle,
    scroll: UniformListScrollHandle,
    /// The text area's bounds from the last frame (clicks and the IME).
    area: Rc<Cell<Option<Bounds<Pixels>>>>,
    generation: u64,
    dragging: bool,
    /// "Saved" shows for a moment after Ctrl+S.
    pub saved_note: Option<SharedString>,
}

impl Editor {
    pub fn new(text: String, cx: &mut Context<Reader>) -> Self {
        Self {
            buf: EditBuffer::new(text),
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            area: Rc::new(Cell::new(None)),
            generation: 0,
            dragging: false,
            saved_note: None,
        }
    }
}

fn run(len: usize, color: gpui::Hsla) -> TextRun {
    TextRun {
        len,
        font: gpui::font(FONT),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

/// Tabs show as one space so byte offsets stay put.
fn display(line: &str) -> SharedString {
    SharedString::from(line.replace('\t', " "))
}

impl Reader {
    pub fn is_editing(&self) -> bool {
        self.md.as_ref().is_some_and(|m| m.edit.is_some())
    }

    pub fn has_unsaved_edits(&self) -> bool {
        self.md
            .as_ref()
            .and_then(|m| m.edit.as_ref())
            .is_some_and(|e| e.buf.dirty)
    }

    /// Ctrl+E: open the editor beside the preview, or close it.
    pub fn toggle_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(md) = self.md.as_mut() else { return };
        if md.edit.is_some() {
            self.done_editing(window, cx);
            return;
        }
        let mut editor = Editor::new(md.source.to_string(), cx);
        // Start at the block on screen.
        let block = self.list.logical_scroll_top().item_ix;
        if let Some(line) = md.block_lines.get(block) {
            let at = editor.buf.offset_at(*line, 0);
            editor.buf.click(at, false);
            editor.scroll.scroll_to_item(*line, ScrollStrategy::Top);
        }
        window.focus(&editor.focus);
        md.edit = Some(editor);
        cx.notify();
    }

    /// Done: close the editor. Unsaved changes stay in the preview until the
    /// tab closes (which asks); Save keeps them.
    fn done_editing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_unsaved_edits() {
            let answer = window.prompt(
                gpui::PromptLevel::Warning,
                "Save your changes?",
                Some("Your edits to this Markdown file are not saved yet."),
                &["Save", "Don't save", "Keep editing"],
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                let choice = answer.await.ok();
                let _ = this.update_in(cx, |r, window, cx| match choice {
                    Some(0) => {
                        r.save_markdown(cx);
                        r.close_editor(window, cx);
                    }
                    Some(1) => {
                        r.revert_edits(cx);
                        r.close_editor(window, cx);
                    }
                    _ => {}
                });
            })
            .detach();
            return;
        }
        self.close_editor(window, cx);
    }

    fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(md) = self.md.as_mut() {
            md.edit = None;
        }
        window.focus(&self.focus);
        cx.notify();
    }

    /// Throw the edits away: re-parse the text as it was loaded or last saved.
    fn revert_edits(&mut self, cx: &mut Context<Self>) {
        let Some(md) = self.md.as_ref() else { return };
        let source = md.source.to_string();
        self.reparse(source, cx);
    }

    /// Ctrl+S: write the file off the UI thread (temp file, then rename).
    pub fn save_markdown(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.md.as_ref().and_then(|m| m.edit.as_ref()) else {
            return;
        };
        let text = edit.buf.text().to_string();
        let path = self.path.clone();
        cx.spawn(async move |this, cx| {
            let saved_text = text.clone();
            let result = cx
                .background_spawn(async move {
                    let tmp = path.with_extension("kitab-saving");
                    std::fs::write(&tmp, text.as_bytes())
                        .and_then(|_| std::fs::rename(&tmp, &path))
                        .map_err(|e| {
                            let _ = std::fs::remove_file(&tmp);
                            e.to_string()
                        })
                })
                .await;
            let _ = this.update(cx, |r, cx| {
                if let Some(md) = r.md.as_mut() {
                    let note = match result {
                        Ok(()) => {
                            md.source = saved_text.into();
                            if let Some(e) = md.edit.as_mut() {
                                e.buf.dirty = false;
                            }
                            "Saved".to_string()
                        }
                        Err(e) => format!("Couldn't save: {e}"),
                    };
                    if let Some(e) = md.edit.as_mut() {
                        e.saved_note = Some(note.into());
                    }
                }
                cx.emit(ReaderEvent::PageChanged);
                cx.notify();
                // The note fades after a few seconds.
                let timer = cx.background_executor().timer(Duration::from_secs(3));
                cx.spawn(async move |this, cx| {
                    timer.await;
                    let _ = this.update(cx, |r, cx| {
                        if let Some(e) = r.md.as_mut().and_then(|m| m.edit.as_mut()) {
                            e.saved_note = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            });
        })
        .detach();
    }

    /// After an edit: keep the caret in view and re-parse the preview soon.
    fn edited(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.md.as_mut().and_then(|m| m.edit.as_mut()) else {
            return;
        };
        let line = edit.buf.line_of(edit.buf.head);
        edit.scroll.scroll_to_item(line, ScrollStrategy::Top);
        edit.generation += 1;
        let (generation, text) = (edit.generation, edit.buf.text().to_string());
        cx.emit(ReaderEvent::PageChanged);
        cx.notify();
        let timer = cx.background_executor().timer(REPARSE_AFTER);
        cx.spawn(async move |this, cx| {
            timer.await;
            let current = this
                .update(cx, |r, _| {
                    r.md.as_ref()
                        .and_then(|m| m.edit.as_ref())
                        .map(|e| e.generation)
                })
                .ok()
                .flatten();
            if current == Some(generation) {
                let _ = this.update(cx, |r, cx| r.reparse(text, cx));
            }
        })
        .detach();
    }

    /// Parse `text` off the UI thread and show it in the preview.
    fn reparse(&mut self, text: String, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let parsed = cx
                .background_spawn(async move {
                    let doc = kitab_md::parse(&text);
                    let lines = super::markdown::block_lines(&text, &doc);
                    (doc, lines)
                })
                .await;
            let _ = this.update(cx, |r, cx| r.show_parsed(parsed.0, parsed.1, cx));
        })
        .detach();
    }

    /// Keyboard: movement, deletion, Enter and Tab, and the Ctrl shortcuts.
    /// Printable text arrives through the input handler (IMEs too).
    fn on_editor_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.md.as_mut().and_then(|m| m.edit.as_mut()) else {
            return;
        };
        let k = &e.keystroke;
        let m = &k.modifiers;
        let (shift, word) = (m.shift, m.control || m.alt);
        let page =
            (f32::from(edit.area.get().map_or(px(600.0), |b| b.size.height)) / LINE_H) as isize;
        let b = &mut edit.buf;
        let mut changed = false;
        let handled = match k.key.as_str() {
            "left" => {
                b.left(word, shift);
                true
            }
            "right" => {
                b.right(word, shift);
                true
            }
            "up" => {
                b.vertical(-1, shift);
                true
            }
            "down" => {
                b.vertical(1, shift);
                true
            }
            "pageup" => {
                b.vertical(-page, shift);
                true
            }
            "pagedown" => {
                b.vertical(page, shift);
                true
            }
            "home" => {
                b.home(m.control, shift);
                true
            }
            "end" => {
                b.end(m.control, shift);
                true
            }
            "backspace" => {
                b.backspace(word);
                changed = true;
                true
            }
            "delete" => {
                b.delete(word);
                changed = true;
                true
            }
            "enter" => {
                // Keep the indentation of the line (and continue a list marker).
                let line = b.line(b.line_of(b.head)).to_string();
                let indent: String = line
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                let rest = line[indent.len()..].to_string();
                let marker = ["- [ ] ", "- [x] ", "- ", "* ", "+ "]
                    .into_iter()
                    .find(|p| rest.starts_with(p))
                    .map(|p| if p.starts_with("- [") { "- [ ] " } else { p });
                let continued = match marker {
                    Some(p) if rest.trim() != p.trim() => format!("\n{indent}{p}"),
                    _ => format!("\n{indent}"),
                };
                b.replace(None, &continued, false);
                changed = true;
                true
            }
            "tab" => {
                b.replace(None, "    ", false);
                changed = true;
                true
            }
            "a" if m.control => {
                b.select_all();
                true
            }
            "c" | "x" if m.control => {
                if b.has_selection() {
                    let text = b.selected_text().to_string();
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                    if k.key == "x" {
                        b.replace(None, "", false);
                        changed = true;
                    }
                }
                true
            }
            "v" if m.control => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    b.replace(None, &text.replace("\r\n", "\n"), false);
                    changed = true;
                }
                true
            }
            "z" if m.control && !shift => {
                changed = b.undo();
                true
            }
            "y" if m.control => {
                changed = b.redo();
                true
            }
            "z" if m.control && shift => {
                changed = b.redo();
                true
            }
            "s" if m.control => {
                self.save_markdown(cx);
                cx.stop_propagation();
                return;
            }
            "escape" => {
                self.done_editing(window, cx);
                cx.stop_propagation();
                return;
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
            if changed {
                self.edited(cx);
            } else if let Some(edit) = self.md.as_mut().and_then(|m| m.edit.as_mut()) {
                let line = edit.buf.line_of(edit.buf.head);
                edit.scroll.scroll_to_item(line, ScrollStrategy::Top);
                cx.notify();
            }
        }
    }

    /// The offset under a window point (clicks and drags).
    fn offset_at_point(&self, p: Point<Pixels>, window: &mut Window) -> Option<usize> {
        let edit = self.md.as_ref()?.edit.as_ref()?;
        let area = edit.area.get()?;
        let scroll_y = f32::from(edit.scroll.0.borrow().base_handle.offset().y);
        let y = f32::from(p.y - area.origin.y) - scroll_y;
        let line = ((y / LINE_H).floor().max(0.0) as usize).min(edit.buf.line_count() - 1);
        let x = f32::from(p.x - area.origin.x) - GUTTER - PAD;
        let text = edit.buf.line(line);
        let shaped = window.text_system().shape_line(
            display(text),
            px(FONT_SIZE),
            &[run(text.len(), gpui::black())],
            None,
        );
        let col_byte = shaped.closest_index_for_x(px(x.max(0.0)));
        Some(edit.buf.line_range(line).start + col_byte.min(text.len()))
    }

    fn editor_mouse_down(
        &mut self,
        e: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(at) = self.offset_at_point(e.position, window) else {
            return;
        };
        let Some(edit) = self.md.as_mut().and_then(|m| m.edit.as_mut()) else {
            return;
        };
        window.focus(&edit.focus);
        match e.click_count {
            2 => edit.buf.select_word(at),
            n if n >= 3 => {
                let line = edit.buf.line_of(at);
                let r = edit.buf.line_range(line);
                edit.buf.click(r.start, false);
                edit.buf.click(r.end, true);
            }
            _ => {
                edit.buf.click(at, e.modifiers.shift);
                edit.dragging = true;
            }
        }
        cx.notify();
    }

    fn editor_mouse_move(
        &mut self,
        e: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dragging = self
            .md
            .as_ref()
            .and_then(|m| m.edit.as_ref())
            .is_some_and(|e| e.dragging);
        if !dragging {
            return;
        }
        if e.pressed_button != Some(MouseButton::Left) {
            if let Some(edit) = self.md.as_mut().and_then(|m| m.edit.as_mut()) {
                edit.dragging = false;
            }
            return;
        }
        if let Some(at) = self.offset_at_point(e.position, window) {
            if let Some(edit) = self.md.as_mut().and_then(|m| m.edit.as_mut()) {
                edit.buf.click(at, true);
                cx.notify();
            }
        }
    }

    /// Lines `range` of the editor, with the selection and the caret.
    fn editor_rows(&self, range: Range<usize>, window: &mut Window, cx: &App) -> Vec<AnyElement> {
        let st = Style::get(cx);
        let Some(edit) = self.md.as_ref().and_then(|m| m.edit.as_ref()) else {
            return Vec::new();
        };
        let focused = edit.focus.is_focused(window);
        let sel = edit.buf.selection();
        let caret_line = edit.buf.line_of(edit.buf.head);
        let text_color = st.c(|c| c.text);
        range
            .map(|ix| {
                let r = edit.buf.line_range(ix);
                let text = edit.buf.line(ix);
                let shaped = window.text_system().shape_line(
                    display(text),
                    px(FONT_SIZE),
                    &[run(text.len(), text_color)],
                    None,
                );
                let x_of = |byte: usize| f32::from(shaped.x_for_index(byte.min(text.len())));
                // Selection on this line.
                let sel_box = (sel.start < r.end + 1 && sel.end > r.start && sel.start != sel.end)
                    .then(|| {
                        let a = sel.start.max(r.start) - r.start;
                        let b = sel.end.min(r.end) - r.start;
                        let (x0, mut x1) = (x_of(a), x_of(b));
                        if sel.end > r.end {
                            x1 += 6.0; // the newline is selected too
                        }
                        div()
                            .absolute()
                            .top_0()
                            .left(px(GUTTER + PAD + x0))
                            .w(px((x1 - x0).max(2.0)))
                            .h(px(LINE_H))
                            .bg(st.c(|c| c.accent).opacity(0.28))
                    });
                let caret = (ix == caret_line && focused).then(|| {
                    let x = x_of(edit.buf.head - r.start);
                    div()
                        .absolute()
                        .top(px(3.0))
                        .left(px(GUTTER + PAD + x))
                        .w(px(2.0))
                        .h(px(LINE_H - 6.0))
                        .bg(st.c(|c| c.accent))
                });
                let marked = edit
                    .buf
                    .marked
                    .clone()
                    .filter(|m| m.start >= r.start && m.end <= r.end)
                    .map(|m| {
                        let (x0, x1) = (x_of(m.start - r.start), x_of(m.end - r.start));
                        div()
                            .absolute()
                            .bottom(px(3.0))
                            .left(px(GUTTER + PAD + x0))
                            .w(px((x1 - x0).max(2.0)))
                            .h(px(1.0))
                            .bg(st.c(|c| c.accent))
                    });
                div()
                    .relative()
                    .h(px(LINE_H))
                    .w_full()
                    .flex()
                    .items_center()
                    .when(ix == caret_line && focused, |d| {
                        d.bg(st.c(|c| c.hover).opacity(0.6))
                    })
                    .children(sel_box)
                    .child(
                        div()
                            .w(px(GUTTER))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .pr(px(10.0))
                            .text_size(px(12.0))
                            .text_color(if ix == caret_line {
                                st.c(|c| c.muted)
                            } else {
                                st.c(|c| c.ring)
                            })
                            .child(format!("{}", ix + 1)),
                    )
                    .child(
                        div()
                            .pl(px(PAD))
                            .whitespace_nowrap()
                            .font_family(FONT)
                            .text_size(px(FONT_SIZE))
                            .text_color(text_color)
                            .child(display(text)),
                    )
                    .children(marked)
                    .children(caret)
                    .into_any_element()
            })
            .collect()
    }

    /// The editor pane: a toolbar, then the lines.
    pub(super) fn editor_view(
        &self,
        st: &Style,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let edit = self.md.as_ref()?.edit.as_ref()?;
        let count = edit.buf.line_count();
        let this = cx.entity();
        let list_this = this.clone();
        let area = edit.area.clone();
        let focus = edit.focus.clone();
        let dirty = edit.buf.dirty;
        let note = edit.saved_note.clone();
        let (line, col) = {
            let l = edit.buf.line_of(edit.buf.head);
            (
                l + 1,
                edit.buf.text()[edit.buf.line_range(l).start..edit.buf.head]
                    .chars()
                    .count()
                    + 1,
            )
        };
        let button = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .h(px(26.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .rounded(px(7.0))
                .text_size(px(12.5))
                .cursor_pointer()
                .when(primary, |b| {
                    b.bg(st.c(|c| c.text)).text_color(st.c(|c| c.bg))
                })
                .when(!primary, |b| {
                    b.bg(st.c(|c| c.raised))
                        .text_color(st.c(|c| c.text))
                        .hover(|b| b.bg(st.hover_bg()))
                })
                .child(label)
        };
        let toolbar = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.0))
            .h(px(40.0))
            .px(px(12.0))
            .border_b_1()
            .border_color(st.c(|c| c.line))
            .bg(st.c(|c| c.panel))
            .child(
                svg()
                    .path(icon("edit"))
                    .size(px(15.0))
                    .text_color(st.c(|c| c.accent)),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.muted))
                    .child(if dirty {
                        "Editing · unsaved"
                    } else {
                        "Editing"
                    }),
            )
            .children(note.map(|n| {
                div()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.ok))
                    .child(n)
            }))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(st.c(|c| c.faint))
                    .child(format!("Ln {line}, Col {col}")),
            )
            .child(
                button("md-save", "Save", false)
                    .tip("Save  Ctrl+S")
                    .on_click(cx.listener(|r, _, _, cx| r.save_markdown(cx))),
            )
            .child(
                button("md-done", "Done", true)
                    .tip("Back to reading  Ctrl+E")
                    .on_click(cx.listener(|r, _, window, cx| r.done_editing(window, cx))),
            );
        let lines = uniform_list("md-editor-lines", count, move |range, window, cx| {
            list_this.read(cx).editor_rows(range, window, cx)
        })
        .track_scroll(edit.scroll.clone())
        .size_full();
        let input_this = this.clone();
        Some(
            div()
                .flex()
                .flex_col()
                .size_full()
                .bg(st.c(|c| c.bg))
                .border_r_1()
                .border_color(st.c(|c| c.line))
                .child(toolbar)
                .child(
                    div()
                        .id("md-editor")
                        .key_context(MD_EDITOR)
                        .track_focus(&focus)
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .cursor_text()
                        .on_key_down(cx.listener(Self::on_editor_key))
                        .on_mouse_down(MouseButton::Left, cx.listener(Self::editor_mouse_down))
                        .on_mouse_move(cx.listener(Self::editor_mouse_move))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|r, _, _, _| {
                                if let Some(e) = r.md.as_mut().and_then(|m| m.edit.as_mut()) {
                                    e.dragging = false;
                                }
                            }),
                        )
                        .child(lines)
                        // Bounds for clicks, and the IME's text input hook.
                        .child(
                            canvas(
                                move |bounds, _, _| area.set(Some(bounds)),
                                move |bounds, _, window, cx| {
                                    window.handle_input(
                                        &focus,
                                        ElementInputHandler::new(bounds, input_this.clone()),
                                        cx,
                                    )
                                },
                            )
                            .absolute()
                            .size_full(),
                        ),
                )
                .into_any_element(),
        )
    }
}

impl EntityInputHandler for Reader {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let b = &self.md.as_ref()?.edit.as_ref()?.buf;
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
        let b = &self.md.as_ref()?.edit.as_ref()?.buf;
        Some(UTF16Selection {
            range: b.range_to_utf16(&b.selection()),
            reversed: b.head < b.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let b = &self.md.as_ref()?.edit.as_ref()?.buf;
        b.marked.as_ref().map(|m| b.range_to_utf16(m))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        if let Some(e) = self.md.as_mut().and_then(|m| m.edit.as_mut()) {
            e.buf.marked = None;
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Control characters arrive as keys (Enter, Tab, Backspace) instead.
        if text.chars().any(|c| c.is_control()) {
            return;
        }
        let Some(e) = self.md.as_mut().and_then(|m| m.edit.as_mut()) else {
            return;
        };
        let b = &mut e.buf;
        let range = range
            .map(|r| b.range_from_utf16(&r))
            .or_else(|| b.marked.clone());
        b.replace(range, text, true);
        b.marked = None;
        self.edited(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(e) = self.md.as_mut().and_then(|m| m.edit.as_mut()) else {
            return;
        };
        let b = &mut e.buf;
        let range = range
            .map(|r| b.range_from_utf16(&r))
            .or_else(|| b.marked.clone())
            .unwrap_or_else(|| b.selection());
        let start = range.start;
        b.replace(Some(range), new_text, false);
        b.marked = (!new_text.is_empty()).then(|| start..start + new_text.len());
        if let Some(sel) = new_selected {
            // Relative to the composition, in UTF-16.
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
        let e = self.md.as_ref()?.edit.as_ref()?;
        let b = &e.buf;
        let start = b.from_utf16(range.start);
        let line = b.line_of(start);
        let text = b.line(line);
        let shaped = window.text_system().shape_line(
            display(text),
            px(FONT_SIZE),
            &[run(text.len(), gpui::black())],
            None,
        );
        let x = f32::from(shaped.x_for_index((start - b.line_range(line).start).min(text.len())));
        let scroll_y = f32::from(e.scroll.0.borrow().base_handle.offset().y);
        Some(Bounds::new(
            point(
                element_bounds.origin.x + px(GUTTER + PAD + x),
                element_bounds.origin.y + px(line as f32 * LINE_H + scroll_y),
            ),
            size(px(2.0), px(LINE_H)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
