// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mouse clicks posted to one window by handle (feature `input`, kitab-bench
//! only). Messages go to that window's queue: nothing else on the desktop sees
//! them, and the real cursor doesn't move.

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_VSC, MapVirtualKeyW};
use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN,
    WM_RBUTTONUP,
};

#[derive(Clone, Copy, Debug)]
pub enum Button {
    Left,
    Right,
    Middle,
}

fn lparam(x: i32, y: i32) -> LPARAM {
    LPARAM((((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF)) as isize)
}

fn post(hwnd: HWND, msg: u32, wparam: usize, x: i32, y: i32) -> bool {
    // SAFETY: PostMessageW only queues a message; an invalid handle makes it fail
    // and return an error, which is reported to the caller.
    unsafe { PostMessageW(Some(hwnd), msg, WPARAM(wparam), lparam(x, y)).is_ok() }
}

/// Move to (`x`, `y`) in client pixels and click there.
pub fn click(hwnd: isize, x: i32, y: i32, button: Button) -> bool {
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    let (down, up, held) = match button {
        Button::Left => (WM_LBUTTONDOWN, WM_LBUTTONUP, 0x0001),
        Button::Right => (WM_RBUTTONDOWN, WM_RBUTTONUP, 0x0002),
        Button::Middle => (WM_MBUTTONDOWN, WM_MBUTTONUP, 0x0010),
    };
    post(hwnd, WM_MOUSEMOVE, 0, x, y) && post(hwnd, down, held, x, y) && post(hwnd, up, 0, x, y)
}

/// Move the pointer to (`x`, `y`) in client pixels (hover states).
pub fn hover(hwnd: isize, x: i32, y: i32) -> bool {
    post(HWND(hwnd as *mut core::ffi::c_void), WM_MOUSEMOVE, 0, x, y)
}

/// Turn the wheel over (`x`, `y`) in client pixels; `notches` > 0 scrolls up.
pub fn wheel(hwnd: isize, x: i32, y: i32, notches: i32) -> bool {
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    let mut pt = POINT { x, y };
    // SAFETY: `pt` is a valid POINT for the call; an invalid window handle
    // makes it return false, which is reported.
    if !unsafe { ClientToScreen(hwnd, &mut pt) }.as_bool() {
        return false;
    }
    // WM_MOUSEWHEEL: the delta is the high word of wparam; the point is in screen pixels.
    let delta = ((notches * 120) as i16 as u16 as usize) << 16;
    post(hwnd, WM_MOUSEMOVE, 0, x, y) && post(hwnd, WM_MOUSEWHEEL, delta, pt.x, pt.y)
}

/// Resize the window to `w` × `h` physical pixels, keeping its position.
pub fn resize(hwnd: isize, w: i32, h: i32) -> bool {
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    // SAFETY: SetWindowPos only changes this window's size; an invalid handle
    // makes it return an error, which is reported.
    unsafe { SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER).is_ok() }
}

/// Press and release the key with virtual-key code `vk` (no modifiers).
pub fn key(hwnd: isize, vk: u16) -> bool {
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    // SAFETY: MapVirtualKeyW only reads the keyboard layout.
    let scan = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } as isize;
    let down = LPARAM(1 | (scan << 16));
    let up = LPARAM(1 | (scan << 16) | (1 << 30) | (1 << 31));
    // SAFETY: PostMessageW only queues messages for this window; an invalid
    // handle makes it fail, which is reported.
    unsafe {
        PostMessageW(Some(hwnd), WM_KEYDOWN, WPARAM(usize::from(vk)), down).is_ok()
            && PostMessageW(Some(hwnd), WM_KEYUP, WPARAM(usize::from(vk)), up).is_ok()
    }
}

/// The virtual-key code that types `c` on a US layout, for letters, digits,
/// space and `/`.
pub fn vk_for(c: char) -> Option<u16> {
    match c {
        'a'..='z' => Some(c.to_ascii_uppercase() as u16),
        'A'..='Z' | '0'..='9' => Some(c as u16),
        ' ' => Some(0x20),
        '/' => Some(0xBF),
        '-' => Some(0xBD),
        _ => None,
    }
}
