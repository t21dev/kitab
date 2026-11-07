// SPDX-License-Identifier: AGPL-3.0-or-later

//! Settings › File types, Advanced and About (DESIGN.md § Settings modal).
//! What they show is loaded off the UI thread by the workspace
//! (`shell/system.rs`) into [`SystemInfo`].

use super::{Click, change, group, icon_row, switch};
use crate::shell::Workspace;
use crate::style::Style;
use crate::tooltip::Tip;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, WeakEntity, div, prelude::FluentBuilder, px,
};
use kitab_os::assoc::{FILE_TYPES, TypeStatus};
use kitab_os::update::{REPO, Release};

/// What File types, Advanced and About show, kept by the workspace.
#[derive(Default)]
pub struct SystemInfo {
    /// Per FILE_TYPES entry; loaded when File types opens.
    pub types: Option<Vec<TypeStatus>>,
    /// The library database's size; loaded when Advanced opens.
    pub db_size: Option<u64>,
    /// Watched folders; loaded when Library opens.
    pub watched: Option<Vec<std::path::PathBuf>>,
    /// Certificates trusted for signatures; loaded when Signing opens.
    pub trusted: Option<Vec<crate::engine::TrustedCert>>,
    pub update: UpdateState,
    /// The update dialog is open.
    pub update_open: bool,
    pub update_serial: u64,
    /// Clear reading history is waiting for its confirmation.
    pub confirm_clear: bool,
}

/// Where the update check stands.
#[derive(Debug, Clone, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Failed(String),
}

/// A format's icon, the same as its tab's.
fn type_icon(t: &kitab_os::assoc::FileType) -> &'static str {
    let ext = t.extensions.first().copied().unwrap_or("pdf");
    crate::shell::format_icon(std::path::Path::new(&format!("x.{ext}")))
}

/// Element ids for each file type's row button.
const TYPE_IDS: [&str; 8] = [
    "type-0", "type-1", "type-2", "type-3", "type-4", "type-5", "type-6", "type-7",
];

