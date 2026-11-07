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
        if let Err(why) = reader.read(cx).print_source() {
            self.toast(why, true, None, cx);
            return;
        }
        self.menu = None;
        self.menu_closing = None;
        let page = reader.read(cx).current_page() + 1;
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
        if self.print.take().is_some() {
            self.refocus(window, cx);
            cx.notify();
        }
    }

    /// The pages the sheet asks for (0-based), or why it can't.
    fn print_pages(&self, cx: &Context<Self>) -> Result<Vec<u32>, String> {
        let Some(s) = self.print.as_ref() else {
            return Err(String::new());
        };
        let r = s.reader.read(cx);
        let count = r.page_count() as u32;
        match s.which {
            Which::All => Ok((0..count).collect()),
            Which::Current => Ok(vec![r.current_page() as u32]),
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
            let r = sheet.reader.read(cx);
            let title = r.title().to_string();
            let Ok((doc, _)) = r.print_source() else {
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
                        let tmp = std::env::temp_dir().join(format!(
                            "kitab-print-{}-{}.pdf",
                            std::process::id(),
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or(0)
                        ));
                        let sent = engine.write_pdf(doc, &pages, &tmp).and_then(|_| {
                            kitab_print::print_pdf(&printer, &tmp, &title, copies, scale, rotate)
                        });
                        let _ = std::fs::remove_file(&tmp);
                        sent
                    })
                    .await;
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
        let r = sheet.reader.read(cx);
        let title = r.title().to_string();
        let Ok((doc, sizes)) = r.print_source() else {
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
                            generation: 0,
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

    pub(super) fn print_view(&mut self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.print.as_ref()?;
        let r = s.reader.read(cx);
        let (title, count) = (r.title(), r.page_count());
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
                .hover(|d| d.bg(st.hover_bg()))
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
    let _ = window;
    0
}
