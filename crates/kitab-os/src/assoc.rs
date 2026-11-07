// SPDX-License-Identifier: AGPL-3.0-or-later

//! File types (PLAN.md §3.13 "File types"). Windows: ProgIDs `Kitab.PDF`,
//! `Kitab.Markdown`… under the current user's classes, each with its document
//! icon from kitab.exe, plus `Capabilities` and `RegisteredApplications` so
//! Kitab appears in Settings › Default apps. Windows 10+ never lets an app set
//! itself as the default silently; the user confirms there. Nothing is written
//! until the user asks (Settings › Reading › Default app). macOS and Linux
//! declare their types in packaging/ (Info.plist, .desktop + shared-mime-info).

/// A format Kitab registers: extensions, ProgID, description, icon resource id
/// in kitab.exe (packaging/windows/kitab.rc), MIME type.
pub struct FileType {
    pub extensions: &'static [&'static str],
    pub prog_id: &'static str,
    pub description: &'static str,
    pub icon_id: u32,
    pub mime: &'static str,
    /// Whether "Make default" claims the type. Not plain text: it would take
    /// over every text file on Linux. (Windows lists it; the user picks.)
    pub claim_default: bool,
}

/// The formats Kitab opens. Not .azw3 (Kindle KF8 isn't read yet) and no
/// image files (Kitab doesn't view them).
pub const FILE_TYPES: &[FileType] = &[
    FileType {
        extensions: &["pdf"],
        prog_id: "Kitab.PDF",
        description: "PDF document",
        icon_id: 101,
        mime: "application/pdf",
        claim_default: true,
    },
    FileType {
        extensions: &["epub"],
        prog_id: "Kitab.EPUB",
        description: "EPUB book",
        icon_id: 102,
        mime: "application/epub+zip",
        claim_default: true,
    },
    FileType {
        extensions: &["md", "markdown", "mdown", "mkd"],
        prog_id: "Kitab.Markdown",
        description: "Markdown document",
        icon_id: 103,
        mime: "text/markdown",
        claim_default: true,
    },
    FileType {
        extensions: &["mobi", "azw"],
        prog_id: "Kitab.MOBI",
        description: "MOBI book",
        icon_id: 104,
        mime: "application/x-mobipocket-ebook",
        claim_default: true,
    },
    FileType {
        extensions: &["fb2"],
        prog_id: "Kitab.FB2",
        description: "FictionBook",
        icon_id: 105,
        mime: "application/x-fictionbook+xml",
        claim_default: true,
    },
    FileType {
        extensions: &["cbz"],
        prog_id: "Kitab.CBZ",
        description: "Comic book archive",
        icon_id: 106,
        mime: "application/vnd.comicbook+zip",
        claim_default: true,
    },
    FileType {
        extensions: &["xps", "oxps"],
        prog_id: "Kitab.XPS",
        description: "XPS document",
        icon_id: 107,
        mime: "application/oxps",
        claim_default: true,
    },
    FileType {
        extensions: &["txt"],
        prog_id: "Kitab.Text",
        description: "Text file",
        icon_id: 108,
        mime: "text/plain",
        claim_default: false,
    },
];

