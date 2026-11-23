// SPDX-License-Identifier: AGPL-3.0-or-later

//! Kitab's tools in Explorer's right-click menu: a "Kitab" entry whose
//! submenu offers what fits the selection (Combine needs two or more PDFs,
//! Make a PDF needs pictures, Make a smaller copy and Export take one file).
//! Picking one starts `kitab.exe --tool WORD FILE...` (the kitab.exe next
//! to this DLL), which opens that tool in Kitab with the files filled in.
//!
//! One `IExplorerCommand` class serves both menus: Windows 11's own (the
//! Store build declares it in its package manifest) and the classic one
//! (`kitab-os::assoc` puts it under the file types' shell verbs). It holds
//! no document code and only starts kitab.exe.

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::Foundation::{
    CLASS_E_NOAGGREGATION, E_FAIL, E_NOTIMPL, E_POINTER, S_FALSE, S_OK,
};
use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx, IClassFactory, IClassFactory_Impl};
use windows::Win32::UI::Shell::{
    ECF_DEFAULT, ECF_HASSUBCOMMANDS, ECS_ENABLED, ECS_HIDDEN, IEnumExplorerCommand,
    IEnumExplorerCommand_Impl, IExplorerCommand, IExplorerCommand_Impl, IShellItemArray, SHStrDupW,
    SIGDN_FILESYSPATH,
};
use windows::core::{BOOL, GUID, HRESULT, HSTRING, IUnknown, Interface, PWSTR, Ref, implement};

/// The menu handler's CLSID (registered by `kitab-os::assoc` and the
/// Store build's manifest).
pub const CLSID_KITAB_MENU: GUID = GUID::from_u128(0x5b2c9e41_7d3a_4f6e_9a1b_c8e2f0d4a713);

/// A tool the menu offers, as `kitab --tool` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuTool {
    Merge,
    Pictures,
    DeletePages,
    MovePages,
    RotatePages,
    Compress,
    Export,
    ExportText,
}

impl MenuTool {
    pub const ALL: [MenuTool; 8] = [
        MenuTool::Merge,
        MenuTool::Pictures,
        MenuTool::DeletePages,
        MenuTool::MovePages,
        MenuTool::RotatePages,
        MenuTool::Compress,
        MenuTool::Export,
        MenuTool::ExportText,
    ];

    /// The word `kitab --tool` takes (kitab-os's `instance::Tool`).
    pub fn word(self) -> &'static str {
        match self {
            MenuTool::Merge => "merge",
            MenuTool::Pictures => "pictures",
            MenuTool::DeletePages => "delete-pages",
            MenuTool::MovePages => "move-pages",
            MenuTool::RotatePages => "rotate-pages",
            MenuTool::Compress => "compress",
            MenuTool::Export => "export",
            MenuTool::ExportText => "export-text",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            MenuTool::Merge => "Combine into one PDF",
            MenuTool::Pictures => "Make a PDF from pictures",
            MenuTool::DeletePages => "Remove pages",
            MenuTool::MovePages => "Reorder pages",
            MenuTool::RotatePages => "Rotate pages",
            MenuTool::Compress => "Make a smaller copy",
            MenuTool::Export => "Export as Markdown",
            MenuTool::ExportText => "Export as text",
        }
    }

    fn id(self) -> GUID {
        // Stable per tool, so Explorer can tell them apart.
        GUID::from_u128(0x5b2c9e41_7d3a_4f6e_9a1b_c8e2f0d4a700 + self as u128 + 1)
    }
}

