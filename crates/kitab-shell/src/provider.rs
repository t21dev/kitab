// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cell::RefCell;
use std::ffi::c_void;
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_POINTER, HMODULE, S_FALSE,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, DeleteObject, HBITMAP,
};
use windows::Win32::System::Com::{
    CoTaskMemFree, IClassFactory, IClassFactory_Impl, IStream, STATFLAG_DEFAULT, STATSTG,
};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleFileNameW, GetModuleHandleExW,
};
use windows::Win32::UI::Shell::PropertiesSystem::{
    IInitializeWithStream, IInitializeWithStream_Impl,
};
use windows::Win32::UI::Shell::{
    IThumbnailProvider, IThumbnailProvider_Impl, WTS_ALPHATYPE, WTSAT_RGB,
};
use windows::core::{BOOL, GUID, HRESULT, IUnknown, Interface, PCWSTR, Ref, implement};

/// The provider's CLSID (registered by `kitab-os::assoc`).
pub const CLSID_KITAB_THUMBNAIL: GUID = GUID::from_u128(0x7080945f_c214_4a8a_b62b_fc5e2e3d047e);
/// How long kitab.exe may take before the thumbnail is given up.
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// The most bytes read from Explorer's stream (as `kitab-thumb::MAX_BYTES`).
const MAX_BYTES: usize = 1 << 30;
const MAX_SIDE: u32 = 1024;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(test)]
thread_local! {
    /// Tests point at a kitab.exe in target/ instead of one next to the DLL.
    pub(crate) static KITAB_EXE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// kitab.exe in the same folder as this DLL.
fn kitab_exe() -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(p) = KITAB_EXE.with(|k| k.borrow().clone()) {
        return Ok(p);
    }
    let mut module = HMODULE::default();
    let anchor = kitab_exe as *const () as *const u16;
    // SAFETY: asks which module contains this function's address; the flag
    // leaves the module's reference count alone, and `module` is a valid out
    // pointer.
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(anchor),
            &mut module,
        )
    }
    .map_err(|e| e.to_string())?;
    let mut buf = vec![0u16; 32_768];
    // SAFETY: `buf` is a writable buffer whose length is passed with it.
    let n = unsafe { GetModuleFileNameW(Some(module), &mut buf) } as usize;
    if n == 0 || n >= buf.len() {
        return Err("could not find the DLL's folder".into());
    }
    let dll = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    Ok(dll.with_file_name("kitab.exe"))
}

/// The extension Explorer's stream names, or a guess from the first bytes.
fn extension(stream: &IStream, head: &[u8]) -> String {
    let mut stat = STATSTG::default();
    // SAFETY: `stat` is a valid out pointer; STATFLAG_DEFAULT asks for the
    // name, which is CoTaskMemAlloc'd and freed right after reading it.
    let name = unsafe {
        if stream.Stat(&mut stat, STATFLAG_DEFAULT).is_ok() && !stat.pwcsName.is_null() {
            let name = stat.pwcsName.to_string().unwrap_or_default();
            CoTaskMemFree(Some(stat.pwcsName.0 as *const c_void));
            name
        } else {
            String::new()
        }
    };
    // Formats Kitab reads; any other name (a renamed file) is sniffed.
    const KNOWN: &[&str] = &[
        "pdf", "md", "markdown", "mdown", "mkd", "txt", "epub", "mobi", "azw", "azw3", "fb2",
        "cbz", "cbr", "cb7", "djvu", "djv", "xps", "oxps",
    ];
    match name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()) {
        Some(ext) if KNOWN.contains(&ext.as_str()) => ext,
        _ if head.starts_with(b"%PDF") || head.windows(5).any(|w| w == b"%PDF-") => "pdf".into(),
        _ => "md".into(),
    }
}

fn read_all(stream: &IStream) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut chunk = vec![0u8; 1 << 20];
    loop {
        let mut read = 0u32;
        // SAFETY: `chunk` is writable for its whole length, which is passed.
        let hr = unsafe {
            stream.Read(
                chunk.as_mut_ptr() as *mut c_void,
                chunk.len() as u32,
                Some(&mut read),
            )
        };
        if hr.is_err() {
            return Err(format!("reading the file failed: {hr:?}"));
        }
        if read == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&chunk[..read as usize]);
        if out.len() > MAX_BYTES {
            return Err("the file is too large for a thumbnail".into());
        }
    }
}

