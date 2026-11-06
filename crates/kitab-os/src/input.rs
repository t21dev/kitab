// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mouse clicks posted to one window by handle (feature `input`, kitab-bench
//! only). Messages go to that window's queue: nothing else on the desktop sees
//! them, and the real cursor doesn't move.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
    WM_RBUTTONDOWN, WM_RBUTTONUP,
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
