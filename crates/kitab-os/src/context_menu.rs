// SPDX-License-Identifier: AGPL-3.0-or-later

//! Kitab's tools in the file manager's right-click menu (Settings › File
//! types › Right-click tools): Combine into one PDF, Make a PDF from
//! pictures, Make a smaller copy, Export as Markdown or text. Each runs
//! `kitab --tool WORD FILE...` (macOS: a `kitab://tool/...` link, which
//! reaches the running app), and Kitab opens that tool with the files.
//!
//! - Windows: `kitab_shell.dll`'s menu handler under the file types' shell
//!   verbs (the classic menu, "Show more options" on Windows 11). The Store
//!   build declares it in its package manifest instead (Windows 11's own
//!   menu), so nothing is written here for it.
//! - macOS: Quick Actions in `~/Library/Services`.
//! - Linux: Files (Nautilus) scripts, Dolphin service menus, Nemo actions,
//!   and Thunar custom actions.
//!
//! Everything written is Kitab's alone and removed by [`uninstall`].

/// One right-click tool: the word `kitab --tool` takes and its title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuEntry {
    pub word: &'static str,
    pub title: &'static str,
    /// File extensions it's for (lower case).
    pub exts: &'static [&'static str],
    /// Two or more files (Combine), or any number (Make a PDF), else one.
    pub many: bool,
}

const PDF: &[&str] = &["pdf"];
const PICTURES: &[&str] = &[
    "jpg", "jpeg", "jfif", "png", "webp", "tif", "tiff", "bmp", "gif",
];
const DOCUMENTS: &[&str] = &["pdf", "epub", "mobi", "azw", "azw3", "fb2", "xps", "oxps"];

pub const ENTRIES: [MenuEntry; 8] = [
    MenuEntry {
        word: "merge",
        title: "Combine into one PDF",
        exts: PDF,
        many: true,
    },
    MenuEntry {
        word: "pictures",
        title: "Make a PDF from pictures",
        exts: PICTURES,
        many: true,
    },
    MenuEntry {
        word: "delete-pages",
        title: "Remove pages",
        exts: PDF,
        many: false,
    },
    MenuEntry {
        word: "move-pages",
        title: "Reorder pages",
        exts: PDF,
        many: false,
    },
    MenuEntry {
        word: "rotate-pages",
        title: "Rotate pages",
        exts: PDF,
        many: false,
    },
    MenuEntry {
        word: "compress",
        title: "Make a smaller copy",
        exts: PDF,
        many: false,
    },
    MenuEntry {
        word: "export",
        title: "Export as Markdown",
        exts: DOCUMENTS,
        many: false,
    },
    MenuEntry {
        word: "export-text",
        title: "Export as text",
        exts: DOCUMENTS,
        many: false,
    },
];

/// Every extension some tool is for.
pub fn all_exts() -> Vec<&'static str> {
    let mut v: Vec<&str> = ENTRIES
        .iter()
        .flat_map(|e| e.exts.iter().copied())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Whether this Kitab manages the menu itself: not the Store build (its
/// manifest does) nor a Flatpak (the sandbox can't write the host's menus).
pub fn manages_menu() -> bool {
    !crate::packaged() && std::env::var_os("FLATPAK_ID").is_none()
}

/// The right-click tools should be there unless turned off: on by default,
/// except for a portable copy, which writes nothing outside its folder
/// unless asked.
pub fn default_on() -> bool {
    crate::portable_dir().is_none()
}

/// Make the menu match the setting (at start-up, and when it changes).
pub fn sync(on: bool) -> Result<(), String> {
    if !manages_menu() {
        return Ok(());
    }
    if on {
        if !installed() {
            install()?;
        }
    } else if installed() {
        uninstall();
    }
    Ok(())
}

/// How the menus start Kitab: this executable (an AppImage's own file).
/// macOS's Quick Actions open a kitab:// link instead.
#[cfg(not(target_os = "macos"))]
fn kitab_command() -> Option<std::path::PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Some(std::path::PathBuf::from(appimage));
    }
    std::env::current_exe().ok()
}

