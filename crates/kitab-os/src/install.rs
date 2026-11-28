// SPDX-License-Identifier: AGPL-3.0-or-later

//! Install an update from inside Kitab (the update dialog's Install). Only
//! when the user presses it: download the release's installer for this
//! computer, check it against the release's `SHA256SUMS.txt`, and keep it
//! ready. It goes in when Kitab quits (Restart now quits and opens the new
//! version), so nothing is replaced while Kitab is running.
//!
//! - Windows, installed with the setup program: the new setup runs silently
//!   over the old version, after Windows' administrator prompt (Kitab is in
//!   Program Files; 1.6.3 and older were per user, and that setup moves
//!   them there).
//! - macOS: the new `Kitab.app` from the release's disk image replaces the
//!   running one where it is (usually /Applications).
//!
//! Elsewhere (the portable zip, Scoop, Linux packages) [`supported`] is
//! `None` and Download opens the release page as before. The Microsoft Store
//! and Flatpak update Kitab themselves (`update::managed_by`).

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where a release's files are downloaded from.
const DOWNLOADS: &str = "https://github.com/t21dev/kitab/releases/download";
/// Larger than any Kitab installer will be; curl stops past it.
const MAX_DOWNLOAD: &str = "524288000";

/// How this copy of Kitab can install an update, when it can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// Windows: run the release's setup program silently.
    WindowsSetup { exe: PathBuf },
    /// macOS: replace this `.app` bundle with the release's.
    MacBundle { app: PathBuf },
}

/// An update downloaded, checked and ready to go in when Kitab quits.
#[derive(Debug, Clone)]
pub struct Staged {
    pub version: String,
    method: Method,
    /// The setup program (Windows) or the unpacked `Kitab.app` (macOS).
    payload: PathBuf,
}

/// The staged update, and whether to open Kitab again after installing it.
static STAGED: Mutex<Option<(Staged, bool)>> = Mutex::new(None);

/// How this copy of Kitab can update itself, or `None` when it can't (then
/// Download opens the release page).
/// Worked out once: the dialog asks on every frame, and the Mac's answer
/// writes a file to find out.
pub fn supported() -> Option<Method> {
    static METHOD: std::sync::OnceLock<Option<Method>> = std::sync::OnceLock::new();
    METHOD
        .get_or_init(|| {
            if crate::update::managed_by().is_some() {
                return None;
            }
            method_for(&std::env::current_exe().ok()?)
        })
        .clone()
}

fn method_for(exe: &Path) -> Option<Method> {
    if cfg!(windows) {
        let dir = exe.parent()?;
        let text = dir.to_string_lossy().to_lowercase();
        // Scoop has its own `scoop update`; the portable zip has no uninstaller.
        if text.contains("\\scoop\\")
            || crate::update::scoop()
            || !dir.join("unins000.exe").is_file()
        {
            return None;
        }
        Some(Method::WindowsSetup {
            exe: exe.to_path_buf(),
        })
    } else if cfg!(target_os = "macos") {
        // …/Kitab.app/Contents/MacOS/kitab
        let app = exe.parent()?.parent()?.parent()?;
        if app.extension().is_none_or(|e| e != "app") {
            return None;
        }
        let parent = app.parent()?;
        // The bundle is swapped inside its folder: that needs write access
        // (an admin account has it for /Applications; a standard one may not).
        let probe = parent.join(".kitab-update-probe");
        std::fs::write(&probe, b"").ok()?;
        let _ = std::fs::remove_file(&probe);
        Some(Method::MacBundle {
            app: app.to_path_buf(),
        })
    } else {
        None
    }
}

/// The release file for this computer: the setup program or the disk image.
pub fn asset_name(version: &str, method: &Method) -> String {
    match method {
        Method::WindowsSetup { .. } => format!("Kitab_{version}_x64_setup.exe"),
        Method::MacBundle { .. } => {
            let arch = if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else {
                "x86_64"
            };
            format!("Kitab_{version}_{arch}.dmg")
        }
    }
}

/// The checksum for `name` in a `SHA256SUMS.txt` (`<hex>  <name>` per line,
/// `*<name>` in binary mode).
pub fn checksum_for<'a>(sums: &'a str, name: &str) -> Option<&'a str> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then_some(hash)
    })
}

