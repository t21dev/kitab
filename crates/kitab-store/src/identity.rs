// SPDX-License-Identifier: AGPL-3.0-or-later

//! Stable file identity (PLAN.md §3.6): recognise a document after a move or
//! rename by its file id first, then by a quick content hash.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::UNIX_EPOCH;

use xxhash_rust::xxh3::Xxh3;

use crate::Result;

/// How many bytes the quick hash reads from each end of the file.
pub const QUICK_HASH_SPAN: u64 = 64 * 1024;

/// Who a file is, independent of its path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    /// Volume serial + file id (Windows) or dev + inode (Unix), as text.
    /// `None` when the file system does not report one (some network shares).
    /// Ids can be reused after a file is deleted, so a match on this alone is
    /// confirmed by the quick hash or by the old path being gone.
    pub file_key: Option<String>,
    /// XXH3-64 over the size, the first 64 KiB and the last 64 KiB.
    pub quick_hash: u64,
    /// File size in bytes.
    pub size: u64,
    /// Last modification time, Unix milliseconds, if the OS reports it.
    pub mtime_ms: Option<i64>,
}

/// Work out a file's identity. Reads at most 128 KiB, so it is cheap even for
/// huge files, but it is file I/O: call it on the io thread, never the UI thread.
pub fn identify(path: &Path) -> Result<FileIdentity> {
    let mut file = File::open(path)?;
    let meta = file.metadata()?;
    let size = meta.len();
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok());

    let quick_hash = quick_hash(&mut file, size)?;
    let file_key = file_id::get_file_id(path).ok().map(file_key_string);

    Ok(FileIdentity {
        file_key,
        quick_hash,
        size,
        mtime_ms,
    })
}

/// Hash `size`, the first [`QUICK_HASH_SPAN`] bytes and the last
/// [`QUICK_HASH_SPAN`] bytes (without reading any byte twice).
fn quick_hash(file: &mut File, size: u64) -> Result<u64> {
    let mut hasher = Xxh3::new();
    hasher.update(&size.to_le_bytes());

    let mut buf = vec![0u8; QUICK_HASH_SPAN as usize];
    let head = read_up_to(file, &mut buf)?;
    hasher.update(&buf[..head]);

    let head = head as u64;
    if size > head {
        let tail_start = size.saturating_sub(QUICK_HASH_SPAN).max(head);
        file.seek(SeekFrom::Start(tail_start))?;
        let tail = read_up_to(file, &mut buf)?;
        hasher.update(&buf[..tail]);
    }
    Ok(hasher.digest())
}

/// Fill `buf` as far as the file allows; returns the bytes read.
fn read_up_to(file: &mut File, buf: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(filled)
}

fn file_key_string(id: file_id::FileId) -> String {
    match id {
        file_id::FileId::Inode {
            device_id,
            inode_number,
        } => format!("u:{device_id:x}:{inode_number:x}"),
        file_id::FileId::LowRes {
            volume_serial_number,
            file_index,
        } => format!("w:{volume_serial_number:x}:{file_index:x}"),
        file_id::FileId::HighRes {
            volume_serial_number,
            file_id,
        } => format!("w:{volume_serial_number:x}:{file_id:x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut f = File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[test]
    fn empty_and_small_files_hash() {
        let dir = tempfile::tempdir().unwrap();
        let a = identify(&write(dir.path(), "a", b"")).unwrap();
        let b = identify(&write(dir.path(), "b", b"x")).unwrap();
        assert_eq!(a.size, 0);
        assert_ne!(a.quick_hash, b.quick_hash);
    }

    #[test]
    fn hash_depends_on_both_ends_not_the_middle() {
        let dir = tempfile::tempdir().unwrap();
        let mut big = vec![7u8; 300 * 1024];
        let base = identify(&write(dir.path(), "base", &big)).unwrap();

        big[150 * 1024] = 8; // middle: not hashed
        let middle = identify(&write(dir.path(), "middle", &big)).unwrap();
        assert_eq!(base.quick_hash, middle.quick_hash);

        let last = big.len() - 1;
        big[last] = 9; // tail: hashed
        let tail = identify(&write(dir.path(), "tail", &big)).unwrap();
        assert_ne!(base.quick_hash, tail.quick_hash);
    }

    #[test]
    fn same_content_same_hash_different_key() {
        let dir = tempfile::tempdir().unwrap();
        let a = identify(&write(dir.path(), "a", b"hello")).unwrap();
        let b = identify(&write(dir.path(), "b", b"hello")).unwrap();
        assert_eq!(a.quick_hash, b.quick_hash);
        assert!(a.file_key.is_some());
        assert_ne!(a.file_key, b.file_key);
    }

    #[test]
    fn missing_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(identify(&dir.path().join("nope.pdf")).is_err());
    }
}