#[cfg(windows)]
mod imp {
    use super::FILE_TYPES;
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegSetValueExW,
    };
    use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
    use windows::core::HSTRING;

    const CAPABILITIES: &str = r"Software\t21\Kitab\Capabilities";
    /// `kitab_shell.dll`'s class (crate `kitab-shell`).
    const THUMB_CLSID: &str = "{7080945F-C214-4A8A-B62B-FC5E2E3D047E}";
    /// The shell's IThumbnailProvider handler slot.
    const THUMB_SLOT: &str = "{E357FCCD-A995-4576-B01F-234630154E96}";

    fn set(path: &str, name: Option<&str>, value: &str) -> Result<(), String> {
        let mut key = HKEY::default();
        // SAFETY: plain registry calls under HKEY_CURRENT_USER; the key handle
        // is closed below and the value buffer outlives the call.
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(path),
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
            .ok()
            .map_err(|e| format!("{path}: {e}"))?;
            let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            let name = name.map(HSTRING::from);
            let result = RegSetValueExW(
                key,
                name.as_ref().map_or(windows::core::PCWSTR::null(), |n| {
                    windows::core::PCWSTR(n.as_ptr())
                }),
                None,
                REG_SZ,
                Some(bytes),
            );
            let _ = RegCloseKey(key);
            result.ok().map_err(|e| format!("{path}: {e}"))
        }
    }

    /// Register Kitab for its file types for the current user.
    pub fn register() -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.to_string_lossy().to_string();
        let open = format!("\"{exe}\" \"%1\"");
        for t in FILE_TYPES {
            let classes = format!(r"Software\Classes\{}", t.prog_id);
            set(&classes, None, t.description)?;
            set(
                &format!(r"{classes}\DefaultIcon"),
                None,
                &format!("\"{exe}\",-{}", t.icon_id),
            )?;
            set(&format!(r"{classes}\shell\open\command"), None, &open)?;
            for ext in t.extensions {
                set(
                    &format!(r"Software\Classes\.{ext}\OpenWithProgids"),
                    Some(t.prog_id),
                    "",
                )?;
                set(
                    &format!(r"{CAPABILITIES}\FileAssociations"),
                    Some(&format!(".{ext}")),
                    t.prog_id,
                )?;
            }
        }
        set(CAPABILITIES, Some("ApplicationName"), "Kitab Reader")?;
        set(
            CAPABILITIES,
            Some("ApplicationDescription"),
            "A fast, native reader for PDF and Markdown",
        )?;
        set(
            r"Software\RegisteredApplications",
            Some("Kitab"),
            CAPABILITIES,
        )?;
        set(
            r"Software\Classes\Applications\kitab.exe\shell\open\command",
            None,
            &open,
        )?;
        set(
            r"Software\Classes\Applications\kitab.exe",
            Some("FriendlyAppName"),
            "Kitab Reader",
        )?;
        // Thumbnails, when kitab_shell.dll ships next to kitab.exe.
        let dll = std::path::Path::new(&exe).with_file_name("kitab_shell.dll");
        if dll.exists() {
            let ids: Vec<&str> = FILE_TYPES.iter().map(|t| t.prog_id).collect();
            register_thumbnails(&dll, &ids)?;
        }
        // SAFETY: tells Explorer the associations changed; no pointers passed.
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
        Ok(())
    }

    /// Register `dll` as the thumbnail provider for these ProgIDs only (never
    /// for an extension, so another reader's thumbnails stay theirs until the
    /// user makes Kitab the default).
    pub fn register_thumbnails(dll: &std::path::Path, prog_ids: &[&str]) -> Result<(), String> {
        let class = format!(r"Software\Classes\CLSID\{THUMB_CLSID}");
        set(&class, None, "Kitab Reader thumbnails")?;
        let server = format!(r"{class}\InprocServer32");
        set(&server, None, &dll.to_string_lossy())?;
        set(&server, Some("ThreadingModel"), "Apartment")?;
        for id in prog_ids {
            set(
                &format!(r"Software\Classes\{id}\ShellEx\{THUMB_SLOT}"),
                None,
                THUMB_CLSID,
            )?;
        }
        // SAFETY: tells Explorer the associations changed; no pointers passed.
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
        Ok(())
    }

    /// Remove the provider's class and its slot under these ProgIDs.
    pub fn unregister_thumbnails(prog_ids: &[&str]) {
        // SAFETY: deletes keys under HKEY_CURRENT_USER that Kitab created.
        unsafe {
            for id in prog_ids {
                let _ = RegDeleteTreeW(
                    HKEY_CURRENT_USER,
                    &HSTRING::from(format!(r"Software\Classes\{id}\ShellEx\{THUMB_SLOT}")),
                );
            }
            let _ = RegDeleteTreeW(
                HKEY_CURRENT_USER,
                &HSTRING::from(format!(r"Software\Classes\CLSID\{THUMB_CLSID}")),
            );
            SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
        }
    }

    /// Remove everything `register` wrote (uninstall, or "stop using Kitab").
    pub fn unregister() {
        let ids: Vec<&str> = FILE_TYPES.iter().map(|t| t.prog_id).collect();
        unregister_thumbnails(&ids);
        // SAFETY: deletes keys under HKEY_CURRENT_USER that Kitab created.
        unsafe {
            for t in FILE_TYPES {
                let _ = RegDeleteTreeW(
                    HKEY_CURRENT_USER,
                    &HSTRING::from(format!(r"Software\Classes\{}", t.prog_id)),
                );
                for ext in t.extensions {
                    let mut key = HKEY::default();
                    let path = HSTRING::from(format!(r"Software\Classes\.{ext}\OpenWithProgids"));
                    if RegCreateKeyExW(
                        HKEY_CURRENT_USER,
                        &path,
                        None,
                        None,
                        REG_OPTION_NON_VOLATILE,
                        KEY_WRITE,
                        None,
                        &mut key,
                        None,
                    )
                    .is_ok()
                    {
                        let _ = RegDeleteValueW(key, &HSTRING::from(t.prog_id));
                        let _ = RegCloseKey(key);
                    }
                }
            }
            let _ = RegDeleteTreeW(
                HKEY_CURRENT_USER,
                &HSTRING::from(r"Software\t21\Kitab\Capabilities"),
            );
            let _ = RegDeleteTreeW(
                HKEY_CURRENT_USER,
                &HSTRING::from(r"Software\Classes\Applications\kitab.exe"),
            );
            let mut key = HKEY::default();
            if RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(r"Software\RegisteredApplications"),
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
            .is_ok()
            {
                let _ = RegDeleteValueW(key, &HSTRING::from("Kitab"));
                let _ = RegCloseKey(key);
            }
            SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
        }
    }

    /// Where the user picks the default (Windows 10+ asks the user).
    pub const DEFAULTS_PAGE: &str = "ms-settings:defaultapps?registeredAppUser=Kitab";
}

