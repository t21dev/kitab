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
    /// Where the caret was when the view last followed it sideways: it
    /// follows again only when the caret moves, not while you scroll.
    followed: Cell<usize>,
    /// How far the text is scrolled sideways, in pixels (0 = the line
    /// starts show). The line numbers stay put; only the text moves.
    hscroll: Rc<Cell<f32>>,
    /// The furthest `hscroll` can go: the widest line just fits.
    hmax: Rc<Cell<f32>>,
    /// The widest line, worked out once per edit: (edit generation, line).
    widest: Cell<Option<(u64, usize)>>,
    /// A scroll bar thumb being dragged.
    bar_drag: Option<BarDrag>,
}

/// Dragging one of the editor's scroll bars: which one, and where on the
/// thumb it was grabbed (pixels from the thumb's start).
#[derive(Clone, Copy)]
struct BarDrag {
    vertical: bool,
    grab: f32,
}

/// The scroll bars' thickness.
const BAR: f32 = 12.0;

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
            followed: Cell::new(usize::MAX),
            hscroll: Rc::new(Cell::new(0.0)),
            hmax: Rc::new(Cell::new(0.0)),
            widest: Cell::new(None),
            bar_drag: None,
        }
    }

    /// The text column's width on screen: the editor less the line numbers,
    /// the left padding and the vertical scroll bar.
    fn text_width(&self) -> f32 {
        self.area
            .get()
            .map_or(0.0, |a| f32::from(a.size.width) - GUTTER - PAD - BAR)
            .max(0.0)
    }

    /// A line's height as the list lays it out. It can differ from
    /// [`LINE_H`] by a fraction of a pixel (rows snap to whole screen
    /// pixels), which adds up over hundreds of lines; clicks use this so
    /// they land on the line under the pointer all the way down the file.
    fn line_h(&self) -> f32 {
        let count = self.buf.line_count().max(1) as f32;
        self.scroll
            .0
            .borrow()
            .last_item_size
            .map(|s| f32::from(s.contents.height) / count)
            .filter(|h| *h > 1.0)
            .unwrap_or(LINE_H)
    }

    /// Up and down: (how far scrolled, the furthest it goes, the view's
    /// height), in pixels.
    fn v_extent(&self) -> (f32, f32, f32) {
        let h = self.scroll.0.borrow().base_handle.clone();
        (
            -f32::from(h.offset().y),
            f32::from(h.max_offset().height),
            f32::from(h.bounds().size.height),
        )
    }

    /// The scroll bars' tracks: (start on screen, length). The vertical one
    /// runs down the right edge, the horizontal one along the bottom under
    /// the text (not under the line numbers).
    fn track(&self, vertical: bool) -> (f32, f32) {
        let Some(a) = self.area.get() else {
            return (0.0, 0.0);
        };
        if vertical {
            let corner = if self.hmax.get() > 0.0 { BAR } else { 0.0 };
            (f32::from(a.top()), f32::from(a.size.height) - corner)
        } else {
            (
                f32::from(a.left()) + GUTTER,
                f32::from(a.size.width) - GUTTER - BAR,
            )
        }
    }

    /// A thumb's (length, start along the track) for scroll position `at`
    /// of `max`, with `view` pixels showing.
    fn thumb(at: f32, max: f32, view: f32, track: f32) -> (f32, f32) {
        let len = (track * view / (view + max).max(1.0)).clamp(track.min(28.0), track);
        let start = if max > 0.0 {
            (track - len) * (at / max).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (len, start)
    }

    /// Scroll so a dragged thumb sits under the pointer.
    fn bar_to(&mut self, drag: BarDrag, p: Point<Pixels>) {
        let (start, track) = self.track(drag.vertical);
        let (at, max, view) = if drag.vertical {
            self.v_extent()
        } else {
            (self.hscroll.get(), self.hmax.get(), self.text_width())
        };
        let (len, _) = Self::thumb(at, max, view, track);
        let along = f32::from(if drag.vertical { p.y } else { p.x }) - start - drag.grab;
        let to = (along / (track - len).max(1.0)).clamp(0.0, 1.0) * max;
        if drag.vertical {
            let h = self.scroll.0.borrow().base_handle.clone();
            h.set_offset(point(h.offset().x, px(-to)));
        } else {
            self.hscroll.set(to);
        }
    }

    /// The line with the most characters (the editor's font is monospaced),
    /// found again only after an edit.
    fn widest_line(&self) -> usize {
        if let Some((generation, line)) = self.widest.get()
            && generation == self.generation
            && line < self.buf.line_count()
        {
            return line;
        }
        let line = (0..self.buf.line_count())
            .max_by_key(|&i| self.buf.line(i).chars().count())
            .unwrap_or(0);
        self.widest.set(Some((self.generation, line)));
        line
    }
}

