// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reader buttons in the title bar, beside the window buttons (Settings ›
//! Appearance › Toolbar puts each one in the title bar, the dock, or
//! nowhere). They act on the active tab; on Home only More shows. They sit in
//! a fixed slot, so the tab strip shares what's left and scrolls as before.

use super::{Workspace, icon_button};
use crate::actions::*;
use crate::settings::TitleItems;
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    Action, AnyElement, Context, Focusable, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled, Window, div, prelude::FluentBuilder, px,
};
use kitab_core::ZoomMode;

/// Buttons in the title bar for `t`, with a document open or not.
pub(super) fn count(t: TitleItems, has_doc: bool) -> usize {
    if !has_doc {
        return usize::from(t.more) + usize::from(t.open);
    }
    [
        t.sidebar, t.find, t.bookmark, t.markup, t.rotate, t.minimal, t.print, t.password, t.more,
        t.open,
    ]
    .iter()
    .filter(|b| **b)
    .count()
        + 2 * usize::from(t.history)
        + 2 * usize::from(t.zoom)
        + 2 * usize::from(t.fit)
}

impl Workspace {
    /// Width the actions take, for the tab strip's arithmetic.
    pub(super) fn title_actions_width(&self, st: &Style, crowded: bool) -> f32 {
        let n = count(Self::shown_items(st, crowded), self.active.is_some());
        if n == 0 {
            0.0
        } else {
            n as f32 * (st.m().icon_button + 2.0) + 12.0
        }
    }