/// Uninstalling (`kitab.exe --unregister`): take the right-click tools away
/// when they are this copy's (its `kitab_shell.dll`) or a removed Kitab's,
/// and leave another Kitab's in place.
pub fn uninstall_if_here() {
    #[cfg(windows)]
    {
        let Some(dll) = std::env::current_exe()
            .ok()
            .map(|exe| exe.with_file_name("kitab_shell.dll"))
        else {
            return;
        };
        if manages_menu()
            && imp::registered_dll().is_some_and(|d| !d.is_file() || crate::same_file(&d, &dll))
        {
            uninstall();
        }
    }
}

/// Re-point the right-click tools at this Kitab when the Kitab they name is
/// gone (it moved; its kitab_shell.dll may linger until a restart while
/// Explorer holds it). The installer runs it as part of `kitab.exe
/// --relink`, before Kitab's first start syncs the setting.
pub fn relink() -> Result<bool, String> {
    #[cfg(windows)]
    {
        let gone = imp::registered_dll()
            .is_some_and(|dll| !dll.is_file() || !dll.with_file_name("kitab.exe").is_file());
        if !manages_menu() || !gone {
            return Ok(false);
        }
        install()?;
        Ok(true)
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

#[cfg(windows)]
pub use imp::install_for;
pub use imp::{install, installed, uninstall};

// ─── Windows ─────────────────────────────────────────────────────────────

#[cfg(windows)]
mod imp {
    use super::all_exts;
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegDeleteTreeW, RegOpenKeyExW, RegSetValueExW,
    };
    use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
    use windows::core::HSTRING;

    /// kitab_shell.dll's menu handler (kitab-shell's `CLSID_KITAB_MENU`).
    pub const MENU_CLSID: &str = "{5B2C9E41-7D3A-4F6E-9A1B-C8E2F0D4A713}";
    const VERB: &str = "KitabTools";

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

    fn verb_key(ext: &str) -> String {
        format!(r"Software\Classes\SystemFileAssociations\.{ext}\shell\{VERB}")
    }

    /// The "Kitab" submenu on every file type a tool is for, from the
    /// kitab_shell.dll next to kitab.exe.
    pub fn install() -> Result<(), String> {
        let exe = super::kitab_command().ok_or("can't tell where Kitab is")?;
        install_for(&exe)
    }

    /// [`install`] for the kitab.exe at `exe` (tests point at a build).
    pub fn install_for(exe: &std::path::Path) -> Result<(), String> {
        let dll = exe.with_file_name("kitab_shell.dll");
        if !dll.exists() {
            return Err(format!("{} isn't there", dll.display()));
        }
        let class = format!(r"Software\Classes\CLSID\{MENU_CLSID}");
        set(&class, None, "Kitab Reader tools")?;
        let server = format!(r"{class}\InprocServer32");
        set(&server, None, &dll.to_string_lossy())?;
        set(&server, Some("ThreadingModel"), "Apartment")?;
        for ext in all_exts() {
            let key = verb_key(ext);
            set(&key, Some("MUIVerb"), "Kitab")?;
            set(&key, Some("Icon"), &format!("{},0", exe.display()))?;
            // The handler builds the submenu (SubCommands empty) and gets the
            // whole selection however many files (MultiSelectModel Player).
            set(&key, Some("ExplorerCommandHandler"), MENU_CLSID)?;
            set(&key, Some("SubCommands"), "")?;
            set(&key, Some("MultiSelectModel"), "Player")?;
        }
        // SAFETY: tells Explorer the associations changed; no pointers passed.
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
        Ok(())
    }

    pub fn uninstall() {
        // SAFETY: deletes keys under HKEY_CURRENT_USER that Kitab created.
        unsafe {
            for ext in all_exts() {
                let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(verb_key(ext)));
            }
            let _ = RegDeleteTreeW(
                HKEY_CURRENT_USER,
                &HSTRING::from(format!(r"Software\Classes\CLSID\{MENU_CLSID}")),
            );
            SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
        }
    }

    /// The menu is there, for this Kitab (a moved or updated Kitab writes
    /// it again).
    pub fn installed() -> bool {
        let mut key = HKEY::default();
        // SAFETY: opens and closes a key under HKEY_CURRENT_USER.
        let open = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(verb_key("pdf")),
                None,
                KEY_READ,
                &mut key,
            )
            .is_ok()
        };
        if !open {
            return false;
        }
        // SAFETY: closing the handle just opened.
        unsafe {
            let _ = RegCloseKey(key);
        }
        // Points at this Kitab's DLL.
        let dll = super::kitab_command().map(|e| e.with_file_name("kitab_shell.dll"));
        let now = registered_dll();
        dll.zip(now)
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case_path(&b))
    }

    trait PathEq {
        fn eq_ignore_ascii_case_path(&self, other: &std::path::Path) -> bool;
    }
    impl PathEq for std::path::PathBuf {
        fn eq_ignore_ascii_case_path(&self, other: &std::path::Path) -> bool {
            self.to_string_lossy()
                .eq_ignore_ascii_case(&other.to_string_lossy())
        }
    }

    /// The DLL the menu class points at now.
    pub(super) fn registered_dll() -> Option<std::path::PathBuf> {
        use windows::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
        let path = HSTRING::from(format!(
            r"Software\Classes\CLSID\{MENU_CLSID}\InprocServer32"
        ));
        let mut buf = vec![0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: reads a string value into `buf`, `len` bytes long.
        let ok = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &path,
                None,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut len),
            )
            .is_ok()
        };
        if !ok {
            return None;
        }
        let n = (len as usize / 2).saturating_sub(1).min(buf.len());
        Some(std::path::PathBuf::from(String::from_utf16_lossy(
            &buf[..n],
        )))
    }
}

