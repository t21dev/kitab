// SPDX-License-Identifier: AGPL-3.0-or-later

//! Home's shelves (PLAN.md Phase 9): Recent, All, Favourites, In progress,
//! Finished and the user's collections, as chips under Home's header; the
//! card menu's Favourite, Finished and collection toggles; a small sheet to
//! name a collection; and the watched folders' scan.

use super::Workspace;
use super::tools_ui::dialog_button;
use crate::library::{CollectionInfo, RecentItem, ShelfKind};
use crate::menu::MenuItem;
use crate::style::Style;
use crate::text_field::{FieldEvent, TextField};
use gpui::{
    AnyElement, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Point, StatefulInteractiveElement, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct ShelfState {
    pub kind: ShelfKind,
    pub items: Option<Vec<RecentItem>>,
    pub collections: Vec<CollectionInfo>,
    pub naming: Option<NameSheet>,
    pub serial: u64,
    /// How many documents the library has (watched folders included), so
    /// Home knows "nothing yet" from "nothing opened yet".
    pub library_size: usize,
    /// The watched folders are being looked through.
    pub scanning: bool,
}

/// Naming a collection: a new one (and the document to put in it), or a
/// rename.
pub(super) struct NameSheet {
    field: Entity<TextField>,
    rename: Option<i64>,
    add: Option<PathBuf>,
    serial: u64,
    _sub: Subscription,
}

impl Workspace {
    /// Reload the collections, the shown shelf, and scan the watched folders
    /// for new documents (background).
    pub(crate) fn refresh_shelves(&mut self, cx: &mut Context<Self>) {
        let library = self.library.clone();
        let kind = self.shelves.kind;
        cx.spawn(async move |this, cx| {
            let (collections, items, size) = cx
                .background_spawn(async move {
                    let all = library.shelf(ShelfKind::All);
                    let size = all.len();
                    let items = match kind {
                        ShelfKind::Recent => None,
                        ShelfKind::All => Some(all),
                        _ => Some(library.shelf(kind)),
                    };
                    (library.collections(), items, size)
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                // A deleted collection's shelf falls back to Recent.
                if let ShelfKind::Collection(id) = ws.shelves.kind
                    && !collections.iter().any(|c| c.id == id)
                {
                    ws.shelves.kind = ShelfKind::Recent;
                }
                ws.shelves.collections = collections;
                ws.shelves.library_size = size;
                // Documents from watched folders, none opened yet: Home
                // shows them (All) rather than an empty Recent.
                if ws.shelves.kind == ShelfKind::Recent && ws.recent.is_empty() && size > 0 {
                    ws.shelves.kind = ShelfKind::All;
                    ws.refresh_shelves(cx);
                    return;
                }
                kitab_os::trace::mark(&format!(
                    "shelf {:?}: {} items",
                    ws.shelves.kind,
                    items.as_ref().map_or(0, Vec::len)
                ));
                ws.shelves.items = items;
                cx.notify();
            });
        })
        .detach();
    }

    /// Look in the watched folders, then refresh what Home shows.
    pub(crate) fn scan_watched(&mut self, cx: &mut Context<Self>) {
        self.scan_watched_then(false, cx);
    }

    /// Look through the watched folders; `say` reports the outcome even
    /// when nothing was new (a folder just added, or Look again now).
    pub(crate) fn scan_watched_then(&mut self, say: bool, cx: &mut Context<Self>) {
        if self.shelves.scanning {
            return;
        }
        self.shelves.scanning = true;
        cx.notify();
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let added = cx
                .background_spawn(async move { library.scan_watched() })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.shelves.scanning = false;
                if added > 0 {
                    ws.refresh_shelves(cx);
                }
                if added > 0 || say {
                    let text = match added {
                        0 => "No new documents in your watched folders.".to_string(),
                        1 => "Found 1 new document. It's on Home under All.".to_string(),
                        n => format!("Found {n} new documents. They're on Home under All."),
                    };
                    ws.toast(text, false, None, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn show_shelf(&mut self, kind: ShelfKind, cx: &mut Context<Self>) {
        self.shelves.kind = kind;
        self.shelves.items = None;
        self.refresh_shelves(cx);
    }

    fn shelf_name(&self, kind: ShelfKind) -> String {
        match kind {
            ShelfKind::Recent => "Recent".into(),
            ShelfKind::All => "All documents".into(),
            ShelfKind::Favourites => "Favourites".into(),
            ShelfKind::InProgress => "In progress".into(),
            ShelfKind::Finished => "Finished".into(),
            ShelfKind::Collection(id) => self
                .shelves
                .collections
                .iter()
                .find(|c| c.id == id)
                .map_or_else(|| "Collection".into(), |c| c.name.clone()),
        }
    }

    /// The chips under Home's header.
    pub(super) fn shelf_chips(&mut self, st: &Style, cx: &mut Context<Self>) -> AnyElement {
        let mut kinds = vec![
            (ShelfKind::Recent, "Recent".to_string()),
            (ShelfKind::All, "All".to_string()),
            (ShelfKind::Favourites, "Favourites".to_string()),
            (ShelfKind::InProgress, "In progress".to_string()),
            (ShelfKind::Finished, "Finished".to_string()),
        ];
        kinds.extend(self.shelves.collections.iter().map(|c| {
            (
                ShelfKind::Collection(c.id),
                format!("{}  {}", c.name, c.count),
            )
        }));
        let mut row = div()
            .flex()
            .flex_wrap()
            .gap(px(6.0))
            .px(px(40.0))
            .pt(px(18.0));
        for (i, (kind, label)) in kinds.into_iter().enumerate() {
            let on = self.shelves.kind == kind;
            let mut chip = div()
                .id(("shelf", i))
                .h(px(28.0))
                .px(px(12.0))
                // A long collection name ends in "…" inside its chip.
                .max_w(px(240.0))
                .min_w_0()
                .flex()
                .items_center()
                .rounded_full()
                .text_size(px(12.5))
                .cursor_pointer()
                .when(on, |d| {
                    d.bg(st.c(|c| c.raised))
                        .shadow(st.elev())
                        .text_color(st.c(|c| c.text))
                })
                .when(!on, |d| {
                    d.text_color(st.c(|c| c.muted))
                        .hover(|d| d.bg(st.hover_bg()))
                })
                .on_click(cx.listener(move |ws, _, _, cx| ws.show_shelf(kind, cx)))
                .child(div().min_w_0().truncate().child(label));
            if let ShelfKind::Collection(id) = kind {
                chip = chip.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        ws.collection_menu(id, e.position, window, cx)
                    }),
                );
            }
            row = row.child(chip);
        }
        row = row.child(
            div()
                .id("shelf-new")
                .h(px(28.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .rounded_full()
                .text_size(px(12.5))
                .text_color(st.c(|c| c.faint))
                .cursor_pointer()
                .hover(|d| d.bg(st.hover_bg()).text_color(st.c(|c| c.text)))
                .on_click(
                    cx.listener(|ws, _, window, cx| ws.name_collection(None, None, window, cx)),
                )
                .child("+ Collection"),
        );
        row.into_any_element()
    }

    /// A shelf other than Recent, as a grid of cards.
    /// The shelf's title above its cards, or why it is empty (the cards
    /// themselves are rows of Home's list).
    pub(super) fn shelf_heading(&self, st: &Style) -> AnyElement {
        let name = self.shelf_name(self.shelves.kind);
        let col = div()
            .flex()
            .flex_col()
            .child(super::section_title(&name, st));
        match self.shelves.items.as_ref() {
            Some(items) if items.is_empty() => col
                .child(
                    div()
                        .px(px(40.0))
                        .pt(px(8.0))
                        .text_size(st.px(|m| m.ui_text))
                        .text_color(st.c(|c| c.faint))
                        .child(match self.shelves.kind {
                            ShelfKind::Favourites => {
                                "No favourites yet. Right-click a book on Home and choose Favourite."
                            }
                            ShelfKind::InProgress => "Nothing in progress.",
                            ShelfKind::Finished => {
                                "Nothing finished yet. Books count as finished at the last page, or when you mark them."
                            }
                            ShelfKind::Collection(_) => {
                                "This collection is empty. Right-click a book on Home to add it."
                            }
                            _ => "No documents yet. Open some, or add a watched folder in Settings › Library.",
                        }),
                )
                .into_any_element(),
            _ => col.into_any_element(),
        }
    }

    /// The card menu, once the document's flags are known.
    pub(super) fn card_menu_flags(
        &mut self,
        path: PathBuf,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let library = self.library.clone();
        let p = path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let flags = cx
                .background_spawn(async move { library.doc_flags(&p) })
                .await;
            let _ = this.update_in(cx, |ws, window, cx| {
                ws.card_menu(path, at, flags, window, cx);
            });
        })
        .detach();
    }

    /// Favourite, Finished and the collections, for the card menu.
    pub(super) fn shelf_items(
        &self,
        path: &std::path::Path,
        flags: Option<(bool, Vec<i64>)>,
        cx: &mut Context<Self>,
    ) -> Vec<MenuItem> {
        let Some((favourite, inside)) = flags else {
            return Vec::new();
        };
        let mut items = vec![MenuItem::Separator];
        let p = path.to_path_buf();
        items.push(
            MenuItem::new(
                "Favourite",
                self.on_ws(cx, move |ws, _, cx| {
                    let (library, p) = (ws.library.clone(), p.clone());
                    cx.spawn(async move |this, cx| {
                        cx.background_spawn(async move { library.set_favourite(&p, !favourite) })
                            .await;
                        // Recent shows the ribbon and progress too, not only the shelves.
                        let _ = this.update(cx, |ws, cx| {
                            ws.refresh_shelves(cx);
                            ws.refresh_recent(cx);
                        });
                    })
                    .detach();
                }),
            )
            .icon("bookmark")
            .checked(favourite),
        );
        let p = path.to_path_buf();
        items.push(
            MenuItem::new(
                "Mark finished",
                self.on_ws(cx, move |ws, _, cx| {
                    let (library, p) = (ws.library.clone(), p.clone());
                    cx.spawn(async move |this, cx| {
                        cx.background_spawn(async move { library.set_finished(&p, true) })
                            .await;
                        // Recent shows the ribbon and progress too, not only the shelves.
                        let _ = this.update(cx, |ws, cx| {
                            ws.refresh_shelves(cx);
                            ws.refresh_recent(cx);
                        });
                    })
                    .detach();
                }),
            )
            .icon("check"),
        );
        for c in self.shelves.collections.iter().take(12) {
            let (id, on, p) = (c.id, inside.contains(&c.id), path.to_path_buf());
            items.push(
                MenuItem::new(
                    format!("In {}", c.name),
                    self.on_ws(cx, move |ws, _, cx| {
                        let (library, p) = (ws.library.clone(), p.clone());
                        cx.spawn(async move |this, cx| {
                            cx.background_spawn(
                                async move { library.set_in_collection(id, &p, !on) },
                            )
                            .await;
                            // Recent shows the ribbon and progress too, not only the shelves.
                            let _ = this.update(cx, |ws, cx| {
                                ws.refresh_shelves(cx);
                                ws.refresh_recent(cx);
                            });
                        })
                        .detach();
                    }),
                )
                .icon("folder-open")
                .checked(on),
            );
        }
        let p = path.to_path_buf();
        items.push(
            MenuItem::new(
                "New collection…",
                self.on_ws(cx, move |ws, window, cx| {
                    ws.name_collection(None, Some(p.clone()), window, cx)
                }),
            )
            .icon("plus"),
        );
        items
    }

    fn collection_menu(
        &mut self,
        id: i64,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = vec![
            MenuItem::new(
                "Rename…",
                self.on_ws(cx, move |ws, window, cx| {
                    ws.name_collection(Some(id), None, window, cx)
                }),
            )
            .icon("edit"),
            MenuItem::new(
                "Delete collection",
                self.on_ws(cx, move |ws, _, cx| {
                    let library = ws.library.clone();
                    cx.spawn(async move |this, cx| {
                        cx.background_spawn(async move { library.delete_collection(id) })
                            .await;
                        let _ = this.update(cx, |ws, cx| {
                            ws.toast(
                                "Collection deleted. Its documents stay in your library.",
                                false,
                                None,
                                cx,
                            );
                            ws.refresh_shelves(cx);
                        });
                    })
                    .detach();
                }),
            )
            .icon("trash"),
        ];
        self.open_menu(at, false, 220.0, items, window, cx);
    }

    /// The name sheet: a new collection (with `add` in it), or a rename.
    fn name_collection(
        &mut self,
        rename: Option<i64>,
        add: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        self.menu_closing = None;
        let current = rename
            .and_then(|id| self.shelves.collections.iter().find(|c| c.id == id))
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let field = cx.new(|cx| TextField::new(&current, false, "Collection name", cx));
        let sub = cx.subscribe_in(
            &field,
            window,
            |ws, _, e: &FieldEvent, window, cx| match e {
                FieldEvent::Submit => ws.finish_naming(window, cx),
                FieldEvent::Cancel => ws.close_naming(window, cx),
                FieldEvent::Changed => cx.notify(),
            },
        );
        field.read(cx).focus(window);
        self.shelves.serial += 1;
        self.shelves.naming = Some(NameSheet {
            field,
            rename,
            add,
            serial: self.shelves.serial,
            _sub: sub,
        });
        cx.notify();
    }

    pub(super) fn close_naming(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shelves.naming.is_none() {
            return;
        }
        self.exit_modal("name", window, cx, |ws, window, cx| {
            if ws.shelves.naming.take().is_some() {
                ws.refocus(window, cx);
                cx.notify();
            }
        });
    }

    fn finish_naming(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = self.shelves.naming.as_ref() else {
            return;
        };
        let name = sheet.field.read(cx).text().trim().to_string();
        if name.is_empty() {
            return;
        }
        let Some(sheet) = self.shelves.naming.take() else {
            return;
        };
        self.refocus(window, cx);
        let library = self.library.clone();
        cx.spawn(async move |this, cx| {
            let made = cx
                .background_spawn(async move {
                    match sheet.rename {
                        Some(id) => {
                            library.rename_collection(id, &name);
                            Some(id)
                        }
                        None => {
                            let id = library.create_collection(&name)?;
                            if let Some(p) = sheet.add {
                                library.set_in_collection(id, &p, true);
                            }
                            Some(id)
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                if let Some(id) = made {
                    ws.shelves.kind = ShelfKind::Collection(id);
                }
                ws.refresh_shelves(cx);
            });
        })
        .detach();
    }

    pub(super) fn naming_view(&mut self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.shelves.naming.as_ref()?;
        let title = if s.rename.is_some() {
            "Rename collection"
        } else {
            "New collection"
        };
        let ready = !s.field.read(cx).text().trim().is_empty();
        let panel = div()
            .id("name-sheet")
            .w(px(380.0))
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
                    .child(title),
            )
            .child(
                div()
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(st.px(|m| m.control_radius))
                    .bg(st.c(|c| c.hover))
                    .border_1()
                    .border_color(st.c(|c| c.line))
                    .child(s.field.clone()),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        dialog_button(st, "name-cancel", "Cancel", false)
                            .on_click(cx.listener(|ws, _, window, cx| ws.close_naming(window, cx))),
                    )
                    .child(
                        dialog_button(
                            st,
                            "name-go",
                            if s.rename.is_some() {
                                "Rename"
                            } else {
                                "Create"
                            },
                            true,
                        )
                        .when(!ready, |b| b.opacity(0.45))
                        .on_click(cx.listener(|ws, _, window, cx| ws.finish_naming(window, cx))),
                    ),
            );
        let serial = s.serial;
        Some(self.modal_layer(
            "name",
            serial,
            panel.into_any_element(),
            st,
            Self::close_naming,
            cx,
        ))
    }
}
