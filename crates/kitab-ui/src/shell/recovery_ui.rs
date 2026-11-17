// SPDX-License-Identifier: AGPL-3.0-or-later

//! Crash recovery in the window (`crate::recovery` keeps the files): every
//! few seconds the open documents' unsaved work is published, a clean close
//! forgets it, and at start-up a crash's leftovers are offered back. The
//! frame guard (`Workspace::render`) and the panic hook write it at once.

use super::Workspace;
use crate::recovery::{self, Unsaved};
use gpui::{AppContext, Context, PromptLevel, Window};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How often unsaved work is published (written only when it changed).
const EVERY: Duration = Duration::from_secs(3);
/// Only the first window offers a crash's leftovers.
static OFFERED: AtomicBool = AtomicBool::new(false);

impl Workspace {
    /// Start publishing unsaved work, and offer a crash's (from `new`).
    pub(super) fn start_recovery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = cx.entity_id().as_u64();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(EVERY).await;
                let Ok(changed) = this.update(cx, |ws, cx| {
                    let work: Vec<Unsaved> = ws
                        .tabs
                        .iter()
                        .filter_map(|t| t.read(cx).unsaved_work())
                        .collect();
                    recovery::publish(id, work)
                }) else {
                    break;
                };
                if changed {
                    cx.background_spawn(async { recovery::write() }).await;
                }
            }
        })
        .detach();
        if !OFFERED.swap(true, Ordering::Relaxed) {
            let left = recovery::take_leftovers();
            if !left.is_empty() {
                self.offer_recovery(left, window, cx);
            }
        }
    }

    fn offer_recovery(&mut self, left: Vec<Unsaved>, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            // After the window and its tabs are back.
            cx.background_executor()
                .timer(Duration::from_millis(800))
                .await;
            let names: Vec<String> = left
                .iter()
                .map(|u| {
                    u.path
                        .file_name()
                        .map_or_else(|| u.path.display().to_string(), |n| n.to_string_lossy().into_owned())
                })
                .collect();
            let detail = format!(
                "Changes not saved in {} were kept. Restore them? They open with the changes in place, still to be saved.",
                names.join(", ")
            );
            let Ok(answer) = this.update_in(cx, |_, window, cx| {
                window.prompt(
                    PromptLevel::Warning,
                    "Kitab closed unexpectedly",
                    Some(&detail),
                    &["Restore", "Let them go"],
                    cx,
                )
            }) else {
                return;
            };
            match answer.await.ok() {
                Some(0) => {
                    let _ = this.update_in(cx, |ws, window, cx| {
                        for work in left {
                            let path = work.path.clone();
                            if !path.exists() {
                                continue;
                            }
                            ws.open_tab(path.clone(), window, cx);
                            if let Some(r) = ws.tabs.iter().find(|t| t.read(cx).path == path).cloned() {
                                r.update(cx, |r, cx| r.restore_work(work, cx));
                            }
                        }
                    });
                    recovery::clear_leftovers();
                }
                Some(_) => recovery::clear_leftovers(),
                // No answer (the window went): asked again next time.
                None => {}
            }
        })
        .detach();
    }
}
