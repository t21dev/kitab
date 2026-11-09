// SPDX-License-Identifier: AGPL-3.0-or-later

//! Page tools… (PLAN.md Phase 9): delete, move or turn pages, compress, add
//! page numbers or a watermark, save the text or the pictures. One sheet,
//! a tool and its few inputs; every tool writes a new file and the open
//! document is never changed.

use super::Workspace;
use super::tools_ui::{dialog_button, stem};
use crate::engine::PageOp;
use crate::icons::icon;
use crate::reader::Reader;
use crate::settings_view::{Click, segmented};
use crate::style::Style;
use crate::text_field::{FieldEvent, TextField};
use gpui::{
    AnyElement, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, PathPromptOptions, SharedString, StatefulInteractiveElement,
    Styled, Subscription, Window, div, prelude::FluentBuilder, px, svg,
};
use kitab_core::parse_page_ranges;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Delete,
    Move,
    Rotate,
    Compress,
    Number,
    Watermark,
    Text,
    Images,
}

impl Tool {
    const ALL: [Tool; 8] = [
        Tool::Delete,
        Tool::Move,
        Tool::Rotate,
        Tool::Compress,
        Tool::Number,
        Tool::Watermark,
        Tool::Text,
        Tool::Images,
    ];

    fn label(self) -> &'static str {
        match self {
            Tool::Delete => "Delete pages",
            Tool::Move => "Move pages",
            Tool::Rotate => "Turn pages",
            Tool::Compress => "Compress",
            Tool::Number => "Page numbers",
            Tool::Watermark => "Watermark",
            Tool::Text => "Save text",
            Tool::Images => "Save pictures",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Tool::Delete => "trash",
            Tool::Move => "pages",
            Tool::Rotate => "rotate",
            Tool::Compress => "scale",
            Tool::Number => "hash",
            Tool::Watermark => "text",
            Tool::Text => "file",
            Tool::Images => "image",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Tool::Delete => "A copy without the pages you list.",
            Tool::Move => "A copy with the pages you list moved before another page.",
            Tool::Rotate => "A copy with the pages you list turned.",
            Tool::Compress => {
                "A smaller copy: unused and repeated parts dropped, fonts, pictures and streams compressed. Pictures keep their quality."
            }
            Tool::Number => "A copy with a number at the bottom of every page.",
            Tool::Watermark => "A copy with light text across every page.",
            Tool::Text => "All the text, page by page, as a plain .txt file.",
            Tool::Images => "Every picture on the pages, as PNG files in a folder you choose.",
        }
    }

    fn takes_pages(self) -> bool {
        matches!(self, Tool::Delete | Tool::Move | Tool::Rotate)
    }
}

pub(super) struct PageToolsSheet {
    reader: Entity<Reader>,
    tool: Tool,
    pages: Entity<TextField>,
    before: Entity<TextField>,
    first: Entity<TextField>,
    mark: Entity<TextField>,
    turn: i32,
    serial: u64,
    _subs: Vec<Subscription>,
}

