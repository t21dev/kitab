// SPDX-License-Identifier: AGPL-3.0-or-later

//! File types (PLAN.md §3.13 "File types"). Windows: ProgIDs `Kitab.PDF`,
//! `Kitab.Markdown`… under the current user's classes, each with its document
//! icon from kitab.exe, plus `Capabilities` and `RegisteredApplications` so
//! Kitab appears in Settings › Default apps. Windows 10+ never lets an app set
//! itself as the default silently; the user confirms there. Nothing is written
//! until the user asks (Settings › File types).
//!
//! Linux: Set up writes Kitab's .desktop entry, thumbnailer and MIME package
//! to the user's data folder when no package installed them (marked, so
//! Remove deletes only those), then makes Kitab the default with `xdg-mime`.
//! Remove takes Kitab out of `mimeapps.list`.
//!
//! macOS: Kitab.app's Info.plist declares the types; Set up makes Kitab the
//! default for each through Launch Services, and Remove hands PDFs, books
//! and text back to Preview, Books and TextEdit.

/// A format Kitab registers: extensions, ProgID, description, icon resource id
/// in kitab.exe (packaging/windows/kitab.rc), MIME type.
pub struct FileType {
    pub extensions: &'static [&'static str],
    pub prog_id: &'static str,
    pub description: &'static str,
    pub icon_id: u32,
    pub mime: &'static str,
    /// Its Uniform Type Identifier on macOS (packaging/macos/Info.plist).
    pub uti: &'static str,
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
        uti: "com.adobe.pdf",
        claim_default: true,
    },
    FileType {
        extensions: &["epub"],
        prog_id: "Kitab.EPUB",
        description: "EPUB book",
        icon_id: 102,
        mime: "application/epub+zip",
        uti: "org.idpf.epub-container",
        claim_default: true,
    },
    FileType {
        extensions: &["md", "markdown", "mdown", "mkd"],
        prog_id: "Kitab.Markdown",
        description: "Markdown document",
        icon_id: 103,
        mime: "text/markdown",
        uti: "net.daringfireball.markdown",
        claim_default: true,
    },
    FileType {
        extensions: &["mobi", "azw"],
        prog_id: "Kitab.MOBI",
        description: "MOBI book",
        icon_id: 104,
        mime: "application/x-mobipocket-ebook",
        uti: "dev.t21.kitab.mobi",
        claim_default: true,
    },
    FileType {
        extensions: &["fb2"],
        prog_id: "Kitab.FB2",
        description: "FictionBook",
        icon_id: 105,
        mime: "application/x-fictionbook+xml",
        uti: "dev.t21.kitab.fb2",
        claim_default: true,
    },
    FileType {
        extensions: &["cbz"],
        prog_id: "Kitab.CBZ",
        description: "Comic book archive",
        icon_id: 106,
        mime: "application/vnd.comicbook+zip",
        uti: "dev.t21.kitab.cbz",
        claim_default: true,
    },
    FileType {
        extensions: &["cbr"],
        prog_id: "Kitab.CBR",
        description: "Comic book archive (RAR)",
        icon_id: 109,
        mime: "application/vnd.comicbook-rar",
        uti: "dev.t21.kitab.cbr",
        claim_default: true,
    },
    FileType {
        extensions: &["cb7"],
        prog_id: "Kitab.CB7",
        description: "Comic book archive (7z)",
        icon_id: 110,
        mime: "application/x-cb7",
        uti: "dev.t21.kitab.cb7",
        claim_default: true,
    },
    FileType {
        extensions: &["djvu", "djv"],
        prog_id: "Kitab.DjVu",
        description: "DjVu document",
        icon_id: 111,
        mime: "image/vnd.djvu",
        uti: "dev.t21.kitab.djvu",
        claim_default: true,
    },
    FileType {
        extensions: &["xps", "oxps"],
        prog_id: "Kitab.XPS",
        description: "XPS document",
        icon_id: 107,
        mime: "application/oxps",
        uti: "com.microsoft.xps",
        claim_default: true,
    },
    FileType {
        extensions: &["txt"],
        prog_id: "Kitab.Text",
        description: "Text file",
        icon_id: 108,
        mime: "text/plain",
        uti: "public.plain-text",
        claim_default: false,
    },
];

