// SPDX-License-Identifier: AGPL-3.0-or-later
//! What is inside a plain `.zip`, read from its index (the central directory
//! at the end of the file) without unpacking anything.
//!
//! A folder of comics often holds them as `.zip` rather than `.cbz`. A folder
//! scan takes such a file for a book only when it is mostly pictures
//! (GitHub #10), so a zip of anything else stays out of the library.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::Format;

/// End of central directory: the record a zip ends with (plus a comment).
const EOCD: u32 = 0x0605_4b50;
/// ZIP64's own end record, and the locator that points to it.
const EOCD64: u32 = 0x0606_4b50;
const EOCD64_LOCATOR: u32 = 0x0706_4b50;
/// One file's entry in the central directory.
const ENTRY: u32 = 0x0201_4b50;
/// The end record is 22 bytes, its comment at most 65,535.
const TAIL: u64 = 22 + 0xffff;
/// More index than this isn't a comic (about 500,000 entries).
const MAX_INDEX: u64 = 32 << 20;

/// Whether the zip at `path` is a comic: at least one picture, and more
/// pictures than other files. Folders, hidden files and macOS's `__MACOSX`
/// copies don't count either way. False for anything that isn't a readable
/// zip.
pub fn zip_is_pictures(path: &Path) -> bool {
    let Some(names) = zip_names(path) else {
        return false;
    };
    let (mut pictures, mut others) = (0usize, 0usize);
    for name in &names {
        if name.ends_with('/') || name.starts_with("__MACOSX/") {
            continue;
        }
        let file = name.rsplit('/').next().unwrap_or(name);
        if file.is_empty() || file.starts_with('.') {
            continue;
        }
        let picture = file
            .rsplit_once('.')
            .and_then(|(_, ext)| Format::from_extension(ext))
            .is_some_and(|f| f == Format::Image);
        if picture {
            pictures += 1;
        } else {
            others += 1;
        }
    }
    pictures > 0 && pictures > others
}

/// The names in a zip's central directory, or `None` when it isn't a zip.
fn zip_names(path: &Path) -> Option<Vec<String>> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let tail_len = len.min(TAIL);
    let mut tail = vec![0u8; usize::try_from(tail_len).ok()?];
    file.seek(SeekFrom::Start(len - tail_len)).ok()?;
    file.read_exact(&mut tail).ok()?;

    // The end record: its signature, searched for from the end.
    let at = (0..=tail.len().checked_sub(22)?)
        .rev()
        .find(|&i| u32_at(&tail, i) == Some(EOCD))?;
    let mut count = u64::from(u16_at(&tail, at + 10)?);
    let mut size = u64::from(u32_at(&tail, at + 12)?);
    let mut offset = u64::from(u32_at(&tail, at + 16)?);

    // ZIP64: a field that's all ones is in the ZIP64 end record instead.
    if count == 0xffff || size == 0xffff_ffff || offset == 0xffff_ffff {
        let loc = at.checked_sub(20)?;
        if u32_at(&tail, loc)? != EOCD64_LOCATOR {
            return None;
        }
        let rec_at = u64_at(&tail, loc + 8)?;
        let mut rec = [0u8; 56];
        file.seek(SeekFrom::Start(rec_at)).ok()?;
        file.read_exact(&mut rec).ok()?;
        if u32_at(&rec, 0)? != EOCD64 {
            return None;
        }
        count = u64_at(&rec, 32)?;
        size = u64_at(&rec, 40)?;
        offset = u64_at(&rec, 48)?;
    }
    if size > MAX_INDEX || offset.checked_add(size)? > len {
        return None;
    }

    let mut index = vec![0u8; usize::try_from(size).ok()?];
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(&mut index).ok()?;
    let mut names = Vec::new();
    let mut i = 0usize;
    while (names.len() as u64) < count {
        if u32_at(&index, i)? != ENTRY {
            break;
        }
        let name_len = usize::from(u16_at(&index, i + 28)?);
        let extra_len = usize::from(u16_at(&index, i + 30)?);
        let comment_len = usize::from(u16_at(&index, i + 32)?);
        let name = index.get(i + 46..i + 46 + name_len)?;
        names.push(String::from_utf8_lossy(name).replace('\\', "/"));
        i += 46 + name_len + extra_len + comment_len;
    }
    Some(names)
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored (uncompressed) zip of empty files with these names.
    fn zip_of(names: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut index = Vec::new();
        for name in names {
            let local = out.len() as u32;
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&[0; 12]); // crc, sizes
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());

            index.extend_from_slice(&ENTRY.to_le_bytes());
            index.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            index.extend_from_slice(&[0; 12]); // crc, sizes
            index.extend_from_slice(&(name.len() as u16).to_le_bytes());
            index.extend_from_slice(&[0; 12]); // extra, comment, disk, attrs
            index.extend_from_slice(&local.to_le_bytes());
            index.extend_from_slice(name.as_bytes());
        }
        let offset = out.len() as u32;
        out.extend_from_slice(&index);
        out.extend_from_slice(&EOCD.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(names.len() as u16).to_le_bytes());
        out.extend_from_slice(&(names.len() as u16).to_le_bytes());
        out.extend_from_slice(&(index.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn check(names: &[&str]) -> bool {
        let dir = std::env::temp_dir().join(format!("kitab-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{}.zip", names.join("_").replace(['/', '.'], "-")));
        std::fs::write(&path, zip_of(names)).unwrap();
        let got = zip_is_pictures(&path);
        let _ = std::fs::remove_file(&path);
        got
    }

    #[test]
    fn pictures_make_a_comic() {
        assert!(check(&["001.jpg", "002.JPG", "003.png"]));
        assert!(check(&[
            "Vol 1/",
            "Vol 1/01.webp",
            "Vol 1/02.webp",
            "ComicInfo.xml"
        ]));
        assert!(check(&[
            "01.jpg",
            "__MACOSX/._01.jpg",
            "__MACOSX/._02.jpg",
            ".DS_Store"
        ]));
    }

    #[test]
    fn other_zips_dont() {
        assert!(!check(&["src/main.rs", "Cargo.toml", "logo.png"]));
        assert!(!check(&["readme.txt"]));
        assert!(!check(&[]));
        assert!(!check(&["dir/"]));
    }

    #[test]
    fn not_a_zip() {
        let path = std::env::temp_dir().join(format!("kitab-notzip-{}.zip", std::process::id()));
        std::fs::write(&path, b"plain text, not a zip at all").unwrap();
        assert!(!zip_is_pictures(&path));
        let _ = std::fs::remove_file(&path);
        assert!(!zip_is_pictures(Path::new("no/such/file.zip")));
    }
}
