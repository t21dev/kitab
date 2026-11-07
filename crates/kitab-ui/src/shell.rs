// SPDX-License-Identifier: AGPL-3.0-or-later

//! The window: title bar with Home and tabs, header row, contents sidebar and the
//! reader (DESIGN.md § Components, "Docked" shell, D10).

use crate::actions::*;
use crate::engine::DocEngine;
use crate::icons::icon;
use crate::library::{Library, RecentItem, SessionTab};
use crate::menu::{self, MenuItem, OpenMenu, Tool};
use crate::motion;
use crate::reader::{Reader, ReaderEvent, open_reader};
use crate::settings::TabsAt;
use crate::settings_view::{self, HexState, Section, SystemInfo};
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, App, AppContext, ClipboardItem, Context, ElementId, Entity, ExternalPaths,
    FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, ParentElement, PathPromptOptions, Pixels, Point, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, WindowControlArea, anchored,
    deferred, div, point, prelude::FluentBuilder, px, svg,
};
use kitab_core::{OutlineItem, ZoomMode};
use kitab_theme::{Accent, AccentPreset, Theme};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

mod palette_ui;
mod save_page;
mod system;
mod tools_ui;
pub use system::VERSION;
use tools_ui::ToolsState;

/// Writes settings.json (the app passes it in; called off the UI thread).
pub struct SettingsSink(pub Arc<dyn Fn(String) + Send + Sync>);
impl gpui::Global for SettingsSink {}

/// Closed tabs remembered for Ctrl+Shift+T.
const CLOSED_MAX: usize = 20;

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
    /// The open menu (⋯ or a right click), and one fading out.
    menu: Option<OpenMenu>,
    menu_closing: Option<OpenMenu>,
    menu_serial: u64,
    /// The Settings modal, and one fading out.
    settings: Option<Section>,
    settings_closing: Option<Section>,
    settings_section: Section,
    settings_serial: u64,
    overlay_focus: FocusHandle,
    /// Built-in themes, parsed when Settings first opens.
    themes: Vec<Theme>,
    closed: Vec<PathBuf>,
    /// The sidebar is sliding shut; it leaves the layout when that ends.
    sidebar_closing: bool,
    /// Bumped on each open so the slide and the row stagger replay; 0 = no motion
    /// (the start-up layout must match the first frame exactly).
    sidebar_serial: u64,
    home_serial: u64,
    /// Set after the first frame: tabs opened later fade in.
    booted: bool,
    hex: HexState,
    tools: ToolsState,
    /// The tab strip scrolls sideways once tabs reach their minimum width.
    tab_scroll: gpui::ScrollHandle,
    /// Scroll the active tab into view on the next frame (it may not be laid out yet).
    reveal_tab: bool,
    /// Minimal mode: the pointer is at the top edge, so the window buttons show.
    peek: bool,
    /// What the sidebar shows for the document: contents or marks.
    side_pane: SidePane,
    /// Settings › File types, Advanced and About, and the update check.
    system: SystemInfo,
    /// The command palette (Ctrl+K), when open.
    palette: Option<palette_ui::Palette>,
}

/// The sidebar's panes for a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidePane {
    #[default]
    Contents,
    Pages,
    Marks,
    Bookmarks,
}

