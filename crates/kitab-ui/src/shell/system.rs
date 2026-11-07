// SPDX-License-Identifier: AGPL-3.0-or-later

//! The workspace's side of Settings › File types, Advanced and About: file
//! association status, cache and history clearing, database upkeep, and the
//! update check with its title-bar pill and dialog. Everything that touches
//! the disk, the registry or the network runs off the UI thread.

use super::Workspace;
use super::tools_ui::dialog_button;
use crate::icons::icon;
use crate::settings_view::{Section, UpdateState};
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, AppContext, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
    svg,
};
use kitab_os::update::{self, NoteBlock};
use std::time::Duration;

/// The running version (the workspace's one version, AGENTS.md).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The first automatic check waits this long after start-up (AGENTS.md rule 1:
/// nothing extra on the start-up path).
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(10);
/// Then once every six hours while Kitab stays open.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

impl Workspace {
    // ── settings pages ──

    /// Load what File types and Advanced show (registry, database size) when
    /// one of them opens.
    pub(crate) fn load_system_info(&mut self, section: Section, cx: &mut Context<Self>) {
        match section {
            Section::FileTypes => {
                cx.spawn(async move |this, cx| {
                    let status = cx
                        .background_spawn(async { kitab_os::assoc::status() })
                        .await;
                    let _ = this.update(cx, |ws, cx| {
                        ws.system.types = Some(status);
                        cx.notify();
                    });
                })
                .detach();
            }
            Section::Advanced => self.load_db_size(cx),
            _ => {}
        }
    }

