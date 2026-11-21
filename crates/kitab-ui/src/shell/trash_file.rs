// SPDX-License-Identifier: AGPL-3.0-or-later

//! GitHub #17: move a book's file to the Recycle Bin (the Trash on macOS and
//! Linux), where it can be put back from. Home's card menu asks first.
//! Reading or presenting, Del once says what a second Del will do; the
//! second opens the next file of the folder in the same tab (Home after the
//! last one) and sends the file to the bin. Kitab forgets it too.

use super::Workspace;
use super::next_book::next_in_folder;
use crate::reader::Reader;
use gpui::{AppContext, Context, Entity, EntityId, Window};
use kitab_os::trash::{TRASH_NAME, move_to_trash};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long the second Del is waited for.
const ARMED_FOR: Duration = Duration::from_secs(5);
/// A held key repeats; only a press after a pause this long is the second.
const PAUSE: Duration = Duration::from_millis(250);

pub(super) struct Armed {
    reader: EntityId,
    first: Instant,
    last: Instant,
    /// The toast that asked, closed when the second press comes.
    toast: u64,
}

fn name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Workspace {
    /// Del in the reader: the first press asks, the second does it.
    pub(super) fn trash_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reader) = self.active_reader().cloned() else {
            return;
        };
        let id = reader.entity_id();
        let now = Instant::now();
        if let Some(armed) = self.trash_armed.as_mut()
            && armed.reader == id
            && now.duration_since(armed.first) < ARMED_FOR
        {
            let paused = now.duration_since(armed.last) >= PAUSE;
            armed.last = now;
            if paused {
                let toast = armed.toast;
                self.trash_armed = None;
                self.dismiss_toast(toast, cx);
                self.trash_and_go_on(reader, window, cx);
            }
            return;
        }
        let path = reader.read(cx).path.clone();
        self.toast(
            format!(
                "Press Del again to move {} to the {TRASH_NAME}.",
                name(&path)
            ),
            false,
            None,
            cx,
        );
        self.trash_armed = Some(Armed {
            reader: id,
            first: now,
            last: now,
            toast: self.tools.toast_serial,
        });
    }

    /// The command palette's and File menu's "Move to Recycle Bin…": one
    /// run, so a dialog asks instead of a second press.
    pub(super) fn trash_asked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reader) = self.active_reader().cloned() else {
            return;
        };
        let path = reader.read(cx).path.clone();
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            &format!("Move {} to the {TRASH_NAME}?", name(&path)),
            Some(&format!(
                "The file goes to the {TRASH_NAME}, where you can put it back from, and the next file in its folder opens."
            )),
            &["Move", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await.ok() != Some(0) {
                return;
            }
            let _ = this.update_in(cx, |ws, window, cx| {
                ws.trash_and_go_on(reader, window, cx);
            });
        })
        .detach();
    }

    /// The next file of the folder takes the tab's place (presenting on, if
    /// it was), or Home shows after the last; then the file goes.
    fn trash_and_go_on(
        &mut self,
        reader: Entity<Reader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = reader.read(cx).path.clone();
        let presenting = reader.read(cx).presenting();
        let from = path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let next = cx
                .background_spawn(async move { next_in_folder(&from) })
                .await;
            let _ = this.update_in(cx, |ws, window, cx| {
                let Some(ix) = ws.tabs.iter().position(|t| *t == reader) else {
                    return;
                };
                match next {
                    Some(next) => {
                        ws.insert_tab(next.clone(), Some(ix + 1), true, window, cx);
                        if presenting
                            && let Some(new) = ws.tabs.iter().find(|t| t.read(cx).path == next)
                        {
                            new.update(cx, |r, cx| r.present_when_open(cx));
                            ws.present_carried = true;
                        }
                        ws.force_close_tab(ix, window, cx);
                    }
                    None => {
                        if presenting {
                            reader.update(cx, |r, cx| r.stop_presenting(cx));
                        }
                        ws.force_close_tab(ix, window, cx);
                        ws.show_home(window, cx);
                    }
                }
                ws.trash_file(path, cx);
            });
        })
        .detach();
    }

    /// "Move to Recycle Bin" on a card: ask, close its tab if open, then go.
    pub(super) fn trash_from_menu(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            &format!("Move {} to the {TRASH_NAME}?", name(&path)),
            Some(&format!(
                "The file goes to the {TRASH_NAME}, where you can put it back from. Kitab forgets its reading place, bookmarks and notes."
            )),
            &["Move", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await.ok() != Some(0) {
                return;
            }
            let _ = this.update_in(cx, |ws, window, cx| {
                if let Some(ix) = ws.tabs.iter().position(|t| t.read(cx).path == path) {
                    ws.force_close_tab(ix, window, cx);
                }
                ws.trash_file(path, cx);
            });
        })
        .detach();
    }

    /// Off the UI thread: the file to the bin (waiting for a just-closed tab
    /// to let go of it), then out of the library.
    fn trash_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        // Ctrl+Shift+T would only find it gone.
        self.closed.retain(|p| *p != path);
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let p = path.clone();
            let done = cx
                .background_spawn(async move {
                    let done = move_to_trash(&p);
                    if done.is_ok() {
                        library.forget(&p);
                    }
                    done
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                match done {
                    Ok(()) => ws.toast(
                        format!("Moved {} to the {TRASH_NAME}.", name(&path)),
                        false,
                        None,
                        cx,
                    ),
                    Err(e) => ws.toast(
                        format!("Couldn't move {} to the {TRASH_NAME}: {e}.", name(&path)),
                        true,
                        None,
                        cx,
                    ),
                }
                ws.refresh_recent(cx);
            });
        })
        .detach();
    }
}
