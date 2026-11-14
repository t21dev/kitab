// SPDX-License-Identifier: AGPL-3.0-or-later

//! Home's library search and reading stats (PLAN.md Phase 9). Search runs
//! over the page text of every document Kitab has opened (indexed in the
//! background); a result opens its document at that page. Reading time is
//! counted while you're active in a document and shown as today, this
//! week, the streak and a four-week strip. All of it stays in Kitab's data.

use super::Workspace;
use crate::icons::icon;
use crate::library::{LibraryHit, StatsView};
use crate::style::Style;
use crate::text_field::{FieldEvent, TextField};
use gpui::{
    AnyElement, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Subscription, Window, div, px, svg,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// How often reading time is counted, and how recent the last input must
/// be for that time to count as reading.
const TICK: Duration = Duration::from_secs(30);
const IDLE_AFTER: Duration = Duration::from_secs(120);

pub(super) struct LibraryState {
    pub search: Entity<TextField>,
    pub query: String,
    pub hits: Option<Vec<LibraryHit>>,
    pub serial: u64,
    pub stats: Option<StatsView>,
    pub last_input: Instant,
    _sub: Option<Subscription>,
}

impl LibraryState {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        let search = cx.new(|cx| TextField::new("", false, "Search your library", cx));
        Self {
            search,
            query: String::new(),
            hits: None,
            serial: 0,
            stats: None,
            last_input: Instant::now(),
            _sub: None,
        }
    }
}

/// "1 h 05 min", "12 min", "under a minute".
pub fn duration_words(secs: i64) -> String {
    let mins = secs / 60;
    match mins {
        0 => "under a minute".into(),
        m if m < 60 => format!("{m} min"),
        m => format!("{} h {:02} min", m / 60, m % 60),
    }
}

