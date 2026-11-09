// SPDX-License-Identifier: AGPL-3.0-or-later

//! Signing a PDF (PLAN.md §3.7), in three steps that each wait for the
//! user: the key (a `.pfx`/`.p12` file and its password, plus an optional
//! reason and place), where the signature goes (a click on a page), and a
//! confirm that names the signer, the page and the new file. Nothing is
//! signed before Sign in the last step. The password lives in the request
//! until the signature is written, and is never stored or logged.

use super::Workspace;
use super::tools_ui::{dialog_button, stem};
use crate::engine::SignRequest;
use crate::reader::Reader;
use crate::style::Style;
use crate::text_field::{FieldEvent, TextField};
use gpui::{
    AnyElement, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, PathPromptOptions, StatefulInteractiveElement, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};
use std::path::{Path, PathBuf};

/// Step one: the key.
pub(super) struct SignSheet {
    reader: Entity<Reader>,
    key_file: Option<PathBuf>,
    password: Entity<TextField>,
    reason: Entity<TextField>,
    location: Entity<TextField>,
    error: Option<String>,
    checking: bool,
    serial: u64,
    _subs: Vec<Subscription>,
}

/// Step two: the key works; waiting for the click on a page.
pub(super) struct SignPlan {
    reader: Entity<Reader>,
    req: SignRequest,
    name: String,
    subject: String,
}

/// Step three: everything known; Sign or Cancel.
pub(super) struct SignConfirm {
    plan: SignPlan,
    out: PathBuf,
    serial: u64,
}