thread_local! {
    /// The code font, noted each time the editor draws (Settings can change
    /// it), for measuring lines between draws.
    static EDITOR_FONT: std::cell::RefCell<gpui::SharedString> =
        std::cell::RefCell::new(crate::style::default_code_font().into());
}

fn run(len: usize, color: gpui::Hsla) -> TextRun {
    TextRun {
        len,
        font: gpui::font(EDITOR_FONT.with(|f| f.borrow().clone())),
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

    /// Unsaved Markdown edits, or form fields filled in and not saved (they
    /// live only in the open document until Save).
    pub fn has_unsaved_edits(&self) -> bool {
        self.md
            .as_ref()
            .and_then(|m| m.edit.as_ref())
            .is_some_and(|e| e.buf.dirty)
            || self.unsaved_fields() > 0
            || self.form.typing.is_some()
    }

    /// Save what the close prompt asked about: the Markdown, or the PDF's
    /// marks and form.
    pub fn save_changes(&mut self, cx: &mut Context<Self>) {
        if self.is_pdf() {
            self.save_marks(cx);
        } else {
            self.save_markdown(cx);
        }
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
        let page = (f32::from(edit.area.get().map_or(px(600.0), |b| b.size.height)) / edit.line_h())
            as isize;
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
        let line = ((y / edit.line_h()).floor().max(0.0) as usize).min(edit.buf.line_count() - 1);
        // Long lines scroll sideways: count the scroll in.
        let x = f32::from(p.x - area.origin.x) - GUTTER - PAD + edit.hscroll.get();
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

    /// The left button comes up: a drag (selection or scroll bar) ends.
    pub(super) fn editor_mouse_up(&mut self, cx: &mut Context<Self>) {
        if let Some(e) = self.md.as_mut().and_then(|m| m.edit.as_mut()) {
            e.dragging = false;
            if e.bar_drag.take().is_some() {
                cx.notify();
            }
        }
    }

    /// The pointer moves anywhere over the tab: extend a drag selection, or
    /// move a scroll bar thumb. Called from the reader, so a drag keeps
    /// going when the pointer leaves the editor.
    pub(super) fn editor_mouse_move(
        &mut self,
        e: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.md.as_mut().and_then(|m| m.edit.as_mut()) else {
            return;
        };
        if !edit.dragging && edit.bar_drag.is_none() {
            return;
        }
        if e.pressed_button != Some(MouseButton::Left) {
            edit.dragging = false;
            edit.bar_drag = None;
            cx.notify();
            return;
        }
        if let Some(drag) = edit.bar_drag {
            edit.bar_to(drag, e.position);
            cx.notify();
            return;
        }
        if let Some(at) = self.offset_at_point(e.position, window)
            && let Some(edit) = self.md.as_mut().and_then(|m| m.edit.as_mut())
        {
            edit.buf.click(at, true);
            cx.notify();
        }
    }

    /// Lines `range` of the editor, with the selection and the caret.
    fn editor_rows(&self, range: Range<usize>, window: &mut Window, cx: &App) -> Vec<AnyElement> {
        let st = Style::get(cx);
        EDITOR_FONT.with(|f| *f.borrow_mut() = st.code_font());
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
                // Text positions, less the sideways scroll.
                let hs = edit.hscroll.get();
                let x_of = |byte: usize| f32::from(shaped.x_for_index(byte.min(text.len()))) - hs;
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
                            .left(px(PAD + x0))
                            .w(px((x1 - x0).max(2.0)))
                            .h(px(LINE_H))
                            .bg(st.c(|c| c.accent).opacity(0.28))
                    });
                let caret = (ix == caret_line && focused).then(|| {
                    let x = x_of(edit.buf.head - r.start);
                    div()
                        .absolute()
                        .top(px(3.0))
                        .left(px(PAD + x))
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
                            .left(px(PAD + x0))
                            .w(px((x1 - x0).max(2.0)))
                            .h(px(1.0))
                            .bg(st.c(|c| c.accent))
                    });
                div()
                    .h(px(LINE_H))
                    .w_full()
                    .flex()
                    .when(ix == caret_line && focused, |d| {
                        d.bg(st.c(|c| c.hover).opacity(0.6))
                    })
                    // The line number stays put while the text scrolls sideways.
                    .child(
                        div()
                            .w(px(GUTTER))
                            .h_full()
                            .flex_none()
                            .flex()
                            .items_center()
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
                    // The text, clipped to its column and moved by the scroll.
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            .children(sel_box)
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left(px(PAD - hs))
                                    .h(px(LINE_H))
                                    .flex()
                                    .items_center()
                                    .whitespace_nowrap()
                                    .font_family(st.code_font())
                                    .text_size(px(FONT_SIZE))
                                    .text_color(text_color)
                                    .child(display(text)),
                            )
                            .children(marked)
                            .children(caret),
                    )
                    .into_any_element()
            })
            .collect()
    }

    /// The editor's scroll bars, where there is somewhere to scroll: down
    /// the right edge, and along the bottom under the text. Drag a thumb, or
    /// click the track to bring the thumb there.
    fn editor_bars(&self, edit: &Editor, st: &Style, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut bars = Vec::new();
        for vertical in [true, false] {
            let (at, max, view) = if vertical {
                edit.v_extent()
            } else {
                (edit.hscroll.get(), edit.hmax.get(), edit.text_width())
            };
            let (_, track) = edit.track(vertical);
            if max < 1.0 || view < 1.0 || track < 1.0 {
                continue;
            }
            let (len, start) = Editor::thumb(at, max, view, track);
            let held = edit.bar_drag.is_some_and(|d| d.vertical == vertical);
            let group: SharedString = if vertical { "md-vbar" } else { "md-hbar" }.into();
            let hover = st.ca(|c| c.text, 0.38);
            let thumb = div()
                .absolute()
                .rounded_full()
                .bg(st.ca(|c| c.text, if held { 0.45 } else { 0.22 }))
                .when(!held, |d| {
                    d.group_hover(group.clone(), move |d| d.bg(hover))
                })
                .map(|d| {
                    if vertical {
                        d.right(px(3.0)).w(px(6.0)).top(px(start)).h(px(len))
                    } else {
                        d.bottom(px(3.0)).h(px(6.0)).left(px(start)).w(px(len))
                    }
                });
            let bar = div()
                .id(if vertical {
                    "md-editor-vbar"
                } else {
                    "md-editor-hbar"
                })
                .group(group)
                .absolute()
                .cursor_default()
                .map(|d| {
                    if vertical {
                        d.top_0().right_0().w(px(BAR)).h(px(track))
                    } else {
                        d.bottom_0().left(px(GUTTER)).h(px(BAR)).w(px(track))
                    }
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |r, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let Some(edit) = r.md.as_mut().and_then(|m| m.edit.as_mut()) else {
                            return;
                        };
                        let (origin, _) = edit.track(vertical);
                        let along =
                            f32::from(if vertical { e.position.y } else { e.position.x }) - origin;
                        // On the thumb: hold it where it was grabbed. Beside
                        // it: the thumb's middle comes to the pointer.
                        let on_thumb = along >= start && along <= start + len;
                        let drag = BarDrag {
                            vertical,
                            grab: if on_thumb { along - start } else { len / 2.0 },
                        };
                        edit.bar_drag = Some(drag);
                        edit.bar_to(drag, e.position);
                        cx.notify();
                    }),
                )
                .child(thumb);
            bars.push(bar.into_any_element());
        }
        bars
    }

    /// The editor pane: a toolbar, then the lines.
    pub(super) fn editor_view(
        &self,
        st: &Style,
        window: &mut Window,
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
        // Long lines scroll sideways, up to where the widest one just fits.
        let text_w = edit.text_width();
        let widest = edit.widest_line();
        let widest_w = {
            let text = edit.buf.line(widest);
            f32::from(
                window
                    .text_system()
                    .shape_line(
                        display(text),
                        px(FONT_SIZE),
                        &[run(text.len(), gpui::black())],
                        None,
                    )
                    .width,
            )
        };
        let hmax = if edit.area.get().is_some() {
            (widest_w + PAD + 24.0 - text_w).max(0.0)
        } else {
            0.0
        };
        edit.hmax.set(hmax);
        // The view follows the caret sideways when it moves past either
        // edge, once per move (scrolling by hand doesn't pull it back).
        let head = edit.buf.head;
        if edit.followed.get() != head && text_w > 0.0 {
            edit.followed.set(head);
            let l = edit.buf.line_of(head);
            let text = edit.buf.line(l);
            let x = f32::from(
                window
                    .text_system()
                    .shape_line(
                        display(text),
                        px(FONT_SIZE),
                        &[run(text.len(), gpui::black())],
                        None,
                    )
                    .x_for_index((head - edit.buf.line_range(l).start).min(text.len())),
            );
            let hs = edit.hscroll.get();
            if x < hs {
                edit.hscroll.set((x - 32.0).max(0.0));
            } else if x > hs + text_w - PAD - 16.0 {
                edit.hscroll.set(x - text_w + PAD + 48.0);
            }
        }
        edit.hscroll.set(edit.hscroll.get().clamp(0.0, hmax));
        let lines = uniform_list("md-editor-lines", count, move |range, window, cx| {
            list_this.read(cx).editor_rows(range, window, cx)
        })
        .track_scroll(edit.scroll.clone())
        // Sideways wheel turns (Shift+wheel) scroll the text sideways (below),
        // never the lines up and down.
        .map(|mut el| {
            el.style().restrict_scroll_to_axis = Some(true);
            el
        })
        .size_full();
        let input_this = this.clone();
        let (hscroll, hmax_cell) = (edit.hscroll.clone(), edit.hmax.clone());
        let bars = self.editor_bars(edit, st, cx);
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
                        .overflow_hidden()
                        .cursor_text()
                        .on_key_down(cx.listener(Self::on_editor_key))
                        .on_mouse_down(MouseButton::Left, cx.listener(Self::editor_mouse_down))
                        .on_scroll_wheel(move |e: &gpui::ScrollWheelEvent, window, cx| {
                            let d = e.delta.pixel_delta(px(LINE_H));
                            let dx = if d.x != px(0.0) {
                                d.x
                            } else if e.modifiers.shift {
                                d.y
                            } else {
                                return;
                            };
                            let max = hmax_cell.get();
                            if max <= 0.0 {
                                return;
                            }
                            hscroll.set((hscroll.get() - f32::from(dx)).clamp(0.0, max));
                            cx.stop_propagation();
                            window.refresh();
                        })
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|r, _, _, cx| r.editor_mouse_up(cx)),
                        )
                        .child(lines)
                        // Bounds for clicks, and the IME's text input hook.
                        // Pinned to the top left: an absolute box without an
                        // inset sits below the lines, and every click would
                        // land a whole editor height off.
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
                            .top_0()
                            .left_0()
                            .size_full(),
                        )
                        .children(bars),
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
                element_bounds.origin.y + px(line as f32 * e.line_h() + scroll_y),
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
