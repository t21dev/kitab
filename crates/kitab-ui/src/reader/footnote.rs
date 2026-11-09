// SPDX-License-Identifier: AGPL-3.0-or-later

//! Footnote popovers (PLAN.md Phase 4, MANIFEST `epub/03`): a link whose own
//! text is a note mark (1, [2], *, †) shows the note beside it instead of
//! jumping away; Go to note still jumps. The mark and the note are read from
//! the pages' characters off the UI thread (`kitab_core::notes`).

use super::*;
use gpui::ElementId;
use kitab_core::notes::{is_note_mark, note_text, text_in};
use kitab_core::{Link, LinkTarget};

/// Notes longer than this are cut, with Go to note for the rest.
const NOTE_MAX: usize = 700;

pub(super) struct NotePop {
    pub mark: String,
    pub text: String,
    /// Window position of the click, where the card opens.
    pub at: Point<Pixels>,
    pub target: LinkTarget,
}

impl Reader {
    /// A click on a link: a note mark opens its note; any other link is
    /// followed.
    pub(super) fn click_link(
        &mut self,
        ix: usize,
        link: Link,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let (Some(doc), LinkTarget::Page { page: to, y }) = (self.doc, link.target.clone()) else {
            self.follow(&link.target, cx);
            return;
        };
        let engine = self.engine.clone();
        let rect = link.rect;
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    let here = engine.page_chars(doc, ix as u32);
                    let mark = text_in(&here, rect);
                    if !is_note_mark(&mark) {
                        return None;
                    }
                    let there = engine.page_chars(doc, to);
                    let text = note_text(&there, y, Some(&mark), NOTE_MAX);
                    (!text.is_empty()).then(|| (mark.trim().to_string(), text))
                })
                .await;
            let _ = this.update(cx, |r, cx| match found {
                Some((mark, text)) => {
                    r.note = Some(NotePop {
                        mark,
                        text,
                        at,
                        target: link.target,
                    });
                    cx.notify();
                }
                None => r.follow(&link.target, cx),
            });
        })
        .detach();
    }

    pub(super) fn close_note(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.note.take().is_some();
        if open {
            cx.notify();
        }
        open
    }

    fn go_to_note(&mut self, cx: &mut Context<Self>) {
        if let Some(n) = self.note.take() {
            self.follow(&n.target, cx);
            cx.notify();
        }
    }

    /// The open note, by the mark it came from.
    pub(super) fn note_view(&self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        let n = self.note.as_ref()?;
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(n.at + gpui::point(px(-12.0), px(14.0)))
                    .snap_to_window_with_margin(px(12.0))
                    .child(crate::motion::enter(
                        div()
                            .id("footnote")
                            .occlude()
                            .on_mouse_down_out(cx.listener(|r, _, _, cx| {
                                r.close_note(cx);
                            }))
                            .w(px(360.0))
                            .max_h(px(320.0))
                            .overflow_y_scroll()
                            .p(px(14.0))
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .rounded(px(12.0))
                            .bg(st.c(|c| c.raised))
                            .shadow(st.float())
                            .border_1()
                            .border_color(st.c(|c| c.line))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(11.5))
                                    .text_color(st.c(|c| c.faint))
                                    .child(format!("Note {}", n.mark))
                                    .child(div().flex_1())
                                    .child(
                                        div()
                                            .id("footnote-go")
                                            .px(px(8.0))
                                            .h(px(24.0))
                                            .flex()
                                            .items_center()
                                            .rounded(px(6.0))
                                            .text_color(st.c(|c| c.accent))
                                            .cursor_pointer()
                                            .hover(|d| d.bg(st.hover_bg()))
                                            .tip("Jump to the note; Alt+Left comes back")
                                            .on_click(cx.listener(|r, _, _, cx| r.go_to_note(cx)))
                                            .child("Go to note"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .line_height(px(20.0))
                                    .text_color(st.c(|c| c.text))
                                    .child(n.text.clone()),
                            ),
                        // Each note opens with a short drop from its mark.
                        ElementId::NamedInteger(
                            "footnote-in".into(),
                            (f32::from(n.at.x) as u64) << 20 ^ f32::from(n.at.y) as u64,
                        ),
                        st.animations,
                        crate::motion::From::Above(4.0),
                        crate::motion::FAST,
                        std::time::Duration::ZERO,
                    )),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
