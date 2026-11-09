// SPDX-License-Identifier: AGPL-3.0-or-later

//! The print sheet (Ctrl+P, PLAN.md §3.8): which pages, how big, and whether
//! pages turn to match the paper; then Print… opens the system's print
//! dialog for the printer and copies, and the pages go out in the
//! background, each drawn by MuPDF at the printer's resolution. Progress and
//! the outcome show in a toast.

use super::Workspace;
use super::tools_ui::dialog_button;
use crate::engine::DocEngine;
use crate::reader::Reader;
use crate::settings_view::{Click, segmented};
use crate::style::Style;
use crate::text_field::{FieldEvent, TextField, new_field};
use gpui::{
    AnyElement, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled, Subscription,
    Window, div, prelude::FluentBuilder, px,
};
use kitab_core::{RenderRequest, parse_page_ranges};
use kitab_print::{Job, PageSize, Printer, Scale, print_pages};
use std::sync::Arc;

/// A Markdown document laid out for paper while it prints: MuPDF's pages
/// of kitab-md's print HTML (A4), closed and deleted when printing ends.
pub(super) struct MdPrint {
    reader: gpui::EntityId,
    doc: crate::engine::DocHandle,
    sizes: Vec<kitab_core::SizePt>,
    file: std::path::PathBuf,
}

/// What printing draws from: the tab's document, or its paper layout.
struct PrintSrc {
    doc: crate::engine::DocHandle,
    sizes: Vec<kitab_core::SizePt>,
    title: String,
    count: u32,
    /// 0-based.
    current: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum Which {
    All,
    Current,
    Range,
}

pub(super) struct PrintSheet {
    reader: Entity<Reader>,
    which: Which,
    range: Entity<TextField>,
    scale: Scale,
    rotate: bool,
    serial: u64,
    _sub: Subscription,
    /// macOS and Linux: the printers CUPS knows (loading: `None`), the one
    /// chosen, and how many copies. Windows asks in its own dialog.
    printers: Option<Result<Vec<kitab_print::PrinterInfo>, String>>,
    printer: Option<String>,
    copies: u32,
}

impl Workspace {
    /// Ctrl+P: the print sheet for the active tab.
    pub(super) fn open_print(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reader) = self.active_reader().cloned() else {
            return;
        };
        if reader.read(cx).is_markdown() {
            self.menu = None;
            self.menu_closing = None;
            self.prepare_md_print(reader, window, cx);
            return;
        }
        if let Err(why) = reader.read(cx).print_source() {
            self.toast(why, true, None, cx);
            return;
        }
        self.menu = None;
        self.menu_closing = None;
        self.start_print(reader, window, cx);
    }

    /// Markdown: written as HTML for paper and laid out on A4 by MuPDF, off
    /// the UI thread; then printed like any document.
    fn prepare_md_print(
        &mut self,
        reader: Entity<Reader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_md_print();
        let Some(doc) = reader.read(cx).markdown_doc() else {
            return;
        };
        // MuPDF lays out with its own fonts, picked by generic family.
        let body = match Style::get(cx).md_face {
            crate::settings::MdFace::System => "sans-serif",
            crate::settings::MdFace::Serif => "serif",
            crate::settings::MdFace::Mono => "monospace",
        }
        .to_string();
        let mono = "monospace".to_string();
        let engine: Arc<dyn DocEngine> = self.engine.clone();
        let id = reader.entity_id();
        cx.spawn_in(window, async move |this, cx| {
            let laid = cx
                .background_spawn(async move {
                    let html = kitab_md::print::to_print_html(
                        &doc,
                        &kitab_md::print::PrintStyle {
                            body_font: &body,
                            mono_font: &mono,
                        },
                    );
                    // kitab-print-*.html: the one HTML the engine opens.
                    let file = kitab_print::temp_pdf().with_extension("html");
                    std::fs::write(&file, html).map_err(|e| e.to_string())?;
                    let (handle, _) = match engine.open(&file, None) {
                        Ok(o) => o,
                        Err(e) => {
                            let _ = std::fs::remove_file(&file);
                            return Err(format!("{e:?}"));
                        }
                    };
                    let a4 = kitab_core::BookLayout {
                        width: 595.0,
                        height: 842.0,
                        em: 10.5,
                        ..kitab_core::BookLayout::DEFAULT
                    };
                    match engine.relayout(handle, a4) {
                        Ok(info) => Ok(MdPrint {
                            reader: id,
                            doc: handle,
                            sizes: info.page_sizes.clone(),
                            file,
                        }),
                        Err(e) => {
                            engine.close(handle);
                            let _ = std::fs::remove_file(&file);
                            Err(e)
                        }
                    }
                })
                .await;
            let _ = this.update_in(cx, |ws, window, cx| match laid {
                Ok(m) => {
                    ws.md_print = Some(m);
                    ws.start_print(reader, window, cx);
                }
                Err(e) => ws.toast(format!("Couldn't print: {e}"), true, None, cx),
            });
        })
        .detach();
    }

