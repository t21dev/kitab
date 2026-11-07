// SPDX-License-Identifier: AGPL-3.0-or-later

//! PDF tools in the shell: save one page, extract pages, merge PDFs, split
//! view, and the toast that reports what happened (PLAN.md Phase 9, pulled
//! forward). Writing files happens off the UI thread; the open document is never
//! changed.

use super::Workspace;
use crate::actions::*;
use crate::icons::icon;
use crate::menu::MenuItem;
use crate::motion;
use crate::reader::Reader;
use crate::style::Style;
use gpui::{
    AnyElement, App, AppContext, Context, ElementId, Entity, FocusHandle, FontWeight,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, PathPromptOptions,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px, svg,
};
use kitab_core::parse_page_ranges;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a toast stays.
const TOAST_FOR: Duration = Duration::from_millis(4200);

#[derive(Clone)]
pub(super) struct Toast {
    pub text: SharedString,
    pub bad: bool,
    /// "Open" for a new PDF (opens a tab), "Show" for other files.
    pub open: Option<PathBuf>,
    pub serial: u64,
}

/// "Save pages as a new PDF".
pub(super) struct ExtractDialog {
    pub reader: Entity<Reader>,
    pub text: String,
    pub serial: u64,
}

pub(super) struct ToolsState {
    pub toast: Option<Toast>,
    pub toast_closing: Option<Toast>,
    pub toast_serial: u64,
    pub extract: Option<ExtractDialog>,
    pub save: Option<super::save_page::SavePageDialog>,
    pub dialog_focus: FocusHandle,
    /// The tab shown beside the active one.
    pub split: Option<Entity<Reader>>,
}

impl ToolsState {
    pub fn new(cx: &mut App) -> Self {
        Self {
            toast: None,
            toast_closing: None,
            toast_serial: 0,
            extract: None,
            save: None,
            dialog_focus: cx.focus_handle(),
            split: None,
        }
    }
}

pub(super) fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "document".into())
}