impl Workspace {
    pub(super) fn open_page_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reader) = self.active_reader().cloned() else {
            return;
        };
        if !reader.read(cx).is_pdf() {
            self.toast("Page tools work on PDFs.", true, None, cx);
            return;
        }
        self.menu = None;
        self.menu_closing = None;
        let page = reader.read(cx).current_page() + 1;
        let field = |text: &str, placeholder: &str, cx: &mut Context<Self>| {
            cx.new(|cx| TextField::new(text, false, placeholder, cx))
        };
        let pages = field(&page.to_string(), "1-3, 7, 10-", cx);
        let before = field("1", "Page number", cx);
        let first = field("1", "1", cx);
        let mark = field("DRAFT", "Text, like DRAFT", cx);
        let mut subs = Vec::new();
        for f in [&pages, &before, &first, &mark] {
            subs.push(
                cx.subscribe_in(f, window, |ws, _, e: &FieldEvent, window, cx| match e {
                    FieldEvent::Submit => ws.run_page_tool(window, cx),
                    FieldEvent::Cancel => ws.close_page_tools(window, cx),
                    FieldEvent::Changed => cx.notify(),
                }),
            );
        }
        pages.read(cx).focus(window);
        self.page_tools_serial += 1;
        self.page_tools = Some(PageToolsSheet {
            reader,
            tool: Tool::Delete,
            pages,
            before,
            first,
            mark,
            turn: 90,
            serial: self.page_tools_serial,
            _subs: subs,
        });
        cx.notify();
    }

    pub(super) fn close_page_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page_tools.is_none() {
            return;
        }
        self.exit_modal("page-tools", window, cx, |ws, window, cx| {
            if ws.page_tools.take().is_some() {
                ws.refocus(window, cx);
                cx.notify();
            }
        });
    }

    /// The operation the sheet describes, or why it can't run yet.
    fn page_op(&self, cx: &Context<Self>) -> Result<PageOp, String> {
        let s = self.page_tools.as_ref().ok_or_else(String::new)?;
        let count = s.reader.read(cx).page_count() as u32;
        let pages = || parse_page_ranges(&s.pages.read(cx).text(), count);
        let number = |f: &Entity<TextField>| f.read(cx).text().trim().parse::<u32>().ok();
        Ok(match s.tool {
            Tool::Delete => {
                let p = pages()?;
                if p.len() as u32 >= count {
                    return Err("That's every page; at least one has to stay.".into());
                }
                PageOp::Delete(p)
            }
            Tool::Move => {
                let p = pages()?;
                let before = number(&s.before)
                    .filter(|n| (1..=count + 1).contains(n))
                    .ok_or(format!("Move before a page from 1 to {}.", count + 1))?;
                PageOp::Move(p, before - 1)
            }
            Tool::Rotate => PageOp::Rotate(pages()?, s.turn),
            Tool::Compress => PageOp::Compress,
            Tool::Number => PageOp::Number(
                number(&s.first).ok_or("Start at a whole number, like 1.".to_string())?,
            ),
            Tool::Watermark => {
                let t = s.mark.read(cx).text().trim().to_string();
                if t.is_empty() {
                    return Err(String::new());
                }
                PageOp::Watermark(t)
            }
            Tool::Text => PageOp::Text,
            Tool::Images => PageOp::Images,
        })
    }

    fn run_page_tool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(op) = self.page_op(cx) else {
            return;
        };
        let Some(sheet) = self.page_tools.take() else {
            return;
        };
        self.refocus(window, cx);
        cx.notify();
        let r = sheet.reader.read(cx);
        let (src, unlock) = (r.path.clone(), r.unlock());
        let dir = src.parent().map(Path::to_path_buf).unwrap_or_default();
        let name = stem(&src);
        let engine = self.engine.clone();
        if op == PageOp::Images {
            let rx = cx.prompt_for_paths(PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some("Save the pictures here".into()),
            });
            cx.spawn(async move |this, cx| {
                let Ok(Ok(Some(mut picked))) = rx.await else {
                    return;
                };
                let Some(folder) = picked.pop() else { return };
                let out = folder.join(format!("{name} pictures"));
                let result = cx
                    .background_spawn(async move {
                        engine
                            .page_tool(&src, unlock.as_deref(), &op, &out)
                            .map(|said| (said, out))
                    })
                    .await;
                let _ = this.update(cx, |ws, cx| match result {
                    Ok((said, out)) => ws.report(Ok(out), &format!("{said} to"), cx),
                    Err(e) => ws.report(Err(e), "", cx),
                });
            })
            .detach();
            return;
        }
        let suggested = match &op {
            PageOp::Text => format!("{name}.txt"),
            PageOp::Delete(_) => format!("{name} (pages deleted).pdf"),
            PageOp::Move(..) => format!("{name} (reordered).pdf"),
            PageOp::Rotate(..) => format!("{name} (turned).pdf"),
            PageOp::Compress => format!("{name} (compressed).pdf"),
            PageOp::Number(_) => format!("{name} (numbered).pdf"),
            PageOp::Watermark(_) => format!("{name} (watermarked).pdf"),
            PageOp::Images => unreachable!("handled above"),
        };
        let ext = if op == PageOp::Text { "txt" } else { "pdf" };
        let rx = cx.prompt_for_new_path(&dir, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut out))) = rx.await else {
                return;
            };
            if out.extension().is_none() {
                out.set_extension(ext);
            }
            let result = cx
                .background_spawn(async move {
                    engine
                        .page_tool(&src, unlock.as_deref(), &op, &out)
                        .map(|said| (said, out))
                })
                .await;
            let _ = this.update(cx, |ws, cx| match result {
                Ok((said, out)) => ws.report(Ok(out), &format!("{said}. Saved to"), cx),
                Err(e) => ws.report(Err(e), "", cx),
            });
        })
        .detach();
    }

    pub(super) fn page_tools_view(
        &mut self,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = self.page_tools.as_ref()?;
        let r = s.reader.read(cx);
        let (title, count) = (r.title(), r.page_count());
        let op = self.page_op(cx);
        let ws = cx.weak_entity();
        // The tools, as a grid of choices.
        let mut grid = div().flex().flex_wrap().gap(px(6.0));
        for (i, t) in Tool::ALL.into_iter().enumerate() {
            let on = s.tool == t;
            grid = grid.child(
                div()
                    .id(("page-tool", i))
                    .w(px(124.0))
                    .h(px(30.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .rounded(px(8.0))
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .when(on, |d| {
                        d.bg(st.c(|c| c.hover))
                            .border_1()
                            .border_color(st.c(|c| c.accent))
                            .text_color(st.c(|c| c.text))
                    })
                    .when(!on, |d| {
                        d.border_1()
                            .border_color(st.c(|c| c.line))
                            .text_color(st.c(|c| c.muted))
                            .hover(|d| d.bg(st.hover_bg()))
                    })
                    .on_click(cx.listener(move |ws, _, window, cx| {
                        if let Some(s) = ws.page_tools.as_mut() {
                            s.tool = t;
                            match t {
                                Tool::Watermark => s.mark.read(cx).focus(window),
                                Tool::Number => s.first.read(cx).focus(window),
                                _ if t.takes_pages() => s.pages.read(cx).focus(window),
                                _ => {}
                            }
                        }
                        cx.notify();
                    }))
                    .child(
                        svg()
                            .path(icon(t.icon()))
                            .size(px(13.0))
                            .flex_none()
                            .text_color(if on {
                                st.c(|c| c.accent)
                            } else {
                                st.c(|c| c.faint)
                            }),
                    )
                    .child(div().truncate().child(t.label())),
            );
        }
        let field = |f: &Entity<TextField>| {
            div()
                .flex_1()
                .px(px(10.0))
                .py(px(6.0))
                .rounded(st.px(|m| m.control_radius))
                .bg(st.c(|c| c.hover))
                .border_1()
                .border_color(st.c(|c| c.line))
                .child(f.clone())
        };
        let row = |label: &'static str, control: AnyElement| {
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .w(px(84.0))
                        .flex_none()
                        .text_size(px(12.5))
                        .text_color(st.c(|c| c.muted))
                        .child(label),
                )
                .child(control)
        };
        let turn = |deg: i32| -> Click {
            let ws = ws.clone();
            Box::new(move |_, cx| {
                let _ = ws.update(cx, |ws, cx| {
                    if let Some(s) = ws.page_tools.as_mut() {
                        s.turn = deg;
                    }
                    cx.notify();
                });
            })
        };
        let (note, bad) = match &op {
            Err(e) => (e.clone(), !e.is_empty()),
            Ok(PageOp::Delete(p) | PageOp::Move(p, _) | PageOp::Rotate(p, _)) => (
                if p.len() == 1 {
                    "1 page".into()
                } else {
                    format!("{} pages", p.len())
                },
                false,
            ),
            Ok(_) => (String::new(), false),
        };
        let ready = op.is_ok();
        let panel = div()
            .id("page-tools")
            .w(px(560.0))
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
                    .child("Page tools"),
            )
            .child(grid)
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.muted))
                    .child(format!(
                        "{} From {title} ({count} pages); the original stays as it is.",
                        s.tool.about()
                    )),
            )
            .when(s.tool.takes_pages(), |d| {
                d.child(row("Pages", field(&s.pages).into_any_element()))
            })
            .when(s.tool == Tool::Move, |d| {
                d.child(row("Before page", field(&s.before).into_any_element()))
            })
            .when(s.tool == Tool::Rotate, |d| {
                d.child(row(
                    "Turn",
                    segmented(
                        vec![
                            (SharedString::from("Right"), s.turn == 90, turn(90)),
                            (SharedString::from("Left"), s.turn == 270, turn(270)),
                            (SharedString::from("Upside down"), s.turn == 180, turn(180)),
                        ],
                        st,
                    ),
                ))
            })
            .when(s.tool == Tool::Number, |d| {
                d.child(row("Start at", field(&s.first).into_any_element()))
            })
            .when(s.tool == Tool::Watermark, |d| {
                d.child(row("Text", field(&s.mark).into_any_element()))
            })
            .when(!note.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(if bad {
                            st.c(|c| c.bad)
                        } else {
                            st.c(|c| c.faint)
                        })
                        .child(note),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        dialog_button(st, "page-tools-cancel", "Cancel", false).on_click(
                            cx.listener(|ws, _, window, cx| ws.close_page_tools(window, cx)),
                        ),
                    )
                    .child(
                        dialog_button(st, "page-tools-go", "Save…", true)
                            .when(!ready, |b| b.opacity(0.45))
                            .on_click(
                                cx.listener(|ws, _, window, cx| ws.run_page_tool(window, cx)),
                            ),
                    ),
            );
        let serial = s.serial;
        Some(self.modal_layer(
            "page-tools",
            serial,
            panel.into_any_element(),
            st,
            Self::close_page_tools,
            cx,
        ))
    }
}
