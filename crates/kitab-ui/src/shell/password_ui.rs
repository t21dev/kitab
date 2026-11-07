// SPDX-License-Identifier: AGPL-3.0-or-later

//! The password sheet (title bar › lock, ⋯ › Password…, the palette): put a
//! password on a PDF, change it, or take it off. Kitab saves a new copy and
//! leaves the open file as it is. Passwords live in the two fields and the
//! background job only; nothing logs or stores them (AGENTS.md rule 13).

use super::Workspace;
use super::tools_ui::{dialog_button, stem};
use crate::reader::Reader;
use crate::settings_view::{Click, segmented};
use crate::style::Style;
use crate::text_field::{FieldEvent, TextField};
use gpui::{
    AnyElement, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled, Subscription,
    Window, div, prelude::FluentBuilder, px,
};
use std::path::Path;

/// What the sheet does with the password.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// Put one on, or replace the one there.
    Set,
    /// Take it off.
    Remove,
}

pub(super) struct PasswordSheet {
    reader: Entity<Reader>,
    /// The PDF has a password (or an owner's lock) now.
    encrypted: bool,
    /// Its owner restricts it and it was opened with the user password, so
    /// Kitab won't lift the lock.
    restricted: bool,
    mode: Mode,
    first: Entity<TextField>,
    again: Entity<TextField>,
    serial: u64,
    _subs: [Subscription; 2],
}

/// The longest password, in characters (MuPDF keeps 127 bytes).
const MAX_CHARS: usize = 100;