    /// Printing is over (sent, cancelled or failed): let go of Markdown's
    /// paper layout.
    fn finish_md_print(&mut self) {
        if let Some(m) = self.md_print.take() {
            self.engine.close(m.doc);
            let _ = std::fs::remove_file(&m.file);
        }
    }

    /// What printing `reader` draws from.
    fn print_src(&self, reader: &Entity<Reader>, cx: &gpui::App) -> Result<PrintSrc, String> {
        let r = reader.read(cx);
        let title = r.title().to_string();
        if let Some(m) = self
            .md_print
            .as_ref()
            .filter(|m| m.reader == reader.entity_id())
        {
            return Ok(PrintSrc {
                doc: m.doc,
                sizes: m.sizes.clone(),
                title,
                count: m.sizes.len() as u32,
                current: 0,
            });
        }
        let (doc, sizes) = r.print_source().map_err(str::to_string)?;
        Ok(PrintSrc {
            doc,
            sizes,
            title,
            count: r.page_count() as u32,
            current: r.current_page() as u32,
        })
    }

    /// The print dialog (or sheet) for `reader`, its source ready.
    fn start_print(&mut self, reader: Entity<Reader>, window: &mut Window, cx: &mut Context<Self>) {
        // Windows: its own print dialog straight away, with everything in
        // it (printer, copies, which pages); Kitab's sheet is for CUPS.
        if kitab_print::SYSTEM_DIALOG {
            self.print_with_system_dialog(reader, window, cx);
            return;
        }
        // macOS and Linux: the system's dialog, given the pages as a PDF.
        if kitab_print::native::AVAILABLE && !self.print_no_portal {
            self.print_with_native_dialog(reader, window, cx);
            return;
        }
        self.open_print_sheet(reader, window, cx);
    }