impl Workspace {
    /// Wire the search field and start the reading clock (from `new`).
    pub(super) fn start_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sub = cx.subscribe_in(
            &self.lib.search.clone(),
            window,
            |ws, _, e: &FieldEvent, window, cx| match e {
                FieldEvent::Changed => ws.search_changed(cx),
                FieldEvent::Submit => {
                    if let Some(hit) = ws.lib.hits.as_ref().and_then(|h| h.first()).cloned() {
                        ws.open_hit(hit, window, cx);
                    }
                }
                FieldEvent::Cancel => ws.clear_search(cx),
            },
        );
        self.lib._sub = Some(sub);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |ws, cx| ws.tick_reading(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Any key or pointer movement: the reader is here.
    pub(super) fn note_input(&mut self) {
        self.lib.last_input = Instant::now();
    }

    /// Every TICK: count it as reading the active document if there was
    /// input lately.
    fn tick_reading(&mut self, cx: &mut Context<Self>) {
        if self.lib.last_input.elapsed() > IDLE_AFTER {
            return;
        }
        let Some(path) = self.active_reader().map(|r| r.read(cx).path.clone()) else {
            return;
        };
        let library = self.library.clone();
        cx.background_spawn(async move {
            let today = kitab_os::local_today();
            library.add_reading(&path, &today, TICK.as_secs() as i64);
        })
        .detach();
    }

    pub(super) fn refresh_stats(&mut self, cx: &mut Context<Self>) {
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let stats = cx
                .background_spawn(async move { library.reading_stats(&kitab_os::local_today()) })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.lib.stats = stats;
                cx.notify();
            });
        })
        .detach();
    }

    fn search_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.lib.search.read(cx).text();
        if query.trim() == self.lib.query.trim() {
            return;
        }
        self.lib.query = query.clone();
        self.lib.serial += 1;
        let serial = self.lib.serial;
        if query.trim().is_empty() {
            self.lib.hits = None;
            cx.notify();
            return;
        }
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            // Wait for a pause in typing.
            cx.background_executor()
                .timer(Duration::from_millis(180))
                .await;
            let stale = this
                .update(cx, |ws, _| ws.lib.serial != serial)
                .unwrap_or(true);
            if stale {
                return;
            }
            let hits = cx
                .background_spawn(async move { library.search(&query, 60) })
                .await;
            let _ = this.update(cx, |ws, cx| {
                if ws.lib.serial == serial {
                    ws.lib.hits = Some(hits);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.lib.search = cx.new(|cx| TextField::new("", false, "Search your library", cx));
        self.lib.query.clear();
        self.lib.hits = None;
        self.lib._sub = None;
        cx.notify();
    }

    fn open_hit(&mut self, hit: LibraryHit, window: &mut Window, cx: &mut Context<Self>) {
        match hit.page {
            Some(page) => self.open_tab_at(hit.path, page, window, cx),
            None => self.open_tab(hit.path, window, cx),
        }
    }

    /// Open `path` (or switch to it) and show `page`.
    pub(super) fn open_tab_at(
        &mut self,
        path: PathBuf,
        page: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_tab(path.clone(), window, cx);
        if let Some(r) = self.tabs.iter().find(|t| t.read(cx).path == path).cloned() {
            r.update(cx, |r, cx| r.show_page(page, cx));
        }
    }

    /// The search field for Home's header.
    pub(super) fn search_field(&self, st: &Style) -> AnyElement {
        // Shrinks before the buttons beside it do, in a narrow window or at
        // a large interface size.
        div()
            .flex_1()
            .min_w(px(120.0))
            .max_w(px(300.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            // The same height as the Settings and Open file… buttons beside it.
            .h(st.px(|m| m.control_height))
            .overflow_hidden()
            .rounded(st.px(|m| m.control_radius))
            .bg(st.c(|c| c.raised))
            .shadow(st.well())
            // Visible on light themes, where raised is the page's own white.
            .border_1()
            .border_color(st.c(|c| c.line))
            .child(
                svg()
                    .path(icon("search"))
                    .size(px(14.0))
                    .flex_none()
                    .text_color(st.c(|c| c.faint)),
            )
            .child(div().flex_1().min_w_0().child(self.lib.search.clone()))
            .into_any_element()
    }

    /// Search results, in place of Home's lists while there's a query.
    pub(super) fn search_results(&mut self, st: &Style, cx: &mut Context<Self>) -> AnyElement {
        let mut col = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(40.0))
            .pt(px(16.0))
            .pb(px(40.0));
        let indexed = self.lib.stats.as_ref().map_or(0, |s| s.indexed);
        match self.lib.hits.clone() {
            None => {
                col = col.child(
                    div()
                        .text_size(st.px(|m| m.ui_text))
                        .text_color(st.c(|c| c.faint))
                        .child("Searching…"),
                )
            }
            Some(hits) if hits.is_empty() => {
                col = col.child(
                    div()
                        .text_size(st.px(|m| m.ui_text))
                        .text_color(st.c(|c| c.faint))
                        .child(format!(
                            "Nothing found in {indexed} documents. Kitab searches the documents you've opened, a few seconds after each opens."
                        )),
                )
            }
            Some(hits) => {
                col = col.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(st.c(|c| c.faint))
                        .child(if hits.len() == 1 {
                            "1 page".to_string()
                        } else {
                            format!("{} pages", hits.len())
                        }),
                );
                for (i, hit) in hits.into_iter().enumerate() {
                    col = col.child(self.hit_row(i, hit, st, cx));
                }
            }
        }
        col.into_any_element()
    }

    fn hit_row(
        &mut self,
        i: usize,
        hit: LibraryHit,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // The snippet with the matched words in the text colour.
        let mut line = div()
            .flex()
            .flex_wrap()
            .text_size(px(12.5))
            .text_color(st.c(|c| c.muted));
        for (j, part) in hit.snippet.split(['[', ']']).enumerate() {
            if part.is_empty() {
                continue;
            }
            let piece = div().child(part.replace('\n', " ").to_string());
            line = line.child(if j % 2 == 1 {
                piece
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(st.c(|c| c.text))
            } else {
                piece
            });
        }
        let title = hit.title.clone();
        let page = hit.page;
        div()
            .id(("hit", i))
            .flex()
            .flex_col()
            .gap(px(3.0))
            .px(px(14.0))
            .py(px(10.0))
            .rounded(px(10.0))
            .cursor_pointer()
            .hover(|d| d.bg(st.hover_bg()))
            .on_click(cx.listener(move |ws, _, window, cx| ws.open_hit(hit.clone(), window, cx)))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .items_baseline()
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(st.c(|c| c.text))
                            .truncate()
                            .child(title),
                    )
                    .children(page.map(|page| {
                        div()
                            .flex_none()
                            .text_size(px(11.5))
                            .text_color(st.c(|c| c.faint))
                            .child(format!("page {}", page + 1))
                    })),
            )
            .child(line)
            .into_any_element()
    }

    /// The reading strip on Home: today, the week, the streak, and a bar
    /// per day for four weeks. Hidden until there's some reading.
    pub(super) fn stats_strip(&self, st: &Style) -> Option<AnyElement> {
        let s = self.lib.stats.as_ref()?;
        if s.recent_days.iter().all(|d| *d == 0) && s.longest_streak == 0 {
            return None;
        }
        let tile = |name: &'static str, label: &'static str, value: String| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .text_size(px(11.5))
                        .text_color(st.c(|c| c.faint))
                        .child(
                            gpui::svg()
                                .path(crate::icons::icon(name))
                                .size(px(13.0))
                                .text_color(st.c(|c| c.accent)),
                        )
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(17.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(st.c(|c| c.text))
                        .child(value),
                )
        };
        let days = |n: u32| {
            if n == 1 {
                "1 day".to_string()
            } else {
                format!("{n} days")
            }
        };
        let most = s.recent_days.iter().copied().max().unwrap_or(0).max(60);
        let mut bars = div().flex().items_end().gap(px(3.0)).h(px(40.0));
        for (i, secs) in s.recent_days.iter().enumerate() {
            let h = if *secs <= 0 {
                2.0
            } else {
                (*secs as f32 / most as f32 * 40.0).max(4.0)
            };
            let today = i + 1 == s.recent_days.len();
            bars = bars.child(div().w(px(7.0)).h(px(h)).rounded(px(2.0)).bg(if *secs > 0 {
                if today {
                    st.c(|c| c.accent)
                } else {
                    st.c(|c| c.ring)
                }
            } else {
                st.c(|c| c.line)
            }));
        }
        Some(
            div()
                .mx(px(40.0))
                .mt(px(12.0))
                .p(px(18.0))
                .flex()
                .flex_wrap()
                .items_end()
                .gap(px(32.0))
                .rounded(px(12.0))
                .bg(st.c(|c| c.raised))
                .shadow(st.elev())
                .child(tile("clock", "Today", duration_words(s.today_seconds)))
                .child(tile(
                    "calendar",
                    "This week",
                    duration_words(s.week_seconds),
                ))
                .child(tile("flame", "Streak", days(s.streak_days)))
                .child(tile("trophy", "Longest", days(s.longest_streak)))
                .child(div().flex_1())
                .child(
                    div().flex().flex_col().gap(px(6.0)).child(bars).child(
                        div()
                            .text_size(px(11.0))
                            .text_color(st.c(|c| c.faint))
                            .child("Last 4 weeks"),
                    ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_plainly() {
        assert_eq!(duration_words(30), "under a minute");
        assert_eq!(duration_words(720), "12 min");
        assert_eq!(duration_words(3900), "1 h 05 min");
    }
}
