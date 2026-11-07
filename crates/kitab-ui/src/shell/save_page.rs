// SPDX-License-Identifier: AGPL-3.0-or-later

//! "Save page as…": one menu entry, one small dialog with every option (the
//! page, the format, and the resolution for images), then the system Save
//! dialog. DESIGN.md § Small dialogs.

use super::Workspace;
use super::tools_ui::{dialog_button, stem};
use crate::icons::icon;
use crate::reader::{PageExport, Reader};
use crate::style::Style;
use gpui::{
    AnyElement, Context, Entity, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px, svg,
};

/// Image resolutions offered, in dots per inch.
pub(crate) const DPI_CHOICES: [u32; 3] = [96, 150, 300];

pub(super) struct SavePageDialog {
    pub reader: Entity<Reader>,
    /// The page field's text (1-based).
    pub page: String,
    pub kind: PageExport,
    pub dpi: u32,
    pub serial: u64,
}

impl SavePageDialog {
    fn page_ix(&self, count: usize) -> Option<usize> {
        let n: usize = self.page.trim().parse().ok()?;
        (1..=count).contains(&n).then(|| n - 1)
    }
}

const KINDS: [(PageExport, &str, &str, &str); 3] = [
    (PageExport::Pdf, "PDF", "file", "Text stays text"),
    (PageExport::Image, "Image", "image", "PNG picture"),
    (PageExport::Text, "Text", "text", "Plain .txt"),
];

impl Workspace {
    pub(super) fn open_save_page(
        &mut self,
        reader: Entity<Reader>,
        page: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind = if reader.read(cx).is_pdf() {
            PageExport::Pdf
        } else {
            PageExport::Image
        };
        self.menu = None;
        self.menu_closing = None;
        self.menu_serial += 1;
        self.tools.save = Some(SavePageDialog {
            reader,
            page: format!("{}", page + 1),
            kind,
            dpi: crate::reader::EXPORT_DPI as u32,
            serial: self.menu_serial,
        });
        window.focus(&self.tools.dialog_focus);
        cx.notify();
    }

