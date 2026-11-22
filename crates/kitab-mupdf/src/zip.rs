// SPDX-License-Identifier: AGPL-3.0-or-later

//! A small ZIP reader: the central directory, then stored or deflated
//! entries. Used for an EPUB's package document and for ZIP comics with WebP
//! pages (MuPDF's comic reader skips those). Everything is bounded: the file
//! is hostile (AGENTS.md rule 8). flate2 is already in kitab.exe through GPUI.
//!
//! Not handled, so the caller falls back to MuPDF: ZIP64 archives, encrypted
//! entries, and compression other than store and deflate.

use std::io::{Read, Seek, SeekFrom};

const MAX_CENTRAL_DIR: u64 = 16 << 20;
const MAX_ENTRIES: usize = 100_000;

pub(crate) struct Entry {
    pub name: String,
    method: u16,
    flags: u16,
    packed: u64,
    pub size: u64,
    offset: u64,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

pub(crate) struct Zip<R> {
    file: R,
    pub entries: Vec<Entry>,
}

impl Zip<std::fs::File> {
    pub(crate) fn open_path(path: &std::path::Path) -> Option<Self> {
        Zip::new(std::fs::File::open(path).ok()?)
    }
}

impl<R: Read + Seek> Zip<R> {
    /// The archive's table of contents. `None` when it isn't a ZIP this
    /// reader understands.
    pub(crate) fn new(mut file: R) -> Option<Self> {
        let len = file.seek(SeekFrom::End(0)).ok()?;
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
                name: String::from_utf8_lossy(name).replace('\\', "/"),
                flags: u16_at(&cd, at + 8)?,
                method: u16_at(&cd, at + 10)?,
                packed: u64::from(u32_at(&cd, at + 20)?),
                size: u64::from(u32_at(&cd, at + 24)?),
                offset: u64::from(u32_at(&cd, at + 42)?),
            });
            at += 46 + name_len + extra + comment;
        }
        Some(Zip { file, entries })
    }

    /// The entry named `name`'s bytes, if at most `max` bytes packed and
    /// unpacked, and stored or deflated.
    pub(crate) fn read(&mut self, name: &str, max: u64) -> Option<Vec<u8>> {
        let ix = self.entries.iter().position(|e| e.name == name)?;
        self.read_at(ix, max)
    }

    /// Entry `ix`'s bytes, as [`Zip::read`].
    pub(crate) fn read_at(&mut self, ix: usize, max: u64) -> Option<Vec<u8>> {
        let e = self.entries.get(ix)?;
        // Bit 0: encrypted. 0xFFFF_FFFF sizes: the real ones are in ZIP64 extras.
        if e.flags & 1 != 0 || e.packed > max || e.size > max {
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
                    .take(max + 1)
                    .read_to_end(&mut out)
                    .ok()?;
                (out.len() as u64 <= max).then_some(out)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A ZIP of stored entries, for tests.
    pub(crate) fn stored(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cd = Vec::new();
        for (name, data) in files {
            let offset = out.len() as u32;
            let crc = 0u32; // not checked by this reader
            let local = |sig: u32, b: &mut Vec<u8>| {
                b.extend_from_slice(&sig.to_le_bytes());
            };
            local(0x0403_4b50, &mut out);
            out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            local(0x0201_4b50, &mut cd);
            cd.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            cd.extend_from_slice(&crc.to_le_bytes());
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&[0; 12]);
            cd.extend_from_slice(&offset.to_le_bytes());
            cd.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&cd);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(cd.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn reads_stored_entries_and_refuses_big_ones() {
        let bytes = stored(&[("a.txt", b"hello"), ("dir\\b.txt", b"world!")]);
        let mut z = Zip::new(std::io::Cursor::new(bytes.as_slice())).unwrap();
        let names: Vec<_> = z.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a.txt", "dir/b.txt"]);
        assert_eq!(z.read("a.txt", 100).as_deref(), Some(&b"hello"[..]));
        assert_eq!(z.read("dir/b.txt", 100).as_deref(), Some(&b"world!"[..]));
        assert!(z.read("a.txt", 4).is_none(), "over the limit");
        assert!(z.read("missing", 100).is_none());
        assert!(Zip::new(std::io::Cursor::new(&b"not a zip"[..])).is_none());
    }
}
