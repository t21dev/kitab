// SPDX-License-Identifier: AGPL-3.0-or-later

//! Search in the sidebar's Contents and Bookmarks lists. One field serves
//! both. In Contents a match keeps the headings above it, opened, so it's
//! seen in its place; folding comes back as it was when the search is
//! cleared (Esc or emptying the field).

use super::{TocRow, Workspace};
use crate::icons::icon;
use crate::style::Style;
use crate::text_field::{FieldEvent, TextField};
use gpui::{
    AnyElement, AppContext, Context, Entity, IntoElement, ParentElement, Styled, Subscription,
    Window, div, px, svg,
};
use kitab_core::OutlineItem;

pub(super) struct SideSearch {
    pub field: Entity<TextField>,
    /// The field's text, lower-cased and trimmed.
    pub query: String,
    _sub: Option<Subscription>,
}

const PLACEHOLDER: &str = "Search this list";

impl SideSearch {
    pub fn new(cx: &mut gpui::App) -> Self {
        Self {
            field: cx.new(|cx| TextField::new("", false, PLACEHOLDER, cx)),
            query: String::new(),
            _sub: None,
        }
    }
}

impl Workspace {
    /// Wire the field (from `new`).
    pub(super) fn start_side_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let field = self.side_search.field.clone();
        let sub = cx.subscribe_in(&field, window, |ws, field, e: &FieldEvent, window, cx| {
            match e {
                FieldEvent::Changed => {
                    ws.side_search.query = field.read(cx).text().trim().to_lowercase();
                }
                FieldEvent::Cancel => ws.clear_side_search(window, cx),
                FieldEvent::Submit => {}
            }
            cx.notify();
        });
        self.side_search._sub = Some(sub);
    }

    /// Esc in the field: empty it (a fresh field, wired again).
    pub(super) fn clear_side_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.side_search.field = cx.new(|cx| TextField::new("", false, PLACEHOLDER, cx));
        self.side_search.query.clear();
        self.start_side_search(window, cx);
        cx.notify();
    }

    /// The search field above a list, then the list.
    pub(super) fn with_side_search(&self, list: AnyElement, st: &Style) -> AnyElement {
        let field = div()
            .flex_none()
            .mx(px(8.0))
            .mt(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .h(st.px(|m| m.control_height))
            .overflow_hidden()
            .rounded(st.px(|m| m.control_radius))
            .bg(st.c(|c| c.raised))
            .shadow(st.well())
            .border_1()
            .border_color(st.c(|c| c.line))
            .child(
                svg()
                    .path(icon("search"))
                    .size(px(13.0))
                    .flex_none()
                    .text_color(st.c(|c| c.faint)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(self.side_search.field.clone()),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(field)
            .child(div().flex_1().min_h_0().child(list))
            .into_any_element()
    }
}

/// Whether a title matches the (lower-cased) query.
pub(super) fn matches(title: &str, query: &str) -> bool {
    query.is_empty() || title.to_lowercase().contains(query)
}

/// The outline's entries that match `query`, each with the entries above it
/// (shown open), as rows for the contents list. Returns whether anything in
/// `items` matched.
pub(super) fn filter_outline(
    items: &[OutlineItem],
    query: &str,
    depth: usize,
    key: &mut Vec<u16>,
    out: &mut Vec<TocRow>,
) -> bool {
    let mut any = false;
    for (i, it) in items.iter().enumerate() {
        if out.len() >= 20_000 {
            break;
        }
        key.push(i.min(u16::MAX as usize) as u16);
        let at = out.len();
        let mut below = Vec::new();
        let kids = depth < 32 && filter_outline(&it.children, query, depth + 1, key, &mut below);
        if kids || matches(&it.title, query) {
            any = true;
            out.insert(
                at,
                TocRow {
                    depth,
                    item: OutlineItem {
                        children: Vec::new(),
                        ..it.clone()
                    },
                    key: key.clone(),
                    // Shown open, and not foldable while searching.
                    kids: false,
                    open: kids,
                },
            );
            out.extend(below);
        }
        key.pop();
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, children: Vec<OutlineItem>) -> OutlineItem {
        OutlineItem {
            title: title.into(),
            page: Some(0),
            uri: None,
            open: false,
            children,
        }
    }

    #[test]
    fn a_match_keeps_the_headings_above_it() {
        let outline = vec![
            item(
                "Part one",
                vec![item("Rivers", vec![]), item("Mountains", vec![])],
            ),
            item(
                "Part two",
                vec![item("Deserts", vec![item("Sahara rivers", vec![])])],
            ),
            item("Index", vec![]),
        ];
        let mut rows = Vec::new();
        assert!(filter_outline(
            &outline,
            "river",
            0,
            &mut Vec::new(),
            &mut rows
        ));
        let titles: Vec<_> = rows
            .iter()
            .map(|r| (r.depth, r.item.title.as_str()))
            .collect();
        assert_eq!(
            titles,
            vec![
                (0, "Part one"),
                (1, "Rivers"),
                (0, "Part two"),
                (1, "Deserts"),
                (2, "Sahara rivers"),
            ]
        );
        let mut none = Vec::new();
        assert!(!filter_outline(
            &outline,
            "ocean",
            0,
            &mut Vec::new(),
            &mut none
        ));
        assert!(none.is_empty());
    }
}
