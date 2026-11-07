// SPDX-License-Identifier: AGPL-3.0-or-later

//! The provider called the way Explorer calls it (class factory → object →
//! IInitializeWithStream → IThumbnailProvider), on the fixtures, with the
//! kitab.exe built in target/. Build it first: `cargo build -p kitab`.

use super::*;
use std::path::Path;
use windows::Win32::Graphics::Gdi::{BITMAP, GetObjectW};
use windows::Win32::UI::Shell::SHCreateStreamOnFileEx;
use windows::core::HSTRING;

const STGM_READ: u32 = 0;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn use_built_kitab() {
    let exe = ["release", "debug"]
        .iter()
        .map(|p| root().join("target").join(p).join("kitab.exe"))
        .find(|p| p.exists())
        .expect("kitab.exe is built (cargo build -p kitab) before these tests");
    KITAB_EXE.with(|k| *k.borrow_mut() = Some(exe));
}

fn provider() -> IUnknown {
    let mut factory: *mut c_void = std::ptr::null_mut();
    // SAFETY: valid pointers to locals, as COM would pass.
    let hr =
        unsafe { DllGetClassObject(&CLSID_KITAB_THUMBNAIL, &IClassFactory::IID, &mut factory) };
    assert!(hr.is_ok(), "{hr:?}");
    // SAFETY: DllGetClassObject returned an IClassFactory with one reference.
    let factory = unsafe { IClassFactory::from_raw(factory) };
    // SAFETY: CreateInstance with no outer object, asking for IUnknown.
    unsafe { factory.CreateInstance::<Option<&IUnknown>, IUnknown>(None) }.expect("instance")
}

/// Width, height and whether the pixels vary.
fn thumbnail_of(path: &Path, size: u32) -> windows::core::Result<(i32, i32, bool)> {
    let object = provider();
    let init: IInitializeWithStream = object.cast()?;
    // SAFETY: opens a read-only stream on a fixture file.
    let stream = unsafe {
        SHCreateStreamOnFileEx(&HSTRING::from(path.as_os_str()), STGM_READ, 0, false, None)
    }?;
    // SAFETY: a valid stream, as Explorer would pass.
    unsafe { init.Initialize(&stream, STGM_READ) }?;
    let thumbs: IThumbnailProvider = object.cast()?;
    let mut hbmp = HBITMAP::default();
    let mut alpha = WTS_ALPHATYPE::default();
    // SAFETY: valid out pointers.
    unsafe { thumbs.GetThumbnail(size, &mut hbmp, &mut alpha) }?;
    let mut bm = BITMAP::default();
    // SAFETY: `hbmp` is the DIB section the provider made; `bm` is its size.
    let n = unsafe {
        GetObjectW(
            hbmp.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut BITMAP as *mut c_void),
        )
    };
    assert!(n > 0 && !bm.bmBits.is_null());
    let len = (bm.bmWidthBytes * bm.bmHeight) as usize;
    // SAFETY: a DIB section's bits are bmWidthBytes × bmHeight bytes.
    let bits = unsafe { std::slice::from_raw_parts(bm.bmBits as *const u8, len) };
    let varies = bits.chunks_exact(4).any(|p| p != &bits[..4]);
    assert_eq!(alpha, WTSAT_RGB);
    // SAFETY: the caller owns the bitmap Explorer receives.
    let _ = unsafe { DeleteObject(hbmp.into()) };
    Ok((bm.bmWidth, bm.bmHeight, varies))
}

fn fixture(rel: &str) -> PathBuf {
    root().join("tests/fixtures").join(rel)
}

#[test]
fn an_unknown_class_is_refused() {
    let mut out: *mut c_void = std::ptr::null_mut();
    let other = GUID::from_u128(1);
    // SAFETY: valid pointers to locals.
    let hr = unsafe { DllGetClassObject(&other, &IClassFactory::IID, &mut out) };
    assert_eq!(hr, CLASS_E_CLASSNOTAVAILABLE);
    assert!(out.is_null());
}

#[test]
fn a_pdf_gets_its_first_page() {
    use_built_kitab();
    let (w, h, varies) = thumbnail_of(&fixture("pdf/01-text-outline.pdf"), 256).expect("thumbnail");
    // The page size from expected.json, scaled to 256 px tall (MuPDF rounds
    // the pixel box outwards, so allow a pixel).
    let exp = std::fs::read_to_string(fixture("expected.json")).expect("expected.json");
    let key = "\"01-text-outline.pdf\"";
    let after = &exp[exp.find(key).expect("01 in expected.json")..];
    let sizes = &after[after.find("\"page_sizes_pt\"").expect("page sizes")..];
    let nums: Vec<f64> = sizes
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .filter(|s| !s.is_empty())
        .take(2)
        .filter_map(|s| s.parse().ok())
        .collect();
    let want_w = 256.0 * nums[0] / nums[1];
    assert_eq!(h, 256);
    assert!((w as f64 - want_w).abs() <= 1.5, "{w} vs {want_w}");
    assert!(varies);
}

#[test]
fn markdown_gets_a_page_preview() {
    use_built_kitab();
    let (w, h, varies) =
        thumbnail_of(&fixture("md/01-gfm-kitchen-sink.md"), 256).expect("thumbnail");
    assert_eq!((w, h), (192, 256));
    assert!(varies);
}

#[test]
fn files_kitab_cant_draw_fall_back_to_the_icon() {
    use_built_kitab();
    for f in [
        "pdf/05c-not-a-pdf.pdf",
        "pdf/04a-encrypted-password-kitab.pdf",
    ] {
        assert!(thumbnail_of(&fixture(f), 128).is_err(), "{f}");
    }
}

#[test]
fn a_missing_kitab_exe_is_an_error_not_a_hang() {
    KITAB_EXE.with(|k| *k.borrow_mut() = Some(PathBuf::from(r"C:\nowhere\kitab.exe")));
    assert!(thumbnail_of(&fixture("pdf/01-text-outline.pdf"), 128).is_err());
}