    /// Kitab's own sheet (Linux without a print portal): pages, size,
    /// printer and copies, sent through CUPS.
    fn open_print_sheet(
        &mut self,
        reader: Entity<Reader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let page = self
            .print_src(&reader, cx)
            .map_or(1, |s| s.current as usize + 1);
        let range = new_field(&format!("{page}"), false, "1-3, 7, 10-", window, cx);
        let sub = cx.subscribe_in(
            &range,
            window,
            |ws, _, e: &FieldEvent, window, cx| match e {
                FieldEvent::Submit => ws.run_print(window, cx),
                FieldEvent::Cancel => ws.close_print(window, cx),
                FieldEvent::Changed => cx.notify(),
            },
        );
        self.print_serial += 1;
        self.print = Some(PrintSheet {
            reader,
            which: Which::All,
            range,
            scale: Scale::Fit,
            rotate: true,
            serial: self.print_serial,
            _sub: sub,
            printers: None,
            printer: None,
            copies: 1,
        });
        if !kitab_print::SYSTEM_DIALOG {
            // The printer list comes from CUPS, off the UI thread.
            cx.spawn(async move |this, cx| {
                let list = cx.background_spawn(async { kitab_print::printers() }).await;
                let _ = this.update(cx, |ws, cx| {
                    if let Some(s) = ws.print.as_mut() {
                        s.printer = list.as_ref().ok().and_then(|l| {
                            l.iter()
                                .find(|p| p.is_default)
                                .or(l.first())
                                .map(|p| p.name.clone())
                        });
                        s.printers = Some(list);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    pub(super) fn close_print(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.print.is_none() {
            return;
        }
        self.exit_modal("print", window, cx, |ws, window, cx| {
            if ws.print.take().is_some() {
                ws.finish_md_print();
                ws.refocus(window, cx);
                cx.notify();
            }
        });
    }

    /// The pages the sheet asks for (0-based), or why it can't.
    fn print_pages(&self, cx: &Context<Self>) -> Result<Vec<u32>, String> {
        let Some(s) = self.print.as_ref() else {
            return Err(String::new());
        };
        let src = self.print_src(&s.reader, cx)?;
        let count = src.count;
        match s.which {
            Which::All => Ok((0..count).collect()),
            Which::Current => Ok(vec![src.current]),
            Which::Range => parse_page_ranges(&s.range.read(cx).text(), count),
        }
    }

    /// Print…: the system dialog, then the pages, all off the UI thread.
    fn run_print(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(pages) = self.print_pages(cx) else {
            return;
        };
        let Some(sheet) = self.print.take() else {
            return;
        };
        let (scale, rotate) = (sheet.scale, sheet.rotate);
        if !kitab_print::SYSTEM_DIALOG {
            let Some(printer) = sheet.printer.clone() else {
                self.print = Some(sheet);
                return;
            };
            let copies = sheet.copies;
            let Ok(PrintSrc { doc, title, .. }) = self.print_src(&sheet.reader, cx) else {
                return;
            };
            self.refocus(window, cx);
            self.toast("Sending to the printer…", false, None, cx);
            let engine: Arc<dyn DocEngine> = self.engine.clone();
            let total = pages.len();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_spawn(async move {
                        // The pages as a PDF, handed to CUPS, then removed.
                        let tmp = kitab_print::temp_pdf();
                        let sent = engine.write_pdf(doc, &pages, &tmp).and_then(|_| {
                            kitab_print::print_pdf(&printer, &tmp, &title, copies, scale, rotate)
                        });
                        let _ = std::fs::remove_file(&tmp);
                        sent
                    })
                    .await;
                let _ = this.update(cx, |ws, _| ws.finish_md_print());
                let _ = this.update(cx, |ws, cx| match result {
                    Ok(()) => {
                        let what = if total == 1 { "page" } else { "pages" };
                        ws.toast(
                            format!("Sent {total} {what} to the printer."),
                            false,
                            None,
                            cx,
                        )
                    }
                    Err(e) => ws.toast(format!("Couldn't print: {e}"), true, None, cx),
                });
            })
            .detach();
            cx.notify();
            return;
        }
        let Ok(PrintSrc {
            doc, sizes, title, ..
        }) = self.print_src(&sheet.reader, cx)
        else {
            return;
        };
        self.refocus(window, cx);
        let owner = owner_of(window);
        let engine: Arc<dyn DocEngine> = self.engine.clone();
        let total = pages.len();
        cx.spawn(async move |this, cx| {
            let (tx, rx) = smol::channel::unbounded::<Result<usize, String>>();
            let job = cx.background_spawn(async move {
                let mut printer = match Printer::choose(owner, 1) {
                    Ok(Some(p)) => p,
                    Ok(None) => return Ok(false),
                    Err(e) => return Err(e),
                };
                let list: Vec<(u32, PageSize)> = pages
                    .iter()
                    .map(|&p| {
                        let s = sizes
                            .get(p as usize)
                            .copied()
                            .unwrap_or(kitab_core::SizePt { w: 612.0, h: 792.0 });
                        (p, PageSize { w: s.w, h: s.h })
                    })
                    .collect();
                let job = Job {
                    title: &title,
                    to_file: None,
                    pages: &list,
                    scale,
                    auto_rotate: rotate,
                };
                print_pages(
                    &mut printer,
                    &job,
                    |page, scale, rotation| {
                        let req = RenderRequest {
                            page,
                            scale,
                            rotation,
                            tile: None,
                            // Never stale: a view change mustn't drop a page being printed.
                            generation: u64::MAX,
                        };
                        match engine.render(doc, req)? {
                            Some(b) => Ok((b.bgra, b.width, b.height)),
                            None => Err("it didn't draw".to_string()),
                        }
                    },
                    |done| {
                        let _ = tx.send_blocking(Ok(done));
                    },
                )
                .map(|_| true)
            });
            // Progress as pages go out, in its own task.
            let watcher = this.clone();
            cx.spawn(async move |cx| {
                while let Ok(Ok(done)) = rx.recv().await {
                    let _ = watcher.update(cx, |ws, cx| {
                        ws.toast(format!("Printing page {done} of {total}…"), false, None, cx)
                    });
                }
            })
            .detach();
            let result = job.await;
            let _ = this.update(cx, |ws, _| ws.finish_md_print());
            let _ = this.update(cx, |ws, cx| match result {
                Ok(true) => {
                    let what = if total == 1 { "page" } else { "pages" };
                    ws.toast(
                        format!("Sent {total} {what} to the printer."),
                        false,
                        None,
                        cx,
                    )
                }
                Ok(false) => {}
                Err(e) => ws.toast(format!("Couldn't print: {e}"), true, None, cx),
            });
        })
        .detach();
        cx.notify();
    }

    /// macOS and Linux: every page written to a PDF in the background, then
    /// the system's print dialog, which picks the printer, copies, pages and
    /// paper and prints. Linux without a print portal gets Kitab's sheet.
    fn print_with_native_dialog(
        &mut self,
        reader: Entity<Reader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(PrintSrc {
            doc, title, count, ..
        }) = self.print_src(&reader, cx)
        else {
            return;
        };
        let engine: Arc<dyn DocEngine> = self.engine.clone();
        if count > 40 {
            self.toast("Getting the pages ready to print…", false, None, cx);
        }
        cx.spawn_in(window, async move |this, cx| {
            let tmp = kitab_print::temp_pdf();
            let out = tmp.clone();
            let written = cx
                .background_spawn(async move {
                    let pages: Vec<u32> = (0..count).collect();
                    engine.write_pdf(doc, &pages, &out)
                })
                .await;
            // The PDF holds the pages now: Markdown's layout can go.
            let _ = this.update(cx, |ws, _| ws.finish_md_print());
            if let Err(e) = written {
                let _ = std::fs::remove_file(&tmp);
                let _ = this.update(cx, |ws, cx| {
                    ws.toast(format!("Couldn't print: {e}"), true, None, cx)
                });
                return;
            }
            use kitab_print::native::{Failure, Outcome, print_with_dialog};
            // macOS: the panel is a sheet on this window, from the main
            // thread; it reads the file after this returns. Linux: the
            // portal's dialog blocks, so it waits off the UI thread.
            let result = if cfg!(target_os = "macos") {
                this.update_in(cx, |_, window, _| {
                    print_with_dialog(owner_of(window), &tmp, &title)
                })
                .unwrap_or(Err(Failure::Unavailable))
            } else {
                let file = tmp.clone();
                let r = cx
                    .background_spawn(async move { print_with_dialog(0, &file, &title) })
                    .await;
                let _ = std::fs::remove_file(&tmp);
                r
            };
            let _ = this.update_in(cx, |ws, window, cx| match result {
                Ok(Outcome::Printed) if !cfg!(target_os = "macos") => {
                    ws.toast("Sent to the printer.", false, None, cx)
                }
                Ok(_) => {}
                Err(Failure::Unavailable) => {
                    ws.print_no_portal = true;
                    ws.open_print_sheet(reader, window, cx);
                }
                Err(Failure::Failed(e)) => ws.toast(format!("Couldn't print: {e}"), true, None, cx),
            });
        })
        .detach();
    }

    /// The system print dialog picks the printer, copies and pages; then
    /// the pages go out in the background, shrunk to fit the paper where
    /// they're bigger and turned to match it.
    fn print_with_system_dialog(
        &mut self,
        reader: Entity<Reader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(PrintSrc {
            doc,
            sizes,
            title,
            count,
            current,
        }) = self.print_src(&reader, cx)
        else {
            return;
        };
        let owner = owner_of(window);
        kitab_os::trace::mark(&format!("print: system dialog, owner {owner:#x}"));
        let engine: Arc<dyn DocEngine> = self.engine.clone();
        cx.spawn(async move |this, cx| {
            // The dialog runs here, on the UI thread, as GPUI runs Windows'
            // file dialogs: a task outside any update, so its modal loop can
            // still dispatch to the window. On a pooled thread PrintDlgEx
            // crashed in COM; on a thread of its own it never appeared.
            kitab_os::trace::mark("print: dialog opening");
            let chosen = Printer::choose_pages(owner, count, current + 1);
            kitab_os::trace::mark("print: dialog closed");
            let (mut printer, chosen) = match chosen {
                Ok(Some(p)) => p,
                Ok(None) => {
                    let _ = this.update(cx, |ws, _| ws.finish_md_print());
                    return;
                }
                Err(e) => {
                    let _ = this.update(cx, |ws, cx| {
                        ws.finish_md_print();
                        ws.toast(format!("Couldn't print: {e}"), true, None, cx)
                    });
                    return;
                }
            };
            let (tx, rx) = smol::channel::unbounded::<(usize, usize)>();
            let job = cx.background_spawn(async move {
                let pages = chosen.pages(count, current);
                if pages.is_empty() {
                    return Err("those pages aren't in this document".to_string());
                }
                let total = pages.len();
                let list: Vec<(u32, PageSize)> = pages
                    .iter()
                    .map(|&p| {
                        let s = sizes
                            .get(p as usize)
                            .copied()
                            .unwrap_or(kitab_core::SizePt { w: 612.0, h: 792.0 });
                        (p, PageSize { w: s.w, h: s.h })
                    })
                    .collect();
                let job = Job {
                    title: &title,
                    to_file: None,
                    pages: &list,
                    scale: Scale::Shrink,
                    auto_rotate: true,
                };
                print_pages(
                    &mut printer,
                    &job,
                    |page, scale, rotation| {
                        let req = RenderRequest {
                            page,
                            scale,
                            rotation,
                            tile: None,
                            // Never stale: a view change mustn't drop a page being printed.
                            generation: u64::MAX,
                        };
                        match engine.render(doc, req)? {
                            Some(b) => Ok((b.bgra, b.width, b.height)),
                            None => Err("it didn't draw".to_string()),
                        }
                    },
                    |done| {
                        let _ = tx.send_blocking((done, total));
                    },
                )
                .map(Some)
            });
            let watcher = this.clone();
            cx.spawn(async move |cx| {
                while let Ok((done, total)) = rx.recv().await {
                    let _ = watcher.update(cx, |ws, cx| {
                        ws.toast(format!("Printing page {done} of {total}…"), false, None, cx)
                    });
                }
            })
            .detach();
            let result = job.await;
            let _ = this.update(cx, |ws, _| ws.finish_md_print());
            let _ = this.update(cx, |ws, cx| match result {
                Ok(Some(n)) => {
                    let what = if n == 1 { "page" } else { "pages" };
                    ws.toast(format!("Sent {n} {what} to the printer."), false, None, cx)
                }
                Ok(None) => {}
                Err(e) => ws.toast(format!("Couldn't print: {e}"), true, None, cx),
            });
        })
        .detach();
    }

    pub(super) fn print_view(&mut self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.print.as_ref()?;
        let count = self
            .print_src(&s.reader, cx)
            .map_or(0, |s| s.count as usize);
        let r = s.reader.read(cx);
        let title = r.title();
        let pages = self.print_pages(cx);
        let ws = cx.weak_entity();
        let set = |f: fn(&mut PrintSheet)| -> Click {
            let ws = ws.clone();
            Box::new(move |_, cx| {
                let _ = ws.update(cx, |ws, cx| {
                    if let Some(s) = ws.print.as_mut() {
                        f(s);
                    }
                    cx.notify();
                });
            })
        };
        let which = segmented(
            vec![
                (
                    SharedString::from("All"),
                    s.which == Which::All,
                    set(|s| s.which = Which::All),
                ),
                (
                    SharedString::from("This page"),
                    s.which == Which::Current,
                    set(|s| s.which = Which::Current),
                ),
                (
                    SharedString::from("Pages…"),
                    s.which == Which::Range,
                    set(|s| s.which = Which::Range),
                ),
            ],
            st,
        );
        let size = segmented(
            vec![
                (
                    SharedString::from("Fit to paper"),
                    s.scale == Scale::Fit,
                    set(|s| s.scale = Scale::Fit),
                ),
                (
                    SharedString::from("Actual size"),
                    s.scale == Scale::Actual,
                    set(|s| s.scale = Scale::Actual),
                ),
                (
                    SharedString::from("Shrink to fit"),
                    s.scale == Scale::Shrink,
                    set(|s| s.scale = Scale::Shrink),
                ),
            ],
            st,
        );
        let turn = segmented(
            vec![
                (
                    SharedString::from("Turn to match the paper"),
                    s.rotate,
                    set(|s| s.rotate = true),
                ),
                (
                    SharedString::from("As they are"),
                    !s.rotate,
                    set(|s| s.rotate = false),
                ),
            ],
            st,
        );
        let label = |t: &'static str| {
            div()
                .w(px(70.0))
                .flex_none()
                .text_size(px(12.5))
                .text_color(st.c(|c| c.muted))
                .child(t)
        };
        let row = |name: &'static str, control: AnyElement| {
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(label(name))
                .child(control)
        };
        let note = match &pages {
            Ok(p) if p.len() == 1 => "1 page".to_string(),
            Ok(p) => format!("{} pages", p.len()),
            Err(e) => e.clone(),
        };
        let bad = pages.is_err();
        let panel = div()
            .id("print-sheet")
            .w(px(480.0))
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
                    .child("Print"),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.muted))
                    .child(format!(
                        "{title} ({count} pages).{}",
                        if kitab_print::SYSTEM_DIALOG {
                            " The printer and copies are chosen next."
                        } else {
                            ""
                        }
                    )),
            )
            .child(row("Pages", which))
            .when(s.which == Which::Range, |d| {
                d.child(
                    div()
                        .ml(px(82.0))
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(st.px(|m| m.control_radius))
                        .bg(st.c(|c| c.hover))
                        .border_1()
                        .border_color(if bad {
                            st.c(|c| c.bad)
                        } else {
                            st.c(|c| c.line)
                        })
                        .child(s.range.clone()),
                )
            })
            .when(!kitab_print::SYSTEM_DIALOG, |d| {
                d.child(self.printer_rows(st, cx))
            })
            .child(row("Size", size))
            .child(row("Pages turn", turn))
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
                        dialog_button(st, "print-cancel", "Cancel", false)
                            .on_click(cx.listener(|ws, _, window, cx| ws.close_print(window, cx))),
                    )
                    .child(
                        dialog_button(st, "print-go", "Print…", true)
                            .when(bad, |b| b.opacity(0.45))
                            .on_click(cx.listener(|ws, _, window, cx| ws.run_print(window, cx))),
                    ),
            );
        let serial = s.serial;
        Some(self.modal_layer(
            "print",
            serial,
            panel.into_any_element(),
            st,
            Self::close_print,
            cx,
        ))
    }
}