fn ext_of(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_pdf(p: &Path) -> bool {
    ext_of(p) == "pdf"
}

/// Pictures Kitab can put in a PDF (as Make a PDF from pictures takes them).
fn is_picture(p: &Path) -> bool {
    matches!(
        ext_of(p).as_str(),
        "jpg" | "jpeg" | "jfif" | "png" | "webp" | "tif" | "tiff" | "bmp" | "gif"
    )
}

/// Documents Kitab exports as Markdown or text.
fn is_document(p: &Path) -> bool {
    matches!(
        ext_of(p).as_str(),
        "pdf" | "epub" | "mobi" | "azw" | "azw3" | "fb2" | "xps" | "oxps"
    )
}

/// The tools that fit `paths`, in the menu's order.
pub fn tools_for(paths: &[PathBuf]) -> Vec<MenuTool> {
    let one = paths.len() == 1;
    MenuTool::ALL
        .into_iter()
        .filter(|t| match t {
            MenuTool::Merge => paths.len() >= 2 && paths.iter().all(|p| is_pdf(p)),
            MenuTool::Pictures => !paths.is_empty() && paths.iter().all(|p| is_picture(p)),
            MenuTool::DeletePages
            | MenuTool::MovePages
            | MenuTool::RotatePages
            | MenuTool::Compress => one && is_pdf(&paths[0]),
            MenuTool::Export | MenuTool::ExportText => one && is_document(&paths[0]),
        })
        .collect()
}

/// The selection's files on disk (a search result or a zip's inside has
/// none, and gets no menu).
fn paths_of(items: &IShellItemArray) -> Vec<PathBuf> {
    let mut out = Vec::new();
    // SAFETY: plain COM calls on a live array; each name Explorer allocates
    // is freed once copied.
    unsafe {
        let Ok(n) = items.GetCount() else {
            return out;
        };
        // A selection of thousands: the menu needn't look at them all.
        for i in 0..n.min(2000) {
            let Ok(item) = items.GetItemAt(i) else {
                continue;
            };
            let Ok(name) = item.GetDisplayName(SIGDN_FILESYSPATH) else {
                return Vec::new();
            };
            if let Ok(s) = name.to_string() {
                out.push(PathBuf::from(s));
            }
            CoTaskMemFree(Some(name.0 as *const c_void));
        }
    }
    out
}

/// `text` as a string Explorer frees (CoTaskMemAlloc).
fn shell_string(text: &str) -> windows::core::Result<PWSTR> {
    // SAFETY: copies a valid, NUL-terminated wide string.
    unsafe { SHStrDupW(&HSTRING::from(text)) }
}

/// kitab.exe's own icon, for the menu.
fn icon() -> windows::core::Result<PWSTR> {
    let exe = crate::provider::kitab_exe().map_err(|_| windows::core::Error::from(E_FAIL))?;
    shell_string(&format!("{},0", exe.display()))
}

/// Start `kitab.exe --tool WORD FILES`; Kitab takes it from there (in the
/// running window, if there is one).
fn launch(tool: MenuTool, paths: &[PathBuf]) -> Result<(), String> {
    let exe = crate::provider::kitab_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg("--tool").arg(tool.word());
    // Windows caps a command line at 32,767 characters: as many files as fit.
    let mut len = 64usize;
    for p in paths {
        len += p.as_os_str().len() + 3;
        if len > 30_000 {
            break;
        }
        cmd.arg(p);
    }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// No panic crosses into Explorer (AGENTS.md rule 10).
fn guarded<T>(f: impl FnOnce() -> windows::core::Result<T>) -> windows::core::Result<T> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Err(E_FAIL.into()))
}

/// The "Kitab" entry: a submenu of the tools that fit the selection.
#[implement(IExplorerCommand)]
pub struct KitabMenu;

impl IExplorerCommand_Impl for KitabMenu_Impl {
    fn GetTitle(&self, _: Ref<'_, IShellItemArray>) -> windows::core::Result<PWSTR> {
        guarded(|| shell_string("Kitab"))
    }

    fn GetIcon(&self, _: Ref<'_, IShellItemArray>) -> windows::core::Result<PWSTR> {
        guarded(icon)
    }

    fn GetToolTip(&self, _: Ref<'_, IShellItemArray>) -> windows::core::Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> windows::core::Result<GUID> {
        Ok(CLSID_KITAB_MENU)
    }

    fn GetState(&self, items: Ref<'_, IShellItemArray>, _: BOOL) -> windows::core::Result<u32> {
        guarded(|| {
            let paths = items.as_ref().map(paths_of).unwrap_or_default();
            Ok(if tools_for(&paths).is_empty() {
                ECS_HIDDEN.0 as u32
            } else {
                ECS_ENABLED.0 as u32
            })
        })
    }

    fn Invoke(
        &self,
        _: Ref<'_, IShellItemArray>,
        _: Ref<'_, IBindCtx>,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn GetFlags(&self) -> windows::core::Result<u32> {
        Ok(ECF_HASSUBCOMMANDS.0 as u32)
    }

    fn EnumSubCommands(&self) -> windows::core::Result<IEnumExplorerCommand> {
        Ok(Tools {
            next: AtomicUsize::new(0),
        }
        .into())
    }
}

/// One tool in the submenu.
#[implement(IExplorerCommand)]
struct ToolCommand(MenuTool);

impl IExplorerCommand_Impl for ToolCommand_Impl {
    fn GetTitle(&self, _: Ref<'_, IShellItemArray>) -> windows::core::Result<PWSTR> {
        guarded(|| shell_string(self.0.title()))
    }

    fn GetIcon(&self, _: Ref<'_, IShellItemArray>) -> windows::core::Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetToolTip(&self, _: Ref<'_, IShellItemArray>) -> windows::core::Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> windows::core::Result<GUID> {
        Ok(self.0.id())
    }

    fn GetState(&self, items: Ref<'_, IShellItemArray>, _: BOOL) -> windows::core::Result<u32> {
        guarded(|| {
            let paths = items.as_ref().map(paths_of).unwrap_or_default();
            Ok(if tools_for(&paths).contains(&self.0) {
                ECS_ENABLED.0 as u32
            } else {
                ECS_HIDDEN.0 as u32
            })
        })
    }