/// Where one format stands with the system (Settings › File types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TypeStatus {
    /// Kitab is listed as an app that opens it.
    pub registered: bool,
    /// Kitab is the default for its first extension; `None` when the system
    /// doesn't say.
    pub default: Option<bool>,
    /// Explorer asks Kitab for its thumbnails (Windows).
    pub thumbnails: bool,
}

#[cfg(windows)]
mod imp {
    use super::{FILE_TYPES, TypeStatus};
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegGetValueW,
        RegSetValueExW,
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

    /// A string value under HKEY_CURRENT_USER, if it is there.
    fn get(path: &str, name: Option<&str>) -> Option<String> {
        let mut buf = [0u16; 512];
        let mut len = (buf.len() * 2) as u32;
        let name = name.map(HSTRING::from);
        // SAFETY: reads one REG_SZ value into a stack buffer whose byte size
        // is passed in `len`; the registry writes at most that many bytes.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(path),
                name.as_ref().map_or(windows::core::PCWSTR::null(), |n| {
                    windows::core::PCWSTR(n.as_ptr())
                }),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut len),
            )
        };
        if result.is_err() {
            return None;
        }
        let chars = (len as usize / 2).min(buf.len());
        let text = String::from_utf16_lossy(&buf[..chars]);
        Some(text.trim_end_matches('\0').to_string())
    }

    /// Where each of FILE_TYPES stands, in the same order.
    pub fn status() -> Vec<TypeStatus> {
        FILE_TYPES
            .iter()
            .map(|t| {
                let command = format!(r"Software\Classes\{}\shell\open\command", t.prog_id);
                // Newer Windows 11 keeps the choice in UserChoiceLatest\ProgId
                // and leaves the old UserChoice key behind with the previous
                // app in it; read the new one first.
                let choice = t.extensions.first().and_then(|ext| {
                    let base = format!(
                        r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.{ext}"
                    );
                    get(&format!(r"{base}\UserChoiceLatest\ProgId"), Some("ProgId"))
                        .filter(|p| !p.is_empty())
                        .or_else(|| get(&format!(r"{base}\UserChoice"), Some("ProgId")))
                });
                let slot = format!(r"Software\Classes\{}\ShellEx\{THUMB_SLOT}", t.prog_id);
                TypeStatus {
                    registered: get(&command, None).is_some(),
                    default: Some(choice.as_deref() == Some(t.prog_id)),
                    thumbnails: get(&slot, None)
                        .is_some_and(|c| c.eq_ignore_ascii_case(THUMB_CLSID)),
                }
            })
            .collect()
    }

    /// Register Kitab for its file types for the current user.
    pub fn register() -> Result<(), String> {
        register_types(&FILE_TYPES.iter().collect::<Vec<_>>())
    }

    /// Register Kitab for one format only (Settings › File types › Set up),
    /// with the app entry it needs.
    pub fn register_one(index: usize) -> Result<(), String> {
        let t = FILE_TYPES.get(index).ok_or("no such file type")?;
        register_types(&[t])
    }

    fn register_types(types: &[&super::FileType]) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.to_string_lossy().to_string();
        let open = format!("\"{exe}\" \"%1\"");
        for t in types {
            let classes = format!(r"Software\Classes\{}", t.prog_id);
            set(&classes, None, t.description)?;
            set(
                &format!(r"{classes}\DefaultIcon"),
                None,
                &format!("\"{exe}\",-{}", t.icon_id),
            )?;
            set(&format!(r"{classes}\shell\open\command"), None, &open)?;
            // Explorer's Print: printed without opening a window (not Markdown,
            // which prints from a later version, nor plain text).
            if !matches!(t.prog_id, "Kitab.Text" | "Kitab.Markdown") {
                set(
                    &format!(r"{classes}\shell\print\command"),
                    None,
                    &format!("\"{exe}\" --print \"%1\""),
                )?;
            }
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
            "A fast, native reader for PDF, books and Markdown",
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
            let ids: Vec<&str> = types.iter().map(|t| t.prog_id).collect();
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
    use super::{FILE_TYPES, TypeStatus};
    use crate::xdg::{self, DESKTOP_ID, WRITTEN_MARK};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn home() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }

    /// `$XDG_DATA_HOME`, or `~/.local/share`.
    fn data_home() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home().map(|h| h.join(".local/share")))
    }

    /// `$XDG_CONFIG_HOME`, or `~/.config`: where `mimeapps.list` lives.
    fn config_home() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home().map(|h| h.join(".config")))
    }

    /// Every data folder, the user's first.
    fn data_dirs() -> Vec<PathBuf> {
        let system = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        data_home()
            .into_iter()
            .chain(
                system
                    .split(':')
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute()),
            )
            .collect()
    }

    /// The first `sub` file in any data folder.
    fn find(sub: &str) -> Option<PathBuf> {
        data_dirs()
            .into_iter()
            .map(|d| d.join(sub))
            .find(|p| p.is_file())
    }

    const DESKTOP: &str = "applications/dev.t21.kitab.desktop";
    const THUMBNAILER: &str = "thumbnailers/dev.t21.kitab.thumbnailer";
    const MIME: &str = "mime/packages/dev.t21.kitab.xml";

    /// The Kitab to run: the AppImage itself when inside one (its own path
    /// is a temporary mount), else this executable.
    fn exe() -> Result<String, String> {
        if let Some(a) = std::env::var_os("APPIMAGE") {
            return Ok(a.to_string_lossy().to_string());
        }
        std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| format!("can't tell where Kitab is: {e}"))
    }

    fn write(path: &Path, text: &str) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Kitab's files in the user's data folder, for what no package
    /// installed; the menus and MIME caches are told.
    fn install() -> Result<(), String> {
        let home = data_home().ok_or("there is no home folder")?;
        let mut desktop = false;
        let mut mime = false;
        if find(DESKTOP).is_none() {
            write(&home.join(DESKTOP), &xdg::desktop_entry(&exe()?))?;
            desktop = true;
        }
        if find(THUMBNAILER).is_none() {
            write(&home.join(THUMBNAILER), &xdg::thumbnailer_entry(&exe()?))?;
        }
        if find(MIME).is_none() {
            write(&home.join(MIME), &xdg::mime_package())?;
            mime = true;
        }
        refresh(&home, desktop, mime);
        Ok(())
    }

    /// Tell the desktop about changed entries (best effort: the tools may
    /// be missing, and the files still work after the next login).
    fn refresh(home: &Path, desktop: bool, mime: bool) {
        if desktop {
            let _ = Command::new("update-desktop-database")
                .arg(home.join("applications"))
                .status();
        }
        if mime {
            let _ = Command::new("update-mime-database")
                .arg(home.join("mime"))
                .status();
        }
    }

    fn query_default(mime: &str) -> Option<String> {
        Command::new("xdg-mime")
            .args(["query", "default", mime])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    }

    /// Where each of FILE_TYPES stands: listed by an installed .desktop
    /// entry, the default per `xdg-mime`, and a thumbnailer present.
    pub fn status() -> Vec<TypeStatus> {
        let listed = find(DESKTOP)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|e| xdg::desktop_mimes(&e))
            .unwrap_or_default();
        let thumbnails = find(THUMBNAILER).is_some();
        FILE_TYPES
            .iter()
            .map(|t| TypeStatus {
                registered: listed.iter().any(|m| m == t.mime),
                default: query_default(t.mime).map(|a| a == DESKTOP_ID),
                thumbnails,
            })
            .collect()
    }

    fn set_default(mime: &str) -> Result<(), String> {
        for m in std::iter::once(mime).chain(xdg::aliases(mime).iter().copied()) {
            let ok = Command::new("xdg-mime")
                .args(["default", DESKTOP_ID, m])
                .status()
                .map_err(|_| "xdg-mime isn't installed (it comes with xdg-utils)".to_string())?
                .success();
            if !ok {
                return Err(format!("xdg-mime could not set {m}"));
            }
        }
        Ok(())
    }

    /// Set up one type: Kitab's files if missing, then the default.
    pub fn register_one(index: usize) -> Result<(), String> {
        let t = FILE_TYPES.get(index).ok_or("no such file type")?;
        install()?;
        set_default(t.mime)
    }

    /// Set up all: Kitab's files, then the default for every type it claims
    /// (not plain text, which would take over every text file).
    pub fn register() -> Result<(), String> {
        install()?;
        FILE_TYPES
            .iter()
            .filter(|t| t.claim_default)
            .try_for_each(|t| set_default(t.mime))
    }

    /// Take Kitab out of `mimeapps.list` (both places it can be), and
    /// delete the files Set up wrote. A package's files stay.
    pub fn unregister() {
        let lists = [
            config_home().map(|c| c.join("mimeapps.list")),
            data_home().map(|d| d.join("applications/mimeapps.list")),
        ];
        for list in lists.into_iter().flatten() {
            if let Ok(text) = std::fs::read_to_string(&list) {
                let clean = xdg::without_kitab(&text);
                if clean != text {
                    let _ = std::fs::write(&list, clean);
                }
            }
        }
        let Some(home) = data_home() else { return };
        let mut gone = [false; 3];
        for (i, sub) in [DESKTOP, THUMBNAILER, MIME].into_iter().enumerate() {
            let path = home.join(sub);
            let ours = std::fs::read_to_string(&path).is_ok_and(|t| t.contains(WRITTEN_MARK));
            if ours {
                gone[i] = std::fs::remove_file(&path).is_ok();
            }
        }
        refresh(&home, gone[0], gone[2]);
    }

    pub const DEFAULTS_PAGE: &str = "";
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{FILE_TYPES, TypeStatus};
    use std::ffi::c_void;

    type CFStringRef = *const c_void;
    const UTF8: u32 = 0x0800_0100; // kCFStringEncodingUTF8
    const ALL_ROLES: u32 = 0xFFFF_FFFF; // kLSRolesAll
    const BUNDLE_ID: &str = "dev.t21.kitab";

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithBytes(
            alloc: *const c_void,
            bytes: *const u8,
            len: isize,
            encoding: u32,
            external: u8,
        ) -> CFStringRef;
        fn CFStringGetCString(s: CFStringRef, buf: *mut u8, size: isize, encoding: u32) -> u8;
        fn CFRelease(cf: *const c_void);
    }

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSCopyDefaultRoleHandlerForContentType(uti: CFStringRef, role: u32) -> CFStringRef;
        fn LSSetDefaultRoleHandlerForContentType(
            uti: CFStringRef,
            role: u32,
            bundle: CFStringRef,
        ) -> i32;
    }

    /// A CFString owned here, released on drop.
    struct Cf(CFStringRef);

    impl Cf {
        fn new(s: &str) -> Option<Cf> {
            // SAFETY: the bytes are valid UTF-8 for `len` bytes; CoreFoundation
            // copies them (external = 0). A null result is checked.
            let r = unsafe {
                CFStringCreateWithBytes(std::ptr::null(), s.as_ptr(), s.len() as isize, UTF8, 0)
            };
            (!r.is_null()).then_some(Cf(r))
        }

        fn text(&self) -> Option<String> {
            let mut buf = [0u8; 256];
            // SAFETY: self.0 is a live CFString; the buffer's size is passed,
            // and CoreFoundation writes a NUL-terminated string within it.
            let ok =
                unsafe { CFStringGetCString(self.0, buf.as_mut_ptr(), buf.len() as isize, UTF8) };
            if ok == 0 {
                return None;
            }
            let end = buf.iter().position(|&b| b == 0)?;
            Some(String::from_utf8_lossy(&buf[..end]).to_string())
        }
    }

    impl Drop for Cf {
        fn drop(&mut self) {
            // SAFETY: created or copied by a CF "Create"/"Copy" call, released once.
            unsafe { CFRelease(self.0) }
        }
    }

    /// The bundle id of the app that opens `uti`, if any.
    fn handler(uti: &str) -> Option<String> {
        let uti = Cf::new(uti)?;
        // SAFETY: `uti` is a live CFString; the result follows the Copy rule
        // and is released by `Cf`.
        let r = unsafe { LSCopyDefaultRoleHandlerForContentType(uti.0, ALL_ROLES) };
        if r.is_null() {
            return None;
        }
        Cf(r).text()
    }

    fn set_handler(uti: &str, bundle: &str) -> Result<(), String> {
        let (Some(u), Some(b)) = (Cf::new(uti), Cf::new(bundle)) else {
            return Err("out of memory".into());
        };
        // SAFETY: both are live CFStrings for the duration of the call.
        let code = unsafe { LSSetDefaultRoleHandlerForContentType(u.0, ALL_ROLES, b.0) };
        if code == 0 {
            Ok(())
        } else {
            Err(format!("macOS refused it (Launch Services error {code})"))
        }
    }

    /// Launch Services knows Kitab only as Kitab.app (its Info.plist).
    fn in_app_bundle() -> bool {
        std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
    }

    /// Where each of FILE_TYPES stands: declared by Kitab.app, and whether
    /// Launch Services opens it with Kitab.
    pub fn status() -> Vec<TypeStatus> {
        let registered = in_app_bundle();
        FILE_TYPES
            .iter()
            .map(|t| TypeStatus {
                registered,
                default: Some(handler(t.uti).is_some_and(|h| h.eq_ignore_ascii_case(BUNDLE_ID))),
                thumbnails: false,
            })
            .collect()
    }

    fn set_up(t: &super::FileType) -> Result<(), String> {
        if !in_app_bundle() {
            return Err("macOS opens files only with Kitab.app, and this Kitab isn't one".into());
        }
        set_handler(t.uti, BUNDLE_ID)
    }

    pub fn register_one(index: usize) -> Result<(), String> {
        set_up(FILE_TYPES.get(index).ok_or("no such file type")?)
    }

    /// Set up all: every type Kitab claims (not plain text).
    pub fn register() -> Result<(), String> {
        FILE_TYPES
            .iter()
            .filter(|t| t.claim_default)
            .try_for_each(set_up)
    }

    /// Hand the types Kitab opens back to the apps macOS ships with. Types
    /// with no such app keep Kitab until another app is chosen in Finder.
    pub fn unregister() {
        for t in FILE_TYPES {
            if !handler(t.uti).is_some_and(|h| h.eq_ignore_ascii_case(BUNDLE_ID)) {
                continue;
            }
            let system = match t.mime {
                "application/pdf" => "com.apple.Preview",
                "application/epub+zip" => "com.apple.iBooksX",
                "text/markdown" | "text/plain" => "com.apple.TextEdit",
                _ => continue,
            };
            let _ = set_handler(t.uti, system);
        }
    }

    pub const DEFAULTS_PAGE: &str = "";
}

pub use imp::{DEFAULTS_PAGE, register, register_one, status, unregister};

/// The system page where the user picks Kitab as the default app, or "".
/// The Store build has no `RegisteredApplications` entry to point at (its
/// manifest declares the types), so it opens the page itself.
pub fn defaults_page() -> &'static str {
    if cfg!(windows) && crate::packaged() {
        "ms-settings:defaultapps"
    } else {
        DEFAULTS_PAGE
    }
}
#[cfg(windows)]
pub use imp::{register_thumbnails, unregister_thumbnails};

#[cfg(all(test, windows))]
mod tests {
    /// Read-only: one answer per format, and reading never writes.
    #[test]
    fn status_covers_every_type() {
        assert_eq!(super::status().len(), super::FILE_TYPES.len());
    }

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
        let pdf = super::status()[0];
        super::unregister();
        assert!(pdf.registered, "status sees the registration");
        assert!(!super::status()[0].registered, "and sees it go");
        assert!(text.contains(",-101"), "{text}");
        let gone = std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Classes\Kitab.PDF"])
            .output()
            .expect("reg runs");
        assert!(!gone.status.success(), "unregister removes the ProgID");
    }
}