impl SidePane {
    fn label(self) -> &'static str {
        match self {
            SidePane::Contents => "Contents",
            SidePane::Pages => "Pages",
            SidePane::Marks => "Marks",
            SidePane::Bookmarks => "Bookmarks",
        }
    }
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
            sidebar: Style::get(cx).sidebar_open,
            focus,
            subs: Vec::new(),
            menu: None,
            menu_closing: None,
            menu_serial: 0,
            settings: None,
            settings_closing: None,
            settings_section: Section::default(),
            settings_serial: 0,
            overlay_focus: cx.focus_handle(),
            themes: Vec::new(),
            closed: Vec::new(),
            sidebar_closing: false,
            sidebar_serial: 0,
            side_pane: SidePane::Contents,
            home_serial: 0,
            booted: false,
            hex: HexState {
                edit: None,
                focus: cx.focus_handle(),
            },
            tools: ToolsState::new(cx),
            tab_scroll: gpui::ScrollHandle::new(),
            reveal_tab: false,
            peek: false,
            system: SystemInfo::default(),
            palette: None,
        };
        // Leaving the hex field ends the edit.
        let hex_blur = cx.on_blur(&this.hex.focus, window, |ws, _, cx| {
            ws.hex.edit = None;
            cx.notify();
        });
        this.subs.push(hex_blur);
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
        this.first_update_check(cx);
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
        self.subs.push(cx.subscribe_in(
            &reader,
            window,
            |this, reader, e: &ReaderEvent, window, cx| {
                match e {
                    // The library knows the document only once it has opened; save again.
                    ReaderEvent::Opened => {
                        this.save_session(cx);
                        cx.notify();
                    }
                    ReaderEvent::PageChanged => cx.notify(),
                    ReaderEvent::ContextMenu(at) => this.page_menu(reader.clone(), *at, window, cx),
                    ReaderEvent::MoreMenu(at) => this.more_menu(Some(*at), window, cx),
                    ReaderEvent::OpenPath(path) => this.open_tab(path.clone(), window, cx),
                    ReaderEvent::BookText(pt) => {
                        // Remembered across launches (settings.json `book_text_size`).
                        let pt = *pt;
                        this.change_style(&move |st: &mut Style| st.book_text = pt, cx);
                    }
                }
            },
        ));
        self.tabs.push(reader);
        let ix = self.tabs.len() - 1;
        self.activate(ix, window, cx);
    }

    fn activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal_tab = true;
        if let Some(t) = self.tabs.get(ix).cloned() {
            if self.tools.split.as_ref() == Some(&t) {
                self.tools.split = self.active_reader().cloned();
            }
            self.active = Some(ix);
            self.save_session(cx);
            let title = t.read(cx).title();
            window.set_window_title(&format!("{title} · Kitab"));
            window.focus(&t.focus_handle(cx));
        }
        cx.notify();
    }

    fn show_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active.is_some() {
            self.home_serial += 1;
        }
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
        // Unsaved Markdown edits: ask first.
        if self.tabs[ix].read(cx).has_unsaved_edits() {
            let reader = self.tabs[ix].clone();
            let name = reader.read(cx).title();
            let answer = window.prompt(
                gpui::PromptLevel::Warning,
                &format!("Save your changes to {name}?"),
                Some("Your edits will be lost if you don't save them."),
                &["Save", "Don't save", "Cancel"],
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                let choice = answer.await.ok();
                let _ = this.update_in(cx, |ws, window, cx| {
                    let Some(ix) = ws.tabs.iter().position(|t| *t == reader) else {
                        return;
                    };
                    match choice {
                        Some(0) => {
                            reader.update(cx, |r, cx| r.save_markdown(cx));
                            ws.force_close_tab(ix, window, cx);
                        }
                        Some(1) => ws.force_close_tab(ix, window, cx),
                        _ => {}
                    }
                });
            })
            .detach();
            return;
        }
        self.force_close_tab(ix, window, cx);
    }

    fn force_close_tab(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if ix >= self.tabs.len() {
            return;
        }
        let closing = self.tabs.remove(ix);
        if self.tools.split.as_ref() == Some(&closing) {
            self.tools.split = None;
        }
        closing.update(cx, |r, cx| r.save_now(cx));
        self.remember_closed(closing.read(cx).path.clone());
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
            self.change_style(&move |s: &mut Style| s.set_theme(theme.clone()), cx);
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
        .flex_none()
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
    fn title_bar(&mut self, st: &Style, window: &Window, cx: &mut Context<Self>) -> AnyElement {
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
                    .flex_none()
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

        // Room for tabs: the bar minus Home, the divider, the "All tabs"
        // button, a 96 px drag area and the three window buttons. Tabs share it
        // down to 120 px each; past that the strip scrolls.
        let n = self.tabs.len().max(1) as f32;
        let fixed = 8.0 + 2.0 * st.m().icon_button + 17.0 + 40.0 + 96.0 + 3.0 * 46.0 + 12.0;
        let room = (f32::from(window.viewport_size().width) - fixed).max(120.0);
        let tab_w = ((room - 4.0 * n) / n).clamp(120.0, 220.0);
        let crowded = n * 224.0 > room;
        if self.reveal_tab {
            if let Some(a) = self.active {
                self.tab_scroll.scroll_to_item(a);
                // GPUI applies it once the tab has been laid out; until then
                // ask for one more frame (an idle window draws none).
                if self.tab_scroll.bounds_for_item(a).is_some() {
                    self.reveal_tab = false;
                } else {
                    cx.notify();
                }
            } else {
                self.reveal_tab = false;
            }
        }
        let mut strip = div()
            .id("tab-strip")
            .flex_none()
            .max_w(px(room))
            .flex()
            .items_center()
            .gap(px(4.0))
            .min_w_0()
            .h_full()
            .overflow_x_scroll()
            .track_scroll(&self.tab_scroll);
        for (ix, tab) in self.tabs.iter().enumerate() {
            let on = self.active == Some(ix);
            // The tab shown in the side pane of a split view.
            let side = !on && self.tools.split.as_ref() == Some(tab);
            let r = tab.read(cx);
            let title = r.title();
            let tab_icon = tab_icon_for(&r.path);
            let animate = self.booted && st.animations;
            let tab_id = tab.entity_id().as_u64();
            let tab_el = div()
                .id(("tab", ix))
                .group("tab")
                .relative()
                .on_mouse_up(
                    MouseButton::Middle,
                    cx.listener(move |ws, _, window, cx| ws.close_tab(ix, window, cx)),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        ws.tab_menu(ix, e.position, window, cx)
                    }),
                )
                .flex()
                .items_center()
                .gap(px(8.0))
                .h(st.px(|m| m.control_height))
                .pl(px(10.0))
                .pr(px(6.0))
                .flex_none()
                .max_w(px(220.0))
                // Crowded: every tab gets an equal share, at least 120 px.
                .when(crowded, |d| d.w(px(tab_w)))
                .rounded(st.px(|m| m.control_radius))
                .text_size(st.px(|m| m.ui_text))
                .text_color(if on {
                    st.c(|c| c.text)
                } else {
                    st.c(|c| c.muted)
                })
                .when(on, |d| d.bg(st.c(|c| c.raised)).shadow(st.elev()))
                .hover(|d| d.text_color(st.c(|c| c.text)))
                .when(side, |d| {
                    d.bg(st.c(|c| c.hover)).text_color(st.c(|c| c.text))
                })
                .on_click(cx.listener(move |ws, _, window, cx| ws.activate(ix, window, cx)))
                .child(svg().path(icon(tab_icon)).size(px(14.0)).text_color(if on {
                    st.c(|c| c.accent)
                } else {
                    st.c(|c| c.faint)
                }))
                .child(div().flex_1().min_w_0().truncate().child(title))
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
                        .when(crowded && !on, |d| {
                            d.absolute().right(px(6.0)).bg(st.c(|c| c.bg))
                        })
                        .group_hover("tab", |d| d.visible())
                        .hover(|d| d.bg(st.c(|c| c.hover)))
                        .tip("Close tab  Ctrl+W")
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
                );
            strip = strip.child(motion::enter(
                tab_el,
                ElementId::NamedInteger("tab-in".into(), tab_id),
                animate,
                motion::From::Above(6.0),
                motion::BASE,
                Duration::ZERO,
            ));
        }

        // "All tabs" appears once the strip overflows: a menu of every tab.
        let overflowing = self.tab_scroll.max_offset().width > px(0.5);
        bar = bar
            .child(strip)
            // New tab: the system Open dialog (Ctrl+O).
            .child(
                icon_button("new-tab", "plus", false, st)
                    .flex_none()
                    .tip("Open a file  Ctrl+O")
                    .on_click(cx.listener(|ws, _, window, cx| ws.open_dialog(window, cx))),
            )
            .when(overflowing, |b| {
                b.child(
                    icon_button("all-tabs", "chevron-down", false, st)
                        .flex_none()
                        .tip("All tabs")
                        .on_click(cx.listener(|ws, e: &gpui::ClickEvent, window, cx| {
                            ws.all_tabs_menu(e.position(), window, cx)
                        })),
                )
            });
        let pill = self.update_pill(st, cx);
        bar.child(
            div()
                .id("drag")
                .flex_1()
                // Always leave room to drag the window.
                .min_w(px(96.0))
                .h_full()
                .window_control_area(WindowControlArea::Drag),
        )
        .children(pill)
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

    fn sidebar_view(
        &mut self,
        reader: Option<&Entity<Reader>>,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = Vec::new();
        let mut current = 0;
        let mut markdown = false;
        if let Some(r) = reader.map(|r| r.read(cx)) {
            flatten(r.outline(), 0, &mut rows);
            current = r.current_page() as u32;
            markdown = r.is_markdown();
        }
        let tabs_here = st.tabs == TabsAt::Sidebar && st.shows_tabs();
        let tabs_panel = tabs_here.then(|| self.vertical_tabs(st, cx));
        // The current entry: the last one whose page is at or before the current page.
        let active_ix = rows
            .iter()
            .rposition(|(_, it)| it.page.is_some_and(|p| p <= current));
        let Some(reader) = reader.cloned() else {
            // Home with tabs in the sidebar: only the open documents.
            return self.sidebar_frame(tabs_panel, None, st, cx);
        };
        if self.side_pane == SidePane::Marks {
            let pane = self.marks_pane(&reader, st, cx);
            return self.sidebar_frame(tabs_panel, Some(pane), st, cx);
        }
        if self.side_pane == SidePane::Pages && !reader.read(cx).is_markdown() {
            let pane = reader.update(cx, |r, cx| r.pages_pane(cx));
            return self.sidebar_frame(tabs_panel, Some(pane), st, cx);
        }
        if self.side_pane == SidePane::Bookmarks {
            let pane = self.bookmarks_pane(&reader, st, cx);
            return self.sidebar_frame(tabs_panel, Some(pane), st, cx);
        }
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
        let stagger = st.animations && self.sidebar_serial > 0;
        let serial = self.sidebar_serial;
        for (ix, (depth, item)) in rows.into_iter().enumerate() {
            let on = Some(ix) == active_ix;
            let target = item.page;
            let menu_reader = reader.clone();
            let menu_title = item.title.clone();
            let reader = reader.clone();
            let row = div()
                .id(("toc", ix))
                .relative()
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        ws.toc_menu(
                            menu_reader.clone(),
                            target,
                            menu_title.clone(),
                            e.position,
                            window,
                            cx,
                        )
                    }),
                )
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
                // Markdown entries point at blocks, not pages: no number.
                .children(item.page.filter(|_| !markdown).map(|p| {
                    div()
                        .text_size(px(11.5))
                        .text_color(st.c(|c| c.faint))
                        .child(format!("{}", p + 1))
                }));
            col = col.child(motion::enter(
                row,
                ElementId::NamedInteger(format!("toc-{serial}").into(), ix as u64),
                stagger,
                motion::From::Left(8.0),
                motion::BASE,
                motion::stagger(ix),
            ));
        }
        self.sidebar_frame(tabs_panel, Some(col.into_any_element()), st, cx)
    }

    /// The Marks pane: export, then one row per mark or annotation.
    fn marks_pane(
        &mut self,
        reader: &Entity<Reader>,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = reader.update(cx, |r, cx| {
            r.load_file_annots(cx);
            r.mark_rows(cx)
        });
        let pdf = reader.read(cx).is_pdf();
        let mut col = div().flex().flex_col().gap(px(1.0)).p(px(8.0));
        let export = |id: &'static str, label: &'static str, csv: bool| {
            let reader = reader.clone();
            div()
                .id(id)
                .px(px(8.0))
                .h(px(24.0))
                .flex()
                .items_center()
                .rounded(st.px(|m| m.control_radius))
                .text_size(px(11.5))
                .text_color(st.c(|c| c.muted))
                .hover(|d| d.bg(st.c(|c| c.hover)).text_color(st.c(|c| c.text)))
                .child(label)
                .on_click(cx.listener(move |ws, _, _, cx| ws.export_marks(&reader, csv, cx)))
        };
        col = col.child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(4.0))
                .pb(px(6.0))
                .text_size(px(11.5))
                .text_color(st.c(|c| c.faint))
                .child("Export")
                .child(export("marks-md", "Markdown", false))
                .child(export("marks-csv", "CSV", true)),
        );
        if rows.is_empty() {
            col = col.child(
                div()
                    .px(px(8.0))
                    .py(px(6.0))
                    .text_size(st.px(|m| m.ui_text))
                    .text_color(st.c(|c| c.faint))
                    .child("No marks yet. Select text and press H, or draw with Ctrl+Shift+D."),
            );
        }
        for (ix, row) in rows.into_iter().enumerate() {
            let page = row.page;
            let jump = reader.clone();
            let dot = match row.colour {
                Some((r, g, b)) => gpui::Rgba {
                    r: f32::from(r) / 255.0,
                    g: f32::from(g) / 255.0,
                    b: f32::from(b) / 255.0,
                    a: 1.0,
                }
                .into(),
                None => st.c(|c| c.ring),
            };
            let pending = match row.source {
                crate::reader::MarkSource::Kitab(_) if pdf => Some("Not saved into the file yet"),
                crate::reader::MarkSource::Kitab(_) => Some("Kept in Kitab"),
                crate::reader::MarkSource::File => None,
            };
            let delete = match row.source {
                crate::reader::MarkSource::Kitab(local) => {
                    let r = reader.clone();
                    Some(
                        div()
                            .id(("mark-del", ix))
                            .size(px(22.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            // Shows when the row is pointed at.
                            .opacity(0.0)
                            .group_hover("mark-row", |s| s.opacity(1.0))
                            .hover(|d| d.bg(st.c(|c| c.raised)))
                            .tip("Delete  Ctrl+Z brings it back")
                            .child(
                                svg()
                                    .path(icon("close"))
                                    .size(px(12.0))
                                    .text_color(st.c(|c| c.muted)),
                            )
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                r.update(cx, |r, cx| r.delete_mark(local, cx));
                            }),
                    )
                }
                crate::reader::MarkSource::File => None,
            };
            col = col.child(
                div()
                    .id(("mark-row", ix))
                    .group("mark-row")
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(st.px(|m| m.control_radius))
                    .hover(|d| d.bg(st.c(|c| c.hover)))
                    .on_click(move |_, _, cx| jump.update(cx, |r, cx| r.jump_to(page, cx)))
                    .child(
                        div()
                            .mt(px(4.0))
                            .size(px(10.0))
                            .flex_none()
                            .rounded_full()
                            .bg(dot),
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
                                    .flex()
                                    .justify_between()
                                    .text_size(st.px(|m| m.ui_text))
                                    .text_color(st.c(|c| c.text))
                                    .child(row.kind.clone())
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(st.c(|c| c.faint))
                                            .child(format!("{}", page + 1)),
                                    ),
                            )
                            .children(row.text.map(|t| {
                                div()
                                    .text_size(px(12.0))
                                    .text_color(st.c(|c| c.muted))
                                    .line_clamp(3)
                                    .child(t)
                            }))
                            .children(pending.map(|p| {
                                div()
                                    .text_size(px(11.0))
                                    .text_color(st.c(|c| c.faint))
                                    .child(p)
                            })),
                    )
                    .children(delete),
            );
        }
        col.into_any_element()
    }

    /// The Bookmarks pane: one row per bookmarked page.
    fn bookmarks_pane(
        &mut self,
        reader: &Entity<Reader>,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (rows, current, can) = {
            let r = reader.read(cx);
            (
                r.bookmarks().to_vec(),
                r.current_page() as u32,
                r.can_bookmark(),
            )
        };
        let mut col = div().flex().flex_col().gap(px(1.0)).p(px(8.0));
        if rows.is_empty() {
            let hint = if can {
                "No bookmarks. Press Ctrl+D on a page to add one."
            } else {
                "Books that re-flow don't take bookmarks yet."
            };
            col = col.child(
                div()
                    .px(px(8.0))
                    .py(px(6.0))
                    .text_size(st.px(|m| m.ui_text))
                    .text_color(st.c(|c| c.faint))
                    .child(hint),
            );
        }
        for (ix, b) in rows.into_iter().enumerate() {
            let page = b.page;
            let on = page == current;
            let (jump, del) = (reader.clone(), reader.clone());
            col = col.child(
                div()
                    .id(("bm-row", ix))
                    .group("bm-row")
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .h(st.px(|m| m.list_row))
                    .px(px(8.0))
                    .rounded(st.px(|m| m.control_radius))
                    .text_size(st.px(|m| m.ui_text))
                    .text_color(if on {
                        st.c(|c| c.text)
                    } else {
                        st.c(|c| c.muted)
                    })
                    .when(on, |d| d.bg(st.c(|c| c.raised)).shadow(st.elev()))
                    .hover(|d| d.bg(st.c(|c| c.hover)).text_color(st.c(|c| c.text)))
                    .on_click(move |_, _, cx| jump.update(cx, |r, cx| r.jump_to(page, cx)))
                    .child(
                        svg()
                            .path(icon("bookmark"))
                            .size(px(13.0))
                            .flex_none()
                            .text_color(st.c(|c| c.accent)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(b.title),
                    )
                    .child(
                        div()
                            .id(("bm-del", ix))
                            .size(px(22.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .opacity(0.0)
                            .group_hover("bm-row", |s| s.opacity(1.0))
                            .hover(|d| d.bg(st.c(|c| c.raised)))
                            .tip("Remove bookmark")
                            .child(
                                svg()
                                    .path(icon("close"))
                                    .size(px(12.0))
                                    .text_color(st.c(|c| c.muted)),
                            )
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                del.update(cx, |r, cx| r.remove_bookmark(page, cx));
                            }),
                    ),
            );
        }
        col.into_any_element()
    }

    /// Save the marks as Markdown or CSV next to the document.
    fn export_marks(&mut self, reader: &Entity<Reader>, csv: bool, cx: &mut Context<Self>) {
        let (content, name, dir) = reader.update(cx, |r, cx| {
            let content = if csv {
                r.marks_csv(cx)
            } else {
                r.marks_markdown(cx)
            };
            let stem = r
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "marks".into());
            let ext = if csv { "csv" } else { "md" };
            let dir = r.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            (content, format!("{stem} - marks.{ext}"), dir)
        });
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(out))) = rx.await else {
                return;
            };
            let written = cx
                .background_spawn({
                    let out = out.clone();
                    async move {
                        std::fs::write(&out, content)
                            .map(|_| out)
                            .map_err(|e| e.to_string())
                    }
                })
                .await;
            let _ = this.update(cx, |ws, cx| ws.report(written, "Exported", cx));
        })
        .detach();
    }

    /// The sidebar: with tabs in the sidebar, a header (sidebar, Home, open)
    /// and the open documents; then Contents when a document is showing.
    fn sidebar_frame(
        &mut self,
        tabs_panel: Option<AnyElement>,
        contents: Option<AnyElement>,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let heading = |text: &'static str| {
            div()
                .flex()
                .items_center()
                .h(px(32.0))
                .px(px(16.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(st.c(|c| c.muted))
                .child(text)
        };
        let has_tabs = tabs_panel.is_some();
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
            .when(has_tabs, |d| {
                d.child(
                    div()
                        .id("sidebar-head")
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(4.0))
                        .h(px(crate::reader::TOP_INSET))
                        .px(px(8.0))
                        .child(
                            icon_button("side-sidebar", "sidebar", true, st)
                                .tip("Sidebar  Ctrl+B")
                                .on_click(cx.listener(|ws, _, _, cx| ws.toggle_sidebar(cx))),
                        )
                        .child(
                            icon_button("side-home", "home", self.active.is_none(), st)
                                .tip("Home  Ctrl+Shift+H")
                                .on_click(
                                    cx.listener(|ws, _, window, cx| ws.show_home(window, cx)),
                                ),
                        )
                        .child(
                            div()
                                .id("side-drag")
                                .flex_1()
                                .h_full()
                                .window_control_area(WindowControlArea::Drag),
                        )
                        .child(
                            icon_button("side-open", "plus", false, st)
                                .tip("Open a file  Ctrl+O")
                                .on_click(
                                    cx.listener(|ws, _, window, cx| ws.open_dialog(window, cx)),
                                ),
                        ),
                )
                .child(heading("Open"))
            })
            .children(tabs_panel)
            .when_some(contents, |d, col| {
                // With tabs on top, the Contents header carries the hide
                // button (with tabs in the sidebar it's in the sidebar's head).
                let pane = self.side_pane;
                let pane_tab = |p: SidePane, icon_name: &'static str, tip: &'static str| {
                    icon_button(icon_name, icon_name, pane == p, st)
                        .tip(tip)
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            ws.side_pane = p;
                            if p == SidePane::Pages {
                                if let Some(r) = ws.active.and_then(|i| ws.tabs.get(i)).cloned() {
                                    r.update(cx, |r, _| r.reveal_in_pages());
                                }
                            }
                            cx.notify();
                        }))
                };
                let contents_head = heading(pane.label())
                    .justify_between()
                    .pr(px(8.0))
                    .child(
                        div()
                            .flex()
                            .gap(px(2.0))
                            .ml_auto()
                            .child(pane_tab(SidePane::Contents, "outline", "Contents"))
                            .child(pane_tab(
                                SidePane::Marks,
                                "highlight",
                                "Marks and annotations",
                            ))
                            .child(pane_tab(SidePane::Pages, "pages", "Page thumbnails"))
                            .child(pane_tab(
                                SidePane::Bookmarks,
                                "bookmark",
                                "Bookmarks  Ctrl+D",
                            )),
                    )
                    .when(!has_tabs, |h| {
                        h.child(
                            icon_button("side-hide", "sidebar", false, st)
                                .tip("Hide sidebar  Ctrl+B")
                                .on_click(cx.listener(|ws, _, _, cx| ws.toggle_sidebar(cx))),
                        )
                    });
                d.child(contents_head).child(
                    div()
                        .id("toc-scroll")
                        .flex_1()
                        .overflow_y_scroll()
                        .child(col),
                )
            })
            .into_any_element()
    }

    /// Open documents as a list (tabs in the sidebar): title, the page you are
    /// on, a close button on hover. Middle-click closes, right-click is the tab menu.
    fn vertical_tabs(&mut self, st: &Style, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div().flex().flex_col().gap(px(2.0)).px(px(8.0)).pb(px(8.0));
        for (ix, tab) in self.tabs.iter().enumerate() {
            let on = self.active == Some(ix);
            let side = !on && self.tools.split.as_ref() == Some(tab);
            let r = tab.read(cx);
            let title = r.title();
            let where_ = (r.status == crate::reader::Status::Open)
                .then(|| format!("{}/{}", r.current_page() + 1, r.page_count()));
            list = list.child(
                div()
                    .id(("vtab", ix))
                    .group("vtab")
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .h(st.px(|m| m.list_row))
                    .pl(px(10.0))
                    .pr(px(6.0))
                    .rounded(st.px(|m| m.control_radius))
                    .text_size(st.px(|m| m.ui_text))
                    .text_color(if on {
                        st.c(|c| c.text)
                    } else {
                        st.c(|c| c.muted)
                    })
                    .when(on, |d| d.bg(st.c(|c| c.raised)).shadow(st.elev()))
                    .when(side, |d| d.bg(st.c(|c| c.hover)))
                    .hover(|d| d.bg(st.c(|c| c.hover)).text_color(st.c(|c| c.text)))
                    .on_click(cx.listener(move |ws, _, window, cx| ws.activate(ix, window, cx)))
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(move |ws, _, window, cx| ws.close_tab(ix, window, cx)),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                            ws.tab_menu(ix, e.position, window, cx)
                        }),
                    )
                    .child(svg().path(icon("file")).size(px(14.0)).text_color(if on {
                        st.c(|c| c.accent)
                    } else {
                        st.c(|c| c.faint)
                    }))
                    .child(div().flex_1().min_w_0().truncate().child(title))
                    .children(where_.map(|w| {
                        div()
                            .flex_none()
                            .text_size(px(11.0))
                            .text_color(st.c(|c| c.faint))
                            .group_hover("vtab", |d| d.invisible())
                            .child(w)
                    }))
                    .child(
                        div()
                            .id(("vclose", ix))
                            .absolute()
                            .right(px(6.0))
                            .size(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .invisible()
                            .group_hover("vtab", |d| d.visible())
                            .hover(|d| d.bg(st.c(|c| c.raised)))
                            .tip("Close tab  Ctrl+W")
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
        if self.tabs.is_empty() {
            list = list.child(
                div()
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_size(st.px(|m| m.ui_text))
                    .text_color(st.c(|c| c.faint))
                    .child("No documents open"),
            );
        }
        div()
            .id("vtabs")
            .flex_none()
            .max_h(px(300.0))
            .overflow_y_scroll()
            .border_b_1()
            .border_color(st.c(|c| c.line))
            .mb(px(4.0))
            .child(list)
            .into_any_element()
    }

    /// Tabs in the sidebar: a transparent strip over the top of the page area
    /// to drag the window, with the window buttons (and, when the sidebar is
    /// closed, the sidebar and Home buttons).
    fn top_strip(&mut self, st: &Style, cx: &mut Context<Self>) -> AnyElement {
        let bare = !self.sidebar && !Style::get(cx).minimal;
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(crate::reader::TOP_INSET))
            .flex()
            .items_center()
            .gap(px(4.0))
            .when(bare, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .h_full()
                        .px(px(6.0))
                        .rounded_br(px(10.0))
                        .bg(st.c(|c| c.bg))
                        .shadow(st.elev())
                        .child(
                            icon_button("strip-sidebar", "sidebar", false, st)
                                .tip("Sidebar  Ctrl+B")
                                .on_click(cx.listener(|ws, _, _, cx| ws.toggle_sidebar(cx))),
                        )
                        .child(
                            icon_button("strip-home", "home", self.active.is_none(), st)
                                .tip("Home  Ctrl+Shift+H")
                                .on_click(
                                    cx.listener(|ws, _, window, cx| ws.show_home(window, cx)),
                                ),
                        ),
                )
            })
            .child(
                div()
                    .id("strip-drag")
                    .flex_1()
                    .h_full()
                    .window_control_area(WindowControlArea::Drag),
            )
            // On a solid corner tab, so they read over any page.
            .child(
                div()
                    .flex()
                    .items_center()
                    .h_full()
                    .when(Style::get(cx).minimal, |d| {
                        d.pl(px(6.0)).child(
                            div()
                                .id("exit-minimal")
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .h(px(26.0))
                                .px(px(10.0))
                                .rounded(px(7.0))
                                .text_size(px(12.0))
                                .text_color(st.c(|c| c.muted))
                                .hover(|d| d.bg(st.c(|c| c.hover)).text_color(st.c(|c| c.text)))
                                .tip("Show everything again  F11")
                                .on_click(cx.listener(|ws, _, _, cx| ws.toggle_minimal(cx)))
                                .child(
                                    svg()
                                        .path(icon("focus"))
                                        .size(px(14.0))
                                        .text_color(st.c(|c| c.muted)),
                                )
                                .child("Exit minimal mode"),
                        )
                    })
                    .rounded_bl(px(10.0))
                    .overflow_hidden()
                    .bg(st.c(|c| c.bg))
                    .shadow(st.elev())
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
                    )),
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
        let settings_button = icon_button("home-settings", "settings", false, st)
            .tip("Settings  Ctrl+,")
            .on_click(cx.listener(|ws, _, window, cx| ws.open_settings(window, cx)));

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
                .child(
                    div()
                        .mt(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(open_button)
                        .child(settings_button),
                )
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
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(settings_button)
                            .child(open_button),
                    ),
            );
        if !continuing.is_empty() {
            let mut row = div().flex().gap(px(16.0)).px(px(40.0)).pt(px(12.0));
            for (i, item) in continuing.into_iter().enumerate() {
                let card = self.continue_card(i, item, st, cx);
                row = row.child(self.card_enter(card, "continue-in", i, st));
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
            let card = self.recent_card(i, item, st, cx);
            grid = grid.child(self.card_enter(card, "recent-in", i + 3, st));
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
        let menu_path = item.path.clone();
        div()
            .id(("continue", i))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    ws.card_menu(menu_path.clone(), e.position, window, cx)
                }),
            )
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
        let menu_path = item.path.clone();
        let missing = item.missing;
        div()
            .id(("recent", i))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    ws.card_menu(menu_path.clone(), e.position, window, cx)
                }),
            )
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
        let side_tabs = st.tabs == TabsAt::Sidebar;
        let title_bar = (!side_tabs && st.shows_tabs()).then(|| self.title_bar(&st, window, cx));
        let show_sidebar = self.sidebar && st.shows_sidebar();
        let body = match self.active_reader().cloned() {
            Some(reader) => {
                // Split view: the other document beside this one; a click there
                // makes it the active pane.
                let side_pane = self.tools.split.clone().map(|side| {
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .border_l_1()
                        .border_color(st.c(|c| c.line))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, window, cx| ws.focus_split(window, cx)),
                        )
                        .child(side)
                });
                let side = if show_sidebar {
                    let inner = self.sidebar_view(Some(&reader), &st, cx);
                    Some(self.sidebar_slide(inner, &st))
                } else {
                    None
                };
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .children(side)
                    .child(div().flex_1().min_w_0().h_full().child(reader))
                    .children(side_pane)
                    .into_any_element()
            }
            None => {
                let home = self.home(&st, cx);
                // Tabs in the sidebar: the open documents stay listed on Home.
                let side = (side_tabs && show_sidebar).then(|| {
                    let inner = self.sidebar_view(None, &st, cx);
                    self.sidebar_slide(inner, &st)
                });
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .children(side)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .pt(px(if side_tabs && !st.minimal {
                                crate::reader::TOP_INSET
                            } else {
                                0.0
                            }))
                            .child(home),
                    )
                    .into_any_element()
            }
        };
        let floating_strip = side_tabs && !st.minimal;
        let body = if floating_strip || st.minimal {
            // The drag strip and window buttons float over the page area. In
            // minimal mode they show only while the pointer is at the top edge.
            let peeking = !st.minimal || self.peek;
            let strip = self.top_strip(&st, cx);
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .relative()
                .child(body)
                .when(peeking, |d| {
                    d.child(
                        div()
                            .id("top-strip")
                            .absolute()
                            .top_0()
                            .right_0()
                            .left(px(if show_sidebar && side_tabs {
                                crate::layout::SIDEBAR_W
                            } else {
                                0.0
                            }))
                            .h(px(crate::reader::TOP_INSET))
                            .on_hover(cx.listener(|ws, over: &bool, _, cx| {
                                if !over && ws.peek {
                                    ws.peek = false;
                                    cx.notify();
                                }
                            }))
                            .child(strip),
                    )
                })
                // Minimal mode: a thin edge at the top brings the strip back.
                .when(st.minimal && !self.peek, |d| {
                    d.child(
                        div()
                            .id("peek-edge")
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(6.0))
                            .on_hover(cx.listener(|ws, over: &bool, _, cx| {
                                if *over {
                                    ws.peek = true;
                                    cx.notify();
                                }
                            })),
                    )
                })
                .into_any_element()
        } else {
            body
        };
        if self.tools.split.is_some() && self.tools.split.as_ref() == self.active_reader() {
            self.tools.split = None;
        }
        let overlays = self.overlays(&st, window, cx);
        self.booted = true;
        div()
            .id("workspace")
            .key_context(WORKSPACE)
            .track_focus(&self.focus)
            .relative()
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
            .on_action(cx.listener(|ws, _: &ToggleSidebar, _, cx| ws.toggle_sidebar(cx)))
            .on_action(cx.listener(|ws, _: &ToggleFileInfo, _, cx| {
                // Remembered across launches (settings.json `file_info_folded`).
                ws.change_style(
                    &|st: &mut Style| st.file_info_folded = !st.file_info_folded,
                    cx,
                );
            }))
            .on_action(cx.listener(|ws, _: &ToggleDock, _, cx| {
                // Remembered across launches (settings.json `dock_hidden`).
                ws.change_style(&|st: &mut Style| st.dock_hidden = !st.dock_hidden, cx);
            }))
            .on_action(cx.listener(|ws, _: &OpenSettings, window, cx| ws.open_settings(window, cx)))
            .on_action(
                cx.listener(|ws, _: &ShowShortcuts, window, cx| ws.show_shortcuts(window, cx)),
            )
            .on_action(cx.listener(|ws, _: &ToggleSplit, _, cx| ws.toggle_split(cx)))
            .on_action(cx.listener(|ws, _: &ExtractPages, window, cx| {
                if let Some(r) = ws.active_reader().cloned() {
                    if r.read(cx).is_pdf() {
                        ws.open_extract(r, window, cx);
                    }
                }
            }))
            .on_action(
                cx.listener(|ws, _: &ReopenClosedTab, window, cx| ws.reopen_closed(window, cx)),
            )
            .on_action(cx.listener(|ws, _: &SelectTab1, window, cx| ws.select_tab(0, window, cx)))
            .on_action(cx.listener(|ws, _: &SelectTab2, window, cx| ws.select_tab(1, window, cx)))
            .on_action(cx.listener(|ws, _: &SelectTab3, window, cx| ws.select_tab(2, window, cx)))
            .on_action(cx.listener(|ws, _: &SelectTab4, window, cx| ws.select_tab(3, window, cx)))
            .on_action(cx.listener(|ws, _: &SelectTab5, window, cx| ws.select_tab(4, window, cx)))
            .on_action(cx.listener(|ws, _: &SelectTab6, window, cx| ws.select_tab(5, window, cx)))
            .on_action(cx.listener(|ws, _: &SelectTab7, window, cx| ws.select_tab(6, window, cx)))
            .on_action(cx.listener(|ws, _: &SelectTab8, window, cx| ws.select_tab(7, window, cx)))
            .on_action(cx.listener(|ws, _: &SelectLastTab, window, cx| {
                let last = ws.tabs.len().saturating_sub(1);
                ws.select_tab(last, window, cx)
            }))
            .on_action(cx.listener(|ws, _: &CycleTheme, _, cx| ws.cycle_theme(cx)))
            .on_action(
                cx.listener(|ws, _: &CommandPalette, window, cx| ws.open_palette(window, cx)),
            )
            .on_action(cx.listener(|ws, _: &CheckForUpdates, window, cx| {
                ws.set_settings_section(Section::About, cx);
                ws.open_settings(window, cx);
                ws.check_for_updates(true, cx);
            }))
            .on_action(cx.listener(|ws, _: &ToggleMinimal, _, cx| ws.toggle_minimal(cx)))
            .on_drop(cx.listener(|ws, paths: &ExternalPaths, window, cx| {
                for p in paths.paths() {
                    ws.open_tab(p.clone(), window, cx);
                }
            }))
            .children(title_bar)
            .child(body)
            .children(overlays)
    }
}