// ─── macOS ───────────────────────────────────────────────────────────────

/// A Quick Action's shell script: the selected files go to Kitab through a
/// `kitab://tool/WORD?list=FILE` link (a list of paths in a temporary file),
/// so they reach the running app like any opened file.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mac_script(word: &str) -> String {
    format!(
        "list=\"$(mktemp \"${{TMPDIR:-/tmp}}/kitab-tool.XXXXXX\")\"\n\
         for f in \"$@\"; do printf '%s\\n' \"$f\" >> \"$list\"; done\n\
         open -b dev.t21.kitab \"kitab://tool/{word}?list=$list\"\n"
    )
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn uti(ext: &str) -> &'static str {
    match ext {
        "pdf" => "com.adobe.pdf",
        "epub" => "org.idpf.epub-container",
        _ => "public.data",
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mac_info_plist(e: &MenuEntry) -> String {
    let mut types: Vec<&str> = e.exts.iter().map(|x| uti(x)).collect();
    if e.word == "pictures" {
        types = vec!["public.image"];
    }
    types.sort_unstable();
    types.dedup();
    let types: String = types
        .iter()
        .map(|t| format!("\t\t\t\t<string>{t}</string>\n"))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>NSServices</key>
	<array>
		<dict>
			<key>NSMenuItem</key>
			<dict>
				<key>default</key>
				<string>Kitab: {title}</string>
			</dict>
			<key>NSMessage</key>
			<string>runWorkflowAsService</string>
			<key>NSRequiredContext</key>
			<dict>
				<key>NSApplicationIdentifier</key>
				<string>com.apple.finder</string>
			</dict>
			<key>NSSendFileTypes</key>
			<array>
{types}			</array>
		</dict>
	</array>
</dict>
</plist>
"#,
        title = xml_escape(e.title),
    )
}