    /// Run a reader action on the active tab: focus it first, so the action
    /// reaches it rather than whatever had focus.
    fn on_reader(
        action: Box<dyn Action>,
    ) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) {
        move |_, window, cx| {
            window.dispatch_action(action.boxed_clone(), cx);
        }
    }

    /// The items to show: all of them, or only More when the tabs would be
    /// squeezed out (their actions stay on their keys and in the palette).
    fn shown_items(st: &Style, crowded: bool) -> TitleItems {
        if crowded {
            TitleItems {
                more: st.title_items.more,
                open: st.title_items.open,
                ..TitleItems::NONE
            }
        } else {
            st.title_items
        }
    }

    pub(super) fn title_actions(
        &self,
        st: &Style,
        crowded: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let t = Self::shown_items(st, crowded);
        let reader = self.active_reader().cloned();
        if count(t, reader.is_some()) == 0 {
            return None;
        }
        let mut row = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.0))
            .mr(px(10.0));
        if let Some(reader) = reader.as_ref() {
            // Print and Password open the workspace's own sheets: called
            // directly, as an action dispatched just after the focus moves
            // to the reader doesn't reach the workspace.
            let open_print =
                cx.listener(|ws, _: &gpui::ClickEvent, window, cx| ws.open_print(window, cx));
            let open_password =
                cx.listener(|ws, _: &gpui::ClickEvent, window, cx| ws.open_password(window, cx));
            let r = reader.read(cx);
            let focus = r.focus_handle(cx);
            let refocus = move |window: &mut Window| window.focus(&focus);
            let button = |id: &'static str,
                          name: &'static str,
                          on: bool,
                          tip: &'static str,
                          action: Box<dyn Action>| {
                let refocus = refocus.clone();
                let run = Self::on_reader(action);
                icon_button(id, name, on, st)
                    .flex_none()
                    .tip(tip)
                    .on_click(move |e, window, cx| {
                        refocus(window);
                        run(e, window, cx);
                    })
            };
            let mode = r.zoom_mode();
            let page = r.current_page() as u32;
            row = row
                .when(t.sidebar, |d| {
                    d.child(button(
                        "t-sidebar",
                        "sidebar",
                        self.sidebar,
                        "Sidebar  Ctrl+B",
                        Box::new(ToggleSidebar),
                    ))
                })
                .when(t.history, |d| {
                    d.child(button(
                        "t-back",
                        "arrow-left",
                        false,
                        "Back  Alt+Left",
                        Box::new(HistoryBack),
                    ))
                    .child(button(
                        "t-forward",
                        "arrow-right",
                        false,
                        "Forward  Alt+Right",
                        Box::new(HistoryForward),
                    ))
                })
                .when(t.zoom, |d| {
                    d.child(button(
                        "t-zoom-out",
                        "minus",
                        false,
                        "Zoom out  Ctrl+-",
                        Box::new(ZoomOut),
                    ))
                    .child(button(
                        "t-zoom-in",
                        "plus",
                        false,
                        "Zoom in  Ctrl+=",
                        Box::new(ZoomIn),
                    ))
                })
                .when(t.fit, |d| {
                    d.child(button(
                        "t-fit-width",
                        "fit-width",
                        mode == ZoomMode::FitWidth,
                        "Fit width  Ctrl+2",
                        Box::new(FitWidth),
                    ))
                    .child(button(
                        "t-fit-page",
                        "fit-page",
                        mode == ZoomMode::FitPage,
                        "Fit page  Ctrl+0",
                        Box::new(FitPage),
                    ))
                })
                .when(t.find, |d| {
                    d.child(button(
                        "t-find",
                        "search",
                        false,
                        "Find  Ctrl+F",
                        Box::new(Find),
                    ))
                })
                .when(t.bookmark && r.can_bookmark(), |d| {
                    d.child(button(
                        "t-bookmark",
                        "bookmark",
                        r.is_bookmarked(page),
                        "Bookmark this page  Ctrl+D",
                        Box::new(ToggleBookmark),
                    ))
                })
                .when(t.markup && r.can_markup(cx), |d| {
                    d.child(button(
                        "t-markup",
                        "pen",
                        r.markup_on(),
                        "Draw and mark up  Ctrl+Shift+D",
                        Box::new(ToggleMarkup),
                    ))
                })
                .when(t.rotate, |d| {
                    d.child(button(
                        "t-rotate",
                        "rotate",
                        false,
                        "Rotate  Ctrl+Shift+=",
                        Box::new(RotateRight),
                    ))
                })
                .when(t.print, |d| {
                    d.child(
                        icon_button("t-print", "print", false, st)
                            .flex_none()
                            .tip("Print  Ctrl+P")
                            .on_click(open_print),
                    )
                })
                .when(t.password && r.is_pdf(), |d| {
                    d.child(
                        icon_button("t-password", "lock", false, st)
                            .flex_none()
                            .tip("Password")
                            .on_click(open_password),
                    )
                })
                .when(t.minimal, |d| {
                    d.child(button(
                        "t-minimal",
                        "focus",
                        st.minimal,
                        "Focus mode  F11",
                        Box::new(ToggleMinimal),
                    ))
                });
        }
        // Open file… stands on its own: there's always something to open.
        if t.open {
            row = row.child(
                icon_button("t-open", "folder-open", false, st)
                    .flex_none()
                    .tip("Open file…  Ctrl+O")
                    .on_click(cx.listener(|ws, _: &gpui::ClickEvent, window, cx| {
                        ws.open_dialog(window, cx)
                    })),
            );
        }
        if t.more {
            row = row.child(
                icon_button("t-more", "more-vertical", false, st)
                    .flex_none()
                    .tip("More")
                    .on_click(cx.listener(|ws, e: &gpui::ClickEvent, window, cx| {
                        let at = e.position();
                        ws.more_menu(Some(gpui::point(at.x, at.y + px(14.0))), window, cx)
                    })),
            );
        }
        Some(row.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_shows_only_more() {
        let t = TitleItems::default();
        // More and Open file… on Home.
        assert_eq!(count(t, false), 2);
        assert!(count(t, true) > 1);
        let none = TitleItems {
            more: false,
            ..TitleItems::NONE
        };
        assert_eq!(count(none, true), 0);
    }
}
