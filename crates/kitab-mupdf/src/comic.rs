// SPDX-License-Identifier: AGPL-3.0-or-later

//! Comic books in RAR and 7z archives (`.cbr`, `.cb7`), and ZIP comics with
//! WebP pages: the pictures, in reading order. Other ZIP and TAR comics stay
//! with MuPDF's own comic reader, which knows them by their content; so does
//! a `.cbr` that is really a ZIP. MuPDF's reader leaves WebP pages out, so a
//! ZIP with any comes here (GitHub #18).

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

/// The most one page of a ZIP comic may unpack to.
const MAX_ZIP_PAGE: u64 = 256 << 20;

/// Whether a ZIP comic needs Kitab's own reader: it has WebP pages, which
/// MuPDF's comic reader skips. Reads only the table of contents.
pub(crate) fn zip_needs_kitab<R: std::io::Read + std::io::Seek>(file: R) -> bool {
    crate::zip::Zip::new(file).is_some_and(|z| {
        z.entries.iter().any(|e| {
            is_page(&e.name)
                && e.name
                    .rsplit_once('.')
                    .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("webp"))
        })
    })
}

/// [`zip_needs_kitab`] for the file at `path`.
pub(crate) fn zip_file_needs_kitab(path: &std::path::Path) -> bool {
    std::fs::File::open(path).is_ok_and(zip_needs_kitab)
}

fn zip_pages(bytes: &[u8], limits: &rar::Limits, first_only: bool) -> Result<Vec<Page>, String> {
    let mut zip =
        crate::zip::Zip::new(std::io::Cursor::new(bytes)).ok_or("the archive can't be read")?;
    if zip.entries.len() > limits.files {
        return Err("the archive has too many files".into());
    }
    let mut wanted: Vec<usize> = (0..zip.entries.len())
        .filter(|&i| is_page(&zip.entries[i].name))
        .collect();
    wanted.sort_by(|&a, &b| natural_cmp(&zip.entries[a].name, &zip.entries[b].name));
    if first_only {
        wanted.truncate(1);
    }
    let mut pages = Vec::with_capacity(wanted.len());
    let mut total = 0u64;
    for ix in wanted {
        total = total.saturating_add(zip.entries[ix].size);
        if total > limits.total {
            return Err("the archive is too large to open".into());
        }
        // A page that can't be unpacked (encrypted, another compression) is
        // left out, as MuPDF's reader would.
        if let Some(data) = zip.read_at(ix, MAX_ZIP_PAGE) {
            pages.push(Page {
                name: zip.entries[ix].name.clone(),
                data,
            });
        }
    }
    Ok(pages)
}

/// Read the pictures out of a RAR, 7z or ZIP comic: all of them, or only the
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
        Kind::Mupdf if bytes.starts_with(b"PK") => zip_pages(bytes, &limits, first_only)?,
        Kind::Mupdf | Kind::Unknown => return Err("not a RAR, 7z or ZIP comic".into()),
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

    /// GitHub #18: a ZIP with WebP pages is read by Kitab, in page order;
    /// one of only JPEGs stays with MuPDF.
    #[test]
    fn zip_comics_with_webp_pages_are_read_here() {
        use crate::zip::tests::stored;
        let webp = stored(&[
            ("p10.webp", b"ten"),
            ("ComicInfo.xml", b"<x/>"),
            ("p2.webp", b"two"),
            ("cover.jpg", b"jpg"),
        ]);
        assert!(zip_needs_kitab(std::io::Cursor::new(webp.as_slice())));
        let comic = load(&webp, false).unwrap();
        let names: Vec<_> = comic.pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["cover.jpg", "p2.webp", "p10.webp"]);
        assert_eq!(comic.pages[1].data, b"two");
        let first = load(&webp, true).unwrap();
        assert_eq!(first.pages.len(), 1);
        assert_eq!(first.pages[0].name, "cover.jpg");

        let jpegs = stored(&[("1.jpg", b"a"), ("2.JPEG", b"b")]);
        assert!(!zip_needs_kitab(std::io::Cursor::new(jpegs.as_slice())));
    }
}
