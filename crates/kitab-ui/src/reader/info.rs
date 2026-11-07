// SPDX-License-Identifier: AGPL-3.0-or-later

//! The file info chip: one quiet line at the bottom left of the reader with
//! the format, file size, page count and the current page's paper size (or a
//! book's text size, or a Markdown file's lines and words). Its tooltip is the
//! file's full path. Settings › Reading turns it off.

use super::*;
use crate::settings::DockAt;
use kitab_core::Format;

/// "58.9 KB", "1.2 MB", "312 B".
pub(super) fn human_size(bytes: u64) -> String {
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
        Format::Cbz => "Comic",
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

    /// The chip's text: "PDF · 58.9 KB · 40 pages · A4".
    pub(super) fn info_line(&self) -> Option<String> {
        let info = self.info.as_ref()?;
        let mut parts: Vec<String> = vec![format_label(info.format).to_string()];
        if let Some(bytes) = self.file_size {
            parts.push(human_size(bytes));
        }
        if let Some(md) = self.md.as_ref() {
            let lines = md.source.lines().count();
            let words = md.source.split_whitespace().count();
            parts.push(plural(lines, "line", "lines"));
            parts.push(plural(words, "word", "words"));
        } else {
            parts.push(plural(info.page_count as usize, "page", "pages"));
            if let Some(pt) = self.book_text() {
                parts.push(format!("{pt:.0} pt text"));
            } else if let Some(size) = info.page_sizes.get(self.current_page()) {
                let paper = paper_name(size.w, size.h)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{:.0} × {:.0} pt", size.w, size.h));
                parts.push(paper);
            }
        }
        if info.encryption.is_some() {
            parts.push("Encrypted".into());
        }
        Some(parts.join("  ·  "))
    }

    /// The chip, bottom left: above the dock when the dock is at the bottom.
    pub(super) fn info_chip(&self, st: &Style) -> Option<AnyElement> {
        if !st.file_info || self.status != Status::Open || st.minimal {
            return None;
        }
        let line = self.info_line()?;
        let above_dock = st.shows_dock() && !st.dock_hidden && st.dock == DockAt::Bottom;
        let bottom = if above_dock { 16.0 + 44.0 + 8.0 } else { 14.0 };
        let path: SharedString = self.path.to_string_lossy().to_string().into();
        Some(
            div()
                .absolute()
                .bottom(px(bottom))
                .left(px(16.0))
                .child(
                    div()
                        .id("file-info")
                        .px(px(10.0))
                        .py(px(4.0))
                        .rounded(px(8.0))
                        .bg(st.c(|c| c.raised))
                        .border_1()
                        .border_color(st.c(|c| c.line))
                        .shadow(st.elev())
                        .text_size(px(11.5))
                        .text_color(st.c(|c| c.muted))
                        .whitespace_nowrap()
                        .tip(path)
                        .child(line),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
