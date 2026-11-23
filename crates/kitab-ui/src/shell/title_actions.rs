// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reader buttons in the title bar, beside the window buttons, in the order
//! Settings › Appearance › Toolbar gives them (GitHub #25). They act on the
//! active tab; on Home only Open file and More show. They sit in a fixed
//! slot, so the tab strip shares what's left and scrolls as before.

use super::{Workspace, icon_button};
use crate::reader::tools::{Click, ToolButton};
use crate::style::Style;
use crate::toolbar::Tool;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, Context, Focusable, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

/// The tools the title bar shows: all of its list, or, when the tabs would
/// be squeezed out, only Open file and More (the rest go to the dock).
pub(crate) fn shown_tools(toolbar: &crate::toolbar::Toolbar, crowded: bool) -> Vec<Tool> {
    toolbar
        .title
        .iter()
        .copied()
        .filter(|t| !crowded || matches!(t, Tool::Open | Tool::More))
        .collect()
}

/// On Home there's no tab to act on: only these make sense.
fn on_home(t: Tool) -> bool {
    matches!(t, Tool::Open | Tool::More)
}

impl Workspace {
    /// The buttons each shown tool puts up, for the active tab (Home: Open
    /// file and More only).
    fn title_buttons(&self, st: &Style, crowded: bool, cx: &gpui::App) -> Vec<(Tool, ToolButton)> {
        let reader = self.active_reader();
        let mut out = Vec::new();
        for tool in shown_tools(&st.toolbar, crowded) {
            match reader {
                Some(r) => out.extend(
                    r.read(cx)
                        .tool_buttons(tool, cx)
                        .into_iter()
                        .map(|b| (tool, b)),
                ),
                None if on_home(tool) => out.push((
                    tool,
                    ToolButton {
                        id: format!("tool-{tool:?}-0").into(),
                        icon: Some(tool.icon()),
                        text: String::new(),
                        tip: if tool == Tool::Open {
                            "Open file…  Ctrl+O".into()
                        } else {
                            "More".into()
                        },
                        on: false,
                        enabled: true,
                        click: if tool == Tool::More {
                            Click::MoreMenu
                        } else {
                            Click::Act(Box::new(crate::actions::OpenFile))
                        },
                    },
                )),
                None => {}
            }
        }
        out
    }

    /// Width the actions take, for the tab strip's arithmetic.
    pub(super) fn title_actions_width(&self, st: &Style, crowded: bool, cx: &gpui::App) -> f32 {
        let buttons = self.title_buttons(st, crowded, cx);
        if buttons.is_empty() {
            return 0.0;
        }
        let w: f32 = buttons
            .iter()
            .map(|(_, b)| if b.icon.is_some() { st.m().icon_button } else { 46.0 } + 2.0)
            .sum();
        w + 12.0
    }

    pub(super) fn title_actions(
        &self,
        st: &Style,
        crowded: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let buttons = self.title_buttons(st, crowded, cx);
        if buttons.is_empty() {
            return None;
        }
        let reader = self.active_reader().cloned();
        let focus = reader.as_ref().map(|r| r.read(cx).focus_handle(cx));
        let mut row = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.0))
            .mr(px(10.0));
        for (tool, b) in buttons {
            let el = match b.icon {
                Some(name) => icon_button(b.id.clone(), name, b.on, st).flex_none(),
                // Zoom's percentage: a small text button (Actual size).
                None => crate::kbd::stop(div().id(b.id.clone()), st)
                    .h(st.px(|m| m.icon_button))
                    .min_w(px(46.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(st.px(|m| m.control_radius))
                    .text_size(px(12.0))
                    .text_color(st.c(|c| c.text))
                    .hover(|d| d.bg(st.c(|c| c.hover)))
                    .child(b.text.clone()),
            }
            .tip(b.tip.clone())
            .when(!b.enabled, |d| d.opacity(0.4));
            if !b.enabled {
                row = row.child(el);
                continue;
            }
            let el = match b.click {
                Click::SpeechMenu => {
                    let Some(r) = reader.clone() else { continue };
                    el.on_click(cx.listener(move |ws, e: &gpui::ClickEvent, window, cx| {
                        let at = e.position();
                        ws.speech_menu(r.clone(), gpui::point(at.x, at.y + px(14.0)), window, cx)
                    }))
                }
                Click::MoreMenu => {
                    el.on_click(cx.listener(|ws, e: &gpui::ClickEvent, window, cx| {
                        let at = e.position();
                        ws.more_menu(Some(gpui::point(at.x, at.y + px(14.0))), window, cx)
                    }))
                }
                // The workspace's own sheets and dialogs: called directly, as
                // an action dispatched just after the focus moves to the
                // reader doesn't reach the workspace.
                Click::Act(_) if matches!(tool, Tool::Print) => el.on_click(
                    cx.listener(|ws, _: &gpui::ClickEvent, window, cx| ws.open_print(window, cx)),
                ),
                Click::Act(_) if matches!(tool, Tool::Password) => {
                    el.on_click(cx.listener(|ws, _: &gpui::ClickEvent, window, cx| {
                        ws.open_password(window, cx)
                    }))
                }
                Click::Act(_) if matches!(tool, Tool::Open) => {
                    el.on_click(cx.listener(|ws, _: &gpui::ClickEvent, window, cx| {
                        ws.open_dialog(window, cx)
                    }))
                }
                Click::Act(_) if matches!(tool, Tool::Present) => {
                    el.on_click(cx.listener(|ws, _: &gpui::ClickEvent, window, cx| {
                        ws.toggle_present(window, cx)
                    }))
                }
                Click::Act(_) if matches!(tool, Tool::Export) => {
                    el.on_click(cx.listener(|ws, _: &gpui::ClickEvent, _, cx| {
                        if let Some(r) = ws.active_reader().cloned() {
                            ws.export_document(r, crate::ExportKind::Markdown, cx);
                        }
                    }))
                }
                // A reader action: focus the tab first, so it reaches it.
                Click::Act(action) => {
                    let focus = focus.clone();
                    el.on_click(move |_, window, cx| {
                        if let Some(f) = &focus {
                            window.focus(f);
                        }
                        window.dispatch_action(action.boxed_clone(), cx);
                    })
                }
            };
            row = row.child(el);
        }
        Some(row.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_squeezed_title_bar_keeps_open_and_more() {
        let t = crate::toolbar::Toolbar::default();
        assert_eq!(shown_tools(&t, true), [Tool::Open, Tool::More]);
        assert_eq!(shown_tools(&t, false), t.title);
    }
}
