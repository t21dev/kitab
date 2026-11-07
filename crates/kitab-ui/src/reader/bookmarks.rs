// SPDX-License-Identifier: AGPL-3.0-or-later

//! Bookmarks (Ctrl+D): named pages, kept in the library. A bookmark is named
//! after the chapter its page is in. Books that re-flow have no fixed pages,
//! so they don't take bookmarks yet.

use super::*;

/// One bookmark: library id (once kept), page, name.
#[derive(Debug, Clone, PartialEq)]
pub struct BookmarkRow {
    pub id: Option<i64>,
    pub page: u32,
    pub title: String,
}

/// The name for a bookmark on `page`: the last contents entry at or before it.
pub(super) fn title_for(page: u32, outline: &[OutlineItem]) -> String {
    fn walk<'a>(items: &'a [OutlineItem], page: u32, best: &mut Option<(u32, &'a str)>) {
        for it in items {
            if let Some(p) = it.page.filter(|&p| p <= page) {
                if best.is_none_or(|(b, _)| p >= b) {
                    *best = Some((p, &it.title));
                }
            }
            walk(&it.children, page, best);
        }
    }
    let mut best = None;
    walk(outline, page, &mut best);
    match best {
        Some((_, title)) => format!("{title} · page {}", page + 1),
        None => format!("Page {}", page + 1),
    }
}

impl Reader {
    /// Bookmarks are possible here (fixed pages or Markdown blocks).
    pub fn can_bookmark(&self) -> bool {
        self.status == Status::Open && !self.is_book()
    }

    pub fn bookmarks(&self) -> &[BookmarkRow] {
        &self.bookmark_rows
    }

    pub fn is_bookmarked(&self, page: u32) -> bool {
        self.bookmark_rows.iter().any(|b| b.page == page)
    }

    /// Load the bookmarks the library keeps (once, after opening).
    pub(super) fn load_bookmarks(&mut self, cx: &mut Context<Self>) {
        let (library, path) = (self.library.clone(), self.path.clone());
        cx.spawn(async move |this, cx| {
            let rows = cx
                .background_spawn(async move { library.bookmarks(&path) })
                .await;
            // Nothing saved: nothing to redraw.
            if rows.is_empty() {
                return;
            }
            let _ = this.update(cx, |r, cx| {
                let pages = r.page_count() as u32;
                r.bookmark_rows = rows
                    .into_iter()
                    .filter(|(_, page, _)| *page < pages)
                    .map(|(id, page, title)| BookmarkRow {
                        id: Some(id),
                        page,
                        title,
                    })
                    .collect();
                r.bookmark_rows.sort_by_key(|b| b.page);
                cx.notify();
            });
        })
        .detach();
    }

    /// Ctrl+D: bookmark the current page, or remove its bookmark.
    pub fn toggle_bookmark(&mut self, cx: &mut Context<Self>) {
        if !self.can_bookmark() {
            return;
        }
        let page = self.current_page() as u32;
        if let Some(at) = self.bookmark_rows.iter().position(|b| b.page == page) {
            let row = self.bookmark_rows.remove(at);
            if let Some(id) = row.id {
                let library = self.library.clone();
                cx.background_spawn(async move { library.remove_bookmark(id) })
                    .detach();
            }
            cx.notify();
            return;
        }
        let title = title_for(page, self.outline());
        self.bookmark_rows.push(BookmarkRow {
            id: None,
            page,
            title: title.clone(),
        });
        self.bookmark_rows.sort_by_key(|b| b.page);
        let (library, path) = (self.library.clone(), self.path.clone());
        cx.spawn(async move |this, cx| {
            let id = cx
                .background_spawn(async move { library.add_bookmark(&path, page, &title) })
                .await;
            let _ = this.update(cx, |r, _| {
                if let Some(row) = r
                    .bookmark_rows
                    .iter_mut()
                    .find(|b| b.page == page && b.id.is_none())
                {
                    row.id = id;
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Remove one bookmark (the pane's ×).
    pub fn remove_bookmark(&mut self, page: u32, cx: &mut Context<Self>) {
        let current = self.current_page();
        // toggle_bookmark works on the current page; go there quietly first.
        if let Some(at) = self.bookmark_rows.iter().position(|b| b.page == page) {
            let row = self.bookmark_rows.remove(at);
            if let Some(id) = row.id {
                let library = self.library.clone();
                cx.background_spawn(async move { library.remove_bookmark(id) })
                    .detach();
            }
        }
        let _ = current;
        cx.notify();
    }

    /// The ribbon on a bookmarked page's top-right corner.
    pub(super) fn bookmark_ribbon(&self, ix: usize, st: &Style) -> Option<AnyElement> {
        self.is_bookmarked(ix as u32).then(|| {
            div()
                .absolute()
                .top_0()
                .right(px(24.0))
                .w(px(14.0))
                .h(px(22.0))
                .bg(st.c(|c| c.accent))
                .rounded_b(px(2.0))
                .into_any_element()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, page: u32, children: Vec<OutlineItem>) -> OutlineItem {
        OutlineItem {
            title: title.into(),
            page: Some(page),
            uri: None,
            open: false,
            children,
        }
    }

    #[test]
    fn bookmarks_are_named_after_their_chapter() {
        let outline = vec![
            item("Chapter 1", 0, vec![item("Section 1.1", 2, vec![])]),
            item("Chapter 2", 5, vec![]),
        ];
        assert_eq!(title_for(3, &outline), "Section 1.1 · page 4");
        assert_eq!(title_for(7, &outline), "Chapter 2 · page 8");
        assert_eq!(title_for(0, &[]), "Page 1");
    }
}