    pub(super) fn close_save_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tools.save.take().is_some() {
            self.refocus(window, cx);
            cx.notify();
        }
    }

    fn set_save_kind(&mut self, kind: PageExport, cx: &mut Context<Self>) {
        if let Some(d) = self.tools.save.as_mut() {
            let pdf_ok = d.reader.read(cx).is_pdf();
            if kind != PageExport::Pdf || pdf_ok {
                d.kind = kind;
                cx.notify();
            }
        }
    }

    fn save_page_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.tools.save.as_mut() else {
            return;
        };
        let k = &e.keystroke;
        match k.key.as_str() {
            "enter" => {
                self.confirm_save_page(window, cx);
                cx.stop_propagation();
                return;
            }
            "backspace" => {
                d.page.pop();
            }
            "left" | "right" => {
                let at = KINDS.iter().position(|(kd, ..)| *kd == d.kind).unwrap_or(0);
                let step = if k.key == "left" { KINDS.len() - 1 } else { 1 };
                let mut next = (at + step) % KINDS.len();
                if KINDS[next].0 == PageExport::Pdf && !d.reader.read(cx).is_pdf() {
                    next = (next + step) % KINDS.len();
                }
                d.kind = KINDS[next].0;
            }
            _ => {
                if let Some(ch) = k.key_char.as_ref() {
                    let digits = ch.chars().all(|c| c.is_ascii_digit());
                    if digits && !k.modifiers.control && !k.modifiers.alt && d.page.len() < 7 {
                        d.page.push_str(ch);
                    }
                }
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn confirm_save_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.tools.save.as_ref() else {
            return;
        };
        let r = d.reader.read(cx);
        let Some(page) = d.page_ix(r.page_count()) else {
            return; // the dialog already says why
        };
        let (reader, kind, dpi) = (d.reader.clone(), d.kind, d.dpi);
        let name = format!("{} - page {}.{}", stem(&r.path), page + 1, kind.extension());
        let dir = r.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        self.close_save_page(window, cx);
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut out))) = rx.await else {
                return;
            };
            if out.extension().is_none() {
                out.set_extension(kind.extension());
            }
            let ws = this.clone();
            let _ = reader.update(cx, |r, cx| {
                r.save_page(page, kind, dpi as f32, out, cx, move |result, cx| {
                    let _ = ws.update(cx, |ws, cx| ws.report(result, "Saved", cx));
                })
            });
        })
        .detach();
    }

    pub(super) fn save_page_view(
        &mut self,
        st: &Style,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let d = self.tools.save.as_ref()?;
        let r = d.reader.read(cx);
        let count = r.page_count();
        let is_pdf = r.is_pdf();
        let page = d.page_ix(count);
        let (kind, dpi, serial) = (d.kind, d.dpi, d.serial);
        let size = page.map(|p| r.page_size_pt(p));
        let focused = self.tools.dialog_focus.is_focused(window);
        let bad = page.is_none();
        let note = match (page, kind, size) {
            (None, ..) => format!("Type a page from 1 to {count}."),
            (Some(_), PageExport::Image, Some(s)) => {
                let k = dpi as f32 / 72.0;
                format!(
                    "{} × {} pixels",
                    (s.w * k).round() as u32,
                    (s.h * k).round() as u32
                )
            }
            (Some(_), PageExport::Pdf, _) => {
                "A one-page PDF. Text stays selectable and links keep working.".into()
            }
            (Some(_), PageExport::Text, _) => "The page's text in reading order.".into(),
            _ => String::new(),
        };

        let field = div()
            .id("save-page-field")
            .track_focus(&self.tools.dialog_focus)
            .on_key_down(cx.listener(Self::save_page_key))
            .w(px(96.0))
            .h(st.px(|m| m.control_height) + px(6.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .rounded(st.px(|m| m.control_radius))
            .bg(st.c(|c| c.hover))
            .shadow(st.well())
            .border_1()
            .border_color(if bad {
                st.c(|c| c.bad)
            } else if focused {
                st.c(|c| c.accent)
            } else {
                st.c(|c| c.line)
            })
            .text_size(st.px(|m| m.ui_text))
            .text_color(st.c(|c| c.text))
            .child(d.page.clone())
            .when(focused, |f| {
                f.child(
                    div()
                        .ml(px(1.0))
                        .w(px(1.0))
                        .h(px(15.0))
                        .bg(st.c(|c| c.accent)),
                )
            });

        let label = |text: &'static str| {
            div()
                .w(px(86.0))
                .flex_none()
                .text_size(px(12.5))
                .text_color(st.c(|c| c.muted))
                .child(text)
        };

        let mut cards = div().flex_1().flex().gap(px(8.0));
        for (ix, (k, name, glyph, sub)) in KINDS.into_iter().enumerate() {
            let on = k == kind;
            let disabled = k == PageExport::Pdf && !is_pdf;
            cards = cards.child(
                div()
                    .id(("save-kind", ix))
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .rounded(px(10.0))
                    // The selected card's 2 px ring takes 1 px of padding, so
                    // nothing inside moves when the choice changes.
                    .when(!on, |c| {
                        c.p(px(11.0)).border_1().border_color(st.c(|c| c.line))
                    })
                    .when(on, |c| {
                        c.p(px(10.0))
                            .bg(st.c(|c| c.raised))
                            .shadow(st.elev())
                            .border_2()
                            .border_color(st.c(|c| c.accent))
                    })
                    .when(disabled, |c| c.opacity(0.45))
                    .when(!on && !disabled, |c| {
                        c.cursor_pointer()
                            .hover(|c| c.bg(st.c(|c| c.hover)))
                            .on_click(cx.listener(move |ws, _, _, cx| ws.set_save_kind(k, cx)))
                    })
                    .child(svg().path(icon(glyph)).size(px(18.0)).text_color(if on {
                        st.c(|c| c.accent)
                    } else {
                        st.c(|c| c.muted)
                    }))
                    .child(
                        div()
                            .text_size(st.px(|m| m.ui_text))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(st.c(|c| c.text))
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(st.c(|c| c.faint))
                            .child(sub),
                    ),
            );
        }

        let resolution = (kind == PageExport::Image).then(|| {
            let mut group = div()
                .p(px(2.0))
                .flex()
                .gap(px(2.0))
                .rounded(st.px(|m| m.control_radius))
                .bg(st.c(|c| c.hover))
                .shadow(st.well());
            for (ix, choice) in DPI_CHOICES.into_iter().enumerate() {
                let on = choice == dpi;
                group = group.child(
                    div()
                        .id(("save-dpi", ix))
                        .h(px(26.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .rounded(px(6.0))
                        .text_size(px(12.0))
                        .text_color(if on {
                            st.c(|c| c.text)
                        } else {
                            st.c(|c| c.muted)
                        })
                        .when(on, |e| e.bg(st.c(|c| c.raised)).shadow(st.elev()))
                        .when(!on, |e| {
                            e.cursor_pointer().hover(|e| e.text_color(st.c(|c| c.text)))
                        })
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            if let Some(d) = ws.tools.save.as_mut() {
                                d.dpi = choice;
                                cx.notify();
                            }
                        }))
                        .child(format!("{choice} dpi")),
                );
            }
            div()
                .flex()
                .items_center()
                .child(label("Resolution"))
                .child(group)
        });

        let panel = div()
            .id("save-page-dialog")
            .relative()
            .w(px(460.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(22.0))
            .rounded(px(16.0))
            .bg(st.c(|c| c.raised))
            .shadow(st.modal())
            .border_1()
            .border_color(st.c(|c| c.line))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(st.c(|c| c.text))
                    .child("Save page"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(label("Page"))
                    .child(field)
                    .child(
                        div()
                            .ml(px(8.0))
                            .text_size(px(12.5))
                            .text_color(st.c(|c| c.faint))
                            .child(format!("of {count}")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .child(div().pt(px(10.0)).child(label("Format")))
                    .child(cards),
            )
            .children(resolution)
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(if bad {
                        st.c(|c| c.bad)
                    } else {
                        st.c(|c| c.faint)
                    })
                    .child(note),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        dialog_button(st, "save-page-cancel", "Cancel", false).on_click(
                            cx.listener(|ws, _, window, cx| ws.close_save_page(window, cx)),
                        ),
                    )
                    .child(
                        dialog_button(st, "save-page-ok", "Save…", true)
                            .when(bad, |b| b.opacity(0.45))
                            .on_click(
                                cx.listener(|ws, _, window, cx| ws.confirm_save_page(window, cx)),
                            ),
                    ),
            );
        Some(self.modal_layer(
            "save-page",
            serial,
            panel.into_any_element(),
            st,
            Self::close_save_page,
            cx,
        ))
    }
}
