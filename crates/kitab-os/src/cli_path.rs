// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `kitab` command in terminals (crate `kitab-cli`), for Settings ›
//! Advanced › Command line.
//!
//! - Windows: the console `bin\kitab.exe` beside the windowed kitab.exe. Its
//!   folder goes on (or comes off) the user's PATH, the same entry the
//!   installer's task writes (packaging/windows/kitab.iss). The Store
//!   package gets `kitab` from its app execution alias instead.
//! - macOS: the app's own binary runs the commands; a symlink in
//!   `~/.local/bin` puts it on PATH.
//! - Linux: the packages install `/usr/bin/kitab`, which runs the commands
//!   itself; nothing to switch.

use std::path::{Path, PathBuf};

/// The program a terminal runs as `kitab`, if this install has one.
pub fn cli_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    #[cfg(windows)]
    {
        let cli = exe.parent()?.join("bin").join("kitab.exe");
        cli.is_file().then_some(cli)
    }
    #[cfg(not(windows))]
    {
        Some(exe)
    }
}

/// Whether a terminal finds `kitab` through Kitab's own entry: the CLI's
/// folder on the user PATH (Windows), the `~/.local/bin` link (macOS),
/// `/usr/bin/kitab` (Linux).
pub fn cli_on_path() -> bool {
    imp::on_path()
}

/// Put the `kitab` command on (or take it off) the user's PATH. Windows and
/// macOS only; Linux packages install it themselves.
pub fn set_cli_on_path(on: bool) -> Result<(), String> {
    imp::set(on)
}

// ── PATH strings (pure, tested on every system) ──

/// One PATH entry as compared: surrounding quotes and spaces, and trailing
/// backslashes or slashes, ignored; case ignored (Windows paths).
#[cfg_attr(not(windows), allow(dead_code))] // used on Windows (and in tests)
fn same_entry(entry: &str, dir: &str) -> bool {
    fn norm(s: &str) -> String {
        s.trim()
            .trim_matches('"')
            .trim()
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    }
    let e = norm(entry);
    !e.is_empty() && e == norm(dir)
}

/// Whether the `;`-separated `path` holds `dir`.
#[cfg_attr(not(windows), allow(dead_code))] // used on Windows (and in tests)
pub(crate) fn path_has(path: &str, dir: &str) -> bool {
    path.split(';').any(|e| same_entry(e, dir))
}

/// `path` with `dir` added at the end, or `None` if it is already there.
/// Every other entry is kept as it is (`%VAR%` entries included).
#[cfg_attr(not(windows), allow(dead_code))] // used on Windows (and in tests)
pub(crate) fn path_with(path: &str, dir: &str) -> Option<String> {
    if path_has(path, dir) {
        return None;
    }
    let rest = path.trim_end_matches(';');
    Some(if rest.trim().is_empty() {
        dir.to_string()
    } else {
        format!("{rest};{dir}")
    })
}

/// `path` without any entry that is `dir`, or `None` if there was none.
/// Every other entry is kept as it is, in order.
#[cfg_attr(not(windows), allow(dead_code))] // used on Windows (and in tests)
pub(crate) fn path_without(path: &str, dir: &str) -> Option<String> {
    if !path_has(path, dir) {
        return None;
    }
    let kept: Vec<&str> = path.split(';').filter(|e| !same_entry(e, dir)).collect();
    Some(kept.join(";").trim_matches(';').to_string())
}

/// The folder that goes on PATH: the CLI's own.
#[cfg_attr(not(windows), allow(dead_code))] // used on Windows only
fn cli_dir(exe: &Path) -> Option<PathBuf> {
    exe.parent().map(Path::to_path_buf)
}