// ── settings, menus and motion ───────────────────────────────────────────

impl Workspace {
    /// Change the look, redraw, and save settings.json off the UI thread.
    pub(crate) fn change_style(&mut self, f: &dyn Fn(&mut Style), cx: &mut Context<Self>) {
        f(cx.global_mut::<Style>());
        let json = Style::get(cx).settings().to_json();
        if cx.has_global::<SettingsSink>() {
            let sink = cx.global::<SettingsSink>().0.clone();
            cx.background_spawn(async move { sink(json) }).detach();
        }
        cx.refresh_windows();
    }

    /// Run `f` after `delay` (exit animations remove their element this way).
    fn after(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let timer = cx.background_executor().timer(delay);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |ws, cx| {
                f(ws, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Give keyboard focus back to the page (or Home) after an overlay closes.
    fn refocus(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.active_reader() {
            Some(r) => window.focus(&r.focus_handle(cx)),
            None => window.focus(&self.focus),
        }
    }

    /// Settings › Reading › Default app: register the file types for this
    /// user (off the UI thread), then open the system's default-apps page.
    pub(crate) fn make_default(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async { kitab_os::assoc::register() })
                .await;
            let _ = this.update(cx, |ws, cx| match result {
                Ok(()) => {
                    let page = kitab_os::assoc::DEFAULTS_PAGE;
                    if !page.is_empty() {
                        cx.open_url(page);
                    }
                    ws.toast(
                        "Kitab is set up for its file types. Pick it in Default apps.",
                        false,
                        None,
                        cx,
                    );
                    ws.load_system_info(Section::FileTypes, cx);
                }
                Err(e) => ws.toast(format!("Couldn't register Kitab: {e}"), true, None, cx),
            });
        })
        .detach();
    }

