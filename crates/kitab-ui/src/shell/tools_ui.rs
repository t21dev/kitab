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
    /// A job under way, 0 to 1: the toast stays, with a bar that fills, and
    /// its text changes in place (see `Workspace::progress_toast`).
    pub progress: Option<f32>,
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
    pub combine: Option<super::combine::CombineDialog>,
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
            combine: None,
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

/// A picture Images to PDF takes, by its extension.
fn is_picture(p: &Path) -> bool {
    const EXT: &[&str] = &[
        "jpg", "jpeg", "jfif", "png", "webp", "tif", "tiff", "bmp", "gif", "jp2", "jpx", "pnm",
        "pbm", "pgm", "ppm", "pam",
    ];
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXT.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// A file name to sort by as people expect: case aside, and runs of digits
/// by their value ("scan 2" before "scan 10").
pub(super) fn natural_key(p: &Path) -> Vec<(u8, u64, String)> {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut key = Vec::new();
    let mut chars = name.chars().peekable();
    while let Some(&c) = chars.peek() {
        let digits = c.is_ascii_digit();
        let mut run = String::new();
        while let Some(&d) = chars.peek() {
            if d.is_ascii_digit() != digits {
                break;
            }
            run.push(d);
            chars.next();
        }
        if digits {
            key.push((0, run.parse().unwrap_or(u64::MAX), run));
        } else {
            key.push((1, 0, run));
        }
    }
    key
}

/// Cancel (secondary) or the dialog's action (primary), DESIGN.md § Buttons.
pub(super) fn dialog_button(
    st: &Style,
    id: &'static str,
    label: &'static str,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    crate::kbd::stop_solid(div().id(id), st)
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
                .hover(|b| b.bg(st.raised_hover()))
        })
        .child(label)
}

impl Workspace {
    // ── toast ──

