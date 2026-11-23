// SPDX-License-Identifier: AGPL-3.0-or-later

//! Page tools… (PLAN.md Phase 9): arrange pages (delete, move or turn them,
//! on the page grid of arrange_ui.rs), compress, add page numbers or a
//! watermark, save the text or the pictures. One sheet, a tool and its few
//! inputs; every tool writes a new file and the open document is never
//! changed.

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
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tool {
    /// Opens the page grid (its own sheet) rather than taking inputs here.
    Arrange,
    Compress,
    Number,
    Watermark,
    Text,
    Images,
}

impl Tool {
    const ALL: [Tool; 6] = [
        Tool::Arrange,
        Tool::Compress,
        Tool::Number,
        Tool::Watermark,
        Tool::Text,
        Tool::Images,
    ];

    fn label(self) -> &'static str {
        match self {
            Tool::Arrange => "Arrange pages",
            Tool::Compress => "Compress",
            Tool::Number => "Page numbers",
            Tool::Watermark => "Watermark",
            Tool::Text => "Save text",
            Tool::Images => "Save pictures",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Tool::Arrange => "pages",
            Tool::Compress => "scale",
            Tool::Number => "hash",
            Tool::Watermark => "text",
            Tool::Text => "file",
            Tool::Images => "image",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Tool::Arrange => "Delete, move or turn pages on a grid of them.",
            Tool::Compress => {
                "A smaller copy. Balanced and Smallest scale down pictures larger than the pages need; text, drawings and fonts stay as sharp as before. Same quality only drops unused and repeated parts."
            }
            Tool::Number => "A copy with a number at the bottom of every page.",
            Tool::Watermark => "A copy with light text across every page.",
            Tool::Text => "All the text, page by page, as a plain .txt file.",
            Tool::Images => "Every picture on the pages, as PNG files in a folder you choose.",
        }
    }
}

/// Compress: the size a quality comes to, found by making the copy.
#[derive(Clone)]
enum Estimate {
    Working,
    /// Bytes before and after, and whether that's the copy itself (a small
    /// PDF) or worked out from a few of its pages.
    Done(u64, u64, bool),
    /// Why there's no smaller copy (already as small as it gets, or an error).
    Failed(String),
}

fn shrink_ix(s: crate::Shrink) -> usize {
    match s {
        crate::Shrink::Lossless => 0,
        crate::Shrink::Balanced => 1,
        crate::Shrink::Smallest => 2,
    }
}

pub(super) struct PageToolsSheet {
    reader: Entity<Reader>,
    tool: Tool,
    /// Opened for one tool (the file manager's right-click menu, File ›
    /// Make a smaller copy…): that tool alone, without the others to pick.
    single: bool,
    first: Entity<TextField>,
    mark: Entity<TextField>,
    /// Compress: how much smaller.
    shrink: crate::Shrink,
    /// Compress: each quality's result, once worked out (by `shrink_ix`).
    estimates: [Option<Estimate>; 3],
    serial: u64,
    _subs: Vec<Subscription>,
}

