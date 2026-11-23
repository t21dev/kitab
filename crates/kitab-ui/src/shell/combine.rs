// SPDX-License-Identifier: AGPL-3.0-or-later

//! Combine into one PDF (DESIGN.md § Combine sheet): before PDFs are merged,
//! a list sets their order. Drag a row, or use its arrows, Alt+↑/↓, Del;
//! Add PDFs… and Sort by name. Opened by Merge with other PDFs… (the open
//! document first) and by the file manager's right-click Combine.
//!
//! The same list sets the order of pictures for Make a PDF from pictures:
//! each picture a page.

use super::Workspace;
use super::tools_ui::{dialog_button, is_picture, natural_key};
use crate::icons::icon;
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, AppContext, Context, CursorStyle, ElementId, FontWeight, Hsla, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, ObjectFit, ParentElement, PathPromptOptions, Render,
    RenderImage, SharedString, StatefulInteractiveElement, Styled, StyledImage, Window, div, img,
    prelude::FluentBuilder, px, svg,
};
use image::Frame;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A PDF in the list. Its page count and first page arrive after it's added.
pub(super) struct CombineItem {
    pub path: PathBuf,
    /// The open document's password, when it came from a tab.
    pub unlock: Option<String>,
    /// `None` while being read; `Err` says why it can't be combined.
    pub pages: Option<Result<u32, &'static str>>,
    pub cover: Option<Arc<RenderImage>>,
}

pub(super) struct CombineDialog {
    pub items: Vec<CombineItem>,
    /// The row the keys act on.
    pub sel: usize,
    pub serial: u64,
    /// Pictures to make a PDF of, not PDFs to combine.
    pub pictures: bool,
}

/// A row being dragged: its index in the dialog with this serial.
#[derive(Clone, Copy)]
struct DraggedRow {
    from: usize,
    serial: u64,
}

/// What follows the pointer: the file's name on a raised chip.
struct RowGhost {
    name: SharedString,
    fg: Hsla,
    bg: Hsla,
    line: Hsla,
}

impl Render for RowGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(30.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .rounded(px(8.0))
            .bg(self.bg)
            .border_1()
            .border_color(self.line)
            .shadow_md()
            .text_size(px(12.5))
            .text_color(self.fg)
            .cursor(CursorStyle::ClosedHand)
            .child(self.name.clone())
    }
}

const ROW_H: f32 = 52.0;
/// Rows shown before the list scrolls.
const ROWS_SHOWN: f32 = 6.0;
const THUMB_PX: u32 = 76;

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Move the item at `from` into the place of the one at `to`: moving down
/// it lands after that one, moving up before it.
fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from >= items.len() {
        return;
    }
    let it = items.remove(from);
    items.insert(to.min(items.len()), it);
}