/// Download, check and unpack `version`. Blocks for as long as the download
/// takes: call it off the UI thread. On success the update is staged and
/// goes in when Kitab quits.
pub fn download(version: &str, method: &Method) -> Result<Staged, String> {
    let dir = std::env::temp_dir().join("kitab-update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|_| "Couldn't make a folder for the download")?;
    let name = asset_name(version, method);
    let base = format!("{DOWNLOADS}/v{version}");

    let sums_path = dir.join("SHA256SUMS.txt");
    fetch(&format!("{base}/SHA256SUMS.txt"), &sums_path)?;
    let sums = std::fs::read_to_string(&sums_path)
        .map_err(|_| "The release's checksums couldn't be read".to_string())?;
    let want = checksum_for(&sums, &name)
        .ok_or_else(|| format!("The release has no checksum for {name}"))?
        .to_ascii_lowercase();

    let file = dir.join(&name);
    fetch(&format!("{base}/{name}"), &file)?;
    if sha256_file(&file)? != want {
        let _ = std::fs::remove_file(&file);
        return Err("The download didn't match its checksum, so it wasn't installed".into());
    }

    let payload = match method {
        Method::WindowsSetup { .. } => file,
        Method::MacBundle { .. } => unpack_dmg(&file, &dir)?,
    };
    let staged = Staged {
        version: version.to_string(),
        method: method.clone(),
        payload,
    };
    if let Ok(mut s) = STAGED.lock() {
        *s = Some((staged.clone(), false));
    }
    Ok(staged)
}

/// Open the new version once it's installed (Restart now), rather than only
/// installing it when Kitab quits.
pub fn relaunch_after_install() {
    if let Ok(mut s) = STAGED.lock()
        && let Some((_, relaunch)) = s.as_mut()
    {
        *relaunch = true;
    }
}

/// Kitab is quitting: start installing the staged update, if there is one.
/// A helper waits for this process to end, installs, and opens Kitab again
/// when Restart now asked for it.
pub fn install_staged() -> std::io::Result<()> {
    let Some((staged, relaunch)) = STAGED.lock().ok().and_then(|mut s| s.take()) else {
        return Ok(());
    };
    spawn_installer(&staged, relaunch)
}

fn fetch(url: &str, to: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new(crate::update::curl());
    cmd.args([
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--proto",
        "=https",
        "--max-time",
        "900",
        "--max-filesize",
        MAX_DOWNLOAD,
        "-H",
        "User-Agent: Kitab-Reader",
        "--output",
    ])
    .arg(to)
    .arg(url);
    no_window(&mut cmd);
    let out = cmd
        .output()
        .map_err(|_| "Couldn't download (curl isn't available)".to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err("The download failed. Check the connection and try again.".into())
    }
}

fn sha256_file(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut f =
        std::fs::File::open(path).map_err(|_| "The download couldn't be read".to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|_| "The download couldn't be read".to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// macOS: copy `Kitab.app` out of the disk image into `dir`.
fn unpack_dmg(dmg: &Path, dir: &Path) -> Result<PathBuf, String> {
    let mount = dir.join("mnt");
    let _ = std::fs::create_dir_all(&mount);
    let ok = std::process::Command::new("/usr/bin/hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-quiet"])
        .arg("-mountpoint")
        .arg(&mount)
        .arg(dmg)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return Err("The disk image couldn't be opened".into());
    }
    let app = dir.join("Kitab.app");
    let copied = std::process::Command::new("/usr/bin/ditto")
        .arg(mount.join("Kitab.app"))
        .arg(&app)
        .status()
        .is_ok_and(|s| s.success());
    let _ = std::process::Command::new("/usr/bin/hdiutil")
        .args(["detach", "-quiet"])
        .arg(&mount)
        .status();
    let _ = std::fs::remove_file(dmg);
    if copied && app.join("Contents").is_dir() {
        Ok(app)
    } else {
        Err("The new version couldn't be copied out of the disk image".into())
    }
}

/// Windows helper: wait for Kitab to close, run the setup silently, open
/// Kitab again if asked. Paths come in as environment variables, so nothing
/// needs quoting.
const WINDOWS_HELPER: &str = "$p = Get-Process -Id $env:KITAB_PID -ErrorAction SilentlyContinue; \
if ($p) { $p.WaitForExit(60000) | Out-Null }; \
Start-Process -FilePath $env:KITAB_SETUP -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/NOCANCEL' -Wait; \
Remove-Item -LiteralPath $env:KITAB_SETUP -ErrorAction SilentlyContinue; \
if ($env:KITAB_RELAUNCH -eq '1') { Start-Process -FilePath $env:KITAB_EXE }";

/// macOS helper: wait for Kitab to close, put the new bundle in the old one's
/// place (the old one is moved aside first and put back if that fails), then
/// open it if asked. Arguments: pid, new bundle, installed bundle, relaunch.
const MAC_HELPER: &str = r#"pid=$1; new=$2; app=$3; relaunch=$4
while kill -0 "$pid" 2>/dev/null; do sleep 0.2; done
dir=$(dirname "$app"); next="$dir/.Kitab-update.app"; old="$dir/.Kitab-old.app"
rm -rf "$next" "$old"
/usr/bin/ditto "$new" "$next" || exit 1
/usr/bin/xattr -cr "$next" 2>/dev/null
if mv "$app" "$old"; then
  if mv "$next" "$app"; then rm -rf "$old"; else mv "$old" "$app"; rm -rf "$next"; exit 1; fi
else
  rm -rf "$next"; exit 1
fi
rm -rf "$new"
[ "$relaunch" = 1 ] && /usr/bin/open "$app"
exit 0"#;

fn spawn_installer(staged: &Staged, relaunch: bool) -> std::io::Result<()> {
    let pid = std::process::id().to_string();
    let relaunch = if relaunch { "1" } else { "0" };
    match &staged.method {
        Method::WindowsSetup { exe } => {
            let ps = std::env::var_os("SystemRoot")
                .map(|r| {
                    PathBuf::from(r)
                        .join("System32")
                        .join("WindowsPowerShell")
                        .join("v1.0")
                        .join("powershell.exe")
                })
                .unwrap_or_else(|| "powershell.exe".into());
            let mut cmd = std::process::Command::new(ps);
            cmd.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-Command",
                WINDOWS_HELPER,
            ])
            .env("KITAB_PID", &pid)
            .env("KITAB_SETUP", &staged.payload)
            .env("KITAB_EXE", exe)
            .env("KITAB_RELAUNCH", relaunch);
            no_window(&mut cmd);
            cmd.spawn().map(|_| ())
        }
        Method::MacBundle { app } => std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(MAC_HELPER)
            .arg("kitab-update")
            .arg(&pid)
            .arg(&staged.payload)
            .arg(app)
            .arg(relaunch)
            .spawn()
            .map(|_| ()),
    }
}

