// SPDX-License-Identifier: AGPL-3.0-or-later

//! Platform code for Kitab Reader (PLAN.md §3.9). The only crate besides
//! kitab-mupdf, kitab-print and kitab-sign allowed `unsafe` (AGENTS.md rule 10).

pub mod assoc;
pub mod instance;
#[cfg(windows)]
pub mod splash;
pub mod trace;
pub mod update;
pub mod xdg;

#[cfg(all(windows, feature = "input"))]
pub mod input;

use std::path::PathBuf;

/// Today's date on this computer's clock and time zone, "YYYY-MM-DD"
/// (reading streaks count the user's own days).
pub fn local_today() -> String {
    #[cfg(windows)]
    {
        // SAFETY: GetLocalTime only fills the struct it is given.
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay)
    }
    #[cfg(unix)]
    {
        // SAFETY: time(NULL) reads the clock; localtime_r writes only into
        // the zeroed struct passed to it and is thread-safe.
        unsafe {
            let now = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&now, &mut tm).is_null() {
                return String::new();
            }
            format!(
                "{:04}-{:02}-{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday
            )
        }
    }
}

/// Kitab's data folder (PRIVACY.md): `%APPDATA%\t21\Kitab`,
/// `~/Library/Application Support/dev.t21.kitab`, `$XDG_DATA_HOME/kitab`.
/// A `portable` file next to the executable puts it in `data/` beside it.
pub fn data_dir() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("portable").is_file() {
                return Some(dir.join("data"));
            }
        }
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("t21").join("Kitab"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support/dev.t21.kitab"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .map(|d| d.join("kitab"))
    }
}

/// Show `path` selected in the system file manager. Spawns the file manager and
/// returns; call it off the UI thread.
pub fn reveal_in_folder(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // explorer parses its own command line: `/select,"C:\a b\c.pdf"`.
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = path.parent().unwrap_or(path);
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map(|_| ())
    }
}

/// Keep a window off the screen while it draws its first frame (`true`),
/// then let it appear (`false`), with no visible step in between. The window
/// is shown and renders as usual; DWM just doesn't composite it. Windows
/// only; elsewhere this does nothing.
pub fn cloak_window(hwnd: isize, cloaked: bool) {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};
        // A Win32 BOOL.
        let value: i32 = i32::from(cloaked);
        // SAFETY: `hwnd` is the app's own live top-level window; the attribute
        // value is a BOOL of the size passed.
        let _ = unsafe {
            DwmSetWindowAttribute(
                HWND(hwnd as *mut core::ffi::c_void),
                DWMWA_CLOAK,
                (&value as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            )
        };
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, cloaked);
    }
}

/// Whether DWM keeps a window off the screen (cloaked). Benchmarks count a
/// window as shown only once it isn't. Windows only; `false` elsewhere.
pub fn is_cloaked(hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
        let mut value: u32 = 0;
        // SAFETY: the attribute is a DWORD written into `value`, whose size is passed.
        let ok = unsafe {
            DwmGetWindowAttribute(
                HWND(hwnd as *mut core::ffi::c_void),
                DWMWA_CLOAKED,
                (&mut value as *mut u32).cast(),
                std::mem::size_of::<u32>() as u32,
            )
        };
        ok.is_ok() && value != 0
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}
