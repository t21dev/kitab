// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kitab FILE...` or plain `kitab` in a terminal: start Kitab Reader. On
//! Windows this program is `{app}\bin\kitab.exe` and the app is
//! `{app}\kitab.exe`, one folder up (the app is a windowed program that
//! can't print to a terminal, so the commands live here).

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The app beside the folder this program is in.
fn app_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "kitab.exe" } else { "kitab" };
    let app = exe.parent()?.parent()?.join(name);
    app.is_file().then_some(app)
}

/// Start the app with `args`, detached, and return the exit code.
pub fn open_app(args: &[OsString]) -> i32 {
    let Some(app) = app_path() else {
        eprintln!(
            "kitab: Kitab Reader wasn't found next to this command (it looks one folder up for \
             the app).\nOpen Kitab from the Start menu or your applications, or reinstall it. \
             For the commands, run: kitab help"
        );
        return crate::out::EXIT_FAILED;
    };
    let mut cmd = Command::new(&app);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Its own process group, without this console: closing the terminal
        // doesn't close the app.
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    match cmd.spawn() {
        Ok(_) => crate::out::EXIT_OK,
        Err(e) => {
            eprintln!("kitab: could not start {}: {e}", app.display());
            crate::out::EXIT_FAILED
        }
    }
}
