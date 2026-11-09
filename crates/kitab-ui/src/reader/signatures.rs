// SPDX-License-Identifier: AGPL-3.0-or-later

//! Signatures in the reader (PLAN.md §3.7): verified off the UI thread when
//! a PDF opens, summed up in the status bar, listed in the sidebar's
//! Signatures pane. Verifying never changes the file and never goes online.

use super::{Reader, ReaderEvent};
use crate::icons::icon;
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, AppContext, Context, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, svg,
};
use kitab_core::{SigStatus, SignatureCheck, overall_status};

/// A signature's status in a few words, naming what changed after signing.
pub fn check_words(c: &SignatureCheck, st: &Style) -> (&'static str, Hsla) {
    match (c.status(), c.modification) {
        (SigStatus::Changed, kitab_core::SigModification::Annotations) => {
            ("Comments added after signing", st.c(|c| c.warn))
        }
        (s, _) => status_words(s, st),
    }
}

/// A status in a few words, and its colour.
pub fn status_words(s: SigStatus, st: &Style) -> (&'static str, Hsla) {
    match s {
        SigStatus::Valid => ("Valid", st.c(|c| c.ok)),
        SigStatus::Unknown => ("Signer not trusted", st.c(|c| c.warn)),
        SigStatus::Changed => ("Changed after signing", st.c(|c| c.warn)),
        SigStatus::Broken => ("Broken", st.c(|c| c.bad)),
    }
}

impl Reader {
    /// Verify the PDF's signatures in the background (again, after the
    /// trusted certificates changed).
    pub fn check_signatures(&mut self, cx: &mut Context<Self>) {
        if !self.is_pdf() {
            return;
        }
        let (engine, path, unlock) = (self.engine.clone(), self.path.clone(), self.unlock());
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move { engine.signatures(&path, unlock.as_deref()) })
                .await;
            let _ = this.update(cx, |r, cx| {
                let found = found.unwrap_or_else(|e| {
                    log::warn!("signatures: {e}");
                    Vec::new()
                });
                // Unsigned (most PDFs) and unchanged: no redraw.
                let changed =
                    r.signatures.as_ref().is_some_and(|s| !s.is_empty()) || !found.is_empty();
                r.signatures = Some(found);
                if changed {
                    cx.emit(ReaderEvent::Signatures);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The verified signatures; `None` until checked (or not a PDF).
    pub fn signatures(&self) -> Option<&[SignatureCheck]> {
        self.signatures.as_deref()
    }

    /// The status bar's badge: the document's worst signature status.
    pub(super) fn signature_badge(&self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sigs = self.signatures.as_deref().filter(|s| !s.is_empty())?;
        let status = overall_status(sigs)?;
        let (words, colour) = status_words(status, st);
        let label = if sigs.len() == 1 {
            match (status, sigs[0].signer_name.as_deref()) {
                (SigStatus::Valid, Some(who)) => format!("Signed by {who}"),
                _ => format!("Signature: {}", words.to_lowercase()),
            }
        } else if status == SigStatus::Valid {
            format!("{} signatures, all valid", sigs.len())
        } else {
            format!("{} signatures: {}", sigs.len(), words.to_lowercase())
        };
        Some(
            div()
                .id("status-signature")
                .flex()
                // A long signer name ends in "…", not past the page number.
                .flex_shrink()
                .min_w_0()
                .max_w(px(280.0))
                .items_center()
                .gap(px(5.0))
                .px(px(6.0))
                .h(px(20.0))
                .rounded(px(5.0))
                .cursor_pointer()
                .text_color(colour)
                .hover(|d| d.bg(st.hover_bg()))
                .tip("Show signatures")
                .on_click(cx.listener(|_, _, _, cx| cx.emit(ReaderEvent::ShowSignatures)))
                .child(
                    svg()
                        .path(icon("shield"))
                        .flex_none()
                        .size(px(12.0))
                        .text_color(colour),
                )
                .child(div().min_w_0().truncate().child(label))
                .into_any_element(),
        )
    }
}
