// SPDX-License-Identifier: AGPL-3.0-or-later

//! The window: title bar with Home and tabs, header row, contents sidebar and the
//! reader (DESIGN.md § Components, "Docked" shell, D10).

use crate::actions::*;
use crate::engine::DocEngine;
use crate::icons::icon;
use crate::library::{Library, RecentItem, SessionTab};
use crate::reader::{Reader, ReaderEvent, open_reader};
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, ExternalPaths, FocusHandle, Focusable,
    FontWeight, InteractiveElement, IntoElement, ParentElement, PathPromptOptions, Render,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Window, WindowControlArea, div,
    prelude::FluentBuilder, px, svg,
};
use kitab_core::{OutlineItem, ZoomMode};
use std::path::PathBuf;
use std::sync::Arc;

pub struct Workspace {
    engine: Arc<dyn DocEngine>,
    library: Arc<dyn Library>,
    recent: Vec<RecentItem>,
    tabs: Vec<Entity<Reader>>,
    /// `None` shows Home.
    active: Option<usize>,
    sidebar: bool,
    focus: FocusHandle,
    subs: Vec<Subscription>,
}

impl Workspace {
    pub fn new(
        engine: Arc<dyn DocEngine>,
        library: Arc<dyn Library>,
        files: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let mut this = Self {
            engine,
            library,
            recent: Vec::new(),
            tabs: Vec::new(),
            active: None,
            sidebar: true,
            focus,
            subs: Vec::new(),
        };
        if files.is_empty() {
            // Start-up mode "Restore last session" (PLAN.md §4, the default): the
            // previous tabs, read off the UI thread.
            this.restore_session(window, cx);
        } else {
            for f in files {
                this.open_tab(f, window, cx);
            }
        }
        if this.tabs.is_empty() {
            window.focus(&this.focus);
        }
        this.refresh_recent(cx);
        this
    }