#[cfg(target_os = "linux")]
mod imp {
    use super::FILE_TYPES;

    /// Linux: the package installs the .desktop file and icons; this sets
    /// Kitab as the default with `xdg-mime` for each type.
    pub fn register() -> Result<(), String> {
        for t in FILE_TYPES.iter().filter(|t| t.claim_default) {
            let ok = std::process::Command::new("xdg-mime")
                .args(["default", "dev.t21.kitab.desktop", t.mime])
                .status()
                .map_err(|e| format!("xdg-mime: {e}"))?
                .success();
            if !ok {
                return Err(format!("xdg-mime could not set {}", t.mime));
            }
        }
        Ok(())
    }

    pub fn unregister() {}

    pub const DEFAULTS_PAGE: &str = "";
}

#[cfg(target_os = "macos")]
mod imp {
    /// macOS: Info.plist declares the types; the user picks Kitab in Finder's
    /// Get Info › Open with. (NSWorkspace.setDefaultApplication comes with the
    /// macOS build.)
    pub fn register() -> Result<(), String> {
        Ok(())
    }

    pub fn unregister() {}

    pub const DEFAULTS_PAGE: &str = "";
}

pub use imp::{DEFAULTS_PAGE, register, unregister};
#[cfg(windows)]
pub use imp::{register_thumbnails, unregister_thumbnails};

#[cfg(all(test, windows))]
mod tests {
    /// Writes to the current user's registry, then removes it again. Run by
    /// hand: `cargo test -p kitab-os -- --ignored register_round_trip`.
    #[test]
    #[ignore = "writes to HKCU; run by hand"]
    fn register_round_trip() {
        super::register().expect("registers");
        let out = std::process::Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Classes\Kitab.PDF\DefaultIcon",
                "/ve",
            ])
            .output()
            .expect("reg runs");
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        super::unregister();
        assert!(text.contains(",-101"), "{text}");
        let gone = std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Classes\Kitab.PDF"])
            .output()
            .expect("reg runs");
        assert!(!gone.status.success(), "unregister removes the ProgID");
    }
}
