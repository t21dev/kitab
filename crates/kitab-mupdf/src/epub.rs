// SPDX-License-Identifier: AGPL-3.0-or-later

//! What an EPUB's package document says that MuPDF doesn't act on: fixed
//! layout (`rendition:layout pre-paginated`) with its viewport, and the page
//! progression direction (right to left for Arabic, Hebrew, manga).
//!
//! Kitab's small ZIP reader ([`crate::zip`]) reads only
//! `META-INF/container.xml`, the OPF and, for fixed layout, the first spine
//! document. Everything is bounded: the file is hostile (AGENTS.md rule 8).
//! roxmltree is already in kitab.exe through GPUI.

use crate::zip::Zip;
use std::path::Path;

/// Facts from the package document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EpubMeta {
    pub fixed_layout: bool,
    /// The fixed-layout viewport in CSS pixels, from the OPF or the first page.
    pub viewport: Option<(f32, f32)>,
    pub right_to_left: bool,
    pub spine_items: usize,
}

/// The most an entry we read may hold, packed or unpacked.
const MAX_ENTRY: u64 = 8 << 20;

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

/// `width=600, height=800` (an OPF `rendition:viewport` or an XHTML viewport meta).
pub fn parse_viewport(s: &str) -> Option<(f32, f32)> {
    let mut w = None;
    let mut h = None;
    for part in s.split([',', ';']) {
        let (k, v) = part.split_once('=')?;
        let v: f32 = v.trim().trim_end_matches("px").parse().ok()?;
        match k.trim() {
            "width" => w = Some(v),
            "height" => h = Some(v),
            _ => {}
        }
    }
    let (w, h) = (w?, h?);
    let ok = |v: f32| v.is_finite() && (50.0..=10_000.0).contains(&v);
    (ok(w) && ok(h)).then_some((w, h))
}

/// The `content` of `<meta name="viewport" …>` in an XHTML page.
fn viewport_meta(xhtml: &str) -> Option<(f32, f32)> {
    let at = xhtml.find("name=\"viewport\"")?;
    let tag_start = xhtml[..at].rfind('<')?;
    let tag_end = at + xhtml[at..].find('>')?;
    let tag = &xhtml[tag_start..tag_end];
    let c = tag.find("content=\"")? + "content=\"".len();
    let end = tag[c..].find('"')?;
    parse_viewport(&tag[c..c + end])
}

/// Read the package document's facts. `None` when the file isn't a readable EPUB.
pub fn read(path: &Path) -> Option<EpubMeta> {
    let mut zip = Zip::open_path(path)?;
    let container = text(zip.read("META-INF/container.xml", MAX_ENTRY)?);
    let container = roxmltree::Document::parse(&container).ok()?;
    let opf_path = container
        .descendants()
        .find(|n| n.has_tag_name("rootfile") || n.tag_name().name() == "rootfile")?
        .attribute("full-path")?
        .to_string();
    let opf_text = text(zip.read(&opf_path, MAX_ENTRY)?);
    let opf = roxmltree::Document::parse(&opf_text).ok()?;
    let mut meta = EpubMeta::default();
    let mut opf_viewport = None;
    for n in opf.descendants().filter(|n| n.tag_name().name() == "meta") {
        let value = n.text().unwrap_or("").trim();
        match n.attribute("property") {
            Some("rendition:layout") => meta.fixed_layout = value == "pre-paginated",
            Some("rendition:viewport") => opf_viewport = parse_viewport(value),
            _ => {}
        }
    }
    let spine = opf.descendants().find(|n| n.tag_name().name() == "spine")?;
    meta.right_to_left = spine.attribute("page-progression-direction") == Some("rtl");
    let refs: Vec<&str> = spine
        .children()
        .filter(|n| n.tag_name().name() == "itemref")
        .filter_map(|n| n.attribute("idref"))
        .collect();
    meta.spine_items = refs.len();
    if meta.fixed_layout {
        meta.viewport = opf_viewport.or_else(|| {
            let href = opf
                .descendants()
                .find(|n| {
                    n.tag_name().name() == "item" && n.attribute("id") == refs.first().copied()
                })?
                .attribute("href")?;
            let base = opf_path.rsplit_once('/').map_or("", |(b, _)| b);
            let first = if base.is_empty() {
                href.to_string()
            } else {
                format!("{base}/{href}")
            };
            viewport_meta(&text(zip.read(&first, MAX_ENTRY)?))
        });
    }
    Some(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewports_parse_and_bad_ones_do_not() {
        assert_eq!(
            parse_viewport("width=600, height=800"),
            Some((600.0, 800.0))
        );
        assert_eq!(
            parse_viewport("height=1200;width=900px"),
            Some((900.0, 1200.0))
        );
        assert_eq!(parse_viewport("width=0, height=800"), None);
        assert_eq!(parse_viewport("width=NaN, height=800"), None);
        assert_eq!(parse_viewport("device-width"), None);
        assert_eq!(
            viewport_meta(
                r#"<head><meta name="viewport" content="width=600, height=800"/></head>"#
            ),
            Some((600.0, 800.0))
        );
    }

    #[test]
    fn not_a_zip_is_none() {
        let dir = std::env::temp_dir().join(format!("kitab-epub-meta-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let p = dir.join("x.epub");
        std::fs::write(&p, b"This is not a zip archive.\n").expect("write");
        assert_eq!(read(&p), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