    fn Invoke(
        &self,
        items: Ref<'_, IShellItemArray>,
        _: Ref<'_, IBindCtx>,
    ) -> windows::core::Result<()> {
        guarded(|| {
            let paths = items.as_ref().map(paths_of).unwrap_or_default();
            launch(self.0, &paths).map_err(|_| E_FAIL.into())
        })
    }

    fn GetFlags(&self) -> windows::core::Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }

    fn EnumSubCommands(&self) -> windows::core::Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

/// The submenu's tools, one after another.
#[implement(IEnumExplorerCommand)]
struct Tools {
    next: AtomicUsize,
}

impl IEnumExplorerCommand_Impl for Tools_Impl {
    // The windows crate fixes this signature (a safe fn taking COM's out
    // pointers); they are checked for null before use.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn Next(&self, celt: u32, out: *mut Option<IExplorerCommand>, fetched: *mut u32) -> HRESULT {
        if out.is_null() {
            return E_POINTER;
        }
        let mut n = 0u32;
        while n < celt {
            let i = self.next.fetch_add(1, Ordering::Relaxed);
            let Some(tool) = MenuTool::ALL.get(i).copied() else {
                break;
            };
            let cmd: IExplorerCommand = ToolCommand(tool).into();
            // SAFETY: `out` holds `celt` slots (COM's contract); n < celt.
            unsafe { out.add(n as usize).write(Some(cmd)) };
            n += 1;
        }
        if !fetched.is_null() {
            // SAFETY: checked non-null.
            unsafe { *fetched = n };
        }
        if n == celt { S_OK } else { S_FALSE }
    }

    fn Skip(&self, celt: u32) -> windows::core::Result<()> {
        self.next.fetch_add(celt as usize, Ordering::Relaxed);
        Ok(())
    }

    fn Reset(&self) -> windows::core::Result<()> {
        self.next.store(0, Ordering::Relaxed);
        Ok(())
    }

    fn Clone(&self) -> windows::core::Result<IEnumExplorerCommand> {
        Ok(Tools {
            next: AtomicUsize::new(self.next.load(Ordering::Relaxed)),
        }
        .into())
    }
}

#[implement(IClassFactory)]
pub(crate) struct MenuFactory;

impl IClassFactory_Impl for MenuFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<'_, IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if ppvobject.is_null() {
            return Err(E_POINTER.into());
        }
        // SAFETY: checked non-null above.
        unsafe { *ppvobject = std::ptr::null_mut() };
        if !punkouter.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let unknown: IUnknown = KitabMenu.into();
        // SAFETY: COM QueryInterface with the caller's iid and out pointer.
        unsafe { unknown.query(riid, ppvobject) }.ok()
    }

    fn LockServer(&self, _flock: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(names: &[&str]) -> Vec<PathBuf> {
        names
            .iter()
            .map(|n| PathBuf::from(format!(r"C:\docs\{n}")))
            .collect()
    }

    #[test]
    fn the_menu_offers_what_fits_the_selection() {
        use MenuTool::*;
        assert_eq!(tools_for(&p(&["a.pdf", "b.PDF"])), [Merge]);
        assert_eq!(
            tools_for(&p(&["a.pdf"])),
            [
                DeletePages,
                MovePages,
                RotatePages,
                Compress,
                Export,
                ExportText
            ]
        );
        assert_eq!(tools_for(&p(&["1.jpg", "2.png", "3.WEBP"])), [Pictures]);
        assert_eq!(tools_for(&p(&["book.epub"])), [Export, ExportText]);
        // Mixed, or nothing Kitab handles: no menu.
        assert!(tools_for(&p(&["a.pdf", "b.jpg"])).is_empty());
        assert!(tools_for(&p(&["notes.docx"])).is_empty());
        assert!(tools_for(&[]).is_empty());
    }

    #[test]
    fn the_words_are_the_ones_kitab_takes() {
        for t in MenuTool::ALL {
            assert!(
                kitab_os::instance::Tool::from_word(t.word()).is_some(),
                "{} isn't a kitab --tool",
                t.word()
            );
        }
    }

    #[test]
    fn the_submenu_lists_every_tool_once() {
        let root: IExplorerCommand = KitabMenu.into();
        // SAFETY: COM calls on our own objects.
        unsafe {
            assert_eq!(root.GetFlags().unwrap(), ECF_HASSUBCOMMANDS.0 as u32);
            let tools = root.EnumSubCommands().unwrap();
            let mut seen = Vec::new();
            loop {
                let mut slot = [None];
                let mut got = 0;
                let hr = tools.Next(&mut slot, Some(&mut got));
                if got == 0 {
                    assert_eq!(hr, S_FALSE);
                    break;
                }
                let cmd = slot[0].take().unwrap();
                let title = cmd.GetTitle(None).unwrap();
                seen.push(title.to_string().unwrap());
                CoTaskMemFree(Some(title.0 as *const c_void));
            }
            assert_eq!(seen.len(), MenuTool::ALL.len());
            assert_eq!(seen[0], "Combine into one PDF");
        }
    }
}
