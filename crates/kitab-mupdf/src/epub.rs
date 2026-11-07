// SPDX-License-Identifier: AGPL-3.0-or-later

//! What an EPUB's package document says that MuPDF doesn't act on: fixed
//! layout (`rendition:layout pre-paginated`) with its viewport, and the page
//! progression direction (right to left for Arabic, Hebrew, manga).
//!
//! A small ZIP reader (central directory, stored or deflated entries) reads
//! only `META-INF/container.xml`, the OPF and, for fixed layout, the first
//! spine document. Everything is bounded: the file is hostile (AGENTS.md
//! rule 8). flate2 and roxmltree are already in kitab.exe through GPUI.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
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

const MAX_CENTRAL_DIR: u64 = 16 << 20;
const MAX_ENTRIES: usize = 100_000;
/// The most an entry we read may hold, packed or unpacked.
const MAX_ENTRY: u64 = 8 << 20;

struct Entry {
    name: String,
    method: u16,
    packed: u64,
    size: u64,
    offset: u64,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

struct Zip {
    file: File,
    entries: Vec<Entry>,
}

impl Zip {
    fn open(path: &Path) -> Option<Zip> {
        let mut file = File::open(path).ok()?;
        let len = file.metadata().ok()?.len();
        // End of central directory: 22 bytes plus a comment of up to 64 KiB.
        let tail_len = len.min(22 + 65_535);
        file.seek(SeekFrom::Start(len - tail_len)).ok()?;
        let mut tail = vec![0u8; tail_len as usize];
        file.read_exact(&mut tail).ok()?;
        let eocd = (0..tail.len().saturating_sub(21))
            .rev()
            .find(|&i| u32_at(&tail, i) == Some(0x0605_4b50))?;
        let count = u16_at(&tail, eocd + 10)? as usize;
        let cd_size = u64::from(u32_at(&tail, eocd + 12)?);
        let cd_offset = u64::from(u32_at(&tail, eocd + 16)?);
        if cd_size > MAX_CENTRAL_DIR || cd_offset + cd_size > len || count > MAX_ENTRIES {
            return None;
        }
        file.seek(SeekFrom::Start(cd_offset)).ok()?;
        let mut cd = vec![0u8; cd_size as usize];
        file.read_exact(&mut cd).ok()?;
        let mut entries = Vec::with_capacity(count);
        let mut at = 0usize;
        while entries.len() < count && u32_at(&cd, at) == Some(0x0201_4b50) {
            let name_len = u16_at(&cd, at + 28)? as usize;
            let extra = u16_at(&cd, at + 30)? as usize;
            let comment = u16_at(&cd, at + 32)? as usize;
            let name = cd.get(at + 46..at + 46 + name_len)?;
            entries.push(Entry {
                name: String::from_utf8_lossy(name).into_owned(),
                method: u16_at(&cd, at + 10)?,
                packed: u64::from(u32_at(&cd, at + 20)?),
                size: u64::from(u32_at(&cd, at + 24)?),
                offset: u64::from(u32_at(&cd, at + 42)?),
            });
            at += 46 + name_len + extra + comment;
        }
        Some(Zip { file, entries })
    }

    /// An entry's bytes, if it is small enough and stored or deflated.
    fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        let e = self.entries.iter().find(|e| e.name == name)?;
        if e.packed > MAX_ENTRY || e.size > MAX_ENTRY {
            return None;
        }
        let (method, packed, size, offset) = (e.method, e.packed, e.size, e.offset);
        self.file.seek(SeekFrom::Start(offset)).ok()?;
        let mut head = [0u8; 30];
        self.file.read_exact(&mut head).ok()?;
        if u32_at(&head, 0) != Some(0x0403_4b50) {
            return None;
        }
        let skip = u64::from(u16_at(&head, 26)?) + u64::from(u16_at(&head, 28)?);
        self.file.seek(SeekFrom::Current(skip as i64)).ok()?;
        let mut data = vec![0u8; packed as usize];
        self.file.read_exact(&mut data).ok()?;
        match method {
            0 => Some(data),
            8 => {
                let mut out = Vec::with_capacity(size as usize);
                flate2::read::DeflateDecoder::new(data.as_slice())
                    .take(MAX_ENTRY + 1)
                    .read_to_end(&mut out)
                    .ok()?;
                (out.len() as u64 <= MAX_ENTRY).then_some(out)
            }
            _ => None,
        }
    }
}

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
    let mut zip = Zip::open(path)?;
    let container = text(zip.read("META-INF/container.xml")?);
    let container = roxmltree::Document::parse(&container).ok()?;
    let opf_path = container
        .descendants()
        .find(|n| n.has_tag_name("rootfile") || n.tag_name().name() == "rootfile")?
        .attribute("full-path")?
        .to_string();
    let opf_text = text(zip.read(&opf_path)?);
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
            viewport_meta(&text(zip.read(&first)?))
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