fn dir_of(p: &Path) -> PathBuf {
    p.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// Cancel (secondary) or the dialog's action (primary), DESIGN.md § Buttons.
pub(super) fn dialog_button(
    st: &Style,
    id: &'static str,
    label: &'static str,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .h(st.px(|m| m.control_height))
        .px(px(16.0))
        .flex()
        .items_center()
        .rounded(st.px(|m| m.control_radius))
        .text_size(st.px(|m| m.ui_text))
        .cursor_pointer()
        .when(primary, |b| {
            b.bg(st.c(|c| c.text))
                .text_color(st.c(|c| c.bg))
                .shadow(st.elev())
        })
        .when(!primary, |b| {
            b.bg(st.c(|c| c.raised))
                .text_color(st.c(|c| c.text))
                .shadow(st.elev())
                .hover(|b| b.bg(st.c(|c| c.hover)))
        })
        .child(label)
}

impl Workspace {
    // ── toast ──

    pub(super) fn toast(
        &mut self,
        text: impl Into<SharedString>,
        bad: bool,
        open: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.tools.toast_serial += 1;
        let serial = self.tools.toast_serial;
        self.tools.toast_closing = None;
        self.tools.toast = Some(Toast {
            text: text.into(),
            bad,
            open,
            serial,
        });
        // Errors stay until closed (DESIGN.md › Toasts); the rest go by.
        if !bad {
            self.after(TOAST_FOR, cx, move |ws, cx| ws.dismiss_toast(serial, cx));
        }
        cx.notify();
    }

    fn dismiss_toast(&mut self, serial: u64, cx: &mut Context<Self>) {
        if self
            .tools
            .toast
            .as_ref()
            .is_some_and(|t| t.serial == serial)
        {
            let t = self.tools.toast.take();
            if Style::get(cx).animations {
                self.tools.toast_closing = t;
                self.after(motion::EXIT_BASE, cx, move |ws, _| {
                    if ws
                        .tools
                        .toast_closing
                        .as_ref()
                        .is_some_and(|t| t.serial == serial)
                    {
                        ws.tools.toast_closing = None;
                    }
                });
            }
            cx.notify();
        }
    }

    /// Report a finished save: "Open" for PDFs, "Show in folder" for the rest.
    pub(super) fn report(
        &mut self,
        result: Result<PathBuf, String>,
        what: &str,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.toast(format!("{what} {name}"), false, Some(path), cx);
            }
            Err(e) => self.toast(e, true, None, cx),
        }
    }

    pub(super) fn toast_view(&mut self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (t, closing) = match (&self.tools.toast, &self.tools.toast_closing) {
            (Some(t), _) => (t.clone(), false),
            (None, Some(t)) => (t.clone(), true),
            _ => return None,
        };
        let serial = t.serial;
        let action = t.open.clone().map(|path| {
            let is_pdf = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
            div()
                .id("toast-action")
                .px(px(10.0))
                .h(px(26.0))
                .flex()
                .items_center()
                .rounded_full()
                .text_color(st.c(|c| c.accent))
                .hover(|d| d.bg(st.c(|c| c.hover)))
                .cursor_pointer()
                .on_click(cx.listener(move |ws, _, window, cx| {
                    if is_pdf {
                        ws.open_tab(path.clone(), window, cx);
                    } else {
                        let p = path.clone();
                        cx.background_spawn(async move {
                            let _ = kitab_os::reveal_in_folder(&p);
                        })
                        .detach();
                    }
                    ws.dismiss_toast(serial, cx);
                }))
                .child(if is_pdf { "Open" } else { "Show in folder" })
        });
        let pill = div()
            .relative()
            .flex()
            .items_center()
            .gap(px(10.0))
            .min_h(px(40.0))
            .pl(px(14.0))
            .pr(px(6.0))
            .rounded(px(20.0))
            .bg(st.c(|c| c.raised))
            .shadow(st.float())
            .text_size(st.px(|m| m.ui_text))
            .text_color(st.c(|c| c.text))
            .child(
                svg()
                    .path(icon(if t.bad { "alert" } else { "check" }))
                    .size(px(16.0))
                    .text_color(if t.bad {
                        st.c(|c| c.bad)
                    } else {
                        st.c(|c| c.ok)
                    }),
            )
            .child(
                div()
                    .max_w(px(520.0))
                    .py(px(9.0))
                    // Errors wrap, so the whole reason shows; others stay on
                    // one line.
                    .when(!t.bad, |d| {
                        d.overflow_hidden().text_ellipsis().whitespace_nowrap()
                    })
                    .child(t.text.clone()),
            )
            .children(action)
            .when(t.bad, |d| {
                d.child(
                    div()
                        .id("toast-close")
                        .size(px(26.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .cursor_pointer()
                        .hover(|d| d.bg(st.c(|c| c.hover)))
                        .on_click(cx.listener(move |ws, _, _, cx| ws.dismiss_toast(serial, cx)))
                        .child(
                            svg()
                                .path(icon("close"))
                                .size(px(12.0))
                                .text_color(st.c(|c| c.muted)),
                        ),
                )
            })
            .when(t.open.is_none() && !t.bad, |d| d.pr(px(14.0)));
        let pill = if closing {
            motion::exit(
                pill,
                ElementId::NamedInteger("toast-out".into(), serial),
                motion::From::Below(10.0),
                motion::EXIT_BASE,
            )
        } else {
            motion::enter(
                pill,
                ElementId::NamedInteger("toast-in".into(), serial),
                st.animations,
                motion::From::Below(10.0),
                motion::BASE,
                Duration::ZERO,
            )
        };
        Some(
            div()
                .absolute()
                .bottom(px(72.0))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(pill)
                .into_any_element(),
        )
    }

    // ── extract pages ──

    pub(super) fn open_extract(
        &mut self,
        reader: Entity<Reader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let page = reader.read(cx).current_page();
        self.menu = None;
        self.menu_closing = None;
        self.tools.extract = Some(ExtractDialog {
            reader,
            text: format!("{}", page + 1),
            serial: self.menu_serial + 1,
        });
        self.menu_serial += 1;
        window.focus(&self.tools.dialog_focus);
        cx.notify();
    }

    pub(super) fn close_extract(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tools.extract.take().is_some() {
            self.refocus(window, cx);
            cx.notify();
        }
    }

    fn extract_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.tools.extract.as_mut() else {
            return;
        };
        let k = &e.keystroke;
        match k.key.as_str() {
            "enter" => {
                self.confirm_extract(window, cx);
                cx.stop_propagation();
                return;
            }
            "backspace" => {
                d.text.pop();
            }
            _ => {
                if let Some(ch) = k.key_char.as_ref() {
                    let ok = ch
                        .chars()
                        .all(|c| c.is_ascii_digit() || matches!(c, ',' | '-' | ' ' | ';'));
                    if ok && !k.modifiers.control && d.text.len() < 200 {
                        d.text.push_str(ch);
                    }
                }
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn confirm_extract(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.tools.extract.as_ref() else {
            return;
        };
        let r = d.reader.read(cx);
        let count = r.page_count() as u32;
        let pages = match parse_page_ranges(&d.text, count) {
            Ok(p) => p,
            Err(_) => return, // the dialog already shows why
        };
        let (src, unlock) = (r.path.clone(), r.unlock());
        let first = pages.first().copied().unwrap_or(0) + 1;
        let last = pages.last().copied().unwrap_or(0) + 1;
        let name = if pages.len() == 1 {
            format!("{} - page {first}.pdf", stem(&src))
        } else {
            format!("{} - pages {first}-{last}.pdf", stem(&src))
        };
        self.close_extract(window, cx);
        let rx = cx.prompt_for_new_path(&dir_of(&src), Some(&name));
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut out))) = rx.await else {
                return;
            };
            if out.extension().is_none() {
                out.set_extension("pdf");
            }
            let n = pages.len();
            let result = cx
                .background_spawn(async move {
                    engine
                        .extract_pages(&src, unlock.as_deref(), &pages, &out)
                        .map(|_| out)
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                let what = if n == 1 {
                    "Saved 1 page to".to_string()
                } else {
                    format!("Saved {n} pages to")
                };
                ws.report(result, &what, cx)
            });
        })
        .detach();
    }

    pub(super) fn extract_view(
        &mut self,
        st: &Style,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let d = self.tools.extract.as_ref()?;
        let r = d.reader.read(cx);
        let count = r.page_count() as u32;
        let title = r.title();
        let parsed = parse_page_ranges(&d.text, count);
        let focused = self.tools.dialog_focus.is_focused(window);
        let (note, bad) = match &parsed {
            Ok(p) if p.len() == 1 => ("1 page".to_string(), false),
            Ok(p) => (format!("{} pages", p.len()), false),
            Err(e) => (e.clone(), true),
        };
        let can_save = parsed.is_ok();
        let serial = d.serial;
        let field = div()
            .id("extract-field")
            .track_focus(&self.tools.dialog_focus)
            .on_key_down(cx.listener(Self::extract_key))
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
            .child(d.text.clone())
            .child(
                div()
                    .ml(px(1.0))
                    .w(px(1.0))
                    .h(px(15.0))
                    .bg(st.c(|c| c.accent)),
            );
        let panel = div()
            .id("extract-dialog")
            .relative()
            .w(px(440.0))
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
                    .child("Save pages as a new PDF"),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.muted))
                    .child(format!(
                        "From {title} ({count} pages). Type pages and ranges, like 1-3, 7, 10-."
                    )),
            )
            .child(field)
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
                        dialog_button(st, "extract-cancel", "Cancel", false).on_click(
                            cx.listener(|ws, _, window, cx| ws.close_extract(window, cx)),
                        ),
                    )
                    .child(
                        dialog_button(st, "extract-save", "Save…", true)
                            .when(!can_save, |b| b.opacity(0.45))
                            .on_click(
                                cx.listener(|ws, _, window, cx| ws.confirm_extract(window, cx)),
                            ),
                    ),
            );
        Some(self.modal_layer(
            "extract",
            serial,
            panel.into_any_element(),
            st,
            Self::close_extract,
            cx,
        ))
    }

    /// A centred modal over a scrim: both fade, the panel rises 10 px. Esc or a
    /// click on the scrim runs `close`.
    pub(super) fn modal_layer(
        &mut self,
        id: &'static str,
        serial: u64,
        panel: AnyElement,
        st: &Style,
        close: fn(&mut Self, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panel = motion::enter(
            div().child(panel),
            ElementId::NamedInteger(format!("{id}-in").into(), serial),
            st.animations,
            motion::From::Below(10.0),
            motion::BASE,
            Duration::ZERO,
        );
        let scrim = motion::enter(
            div().absolute().inset_0().bg(st.c(|c| c.scrim)),
            ElementId::NamedInteger(format!("{id}-scrim").into(), serial),
            st.animations,
            motion::From::Fade,
            motion::BASE,
            Duration::ZERO,
        );
        div()
            .id(SharedString::from(format!("{id}-layer")))
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .key_context(OVERLAY)
            .on_action(cx.listener(move |ws, _: &CloseOverlay, window, cx| close(ws, window, cx)))
            // A click beside a dialog the user types into doesn't throw the
            // typing away; Esc and Cancel close those.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, window, cx| {
                    let typed = matches!(
                        id,
                        "password" | "sign" | "page-tools" | "name" | "print" | "extract"
                    );
                    if !typed {
                        close(ws, window, cx)
                    }
                }),
            )
            .child(scrim)
            .child(panel)
            .into_any_element()
    }

    // ── merge ──

    /// Pick more PDFs and write `reader`'s document followed by them to a new PDF.
    pub(super) fn merge_with(&mut self, reader: Entity<Reader>, cx: &mut Context<Self>) {
        let r = reader.read(cx);
        let (first, unlock) = (r.path.clone(), r.unlock());
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add to merge".into()),
        });
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(more))) = rx.await else { return };
            let more: Vec<PathBuf> = more.into_iter().filter(|p| *p != first).collect();
            if more.is_empty() {
                return;
            }
            let name = format!("{} + {} more.pdf", stem(&first), more.len());
            let Ok(save_rx) = this.update(cx, |_, cx| {
                cx.prompt_for_new_path(&dir_of(&first), Some(&name))
            }) else {
                return;
            };
            let Ok(Ok(Some(mut out))) = save_rx.await else {
                return;
            };
            if out.extension().is_none() {
                out.set_extension("pdf");
            }
            let n = more.len() + 1;
            let mut inputs = vec![(first, unlock)];
            inputs.extend(more.into_iter().map(|p| (p, None)));
            let result = cx
                .background_spawn(
                    async move { engine.merge(&inputs, &out).map(|pages| (out, pages)) },
                )
                .await;
            let _ = this.update(cx, |ws, cx| match result {
                Ok((out, pages)) => ws.report(
                    Ok(out),
                    &format!("Merged {n} PDFs ({pages} pages) into"),
                    cx,
                ),
                Err(e) => ws.report(Err(e), "", cx),
            });
        })
        .detach();
    }

    // ── split view ──

    /// Show `reader` beside the active tab.
    pub(super) fn split_with(&mut self, reader: Entity<Reader>, cx: &mut Context<Self>) {
        if self.active_reader() != Some(&reader) {
            self.tools.split = Some(reader);
            cx.notify();
        }
    }

    pub(super) fn close_split(&mut self, cx: &mut Context<Self>) {
        if self.tools.split.take().is_some() {
            cx.notify();
        }
    }

    /// Open `path` in a tab beside the active one, keeping the active one where it is.
    pub(super) fn open_to_side(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let before = self.active;
        self.open_tab(path, window, cx);
        let opened = self.active_reader().cloned();
        if let (Some(ix), Some(side)) = (before, opened) {
            if self.active != Some(ix) {
                self.activate(ix, window, cx);
                self.split_with(side, cx);
            }
        }
    }

    /// Ctrl+\: split with the next tab, or close the split.
    pub(super) fn toggle_split(&mut self, cx: &mut Context<Self>) {
        if self.tools.split.is_some() {
            self.close_split(cx);
        } else if let Some(a) = self.active {
            if self.tabs.len() > 1 {
                let next = self.tabs[(a + 1) % self.tabs.len()].clone();
                self.split_with(next, cx);
            }
        }
    }

    /// Clicking the side pane makes it the active one (the header, contents and
    /// shortcuts follow it); the old active tab moves to the side.
    pub(super) fn focus_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(side) = self.tools.split.clone() else {
            return;
        };
        let Some(ix) = self.tabs.iter().position(|t| *t == side) else {
            self.tools.split = None;
            return;
        };
        self.tools.split = self.active_reader().cloned();
        self.activate(ix, window, cx);
    }

    /// Menu rows shared by the page and tab menus.
    pub(super) fn pdf_tool_items(
        &self,
        reader: &Entity<Reader>,
        cx: &mut Context<Self>,
    ) -> Vec<MenuItem> {
        let is_pdf = reader.read(cx).is_pdf();
        let side = self.tools.split.as_ref() == Some(reader)
            || self.active_reader() == Some(reader) && self.tools.split.is_some();
        let (r1, r2, r3) = (reader.clone(), reader.clone(), reader.clone());
        let mut items = vec![
            MenuItem::new(
                "Page tools…",
                self.on_ws(cx, |ws, window, cx| ws.open_page_tools(window, cx)),
            )
            .icon("pages")
            .disabled(!is_pdf),
            MenuItem::new(
                "Save pages as a new PDF…",
                self.on_ws(cx, move |ws, window, cx| {
                    ws.open_extract(r1.clone(), window, cx)
                }),
            )
            .icon("file")
            .key("Ctrl+Shift+E")
            .disabled(!is_pdf),
            MenuItem::new(
                "Merge with other PDFs…",
                self.on_ws(cx, move |ws, _, cx| ws.merge_with(r2.clone(), cx)),
            )
            .icon("copy")
            .disabled(!is_pdf),
        ];
        items.push(MenuItem::Separator);
        if side {
            items.push(
                MenuItem::new(
                    "Close split view",
                    self.on_ws(cx, |ws, _, cx| ws.close_split(cx)),
                )
                .icon("sidebar")
                .key("Ctrl+\\"),
            );
        } else {
            items.push(
                MenuItem::new(
                    "Open to the side",
                    self.on_ws(cx, move |ws, _, cx| ws.split_with(r3.clone(), cx)),
                )
                .icon("sidebar")
                .disabled(self.active_reader() == Some(reader)),
            );
        }
        items
    }
}
