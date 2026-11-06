// SPDX-License-Identifier: AGPL-3.0-or-later

//! Hairline icons (DESIGN.md § Icons: 24 grid, 1.4 px stroke, round caps),
//! embedded in the binary so nothing is read from disk at start-up.

use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

const ICONS: &[(&str, &[u8])] = &[
    (
        "icons/alert.svg",
        include_bytes!("../../../assets/icons/alert.svg"),
    ),
    (
        "icons/book.svg",
        include_bytes!("../../../assets/icons/book.svg"),
    ),
    (
        "icons/bookmark.svg",
        include_bytes!("../../../assets/icons/bookmark.svg"),
    ),
    (
        "icons/check.svg",
        include_bytes!("../../../assets/icons/check.svg"),
    ),
    (
        "icons/chevron-down.svg",
        include_bytes!("../../../assets/icons/chevron-down.svg"),
    ),
    (
        "icons/chevron-left.svg",
        include_bytes!("../../../assets/icons/chevron-left.svg"),
    ),
    (
        "icons/chevron-right.svg",
        include_bytes!("../../../assets/icons/chevron-right.svg"),
    ),
    (
        "icons/close.svg",
        include_bytes!("../../../assets/icons/close.svg"),
    ),
    (
        "icons/copy.svg",
        include_bytes!("../../../assets/icons/copy.svg"),
    ),
    (
        "icons/density-compact.svg",
        include_bytes!("../../../assets/icons/density-compact.svg"),
    ),
    (
        "icons/density-dense.svg",
        include_bytes!("../../../assets/icons/density-dense.svg"),
    ),
    (
        "icons/density-standard.svg",
        include_bytes!("../../../assets/icons/density-standard.svg"),
    ),
    (
        "icons/file.svg",
        include_bytes!("../../../assets/icons/file.svg"),
    ),
    (
        "icons/fit-page.svg",
        include_bytes!("../../../assets/icons/fit-page.svg"),
    ),
    (
        "icons/fit-width.svg",
        include_bytes!("../../../assets/icons/fit-width.svg"),
    ),
    (
        "icons/focus.svg",
        include_bytes!("../../../assets/icons/focus.svg"),
    ),
    (
        "icons/folder-open.svg",
        include_bytes!("../../../assets/icons/folder-open.svg"),
    ),
    (
        "icons/hash.svg",
        include_bytes!("../../../assets/icons/hash.svg"),
    ),
    (
        "icons/highlight.svg",
        include_bytes!("../../../assets/icons/highlight.svg"),
    ),
    (
        "icons/home.svg",
        include_bytes!("../../../assets/icons/home.svg"),
    ),
    (
        "icons/image.svg",
        include_bytes!("../../../assets/icons/image.svg"),
    ),
    (
        "icons/info.svg",
        include_bytes!("../../../assets/icons/info.svg"),
    ),
    (
        "icons/keyboard.svg",
        include_bytes!("../../../assets/icons/keyboard.svg"),
    ),
    (
        "icons/lock.svg",
        include_bytes!("../../../assets/icons/lock.svg"),
    ),
    (
        "icons/markdown.svg",
        include_bytes!("../../../assets/icons/markdown.svg"),
    ),
    (
        "icons/minus.svg",
        include_bytes!("../../../assets/icons/minus.svg"),
    ),
    (
        "icons/moon.svg",
        include_bytes!("../../../assets/icons/moon.svg"),
    ),
    (
        "icons/more.svg",
        include_bytes!("../../../assets/icons/more.svg"),
    ),
    (
        "icons/more-vertical.svg",
        include_bytes!("../../../assets/icons/more-vertical.svg"),
    ),
    (
        "icons/outline.svg",
        include_bytes!("../../../assets/icons/outline.svg"),
    ),
    (
        "icons/pages.svg",
        include_bytes!("../../../assets/icons/pages.svg"),
    ),
    (
        "icons/plus.svg",
        include_bytes!("../../../assets/icons/plus.svg"),
    ),
    (
        "icons/rotate.svg",
        include_bytes!("../../../assets/icons/rotate.svg"),
    ),
    (
        "icons/search.svg",
        include_bytes!("../../../assets/icons/search.svg"),
    ),
    (
        "icons/settings.svg",
        include_bytes!("../../../assets/icons/settings.svg"),
    ),
    (
        "icons/sidebar.svg",
        include_bytes!("../../../assets/icons/sidebar.svg"),
    ),
    (
        "icons/sun.svg",
        include_bytes!("../../../assets/icons/sun.svg"),
    ),
    (
        "icons/text.svg",
        include_bytes!("../../../assets/icons/text.svg"),
    ),
    (
        "icons/win-max.svg",
        include_bytes!("../../../assets/icons/win-max.svg"),
    ),
    (
        "icons/win-min.svg",
        include_bytes!("../../../assets/icons/win-min.svg"),
    ),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, b)| Cow::Borrowed(*b)))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, _)| SharedString::from(*p))
            .collect())
    }
}

/// An icon's asset path by name, e.g. `icon("home")`.
pub fn icon(name: &str) -> SharedString {
    SharedString::from(format!("icons/{name}.svg"))
}
