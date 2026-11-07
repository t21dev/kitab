// SPDX-License-Identifier: AGPL-3.0-or-later

//! What sits on top of a page bitmap: links (fetched once the page has drawn)
//! and the page's characters for text selection (fetched when the pointer
//! first moves over the page). Both are kept for pages near the current one
//! only (AGENTS.md rule 5).

use super::Reader;
use super::geom::rect_to_view;
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, AppContext, Context, CursorStyle, InteractiveElement, IntoElement, ListOffset,
    SharedString, StatefulInteractiveElement, Styled, div, px,
};
use kitab_core::{Link, LinkTarget, TextChar};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// Pages kept either side of the current one when the maps are trimmed.
const KEEP: u32 = 12;
/// Trim once a map holds this many pages.
const TRIM_AT: usize = 40;

#[derive(Default)]
pub(super) struct Layers {
    pub links: HashMap<u32, Rc<Vec<Link>>>,
    pub links_pending: HashSet<u32>,
    pub chars: HashMap<u32, Rc<Vec<TextChar>>>,
    pub chars_pending: HashSet<u32>,
}

impl Layers {
    fn trim(&mut self, around: u32) {
        let near = |p: &u32| p.abs_diff(around) <= KEEP;
        if self.links.len() > TRIM_AT {
            self.links.retain(|p, _| near(p));
        }
        if self.chars.len() > TRIM_AT {
            self.chars.retain(|p, _| near(p));
        }
    }
}

/// Schemes a link may open. Anything else in a document (file:, javascript:,
/// custom handlers) is ignored (AGENTS.md rule 11).
pub(super) fn safe_uri(uri: &str) -> bool {
    let lower = uri.trim().to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|s| lower.starts_with(s))
}

impl Reader {
    /// Fetch page `ix`'s links once (after its bitmap is in).
    pub(super) fn ensure_links(&mut self, ix: u32, cx: &mut Context<Self>) {
        let Some(doc) = self.doc else { return };
        if self.layers.links.contains_key(&ix) || !self.layers.links_pending.insert(ix) {
            return;
        }
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let links = cx
                .background_spawn(async move { engine.links(doc, ix) })
                .await;
            let _ = this.update(cx, |r, cx| {
                r.layers.links_pending.remove(&ix);
                r.layers.links.insert(ix, Rc::new(links));
                r.layers.trim(r.current_page() as u32);
                cx.notify();
            });
        })
        .detach();
    }

    /// Fetch page `ix`'s characters once (the pointer is over it).
    pub(super) fn ensure_chars(&mut self, ix: u32, cx: &mut Context<Self>) {
        let Some(doc) = self.doc else { return };
        if self.layers.chars.contains_key(&ix) || !self.layers.chars_pending.insert(ix) {
            return;
        }
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let chars = cx
                .background_spawn(async move { engine.page_chars(doc, ix) })
                .await;
            let _ = this.update(cx, |r, cx| {
                r.layers.chars_pending.remove(&ix);
                r.layers.chars.insert(ix, Rc::new(chars));
                r.layers.trim(r.current_page() as u32);
                cx.notify();
            });
        })
        .detach();
    }

    /// Follow a link: a page in this document (remembered for Alt+Left), or a
    /// web or mail address in the system browser.
    pub(super) fn follow(&mut self, target: &LinkTarget, cx: &mut Context<Self>) {
        match target {
            LinkTarget::Page { page, y } => {
                let page = *page as usize;
                self.jump(page, cx);
                if let Some(y) = y {
                    let ppt = self.px_per_pt(page);
                    let size = self.raw_page_size(page);
                    let at = kitab_core::RectPt {
                        x: 0.0,
                        y: *y,
                        w: 0.0,
                        h: 0.0,
                    };
                    let view = rect_to_view(at, size, self.rotation, ppt);
                    self.list.scroll_to(ListOffset {
                        item_ix: page,
                        offset_in_item: px((self.sheet_top_pad(page) + view.y - 12.0).max(0.0)),
                    });
                }
            }
            LinkTarget::Uri(uri) if safe_uri(uri) => cx.open_url(uri),
            LinkTarget::Uri(_) => {}
        }
    }

    /// Clickable areas for page `ix`'s links.
    pub(super) fn link_marks(
        &self,
        ix: usize,
        ppt: f32,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(links) = self.layers.links.get(&(ix as u32)).cloned() else {
            return Vec::new();
        };
        let size = self.raw_page_size(ix);
        // The page's characters (loaded once the pointer is over it) tell a
        // footnote mark from a link to another page.
        let chars = self.layers.chars.get(&(ix as u32)).cloned();
        links
            .iter()
            .enumerate()
            .map(|(i, link)| {
                let r = rect_to_view(link.rect, size, self.rotation, ppt);
                let note = chars.as_ref().is_some_and(|c| {
                    kitab_core::notes::is_note_mark(&kitab_core::notes::text_in(c, link.rect))
                });
                let tip: SharedString = match &link.target {
                    LinkTarget::Page { .. } if note => "Show the note".into(),
                    LinkTarget::Page { page, .. } => format!("Go to page {}", page + 1).into(),
                    LinkTarget::Uri(u) if safe_uri(u) => u.clone().into(),
                    LinkTarget::Uri(_) => "Kitab doesn't open this kind of link".into(),
                };
                let link = link.clone();
                div()
                    .id(("link", ix * 10_000 + i))
                    .absolute()
                    .left(px(r.x))
                    .top(px(r.y))
                    .w(px(r.w))
                    .h(px(r.h))
                    .rounded(px(2.0))
                    .cursor(CursorStyle::PointingHand)
                    .hover(|d| d.bg(st.c(|c| c.accent).opacity(0.12)))
                    .tip(tip)
                    .on_click(cx.listener(move |r, e: &gpui::ClickEvent, _, cx| {
                        r.click_link(ix, link.clone(), e.position(), cx)
                    }))
                    .into_any_element()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::safe_uri;

    #[test]
    fn only_web_and_mail_links_open() {
        assert!(safe_uri("https://kitab.t21.dev"));
        assert!(safe_uri("HTTP://example.com"));
        assert!(safe_uri("mailto:someone@example.com"));
        assert!(!safe_uri("file:///C:/Windows/system32/calc.exe"));
        assert!(!safe_uri("javascript:alert(1)"));
        assert!(!safe_uri("ms-settings:"));
        assert!(!safe_uri("\\\\server\\share\\x.exe"));
    }
}