/// A secondary button for a well row ("Make default…", "Clear").
fn button(id: &'static str, label: &str, danger: bool, st: &Style, on_click: Click) -> AnyElement {
    div()
        .id(id)
        .flex_none()
        .h(px(28.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .rounded(px(7.0))
        .bg(st.c(|c| c.raised))
        .shadow(st.elev())
        .text_size(px(12.5))
        .text_color(if danger {
            st.c(|c| c.bad)
        } else {
            st.c(|c| c.text)
        })
        .cursor_pointer()
        .hover(|b| b.bg(st.c(|c| c.hover)))
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(label.to_string())
        .into_any_element()
}

/// Run `f` on the workspace from a click.
fn act(ws: &WeakEntity<Workspace>, f: fn(&mut Workspace, &mut gpui::Context<Workspace>)) -> Click {
    let ws = ws.clone();
    Box::new(move |_, cx| {
        let _ = ws.update(cx, f);
    })
}

/// A small status label at the end of a row.
fn status(text: &str, on: bool, st: &Style) -> AnyElement {
    div()
        .flex_none()
        .px(px(8.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .rounded_full()
        .text_size(px(11.5))
        .when(on, |d| {
            d.text_color(st.c(|c| c.accent))
                .border_1()
                .border_color(st.c(|c| c.accent))
        })
        .when(!on, |d| d.text_color(st.c(|c| c.faint)))
        .child(text.to_string())
        .into_any_element()
}

fn rows(items: Vec<AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .children(items)
        .into_any_element()
}

// ── Shortcuts ──

/// Settings › Shortcuts' first row: keymap.json.
pub(super) fn keymap_row(ws: &WeakEntity<Workspace>, st: &Style) -> AnyElement {
    icon_row(
        Some("keyboard"),
        "Your own keys",
        "keymap.json adds to these keys or changes them. Save it and come back to Kitab; it applies at once. The palette (Ctrl+K) shows the keys in force.",
        button(
            "keymap-edit",
            "Edit keymap.json",
            false,
            st,
            act(ws, |ws, cx| ws.edit_keymap(cx)),
        ),
        st,
    )
}

// ── File types ───────────────────────────────────────────────────────────

pub(super) fn file_types(ws: &WeakEntity<Workspace>, info: &SystemInfo, st: &Style) -> AnyElement {
    let windows = cfg!(windows);
    let mut list = Vec::new();
    for (i, t) in FILE_TYPES.iter().enumerate() {
        let s = info.types.as_ref().and_then(|v| v.get(i)).copied();
        let exts = t
            .extensions
            .iter()
            .map(|e| format!(".{e}"))
            .collect::<Vec<_>>()
            .join("  ");
        // A type that isn't set up gets its own Set up; one set up but owned
        // by another app gets Make default (Windows asks in Default apps).
        let set_up: Click = {
            let ws = ws.clone();
            Box::new(move |_, cx| {
                let _ = ws.update(cx, |ws, cx| ws.set_up_type(i, cx));
            })
        };
        let control = match s {
            None => status("…", false, st),
            Some(s) if s.default == Some(true) => status("Default", true, st),
            Some(s) if !s.registered => {
                button(TYPE_IDS[i % TYPE_IDS.len()], "Set up", false, st, set_up)
            }
            Some(s) if s.default.is_none() => status("Set up", false, st),
            Some(_) => div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(status("Another app", false, st))
                .child(button(
                    TYPE_IDS[i % TYPE_IDS.len()],
                    "Make default",
                    false,
                    st,
                    set_up,
                ))
                .into_any_element(),
        };
        list.push(icon_row(
            Some(type_icon(t)),
            t.description,
            &exts,
            control,
            st,
        ));
    }
    let defaults = !kitab_os::assoc::DEFAULTS_PAGE.is_empty();
    let actions = div()
        .flex()
        .flex_wrap()
        .gap(px(8.0))
        .child(button(
            "assoc-default",
            "Set up all…",
            false,
            st,
            act(ws, |ws, cx| ws.make_default(cx)),
        ))
        .when(defaults, |d| {
            d.child(button(
                "assoc-apps",
                "Open Default apps",
                false,
                st,
                act(ws, |ws, cx| ws.open_default_apps(cx)),
            ))
        })
        .child(button(
            "assoc-refresh",
            "Refresh",
            false,
            st,
            act(ws, |ws, cx| {
                ws.load_system_info(super::Section::FileTypes, cx)
            }),
        ));
    let note = if windows {
        "Kitab opens these files when you double-click them. Set up registers a type for your account (Set up all does every one); Windows then asks you to pick Kitab in Default apps."
    } else if cfg!(target_os = "macos") {
        "Kitab opens these files when you double-click them. Set up makes Kitab the default app for that type in Finder; Set up all does every one but plain text."
    } else {
        "Kitab opens these files when you double-click them. Set up adds Kitab to your apps if it isn't there and makes it the default for that type; Set up all does every one but plain text."
    };
    let thumbs = info.types.as_ref().map(|v| v.iter().any(|s| s.thumbnails));
    let mut col = div().flex().flex_col().gap(px(28.0)).child(group(
        "Default app",
        Some(note),
        rows({
            list.push(actions.into_any_element());
            list
        }),
        st,
    ));
    // macOS has no Quick Look extension yet, so no thumbnails there.
    if !cfg!(target_os = "macos") {
        let (about, label) = if windows {
            (
                "Page previews of PDFs, books and Markdown in File Explorer, drawn by Kitab. Make default turns them on.",
                "File Explorer thumbnails",
            )
        } else {
            (
                "Page previews of PDFs, books and Markdown in your file manager, drawn by Kitab. Set up turns them on.",
                "File manager thumbnails",
            )
        };
        col = col.child(group(
            "Thumbnails",
            Some(about),
            icon_row(
                Some("image"),
                label,
                "",
                match thumbs {
                    None => status("…", false, st),
                    Some(true) => status("On", true, st),
                    Some(false) => status("Off", false, st),
                },
                st,
            ),
            st,
        ));
    }
    col.child(group(
        "Stop using Kitab for these files",
        None,
        icon_row(
            Some("trash"),
            "Remove Kitab's file types",
            if cfg!(target_os = "macos") {
                "Hands PDFs, EPUBs and text back to Preview, Books and TextEdit. Your files are not touched."
            } else {
                "Takes Kitab off the Open with lists and stops its thumbnails, for your account. Your files are not touched."
            },
            button(
                "assoc-remove",
                "Remove",
                true,
                st,
                act(ws, |ws, cx| ws.unregister_types(cx)),
            ),
            st,
        ),
        st,
    ))
    .into_any_element()
}

// ── Advanced ─────────────────────────────────────────────────────────────

// ── Library ──────────────────────────────────────────────────────────────

const WATCH_IDS: [&str; 10] = [
    "watch-0", "watch-1", "watch-2", "watch-3", "watch-4", "watch-5", "watch-6", "watch-7",
    "watch-8", "watch-9",
];

pub(super) fn library(ws: &WeakEntity<Workspace>, info: &SystemInfo, st: &Style) -> AnyElement {
    let mut list = Vec::new();
    match info.watched.as_deref() {
        None => list.push(icon_row(Some("folder-open"), "Loading…", "", status("…", false, st), st)),
        Some([]) => list.push(icon_row(
            Some("folder-open"),
            "No watched folders",
            "Documents in a watched folder (and the folders inside it) join your library without being opened: they show in Home › All and can go in collections. Search covers each one after it has been opened once.",
            div().into_any_element(),
            st,
        )),
        Some(folders) => {
            for (i, f) in folders.iter().enumerate() {
                let path = f.clone();
                let ws = ws.clone();
                let remove: Click = Box::new(move |_, cx| {
                    let path = path.clone();
                    let _ = ws.update(cx, move |ws, cx| ws.unwatch_folder(path, cx));
                });
                let name = f
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| f.display().to_string());
                list.push(icon_row(
                    Some("folder-open"),
                    &name,
                    &f.display().to_string(),
                    button(WATCH_IDS[i % WATCH_IDS.len()], "Remove", true, st, remove),
                    st,
                ));
            }
        }
    }
    list.push(
        div()
            .flex()
            .gap(px(8.0))
            .child(button(
                "watch-add",
                "Add folder…",
                false,
                st,
                act(ws, |ws, cx| ws.watch_folder(cx)),
            ))
            .child(button(
                "watch-scan",
                "Look again now",
                false,
                st,
                act(ws, |ws, cx| ws.scan_watched(cx)),
            ))
            .into_any_element(),
    );
    div()
        .flex()
        .flex_col()
        .gap(px(28.0))
        .child(group(
            "Watched folders",
            Some("Kitab looks in these folders when it starts and when you ask. Nothing is moved or changed; removing a folder keeps the documents already found."),
            rows(list),
            st,
        ))
        .into_any_element()
}

// ── Signing ──────────────────────────────────────────────────────────────

const TRUST_IDS: [&str; 12] = [
    "trust-0", "trust-1", "trust-2", "trust-3", "trust-4", "trust-5", "trust-6", "trust-7",
    "trust-8", "trust-9", "trust-10", "trust-11",
];

pub(super) fn signing(ws: &WeakEntity<Workspace>, info: &SystemInfo, st: &Style) -> AnyElement {
    let mut list = Vec::new();
    match info.trusted.as_deref() {
        None => list.push(icon_row(Some("shield"), "Loading…", "", status("…", false, st), st)),
        Some([]) => list.push(icon_row(
            Some("shield"),
            "No trusted certificates",
            "A signature from a certificate you don't trust shows as \"signer not trusted\". Add your organisation's root certificate, or the signer's own, to trust it.",
            div().into_any_element(),
            st,
        )),
        Some(certs) => {
            for (i, c) in certs.iter().enumerate() {
                let path = c.path.clone();
                let ws = ws.clone();
                let remove: Click = Box::new(move |_, cx| {
                    let path = path.clone();
                    let _ = ws.update(cx, move |ws, cx| ws.untrust_cert(path, cx));
                });
                let short: String = c.fingerprint.chars().take(23).collect();
                list.push(icon_row(
                    Some("shield"),
                    &c.subject,
                    &format!("SHA-256 {short}…"),
                    button(TRUST_IDS[i % TRUST_IDS.len()], "Remove", true, st, remove),
                    st,
                ));
            }
        }
    }
    list.push(
        div()
            .flex()
            .gap(px(8.0))
            .child(button(
                "trust-add",
                "Add certificate…",
                false,
                st,
                act(ws, |ws, cx| ws.add_trusted_cert(cx)),
            ))
            .into_any_element(),
    );
    div()
        .flex()
        .flex_col()
        .gap(px(28.0))
        .child(group(
            "Trusted certificates",
            Some("Kitab checks every signature on your computer: the signed bytes, the signature, and whether the signer's certificate leads to one you trust here. It never asks the internet."),
            rows(list),
            st,
        ))
        .into_any_element()
}

pub(super) fn advanced(ws: &WeakEntity<Workspace>, info: &SystemInfo, st: &Style) -> AnyElement {
    let size = info
        .db_size
        .map(|b| format!(" It takes {} now.", crate::reader::human_size(b)))
        .unwrap_or_default();
    let clear_control = if info.confirm_clear {
        div()
            .flex()
            .gap(px(8.0))
            .child(button(
                "history-cancel",
                "Cancel",
                false,
                st,
                act(ws, |ws, cx| ws.ask_clear_history(false, cx)),
            ))
            .child(button(
                "history-confirm",
                "Clear history",
                true,
                st,
                act(ws, |ws, cx| ws.clear_history(cx)),
            ))
            .into_any_element()
    } else {
        button(
            "history-clear",
            "Clear…",
            false,
            st,
            act(ws, |ws, cx| ws.ask_clear_history(true, cx)),
        )
    };
    let folder = kitab_os::data_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_else(|| "Not available".into());
    div()
        .flex()
        .flex_col()
        .gap(px(28.0))
        .child(group(
            "Memory",
            None,
            icon_row(
                Some("memory"),
                "Clear cached pages",
                "Frees the memory held by rendered pages, fonts and images. Pages render again as you read.",
                button("cache-clear", "Clear", false, st, act(ws, |ws, cx| ws.clear_caches(cx))),
                st,
            ),
            st,
        ))
        .child(group(
            "History",
            None,
            icon_row(
                Some("history"),
                "Clear reading history",
                if info.confirm_clear {
                    "Forget Recent files, reading positions and closed tabs? This can't be undone."
                } else {
                    "Forgets Recent files, reading positions and closed tabs. Your marks, bookmarks and open tabs stay."
                },
                clear_control,
                st,
            ),
            st,
        ))
        .child(group(
            "Library database",
            None,
            rows(vec![
                icon_row(
                    Some("database"),
                    "Optimize the database",
                    &format!("Compacts the file that holds your library, marks and bookmarks.{size}"),
                    button("db-optimize", "Optimize", false, st, act(ws, |ws, cx| ws.optimize_db(cx))),
                    st,
                ),
                icon_row(
                    Some("folder-open"),
                    "Data folder",
                    &folder,
                    button("data-open", "Show", false, st, act(ws, |ws, cx| ws.open_data_folder(cx))),
                    st,
                ),
            ]),
            st,
        ))
        .into_any_element()
}

// ── About ────────────────────────────────────────────────────────────────

pub(super) fn about(ws: &WeakEntity<Workspace>, info: &SystemInfo, st: &Style) -> AnyElement {
    let version = crate::shell::VERSION;
    let (line, control) = match &info.update {
        UpdateState::Idle => (
            format!("You have version {version}."),
            button(
                "update-check",
                "Check now",
                false,
                st,
                act(ws, |ws, cx| ws.check_for_updates(true, cx)),
            ),
        ),
        UpdateState::Checking => ("Checking…".to_string(), status("Checking", false, st)),
        UpdateState::UpToDate => (
            format!("Kitab {version} is the latest version."),
            button(
                "update-check",
                "Check again",
                false,
                st,
                act(ws, |ws, cx| ws.check_for_updates(true, cx)),
            ),
        ),
        UpdateState::Available(r) => (
            format!("Kitab {} is available. You have {version}.", r.version),
            button(
                "update-view",
                "View update",
                false,
                st,
                act(ws, |ws, cx| ws.open_update(cx)),
            ),
        ),
        UpdateState::Failed(e) => (
            e.clone(),
            button(
                "update-check",
                "Try again",
                false,
                st,
                act(ws, |ws, cx| ws.check_for_updates(true, cx)),
            ),
        ),
    };
    let link = |id: &'static str,
                glyph: &'static str,
                title: &'static str,
                body: &'static str,
                url: String| {
        let open: Click = Box::new(move |_, cx| cx.open_url(&url));
        icon_row(
            Some(glyph),
            title,
            body,
            button(id, "Open", false, st, open),
            st,
        )
    };
    div()
        .flex()
        .flex_col()
        .gap(px(28.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(20.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(st.c(|c| c.text))
                        .child("Kitab Reader"),
                )
                .child(
                    div()
                        .id("about-version")
                        .text_size(px(12.5))
                        .text_color(st.c(|c| c.muted))
                        .tip("The version you're running")
                        .child(format!("Version {version}")),
                )
                .child(
                    div()
                        .pt(px(4.0))
                        .text_size(px(12.5))
                        .text_color(st.c(|c| c.muted))
                        .child("Your library has a fast lane now. Rust underneath, a calm page on top. Free software by t21."),
                ),
        )
        .child(group(
            "Updates",
            None,
            rows(vec![
                icon_row(Some("download"), "Check for updates", &line, control, st),
                icon_row(
                    Some("refresh"),
                    "Check on start-up",
                    "Shortly after Kitab opens, and every six hours, it asks GitHub for the latest version number. Nothing about you or your files is sent.",
                    switch(
                        "update-auto",
                        st.update_check,
                        st,
                        change(ws, |s: &mut Style| s.update_check = !s.update_check),
                    ),
                    st,
                ),
            ]),
            st,
        ))
        .child(group(
            "Links",
            None,
            rows(vec![
                link("about-repo", "code", "Source code", "github.com/t21dev/kitab", REPO.to_string()),
                link("about-changes", "history", "What's new", "The changelog for every version.", format!("{REPO}/blob/main/CHANGELOG.md")),
                link("about-issue", "issue", "Report a problem", "Opens a new issue on GitHub.", format!("{REPO}/issues/new")),
                link("about-privacy", "shield", "Privacy policy", "No telemetry; your files stay on your computer.", format!("{REPO}/blob/main/PRIVACY.md")),
                link("about-licence", "scale", "Licence", "GNU Affero General Public License 3.0 or later.", format!("{REPO}/blob/main/LICENSE")),
                link("about-notice", "file", "Third-party notices", "MuPDF, GPUI, fonts and the other parts Kitab is built from.", format!("{REPO}/blob/main/NOTICE")),
            ]),
            st,
        ))
        .into_any_element()
}