impl Workspace {
    /// macOS and Linux: the printer list and copies, in the sheet.
    fn printer_rows(&self, st: &Style, cx: &mut Context<Self>) -> AnyElement {
        let Some(s) = self.print.as_ref() else {
            return div().into_any_element();
        };
        let label = |t: &'static str| {
            div()
                .w(px(70.0))
                .flex_none()
                .text_size(px(12.5))
                .text_color(st.c(|c| c.muted))
                .child(t)
        };
        let list: AnyElement = match &s.printers {
            None => div()
                .text_size(px(12.5))
                .text_color(st.c(|c| c.faint))
                .child("Looking for printers…")
                .into_any_element(),
            Some(Err(e)) => div()
                .text_size(px(12.5))
                .text_color(st.c(|c| c.bad))
                .child(e.clone())
                .into_any_element(),
            Some(Ok(list)) => {
                let mut col = div().flex().flex_col().gap(px(2.0)).flex_1().min_w_0();
                for (i, p) in list.iter().enumerate() {
                    let on = s.printer.as_deref() == Some(p.name.as_str());
                    let name = p.name.clone();
                    col = col.child(
                        div()
                            .id(("printer", i))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .h(px(28.0))
                            .px(px(10.0))
                            .rounded(px(7.0))
                            .text_size(px(12.5))
                            .cursor_pointer()
                            .when(on, |d| d.bg(st.c(|c| c.hover)).text_color(st.c(|c| c.text)))
                            .when(!on, |d| d.text_color(st.c(|c| c.muted)))
                            .hover(|d| d.bg(st.hover_bg()))
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                if let Some(s) = ws.print.as_mut() {
                                    s.printer = Some(name.clone());
                                }
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(p.name.replace('_', " ")),
                            )
                            .when(p.is_default, |d| {
                                d.child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(st.c(|c| c.faint))
                                        .child("Default"),
                                )
                            }),
                    );
                }
                col.into_any_element()
            }
        };
        let step = |id: &'static str, glyph: &'static str, delta: i32| {
            div()
                .id(id)
                .size(px(26.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .bg(st.c(|c| c.raised))
                .shadow(st.elev())
                .cursor_pointer()
                .hover(|d| d.bg(st.raised_hover()))
                .on_click(cx.listener(move |ws, _, _, cx| {
                    if let Some(s) = ws.print.as_mut() {
                        s.copies = (s.copies as i32 + delta).clamp(1, 99) as u32;
                    }
                    cx.notify();
                }))
                .child(glyph)
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.0))
                    .child(label("Printer"))
                    .child(div().max_h(px(150.0)).flex_1().min_w_0().child(list)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(label("Copies"))
                    .child(step("copies-less", "−", -1))
                    .child(
                        div()
                            .w(px(28.0))
                            .flex()
                            .justify_center()
                            .text_size(px(13.0))
                            .text_color(st.c(|c| c.text))
                            .child(s.copies.to_string()),
                    )
                    .child(step("copies-more", "+", 1)),
            )
            .into_any_element()
    }
}

/// Kitab's window handle, so the system dialog sits in front of it.
fn owner_of(window: &Window) -> isize {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(RawWindowHandle::Win32(h)) =
            HasWindowHandle::window_handle(window).map(|h| h.as_raw())
        {
            return h.hwnd.get();
        }
    }
    // macOS: the window's content view (the print panel finds its window).
    #[cfg(target_os = "macos")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(RawWindowHandle::AppKit(h)) =
            HasWindowHandle::window_handle(window).map(|h| h.as_raw())
        {
            return h.ns_view.as_ptr() as isize;
        }
    }
    let _ = window;
    0
}
