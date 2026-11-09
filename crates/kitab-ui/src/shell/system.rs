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
            Section::Signing => self.load_trusted(cx),
            Section::Library => self.load_watched(cx),
            _ => {}
        }
    }

    fn load_watched(&mut self, cx: &mut Context<Self>) {
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let list = cx
                .background_spawn(async move { library.watched_folders() })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.system.watched = Some(list);
                cx.notify();
            });
        })
        .detach();
    }

    /// Library › Add folder…: watch a folder and look in it now.
    pub(crate) fn watch_folder(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Watch".into()),
        });
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut picked))) = rx.await else {
                return;
            };
            let Some(dir) = picked.pop() else { return };
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| dir.display().to_string());
            let _ = this.update(cx, |ws, cx| {
                ws.shelves.scanning = true;
                ws.toast(format!("Looking in {name}…"), false, None, cx);
                cx.notify();
            });
            let added = cx
                .background_spawn(async move {
                    library.watch_folder(&dir);
                    library.scan_watched()
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.shelves.scanning = false;
                let what = match added {
                    0 => "no new documents".to_string(),
                    1 => "1 new document, on Home under All".to_string(),
                    n => format!("{n} new documents, on Home under All"),
                };
                ws.toast(format!("Watching {name}: {what}."), false, None, cx);
                ws.load_watched(cx);
                ws.refresh_shelves(cx);
            });
        })
        .detach();
    }

    /// Ask before removing a watched folder (`None` cancels).
    pub(crate) fn ask_unwatch(&mut self, path: Option<std::path::PathBuf>, cx: &mut Context<Self>) {
        self.system.confirm_unwatch = path;
        cx.notify();
    }

    pub(crate) fn unwatch_folder(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        self.system.confirm_unwatch = None;
        let library = self.library.clone();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        cx.spawn(async move |this, cx| {
            let gone = cx
                .background_spawn(async move { library.unwatch_folder(&path) })
                .await;
            let _ = this.update(cx, |ws, cx| {
                let what = match gone {
                    0 => "no documents left your library".to_string(),
                    1 => "1 document left your library".to_string(),
                    n => format!("{n} documents left your library"),
                };
                ws.toast(format!("Stopped watching {name}: {what}."), false, None, cx);
                ws.load_watched(cx);
                ws.refresh_shelves(cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn load_trusted(&mut self, cx: &mut Context<Self>) {
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let list = cx
                .background_spawn(async move { engine.trusted_certs() })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.system.trusted = Some(list);
                cx.notify();
            });
        })
        .detach();
    }

    /// The trust list changed: list it again, and verify every open tab's
    /// signatures again.
    fn trust_changed(&mut self, cx: &mut Context<Self>) {
        self.load_trusted(cx);
        for tab in self.tabs.clone() {
            tab.update(cx, |r, cx| r.check_signatures(cx));
        }
    }

    /// Signing › Add certificate…: pick PEM or DER files and trust them.
    pub(crate) fn add_trusted_cert(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Trust".into()),
        });
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let result = cx
                .background_spawn(async move {
                    let mut added = Vec::new();
                    for p in paths {
                        added.extend(engine.trust_cert(&p)?);
                    }
                    Ok::<_, String>(added)
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                match result {
                    Ok(added) if added.len() == 1 => {
                        ws.toast(format!("Trusted {}.", added[0]), false, None, cx)
                    }
                    Ok(added) => ws.toast(
                        format!("Trusted {} certificates.", added.len()),
                        false,
                        None,
                        cx,
                    ),
                    Err(e) => ws.toast(format!("Couldn't trust it: {e}"), true, None, cx),
                }
                ws.trust_changed(cx);
            });
        })
        .detach();
    }

    /// Signing › Remove: stop trusting one certificate.
    pub(crate) fn untrust_cert(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let engine = self.engine.clone();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_spawn(async move { engine.untrust_cert(&path) })
                .await;
            let _ = this.update(cx, |ws, cx| {
                if let Err(e) = r {
                    ws.toast(format!("Couldn't remove it: {e}"), true, None, cx);
                }
                ws.trust_changed(cx);
            });
        })
        .detach();
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

    /// The window went to the background: if it's still there in a little
    /// while, MuPDF trims its cache (decoded fonts and images), which costs
    /// nothing you can see. Rendered pages stay, so no tab redraws later.
    pub(crate) fn release_when_idle(&mut self, cx: &mut Context<Self>) {
        const AFTER: std::time::Duration = std::time::Duration::from_secs(20);
        self.after(AFTER, cx, |ws, cx| {
            if ws.window_active {
                return;
            }
            // Only MuPDF's own store: tabs keep their rendered pages, so
            // coming back to one never redraws it (that showed as flicker).
            let engine = ws.engine.clone();
            cx.background_spawn(async move { engine.shrink_caches() })
                .detach();
        });
    }

    /// Advanced › Clear cached pages: every tab's rendered pages, and the
    /// engine's decoded fonts and images. Visible pages render again.
    pub(crate) fn clear_caches(&mut self, cx: &mut Context<Self>) {
        for tab in self.tabs.clone() {
            tab.update(cx, |r, cx| r.drop_cached_pages(cx));
        }
        self.home.drop_covers();
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

    pub(crate) fn ask_restore_defaults(&mut self, ask: bool, cx: &mut Context<Self>) {
        self.system.confirm_reset = ask;
        cx.notify();
    }

    /// Advanced › Restore default settings: every setting as Kitab comes,
    /// saved at once. The library, marks and tabs aren't settings and stay.
    pub(crate) fn restore_defaults(&mut self, cx: &mut Context<Self>) {
        if self.themes.is_empty() {
            self.themes = kitab_theme::builtin_all().unwrap_or_default();
        }
        let defaults = crate::settings::Settings::default();
        let theme = self.themes.iter().find(|t| t.id == defaults.theme).cloned();
        self.change_style(
            &move |st: &mut Style| {
                let theme = theme.clone().unwrap_or_else(|| st.base_theme().clone());
                *st = Style::from_settings(theme, &defaults);
            },
            cx,
        );
        self.system.confirm_reset = false;
        self.focus_sidebar = false;
        self.toast("Settings are back to their defaults.", false, None, cx);
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

    // ── keymap.json ──

    /// Read keymap.json off the UI thread and apply it if it changed since
    /// the last read (start-up, and whenever the window comes to the front).
    pub(super) fn load_keymap(&mut self, cx: &mut Context<Self>) {
        let Some(path) = kitab_os::data_dir().map(|d| d.join("keymap.json")) else {
            return;
        };
        let known = self.keymap_mtime;
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move {
                    let meta = std::fs::metadata(&path).ok();
                    let mtime = meta.as_ref().and_then(|m| m.modified().ok());
                    if mtime == known {
                        return None;
                    }
                    let text = match meta {
                        Some(m) if m.len() > crate::keymap::MAX_BYTES => {
                            Some(Err("the file is too big".to_string()))
                        }
                        Some(_) => Some(std::fs::read_to_string(&path).map_err(|e| e.to_string())),
                        None => None,
                    };
                    Some((mtime, text))
                })
                .await;
            let Some((mtime, text)) = read else { return };
            let _ = this.update(cx, |ws, cx| {
                ws.keymap_mtime = mtime;
                let parsed = match text {
                    // Gone: back to the defaults.
                    None => Ok(Vec::new()),
                    Some(Ok(t)) => crate::keymap::parse(&t),
                    Some(Err(e)) => Err(e),
                };
                match parsed {
                    Ok(user) => {
                        let errors = crate::keymap::apply(cx, &user);
                        if let Some(first) = errors.first() {
                            let more = match errors.len() {
                                1 => String::new(),
                                n => format!(" (and {} more)", n - 1),
                            };
                            ws.toast(format!("keymap.json: {first}{more}."), true, None, cx);
                        }
                    }
                    Err(e) => ws.toast(
                        format!("keymap.json: {e}. Keeping the keys you had."),
                        true,
                        None,
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    /// Edit keymap.json: write one with instructions if there isn't one,
    /// then open it in the system's editor.
    pub(crate) fn edit_keymap(&mut self, cx: &mut Context<Self>) {
        let Some(path) = kitab_os::data_dir().map(|d| d.join("keymap.json")) else {
            return;
        };
        let template = crate::keymap::template(cx.all_action_names());
        cx.spawn(async move |this, cx| {
            let p = path.clone();
            let made = cx
                .background_spawn(async move {
                    if p.exists() {
                        return Ok(());
                    }
                    if let Some(dir) = p.parent() {
                        std::fs::create_dir_all(dir)?;
                    }
                    std::fs::write(&p, template)
                })
                .await;
            let _ = this.update(cx, |ws, cx| match made {
                Ok(()) => cx.open_with_system(&path),
                Err(e) => ws.toast(format!("Couldn't make keymap.json: {e}"), true, None, cx),
            });
        })
        .detach();
    }

    // ── updates ──

    /// Start-up: the first automatic check, 10 s after the window opened, if
    /// Settings › About › Check on start-up is on.
    pub(super) fn schedule_update_check(&mut self, delay: Duration, cx: &mut Context<Self>) {
        if update::managed_by().is_some() {
            return;
        }
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
                .hover(|d| d.bg(st.hover_bg()))
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
            format!("Released {}. ", r.published)
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
                        "{released}You have {VERSION}. Install the new version over it: your library, marks and settings stay."
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
                        dialog_button(st, "update-later", "Skip this version", false)
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
