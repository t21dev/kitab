// SPDX-License-Identifier: AGPL-3.0-or-later

//! The Pages pane: a virtual list of page thumbnails (only the visible ones
//! render), the current page ringed, a click to go there. Thumbnails share the
//! reader's page cache, which is bounded in bytes, so they never grow
//! without limit, and a thumbnail doubles as the blurry stand-in while a big
//! page renders.

use super::*;
use gpui::{UniformListScrollHandle, uniform_list};

/// Thumbnail width in logical pixels.
pub const THUMB_W: f32 = 132.0;
/// Row height: the tallest thumbnail (a portrait page) plus its number.
const ROW_H: f32 = THUMB_W * 1.45 + 30.0;

pub(super) struct PagesState {
    pub scroll: UniformListScrollHandle,
    /// The rows on screen: low 32 bits first, high 32 bits last (renders
    /// for thumbnails scrolled away are skipped).
    pub visible: Arc<std::sync::atomic::AtomicU64>,
    /// Scroll to the current page when the pane opens.
    pub reveal: bool,
}

impl Default for PagesState {
    fn default() -> Self {
        Self {
            scroll: UniformListScrollHandle::new(),
            visible: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            reveal: true,
        }
    }
}

impl Reader {
    /// The thumbnail for page `ix`, asking for it if the cache hasn't got it.
    fn thumbnail(
        &mut self,
        ix: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<RenderImage>> {
        let size = self.page_size(ix);
        let scale = THUMB_W * window.scale_factor() / size.w.max(1.0);
        let key = PageKey {
            page: ix as u32,
            scale_key: scale_key(scale),
            rotation: self.rotation,
        };
        if let Some(image) = self.cache.get(&key) {
            return Some(image);
        }
        let visible = self.pages.visible.clone();
        let page = ix as u32;
        let wanted: crate::engine::Wanted = Arc::new(move || {
            let v = visible.load(std::sync::atomic::Ordering::Relaxed);
            let (lo, hi) = ((v & 0xFFFF_FFFF) as u32, (v >> 32) as u32);
            page + 2 >= lo && page <= hi + 2
        });
        self.request_for(ix, key, scale, RenderFor::Thumbnail(wanted), cx);
        // A sharper render of the page is fine as a stand-in meanwhile.
        self.cache.any_for_page(page, self.rotation)
    }

    /// The Pages pane for the sidebar.
    pub fn pages_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let count = self.page_count();
        if self.pages.reveal && count > 0 {
            self.pages
                .scroll
                .scroll_to_item(self.current_page(), gpui::ScrollStrategy::Center);
            self.pages.reveal = false;
        }
        uniform_list(
            "pages-pane",
            count,
            cx.processor(|r, range: std::ops::Range<usize>, window, cx| {
                let lo = range.start as u64;
                let hi = range.end.saturating_sub(1) as u64;
                r.pages
                    .visible
                    .store(lo | (hi << 32), std::sync::atomic::Ordering::Relaxed);
                let st = Style::get(cx).clone();
                let current = r.current_page();
                range
                    .map(|ix| r.thumb_row(ix, ix == current, &st, window, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(self.pages.scroll.clone())
        .size_full()
        .into_any_element()
    }

    fn thumb_row(
        &mut self,
        ix: usize,
        on: bool,
        st: &Style,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let size = self.page_size(ix);
        let h = THUMB_W * size.h / size.w.max(1.0);
        let image = self.thumbnail(ix, window, cx);
        div()
            .id(("thumb", ix))
            .h(px(ROW_H))
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .on_click(cx.listener(move |r, _, _, cx| r.jump(ix, cx)))
            .child(
                div()
                    .w(px(THUMB_W))
                    .h(px(h.min(THUMB_W * 1.45)))
                    .bg(gpui::white())
                    .rounded(px(3.0))
                    .overflow_hidden()
                    .border_2()
                    .border_color(if on {
                        st.c(|c| c.accent)
                    } else {
                        st.c(|c| c.line)
                    })
                    .shadow(st.elev())
                    .when(self.is_bookmarked(ix as u32), |d| {
                        d.relative().child(
                            div()
                                .absolute()
                                .top_0()
                                .right(px(8.0))
                                .w(px(8.0))
                                .h(px(13.0))
                                .bg(st.c(|c| c.accent)),
                        )
                    })
                    .children(image.map(|i| img(ImageSource::Render(i)).size_full())),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(if on {
                        st.c(|c| c.text)
                    } else {
                        st.c(|c| c.faint)
                    })
                    .child(format!("{}", ix + 1)),
            )
            .into_any_element()
    }
}