/// Automator's workflow: one "Run Shell Script" action, files passed as
/// arguments, in Finder's Quick Actions.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mac_document_wflow(e: &MenuEntry) -> String {
    let script = xml_escape(&mac_script(e.word));
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AMApplicationBuild</key>
	<string>521</string>
	<key>AMApplicationVersion</key>
	<string>2.10</string>
	<key>AMDocumentVersion</key>
	<string>2</string>
	<key>actions</key>
	<array>
		<dict>
			<key>action</key>
			<dict>
				<key>AMAccepts</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Optional</key>
					<true/>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>AMActionVersion</key>
				<string>2.0.3</string>
				<key>AMApplication</key>
				<array>
					<string>Automator</string>
				</array>
				<key>AMParameterProperties</key>
				<dict>
					<key>COMMAND_STRING</key>
					<dict/>
					<key>CheckedForUserDefaultShell</key>
					<dict/>
					<key>inputMethod</key>
					<dict/>
					<key>shell</key>
					<dict/>
					<key>source</key>
					<dict/>
				</dict>
				<key>AMProvides</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>ActionBundlePath</key>
				<string>/System/Library/Automator/Run Shell Script.action</string>
				<key>ActionName</key>
				<string>Run Shell Script</string>
				<key>ActionParameters</key>
				<dict>
					<key>COMMAND_STRING</key>
					<string>{script}</string>
					<key>CheckedForUserDefaultShell</key>
					<true/>
					<key>inputMethod</key>
					<integer>1</integer>
					<key>shell</key>
					<string>/bin/bash</string>
					<key>source</key>
					<string></string>
				</dict>
				<key>BundleIdentifier</key>
				<string>com.apple.RunShellScript</string>
				<key>CFBundleVersion</key>
				<string>2.0.3</string>
				<key>CanShowSelectedItemsWhenRun</key>
				<false/>
				<key>CanShowWhenRun</key>
				<true/>
				<key>Category</key>
				<array>
					<string>AMCategoryUtilities</string>
				</array>
				<key>Class Name</key>
				<string>RunShellScriptAction</string>
				<key>InputUUID</key>
				<string>8F1B4C2E-6A1D-4B7A-9C55-0E0C7D3A1B01</string>
				<key>Keywords</key>
				<array>
					<string>Shell</string>
					<string>Script</string>
				</array>
				<key>OutputUUID</key>
				<string>8F1B4C2E-6A1D-4B7A-9C55-0E0C7D3A1B02</string>
				<key>UUID</key>
				<string>8F1B4C2E-6A1D-4B7A-9C55-0E0C7D3A1B03</string>
				<key>UnlocalizedApplications</key>
				<array>
					<string>Automator</string>
				</array>
				<key>arguments</key>
				<dict/>
				<key>isViewVisible</key>
				<integer>1</integer>
			</dict>
		</dict>
	</array>
	<key>connectors</key>
	<dict/>
	<key>workflowMetaData</key>
	<dict>
		<key>applicationBundleIDsByPath</key>
		<dict/>
		<key>applicationPaths</key>
		<array/>
		<key>inputTypeIdentifier</key>
		<string>com.apple.Automator.fileSystemObject</string>
		<key>outputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>presentationMode</key>
		<integer>15</integer>
		<key>processesInput</key>
		<false/>
		<key>serviceInputTypeIdentifier</key>
		<string>com.apple.Automator.fileSystemObject</string>
		<key>serviceOutputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>serviceProcessesInput</key>
		<false/>
		<key>systemImageName</key>
		<string>NSActionTemplate</string>
		<key>useAutomaticInputType</key>
		<false/>
		<key>workflowTypeIdentifier</key>
		<string>com.apple.Automator.servicesMenu</string>
	</dict>