    /// F11: hide what Settings › Reading › Minimal mode lists, or show it again.
    pub(crate) fn toggle_minimal(&mut self, cx: &mut Context<Self>) {
        let style = cx.global_mut::<Style>();
        style.minimal = !style.minimal;
        self.peek = false;
        cx.refresh_windows();
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        // Remembered across launches (settings.json `sidebar_open`).
        let open = !(self.sidebar && !self.sidebar_closing);
        self.change_style(&move |st: &mut Style| st.sidebar_open = open, cx);
        let animate = Style::get(cx).animations;
        if self.sidebar && !self.sidebar_closing {
            if animate {
                self.sidebar_closing = true;
                let serial = self.sidebar_serial;
                self.after(motion::EXIT_BASE, cx, move |ws, _| {
                    if ws.sidebar_closing && ws.sidebar_serial == serial {
                        ws.sidebar = false;
                        ws.sidebar_closing = false;
                    }
                });
            } else {
                self.sidebar = false;
            }
        } else {
            self.sidebar = true;
            self.sidebar_closing = false;
            self.sidebar_serial += 1;
        }
        cx.notify();
    }

    /// The sidebar keeps its full width while it shows (the page reflows once,
    /// not every frame) and its content fades and slides 12 px (DESIGN.md:
    /// never animate layout sizes).
    fn sidebar_slide(&self, inner: AnyElement, st: &Style) -> AnyElement {
        let frame = div()
            .relative()
            .w(px(crate::layout::SIDEBAR_W))
            .h_full()
            .flex_none()
            .child(inner);
        if self.sidebar_closing {
            motion::exit(
                frame,
                ElementId::NamedInteger("sidebar-out".into(), self.sidebar_serial),
                motion::From::Left(12.0),
                motion::EXIT_BASE,
            )
        } else {
            motion::enter(
                frame,
                ElementId::NamedInteger("sidebar-in".into(), self.sidebar_serial),
                st.animations && self.sidebar_serial > 0,
                motion::From::Left(12.0),
                motion::BASE,
                Duration::ZERO,
            )
        }
    }

