// SPDX-License-Identifier: AGPL-3.0-or-later

//! The first frame (ADR 0002 step 3, the SumatraPDF approach): a plain Win32
//! window painted with GDI on its own thread, showing the window's skeleton and
//! page 1 where the GPU window will draw them, while GPUI sets up its device and
//! fonts. `close()` removes it once GPUI has painted. Measured: page on screen
//! about 20 ms after start (docs/perf/baseline.md).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, CreateSolidBrush, DIB_RGB_COLORS,
    DeleteObject, EndPaint, FillRect, PAINTSTRUCT, SetDIBitsToDevice, UpdateWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, GetMessageW, GetSystemMetrics, IDC_ARROW, LoadCursorW, MSG, PostMessageW,
    PostQuitMessage, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNORMAL, ShowWindow,
    TranslateMessage, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_PAINT, WNDCLASSW,
    WS_OVERLAPPEDWINDOW,
};
use windows::core::w;

/// A rendered page: BGRA, top-down, composited on white.
pub struct Page {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// A filled rectangle in logical pixels of the client area, colour as (r, g, b).
#[derive(Clone, Copy)]
pub struct Fill {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rgb: (u8, u8, u8),
}

/// What the first frame shows, in logical pixels: the window's client size,
/// the skeleton fills (back to front), and where page 1 goes.
pub struct Frame {
    pub client_w: f32,
    pub client_h: f32,
    pub fills: Vec<Fill>,
    pub page: Page,
    pub page_x: f32,
    pub page_y: f32,
    pub dark_title_bar: bool,
}

struct Paint {
    fills: Vec<(RECT, u32)>,
    page: Page,
    page_x: i32,
    page_y: i32,
}

static PAINT: OnceLock<Paint> = OnceLock::new();
static HWND_RAW: AtomicIsize = AtomicIsize::new(0);

/// Must run before any window exists in the process. GPUI asks for the same mode.
pub fn set_dpi_aware() {
    // SAFETY: a plain Win32 call with a constant; failure (already set) is harmless.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// Device pixels per logical pixel on the primary monitor.
pub fn scale() -> f32 {
    // SAFETY: no arguments; returns the system DPI.
    let dpi = unsafe { GetDpiForSystem() };
    dpi as f32 / 96.0
}

/// Show the first frame on its own thread. `build` runs on that thread (it opens
/// and renders the document) and gets the device scale.
pub fn show(build: impl FnOnce(f32) -> Option<Frame> + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name("kitab-splash".into())
        .spawn(move || {
            let scale = scale();
            let Some(frame) = build(scale) else { return };
            let s = |v: f32| (v * scale).round() as i32;
            let colorref =
                |(r, g, b): (u8, u8, u8)| u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16);
            let fills = frame
                .fills
                .iter()
                .map(|f| {
                    (
                        RECT {
                            left: s(f.x),
                            top: s(f.y),
                            right: s(f.x + f.w),
                            bottom: s(f.y + f.h),
                        },
                        colorref(f.rgb),
                    )
                })
                .collect();
            let (cw, ch, dark) = (s(frame.client_w), s(frame.client_h), frame.dark_title_bar);
            let _ = PAINT.set(Paint {
                fills,
                page: frame.page,
                page_x: s(frame.page_x),
                page_y: s(frame.page_y),
            });
            // SAFETY: standard window creation and message loop on this thread; every
            // pointer passed lives for the duration of its call.
            unsafe { run(cw, ch, scale, dark) };
        });
}

/// Close the first frame (from any thread). Safe to call when there is none.
pub fn close() {
    let raw = HWND_RAW.swap(0, Ordering::SeqCst);
    if raw != 0 {
        // SAFETY: posting to a window handle is allowed from any thread; if the
        // window is already gone the call fails harmlessly.
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe fn run(client_w: i32, client_h: i32, scale: f32, dark: bool) {
    // SAFETY (whole function): Win32 calls on this thread with valid arguments.
    unsafe {
        let Ok(instance) = GetModuleHandleW(None) else {
            return;
        };
        let class = w!("KitabFirstFrame");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return;
        }
        // Same place as GPUI's Bounds::centered on the primary display.
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let mut rect = RECT {
            left: (sw - client_w) / 2,
            top: (sh - client_h) / 2,
            right: (sw + client_w) / 2,
            bottom: (sh + client_h) / 2,
        };
        let _ = AdjustWindowRectExForDpi(
            &mut rect,
            WS_OVERLAPPEDWINDOW,
            false,
            WINDOW_EX_STYLE(0),
            (scale * 96.0) as u32,
        );
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("Kitab"),
            WS_OVERLAPPEDWINDOW,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(instance.into()),
            None,
        ) else {
            return;
        };
        if dark {
            let on: i32 = 1;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&on as *const i32).cast(),
                4,
            );
        }
        HWND_RAW.store(hwnd.0 as isize, Ordering::SeqCst);
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = UpdateWindow(hwnd);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY (whole function): called by the system on this window's thread with
    // valid arguments; GDI calls use the DC BeginPaint returns.
    unsafe {
        match msg {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                if let Some(p) = PAINT.get() {
                    let mut client = RECT::default();
                    let _ = GetClientRect(hwnd, &mut client);
                    for (i, (r, color)) in p.fills.iter().enumerate() {
                        let brush = CreateSolidBrush(COLORREF(*color));
                        // The first fill is the window background: stretch it to the real client area.
                        FillRect(hdc, if i == 0 { &client } else { r }, brush);
                        let _ = DeleteObject(brush.into());
                    }
                    let (w, h) = (p.page.width as i32, p.page.height as i32);
                    let info = BITMAPINFO {
                        bmiHeader: BITMAPINFOHEADER {
                            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                            biWidth: w,
                            biHeight: -h, // top-down
                            biPlanes: 1,
                            biBitCount: 32,
                            biCompression: BI_RGB.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    SetDIBitsToDevice(
                        hdc,
                        p.page_x,
                        p.page_y,
                        w as u32,
                        h as u32,
                        0,
                        0,
                        0,
                        h as u32,
                        p.page.bgra.as_ptr().cast(),
                        &info,
                        DIB_RGB_COLORS,
                    );
                }
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