impl Workspace {
    /// Open the sheet for the active tab, if it is a PDF.
    pub(super) fn open_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reader) = self.active_reader().cloned() else {
            return;
        };
        let Some((encrypted, restricted)) = reader.read(cx).lock_state() else {
            self.toast("Passwords are for PDFs.", true, None, cx);
            return;
        };
        self.menu = None;
        self.menu_closing = None;
        let field = |placeholder: &str, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut f = TextField::new("", false, placeholder, cx);
                f.set_secret(true);
                f
            })
        };
        let first = field("Password", cx);
        let again = field("Type it again", cx);
        let next = again.clone();
        let sub_first = cx.subscribe_in(
            &first,
            window,
            move |ws, _, e: &FieldEvent, window, cx| match e {
                FieldEvent::Submit => next.read(cx).focus(window),
                FieldEvent::Cancel => ws.close_password(window, cx),
                FieldEvent::Changed => cx.notify(),
            },
        );
        let sub_again = cx.subscribe_in(
            &again,
            window,
            |ws, _, e: &FieldEvent, window, cx| match e {
                FieldEvent::Submit => ws.run_password(window, cx),
                FieldEvent::Cancel => ws.close_password(window, cx),
                FieldEvent::Changed => cx.notify(),
            },
        );
        first.read(cx).focus(window);
        self.password_serial += 1;
        self.password = Some(PasswordSheet {
            reader,
            encrypted,
            restricted,
            mode: Mode::Set,
            first,
            again,
            serial: self.password_serial,
            _subs: [sub_first, sub_again],
        });
        cx.notify();
    }

    pub(super) fn close_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.password.take().is_some() {
            self.refocus(window, cx);
            cx.notify();
        }
    }

    /// The new password, or why the sheet can't go on yet (an empty reason
    /// means "nothing typed yet", which shows no message).
    fn password_choice(&self, cx: &Context<Self>) -> Result<Option<String>, String> {
        let Some(s) = self.password.as_ref() else {
            return Err(String::new());
        };
        if s.restricted {
            return Err("This PDF's owner restricts it. Open it with the owner password to change its password.".into());
        }
        if s.mode == Mode::Remove {
            return Ok(None);
        }
        let (a, b) = (s.first.read(cx).text(), s.again.read(cx).text());
        if a.chars().count() > MAX_CHARS {
            return Err(format!("Up to {MAX_CHARS} characters."));
        }
        if a.is_empty() || b.is_empty() {
            return Err(String::new());
        }
        if a != b {
            return Err("The two don't match.".into());
        }
        Ok(Some(a))
    }

    /// Save…: where the copy goes, then the copy, off the UI thread.
    fn run_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(new_password) = self.password_choice(cx) else {
            return;
        };
        let Some(sheet) = self.password.take() else {
            return;
        };
        self.refocus(window, cx);
        cx.notify();
        let r = sheet.reader.read(cx);
        let (src, unlock) = (r.path.clone(), r.unlock());
        let name = if new_password.is_some() {
            format!("{} (protected).pdf", stem(&src))
        } else {
            format!("{} (no password).pdf", stem(&src))
        };
        let dir = src.parent().map(Path::to_path_buf).unwrap_or_default();
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut out))) = rx.await else {
                return;
            };
            if out.extension().is_none() {
                out.set_extension("pdf");
            }
            let what = if new_password.is_some() {
                "Saved a copy with a password to"
            } else {
                "Saved a copy without a password to"
            };
            let result = cx
                .background_spawn(async move {
                    engine
                        .set_password(&src, unlock.as_deref(), new_password.as_deref(), &out)
                        .map(|_| out)
                })
                .await;
            let _ = this.update(cx, |ws, cx| ws.report(result, what, cx));
        })
        .detach();
    }

    pub(super) fn password_view(
        &mut self,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = self.password.as_ref()?;
        let title = s.reader.read(cx).title();
        let choice = self.password_choice(cx);
        let ws = cx.weak_entity();
        let set = |mode: Mode| -> Click {
            let ws = ws.clone();
            Box::new(move |_, cx| {
                let _ = ws.update(cx, |ws, cx| {
                    if let Some(s) = ws.password.as_mut() {
                        s.mode = mode;
                    }
                    cx.notify();
                });
            })
        };
        let heading = if s.encrypted {
            "Change or remove the password"
        } else {
            "Add a password"
        };
        let about = match s.mode {
            Mode::Set => format!(
                "Kitab saves a copy that asks for this password to open; {title} stays as it is. Keep the password somewhere safe: nobody can recover it."
            ),
            Mode::Remove => {
                format!("Kitab saves a copy anyone can open; {title} keeps its password.")
            }
        };
        let field = |f: &Entity<TextField>, bad: bool| {
            div()
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
                .child(f.clone())
        };
        let (note, bad) = match &choice {
            Err(e) => (e.clone(), !e.is_empty()),
            Ok(_) => (String::new(), false),
        };
        let mismatch = bad && s.mode == Mode::Set && !s.restricted;
        let ready = choice.is_ok();
        let panel = div()
            .id("password-sheet")
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
                    .child(heading),
            )
            .when(s.encrypted && !s.restricted, |d| {
                d.child(segmented(
                    vec![
                        (
                            SharedString::from("New password"),
                            s.mode == Mode::Set,
                            set(Mode::Set),
                        ),
                        (
                            SharedString::from("No password"),
                            s.mode == Mode::Remove,
                            set(Mode::Remove),
                        ),
                    ],
                    st,
                ))
            })
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.muted))
                    .child(about),
            )
            .when(s.mode == Mode::Set && !s.restricted, |d| {
                d.child(field(&s.first, false))
                    .child(field(&s.again, mismatch))
            })
            .when(!note.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(st.c(|c| c.bad))
                        .child(note),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        dialog_button(st, "password-cancel", "Cancel", false).on_click(
                            cx.listener(|ws, _, window, cx| ws.close_password(window, cx)),
                        ),
                    )
                    .child(
                        dialog_button(st, "password-save", "Save copy…", true)
                            .when(!ready, |b| b.opacity(0.45))
                            .on_click(cx.listener(|ws, _, window, cx| ws.run_password(window, cx))),
                    ),
            );
        let serial = s.serial;
        Some(self.modal_layer(
            "password",
            serial,
            panel.into_any_element(),
            st,
            Self::close_password,
            cx,
        ))
    }
}
