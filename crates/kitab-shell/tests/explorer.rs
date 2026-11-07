// SPDX-License-Identifier: AGPL-3.0-or-later

//! End to end through the Windows shell: register kitab_shell.dll for a
//! test-only extension under HKCU, ask the shell for thumbnails with
//! IShellItemImageFactory (Explorer's own path, out of process in its
//! thumbnail host), then remove every key again.
//!
//! Writes to HKCU, so it runs by hand after a release build:
//! `cargo build --release -p kitab -p kitab-shell`
//! `cargo test -p kitab-shell --test explorer -- --ignored`

#![cfg(windows)]

use std::path::PathBuf;
use std::process::Command;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{BITMAP, DeleteObject, GetObjectW};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY,
};
use windows::core::HSTRING;

const EXT: &str = ".kitabthumbtest";
const PROG_ID: &str = "Kitab.ThumbTest";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn reg(args: &[&str]) -> bool {
    Command::new("reg")
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Removes the test keys however the test ends.
struct Cleanup;
impl Drop for Cleanup {
    fn drop(&mut self) {
        kitab_os::assoc::unregister_thumbnails(&[PROG_ID]);
        reg(&["delete", &format!(r"HKCU\Software\Classes\{EXT}"), "/f"]);
        reg(&["delete", &format!(r"HKCU\Software\Classes\{PROG_ID}"), "/f"]);
    }
}

/// (width, height, pixels vary) of the shell's thumbnail for `path`.
fn shell_thumbnail(path: &std::path::Path) -> windows::core::Result<(i32, i32, bool)> {
    // SAFETY: parses an existing file path into a shell item.
    let item: IShellItemImageFactory =
        unsafe { SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None) }?;
    // SAFETY: asks for a thumbnail (never an icon) up to 256 px.
    let hbmp = unsafe { item.GetImage(SIZE { cx: 256, cy: 256 }, SIIGBF_THUMBNAILONLY) }?;
    let mut bm = BITMAP::default();
    // SAFETY: `hbmp` is the bitmap the shell returned; `bm` is its size.
    unsafe {
        GetObjectW(
            hbmp.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut BITMAP as *mut std::ffi::c_void),
        )
    };
    let varies = if bm.bmBits.is_null() {
        true
    } else {
        let len = (bm.bmWidthBytes * bm.bmHeight) as usize;
        // SAFETY: a DIB section's bits are bmWidthBytes × bmHeight bytes.
        let bits = unsafe { std::slice::from_raw_parts(bm.bmBits as *const u8, len) };
        bits.chunks_exact(4).any(|p| p[..3] != bits[..3])
    };
    // SAFETY: the caller owns the returned bitmap.
    let _ = unsafe { DeleteObject(hbmp.into()) };
    Ok((bm.bmWidth, bm.bmHeight, varies))
}

#[test]
#[ignore = "writes to HKCU; run by hand after a release build"]
fn explorer_draws_kitab_thumbnails() {
    let dll = root().join("target/release/kitab_shell.dll");
    assert!(
        dll.exists(),
        "build first: cargo build --release -p kitab -p kitab-shell"
    );
    assert!(root().join("target/release/kitab.exe").exists());
    let _cleanup = Cleanup;
    assert!(reg(&[
        "add",
        &format!(r"HKCU\Software\Classes\{EXT}"),
        "/ve",
        "/d",
        PROG_ID,
        "/f"
    ]));
    kitab_os::assoc::register_thumbnails(&dll, &[PROG_ID]).expect("register");

    // SAFETY: this test thread becomes a single-threaded apartment, as Explorer's are.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    // Fresh files every run, so the thumbnail cache can't answer for us.
    let dir = std::env::temp_dir().join(format!("kitab-explorer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let fixtures = root().join("tests/fixtures");
    let cases = [
        ("pdf/01-text-outline.pdf", "a"),
        ("md/01-gfm-kitchen-sink.md", "b"),
    ];
    for (src, stem) in cases {
        let path = dir.join(format!("{stem}{EXT}"));
        std::fs::copy(fixtures.join(src), &path).expect("copy fixture");
        let (w, h, varies) = shell_thumbnail(&path).unwrap_or_else(|e| panic!("{src}: {e}"));
        assert_eq!(w.max(h), 256, "{src}: {w}×{h}");
        assert!(varies, "{src}: the thumbnail has content");
    }
    // A file Kitab can't draw gets no thumbnail (the shell shows its icon).
    // (Under the test extension a text file is sniffed as Markdown and drawn,
    // so the locked PDF is the case here.)
    let bad = dir.join(format!("c{EXT}"));
    std::fs::copy(fixtures.join("pdf/04a-encrypted-password-kitab.pdf"), &bad).expect("copy");
    assert!(shell_thumbnail(&bad).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