impl Workspace {
    pub(super) fn open_page_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(reader) = self.active_reader().cloned() {
            self.open_page_tools_for(reader, None, window, cx);
        }
    }

    /// Page tools on `reader`, its tab brought forward; on `tool` alone
    /// when given, else on the picker of every tool.
    pub(super) fn open_page_tools_for(
        &mut self,
        reader: Entity<Reader>,
        tool: Option<Tool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !reader.read(cx).is_pdf() {
            self.toast("Page tools work on PDFs.", true, None, cx);
            return;
        }
        if let Some(ix) = self.tabs.iter().position(|t| *t == reader)
            && self.active != Some(ix)
        {
            self.activate(ix, window, cx);
        }
        self.menu = None;
        self.menu_closing = None;
        if tool == Some(Tool::Arrange) {
            self.open_arrange(reader, window, cx);
            return;
        }
        let field = |text: &str, placeholder: &str, cx: &mut Context<Self>| {
            cx.new(|cx| TextField::new(text, false, placeholder, cx))
        };
        let first = field("1", "1", cx);
        let mark = field("DRAFT", "Text, like DRAFT", cx);
        let mut subs = Vec::new();
        for f in [&first, &mark] {
            subs.push(
                cx.subscribe_in(f, window, |ws, _, e: &FieldEvent, window, cx| match e {
                    FieldEvent::Submit => ws.run_page_tool(window, cx),
                    FieldEvent::Cancel => ws.close_page_tools(window, cx),
                    FieldEvent::Changed => cx.notify(),
                }),
            );
        }
        let single = tool.is_some();
        let tool = tool.unwrap_or(Tool::Compress);
        match tool {
            Tool::Watermark => mark.read(cx).focus(window),
            Tool::Number => first.read(cx).focus(window),
            _ => {}
        }
        self.page_tools_serial += 1;
        self.page_tools = Some(PageToolsSheet {
            reader,
            tool,
            single,
            first,
            mark,
            shrink: crate::Shrink::Balanced,
            estimates: [None, None, None],
            serial: self.page_tools_serial,
            _subs: subs,
        });
        self.compress_estimate(cx);
        cx.notify();
    }

    /// Compress shows what its copy will weigh, worked out in the background
    /// once per quality: a small PDF's copy is made (and thrown away); a big
    /// one's from a few of its pages, spread through it, compressed alone:
    /// the whole copy takes minutes on a long scan, a few pages a moment.
    fn compress_estimate(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.page_tools.as_mut() else {
            return;
        };
        let ix = shrink_ix(s.shrink);
        if s.tool != Tool::Compress || s.estimates[ix].is_some() {
            return;
        }
        s.estimates[ix] = Some(Estimate::Working);
        let (shrink, serial) = (s.shrink, s.serial);
        let r = s.reader.read(cx);
        let (src, unlock, count) = (r.path.clone(), r.unlock(), r.page_count() as u32);
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let estimate = cx
                .background_spawn(async move {
                    const SAMPLE: u32 = 4;
                    let before = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
                    let tmp = |what: &str| {
                        std::env::temp_dir().join(format!(
                            "kitab-estimate-{}-{serial}-{ix}-{what}.pdf",
                            std::process::id()
                        ))
                    };
                    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).ok();
                    let compress = PageOp::Compress(shrink);
                    let exact = count <= SAMPLE + 2;
                    // The pages measured: all of a small PDF, or a few of a big one.
                    let (from, from_size) = if exact {
                        (src.clone(), Some(before))
                    } else {
                        let pages = (0..SAMPLE)
                            .map(|i| (i * 2 + 1) * count / (SAMPLE * 2))
                            .map(|p| (p, 0))
                            .collect();
                        let sample = tmp("sample");
                        if let Err(e) = engine.page_tool(
                            &src,
                            unlock.as_deref(),
                            &PageOp::Arrange(pages),
                            &sample,
                        ) {
                            return Estimate::Failed(e);
                        }
                        let n = size(&sample);
                        (sample, n)
                    };
                    let out = tmp("out");
                    let made = engine.page_tool(&from, unlock.as_deref(), &compress, &out);
                    let after = size(&out);
                    let _ = std::fs::remove_file(&out);
                    if !exact {
                        let _ = std::fs::remove_file(&from);
                    }
                    match (made, after, from_size) {
                        (Ok(_), Some(after), Some(measured)) if measured > 0 => {
                            // The whole file shrinks as its sample did.
                            let after = (before as f64 * after as f64 / measured as f64) as u64;
                            Estimate::Done(before, after.min(before), exact)
                        }
                        (Err(e), ..) => Estimate::Failed(e),
                        _ => Estimate::Failed("Couldn't work out the new size.".into()),
                    }
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                if let Some(s) = ws.page_tools.as_mut().filter(|s| s.serial == serial) {
                    s.estimates[ix] = Some(estimate);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// File › Make a smaller copy… (and Ctrl+K): Page tools, on Compress.
    pub(super) fn open_page_tools_on_compress(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(reader) = self.active_reader().cloned() {
            self.open_page_tools_for(reader, Some(Tool::Compress), window, cx);
        }
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
        let number = |f: &Entity<TextField>| f.read(cx).text().trim().parse::<u32>().ok();
        Ok(match s.tool {
            Tool::Arrange => return Err(String::new()),
            Tool::Compress => PageOp::Compress(s.shrink),
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
                let picked = rx.await;
                // The picker couldn't open (Linux without a desktop portal): say so.
                if matches!(picked, Ok(Err(_)) | Err(_)) {
                    let _ = this.update(cx, |ws, cx| ws.picker_failed(cx));
                }
                let Ok(Ok(Some(mut picked))) = picked else {
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
            PageOp::Compress(_) => format!("{name} (compressed).pdf"),
            PageOp::Number(_) => format!("{name} (numbered).pdf"),
            PageOp::Watermark(_) => format!("{name} (watermarked).pdf"),
            _ => format!("{name} (edited).pdf"),
        };
        let ext = if op == PageOp::Text { "txt" } else { "pdf" };
        self.save_page_op(&sheet.reader, op, suggested, ext, cx);
    }

    /// Ask where, then write `op` on `reader`'s file there (`suggested`
    /// as the name, `ext` when none is typed) and say how it went.
    pub(super) fn save_page_op(
        &mut self,
        reader: &Entity<Reader>,
        op: PageOp,
        suggested: String,
        ext: &'static str,
        cx: &mut Context<Self>,
    ) {
        let r = reader.read(cx);
        let (src, unlock) = (r.path.clone(), r.unlock());
        let dir = src.parent().map(Path::to_path_buf).unwrap_or_default();
        let engine = self.engine.clone();
        let rx = cx.prompt_for_new_path(&dir, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let picked = rx.await;
            // The picker couldn't open (Linux without a desktop portal): say so.
            if matches!(picked, Ok(Err(_)) | Err(_)) {
                let _ = this.update(cx, |ws, cx| ws.picker_failed(cx));
            }
            let Ok(Ok(Some(mut out))) = picked else {
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
                        if t == Tool::Arrange {
                            if let Some(r) = ws.page_tools.as_ref().map(|s| s.reader.clone()) {
                                ws.open_arrange(r, window, cx);
                            }
                            return;
                        }
                        if let Some(s) = ws.page_tools.as_mut() {
                            s.tool = t;
                            match t {
                                Tool::Watermark => s.mark.read(cx).focus(window),
                                Tool::Number => s.first.read(cx).focus(window),
                                _ => {}
                            }
                        }
                        ws.compress_estimate(cx);
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
        let (note, bad) = match &op {
            Err(e) => (e.clone(), !e.is_empty()),
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
                    .child(if s.single {
                        s.tool.label()
                    } else {
                        "Page tools"
                    }),
            )
            .when(!s.single, |d| d.child(grid))
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.muted))
                    .child(format!(
                        "{} From {title} ({count} pages); the original stays as it is.",
                        s.tool.about()
                    )),
            )
            .when(s.tool == Tool::Compress, |d| {
                use crate::Shrink;
                let shrink = |to: Shrink| -> Click {
                    let ws = ws.clone();
                    Box::new(move |_, cx| {
                        let _ = ws.update(cx, |ws, cx| {
                            if let Some(s) = ws.page_tools.as_mut() {
                                s.shrink = to;
                            }
                            ws.compress_estimate(cx);
                            cx.notify();
                        });
                    })
                };
                d.child(row(
                    "Quality",
                    segmented(
                        vec![
                            (
                                SharedString::from("Balanced"),
                                s.shrink == Shrink::Balanced,
                                shrink(Shrink::Balanced),
                            ),
                            (
                                SharedString::from("Smallest"),
                                s.shrink == Shrink::Smallest,
                                shrink(Shrink::Smallest),
                            ),
                            (
                                SharedString::from("Same quality"),
                                s.shrink == Shrink::Lossless,
                                shrink(Shrink::Lossless),
                            ),
                        ],
                        st,
                    ),
                ))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(st.c(|c| c.faint))
                        .child(match s.shrink {
                            Shrink::Balanced => {
                                "Pictures at most 150 dpi: fine for reading and printing."
                            }
                            Shrink::Smallest => "Pictures at most 96 dpi: for sending by email.",
                            Shrink::Lossless => "Pictures unchanged.",
                        }),
                )
                .child(row("Size", {
                    use crate::reader::human_size as size;
                    let (text, strong) = match &s.estimates[shrink_ix(s.shrink)] {
                        None | Some(Estimate::Working) => ("Estimating…".to_string(), false),
                        Some(Estimate::Done(before, after, exact)) => {
                            let pct = (after * 100)
                                .checked_div(*before)
                                .map_or(0, |share| 100 - share.min(100));
                            let text = if *exact {
                                format!("{} → {} ({pct}% smaller)", size(*before), size(*after))
                            } else if pct < 3 {
                                "About the same size: little to gain.".to_string()
                            } else {
                                format!(
                                    "About {pct}% smaller: {} → about {}",
                                    size(*before),
                                    size(*after)
                                )
                            };
                            (text, true)
                        }
                        Some(Estimate::Failed(why)) => (why.clone(), false),
                    };
                    div()
                        .text_size(px(12.5))
                        .text_color(if strong {
                            st.c(|c| c.text)
                        } else {
                            st.c(|c| c.faint)
                        })
                        .child(text)
                        .into_any_element()
                }))
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