#[cfg(windows)]
mod imp {
    use super::{cli_dir, cli_exe, path_has, path_with, path_without};
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, LPARAM, WPARAM};
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
        RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegCloseKey, RegDeleteValueW,
        RegGetValueW, RegOpenKeyExW, RegSetValueExW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };
    use windows::core::{HSTRING, PCWSTR, w};

    const ENV: &str = "Environment";

    /// The user's PATH (HKCU\Environment `Path`) as stored, unexpanded, with
    /// its type. A missing value is an empty REG_EXPAND_SZ; any other read
    /// failure is an error, so nothing is written over a PATH that couldn't
    /// be read.
    fn read() -> Result<(String, REG_VALUE_TYPE), String> {
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        let mut len = 0u32;
        let mut kind = REG_VALUE_TYPE::default();
        // Asks the size first; a few tries in case PATH grows in between.
        for _ in 0..4 {
            // SAFETY: with no buffer, RegGetValueW only writes the value's
            // byte size into `len` and its type into `kind`.
            let r = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    &HSTRING::from(ENV),
                    w!("Path"),
                    flags,
                    Some(&mut kind),
                    None,
                    Some(&mut len),
                )
            };
            if r == ERROR_FILE_NOT_FOUND {
                return Ok((String::new(), REG_EXPAND_SZ));
            }
            r.ok().map_err(|e| format!("PATH: {e}"))?;
            let mut buf = vec![0u16; (len as usize).div_ceil(2) + 1];
            let mut got = (buf.len() * 2) as u32;
            // SAFETY: `buf` holds `got` bytes, which RegGetValueW never
            // writes past; `kind` outlives the call.
            let r = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    &HSTRING::from(ENV),
                    w!("Path"),
                    flags,
                    Some(&mut kind),
                    Some(buf.as_mut_ptr().cast()),
                    Some(&mut got),
                )
            };
            if r == ERROR_MORE_DATA {
                continue;
            }
            if r == ERROR_FILE_NOT_FOUND {
                return Ok((String::new(), REG_EXPAND_SZ));
            }
            r.ok().map_err(|e| format!("PATH: {e}"))?;
            let chars = (got as usize / 2).min(buf.len());
            let text = String::from_utf16_lossy(&buf[..chars]);
            return Ok((text.trim_end_matches('\0').to_string(), kind));
        }
        Err("PATH kept changing while it was read".into())
    }

    /// Write the user's PATH back with the type it had (an empty one is
    /// removed rather than left blank).
    fn write(value: &str, kind: REG_VALUE_TYPE) -> Result<(), String> {
        let kind = if kind == REG_SZ {
            REG_SZ
        } else {
            REG_EXPAND_SZ
        };
        let mut key = HKEY::default();
        // SAFETY: opens HKCU\Environment, which every account has; the
        // handle is closed below and the data buffer outlives the call.
        unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(ENV),
                None,
                KEY_READ | KEY_WRITE,
                &mut key,
            )
            .ok()
            .map_err(|e| format!("Environment: {e}"))?;
            let result = if value.is_empty() {
                let r = RegDeleteValueW(key, w!("Path"));
                if r == ERROR_FILE_NOT_FOUND {
                    Ok(())
                } else {
                    r.ok()
                }
            } else {
                let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
                let bytes = std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2);
                RegSetValueExW(key, w!("Path"), None, kind, Some(bytes)).ok()
            };
            let _ = RegCloseKey(key);
            result.map_err(|e| format!("PATH: {e}"))
        }
    }

    /// Tell running programs (Explorer, so the terminals it starts) that the
    /// environment changed. A hung window is skipped after a second.
    fn broadcast() {
        let mut out = 0usize;
        // SAFETY: the string is a static wide literal that outlives the
        // call; SendMessageTimeoutW only writes the result into `out`.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(PCWSTR::as_ptr(&w!("Environment")) as isize),
                SMTO_ABORTIFHUNG,
                1000,
                Some(&mut out),
            );
        }
    }

    fn dir() -> Option<String> {
        cli_exe()
            .and_then(|exe| cli_dir(&exe))
            .map(|d| d.display().to_string())
    }

    pub fn on_path() -> bool {
        let Some(dir) = dir() else { return false };
        read().is_ok_and(|(path, _)| path_has(&path, &dir))
    }

    pub fn set(on: bool) -> Result<(), String> {
        let dir = dir().ok_or("bin\\kitab.exe isn't next to Kitab")?;
        let (path, kind) = read()?;
        let changed = if on {
            path_with(&path, &dir)
        } else {
            path_without(&path, &dir)
        };
        if let Some(new) = changed {
            write(&new, kind)?;
            broadcast();
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::cli_exe;
    use std::path::PathBuf;

    /// `~/.local/bin/kitab`.
    fn link() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/bin/kitab"))
    }

    pub fn on_path() -> bool {
        // Ours if it is a link to this app's binary.
        match (link(), cli_exe()) {
            (Some(l), Some(exe)) => std::fs::read_link(&l).is_ok_and(|t| t == exe),
            _ => false,
        }
    }

    pub fn set(on: bool) -> Result<(), String> {
        let link = link().ok_or("No home folder")?;
        if on {
            let exe = cli_exe().ok_or("Kitab's binary wasn't found")?;
            if let Some(dir) = link.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            // Replace an old link (a moved app), never a real file.
            if std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()) {
                std::fs::remove_file(&link).map_err(|e| format!("{}: {e}", link.display()))?;
            }
            std::os::unix::fs::symlink(&exe, &link).map_err(|e| format!("{}: {e}", link.display()))
        } else {
            if std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()) {
                std::fs::remove_file(&link).map_err(|e| format!("{}: {e}", link.display()))?;
            }
            Ok(())
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    pub fn on_path() -> bool {
        std::path::Path::new("/usr/bin/kitab").exists()
    }

    pub fn set(_on: bool) -> Result<(), String> {
        Err("The Kitab package installs the kitab command in /usr/bin".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = r"C:\Users\a\AppData\Local\Programs\Kitab\bin";

    #[test]
    fn adds_once_at_the_end() {
        assert_eq!(
            path_with(r"C:\one;%USERPROFILE%\two", DIR).as_deref(),
            Some(r"C:\one;%USERPROFILE%\two;C:\Users\a\AppData\Local\Programs\Kitab\bin")
        );
        // Already there, in another case or with a trailing backslash.
        assert_eq!(path_with(&format!(r"C:\one;{DIR}"), DIR), None);
        assert_eq!(
            path_with(&format!(r"C:\one;{}\", DIR.to_uppercase()), DIR),
            None
        );
        assert_eq!(path_with(&format!("\"{DIR}\";C:\\one"), DIR), None);
    }

    #[test]
    fn adds_to_empty_or_trailing_separator() {
        assert_eq!(path_with("", DIR).as_deref(), Some(DIR));
        assert_eq!(path_with("  ", DIR).as_deref(), Some(DIR));
        assert_eq!(path_with(";", DIR).as_deref(), Some(DIR));
        assert_eq!(path_with(r"C:\one;", DIR), Some(format!(r"C:\one;{DIR}")));
    }

    #[test]
    fn removes_only_that_entry() {
        let path = format!(
            r"%SystemRoot%\x;{}\;C:\one;;C:\Kitab\bin2",
            DIR.to_lowercase()
        );
        assert_eq!(
            path_without(&path, DIR).as_deref(),
            Some(r"%SystemRoot%\x;C:\one;;C:\Kitab\bin2")
        );
        assert_eq!(path_without(r"C:\one;C:\two", DIR), None);
        assert_eq!(path_without("", DIR), None);
        assert_eq!(path_without(DIR, DIR).as_deref(), Some(""));
        // Twice (an old install) both go.
        assert_eq!(
            path_without(&format!(r"{DIR};C:\one;{DIR}\"), DIR).as_deref(),
            Some(r"C:\one")
        );
    }

    #[test]
    fn a_prefix_or_empty_entry_is_not_a_match() {
        assert!(!path_has(r"C:\Users\a\AppData\Local\Programs\Kitab", DIR));
        assert!(!path_has(&format!(r"{DIR}x"), DIR));
        assert!(!path_has(";;", ""));
        assert!(path_has(&format!(r" {DIR} "), DIR));
    }
}