</dict>
</plist>
"#
    )
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{ENTRIES, MenuEntry};
    use std::path::PathBuf;

    fn services() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Services"))
    }

    fn bundle_name(e: &MenuEntry) -> String {
        format!("Kitab - {}.workflow", e.title)
    }

    pub fn install() -> Result<(), String> {
        let dir = services().ok_or("no home folder")?;
        for e in &ENTRIES {
            let contents = dir.join(bundle_name(e)).join("Contents");
            std::fs::create_dir_all(&contents).map_err(|e| e.to_string())?;
            std::fs::write(contents.join("Info.plist"), super::mac_info_plist(e))
                .map_err(|e| e.to_string())?;
            std::fs::write(
                contents.join("document.wflow"),
                super::mac_document_wflow(e),
            )
            .map_err(|e| e.to_string())?;
        }
        // Finder picks new services up after the services cache is refreshed.
        let _ = std::process::Command::new("/System/Library/CoreServices/pbs")
            .arg("-update")
            .status();
        Ok(())
    }

    pub fn uninstall() {
        let Some(dir) = services() else { return };
        for e in &ENTRIES {
            let _ = std::fs::remove_dir_all(dir.join(bundle_name(e)));
        }
        let _ = std::process::Command::new("/System/Library/CoreServices/pbs")
            .arg("-update")
            .status();
    }

    pub fn installed() -> bool {
        services().is_some_and(|d| d.join(bundle_name(&ENTRIES[0])).exists())
    }
}