    fn load_db_size(&mut self, cx: &mut Context<Self>) {
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let size = cx
                .background_spawn(async move { library.size_bytes() })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.system.db_size = size;
                cx.notify();
            });
        })
        .detach();
    }

    /// File types › Set up (one row): register that format only, then show
    /// the system's default-apps page so the user can pick Kitab for it.
    pub(crate) fn set_up_type(&mut self, index: usize, cx: &mut Context<Self>) {
        let name = kitab_os::assoc::FILE_TYPES
            .get(index)
            .map_or("file", |t| t.description);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let r = kitab_os::assoc::register_one(index);
                    (r, kitab_os::assoc::status())
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                let (r, status) = result;
                ws.system.types = Some(status);
                match r {
                    Ok(()) => {
                        ws.open_default_apps(cx);
                        let ask = if kitab_os::assoc::DEFAULTS_PAGE.is_empty() {
                            ""
                        } else {
                            " Pick Kitab for it in Default apps."
                        };
                        ws.toast(
                            format!("Kitab is set up for {name}s.{ask}"),
                            false,
                            None,
                            cx,
                        );
                    }
                    Err(e) => ws.toast(format!("Couldn't set up {name}s: {e}"), true, None, cx),
                }
            });
        })
        .detach();
    }

    /// File types › Remove: take Kitab's file types and thumbnails off this
    /// account.
    pub(crate) fn unregister_types(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let status = cx
                .background_spawn(async {
                    kitab_os::assoc::unregister();
                    kitab_os::assoc::status()
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.system.types = Some(status);
                ws.toast("Kitab no longer opens these files.", false, None, cx);
            });
        })
        .detach();
    }

    /// File types › Default apps: the system's page.
    pub(crate) fn open_default_apps(&mut self, cx: &mut Context<Self>) {
        let page = kitab_os::assoc::DEFAULTS_PAGE;
        if !page.is_empty() {
            cx.open_url(page);
        }
    }

    /// Advanced › Clear cached pages: every tab's rendered pages, and the
    /// engine's decoded fonts and images. Visible pages render again.
    pub(crate) fn clear_caches(&mut self, cx: &mut Context<Self>) {
        for tab in self.tabs.clone() {
            tab.update(cx, |r, cx| r.drop_cached_pages(cx));
        }
        let engine = self.engine.clone();
        cx.background_spawn(async move { engine.shrink_caches() })
            .detach();
        self.toast("Cached pages cleared.", false, None, cx);
    }

    /// Advanced › Clear reading history, after the row's confirmation.
    pub(crate) fn clear_history(&mut self, cx: &mut Context<Self>) {
        self.system.confirm_clear = false;
        self.closed.clear();
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let ok = cx
                .background_spawn(async move { library.clear_history() })
                .await;
            let _ = this.update(cx, |ws, cx| {
                if ok {
                    ws.refresh_recent(cx);
                    ws.toast("Reading history cleared.", false, None, cx);
                } else {
                    ws.toast("Couldn't clear the history.", true, None, cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn ask_clear_history(&mut self, ask: bool, cx: &mut Context<Self>) {
        self.system.confirm_clear = ask;
        cx.notify();
    }

    /// Advanced › Optimize: compact the library database.
    pub(crate) fn optimize_db(&mut self, cx: &mut Context<Self>) {
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let sizes = cx.background_spawn(async move { library.optimize() }).await;
            let _ = this.update(cx, |ws, cx| match sizes {
                Some((before, after)) => {
                    ws.system.db_size = Some(after);
                    ws.toast(
                        format!(
                            "Database optimized: {} → {}.",
                            crate::reader::human_size(before),
                            crate::reader::human_size(after)
                        ),
                        false,
                        None,
                        cx,
                    );
                }
                None => ws.toast("Couldn't optimize the database.", true, None, cx),
            });
        })
        .detach();
    }

    /// Advanced › Data folder: show it in the file manager.
    pub(crate) fn open_data_folder(&mut self, cx: &mut Context<Self>) {
        if let Some(dir) = kitab_os::data_dir() {
            cx.background_spawn(async move {
                let _ = kitab_os::reveal_in_folder(&dir);
            })
            .detach();
        }
    }

    // ── updates ──

    /// Start-up: the first automatic check, 10 s after the window opened, if
    /// Settings › About › Check on start-up is on.
    pub(super) fn schedule_update_check(&mut self, delay: Duration, cx: &mut Context<Self>) {
        self.after(delay, cx, |ws, cx| {
            if Style::get(cx).update_check {
                ws.check_for_updates(false, cx);
            }
            ws.schedule_update_check(CHECK_EVERY, cx);
        });
    }

    pub(super) fn first_update_check(&mut self, cx: &mut Context<Self>) {
        self.schedule_update_check(FIRST_CHECK_AFTER, cx);
    }

    /// Ask GitHub for the latest release. `manual` (About › Check now) shows
    /// progress and errors; an automatic check stays quiet unless there's an
    /// update.
    pub(crate) fn check_for_updates(&mut self, manual: bool, cx: &mut Context<Self>) {
        if matches!(self.system.update, UpdateState::Checking) {
            return;
        }
        let before = std::mem::replace(&mut self.system.update, UpdateState::Checking);
        if !manual {
            // Keep showing what was known while a quiet check runs.
            self.system.update = before.clone();
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async { update::fetch_latest() }).await;
            let _ = this.update(cx, |ws, cx| {
                ws.system.update = match result {
                    Ok(r) if update::is_newer(&r, VERSION) => UpdateState::Available(r),
                    Ok(_) => UpdateState::UpToDate,
                    Err(e) if manual => UpdateState::Failed(e),
                    Err(_) => before,
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn open_update(&mut self, cx: &mut Context<Self>) {
        self.system.update_open = true;
        self.system.update_serial += 1;
        cx.notify();
    }

    fn close_update(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.system.update_open = false;
        cx.notify();
    }

    /// Later: hide this version's pill until a newer one comes out.
    fn skip_update(&mut self, cx: &mut Context<Self>) {
        if let UpdateState::Available(r) = &self.system.update {
            let v = r.version.clone();
            self.change_style(&move |s: &mut Style| s.skipped_update = v.clone(), cx);
        }
        self.system.update_open = false;
        cx.notify();
    }

    /// Download: the release page in the browser. Kitab never installs
    /// anything by itself.
    fn download_update(&mut self, cx: &mut Context<Self>) {
        if let UpdateState::Available(r) = &self.system.update {
            cx.open_url(&r.url);
        }
        self.system.update_open = false;
        cx.notify();
    }

    /// The title bar's "Update" pill, when a newer release is out and wasn't
    /// put off with Later.
    pub(super) fn update_pill(&self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        let UpdateState::Available(r) = &self.system.update else {
            return None;
        };
        if r.version == st.skipped_update {
            return None;
        }
        Some(
            div()
                .id("update-pill")
                .flex_none()
                .flex()
                .items_center()
                .gap(px(5.0))
                .h(px(24.0))
                .pl(px(8.0))
                .pr(px(10.0))
                .mr(px(6.0))
                .rounded_full()
                .border_1()
                .border_color(st.c(|c| c.accent))
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(st.c(|c| c.accent))
                .cursor_pointer()
                .hover(|d| d.bg(st.c(|c| c.hover)))
                .tip(format!("Kitab {} is available", r.version))
                .on_click(cx.listener(|ws, _, _, cx| ws.open_update(cx)))
                .child(
                    svg()
                        .path(icon("arrow-down"))
                        .size(px(12.0))
                        .text_color(st.c(|c| c.accent)),
                )
                .child("Update")
                .into_any_element(),
        )
    }

    /// "Kitab X is available": the release notes, Later and Download.
    pub(super) fn update_dialog(
        &mut self,
        st: &Style,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.system.update_open {
            return None;
        }
        let UpdateState::Available(r) = &self.system.update else {
            return None;
        };
        let blocks = update::note_blocks(&r.notes, 60);
        let mut notes = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(14.0))
            .py(px(12.0));
        for b in blocks.iter() {
            notes = notes.child(match b {
                NoteBlock::Heading(t) => div()
                    .pt(px(4.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(st.c(|c| c.text))
                    .child(t.clone()),
                NoteBlock::Bullet(t) => div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .mt(px(7.0))
                            .size(px(4.0))
                            .flex_none()
                            .rounded_full()
                            .bg(st.c(|c| c.faint)),
                    )
                    .child(div().min_w_0().child(t.clone())),
                NoteBlock::Text(t) => div().child(t.clone()),
            });
        }
        let released = if r.published.is_empty() {
            String::new()
        } else {
            format!(", released {}", r.published)
        };
        let panel = div()
            .id("update-dialog")
            .w(px(460.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(22.0))
            .rounded(px(16.0))
            .bg(st.c(|c| c.raised))
            .shadow(st.modal())
            .border_1()
            .border_color(st.c(|c| c.line))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(st.c(|c| c.text))
                    .child(format!("Kitab {} is available", r.version)),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(st.c(|c| c.muted))
                    .child(format!(
                        "You have {VERSION}{released}. Download the new installer to update; your library, marks and settings are kept."
                    )),
            )
            .when(!blocks.is_empty(), |d| {
                d.child(
                    div()
                        .id("update-notes")
                        .max_h(px(260.0))
                        .overflow_y_scroll()
                        .rounded(px(10.0))
                        .bg(st.c(|c| c.panel))
                        .shadow(st.well())
                        .text_size(px(12.5))
                        .text_color(st.c(|c| c.muted))
                        .child(notes),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        dialog_button(st, "update-later", "Later", false)
                            .tip("Hide the Update button until a newer version")
                            .on_click(cx.listener(|ws, _, _, cx| ws.skip_update(cx))),
                    )
                    .child(
                        dialog_button(st, "update-download", "Download", true)
                            .tip("Open the release page in your browser")
                            .on_click(cx.listener(|ws, _, _, cx| ws.download_update(cx))),
                    ),
            );
        let serial = self.system.update_serial;
        Some(self.modal_layer(
            "update",
            serial,
            panel.into_any_element(),
            st,
            Self::close_update,
            cx,
        ))
    }
}