#[derive(Default)]
pub(super) struct SignState {
    pub sheet: Option<SignSheet>,
    pub plan: Option<SignPlan>,
    pub confirm: Option<SignConfirm>,
    pub serial: u64,
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

impl Workspace {
    /// Sign…: step one for the active tab.
    pub(super) fn open_sign(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reader) = self.active_reader().cloned() else {
            return;
        };
        match reader.read(cx).lock_state() {
            None => {
                self.toast("Only PDFs can be signed.", true, None, cx);
                return;
            }
            Some((true, _)) => {
                self.toast(
                    "This PDF has a password. Save a copy without it (Password…) to sign it.",
                    true,
                    None,
                    cx,
                );
                return;
            }
            Some(_) => {}
        }
        self.menu = None;
        self.menu_closing = None;
        let field = |placeholder: &str, secret: bool, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut f = TextField::new("", false, placeholder, cx);
                f.set_secret(secret);
                f
            })
        };
        let password = field("The certificate file's password", true, cx);
        let reason = field("Reason (optional)", false, cx);
        let location = field("Place (optional)", false, cx);
        let mut subs = Vec::new();
        for f in [&password, &reason, &location] {
            subs.push(
                cx.subscribe_in(f, window, |ws, _, e: &FieldEvent, window, cx| match e {
                    FieldEvent::Submit => ws.check_sign_key(cx),
                    FieldEvent::Cancel => ws.close_sign(window, cx),
                    FieldEvent::Changed => cx.notify(),
                }),
            );
        }
        password.read(cx).focus(window);
        self.sign.serial += 1;
        self.sign.sheet = Some(SignSheet {
            reader,
            key_file: None,
            password,
            reason,
            location,
            error: None,
            checking: false,
            serial: self.sign.serial,
            _subs: subs,
        });
        cx.notify();
    }

    pub(super) fn close_sign(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sign.sheet.is_none() {
            return;
        }
        self.exit_modal("sign", window, cx, |ws, window, cx| {
            if ws.sign.sheet.take().is_some() {
                ws.refocus(window, cx);
                cx.notify();
            }
        });
    }

    fn choose_sign_key(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose certificate".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.pop() else { return };
            let _ = this.update(cx, |ws, cx| {
                if let Some(s) = ws.sign.sheet.as_mut() {
                    s.key_file = Some(path);
                    s.error = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Next: open the key with its password, then wait for the click.
    fn check_sign_key(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.sign.sheet.as_mut() else {
            return;
        };
        let Some(key_file) = s.key_file.clone() else {
            s.error = Some("Choose a certificate file (.pfx or .p12) first.".into());
            cx.notify();
            return;
        };
        let password = s.password.read(cx).text();
        let text = |f: &Entity<TextField>, cx: &Context<Self>| {
            let t = f.read(cx).text().trim().to_string();
            (!t.is_empty()).then_some(t)
        };
        let (reason, location) = (text(&s.reason, cx), text(&s.location, cx));
        s.checking = true;
        s.error = None;
        cx.notify();
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let (file, pw) = (key_file.clone(), password.clone());
            let who = cx
                .background_spawn(async move { engine.signer_of(&file, &pw) })
                .await;
            let _ = this.update(cx, |ws, cx| {
                let Some(sheet) = ws.sign.sheet.as_mut() else {
                    return;
                };
                match who {
                    Err(e) => {
                        sheet.checking = false;
                        sheet.error = Some(format!("Can't use it: {e}."));
                        cx.notify();
                    }
                    Ok((name, subject)) => {
                        let Some(sheet) = ws.sign.sheet.take() else {
                            return;
                        };
                        let reader = sheet.reader.clone();
                        ws.sign.plan = Some(SignPlan {
                            reader: reader.clone(),
                            req: SignRequest {
                                key_file,
                                password,
                                page: 0,
                                rect: [0.0; 4],
                                reason,
                                location,
                            },
                            name,
                            subject,
                        });
                        reader.update(cx, |r, cx| r.start_placing(cx));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// The reader's click (or its cancel): pick the new file, then confirm.
    pub(super) fn signature_placed(
        &mut self,
        spot: Option<(u32, [f32; 4])>,
        cx: &mut Context<Self>,
    ) {
        let Some(mut plan) = self.sign.plan.take() else {
            return;
        };
        let Some((page, rect)) = spot else {
            cx.notify();
            return;
        };
        plan.req.page = page;
        plan.req.rect = rect;
        let src = plan.reader.read(cx).path.clone();
        let dir = src.parent().map(Path::to_path_buf).unwrap_or_default();
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{} (signed).pdf", stem(&src))));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut out))) = rx.await else {
                return;
            };
            if out.extension().is_none() {
                out.set_extension("pdf");
            }
            let _ = this.update(cx, |ws, cx| {
                ws.sign.serial += 1;
                ws.sign.confirm = Some(SignConfirm {
                    plan,
                    out,
                    serial: ws.sign.serial,
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn close_sign_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sign.confirm.is_none() {
            return;
        }
        self.exit_modal("sign-confirm", window, cx, |ws, window, cx| {
            if ws.sign.confirm.take().is_some() {
                ws.refocus(window, cx);
                cx.notify();
            }
        });
    }

    /// Sign: write the signed copy, off the UI thread.
    fn run_sign(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.sign.confirm.take() else {
            return;
        };
        self.refocus(window, cx);
        let src = c.plan.reader.read(cx).path.clone();
        let (req, out) = (c.plan.req, c.out);
        let engine = self.engine.clone();
        self.toast("Signing…", false, None, cx);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { engine.sign_pdf(&src, &req, &out).map(|_| out) })
                .await;
            let _ = this.update(cx, |ws, cx| ws.report(result, "Signed and saved to", cx));
        })
        .detach();
        cx.notify();
    }

    fn sign_panel(st: &Style) -> gpui::Stateful<gpui::Div> {
        div()
            .id("sign-panel")
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
    }

    pub(super) fn sign_view(&mut self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.sign.sheet.as_ref()?;
        let title = s.reader.read(cx).title();
        let field = |f: &Entity<TextField>| {
            div()
                .px(px(10.0))
                .py(px(6.0))
                .rounded(st.px(|m| m.control_radius))
                .bg(st.c(|c| c.hover))
                .border_1()
                .border_color(st.c(|c| c.line))
                .child(f.clone())
        };
        let key_row = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .text_color(if s.key_file.is_some() {
                        st.c(|c| c.text)
                    } else {
                        st.c(|c| c.faint)
                    })
                    .child(
                        s.key_file
                            .as_deref()
                            .map(file_name)
                            .unwrap_or_else(|| "No certificate file chosen".into()),
                    ),
            )
            .child(
                dialog_button(st, "sign-choose", "Choose…", false)
                    .on_click(cx.listener(|ws, _, _, cx| ws.choose_sign_key(cx))),
            );
        let ready = s.key_file.is_some() && !s.checking;
        let panel = Self::sign_panel(st)
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(st.c(|c| c.text))
                    .child("Sign this PDF"),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.muted))
                    .child(format!(
                        "Sign {title} with your certificate file (.pfx or .p12). Next you click where the signature goes; Kitab saves a signed copy."
                    )),
            )
            .child(key_row)
            .child(field(&s.password))
            .child(field(&s.reason))
            .child(field(&s.location))
            .when_some(s.error.clone(), |d, e| {
                d.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(st.c(|c| c.bad))
                        .child(e),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        dialog_button(st, "sign-cancel", "Cancel", false)
                            .on_click(cx.listener(|ws, _, window, cx| ws.close_sign(window, cx))),
                    )
                    .child(
                        dialog_button(
                            st,
                            "sign-next",
                            if s.checking { "Checking…" } else { "Next: place it" },
                            true,
                        )
                        .when(!ready, |b| b.opacity(0.45))
                        .on_click(cx.listener(|ws, _, _, cx| ws.check_sign_key(cx))),
                    ),
            );
        let serial = s.serial;
        Some(self.modal_layer(
            "sign",
            serial,
            panel.into_any_element(),
            st,
            Self::close_sign,
            cx,
        ))
    }

    pub(super) fn sign_confirm_view(
        &mut self,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let c = self.sign.confirm.as_ref()?;
        let title = c.plan.reader.read(cx).title();
        let row = |label: &'static str, value: String, note: Option<String>| {
            div()
                .flex()
                .gap(px(12.0))
                .child(
                    div()
                        .w(px(70.0))
                        .flex_none()
                        .text_size(px(12.5))
                        .text_color(st.c(|c| c.muted))
                        .child(label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(st.c(|c| c.text))
                                .child(value),
                        )
                        .when_some(note, |d, n| {
                            d.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(st.c(|c| c.faint))
                                    .child(n),
                            )
                        }),
                )
        };
        let mut what = format!("A new signature on page {} of {title}", c.plan.req.page + 1);
        if let Some(r) = &c.plan.req.reason {
            what.push_str(&format!(", reason: {r}"));
        }
        let panel = Self::sign_panel(st)
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(st.c(|c| c.text))
                    .child("Sign it?"),
            )
            .child(row(
                "Signer",
                c.plan.name.clone(),
                Some(c.plan.subject.clone()),
            ))
            .child(row("Signature", what, None))
            .child(row(
                "Saved as",
                file_name(&c.out),
                Some(
                    c.out
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                ),
            ))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(st.c(|c| c.faint))
                    .child(
                        "The original stays as it is. Signatures already in the file stay valid.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        dialog_button(st, "sign-confirm-cancel", "Cancel", false).on_click(
                            cx.listener(|ws, _, window, cx| ws.close_sign_confirm(window, cx)),
                        ),
                    )
                    .child(
                        dialog_button(st, "sign-confirm-go", "Sign", true)
                            .on_click(cx.listener(|ws, _, window, cx| ws.run_sign(window, cx))),
                    ),
            );
        let serial = c.serial;
        Some(self.modal_layer(
            "sign-confirm",
            serial,
            panel.into_any_element(),
            st,
            Self::close_sign_confirm,
            cx,
        ))
    }
}
