// SPDX-License-Identifier: AGPL-3.0-or-later

//! End to end through the Windows shell: register the right-click tools the
//! way Kitab does (`kitab-os::context_menu`), ask the shell for the classic
//! context menu of real PDFs (IContextMenu, as Explorer builds it, without
//! showing it), read its items, then remove every key again.
//!
//! Writes to HKCU (the "Kitab" entry on PDFs, for the few seconds the test
//! runs), so it runs by hand after building the DLL and kitab.exe:
//! `cargo build -p kitab -p kitab-shell`
//! `cargo test -p kitab-shell --test menu -- --ignored`

#![cfg(windows)]
// Tests may unwrap (AGENTS.md rule 9).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    BHID_SFUIObject, CMF_NORMAL, IContextMenu, IContextMenu3, ILFree, IShellItem, IShellItemArray,
    SHCreateItemFromParsingName, SHCreateShellItemArrayFromIDLists, SHGetIDListFromObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, GetMenuItemCount, GetMenuStringW, GetSubMenu, HMENU,
    MF_BYPOSITION, WM_INITMENUPOPUP,
};
use windows::core::{HSTRING, Interface};

fn root() -> PathBuf {
    // A plain absolute path: the shell won't parse one with `..` in it.
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .unwrap_or(crate_dir)
}

/// Removes the keys however the test ends.
struct Cleanup;
impl Drop for Cleanup {
    fn drop(&mut self) {
        kitab_os::context_menu::uninstall();
    }
}

/// Every item's text in `menu`, a submenu's as "Parent > Child". A
/// submenu the shell fills only as it opens is told it's opening first.
fn items(cm: &IContextMenu3, menu: HMENU, prefix: &str, out: &mut Vec<String>) {
    // SAFETY: walks a live menu we created; buffers outlive each call.
    unsafe {
        for i in 0..GetMenuItemCount(Some(menu)).max(0) {
            let mut buf = [0u16; 256];
            let n = GetMenuStringW(menu, i as u32, Some(&mut buf), MF_BYPOSITION);
            let text = String::from_utf16_lossy(&buf[..n.max(0) as usize]).replace('&', "");
            if text.is_empty() {
                continue;
            }
            let full = format!("{prefix}{text}");
            out.push(full.clone());
            let sub = GetSubMenu(menu, i);
            if !sub.is_invalid() {
                let _ = cm.HandleMenuMsg2(
                    WM_INITMENUPOPUP,
                    WPARAM(sub.0 as usize),
                    LPARAM(i as isize),
                    None,
                );
                items(cm, sub, &format!("{full} > "), out);
            }
        }
    }
}

/// The classic right-click menu for `files`, as text.
fn menu_for(files: &[PathBuf]) -> Vec<String> {
    // SAFETY: shell calls on items made here; ID lists freed after use.
    unsafe {
        let mut pidls: Vec<*const ITEMIDLIST> = Vec::new();
        for f in files {
            let item: IShellItem =
                SHCreateItemFromParsingName(&HSTRING::from(f.as_os_str()), None).unwrap();
            pidls.push(SHGetIDListFromObject(&item).unwrap());
        }
        let array: IShellItemArray = SHCreateShellItemArrayFromIDLists(&pidls).unwrap();
        for p in pidls {
            ILFree(Some(p));
        }
        let cm: IContextMenu = array.BindToHandler(None, &BHID_SFUIObject).unwrap();
        let menu = CreatePopupMenu().unwrap();
        cm.QueryContextMenu(menu, 0, 1, 0x7FFF, CMF_NORMAL).unwrap();
        let mut out = Vec::new();
        let cm3: IContextMenu3 = cm.cast().unwrap();
        items(&cm3, menu, "", &mut out);
        let _ = DestroyMenu(menu);
        out
    }
}

#[test]
#[ignore = "writes to HKCU; run by hand after building kitab and kitab-shell"]
fn explorer_offers_the_tools_that_fit() {
    let exe = root().join("target").join("debug").join("kitab.exe");
    assert!(exe.exists(), "build kitab first");
    assert!(
        exe.with_file_name("kitab_shell.dll").exists(),
        "build kitab-shell first"
    );
    // SAFETY: plain COM start-up on this thread.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let _cleanup = Cleanup;
    kitab_os::context_menu::install_for(&exe).unwrap();

    let pdf = |n: &str| root().join("tests").join("fixtures").join("pdf").join(n);
    let one = menu_for(&[pdf("01-text-outline.pdf")]);
    let two = menu_for(&[pdf("01-text-outline.pdf"), pdf("02-mixed-page-sizes.pdf")]);
    let kitab = |m: &[String]| -> Vec<String> {
        m.iter()
            .filter(|t| t.starts_with("Kitab"))
            .cloned()
            .collect()
    };
    eprintln!("one PDF: {:?}\ntwo PDFs: {:?}", kitab(&one), kitab(&two));
    assert!(
        one.iter().any(|t| t == "Kitab"),
        "no Kitab entry for one PDF: {one:?}"
    );
    assert!(one.iter().any(|t| t == "Kitab > Make a smaller copy"));
    assert!(one.iter().any(|t| t == "Kitab > Export as Markdown"));
    assert!(!one.iter().any(|t| t == "Kitab > Combine into one PDF"));
    assert!(
        two.iter().any(|t| t == "Kitab > Combine into one PDF"),
        "{two:?}"
    );
    assert!(!two.iter().any(|t| t == "Kitab > Make a smaller copy"));
}
