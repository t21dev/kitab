// SPDX-License-Identifier: AGPL-3.0-or-later

//! Comic books in RAR and 7z archives (`.cbr`, `.cb7`): the pictures, in
//! reading order. ZIP and TAR comics stay with MuPDF's own comic reader, which
//! knows them by their content; so does a `.cbr` that is really a ZIP.

use std::io::Read;

use crate::rar;

/// What a comic archive really is, from its first bytes (the extension often
/// lies: many `.cbr` files are ZIPs, some `.cbz` files are RARs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// ZIP or TAR: MuPDF reads it.
    Mupdf,
    Rar,
    SevenZ,
    Unknown,
}

const SEVEN_Z: &[u8] = b"7z\xbc\xaf\x27\x1c";

pub(crate) fn sniff(head: &[u8]) -> Kind {
    if rar::is_rar(head) {
        Kind::Rar
    } else if head.starts_with(SEVEN_Z) {
        Kind::SevenZ
    } else if head.starts_with(b"PK") || head.get(257..262) == Some(b"ustar".as_slice()) {
        Kind::Mupdf
    } else {
        Kind::Unknown
    }
}

/// What a file of `path` really is, from its first bytes.
pub(crate) fn sniff_file(path: &std::path::Path) -> Kind {
    let mut head = [0u8; 512];
    let n = std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .unwrap_or(0);
    sniff(&head[..n])
}

/// One page: the picture file's name and its bytes, still encoded.
pub(crate) struct Page {
    pub name: String,
    pub data: Vec<u8>,
}

/// The pages of a comic, in reading order.
pub(crate) struct Comic {
    pub pages: Vec<Page>,
}

/// Read the pictures out of a RAR or 7z comic: all of them, or only the
/// first page (a thumbnail), decoding no further than it needs.
pub(crate) fn load(bytes: &[u8], first_only: bool) -> Result<Comic, String> {
    let limits = rar::Limits::COMIC;
    let mut pages: Vec<Page> = match sniff(bytes) {
        Kind::Rar => {
            let first = if first_only {
                let first = rar::names(bytes, &limits)?
                    .into_iter()
                    .filter(|n| is_page(n))
                    .min_by(|a, b| natural_cmp(a, b));
                Some(first.ok_or("the archive has no pictures")?)
            } else {
                None
            };
            let want = |n: &str| is_page(n) && first.as_deref().is_none_or(|f| f == n);
            rar::extract(bytes, want, &limits)?
                .into_iter()
                .map(|e| Page {
                    name: e.name,
                    data: e.data,
                })
                .collect()
        }
        Kind::SevenZ => seven_z(bytes, &limits, first_only)?,
        Kind::Mupdf | Kind::Unknown => return Err("not a RAR or 7z comic".into()),
    };
    pages.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    if pages.is_empty() {
        return Err("the archive has no pictures".into());
    }
    Ok(Comic { pages })
}

fn seven_z(bytes: &[u8], limits: &rar::Limits, first_only: bool) -> Result<Vec<Page>, String> {
    use sevenz_rust2::{ArchiveReader, Password};
    let mut reader =
        ArchiveReader::new(std::io::Cursor::new(bytes), Password::empty()).map_err(seven_z_err)?;
    let files = &reader.archive().files;
    if files.len() > limits.files {
        return Err("the archive has too many files".into());
    }
    let first: Option<String> = if first_only {
        let first = files
            .iter()
            .filter(|f| !f.is_directory && f.has_stream && is_page(&f.name))
            .map(|f| f.name.clone())
            .min_by(|a, b| natural_cmp(a, b));
        Some(first.ok_or("the archive has no pictures")?)
    } else {
        None
    };
    let mut pages = Vec::new();
    let mut total = 0u64;
    let mut too_big = false;
    reader
        .for_each_entries(|entry, data| {
            if entry.is_directory
                || !entry.has_stream
                || !is_page(&entry.name)
                || first.as_deref().is_some_and(|f| f != entry.name)
            {
                // Left unread: the reader decodes past it (solid blocks need it).
                return Ok(true);
            }
            total = total.saturating_add(entry.size);
            if total > limits.total {
                too_big = true;
                return Ok(false);
            }
            let mut buf =
                Vec::with_capacity(usize::try_from(entry.size).unwrap_or(0).min(64 << 20));
            data.read_to_end(&mut buf)?;
            pages.push(Page {
                name: entry.name.replace('\\', "/"),
                data: buf,
            });
            // The first page is all a thumbnail needs: stop there.
            Ok(first.is_none())
        })
        .map_err(seven_z_err)?;
    if too_big {
        return Err("the archive is too large to open".into());
    }
    Ok(pages)
}