/// Run kitab.exe on `bytes`; returns width, height and BGRA rows.
pub fn render_with_kitab(
    bytes: Vec<u8>,
    ext: &str,
    size: u32,
) -> Result<(u32, u32, Vec<u8>), String> {
    let exe = kitab_exe()?;
    let mut child = Command::new(&exe)
        .args([
            "--thumbnail",
            "-s",
            &size.to_string(),
            "--stdin",
            ext,
            "--bgra",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("could not start kitab.exe: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    // Write and read on their own threads: a large file fills the pipe.
    std::thread::spawn(move || {
        let _ = stdin.write_all(&bytes);
    });
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let r = stdout.read_to_end(&mut out).map(|_| out);
        let _ = tx.send(r);
    });
    let out = match rx.recv_timeout(TIMEOUT) {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            let _ = child.kill();
            return Err(e.to_string());
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("kitab.exe took too long".into());
        }
    };
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("kitab.exe could not draw this file".into());
    }
    if out.len() < 8 {
        return Err("kitab.exe returned no image".into());
    }
    let w = u32::from_le_bytes([out[0], out[1], out[2], out[3]]);
    let h = u32::from_le_bytes([out[4], out[5], out[6], out[7]]);
    let ok = (1..=MAX_SIDE).contains(&w)
        && (1..=MAX_SIDE).contains(&h)
        && out.len() == 8 + (w as usize) * (h as usize) * 4;
    if !ok {
        return Err("kitab.exe returned a malformed image".into());
    }
    Ok((w, h, out[8..].to_vec()))
}

/// A top-down 32-bit DIB section holding `bgra`.
fn bitmap(w: u32, h: u32, bgra: &[u8]) -> Result<HBITMAP, String> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    // SAFETY: `info` describes a w × h 32-bit DIB; `bits` receives its pixels.
    let hbmp = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }
        .map_err(|e| e.to_string())?;
    if bits.is_null() {
        // SAFETY: `hbmp` was just created and is not used elsewhere.
        let _ = unsafe { DeleteObject(hbmp.into()) };
        return Err("no bitmap memory".into());
    }
    // SAFETY: the DIB has exactly w*h*4 bytes, the length `bgra` was checked to have.
    unsafe { std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len()) };
    Ok(hbmp)
}

#[implement(IThumbnailProvider, IInitializeWithStream)]
#[derive(Default)]
pub struct ThumbnailProvider {
    stream: RefCell<Option<IStream>>,
}

impl IInitializeWithStream_Impl for ThumbnailProvider_Impl {
    fn Initialize(&self, pstream: Ref<'_, IStream>, _grfmode: u32) -> windows::core::Result<()> {
        let stream = pstream.ok()?.clone();
        *self.stream.borrow_mut() = Some(stream);
        Ok(())
    }
}

impl ThumbnailProvider_Impl {
    fn thumbnail(&self, cx: u32) -> Result<HBITMAP, String> {
        let stream = self.stream.borrow().clone().ok_or("not initialised")?;
        let bytes = read_all(&stream)?;
        let ext = extension(&stream, &bytes[..bytes.len().min(1024)]);
        let (w, h, bgra) = render_with_kitab(bytes, &ext, cx.clamp(16, MAX_SIDE))?;
        bitmap(w, h, &bgra)
    }
}

impl IThumbnailProvider_Impl for ThumbnailProvider_Impl {
    // The windows crate fixes this signature (a safe fn taking COM's out
    // pointers); they are checked for null before use.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn GetThumbnail(
        &self,
        cx: u32,
        phbmp: *mut HBITMAP,
        pdwalpha: *mut WTS_ALPHATYPE,
    ) -> windows::core::Result<()> {
        if phbmp.is_null() || pdwalpha.is_null() {
            return Err(E_POINTER.into());
        }
        // No panic crosses the COM boundary (AGENTS.md rule 10).
        let result = catch_unwind(AssertUnwindSafe(|| self.thumbnail(cx)));
        match result {
            Ok(Ok(hbmp)) => {
                // SAFETY: both out pointers were checked above.
                unsafe {
                    *phbmp = hbmp;
                    *pdwalpha = WTSAT_RGB;
                }
                Ok(())
            }
            // Explorer shows the file's icon instead.
            _ => Err(E_FAIL.into()),
        }
    }
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
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
        let unknown: IUnknown = ThumbnailProvider::default().into();
        // SAFETY: COM QueryInterface with the caller's iid and out pointer.
        unsafe { unknown.query(riid, ppvobject) }.ok()
    }

    fn LockServer(&self, _flock: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}

/// COM entry point: the class factory for [`CLSID_KITAB_THUMBNAIL`].
///
/// # Safety
/// Called by COM with valid pointers (checked for null anyway).
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    let result = catch_unwind(|| {
        if ppv.is_null() || rclsid.is_null() || riid.is_null() {
            return E_POINTER;
        }
        // SAFETY: pointers checked non-null above; COM owns their memory.
        unsafe {
            *ppv = std::ptr::null_mut();
            if *rclsid != CLSID_KITAB_THUMBNAIL {
                return CLASS_E_CLASSNOTAVAILABLE;
            }
            let factory: IClassFactory = Factory.into();
            factory.query(riid, ppv)
        }
    });
    result.unwrap_or(E_FAIL)
}

/// The DLL stays loaded in the thumbnail host; it holds no state worth
/// unloading for.
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}

#[cfg(test)]
mod tests;