    fn restore_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let library = self.library.clone();
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let tabs = cx
                .background_spawn(async move { library.last_session() })
                .await;
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |ws, cx| {
                    let mut active = None;
                    for t in tabs.iter().filter(|t| t.path.exists()) {
                        ws.open_tab(t.path.clone(), window, cx);
                        if t.active {
                            active = Some(ws.tabs.len() - 1);
                        }
                    }
                    if let Some(i) = active {
                        ws.activate(i, window, cx);
                    }
                });
            });
        })
        .detach();
    }

    fn refresh_recent(&mut self, cx: &mut Context<Self>) {
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let recent = cx.background_spawn(async move { library.recent(24) }).await;
            let _ = this.update(cx, |ws, cx| {
                ws.recent = recent;
                cx.notify();
            });
        })
        .detach();
    }

    fn save_session(&mut self, cx: &mut Context<Self>) {
        let tabs: Vec<SessionTab> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| SessionTab {
                path: t.read(cx).path.clone(),
                active: self.active == Some(i),
            })
            .collect();
        let library = self.library.clone();
        cx.background_spawn(async move { library.save_session(tabs) })
            .detach();
    }

    /// Save every tab's place (on quit).
    pub fn save_all(&mut self, cx: &mut Context<Self>) {
        for t in &self.tabs {
            t.update(cx, |r, cx| r.save_now(cx));
        }
        self.save_session(cx);
    }

    pub fn open_tab(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        // Already open: switch to it.
        if let Some(ix) = self.tabs.iter().position(|t| t.read(cx).path == path) {
            self.activate(ix, window, cx);
            return;
        }
        let reader = open_reader(path, self.engine.clone(), self.library.clone(), window, cx);
        self.subs
            .push(cx.subscribe(&reader, |this, _, e: &ReaderEvent, cx| {
                match e {
                    // The library knows the document only once it has opened; save again.
                    ReaderEvent::Opened => {
                        this.save_session(cx);
                        cx.notify();
                    }
                    ReaderEvent::PageChanged => cx.notify(),
                }
            }));
        self.tabs.push(reader);
        let ix = self.tabs.len() - 1;
        self.activate(ix, window, cx);
    }

    fn activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(t) = self.tabs.get(ix).cloned() {
            self.active = Some(ix);
            self.save_session(cx);
            let title = t.read(cx).title();
            window.set_window_title(&format!("{title} · Kitab"));
            window.focus(&t.focus_handle(cx));
        }
        cx.notify();
    }

    fn show_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active = None;
        self.refresh_recent(cx);
        self.save_session(cx);
        window.set_window_title("Kitab");
        window.focus(&self.focus);
        cx.notify();
    }

    fn close_tab(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if ix >= self.tabs.len() {
            return;
        }
        let closing = self.tabs.remove(ix);
        closing.update(cx, |r, cx| r.save_now(cx));
        if self.tabs.is_empty() {
            self.show_home(window, cx);
        } else {
            let next = ix.min(self.tabs.len() - 1);
            self.activate(next, window, cx);
        }
    }

    fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = handle.update(cx, |_, window, cx| {
                    let _ = this.update(cx, |ws, cx| {
                        for p in paths {
                            ws.open_tab(p, window, cx);
                        }
                    });
                });
            }
        })
        .detach();
    }

    fn cycle_theme(&mut self, cx: &mut Context<Self>) {
        let ids = kitab_theme::BUILTIN_IDS;
        let current = Style::get(cx).theme.id.clone();
        let next = ids
            .iter()
            .position(|id| *id == current)
            .map_or(0, |i| (i + 1) % ids.len());
        if let Ok(theme) = kitab_theme::builtin(ids[next]) {
            cx.global_mut::<Style>().theme = theme;
            cx.refresh_windows();
        }
    }

    fn active_reader(&self) -> Option<&Entity<Reader>> {
        self.active.and_then(|i| self.tabs.get(i))
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

// ── pieces ───────────────────────────────────────────────────────────────

fn icon_button(
    id: impl Into<SharedString>,
    name: &str,
    on: bool,
    st: &Style,
) -> gpui::Stateful<gpui::Div> {
    let size = st.px(|m| m.icon_button);
    div()
        .id(id.into())
        .size(size)
        .flex()
        .items_center()
        .justify_center()
        .rounded(st.px(|m| m.control_radius))
        .text_color(if on {
            st.c(|c| c.accent)
        } else {
            st.c(|c| c.muted)
        })
        .when(on, |d| d.bg(st.c(|c| c.raised)).shadow(st.elev()))
        .hover(|d| d.bg(st.c(|c| c.hover)).text_color(st.c(|c| c.text)))
        .active(|d| d.bg(st.c(|c| c.hover)))
        .child(
            svg()
                .path(icon(name))
                .size(st.px(|m| m.icon_size))
                .text_color(if on {
                    st.c(|c| c.accent)
                } else {
                    st.c(|c| c.muted)
                }),
        )
}

fn window_button(
    id: &'static str,
    name: &str,
    area: WindowControlArea,
    st: &Style,
    danger: bool,
) -> AnyElement {
    let hover_bg = if danger {
        st.ca(|c| c.bad, 0.9)
    } else {
        st.c(|c| c.hover)
    };
    div()
        .id(id)
        .w(px(46.0))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .window_control_area(area)
        .text_color(st.c(|c| c.muted))
        .hover(move |d| d.bg(hover_bg))
        .child(
            svg()
                .path(icon(name))
                .size(px(14.0))
                .text_color(st.c(|c| c.muted)),
        )
        .into_any_element()
}

/// Flatten the outline with depth, for the sidebar.
fn flatten(items: &[OutlineItem], depth: usize, out: &mut Vec<(usize, OutlineItem)>) {
    for it in items {
        out.push((
            depth,
            OutlineItem {
                children: Vec::new(),
                ..it.clone()
            },
        ));
        if it.open || depth == 0 {
            flatten(&it.children, depth + 1, out);
        }
    }
}

impl Workspace {
    fn title_bar(&mut self, st: &Style, cx: &mut Context<Self>) -> AnyElement {
        let home_on = self.active.is_none();
        let mut bar = div()
            .flex()
            .flex_none()
            .items_center()
            .h(st.px(|m| m.title_bar))
            .pl(px(8.0))
            .gap(px(4.0))
            .bg(st.c(|c| c.bg))
            .border_b_1()
            .border_color(st.c(|c| c.line))
            .child(
                icon_button("home", "home", home_on, st)
                    .tip("Home  Ctrl+Shift+H")
                    .on_click(cx.listener(|ws, _, window, cx| ws.show_home(window, cx))),
            )
            .child(
                div()
                    .w(px(1.0))
                    .h(px(18.0))
                    .mx(px(4.0))
                    .bg(st.c(|c| c.line)),
            );

        for (ix, tab) in self.tabs.iter().enumerate() {
            let on = self.active == Some(ix);
            let r = tab.read(cx);
            let title = r.title();
            let tab_icon = if r
                .path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
            {
                "markdown"
            } else {
                "file"
            };
            bar = bar.child(
                div()
                    .id(("tab", ix))
                    .group("tab")
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .h(st.px(|m| m.control_height))
                    .pl(px(10.0))
                    .pr(px(6.0))
                    .max_w(px(220.0))
                    .rounded(st.px(|m| m.control_radius))
                    .text_size(st.px(|m| m.ui_text))
                    .text_color(if on {
                        st.c(|c| c.text)
                    } else {
                        st.c(|c| c.muted)
                    })
                    .when(on, |d| d.bg(st.c(|c| c.raised)).shadow(st.elev()))
                    .hover(|d| d.text_color(st.c(|c| c.text)))
                    .on_click(cx.listener(move |ws, _, window, cx| ws.activate(ix, window, cx)))
                    .child(svg().path(icon(tab_icon)).size(px(14.0)).text_color(if on {
                        st.c(|c| c.accent)
                    } else {
                        st.c(|c| c.faint)
                    }))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(title),
                    )
                    .child(
                        div()
                            .id(("close", ix))
                            .size(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .invisible()
                            .when(on, |d| d.visible())
                            .group_hover("tab", |d| d.visible())
                            .hover(|d| d.bg(st.c(|c| c.hover)))
                            .on_click(cx.listener(move |ws, _, window, cx| {
                                cx.stop_propagation();
                                ws.close_tab(ix, window, cx)
                            }))
                            .child(
                                svg()
                                    .path(icon("close"))
                                    .size(px(11.0))
                                    .text_color(st.c(|c| c.faint)),
                            ),
                    ),
            );
        }

        bar.child(
            div()
                .id("drag")
                .flex_1()
                .h_full()
                .window_control_area(WindowControlArea::Drag),
        )
        .child(window_button(
            "win-min",
            "win-min",
            WindowControlArea::Min,
            st,
            false,
        ))
        .child(window_button(
            "win-max",
            "win-max",
            WindowControlArea::Max,
            st,
            false,
        ))
        .child(window_button(
            "win-close",
            "close",
            WindowControlArea::Close,
            st,
            true,
        ))
        .into_any_element()
    }

    fn header_row(
        &mut self,
        reader: &Entity<Reader>,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let r = reader.read(cx);
        let title = r.title();
        let author = r.author();
        let percent = r.zoom_percent();
        let mode = r.zoom_mode();
        let open = r.status == crate::reader::Status::Open;
        let crumb = match author {
            Some(a) => format!("{title} · {a}"),
            None => title.to_string(),
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.0))
            .h(st.px(|m| m.header_row))
            .px(px(12.0))
            .bg(st.c(|c| c.bg))
            .border_b_1()
            .border_color(st.c(|c| c.line))
            .child(
                icon_button("sidebar", "sidebar", self.sidebar, st)
                    .tip("Contents  Ctrl+B")
                    .on_click(cx.listener(|ws, _, _, cx| {
                        ws.sidebar = !ws.sidebar;
                        cx.notify()
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .ml(px(4.0))
                    .text_size(st.px(|m| m.ui_text))
                    .overflow_hidden()
                    .child(div().text_color(st.c(|c| c.faint)).child("Library"))
                    .child(div().text_color(st.c(|c| c.faint)).child("›"))
                    .child(
                        div()
                            .text_color(st.c(|c| c.muted))
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(crumb),
                    ),
            )
            .child(div().flex_1())
            .when(open, |d| {
                d.child(
                    // Zoom group: a well holding − 100 % + (DESIGN.md § Icon group).
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .p(px(2.0))
                        .rounded(st.px(|m| m.control_radius))
                        .bg(st.c(|c| c.hover))
                        .shadow(st.well())
                        .child(
                            icon_button("zoom-out", "minus", false, st)
                                .tip("Zoom out  Ctrl+−")
                                .on_click(|_, w, cx| w.dispatch_action(Box::new(ZoomOut), cx)),
                        )
                        .child(
                            div()
                                .w(px(52.0))
                                .text_center()
                                .text_size(px(12.0))
                                .text_color(st.c(|c| c.text))
                                .child(format!("{percent} %")),
                        )
                        .child(
                            icon_button("zoom-in", "plus", false, st)
                                .tip("Zoom in  Ctrl+=")
                                .on_click(|_, w, cx| w.dispatch_action(Box::new(ZoomIn), cx)),
                        ),
                )
                .child(
                    icon_button("fit-width", "fit-width", mode == ZoomMode::FitWidth, st)
                        .tip("Fit width  Ctrl+2")
                        .on_click(|_, w, cx| w.dispatch_action(Box::new(FitWidth), cx)),
                )
                .child(
                    icon_button("fit-page", "fit-page", mode == ZoomMode::FitPage, st)
                        .tip("Fit page  Ctrl+0")
                        .on_click(|_, w, cx| w.dispatch_action(Box::new(FitPage), cx)),
                )
                .child(
                    icon_button("rotate", "rotate", false, st)
                        .tip("Rotate  Ctrl+Shift+=")
                        .on_click(|_, w, cx| w.dispatch_action(Box::new(RotateRight), cx)),
                )
            })
            .child(
                icon_button(
                    "theme",
                    if st.theme.appearance == kitab_theme::Appearance::Light {
                        "sun"
                    } else {
                        "moon"
                    },
                    false,
                    st,
                )
                .tip("Next theme  Ctrl+Shift+L")
                .on_click(cx.listener(|ws, _, _, cx| ws.cycle_theme(cx))),
            )
            .into_any_element()
    }

    fn sidebar_view(
        &mut self,
        reader: &Entity<Reader>,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let r = reader.read(cx);
        let mut rows = Vec::new();
        flatten(r.outline(), 0, &mut rows);
        let current = r.current_page() as u32;
        // The current entry: the last one whose page is at or before the current page.
        let active_ix = rows
            .iter()
            .rposition(|(_, it)| it.page.is_some_and(|p| p <= current));
        let reader = reader.clone();
        let mut col = div().flex().flex_col().gap(px(1.0)).p(px(8.0));
        if rows.is_empty() {
            col = col.child(
                div()
                    .px(px(8.0))
                    .py(px(6.0))
                    .text_size(st.px(|m| m.ui_text))
                    .text_color(st.c(|c| c.faint))
                    .child("This document has no contents list."),
            );
        }
        for (ix, (depth, item)) in rows.into_iter().enumerate() {
            let on = Some(ix) == active_ix;
            let target = item.page;
            let reader = reader.clone();
            col = col.child(
                div()
                    .id(("toc", ix))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .h(st.px(|m| m.list_row))
                    .pl(px(8.0 + depth as f32 * 14.0))
                    .pr(px(8.0))
                    .rounded(st.px(|m| m.control_radius))
                    .text_size(st.px(|m| m.ui_text))
                    .text_color(if on {
                        st.c(|c| c.text)
                    } else {
                        st.c(|c| c.muted)
                    })
                    .when(on, |d| d.bg(st.c(|c| c.raised)).shadow(st.elev()))
                    .hover(|d| d.bg(st.c(|c| c.hover)).text_color(st.c(|c| c.text)))
                    .on_click(move |_, _, cx| {
                        if let Some(p) = target {
                            reader.update(cx, |r, cx| r.jump_to(p, cx));
                        }
                    })
                    .child(
                        div()
                            .size(px(10.0))
                            .flex_none()
                            .rounded_full()
                            .border_1()
                            .border_color(if on {
                                st.c(|c| c.accent)
                            } else {
                                st.c(|c| c.ring)
                            })
                            .when(on, |d| d.bg(st.c(|c| c.accent))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(item.title),
                    )
                    .children(item.page.map(|p| {
                        div()
                            .text_size(px(11.5))
                            .text_color(st.c(|c| c.faint))
                            .child(format!("{}", p + 1))
                    })),
            );
        }
        div()
            .id("sidebar")
            .w(px(crate::layout::SIDEBAR_W))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(st.c(|c| c.panel))
            .border_r_1()
            .border_color(st.c(|c| c.line))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(36.0))
                    .px(px(16.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(st.c(|c| c.muted))
                    .child("Contents"),
            )
            .child(
                div()
                    .id("toc-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(col),
            )
            .into_any_element()
    }

    fn home(&mut self, st: &Style, cx: &mut Context<Self>) -> AnyElement {
        let open_button = div()
            .id("open")
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(st.px(|m| m.control_height))
            .px(px(14.0))
            .rounded(st.px(|m| m.control_radius))
            .bg(st.c(|c| c.raised))
            .shadow(st.elev())
            .text_size(st.px(|m| m.ui_text))
            .text_color(st.c(|c| c.text))
            .hover(|d| d.bg(st.c(|c| c.hover)))
            .on_click(cx.listener(|ws, _, window, cx| ws.open_dialog(window, cx)))
            .child(
                svg()
                    .path(icon("folder-open"))
                    .size(px(15.0))
                    .text_color(st.c(|c| c.muted)),
            )
            .child("Open file…");

        if self.recent.is_empty() {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.0))
                .bg(st.c(|c| c.bg))
                .child(
                    svg()
                        .path(icon("book"))
                        .size(px(64.0))
                        .text_color(st.c(|c| c.ring)),
                )
                .child(
                    div()
                        .text_size(px(22.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(st.c(|c| c.text))
                        .child("No books yet"),
                )
                .child(
                    div()
                        .text_size(st.px(|m| m.ui_text))
                        .text_color(st.c(|c| c.muted))
                        .child("Drop a PDF here, or press Ctrl+O. Kitab remembers where you stop."),
                )
                .child(div().mt(px(8.0)).child(open_button))
                .into_any_element();
        }

        let continuing: Vec<RecentItem> = self
            .recent
            .iter()
            .filter(|r| !r.missing && r.progress > 0.0 && r.progress < 0.995)
            .take(3)
            .cloned()
            .collect();
        let mut page = div()
            .id("home")
            .size_full()
            .overflow_y_scroll()
            .bg(st.c(|c| c.bg))
            .child(
                div()
                    .flex()
                    .items_center()
                    .px(px(40.0))
                    .pt(px(32.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(st.c(|c| c.text))
                            .child("Home"),
                    )
                    .child(open_button),
            );
        if !continuing.is_empty() {
            let mut row = div().flex().gap(px(16.0)).px(px(40.0)).pt(px(12.0));
            for (i, item) in continuing.into_iter().enumerate() {
                row = row.child(self.continue_card(i, item, st, cx));
            }
            page = page.child(section_title("Continue reading", st)).child(row);
        }
        let mut grid = div()
            .flex()
            .flex_wrap()
            .gap(px(20.0))
            .px(px(40.0))
            .pt(px(12.0))
            .pb(px(40.0));
        for (i, item) in self.recent.clone().into_iter().enumerate() {
            grid = grid.child(self.recent_card(i, item, st, cx));
        }
        page.child(section_title("Recent", st))
            .child(grid)
            .into_any_element()
    }

    fn continue_card(
        &mut self,
        i: usize,
        item: RecentItem,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = item.path.clone();
        div()
            .id(("continue", i))
            .flex()
            .gap(px(14.0))
            .w(px(320.0))
            .p(px(12.0))
            .rounded(px(12.0))
            .bg(st.c(|c| c.raised))
            .shadow(st.elev())
            .hover(|d| d.bg(st.c(|c| c.hover)))
            .on_click(cx.listener(move |ws, _, window, cx| ws.open_tab(path.clone(), window, cx)))
            .child(cover(&item.title, 56.0, st))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(st.c(|c| c.text))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(item.title.clone()),
                    )
                    .children(item.author.clone().map(|a| {
                        div()
                            .text_size(px(12.0))
                            .text_color(st.c(|c| c.muted))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(a)
                    }))
                    .child(div().flex_1())
                    .child(progress_bar(item.progress, st))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(st.c(|c| c.faint))
                            .child(format!("{} % read", (item.progress * 100.0).round() as u32)),
                    ),
            )
            .into_any_element()
    }

    fn recent_card(
        &mut self,
        i: usize,
        item: RecentItem,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = item.path.clone();
        let missing = item.missing;
        div()
            .id(("recent", i))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .w(px(132.0))
            .when(missing, |d| d.opacity(0.5))
            .when(!missing, |d| {
                d.on_click(
                    cx.listener(move |ws, _, window, cx| ws.open_tab(path.clone(), window, cx)),
                )
            })
            .child(
                div().relative().child(cover(&item.title, 132.0, st)).child(
                    div()
                        .absolute()
                        .top(px(8.0))
                        .right(px(8.0))
                        .px(px(6.0))
                        .py(px(1.0))
                        .rounded(px(5.0))
                        .bg(st.ca(|c| c.bg, 0.85))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(st.c(|c| c.muted))
                        .child(format!("{:?}", item.format).to_uppercase()),
                ),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(st.c(|c| c.text))
                    .line_clamp(2)
                    .child(item.title.clone()),
            )
            .children(item.author.clone().map(|a| {
                div()
                    .text_size(px(12.0))
                    .text_color(st.c(|c| c.muted))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(a)
            }))
            .child(if missing {
                div()
                    .text_size(px(11.5))
                    .text_color(st.c(|c| c.faint))
                    .child("File not found")
                    .into_any_element()
            } else {
                progress_bar(item.progress, st)
            })
            .into_any_element()
    }
}

fn section_title(text: &str, st: &Style) -> AnyElement {
    div()
        .px(px(40.0))
        .pt(px(28.0))
        .text_size(px(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(st.c(|c| c.muted))
        .child(text.to_string())
        .into_any_element()
}

fn progress_bar(progress: f32, st: &Style) -> AnyElement {
    div()
        .h(px(4.0))
        .w_full()
        .rounded_full()
        .bg(st.c(|c| c.hover))
        .child(
            div()
                .h_full()
                .w(gpui::relative(progress.clamp(0.0, 1.0)))
                .rounded_full()
                .bg(st.c(|c| c.accent)),
        )
        .into_any_element()
}

/// A generated cover (DESIGN.md § Covers): a muted hue from the title's hash, the
/// title's initial, and a spine shadow. Real covers arrive with the library.
fn cover(title: &str, width: f32, st: &Style) -> AnyElement {
    let hash = title.bytes().fold(2166136261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    });
    let hue = (hash % 360) as f32 / 360.0;
    let dark = st.theme.appearance != kitab_theme::Appearance::Light;
    let fill = gpui::Hsla {
        h: hue,
        s: 0.28,
        l: if dark { 0.30 } else { 0.72 },
        a: 1.0,
    };
    let ink = gpui::Hsla {
        h: hue,
        s: 0.35,
        l: if dark { 0.82 } else { 0.25 },
        a: 1.0,
    };
    let spine = gpui::Hsla {
        h: hue,
        s: 0.3,
        l: if dark { 0.22 } else { 0.6 },
        a: 1.0,
    };
    let initial = title
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    div()
        .w(px(width))
        .h(px(width * 1.5))
        .flex_none()
        .flex()
        .rounded(px(4.0))
        .overflow_hidden()
        .bg(fill)
        .shadow(st.elev())
        .child(div().w(px((width * 0.035).max(3.0))).h_full().bg(spine))
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(width * 0.42))
                .font_family("Georgia")
                .text_color(ink)
                .child(initial),
        )
        .into_any_element()
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let st = Style::get(cx).clone();
        let title_bar = self.title_bar(&st, cx);
        let body = match self.active_reader().cloned() {
            Some(reader) => {
                let header = self.header_row(&reader, &st, cx);
                let side = if self.sidebar {
                    Some(self.sidebar_view(&reader, &st, cx))
                } else {
                    None
                };
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(header)
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .children(side)
                            .child(div().flex_1().min_w_0().h_full().child(reader)),
                    )
                    .into_any_element()
            }
            None => self.home(&st, cx),
        };
        let _ = window;
        div()
            .id("workspace")
            .key_context(WORKSPACE)
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(st.c(|c| c.bg))
            .text_color(st.c(|c| c.text))
            .font_family("Segoe UI")
            .on_action(cx.listener(|ws, _: &OpenFile, window, cx| ws.open_dialog(window, cx)))
            .on_action(cx.listener(|ws, _: &CloseTab, window, cx| {
                if let Some(i) = ws.active {
                    ws.close_tab(i, window, cx)
                }
            }))
            .on_action(cx.listener(|ws, _: &NextTab, window, cx| {
                if !ws.tabs.is_empty() {
                    let next = ws.active.map_or(0, |i| (i + 1) % ws.tabs.len());
                    ws.activate(next, window, cx)
                }
            }))
            .on_action(cx.listener(|ws, _: &PrevTab, window, cx| {
                if !ws.tabs.is_empty() {
                    let n = ws.tabs.len();
                    let prev = ws.active.map_or(n - 1, |i| (i + n - 1) % n);
                    ws.activate(prev, window, cx)
                }
            }))
            .on_action(cx.listener(|ws, _: &ShowHome, window, cx| ws.show_home(window, cx)))
            .on_action(cx.listener(|ws, _: &ToggleSidebar, _, cx| {
                ws.sidebar = !ws.sidebar;
                cx.notify()
            }))
            .on_action(cx.listener(|ws, _: &CycleTheme, _, cx| ws.cycle_theme(cx)))
            .on_drop(cx.listener(|ws, paths: &ExternalPaths, window, cx| {
                for p in paths.paths() {
                    ws.open_tab(p.clone(), window, cx);
                }
            }))
            .child(title_bar)
            .child(body)
    }
}