impl Workspace {
    /// Show the Combine sheet with `inputs` in this order.
    pub(super) fn open_combine(
        &mut self,
        inputs: Vec<(PathBuf, Option<String>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_order_sheet(inputs, false, window, cx);
    }

    /// Show the sheet that sets the order of `pictures` before they become
    /// a PDF, one a page.
    pub(super) fn open_pictures_order(
        &mut self,
        pictures: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let inputs = pictures.into_iter().map(|p| (p, None)).collect();
        self.open_order_sheet(inputs, true, window, cx);
    }

    fn open_order_sheet(
        &mut self,
        inputs: Vec<(PathBuf, Option<String>)>,
        pictures: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        self.menu_closing = None;
        self.menu_serial += 1;
        let serial = self.menu_serial;
        self.tools.combine = Some(CombineDialog {
            items: Vec::new(),
            sel: 0,
            serial,
            pictures,
        });
        self.add_to_combine(inputs, cx);
        window.focus(&self.tools.dialog_focus);
        cx.notify();
    }

    /// Append PDFs not in the list yet, and read each one's page count and
    /// first page off the UI thread.
    fn add_to_combine(&mut self, inputs: Vec<(PathBuf, Option<String>)>, cx: &mut Context<Self>) {
        let Some(d) = self.tools.combine.as_mut() else {
            return;
        };
        let (serial, pictures) = (d.serial, d.pictures);
        for (path, unlock) in inputs {
            if d.items.iter().any(|i| i.path == path) {
                continue;
            }
            d.items.push(CombineItem {
                path: path.clone(),
                unlock: unlock.clone(),
                pages: None,
                cover: None,
            });
            let (engine, library) = (self.engine.clone(), self.library.clone());
            cx.spawn(async move |this, cx| {
                let p = path.clone();
                let (pages, cover) = cx
                    .background_spawn(async move {
                        use kitab_core::OpenError;
                        // A picture is one page; whether it can be read, making
                        // the PDF says (its preview is only a nicety).
                        let pages = if pictures {
                            Ok(1)
                        } else {
                            match engine.open(&p, unlock.as_deref()) {
                                Ok((h, info)) => {
                                    engine.close(h);
                                    Ok(info.page_count)
                                }
                                Err(OpenError::NeedsPassword | OpenError::WrongPassword) => {
                                    Err("Has a password: open it in Kitab first")
                                }
                                Err(OpenError::NotFound) => Err("Not found"),
                                Err(_) => Err("Kitab can't read this file"),
                            }
                        };
                        let cover = pages.is_ok().then(|| library.cover(&p, THUMB_PX)).flatten();
                        let cover = cover.and_then(|png| {
                            let mut rgba = image::load_from_memory(&png).ok()?.to_rgba8();
                            // GPUI takes BGRA.
                            for px in rgba.pixels_mut() {
                                px.0.swap(0, 2);
                            }
                            Some(Arc::new(RenderImage::new([Frame::new(rgba)])))
                        });
                        (pages, cover)
                    })
                    .await;
                let _ = this.update(cx, |ws, cx| {
                    let Some(d) = ws.tools.combine.as_mut().filter(|d| d.serial == serial) else {
                        return;
                    };
                    if let Some(it) = d.items.iter_mut().find(|i| i.path == path) {
                        it.pages = Some(pages);
                        it.cover = cover;
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub(super) fn close_combine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tools.combine.is_none() {
            return;
        }
        self.exit_modal("combine", window, cx, |ws, window, cx| {
            if ws.tools.combine.take().is_some() {
                ws.refocus(window, cx);
                cx.notify();
            }
        });
    }

    fn combine_move(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if let Some(d) = self.tools.combine.as_mut()
            && from < d.items.len()
            && to < d.items.len()
            && from != to
        {
            move_item(&mut d.items, from, to);
            d.sel = to;
            cx.notify();
        }
    }

    fn combine_remove(&mut self, at: usize, cx: &mut Context<Self>) {
        if let Some(d) = self.tools.combine.as_mut()
            && at < d.items.len()
        {
            d.items.remove(at);
            d.sel = d.sel.min(d.items.len().saturating_sub(1));
            cx.notify();
        }
    }

    fn combine_sort(&mut self, cx: &mut Context<Self>) {
        if let Some(d) = self.tools.combine.as_mut() {
            d.items.sort_by_cached_key(|i| natural_key(&i.path));
            d.sel = 0;
            cx.notify();
        }
    }

    fn combine_add(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add to the list".into()),
        });
        let pictures = self.tools.combine.as_ref().is_some_and(|d| d.pictures);
        cx.spawn(async move |this, cx| {
            let picked = rx.await;
            if matches!(picked, Ok(Err(_)) | Err(_)) {
                let _ = this.update(cx, |ws, cx| ws.picker_failed(cx));
            }
            let Ok(Ok(Some(paths))) = picked else { return };
            let wanted = |p: &PathBuf| {
                if pictures {
                    is_picture(p)
                } else {
                    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
                }
            };
            let files: Vec<(PathBuf, Option<String>)> = paths
                .into_iter()
                .filter(wanted)
                .map(|p| (p, None))
                .collect();
            let _ = this.update(cx, |ws, cx| {
                ws.add_to_combine(files, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// The list can be combined: two or more PDFs (one or more pictures),
    /// none that can't be read.
    fn combine_ready(&self) -> bool {
        self.tools.combine.as_ref().is_some_and(|d| {
            let least = if d.pictures { 1 } else { 2 };
            d.items.len() >= least && d.items.iter().all(|i| !matches!(i.pages, Some(Err(_))))
        })
    }

    fn confirm_combine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.combine_ready() {
            return;
        }
        let Some(d) = self.tools.combine.as_ref() else {
            return;
        };
        let inputs: Vec<(PathBuf, Option<String>)> = d
            .items
            .iter()
            .map(|i| (i.path.clone(), i.unlock.clone()))
            .collect();
        let pictures = d.pictures;
        self.close_combine(window, cx);
        if pictures {
            self.save_pictures_pdf(inputs.into_iter().map(|(p, _)| p).collect(), cx);
        } else {
            self.merge_inputs(inputs, cx);
        }
    }

    fn combine_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.tools.combine.as_ref() else {
            return;
        };
        let (sel, n) = (d.sel, d.items.len());
        let alt = e.keystroke.modifiers.alt;
        match e.keystroke.key.as_str() {
            "enter" => self.confirm_combine(window, cx),
            "up" if alt && sel > 0 => self.combine_move(sel, sel - 1, cx),
            "down" if alt && sel + 1 < n => self.combine_move(sel, sel + 1, cx),
            "up" | "down" => {
                if let Some(d) = self.tools.combine.as_mut() {
                    d.sel = if e.keystroke.key == "up" {
                        sel.saturating_sub(1)
                    } else {
                        (sel + 1).min(n.saturating_sub(1))
                    };
                    cx.notify();
                }
            }
            "delete" | "backspace" => self.combine_remove(sel, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn combine_view(
        &mut self,
        st: &Style,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let d = self.tools.combine.as_ref()?;
        let (serial, pictures) = (d.serial, d.pictures);
        let focused = self.tools.dialog_focus.is_focused(window);
        let n = d.items.len();
        let total: Option<u32> = d.items.iter().map(|i| i.pages.and_then(Result::ok)).sum();
        let blocked = d.items.iter().any(|i| matches!(i.pages, Some(Err(_))));
        let note = if pictures {
            match n {
                0 => "Add at least one picture.".to_string(),
                1 => "1 picture, 1 page".into(),
                n => format!("{n} pictures, {n} pages"),
            }
        } else if n < 2 {
            "Add at least two PDFs.".to_string()
        } else if blocked {
            "Take out the PDF marked in red to combine.".to_string()
        } else {
            match total {
                Some(1) => "1 page in all".into(),
                Some(t) => format!("{t} pages in all"),
                None => format!("{n} PDFs"),
            }
        };
        let ready = self.combine_ready();
        let mut list = div()
            .id("combine-list")
            .flex()
            .flex_col()
            .p(px(4.0))
            .max_h(px(ROW_H * ROWS_SHOWN + 8.0))
            .overflow_y_scroll()
            .rounded(px(12.0))
            .bg(st.c(|c| c.hover))
            .shadow(st.well());
        for (i, it) in d.items.iter().enumerate() {
            list = list.child(self.combine_row(i, it, i == d.sel && focused, n, serial, st, cx));
        }
        if n == 0 {
            list = list.child(
                div()
                    .h(px(ROW_H))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.faint))
                    .child(if pictures {
                        "No pictures yet."
                    } else {
                        "No PDFs yet."
                    }),
            );
        }

        let ghost_btn = |id: &'static str, label: &'static str, ico: Option<&'static str>| {
            crate::kbd::stop(div().id(id), st)
                .h(st.px(|m| m.control_height))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .rounded(st.px(|m| m.control_radius))
                .text_size(st.px(|m| m.ui_text))
                .text_color(st.c(|c| c.muted))
                .cursor_pointer()
                .hover(|b| b.bg(st.hover_bg()).text_color(st.c(|c| c.text)))
                .when_some(ico, |b, ico| {
                    b.child(
                        svg()
                            .path(icon(ico))
                            .size(px(14.0))
                            .text_color(st.c(|c| c.muted)),
                    )
                })
                .child(label)
        };

        let panel = div()
            .id("combine-dialog")
            .track_focus(&self.tools.dialog_focus)
            .on_key_down(cx.listener(Self::combine_key))
            .relative()
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
                    .child(if pictures {
                        "Make a PDF from pictures"
                    } else {
                        "Combine into one PDF"
                    }),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.muted))
                    .child(if pictures {
                        "Drag to set the order: each picture becomes a page, first to last."
                    } else {
                        "Drag to set the order. The originals stay as they are."
                    }),
            )
            .child(list)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        ghost_btn(
                            "combine-add",
                            if pictures {
                                "Add pictures…"
                            } else {
                                "Add PDFs…"
                            },
                            Some("plus"),
                        )
                        .on_click(cx.listener(|ws, _, _, cx| ws.combine_add(cx))),
                    )
                    .child(
                        ghost_btn("combine-sort", "Sort by name", None)
                            .tip("Put them in the order a folder lists them (2 before 10)")
                            .on_click(cx.listener(|ws, _, _, cx| ws.combine_sort(cx))),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .text_size(px(12.0))
                            .text_color(if !ready {
                                st.c(|c| c.bad)
                            } else {
                                st.c(|c| c.faint)
                            })
                            .child(note),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        dialog_button(st, "combine-cancel", "Cancel", false).on_click(
                            cx.listener(|ws, _, window, cx| ws.close_combine(window, cx)),
                        ),
                    )
                    .child(
                        dialog_button(
                            st,
                            "combine-go",
                            if pictures {
                                "Make PDF…"
                            } else {
                                "Combine…"
                            },
                            true,
                        )
                        .when(!ready, |b| b.opacity(0.45))
                        .on_click(cx.listener(|ws, _, window, cx| ws.confirm_combine(window, cx))),
                    ),
            );
        Some(self.modal_layer(
            "combine",
            serial,
            panel.into_any_element(),
            st,
            Self::close_combine,
            cx,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn combine_row(
        &self,
        i: usize,
        it: &CombineItem,
        keyed: bool,
        n: usize,
        serial: u64,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = st.c(|c| c.accent);
        let name = file_name(&it.path);
        let pictures = self.tools.combine.as_ref().is_some_and(|d| d.pictures);
        let (meta, bad) = match it.pages {
            _ if pictures => (format!("Page {}", i + 1), false),
            None => ("Reading…".to_string(), false),
            Some(Ok(1)) => ("1 page".to_string(), false),
            Some(Ok(p)) => (format!("{p} pages"), false),
            Some(Err(e)) => (e.to_string(), true),
        };
        // The first page, at its own shape inside a 28 × 38 slot.
        let thumb = div()
            .w(px(30.0))
            .h(px(38.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(match &it.cover {
                Some(image) => div()
                    .size_full()
                    .rounded(px(2.0))
                    .overflow_hidden()
                    .bg(gpui::white())
                    .shadow(st.elev())
                    .child(
                        img(image.clone())
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    ),
                None => div()
                    .w(px(28.0))
                    .h(px(38.0))
                    .rounded(px(2.0))
                    .border_1()
                    .border_color(st.c(|c| c.line)),
            });
        let arrow = |id: &str, ico: &'static str, tip: &'static str, on: bool| {
            crate::kbd::stop(
                div().id(ElementId::NamedInteger(id.to_string().into(), i as u64)),
                st,
            )
            .size(px(26.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(st.px(|m| m.control_radius))
            .when(on, |b| {
                b.cursor_pointer().hover(|b| b.bg(st.hover_bg())).tip(tip)
            })
            .child(svg().path(icon(ico)).size(px(14.0)).text_color(if on {
                st.c(|c| c.muted)
            } else {
                st.c(|c| c.ring)
            }))
        };
        let grip = div()
            .flex_none()
            .grid()
            .grid_cols(2)
            .gap(px(3.0))
            .children((0..6).map(|_| div().size(px(2.5)).rounded_full().bg(st.c(|c| c.ring))));
        let ghost_name: SharedString = name.clone().into();
        let (gfg, gbg, gline) = (st.c(|c| c.text), st.c(|c| c.raised), st.c(|c| c.line));
        div()
            .id(ElementId::NamedInteger("combine-row".into(), i as u64))
            .h(px(ROW_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .rounded(px(8.0))
            .border_t_2()
            .border_b_2()
            .border_color(gpui::transparent_black())
            .cursor(CursorStyle::OpenHand)
            .when(keyed, |r| r.bg(st.c(|c| c.raised)).shadow(st.elev()))
            .when(!keyed, |r| r.hover(|r| r.bg(st.c(|c| c.raised))))
            .tip(format!("{name}: drag to move"))
            .on_click(cx.listener(move |ws, _, _, cx| {
                if let Some(d) = ws.tools.combine.as_mut() {
                    d.sel = i;
                    cx.notify();
                }
            }))
            .on_drag(DraggedRow { from: i, serial }, move |_, _, _, cx| {
                cx.new(|_| RowGhost {
                    name: ghost_name.clone(),
                    fg: gfg,
                    bg: gbg,
                    line: gline,
                })
            })
            // Where it lands: an accent bar on the edge it goes past.
            .drag_over::<DraggedRow>(move |s, d: &DraggedRow, _, _| {
                if d.serial != serial || d.from == i {
                    s
                } else if d.from < i {
                    s.border_b_2().border_color(accent)
                } else {
                    s.border_t_2().border_color(accent)
                }
            })
            .on_drop(cx.listener(move |ws, d: &DraggedRow, _, cx| {
                if d.serial == serial {
                    ws.combine_move(d.from, i, cx);
                }
            }))
            .child(grip)
            .child(
                div()
                    .w(px(16.0))
                    .flex_none()
                    .text_size(px(11.5))
                    .text_color(st.c(|c| c.faint))
                    .child(format!("{}", i + 1)),
            )
            .child(thumb)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(st.px(|m| m.ui_text))
                            .text_color(st.c(|c| c.text))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(if bad {
                                st.c(|c| c.bad)
                            } else {
                                st.c(|c| c.faint)
                            })
                            .child(meta),
                    ),
            )
            .child(
                arrow("combine-up", "chevron-up", "Move up  Alt+↑", i > 0).on_click(cx.listener(
                    move |ws, _, _, cx| {
                        if i > 0 {
                            ws.combine_move(i, i - 1, cx)
                        }
                    },
                )),
            )
            .child(
                arrow(
                    "combine-down",
                    "chevron-down",
                    "Move down  Alt+↓",
                    i + 1 < n,
                )
                .on_click(cx.listener(move |ws, _, _, cx| ws.combine_move(i, i + 1, cx))),
            )
            .child(
                arrow("combine-remove", "close", "Take out  Del", true)
                    .on_click(cx.listener(move |ws, _, _, cx| ws.combine_remove(i, cx))),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::move_item;

    #[test]
    fn a_dropped_row_takes_the_place_of_the_one_under_it() {
        // Down: 0 dropped on 2 ends up third.
        let mut v = vec!['a', 'b', 'c', 'd'];
        move_item(&mut v, 0, 2);
        assert_eq!(v, ['b', 'c', 'a', 'd']);
        // Up: 3 dropped on 1 ends up second.
        let mut v = vec!['a', 'b', 'c', 'd'];
        move_item(&mut v, 3, 1);
        assert_eq!(v, ['a', 'd', 'b', 'c']);
        // Past the end or out of range: nothing breaks.
        let mut v = vec!['a', 'b'];
        move_item(&mut v, 0, 9);
        assert_eq!(v, ['b', 'a']);
        move_item(&mut v, 5, 0);
        assert_eq!(v, ['b', 'a']);
    }
}
