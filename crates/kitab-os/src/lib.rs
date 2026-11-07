// SPDX-License-Identifier: AGPL-3.0-or-later

//! Platform code for Kitab Reader (PLAN.md §3.9). The only crate besides
//! kitab-mupdf, kitab-print and kitab-sign allowed `unsafe` (AGENTS.md rule 10).

pub mod assoc;
pub mod instance;
#[cfg(windows)]
pub mod splash;
pub mod update;

#[cfg(all(windows, feature = "input"))]
pub mod input;

use std::path::PathBuf;

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