    /// The system's file picker couldn't open. On Linux it comes from the
    /// desktop portal (xdg-desktop-portal and the desktop's own backend);
    /// without one, Open file did nothing at all.
    pub(super) fn picker_failed(&mut self, cx: &mut Context<Self>) {
        let text = if cfg!(target_os = "linux") || cfg!(target_os = "freebsd") {
            "Couldn't open the file picker. Kitab uses your desktop's: install xdg-desktop-portal and the one for your desktop (xdg-desktop-portal-kde on KDE, -gnome on GNOME, -gtk elsewhere), then try again. Dropping a file on the window works meanwhile."
        } else {
            "Couldn't open the file picker. Try again, or drop a file on the window."
        };
        self.toast(text, true, None, cx);
    }

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
            progress: None,
        });
        // Errors stay until closed (DESIGN.md › Toasts); the rest go by.
        if !bad {
            self.after(TOAST_FOR, cx, move |ws, cx| ws.dismiss_toast(serial, cx));
        }
        cx.notify();
    }

    /// A long job's progress (printing many pages): one toast that stays
    /// where it is while its text and bar change, instead of a new toast
    /// for every step. The job's outcome replaces it with [`Self::toast`];
    /// a job that ends without one calls [`Self::end_progress`].
    pub(super) fn progress_toast(
        &mut self,
        text: impl Into<SharedString>,
        done: usize,
        total: usize,
        cx: &mut Context<Self>,
    ) {
        let progress = Some((done as f32 / total.max(1) as f32).clamp(0.0, 1.0));
        match self.tools.toast.as_mut().filter(|t| t.progress.is_some()) {
            Some(t) => {
                t.text = text.into();
                t.progress = progress;
            }
            None => {
                self.tools.toast_serial += 1;
                self.tools.toast_closing = None;
                self.tools.toast = Some(Toast {
                    text: text.into(),
                    bad: false,
                    open: None,
                    serial: self.tools.toast_serial,
                    progress,
                });
            }
        }
        cx.notify();
    }

    /// The job behind a progress toast ended with nothing to say (it was
    /// cancelled): the toast goes.
    pub(super) fn end_progress(&mut self, cx: &mut Context<Self>) {
        if let Some(serial) = self
            .tools
            .toast
            .as_ref()
            .filter(|t| t.progress.is_some())
            .map(|t| t.serial)
        {
            self.dismiss_toast(serial, cx);
        }
    }

    pub(super) fn dismiss_toast(&mut self, serial: u64, cx: &mut Context<Self>) {
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
                .hover(|d| d.bg(st.hover_bg()))
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
                    .path(icon(if t.bad {
                        "alert"
                    } else if t.progress.is_some() {
                        "print"
                    } else {
                        "check"
                    }))
                    .size(px(16.0))
                    .text_color(if t.bad {
                        st.c(|c| c.bad)
                    } else if t.progress.is_some() {
                        st.c(|c| c.muted)
                    } else {
                        st.c(|c| c.ok)
                    }),
            )
            // A job under way: a thin bar along the bottom that fills.
            .children(t.progress.map(|done| {
                div()
                    .absolute()
                    .left(px(16.0))
                    .right(px(16.0))
                    .bottom(px(5.0))
                    .h(px(2.0))
                    .rounded_full()
                    .bg(st.c(|c| c.line))
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(done))
                            .rounded_full()
                            .bg(st.c(|c| c.accent)),
                    )
            }))
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
                        .hover(|d| d.bg(st.hover_bg()))
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
        // Settings › Appearance › Messages: top centre by default, under the
        // title bar; at the bottom, above the dock. Each slides in from its
        // own edge and leaves the same way.
        use crate::settings::ToastAt;
        let at = st.toast_at;
        let from = if at.top() {
            motion::From::Above(10.0)
        } else {
            motion::From::Below(10.0)
        };
        let pill = if closing {
            motion::exit(
                pill,
                ElementId::NamedInteger("toast-out".into(), serial),
                from,
                motion::EXIT_BASE,
            )
        } else {
            motion::enter(
                pill,
                ElementId::NamedInteger("toast-in".into(), serial),
                st.animations,
                from,
                motion::BASE,
                Duration::ZERO,
            )
        };
        let row = div().absolute().left_0().right_0().px(px(16.0)).flex();
        let row = if at.top() {
            row.top(st.px(|m| m.title_bar) + px(12.0))
        } else {
            row.bottom(px(72.0))
        };
        let row = match at {
            ToastAt::TopLeft | ToastAt::BottomLeft => row.justify_start(),
            ToastAt::TopCenter | ToastAt::BottomCenter => row.justify_center(),
            ToastAt::TopRight | ToastAt::BottomRight => row.justify_end(),
        };
        Some(row.child(pill).into_any_element())
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
        if self.tools.extract.is_none() {
            return;
        }
        self.exit_modal("extract", window, cx, |ws, window, cx| {
            if ws.tools.extract.take().is_some() {
                ws.refocus(window, cx);
                cx.notify();
            }
        });
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
            let picked = rx.await;
            // The picker couldn't open (Linux without a desktop portal): say so.
            if matches!(picked, Ok(Err(_)) | Err(_)) {
                let _ = this.update(cx, |ws, cx| ws.picker_failed(cx));
            }
            let Ok(Ok(Some(mut out))) = picked else {
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

    /// Save a copy of the PDF with the marks and filled-in fields, leaving
    /// the file itself as it is (the marks stay unsaved in it).
    pub(super) fn save_copy_with_marks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reader) = self.active_reader().cloned() else {
            return;
        };
        let r = reader.read(cx);
        if !r.is_pdf() {
            return;
        }
        let (src, unlock) = (r.path.clone(), r.unlock());
        let (marks, edits) = r.pending_marks();
        if marks.is_empty() && edits.is_empty() {
            self.toast("There are no marks to save yet.", false, None, cx);
            return;
        }
        let rx =
            cx.prompt_for_new_path(&dir_of(&src), Some(&format!("{} (marked).pdf", stem(&src))));
        let engine = self.engine.clone();
        cx.spawn_in(window, async move |this, cx| {
            let picked = rx.await;
            // The picker couldn't open (Linux without a desktop portal): say so.
            if matches!(picked, Ok(Err(_)) | Err(_)) {
                let _ = this.update(cx, |ws, cx| ws.picker_failed(cx));
            }
            let Ok(Ok(Some(mut out))) = picked else {
                return;
            };
            if out.extension().is_none() {
                out.set_extension("pdf");
            }
            // The same file: that's Save (Ctrl+S), which reopens it after.
            if out == src {
                let _ = reader.update(cx, |r, cx| r.save_marks(cx));
                return;
            }
            let result = cx
                .background_spawn(async move {
                    engine
                        .write_marks(&src, unlock.as_deref(), &marks, &edits, &out)
                        .map(|_| out)
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.report(result, "Saved a copy with your marks to", cx)
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
            // Long ranges stay inside the box, the end (and the caret) in view.
            .overflow_hidden()
            .when(d.text.chars().count() > 40, |f| f.justify_end())
            .child(div().flex_none().whitespace_nowrap().child(d.text.clone()))
            .child(
                div()
                    .flex_none()
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

    /// Close dialog `id` the way it came: it fades and sinks for
    /// [`motion::EXIT_BASE`], then `finish` drops it. At once with animations
    /// off, or when it's already leaving (a second Esc).
    pub(super) fn exit_modal(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
        finish: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let serial = self.modal_serials.get(id).copied();
        let leaving = self.modal_exit.is_some_and(|(i, _)| i == id);
        let Some(serial) = serial.filter(|_| Style::get(cx).animations && !leaving) else {
            self.modal_exit = None;
            finish(self, window, cx);
            return;
        };
        self.modal_exit = Some((id, serial));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(motion::EXIT_BASE).await;
            let _ = this.update_in(cx, |ws, window, cx| {
                if ws.modal_exit == Some((id, serial)) {
                    ws.modal_exit = None;
                    finish(ws, window, cx);
                }
            });
        })
        .detach();
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
        // Drawn again (reopened) while an earlier one was leaving: that one is gone.
        if self.modal_serials.insert(id, serial) != Some(serial)
            && self.modal_exit.is_some_and(|(i, _)| i == id)
        {
            self.modal_exit = None;
        }
        let leaving = self.modal_exit == Some((id, serial));
        let (panel, scrim) = if leaving {
            (
                motion::exit(
                    div().child(panel),
                    ElementId::NamedInteger(format!("{id}-out").into(), serial),
                    motion::From::Below(10.0),
                    motion::EXIT_BASE,
                ),
                motion::exit(
                    div().absolute().inset_0().bg(st.c(|c| c.scrim)),
                    ElementId::NamedInteger(format!("{id}-scrim-out").into(), serial),
                    motion::From::Fade,
                    motion::EXIT_BASE,
                ),
            )
        } else {
            (
                motion::enter(
                    div().child(panel),
                    ElementId::NamedInteger(format!("{id}-in").into(), serial),
                    st.animations,
                    motion::From::Below(10.0),
                    motion::BASE,
                    Duration::ZERO,
                ),
                motion::enter(
                    div().absolute().inset_0().bg(st.c(|c| c.scrim)),
                    ElementId::NamedInteger(format!("{id}-scrim").into(), serial),
                    st.animations,
                    motion::From::Fade,
                    motion::BASE,
                    Duration::ZERO,
                ),
            )
        };
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
                        "password"
                            | "sign"
                            | "page-tools"
                            | "name"
                            | "print"
                            | "extract"
                            | "combine"
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

    /// Pick more PDFs, then set the order in the Combine sheet, `reader`'s
    /// document first.
    pub(super) fn merge_with(
        &mut self,
        reader: Entity<Reader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = reader.read(cx);
        let (first, unlock) = (r.path.clone(), r.unlock());
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add to combine".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let picked = rx.await;
            // The picker couldn't open (Linux without a desktop portal): say so.
            if matches!(picked, Ok(Err(_)) | Err(_)) {
                let _ = this.update(cx, |ws, cx| ws.picker_failed(cx));
            }
            let Ok(Ok(Some(more))) = picked else { return };
            let mut inputs = vec![(first.clone(), unlock)];
            inputs.extend(
                more.into_iter()
                    .filter(|p| *p != first)
                    .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
                    .map(|p| (p, None)),
            );
            if inputs.len() < 2 {
                return;
            }
            let _ = this.update_in(cx, |ws, window, cx| ws.open_combine(inputs, window, cx));
        })
        .detach();
    }

    /// Ask where, then write `inputs` in this order to one new PDF.
    pub(super) fn merge_inputs(
        &mut self,
        inputs: Vec<(PathBuf, Option<String>)>,
        cx: &mut Context<Self>,
    ) {
        let Some((first, _)) = inputs.first().cloned() else {
            return;
        };
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let name = format!("{} + {} more.pdf", stem(&first), inputs.len() - 1);
            let Ok(save_rx) = this.update(cx, |_, cx| {
                cx.prompt_for_new_path(&dir_of(&first), Some(&name))
            }) else {
                return;
            };
            let picked = save_rx.await;
            // The picker couldn't open (Linux without a desktop portal): say so.
            if matches!(picked, Ok(Err(_)) | Err(_)) {
                let _ = this.update(cx, |ws, cx| ws.picker_failed(cx));
            }
            let Ok(Ok(Some(mut out))) = picked else {
                return;
            };
            if out.extension().is_none() {
                out.set_extension("pdf");
            }
            let n = inputs.len();
            let result = cx
                .background_spawn(
                    async move { engine.merge(&inputs, &out).map(|pages| (out, pages)) },
                )
                .await;
            let _ = this.update(cx, |ws, cx| match result {
                Ok((out, pages)) => ws.report(
                    Ok(out),
                    &format!("Combined {n} PDFs ({pages} pages) into"),
                    cx,
                ),
                Err(e) => ws.report(Err(e), "", cx),
            });
        })
        .detach();
    }

    // ── export as Markdown or text ──

    /// Ask where, then write the document in `reader` as Markdown (pictures
    /// in `<name>_images/` beside it) or plain text. The message offers to
    /// open the result, which Kitab reads too.
    pub(super) fn export_document(
        &mut self,
        reader: Entity<Reader>,
        kind: crate::ExportKind,
        cx: &mut Context<Self>,
    ) {
        let r = reader.read(cx);
        let (src, unlock) = (r.path.clone(), r.unlock());
        let ext = match kind {
            crate::ExportKind::Markdown => "md",
            crate::ExportKind::Text => "txt",
        };
        let name = format!("{}.{ext}", stem(&src));
        let save_rx = cx.prompt_for_new_path(&dir_of(&src), Some(&name));
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let picked = save_rx.await;
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
                        .export_document(&src, unlock.as_deref(), kind, &out)
                        .map(|done| (out, done))
                })
                .await;
            let _ = this.update(cx, |ws, cx| match result {
                Ok((out, done)) => {
                    let pages = if done.pages == 1 {
                        crate::i18n::tr("1 page").to_string()
                    } else {
                        crate::i18n::trf("{count} pages", &[("count", &done.pages.to_string())])
                    };
                    let what = match (kind, done.images) {
                        (crate::ExportKind::Text, _) => {
                            crate::i18n::trf("Exported {pages} as text:", &[("pages", &pages)])
                        }
                        (crate::ExportKind::Markdown, 0) => {
                            crate::i18n::trf("Exported {pages} as Markdown:", &[("pages", &pages)])
                        }
                        (crate::ExportKind::Markdown, n) => crate::i18n::trf(
                            "Exported {pages} as Markdown, with {count} pictures:",
                            &[("pages", &pages), ("count", &n.to_string())],
                        ),
                    };
                    ws.report(Ok(out), &what, cx);
                }
                Err(e) => ws.report(Err(e), "", cx),
            });
        })
        .detach();
    }

    // ── pictures to PDF ──

    /// Pick pictures and write them, one a page, to a new PDF (sorted by
    /// name, as a folder of scans or photos lists them; Page tools › Move
    /// changes the order afterwards).
    pub(super) fn images_to_pdf(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Make a PDF".into()),
        });
        cx.spawn(async move |this, cx| {
            let picked = rx.await;
            // The picker couldn't open (Linux without a desktop portal): say so.
            if matches!(picked, Ok(Err(_)) | Err(_)) {
                let _ = this.update(cx, |ws, cx| ws.picker_failed(cx));
            }
            let Ok(Ok(Some(paths))) = picked else { return };
            let _ = this.update(cx, |ws, cx| ws.pictures_to_pdf(paths, cx));
        })
        .detach();
    }

    /// The pictures among `paths`, in name order, to one new PDF (asks where).
    pub(super) fn pictures_to_pdf(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let mut pictures: Vec<PathBuf> = paths.into_iter().filter(|p| is_picture(p)).collect();
            if pictures.is_empty() {
                let _ = this.update(cx, |ws, cx| {
                    ws.report(
                        Err("those aren't pictures Kitab can put in a PDF (JPEG, PNG, WebP, TIFF, BMP, GIF)".into()),
                        "",
                        cx,
                    )
                });
                return;
            }
            pictures.sort_by_cached_key(|p| natural_key(p));
            let first = pictures[0].clone();
            let name = format!("{}.pdf", stem(&first));
            let Ok(save_rx) = this.update(cx, |_, cx| {
                cx.prompt_for_new_path(&dir_of(&first), Some(&name))
            }) else {
                return;
            };
            let picked = save_rx.await;
            if matches!(picked, Ok(Err(_)) | Err(_)) {
                let _ = this.update(cx, |ws, cx| ws.picker_failed(cx));
            }
            let Ok(Ok(Some(mut out))) = picked else {
                return;
            };
            if out.extension().is_none() {
                out.set_extension("pdf");
            }
            let n = pictures.len();
            let result = cx
                .background_spawn(async move {
                    engine.images_to_pdf(&pictures, &out).map(|_| out)
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                let what = if n == 1 {
                    "Made a PDF of 1 picture:".to_string()
                } else {
                    format!("Made a PDF of {n} pictures:")
                };
                ws.report(result, &what, cx)
            });
        })
        .detach();
    }

    // ── tools from the file manager's right-click menu ──

    /// A tool picked in Explorer, Finder or a Linux file manager on `paths`
    /// (`kitab --tool …`): Kitab opens that tool with the files filled in.
    pub(crate) fn run_tool(
        &mut self,
        tool: kitab_os::instance::Tool,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use kitab_os::instance::Tool;
        let is_pdf = |p: &PathBuf| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        match tool {
            Tool::Merge => {
                let mut pdfs: Vec<PathBuf> = paths.into_iter().filter(is_pdf).collect();
                // The order a folder lists them in (2 before 10).
                pdfs.sort_by_cached_key(|p| natural_key(p));
                if pdfs.len() < 2 {
                    self.report(
                        Err("Select two or more PDFs to combine them.".into()),
                        "",
                        cx,
                    );
                    return;
                }
                self.open_combine(pdfs.into_iter().map(|p| (p, None)).collect(), window, cx);
            }
            Tool::Pictures => self.pictures_to_pdf(paths, cx),
            Tool::Compress | Tool::DeletePages | Tool::MovePages | Tool::RotatePages => {
                use super::page_tools_ui::Tool as Page;
                let page = match tool {
                    Tool::DeletePages => Page::Delete,
                    Tool::MovePages => Page::Move,
                    Tool::RotatePages => Page::Rotate,
                    _ => Page::Compress,
                };
                let Some(pdf) = paths.into_iter().find(is_pdf) else {
                    self.report(Err("Page tools work on PDFs.".into()), "", cx);
                    return;
                };
                self.open_tab(pdf, window, cx);
                // Page tools need the PDF open: once it is, on that tool.
                self.when_active_opens(window, cx, move |ws, window, cx| {
                    ws.open_page_tools_on(page, window, cx)
                });
            }
            Tool::Export | Tool::ExportText => {
                let Some(doc) = paths.into_iter().next() else {
                    return;
                };
                let kind = if tool == Tool::Export {
                    crate::ExportKind::Markdown
                } else {
                    crate::ExportKind::Text
                };
                self.open_tab(doc, window, cx);
                if let Some(r) = self.active_reader().cloned() {
                    self.export_document(r, kind, cx);
                }
            }
        }
    }

    /// Run `f` once the active tab's document has opened (15 s at most; a
    /// file that fails to open shows its own message instead).
    fn when_active_opens(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let Some(reader) = self.active_reader().cloned() else {
            return;
        };
        let mut f = Some(f);
        cx.spawn_in(window, async move |this, cx| {
            for _ in 0..150 {
                let open = cx
                    .update(|_, cx| reader.read(cx).info_ready())
                    .unwrap_or(false);
                if open {
                    let _ = this.update_in(cx, |ws, window, cx| {
                        if let Some(f) = f.take() {
                            f(ws, window, cx)
                        }
                    });
                    return;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
            }
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
        if let (Some(ix), Some(side)) = (before, opened)
            && self.active != Some(ix)
        {
            self.activate(ix, window, cx);
            self.split_with(side, cx);
        }
    }

    /// Ctrl+\: split with the next tab, or close the split.
    pub(super) fn toggle_split(&mut self, cx: &mut Context<Self>) {
        if self.tools.split.is_some() {
            self.close_split(cx);
        } else if let Some(a) = self.active
            && self.tabs.len() > 1
        {
            let next = self.tabs[(a + 1) % self.tabs.len()].clone();
            self.split_with(next, cx);
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
        let (r1, r2, r3, r4) = (
            reader.clone(),
            reader.clone(),
            reader.clone(),
            reader.clone(),
        );
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
                crate::i18n::tr("Export as Markdown…"),
                self.on_ws(cx, move |ws, _, cx| {
                    ws.export_document(r4.clone(), crate::ExportKind::Markdown, cx)
                }),
            )
            .icon("file"),
            MenuItem::new(
                "Merge with other PDFs…",
                self.on_ws(cx, move |ws, window, cx| {
                    ws.merge_with(r2.clone(), window, cx)
                }),
            )
            .icon("copy")
            .disabled(!is_pdf),
            MenuItem::new(
                "Make a PDF from pictures…",
                self.on_ws(cx, |ws, _, cx| ws.images_to_pdf(cx)),
            )
            .icon("file"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_sort_as_a_folder_lists_them() {
        let mut names: Vec<PathBuf> = ["Scan 10.jpg", "scan 2.jpg", "Scan 1.jpg", "cover.png"]
            .iter()
            .map(PathBuf::from)
            .collect();
        names.sort_by_cached_key(|p| natural_key(p));
        let got: Vec<_> = names
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            got,
            ["cover.png", "Scan 1.jpg", "scan 2.jpg", "Scan 10.jpg"]
        );
    }

    #[test]
    fn only_pictures_are_taken() {
        assert!(is_picture(Path::new("a/IMG_0001.JPG")));
        assert!(is_picture(Path::new("page.webp")));
        assert!(!is_picture(Path::new("book.pdf")));
        assert!(!is_picture(Path::new("noext")));
    }
}