#[cfg_attr(not(windows), allow(unused_variables))]
fn no_window(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn checksums_are_found_by_file_name() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!(
            "{a}  Kitab_0.7.0_x64_setup.exe\n{b} *Kitab_0.7.0_aarch64.dmg\nshort  Kitab_0.7.0_x86_64.dmg\n"
        );
        assert_eq!(
            checksum_for(&sums, "Kitab_0.7.0_x64_setup.exe"),
            Some(a.as_str())
        );
        assert_eq!(
            checksum_for(&sums, "Kitab_0.7.0_aarch64.dmg"),
            Some(b.as_str())
        );
        // Not a SHA-256: refused.
        assert_eq!(checksum_for(&sums, "Kitab_0.7.0_x86_64.dmg"), None);
        assert_eq!(checksum_for(&sums, "Kitab_0.7.0_x64_portable.zip"), None);
    }

    #[test]
    fn asset_names_match_the_release() {
        let win = Method::WindowsSetup {
            exe: "kitab.exe".into(),
        };
        assert_eq!(asset_name("0.7.0", &win), "Kitab_0.7.0_x64_setup.exe");
        let mac = Method::MacBundle {
            app: "/Applications/Kitab.app".into(),
        };
        assert!(asset_name("0.7.0", &mac).ends_with(".dmg"));
    }

    #[test]
    fn sha256_of_a_file() {
        let dir = std::env::temp_dir().join(format!("kitab-sha-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x");
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Downloads a real release from GitHub (about 10 MB) and checks it.
    /// Run by hand: `cargo test -p kitab-os -- --ignored real_release`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn a_real_release_downloads_and_matches_its_checksum() {
        let method = Method::WindowsSetup {
            exe: "kitab.exe".into(),
        };
        let staged = download("0.6.2", &method).unwrap();
        assert!(staged.payload.is_file());
        // Not installed: forget it so nothing runs.
        STAGED.lock().unwrap().take();
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("kitab-update"));
    }

    #[cfg(windows)]
    #[test]
    fn a_portable_copy_cannot_update_itself() {
        let dir = std::env::temp_dir().join(format!("kitab-portable-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("kitab.exe");
        assert_eq!(method_for(&exe), None);
        std::fs::write(dir.join("unins000.exe"), b"").unwrap();
        assert_eq!(
            method_for(&exe),
            Some(Method::WindowsSetup { exe: exe.clone() })
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
