// SPDX-License-Identifier: AGPL-3.0-or-later

//! "Window first" (ADR 0002, step 3), the way SumatraPDF starts: a plain Win32
//! window painted with GDI shows the first page while GPUI creates its GPU
//! device and fonts. It runs on its own thread with its own message loop, so it
//! paints (and answers the system) while the main thread is busy in GPUI's set-up.
//! GPUI's window opens at the same place; when it has painted its first page,
//! `close()` removes the splash.
//!
//! Spike code: the production version belongs in `kitab-os` (AGENTS.md rule 10).

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
    GetMessageW, GetSystemMetrics, IDC_ARROW, LoadCursorW, MSG, PostMessageW, PostQuitMessage,
    RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNORMAL, ShowWindow, TranslateMessage,
    WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_PAINT, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};
use windows::core::w;

/// The page in BGRA, top-down, already composited on white.
pub struct Page {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

struct Paint {
    page: Page,
    /// Desk colour as 0x00BBGGRR (COLORREF).
    desk: u32,
    /// Gap above the page and between pages, in device pixels (GPUI's py(22) at this DPI).
    top: i32,
}

static PAINT: OnceLock<Paint> = OnceLock::new();
static HWND_RAW: AtomicIsize = AtomicIsize::new(0);

/// Must run before any window exists in the process. GPUI asks for the same mode.
pub fn set_dpi_aware() {
    // SAFETY: plain Win32 call with a constant; failure (already set) is harmless.
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

/// Show the splash on its own thread. `render` runs on that thread and returns page 1.
/// `client_logical` is GPUI's window size in logical pixels; `dark` picks a dark title bar.
pub fn show(
    render: impl FnOnce(f32) -> Option<Page> + Send + 'static,
    desk_rgb: (u8, u8, u8),
    client_logical: f32,
    dark: bool,
) {
    std::thread::spawn(move || {
        let scale = scale();
        let Some(page) = render(scale) else { return };
        let desk =
            u32::from(desk_rgb.0) | (u32::from(desk_rgb.1) << 8) | (u32::from(desk_rgb.2) << 16);
        let _ = PAINT.set(Paint {
            page,
            desk,
            top: (22.0 * scale).round() as i32,
        });
        // SAFETY: standard window creation and message loop on this thread; every
        // pointer passed lives for the duration of the call.
        unsafe { run(scale, client_logical, dark) };
    });
}

/// Close the splash (from any thread). Safe to call when there is none.
pub fn close() {
    let raw = HWND_RAW.swap(0, Ordering::SeqCst);
    if raw != 0 {
        // SAFETY: posting to a window handle is allowed from any thread; if the
        // window is already gone the call just fails.
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe fn run(scale: f32, client_logical: f32, dark: bool) {
    unsafe {
        let Ok(instance) = GetModuleHandleW(None) else {
            return;
        };
        let class = w!("KitabSplash");
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
        // Same place as GPUI's Bounds::centered on the primary display: the client
        // area is client_logical × client_logical logical pixels, centred.
        let side = (client_logical * scale).round() as i32;
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let mut rect = RECT {
            left: (sw - side) / 2,
            top: (sh - side) / 2,
            right: (sw + side) / 2,
            bottom: (sh + side) / 2,
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
            w!(""),
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
        crate::mark("splash_painted");
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
    // valid arguments; GDI calls get the window's own DC from BeginPaint.
    unsafe {
        match msg {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                if let Some(p) = PAINT.get() {
                    let brush = CreateSolidBrush(COLORREF(p.desk));
                    let mut client = RECT::default();
                    let _ =
                        windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client);
                    FillRect(hdc, &client, brush);
                    let _ = DeleteObject(brush.into());
                    let (w, h) = (p.page.width as i32, p.page.height as i32);
                    let x = ((client.right - w) / 2).max(0);
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
                        x,
                        p.top,
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