// ─── Linux ───────────────────────────────────────────────────────────────

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::{ENTRIES, MenuEntry, kitab_command};
    use std::path::{Path, PathBuf};

    /// Marks every file Kitab writes, so uninstall removes only those.
    const MARK: &str = "kitab-right-click-tools";

    fn home() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }

    fn data_home() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".local/share")))
    }

    fn config_home() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".config")))
    }

    fn quote(p: &Path) -> String {
        format!("'{}'", p.to_string_lossy().replace('\'', r"'\''"))
    }

    fn mimes(e: &MenuEntry) -> Vec<&'static str> {
        let mut v: Vec<&str> = e
            .exts
            .iter()
            .map(|x| match *x {
                "pdf" => "application/pdf",
                "jpg" | "jpeg" | "jfif" => "image/jpeg",
                "png" => "image/png",
                "webp" => "image/webp",
                "tif" | "tiff" => "image/tiff",
                "bmp" => "image/bmp",
                "gif" => "image/gif",
                "epub" => "application/epub+zip",
                "mobi" | "azw" | "azw3" => "application/x-mobipocket-ebook",
                "fb2" => "application/x-fictionbook+xml",
                _ => "application/oxps",
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    fn nautilus_dir() -> Option<PathBuf> {
        data_home().map(|d| d.join("nautilus/scripts"))
    }
    fn dolphin_file() -> Option<PathBuf> {
        data_home().map(|d| d.join("kio/servicemenus/dev.t21.kitab-tools.desktop"))
    }
    fn nemo_dir() -> Option<PathBuf> {
        data_home().map(|d| d.join("nemo/actions"))
    }
    fn thunar_file() -> Option<PathBuf> {
        config_home().map(|d| d.join("Thunar/uca.xml"))
    }

    /// Files (Nautilus): an executable script per tool, in a Kitab folder:
    /// Scripts › Kitab › the tools.
    fn nautilus_script(kitab: &Path, e: &MenuEntry) -> String {
        format!(
            "#!/bin/sh\n# {MARK}\n# Kitab: {title}. Files hands the selection over as arguments.\nexec {kitab} --tool {word} \"$@\"\n",
            title = e.title,
            kitab = quote(kitab),
            word = e.word,
        )
    }

    /// Dolphin: one service menu with every tool, by MIME type.
    fn dolphin_menu(kitab: &Path) -> String {
        let mut all: Vec<&str> = ENTRIES.iter().flat_map(mimes).collect();
        all.sort_unstable();
        all.dedup();
        let mut out = format!(
            "# {MARK}\n[Desktop Entry]\nType=Service\nMimeType={};\nActions={}\nX-KDE-Submenu=Kitab\nX-KDE-Priority=TopLevel\n",
            all.join(";"),
            ENTRIES.iter().map(|e| e.word).collect::<Vec<_>>().join(";"),
        );
        for e in &ENTRIES {
            out.push_str(&format!(
                "\n[Desktop Action {word}]\nName={title}\nIcon=dev.t21.kitab\nExec={kitab} --tool {word} %F\n",
                word = e.word,
                title = e.title,
                kitab = quote(kitab),
            ));
        }
        out
    }

    /// Nemo: an action per tool, by extension and how many files.
    fn nemo_action(kitab: &Path, e: &MenuEntry) -> String {
        let selection = match (e.many, e.word) {
            (true, "merge") => "m",
            (true, _) => "notnone",
            (false, _) => "s",
        };
        format!(
            "# {MARK}\n[Nemo Action]\nName=Kitab: {title}\nComment=Open in Kitab: {title}\nExec={kitab} --tool {word} %F\nIcon-Name=dev.t21.kitab\nSelection={selection}\nExtensions={exts};\n",
            title = e.title,
            kitab = quote(kitab),
            word = e.word,
            exts = e.exts.join(";"),
        )
    }

    /// Thunar: custom actions, appended to the user's uca.xml between
    /// markers (everything else in it stays as it was).
    fn thunar_actions(kitab: &Path) -> String {
        let mut out = format!("<!-- {MARK} begin -->\n");
        for e in &ENTRIES {
            let patterns: String = e
                .exts
                .iter()
                .map(|x| format!("*.{x};*.{};", x.to_uppercase()))
                .collect();
            let kind = if e.word == "pictures" {
                "<image-files/>"
            } else {
                "<other-files/>"
            };
            out.push_str(&format!(
                "<action>\n\t<icon>dev.t21.kitab</icon>\n\t<name>Kitab: {title}</name>\n\t<unique-id>kitab-{word}</unique-id>\n\t<command>{kitab} --tool {word} %F</command>\n\t<description>Open in Kitab: {title}</description>\n\t<patterns>{patterns}</patterns>\n\t{kind}\n</action>\n",
                title = e.title,
                word = e.word,
                kitab = quote(kitab).replace('&', "&amp;").replace('<', "&lt;"),
            ));
        }
        out.push_str(&format!("<!-- {MARK} end -->\n"));
        out
    }

    fn without_thunar_block(xml: &str) -> String {
        let begin = format!("<!-- {MARK} begin -->");
        let end = format!("<!-- {MARK} end -->\n");
        match (xml.find(&begin), xml.find(&end)) {
            (Some(a), Some(b)) if b > a => format!("{}{}", &xml[..a], &xml[b + end.len()..]),
            _ => xml.to_string(),
        }
    }

    fn write_exec(path: &Path, text: &str) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, text).map_err(|e| e.to_string())?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())
    }

    fn nautilus_path(e: &MenuEntry) -> Option<PathBuf> {
        nautilus_dir().map(|d| d.join("Kitab").join(e.title))
    }

    fn nemo_path(e: &MenuEntry) -> Option<PathBuf> {
        nemo_dir().map(|d| d.join(format!("kitab-{}.nemo_action", e.word)))
    }

    pub fn install() -> Result<(), String> {
        let kitab = kitab_command().ok_or("can't tell where Kitab is")?;
        for e in &ENTRIES {
            if let Some(p) = nautilus_path(e) {
                write_exec(&p, &nautilus_script(&kitab, e))?;
            }
            if let Some(p) = nemo_path(e) {
                if let Some(dir) = p.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(&p, nemo_action(&kitab, e)).map_err(|e| e.to_string())?;
            }
        }
        // Dolphin (KDE 6) runs only service menus marked executable.
        if let Some(p) = dolphin_file() {
            write_exec(&p, &dolphin_menu(&kitab))?;
        }
        // Thunar: only where Thunar has made its file (it's installed).
        if let Some(p) = thunar_file()
            && let Ok(xml) = std::fs::read_to_string(&p)
        {
            let xml = without_thunar_block(&xml);
            if let Some(at) = xml.rfind("</actions>") {
                let new = format!("{}{}{}", &xml[..at], thunar_actions(&kitab), &xml[at..]);
                std::fs::write(&p, new).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    /// Remove a file only if Kitab wrote it.
    fn remove_ours(p: &Path) {
        if std::fs::read_to_string(p).is_ok_and(|t| t.contains(MARK)) {
            let _ = std::fs::remove_file(p);
        }
    }

    pub fn uninstall() {
        for e in &ENTRIES {
            if let Some(p) = nautilus_path(e) {
                remove_ours(&p);
            }
            if let Some(p) = nemo_path(e) {
                remove_ours(&p);
            }
        }
        // The Kitab folder in Scripts, once it's empty (never the user's own files).
        if let Some(d) = nautilus_dir() {
            let _ = std::fs::remove_dir(d.join("Kitab"));
        }
        if let Some(p) = dolphin_file() {
            remove_ours(&p);
        }
        if let Some(p) = thunar_file()
            && let Ok(xml) = std::fs::read_to_string(&p)
            && xml.contains(MARK)
        {
            let _ = std::fs::write(&p, without_thunar_block(&xml));
        }
    }

    pub fn installed() -> bool {
        let Some(kitab) = kitab_command() else {
            return false;
        };
        // There, and for this Kitab (a moved AppImage writes them again).
        nautilus_path(&ENTRIES[0])
            .and_then(|p| std::fs::read_to_string(p).ok())
            .is_some_and(|t| t.contains(MARK) && t.contains(&quote(&kitab)))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn menus_name_kitab_and_its_tools() {
            let k = Path::new("/opt/Kitab's/kitab");
            let d = dolphin_menu(k);
            assert!(d.contains(
                "Actions=merge;pictures;delete-pages;move-pages;rotate-pages;compress;export;export-text"
            ));
            assert!(d.contains("Exec='/opt/Kitab'\\''s/kitab' --tool merge %F"));
            assert!(d.contains("application/pdf") && d.contains("image/webp"));
            let n = nemo_action(k, &ENTRIES[0]);
            assert!(n.contains("Selection=m") && n.contains("Extensions=pdf;"));
            assert!(nautilus_script(k, &ENTRIES[5]).contains("--tool compress \"$@\""));
            // Grouped: Scripts › Kitab › the tool, and Dolphin's Kitab submenu.
            assert!(
                nautilus_path(&ENTRIES[0])
                    .is_some_and(|p| p.ends_with("nautilus/scripts/Kitab/Combine into one PDF"))
            );
            assert!(d.contains("X-KDE-Submenu=Kitab"));
        }

        #[test]
        fn thunar_keeps_the_users_own_actions() {
            let k = Path::new("/usr/bin/kitab");
            let mine = "<?xml version=\"1.0\"?>\n<actions>\n<action><name>Mine</name></action>\n</actions>\n";
            let at = mine.rfind("</actions>").unwrap();
            let with = format!("{}{}{}", &mine[..at], thunar_actions(k), &mine[at..]);
            assert!(with.contains("kitab-merge") && with.contains("Mine"));
            assert_eq!(without_thunar_block(&with), mine);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_one_kitab_takes() {
        for e in ENTRIES {
            assert!(
                crate::instance::Tool::from_word(e.word).is_some(),
                "{}",
                e.word
            );
        }
        assert!(all_exts().contains(&"pdf") && all_exts().contains(&"webp"));
    }

    /// Writes the Quick Actions' plists to KITAB_DUMP_MAC (a folder) when
    /// set, for checking with a plist parser off macOS.
    #[test]
    fn mac_quick_actions_are_plists() {
        for e in &ENTRIES {
            let (info, wflow) = (mac_info_plist(e), mac_document_wflow(e));
            assert!(info.contains(&format!("Kitab: {}", e.title)));
            assert!(wflow.contains(&format!("kitab://tool/{}?list=", e.word)));
            if let Some(dir) = std::env::var_os("KITAB_DUMP_MAC") {
                let dir = std::path::PathBuf::from(dir);
                let _ = std::fs::create_dir_all(&dir);
                std::fs::write(dir.join(format!("{}.Info.plist", e.word)), &info).unwrap();
                std::fs::write(dir.join(format!("{}.document.wflow", e.word)), &wflow).unwrap();
            }
        }
    }

    #[test]
    fn the_mac_script_hands_the_files_over_as_a_list() {
        let s = mac_script("merge");
        assert!(s.contains("kitab://tool/merge?list="));
        assert!(s.contains("for f in \"$@\""));
    }
}
