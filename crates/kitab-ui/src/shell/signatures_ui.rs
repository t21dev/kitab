// SPDX-License-Identifier: AGPL-3.0-or-later

//! The sidebar's Signatures pane (PLAN.md §3.7): one card per signature with
//! its status, signer, time and what the check found, and a way to the page
//! it sits on and to the trusted certificates.

use super::Workspace;
use crate::icons::icon;
use crate::reader::{Reader, status_words};
use crate::style::Style;
use gpui::{
    AnyElement, Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px, svg,
};
use kitab_core::{SigCoverage, SigModification, SigStatus, SignatureCheck};

/// What the check found, line by line, in plain words.
pub fn findings(c: &SignatureCheck) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(p) = &c.problem {
        out.push(format!("Couldn't be checked: {p}."));
        return out;
    }
    out.push(if c.intact {
        "The signed content hasn't been altered.".into()
    } else {
        "The signed content was altered after signing.".into()
    });
    if !c.valid {
        out.push("The signature doesn't match the signer's certificate.".into());
    }
    match c.coverage {
        SigCoverage::EntireFile => {}
        SigCoverage::EntireRevision => match c.modification {
            SigModification::None => out.push("Later revisions add nothing that matters.".into()),
            SigModification::FormFilling => {
                out.push("Later, form fields were filled or others signed.".into())
            }
            SigModification::Annotations => {
                out.push("Later, comments or markup were added.".into())
            }
            SigModification::Other => out.push("Later, the document itself was changed.".into()),
        },
        SigCoverage::Partial => {
            out.push("Parts of the file aren't covered by the signature.".into())
        }
    }
    if c.trusted {
        out.push("The signer's certificate is one you trust.".into());
    } else {
        out.push("The signer's certificate isn't one you trust.".into());
    }
    if c.expired {
        out.push("The certificate wasn't valid when it signed.".into());
    }
    out
}

impl Workspace {
    pub(super) fn signatures_pane(
        &mut self,
        reader: &Entity<Reader>,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sigs: Vec<SignatureCheck> = reader
            .read(cx)
            .signatures()
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let mut col = div().flex().flex_col().gap(px(10.0)).p(px(10.0));
        if sigs.is_empty() {
            return col
                .child(
                    div()
                        .px(px(6.0))
                        .text_size(st.px(|m| m.ui_text))
                        .text_color(st.c(|c| c.faint))
                        .child("This document has no signatures."),
                )
                .into_any_element();
        }
        for (ix, c) in sigs.iter().enumerate() {
            let status = c.status();
            let (words, colour) = status_words(status, st);
            let jump = reader.clone();
            let page = c.page;
            let mut card = div()
                .id(("sig-card", ix))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(12.0))
                .rounded(px(10.0))
                .bg(st.c(|c| c.raised))
                .shadow(st.elev())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(11.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colour)
                        .child(svg().path(icon("shield")).size(px(13.0)).text_color(colour))
                        .child(words),
                )
                .child(
                    div()
                        .text_size(st.px(|m| m.ui_text))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(st.c(|c| c.text))
                        .child(
                            c.signer_name
                                .clone()
                                .unwrap_or_else(|| "Unknown signer".into()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(st.c(|c| c.muted))
                        .child(match (&c.when, page) {
                            (Some(w), Some(p)) => format!("{w} · {} · page {}", c.field, p + 1),
                            (Some(w), None) => format!("{w} · {}", c.field),
                            (None, Some(p)) => format!("{} · page {}", c.field, p + 1),
                            (None, None) => c.field.clone(),
                        }),
                );
            for extra in [&c.reason, &c.location].into_iter().flatten() {
                card = card.child(
                    div()
                        .text_size(px(11.5))
                        .text_color(st.c(|c| c.muted))
                        .child(extra.clone()),
                );
            }
            for line in findings(c) {
                card = card.child(
                    div()
                        .text_size(px(11.5))
                        .text_color(st.c(|c| c.faint))
                        .child(line),
                );
            }
            if let Some(s) = &c.signer {
                card = card.child(
                    div()
                        .text_size(px(10.5))
                        .text_color(st.c(|c| c.faint))
                        .child(s.clone()),
                );
            }
            let link = |id: &'static str, text: &'static str| {
                div()
                    .id((id, ix))
                    .text_size(px(11.5))
                    .text_color(st.c(|c| c.accent))
                    .cursor_pointer()
                    .hover(|d| d.underline())
                    .child(text)
            };
            card =
                card.child(
                    div()
                        .flex()
                        .gap(px(14.0))
                        .pt(px(2.0))
                        .when_some(page, |d, p| {
                            d.child(link("sig-go", "Go to page").on_click(move |_, _, cx| {
                                jump.update(cx, |r, cx| r.jump_to(p, cx))
                            }))
                        })
                        .when(status == SigStatus::Unknown, |d| {
                            d.child(link("sig-trust", "Trusted certificates…").on_click(
                                cx.listener(|ws, _, window, cx| {
                                    ws.open_settings_at(
                                        crate::settings_view::Section::Signing,
                                        window,
                                        cx,
                                    )
                                }),
                            ))
                        }),
                );
            col = col.child(card);
        }
        col.into_any_element()
    }
}