fn seven_z_err(e: sevenz_rust2::Error) -> String {
    let text = e.to_string();
    if text.to_ascii_lowercase().contains("password") {
        "password-protected archives aren't supported".into()
    } else {
        format!("the archive can't be read ({text})")
    }
}

/// Picture files MuPDF's comic reader shows, plus WebP (decoded by Kitab).
/// macOS resource forks (`__MACOSX/`, `._name`) are not pages.
pub(crate) fn is_page(name: &str) -> bool {
    const EXT: &[&str] = &[
        "bmp", "gif", "hdp", "j2k", "jb2", "jbig2", "jp2", "jpeg", "jpg", "jpx", "jxr", "pam",
        "pbm", "pgm", "pkm", "png", "pnm", "ppm", "tif", "tiff", "wdp", "webp",
    ];
    let file = name.rsplit('/').next().unwrap_or(name);
    if name.starts_with("__MACOSX/") || name.contains("/__MACOSX/") || file.starts_with("._") {
        return false;
    }
    file.rsplit_once('.')
        .is_some_and(|(_, ext)| EXT.iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

/// Page order as MuPDF's comic reader sorts it: runs of digits compare as
/// numbers ("2" before "10"), letters without case.
pub(crate) fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        let (x, y);
        if a.get(i).is_some_and(u8::is_ascii_digit) && b.get(j).is_some_and(u8::is_ascii_digit) {
            let start = (i, j);
            while a.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            while b.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            // Compare the numbers without overflow: drop leading zeros, then
            // the longer is bigger, then digit by digit.
            let na = trim_zeros(&a[start.0..i]);
            let nb = trim_zeros(&b[start.1..j]);
            let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
            if ord.is_ne() {
                return ord;
            }
            continue;
        } else {
            x = a.get(i).map_or(0, u8::to_ascii_uppercase);
            y = b.get(j).map_or(0, u8::to_ascii_uppercase);
            i += 1;
            j += 1;
        }
        if x != y {
            return x.cmp(&y);
        }
    }
    std::cmp::Ordering::Equal
}

fn trim_zeros(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|&&c| c == b'0').count();
    &s[n.min(s.len().saturating_sub(1))..]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering::*;

    #[test]
    fn pages_sort_like_people_count() {
        let mut v = vec!["p10.jpg", "p2.jpg", "P1.jpg", "p02b.jpg", "cover.jpg"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["cover.jpg", "P1.jpg", "p2.jpg", "p02b.jpg", "p10.jpg"]);
        assert_eq!(
            natural_cmp("a99999999999999999999999", "a100000000000000000000000"),
            Less
        );
        assert_eq!(natural_cmp("x", "x"), Equal);
    }

    #[test]
    fn only_pictures_are_pages() {
        assert!(is_page("Vol 1/001.JPG"));
        assert!(is_page("page.webp"));
        assert!(!is_page("ComicInfo.xml"));
        assert!(!is_page("__MACOSX/._001.jpg"));
        assert!(!is_page("dir/._001.jpg"));
        assert!(!is_page("jpg"));
    }

    #[test]
    fn sniffing_ignores_the_extension() {
        assert_eq!(sniff(b"Rar!\x1a\x07\x00"), Kind::Rar);
        assert_eq!(sniff(b"Rar!\x1a\x07\x01\x00"), Kind::Rar);
        assert_eq!(sniff(b"7z\xbc\xaf\x27\x1c\x00\x04"), Kind::SevenZ);
        assert_eq!(sniff(b"PK\x03\x04"), Kind::Mupdf);
        assert_eq!(sniff(b"hello"), Kind::Unknown);
        assert!(load(b"hello", false).is_err());
    }
}