    /// Home cards rise in, one after another, each time Home is shown.
    fn card_enter(&self, card: AnyElement, name: &str, ix: usize, st: &Style) -> AnyElement {
        motion::enter(
            div().relative().child(card),
            ElementId::NamedInteger(format!("{name}-{}", self.home_serial).into(), ix as u64),
            st.animations && self.home_serial > 0,
            motion::From::Below(10.0),
            motion::BASE,
            motion::stagger(ix),
        )
    }

    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.themes.is_empty() {
            self.themes = kitab_theme::builtin_all().unwrap_or_default();
        }
        self.menu = None;
        self.menu_closing = None;
        self.settings = Some(self.settings_section);
        self.settings_closing = None;
        self.settings_serial += 1;
        self.system.confirm_clear = false;
        self.load_system_info(self.settings_section, cx);
        window.focus(&self.overlay_focus);
        cx.notify();
    }

    pub(crate) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(section) = self.settings.take() {
            if Style::get(cx).animations {
                self.settings_closing = Some(section);
                let serial = self.settings_serial;
                self.after(motion::EXIT_BASE, cx, move |ws, _| {
                    if ws.settings_serial == serial {
                        ws.settings_closing = None;
                    }
                });
            }
            self.refocus(window, cx);
            cx.notify();
        }
    }

    fn show_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_section = Section::Shortcuts;
        self.open_settings(window, cx);
    }

    pub(crate) fn edit_hex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = match Style::get(cx).accent {
            Accent::Custom(c) => c.to_string().trim_start_matches('#').to_string(),
            Accent::Preset(_) => String::new(),
        };
        self.hex.edit = Some(current);
        window.focus(&self.hex.focus);
        cx.notify();
    }

    /// Typing in the hex field: hex digits, Backspace, Enter to finish. Six
    /// digits apply at once.
    pub(crate) fn hex_key(
        &mut self,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.hex.edit.as_mut() else {
            return;
        };
        let k = &e.keystroke;
        if k.key == "enter" {
            self.hex.edit = None;
            window.focus(&self.overlay_focus);
        } else if k.key == "backspace" {
            text.pop();
        } else if let Some(ch) = k.key_char.as_ref() {
            let ch = ch.trim_start_matches('#');
            if !k.modifiers.control
                && ch.chars().all(|c| c.is_ascii_hexdigit())
                && text.len() + ch.len() <= 6
            {
                text.push_str(&ch.to_uppercase());
            }
        }
        let typed = self.hex.edit.clone().unwrap_or_default();
        if typed.len() == 6 {
            if let Some(Accent::Custom(c)) = Accent::parse(&format!("#{typed}")) {
                self.change_style(&move |s: &mut Style| s.set_accent(Accent::Custom(c)), cx);
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn set_settings_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.settings_section = section;
        if self.settings.is_some() {
            self.settings = Some(section);
        }
        self.system.confirm_clear = false;
        self.load_system_info(section, cx);
        cx.notify();
    }

    fn open_menu(
        &mut self,
        at: Point<Pixels>,
        align_right: bool,
        width: f32,
        items: Vec<MenuItem>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_serial += 1;
        self.menu_closing = None;
        self.menu = Some(OpenMenu {
            at,
            align_right,
            items,
            serial: self.menu_serial,
            width,
        });
        window.focus(&self.overlay_focus);
        cx.notify();
    }

    pub(crate) fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(m) = self.menu.take() {
            if Style::get(cx).animations {
                let serial = m.serial;
                self.menu_closing = Some(m);
                self.after(motion::EXIT_FAST, cx, move |ws, _| {
                    if ws.menu_closing.as_ref().is_some_and(|m| m.serial == serial) {
                        ws.menu_closing = None;
                    }
                });
            }
            self.refocus(window, cx);
            cx.notify();
        }
    }

    fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tools.extract.is_some() {
            self.close_extract(window, cx);
        } else if self.tools.save.is_some() {
            self.close_save_page(window, cx);
        } else if self.menu.is_some() {
            self.close_menu(window, cx);
        } else {
            self.close_settings(window, cx);
        }
    }

    /// A handler that runs `f` on the workspace from a menu row.
    fn on_ws(
        &self,
        cx: &mut Context<Self>,
        f: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl Fn(&mut Window, &mut App) + 'static {
        let ws = cx.weak_entity();
        move |window, cx| {
            let _ = ws.update(cx, |ws, cx| f(ws, window, cx));
        }
    }

    /// The ⋯ menu: theme and density rows, the contents panel, Settings.
    /// The ⋯ menu, opened from the dock's button at `at` (or the top-right corner).
    fn more_menu(
        &mut self,
        at: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.themes.is_empty() {
            self.themes = kitab_theme::builtin_all().unwrap_or_default();
        }
        let vp = window.viewport_size();
        let st = Style::get(cx).clone();
        let at = at.unwrap_or(point(vp.width - px(12.0), px(st.m().title_bar + 8.0)));
        // Open towards the middle of the window from wherever the button is.
        let align_right = at.x > vp.width / 2.0;
        let ws = cx.weak_entity();
        let themes = self.themes.clone();
        let theme_row = {
            let ws = ws.clone();
            MenuItem::Custom(Rc::new(move |st: &Style, _cx: &mut App| {
                let mut row = div().flex().gap(px(6.0)).px(px(10.0)).py(px(6.0));
                for t in &themes {
                    let selected = t.id == st.theme.id;
                    let theme = t.clone();
                    let ws = ws.clone();
                    row = row.child(
                        div()
                            .id(SharedString::from(format!("swatch-{}", t.id)))
                            .size(px(26.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .cursor_pointer()
                            .bg(crate::style::hsla(t.colors.bg))
                            .border_2()
                            .border_color(if selected {
                                st.c(|c| c.accent)
                            } else {
                                crate::style::hsla(t.colors.line)
                            })
                            .tip(t.name.clone())
                            .on_click(move |_, _, cx| {
                                let theme = theme.clone();
                                let _ = ws.update(cx, |ws, cx| {
                                    ws.change_style(
                                        &move |s: &mut Style| s.set_theme(theme.clone()),
                                        cx,
                                    )
                                });
                            })
                            .child(
                                div()
                                    .size(px(8.0))
                                    .rounded_full()
                                    .bg(crate::style::hsla(t.colors.accent)),
                            ),
                    );
                }
                row.into_any_element()
            }))
        };
        let density_row = {
            let ws = ws.clone();
            MenuItem::Custom(Rc::new(move |st: &Style, _cx: &mut App| {
                let mut group = div()
                    .mx(px(10.0))
                    .my(px(4.0))
                    .p(px(2.0))
                    .flex()
                    .gap(px(2.0))
                    .rounded(st.px(|m| m.control_radius))
                    .bg(st.c(|c| c.hover))
                    .shadow(st.well());
                for (ix, (d, label, glyph)) in [
                    (kitab_theme::Density::Dense, "Dense", "density-dense"),
                    (kitab_theme::Density::Compact, "Compact", "density-compact"),
                    (
                        kitab_theme::Density::Standard,
                        "Standard",
                        "density-standard",
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    let on = st.density == d;
                    let ws = ws.clone();
                    group = group.child(
                        div()
                            .id(("density-opt", ix))
                            .flex_1()
                            .h(px(26.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(5.0))
                            .rounded(px(6.0))
                            .text_size(px(12.0))
                            .text_color(if on {
                                st.c(|c| c.text)
                            } else {
                                st.c(|c| c.muted)
                            })
                            .when(on, |e| e.bg(st.c(|c| c.raised)).shadow(st.elev()))
                            .when(!on, |e| e.hover(|e| e.text_color(st.c(|c| c.text))))
                            .on_click(move |_, _, cx| {
                                let _ = ws.update(cx, |ws, cx| {
                                    ws.change_style(&move |s: &mut Style| s.density = d, cx)
                                });
                            })
                            .child(svg().path(icon(glyph)).size(px(13.0)).text_color(if on {
                                st.c(|c| c.accent)
                            } else {
                                st.c(|c| c.muted)
                            }))
                            .child(label),
                    );
                }
                group.into_any_element()
            }))
        };
        let accent_row = {
            let ws = ws.clone();
            MenuItem::Custom(Rc::new(move |st: &Style, _cx: &mut App| {
                let mut row = div().flex().gap(px(6.0)).px(px(10.0)).py(px(6.0));
                for (ix, p) in AccentPreset::ALL.into_iter().enumerate() {
                    let selected = st.accent == Accent::Preset(p);
                    let color = Accent::Preset(p).resolve(st.base_theme());
                    let ws = ws.clone();
                    row = row.child(
                        div()
                            .id(("accent-dot", ix))
                            .size(px(22.0))
                            .p(px(2.0))
                            .rounded_full()
                            .cursor_pointer()
                            .border_2()
                            .border_color(if selected {
                                st.c(|c| c.text)
                            } else {
                                st.ca(|c| c.line, 0.0)
                            })
                            .tip(p.name())
                            .on_click(move |_, _, cx| {
                                let _ = ws.update(cx, |ws, cx| {
                                    ws.change_style(
                                        &move |s: &mut Style| s.set_accent(Accent::Preset(p)),
                                        cx,
                                    )
                                });
                            })
                            .child(
                                div()
                                    .size_full()
                                    .rounded_full()
                                    .bg(crate::style::hsla(color)),
                            ),
                    );
                }
                row.into_any_element()
            }))
        };
        let items = vec![
            MenuItem::Heading("Theme".into()),
            theme_row,
            MenuItem::Heading("Accent".into()),
            accent_row,
            MenuItem::Heading("Density".into()),
            density_row,
            MenuItem::Separator,
            MenuItem::new(
                "Contents",
                self.on_ws(cx, |ws, _, cx| ws.toggle_sidebar(cx)),
            )
            .icon("sidebar")
            .key("F12")
            .checked(self.sidebar && !self.sidebar_closing),
            MenuItem::new(
                "Go to page…",
                self.on_ws(cx, |ws, window, cx| {
                    if let Some(r) = ws.active_reader().cloned() {
                        r.update(cx, |r, cx| r.edit_page(window, cx));
                    }
                }),
            )
            .icon("hash")
            .key("Ctrl+G")
            .disabled(self.active.is_none()),
            MenuItem::Separator,
            MenuItem::new(
                "Minimal mode",
                self.on_ws(cx, |ws, _, cx| ws.toggle_minimal(cx)),
            )
            .icon("focus")
            .key("F11")
            .checked(Style::get(cx).minimal),
            MenuItem::new(
                "Command palette",
                self.on_ws(cx, |ws, window, cx| ws.open_palette(window, cx)),
            )
            .icon("search")
            .key("Ctrl+K"),
            MenuItem::new(
                "Keyboard shortcuts",
                self.on_ws(cx, |ws, window, cx| ws.show_shortcuts(window, cx)),
            )
            .icon("keyboard")
            .key("Ctrl+/"),
            MenuItem::new(
                "Settings…",
                self.on_ws(cx, |ws, window, cx| ws.open_settings(window, cx)),
            )
            .icon("settings")
            .key("Ctrl+,"),
        ];
        self.open_menu(at, align_right, 280.0, items, window, cx);
    }

    /// Right click on a page.
    fn page_menu(
        &mut self,
        reader: Entity<Reader>,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = reader.read(cx);
        let path = r.path.clone();
        let (page, has_doc) = (r.current_page(), r.status == crate::reader::Status::Open);
        let (sel_preview, may_copy) = (r.selection_preview(), r.may_copy());
        let mode = r.zoom_mode();
        // One toolbar row (back/forward, zoom, rotate) that reads the reader's
        // state each frame and keeps the menu open, so it can be pressed again;
        // everything else is an ordinary row (DESIGN.md § Menus).
        let tool_row = {
            let reader = reader.clone();
            MenuItem::Custom(Rc::new(move |st: &Style, cx: &mut App| {
                let r = reader.read(cx);
                let (back, forward, percent) =
                    (r.can_go_back(), r.can_go_forward(), r.zoom_percent());
                let go = |f: fn(&mut Reader, &mut Context<Reader>)| {
                    let reader = reader.clone();
                    move |_: &mut Window, cx: &mut App| reader.update(cx, f)
                };
                menu::tool_row(
                    "menu-tools",
                    vec![
                        vec![
                            Tool::new(
                                "arrow-left",
                                "Back (Alt+Left)",
                                go(|r, cx| r.history(true, cx)),
                            )
                            .disabled(!back),
                            Tool::new(
                                "arrow-right",
                                "Forward (Alt+Right)",
                                go(|r, cx| r.history(false, cx)),
                            )
                            .disabled(!forward),
                        ],
                        vec![
                            Tool::new(
                                "minus",
                                "Zoom out (Ctrl+-)",
                                go(|r, cx| r.zoom_step(false, cx)),
                            ),
                            Tool::new(
                                "plus",
                                "Zoom in (Ctrl+=)",
                                go(|r, cx| r.zoom_step(true, cx)),
                            ),
                        ],
                        vec![
                            Tool::new(
                                "rotate-left",
                                "Rotate counter-clockwise (Ctrl+Shift+-)",
                                go(|r, cx| r.rotate(false, cx)),
                            ),
                            Tool::new(
                                "rotate",
                                "Rotate clockwise (Ctrl+Shift+=)",
                                go(|r, cx| r.rotate(true, cx)),
                            ),
                        ],
                    ],
                    Some(1),
                    Some(format!("{percent} %").into()),
                    st,
                )
            }))
        };
        let zoom = |label: &'static str, key: &'static str, m: ZoomMode| {
            let reader = reader.clone();
            MenuItem::new(label, move |_, cx: &mut App| {
                reader.update(cx, |r, cx| r.set_zoom_mode(m, cx))
            })
            .key(key)
            .checked(mode == m)
        };
        let mut items = vec![
            tool_row,
            MenuItem::Separator,
            zoom("Fit width", "Ctrl+2", ZoomMode::FitWidth),
            zoom("Fit page", "Ctrl+0", ZoomMode::FitPage),
            zoom("Actual size", "Ctrl+1", ZoomMode::ActualSize),
            MenuItem::Separator,
            MenuItem::new("Go to page…", {
                let reader = reader.clone();
                move |window: &mut Window, cx: &mut App| {
                    reader.update(cx, |r, cx| r.edit_page(window, cx))
                }
            })
            .icon("hash")
            .key("Ctrl+G"),
            {
                let marked = reader.read(cx).is_bookmarked(page as u32);
                let label = if marked {
                    "Remove bookmark"
                } else {
                    "Bookmark this page"
                };
                MenuItem::new(label, {
                    let reader = reader.clone();
                    move |_, cx: &mut App| {
                        reader.update(cx, |r, cx| {
                            r.jump_to(page as u32, cx);
                            r.toggle_bookmark(cx)
                        })
                    }
                })
                .icon("bookmark")
                .key("Ctrl+D")
                .disabled(!reader.read(cx).can_bookmark())
            },
            MenuItem::Separator,
        ];
        if let Some(preview) = sel_preview {
            items.push(
                MenuItem::new("Copy", {
                    let reader = reader.clone();
                    move |_, cx: &mut App| reader.update(cx, |r, cx| r.copy_selection(cx))
                })
                .icon("copy")
                .key("Ctrl+C")
                .disabled(!may_copy),
            );
            items.push(
                MenuItem::new(format!("Find “{preview}”"), {
                    let reader = reader.clone();
                    move |window: &mut Window, cx: &mut App| {
                        reader.update(cx, |r, cx| r.find_selection(window, cx))
                    }
                })
                .icon("search")
                .key("Ctrl+F"),
            );
            // Text markup from the selection (when markup is on here).
            if reader.read(cx).can_markup(cx) {
                for (label, kind, icon_name, key) in [
                    ("Highlight", "highlight", "highlight", Some("H")),
                    ("Underline", "underline", "minus", None),
                    ("Strike through", "strike", "line", None),
                ] {
                    let item = MenuItem::new(label, {
                        let reader = reader.clone();
                        move |_, cx: &mut App| reader.update(cx, |r, cx| r.mark_selection(kind, cx))
                    })
                    .icon(icon_name);
                    items.push(match key {
                        Some(k) => item.key(k),
                        None => item,
                    });
                }
            }
            items.push(MenuItem::Separator);
        }
        items.extend([
            MenuItem::new(format!("Copy text of page {}", page + 1), {
                let reader = reader.clone();
                move |_, cx: &mut App| reader.update(cx, |r, cx| r.copy_page_text(page, cx))
            })
            .icon("text")
            .disabled(!has_doc),
            MenuItem::new(format!("Copy page {} as image", page + 1), {
                let reader = reader.clone();
                move |_, cx: &mut App| reader.update(cx, |r, cx| r.copy_page_image(page, cx))
            })
            .icon("image")
            .disabled(!has_doc),
            MenuItem::Separator,
            MenuItem::new("Save page as…", {
                let reader = reader.clone();
                self.on_ws(cx, move |ws, window, cx| {
                    ws.open_save_page(reader.clone(), page, window, cx)
                })
            })
            .icon("file")
            .disabled(!has_doc),
        ]);
        items.extend(self.pdf_tool_items(&reader, cx));
        items.push(MenuItem::Separator);
        items.push(file_item_copy(&path));
        items.push(file_item_reveal(&path));
        self.open_menu(at, false, 300.0, items, window, cx);
    }

    /// Every open tab, the active one checked; the menu scrolls when long.
    fn all_tabs_menu(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let items: Vec<MenuItem> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(ix, t)| {
                MenuItem::new(
                    t.read(cx).title(),
                    self.on_ws(cx, move |ws, window, cx| ws.activate(ix, window, cx)),
                )
                .checked(self.active == Some(ix))
            })
            .collect();
        self.open_menu(at, false, 320.0, items, window, cx);
    }

    fn tab_menu(
        &mut self,
        ix: usize,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.tabs.get(ix).map(|t| t.read(cx).path.clone()) else {
            return;
        };
        let n = self.tabs.len();
        let items = vec![
            MenuItem::new(
                "Close",
                self.on_ws(cx, move |ws, window, cx| ws.close_tab(ix, window, cx)),
            )
            .icon("close")
            .key("Ctrl+W"),
            MenuItem::new(
                "Close other tabs",
                self.on_ws(cx, move |ws, window, cx| {
                    ws.close_where(window, cx, |i| i != ix)
                }),
            )
            .disabled(n < 2),
            MenuItem::new(
                "Close tabs to the right",
                self.on_ws(cx, move |ws, window, cx| {
                    ws.close_where(window, cx, |i| i > ix)
                }),
            )
            .disabled(ix + 1 >= n),
            MenuItem::Separator,
            MenuItem::new(
                "Reopen closed tab",
                self.on_ws(cx, |ws, window, cx| ws.reopen_closed(window, cx)),
            )
            .key("Ctrl+Shift+T")
            .disabled(self.closed.is_empty()),
            MenuItem::Separator,
        ];
        let mut items = items;
        if let Some(reader) = self.tabs.get(ix).cloned() {
            items.extend(self.pdf_tool_items(&reader, cx));
            items.push(MenuItem::Separator);
        }
        items.push(file_item_copy(&path));
        items.push(file_item_reveal(&path));
        self.open_menu(at, false, 300.0, items, window, cx);
    }

    fn card_menu(
        &mut self,
        path: PathBuf,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let exists = path.exists();
        let open = path.clone();
        let items = vec![
            MenuItem::new(
                "Open",
                self.on_ws(cx, move |ws, window, cx| {
                    ws.open_tab(open.clone(), window, cx)
                }),
            )
            .icon("book")
            .disabled(!exists),
            MenuItem::new("Open to the side", {
                let side = path.clone();
                self.on_ws(cx, move |ws, window, cx| {
                    ws.open_to_side(side.clone(), window, cx)
                })
            })
            .icon("sidebar")
            .disabled(!exists || self.tabs.is_empty()),
            MenuItem::Separator,
            file_item_copy(&path),
            file_item_reveal(&path).disabled(!exists),
        ];
        self.open_menu(at, false, 230.0, items, window, cx);
    }

    fn toc_menu(
        &mut self,
        reader: Entity<Reader>,
        page: Option<u32>,
        title: String,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = match page {
            Some(p) => format!("Go to page {}", p + 1),
            None => "Go to section".to_string(),
        };
        let items = vec![
            MenuItem::new(label, move |_, cx: &mut App| {
                if let Some(p) = page {
                    reader.update(cx, |r, cx| r.jump_to(p, cx));
                }
            })
            .icon("chevron-right")
            .disabled(page.is_none()),
            MenuItem::new("Copy title", move |_, cx: &mut App| {
                cx.write_to_clipboard(ClipboardItem::new_string(title.clone()))
            })
            .icon("copy"),
        ];
        self.open_menu(at, false, 220.0, items, window, cx);
    }

    fn remember_closed(&mut self, path: PathBuf) {
        self.closed.retain(|p| *p != path);
        self.closed.push(path);
        if self.closed.len() > CLOSED_MAX {
            self.closed.remove(0);
        }
    }

    fn reopen_closed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.closed.pop() {
            self.open_tab(path, window, cx);
        }
    }

    fn select_tab(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if ix < self.tabs.len() {
            self.activate(ix, window, cx);
        }
    }

    /// Close every tab whose index matches `close`.
    fn close_where(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        close: impl Fn(usize) -> bool,
    ) {
        let active = self.active.and_then(|i| self.tabs.get(i).cloned());
        let tabs = std::mem::take(&mut self.tabs);
        for (i, t) in tabs.into_iter().enumerate() {
            if close(i) {
                if self.tools.split.as_ref() == Some(&t) {
                    self.tools.split = None;
                }
                t.update(cx, |r, cx| r.save_now(cx));
                self.remember_closed(t.read(cx).path.clone());
            } else {
                self.tabs.push(t);
            }
        }
        let keep = active.and_then(|a| self.tabs.iter().position(|t| *t == a));
        match keep.or(if self.tabs.is_empty() { None } else { Some(0) }) {
            Some(i) => self.activate(i, window, cx),
            None => self.show_home(window, cx),
        }
    }

    /// The open menu and Settings, over everything.
    fn overlays(
        &mut self,
        st: &Style,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out = Vec::new();
        let ws = cx.weak_entity();
        out.extend(self.toast_view(st, cx));
        out.extend(self.extract_view(st, window, cx));
        out.extend(self.save_page_view(st, window, cx));
        out.extend(self.update_dialog(st, cx));
        out.extend(self.palette_view(st, cx));

        if let Some(section) = self.settings.or(self.settings_closing) {
            let closing = self.settings.is_none();
            let serial = self.settings_serial;
            let scrim = div().absolute().inset_0().bg(st.c(|c| c.scrim));
            let panel = div().relative().child(settings_view::panel(
                ws.clone(),
                section,
                st,
                &self.themes,
                &self.hex,
                &self.system,
                window,
            ));
            let (scrim, panel) = if closing {
                (
                    motion::exit(
                        scrim,
                        ElementId::NamedInteger("scrim-out".into(), serial),
                        motion::From::Fade,
                        motion::EXIT_BASE,
                    ),
                    motion::exit(
                        panel,
                        ElementId::NamedInteger("settings-out".into(), serial),
                        motion::From::Below(10.0),
                        motion::EXIT_BASE,
                    ),
                )
            } else {
                (
                    motion::enter(
                        scrim,
                        ElementId::NamedInteger("scrim-in".into(), serial),
                        st.animations,
                        motion::From::Fade,
                        motion::BASE,
                        Duration::ZERO,
                    ),
                    motion::enter(
                        panel,
                        ElementId::NamedInteger("settings-in".into(), serial),
                        st.animations,
                        motion::From::Below(10.0),
                        motion::BASE,
                        Duration::ZERO,
                    ),
                )
            };
            let mut layer = div()
                .id("settings-layer")
                // Nothing under the dialog gets the mouse or the wheel.
                .occlude()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(scrim)
                .child(panel);
            if !closing {
                layer =
                    layer
                        .key_context(OVERLAY)
                        .track_focus(&self.overlay_focus)
                        .on_action(cx.listener(|ws, _: &CloseOverlay, window, cx| {
                            ws.close_overlay(window, cx)
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, window, cx| ws.close_settings(window, cx)),
                        );
            }
            out.push(layer.into_any_element());
        }

        if let Some(m) = self.menu.clone().or_else(|| self.menu_closing.clone()) {
            let closing = self.menu.is_none();
            let close: menu::Handler = {
                let ws = ws.clone();
                Rc::new(move |window, cx| {
                    let _ = ws.update(cx, |ws, cx| ws.close_menu(window, cx));
                })
            };
            // Leave the menu's margin free above and below in a short window.
            let max_h = window.viewport_size().height - px(16.0);
            let panel = div()
                .relative()
                .child(menu::panel(&m, st, close, max_h, cx));
            let panel = if closing {
                motion::exit(
                    panel,
                    ElementId::NamedInteger("menu-out".into(), m.serial),
                    motion::From::Above(4.0),
                    motion::EXIT_FAST,
                )
            } else {
                motion::enter(
                    panel,
                    ElementId::NamedInteger("menu-in".into(), m.serial),
                    st.animations,
                    motion::From::Above(4.0),
                    motion::FAST,
                    Duration::ZERO,
                )
            };
            let positioned = deferred(
                anchored()
                    .position(m.at)
                    .anchor(if m.align_right {
                        gpui::Corner::TopRight
                    } else {
                        gpui::Corner::TopLeft
                    })
                    .snap_to_window_with_margin(px(8.0))
                    .child(panel),
            )
            .with_priority(1);
            let mut layer = div().id("menu-layer").absolute().inset_0().occlude();
            if !closing {
                // A click anywhere else closes the menu (and doesn't reach what's under it).
                layer =
                    layer
                        .key_context(OVERLAY)
                        .track_focus(&self.overlay_focus)
                        .on_action(cx.listener(|ws, _: &CloseOverlay, window, cx| {
                            ws.close_overlay(window, cx)
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, window, cx| ws.close_menu(window, cx)),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|ws, _, window, cx| ws.close_menu(window, cx)),
                        );
            }
            out.push(layer.child(positioned).into_any_element());
        }
        out
    }
}

fn file_item_copy(path: &Path) -> MenuItem {
    let text = path.display().to_string();
    MenuItem::new("Copy file path", move |_, cx: &mut App| {
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
    })
    .icon("copy")
}

fn file_item_reveal(path: &Path) -> MenuItem {
    let path = path.to_path_buf();
    MenuItem::new(
        if cfg!(target_os = "macos") {
            "Show in Finder"
        } else {
            "Show in folder"
        },
        move |_, cx: &mut App| {
            let path = path.clone();
            cx.background_spawn(async move {
                if let Err(e) = kitab_os::reveal_in_folder(&path) {
                    log::warn!("could not open the file manager: {e}");
                }
            })
            .detach();
        },
    )
    .icon("folder-open")
}

/// The tab's icon for a file's format (PDF, Markdown, books, comics, text, XPS).
/// A document's icon by its format (tabs, Settings › File types).
pub(crate) fn format_icon(path: &std::path::Path) -> &'static str {
    tab_icon_for(path)
}

fn tab_icon_for(path: &std::path::Path) -> &'static str {
    use kitab_core::Format;
    let format = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(Format::from_extension);
    match format {
        Some(Format::Markdown) => "markdown",
        Some(Format::Epub | Format::Mobi | Format::Fb2) => "book",
        Some(Format::Cbz) => "image",
        Some(Format::Txt) => "text",
        Some(Format::Xps) => "pages",
        _ => "file",
    }
}

#[cfg(test)]
mod tab_icon_tests {
    #[test]
    fn each_format_has_its_icon() {
        let icon = |p: &str| super::tab_icon_for(std::path::Path::new(p));
        assert_eq!(icon("a.pdf"), "file");
        assert_eq!(icon("a.MD"), "markdown");
        assert_eq!(icon("a.epub"), "book");
        assert_eq!(icon("a.fb2"), "book");
        assert_eq!(icon("a.mobi"), "book");
        assert_eq!(icon("a.cbz"), "image");
        assert_eq!(icon("a.txt"), "text");
        assert_eq!(icon("a.xps"), "pages");
    }
}
