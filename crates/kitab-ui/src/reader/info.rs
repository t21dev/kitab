// SPDX-License-Identifier: AGPL-3.0-or-later

//! The status bar: a slim strip pinned to the bottom of the reader. Facts
//! about the file on the left (format, size, pages or lines, words and
//! reading time for Markdown, paper or text size), where you are on the
//! right (position, zoom). Each item is a switch in Settings › Reading ›
//! Status bar; Ctrl+I shows or hides the bar. The left side's tooltip is the
//! file's full path.

use super::*;
use crate::settings::StatusItems;
use kitab_core::Format;

/// The bar's height; the dock and the page navigator float above it.
pub const BAR_H: f32 = 26.0;
/// Reading speed for "N min read".
const WORDS_PER_MINUTE: usize = 230;

/// "58.9 KB", "1.2 MB", "312 B".
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64 / 1024.0;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if v >= 100.0 {
        format!("{v:.0} {}", UNITS[unit])
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

/// A file's path for the status bar, the home folder shown as ~.
pub(super) fn short_path(path: &std::path::Path) -> String {
    let full = path.to_string_lossy().to_string();
    let full = full.strip_prefix(r"\\?\").unwrap_or(&full).to_string();
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    if !home.is_empty() && full.to_lowercase().starts_with(&home.to_lowercase()) {
        let rest = &full[home.len()..];
        if rest.is_empty() || rest.starts_with(['\\', '/']) {
            return format!("~{rest}");
        }
    }
    full
}

/// The paper a page size is, either way round, within 2 pt.
pub(super) fn paper_name(w: f32, h: f32) -> Option<&'static str> {
    const PAPERS: [(&str, f32, f32); 7] = [
        ("A3", 842.0, 1191.0),
        ("A4", 595.0, 842.0),
        ("A5", 420.0, 595.0),
        ("Letter", 612.0, 792.0),
        ("Legal", 612.0, 1008.0),
        ("Tabloid", 792.0, 1224.0),
        ("B5", 499.0, 709.0),
    ];
    let (a, b) = (w.min(h), w.max(h));
    PAPERS
        .iter()
        .find(|(_, pw, ph)| (a - pw).abs() <= 2.0 && (b - ph).abs() <= 2.0)
        .map(|(n, ..)| *n)
}

fn format_label(f: Format) -> &'static str {
    match f {
        Format::Pdf => "PDF",
        Format::Xps => "XPS",
        Format::Epub => "EPUB",
        Format::Mobi => "MOBI",
        Format::Fb2 => "FB2",
        Format::Cbz | Format::Cbr | Format::Cb7 => "Comic",
        Format::Djvu => "DjVu",
        Format::Image => "Image",
        Format::Txt => "Text",
        Format::Markdown => "Markdown",
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    let word = if n == 1 { one } else { many };
    format!("{n} {word}")
}

impl Reader {
    /// Read the file's size off the UI thread (once, after opening).
    pub(super) fn load_file_size(&mut self, cx: &mut Context<Self>) {
        let path = self.path.clone();
        cx.spawn(async move |this, cx| {
            let size = cx
                .background_spawn(async move { std::fs::metadata(&path).ok().map(|m| m.len()) })
                .await;
            let _ = this.update(cx, |r, cx| {
                r.file_size = size;
                cx.notify();
            });
        })
        .detach();
    }

    /// The left side: facts about the file.
    pub(super) fn status_left(&self, items: StatusItems) -> Vec<String> {
        let Some(info) = self.info.as_ref() else {
            return Vec::new();
        };
        let mut parts = Vec::new();
        if items.format {
            parts.push(format_label(info.format).to_string());
        }
        if items.size
            && let Some(bytes) = self.file_size
        {
            parts.push(human_size(bytes));
        }
        if let Some(md) = self.md.as_ref() {
            let words = md.source.split_whitespace().count();
            if items.pages {
                parts.push(plural(md.source.lines().count(), "line", "lines"));
            }
            if items.words {
                parts.push(plural(words, "word", "words"));
            }
            if items.reading_time && words > 0 {
                parts.push(format!("{} min read", words.div_ceil(WORDS_PER_MINUTE)));
            }
        } else {
            if items.pages {
                parts.push(plural(info.page_count as usize, "page", "pages"));
            }
            if items.paper {
                if let Some(pt) = self.book_text() {
                    parts.push(format!("{pt:.0} pt text"));
                } else if let Some(size) = info.page_sizes.get(self.current_page()) {
                    let paper = paper_name(size.w, size.h)
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("{:.0} × {:.0} pt", size.w, size.h));
                    parts.push(paper);
                }
            }
        }
        if info.encryption.is_some() {
            parts.push("Encrypted".into());
        }
        // Last, so a long path is what gets cut short, not the facts.
        if items.path {
            parts.push(short_path(&self.path));
        }
        parts
    }

    /// The right side: where you are.
    pub(super) fn status_right(&self, items: StatusItems) -> Vec<String> {
        let mut parts = Vec::new();
        let count = self.page_count().max(1);
        if items.position {
            if self.md.is_some() {
                let at = self.current_page();
                let percent = if count > 1 {
                    (at as f32 / (count - 1) as f32 * 100.0).round() as u32
                } else {
                    100
                };
                parts.push(format!("{percent} %"));
            } else {
                parts.push(format!("Page {} of {count}", self.current_page() + 1));
            }
        }
        // A book's zoom is its text size, shown on the left.
        if items.zoom && self.md.is_none() && self.book_text().is_none() {
            parts.push(format!("{} %", self.zoom_percent()));
        }
        parts
    }

    /// Whether the bar shows: on in Settings, an open document, not in
    /// minimal mode.
    pub(super) fn status_bar_shown(&self, st: &Style) -> bool {
        st.file_info && self.status == Status::Open && !st.minimal
    }

    /// The bar, pinned to the bottom of the reader.
    pub(super) fn status_bar(&self, st: &Style, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.status_bar_shown(st) {
            return None;
        }
        let badge = self.signature_badge(st, cx);
        let left = self.status_left(st.status_items).join("  ·  ");
        let right = self.status_right(st.status_items).join("  ·  ");
        let path: SharedString = self.path.to_string_lossy().to_string().into();
        Some(
            div()
                .id("status-bar")
                .flex_none()
                .h(px(BAR_H))
                .w_full()
                .flex()
                .items_center()
                .gap(px(16.0))
                .px(px(12.0))
                .bg(st.c(|c| c.bg))
                .border_t_1()
                .border_color(st.c(|c| c.line))
                .text_size(px(11.5))
                .text_color(st.c(|c| c.muted))
                .whitespace_nowrap()
                .children(badge)
                .child(
                    div()
                        .id("status-left")
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .tip(path)
                        .child(left),
                )
                .child(div().flex_none().child(right))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_under_home_starts_with_a_tilde() {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        if home.is_empty() {
            return;
        }
        let p = std::path::Path::new(&home).join("Books").join("a.pdf");
        let s = short_path(&p);
        assert!(s.starts_with('~') && s.ends_with("a.pdf"), "{s}");
        // A folder that only starts like home is shown in full.
        let other = format!("{home}x");
        assert!(!short_path(std::path::Path::new(&other)).starts_with('~'));
    }

    #[test]
    fn sizes_read_like_a_file_manager() {
        assert_eq!(human_size(312), "312 B");
        assert_eq!(human_size(60_307), "58.9 KB");
        assert_eq!(human_size(22_718_976), "21.7 MB");
        assert_eq!(human_size(300 * 1024 * 1024), "300 MB");
    }

    #[test]
    fn paper_sizes_either_way_round() {
        assert_eq!(paper_name(595.0, 842.0), Some("A4"));
        assert_eq!(paper_name(842.0, 595.3), Some("A4"));
        assert_eq!(paper_name(612.0, 792.0), Some("Letter"));
        assert_eq!(paper_name(400.0, 600.0), None);
    }
}
