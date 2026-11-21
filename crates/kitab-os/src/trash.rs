// SPDX-License-Identifier: AGPL-3.0-or-later

//! Move a file to the system's Recycle Bin or Trash (GitHub #17), where it
//! can be put back from. Never deletes for good without the system asking.

use std::io;
use std::path::Path;

/// What the system calls it, for menus and messages.
pub const TRASH_NAME: &str = if cfg!(windows) {
    "Recycle Bin"
} else {
    "Trash"
};

/// Move `path` to the Recycle Bin (Windows) or Trash (macOS, Linux). Blocks
/// while the system works: call it off the UI thread. A file still held
/// open for a moment (a tab that just closed) is tried again for up to 3 s.
pub fn move_to_trash(path: &Path) -> io::Result<()> {
    let path = std::path::absolute(path)?;
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the file isn't there",
        ));
    }
    let start = std::time::Instant::now();
    loop {
        match trash_once(&path) {
            Ok(()) => return Ok(()),
            // Declined (Windows asked to delete it for good): don't ask again.
            Err(e) if e.kind() == io::ErrorKind::Interrupted => return Err(e),
            Err(e) => {
                if start.elapsed() > std::time::Duration::from_secs(3) {
                    return Err(e);
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
    }
}

#[cfg(windows)]
fn trash_once(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
        FOF_WANTNUKEWARNING, SHFILEOPSTRUCTW, SHFileOperationW,
    };
    use windows::core::PCWSTR;
    // pFrom is a list of names, each ended by a NUL, the list by another.
    let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
    from.extend([0, 0]);
    // FOF_ALLOWUNDO sends it to the Recycle Bin. Where there isn't one (a
    // network share, a file too big for it) FOF_WANTNUKEWARNING makes
    // Windows ask before deleting it for good, despite FOF_NOCONFIRMATION.
    let flags =
        FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT | FOF_WANTNUKEWARNING;
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: flags.0 as u16,
        ..Default::default()
    };
    // SAFETY: `op` is fully set up; `from` is double-NUL ended and outlives
    // the synchronous call; no name mappings are asked for.
    let r = unsafe { SHFileOperationW(&mut op) };
    if op.fAnyOperationsAborted.as_bool() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "it wasn't deleted",
        ));
    }
    if r != 0 {
        return Err(io::Error::other(format!("Windows answered {r:#x}")));
    }
    if path.exists() {
        return Err(io::Error::other("the file is still there"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn trash_once(path: &Path) -> io::Result<()> {
    // Finder's delete moves to the Trash and keeps "Put Back". The path goes
    // in as an argument, never into the script's text.
    let out = std::process::Command::new("osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "tell application \"Finder\" to delete (POSIX file (item 1 of argv) as alias)",
            "-e",
            "end run",
        ])
        .arg(path)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn trash_once(path: &Path) -> io::Result<()> {
    // GLib's gio knows every trash the desktop does (removable drives too).
    if let Ok(out) = std::process::Command::new("gio")
        .arg("trash")
        .arg(path)
        .output()
        && out.status.success()
    {
        return Ok(());
    }
    home_trash(path)
}

/// The freedesktop.org home trash, for files on the same drive as it.
#[cfg(all(unix, not(target_os = "macos")))]
fn home_trash(path: &Path) -> io::Result<()> {
    use std::io::Write;
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home folder"))?
        .join("Trash");
    let (files, info) = (base.join("files"), base.join("info"));
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&info)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?
        .to_string_lossy()
        .into_owned();
    // A name not yet in the trash: "a.pdf", "a.2.pdf", ...
    for n in 1..1000 {
        let pick = if n == 1 {
            name.clone()
        } else {
            match name.rsplit_once('.') {
                Some((stem, ext)) if !stem.is_empty() => format!("{stem}.{n}.{ext}"),
                _ => format!("{name}.{n}"),
            }
        };
        let info_path = info.join(format!("{pick}.trashinfo"));
        let mut f = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&info_path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        writeln!(
            f,
            "[Trash Info]\nPath={}\nDeletionDate={}",
            percent_path(path),
            now_iso()
        )?;
        return match std::fs::rename(path, files.join(&pick)) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&info_path);
                Err(e)
            }
        };
    }
    Err(io::Error::other(
        "the trash has too many files of that name",
    ))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn percent_path(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut s = String::new();
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

#[cfg(all(unix, not(target_os = "macos")))]
fn now_iso() -> String {
    // SAFETY: time(NULL) reads the clock; localtime_r writes only into the
    // zeroed struct passed to it.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return String::new();
        }
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_error() {
        let p = std::env::temp_dir().join("kitab-no-such-file-to-trash.pdf");
        assert_eq!(
            move_to_trash(&p).map_err(|e| e.kind()),
            Err(io::ErrorKind::NotFound)
        );
    }

    /// Puts a real file in the Recycle Bin / Trash.
    #[test]
    #[ignore = "uses the system's trash"]
    fn a_file_goes_to_the_trash() {
        let p = std::env::temp_dir().join(format!("kitab-trash-{}.txt", std::process::id()));
        std::fs::write(&p, b"x").unwrap();
        move_to_trash(&p).unwrap();
        assert!(!p.exists());
    }
}
