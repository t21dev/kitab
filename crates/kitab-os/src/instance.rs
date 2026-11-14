// SPDX-License-Identifier: AGPL-3.0-or-later

//! One Kitab per user (PLAN.md Phase 2, single instance). The first process
//! listens on a per-user channel; a later one (a double-clicked file, say)
//! hands its file paths over and exits, and the first opens them as tabs.
//!
//! Windows: a named pipe, inbound only, local clients only. Linux: a Unix
//! socket in the runtime folder. macOS needs neither: the system sends file
//! opens to the running app (GPUI's open-URLs callback).
//!
//! The message is text: `KITAB1`, then one absolute path per line. A message
//! over 64 KiB, or one that isn't that shape, is ignored. Paths are only
//! opened, never run; the reader treats every file as hostile.

use std::path::PathBuf;

/// Largest message read; more is a broken or hostile client.
const MAX_MESSAGE: usize = 64 * 1024;
const MAGIC: &str = "KITAB1";

/// What `claim` found.
pub enum Claim {
    /// This process is the one Kitab: call [`Listener::run`] to receive paths.
    Primary(Listener),
    /// Another Kitab took the paths (and comes to the front): exit now.
    Forwarded,
}

// macOS hands opens to the running app itself, without a message.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn message(files: &[PathBuf]) -> Vec<u8> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut text = String::from(MAGIC);
    for f in files {
        let abs = if f.is_absolute() {
            f.clone()
        } else {
            cwd.join(f)
        };
        if let Some(s) = abs.to_str()
            && !s.contains('\n')
        {
            text.push('\n');
            text.push_str(s);
        }
    }
    text.into_bytes()
}

/// Parse a message into paths (an empty list means "come to the front").
pub fn parse(bytes: &[u8]) -> Option<Vec<PathBuf>> {
    if bytes.len() > MAX_MESSAGE {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.split('\n');
    if lines.next()? != MAGIC {
        return None;
    }
    Some(
        lines
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .collect(),
    )
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn user_key() -> String {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".into());
    user.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(40)
        .collect()
}

/// Hand `files` to a running Kitab, or become the one.
pub fn claim(files: &[PathBuf]) -> Claim {
    imp::claim(files)
}

pub struct Listener {
    inner: imp::Inner,
}

impl Listener {
    /// Receive messages on a background thread for the life of the process;
    /// `on_paths` gets each one (possibly empty: just come to the front). The
    /// thread blocks in the OS between messages, so it costs nothing idle.
    pub fn run(self, on_paths: impl Fn(Vec<PathBuf>) + Send + 'static) {
        let inner = self.inner;
        let _ = std::thread::Builder::new()
            .name("kitab-instance".into())
            .spawn(move || imp::serve(inner, &on_paths));
    }
}

#[cfg(windows)]
mod imp {
    use super::{Claim, Listener, MAX_MESSAGE, message, parse, user_key};
    use std::path::PathBuf;
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_WRITE, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_SHARE_NONE,
        OPEN_EXISTING, PIPE_ACCESS_INBOUND, ReadFile, WriteFile,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
        WaitNamedPipeW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};
    use windows::core::HSTRING;

    pub struct Inner {
        name: HSTRING,
        first: isize,
    }

    // SAFETY: the handle is moved to the listener thread and used only there.
    unsafe impl Send for Inner {}

    fn pipe_name() -> HSTRING {
        HSTRING::from(format!(r"\\.\pipe\kitab-{}-v1", user_key()))
    }

    fn send(name: &HSTRING, bytes: &[u8]) -> bool {
        for _ in 0..3 {
            // SAFETY: plain Win32 call with a valid, NUL-terminated name.
            let handle = unsafe {
                CreateFileW(
                    name,
                    GENERIC_WRITE.0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                )
            };
            match handle {
                Ok(h) => {
                    let mut written = 0u32;
                    // SAFETY: `h` is an open pipe handle; the buffer outlives the call.
                    let ok = unsafe { WriteFile(h, Some(bytes), Some(&mut written), None) }.is_ok();
                    // SAFETY: closing the handle we opened.
                    let _ = unsafe { CloseHandle(h) };
                    return ok && written as usize == bytes.len();
                }
                Err(_) => {
                    // SAFETY: reads the calling thread's last error.
                    if unsafe { GetLastError() } == ERROR_PIPE_BUSY {
                        // SAFETY: waits on a pipe by name; no pointers kept.
                        let _ = unsafe { WaitNamedPipeW(name, 1000) };
                        continue;
                    }
                    return false;
                }
            }
        }
        false
    }

    fn create(name: &HSTRING, first: bool) -> Option<HANDLE> {
        let mut open_mode = PIPE_ACCESS_INBOUND;
        if first {
            open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        // SAFETY: plain Win32 call; the default security descriptor lets
        // only this user (and administrators) write to the pipe.
        let h = unsafe {
            CreateNamedPipeW(
                name,
                open_mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                0,
                MAX_MESSAGE as u32,
                0,
                None,
            )
        };
        (h != INVALID_HANDLE_VALUE && !h.is_invalid()).then_some(h)
    }

    pub fn claim(files: &[PathBuf]) -> Claim {
        let name = pipe_name();
        let bytes = message(files);
        // The running Kitab may bring its window to the front: Windows lets
        // only the foreground process (this one, just launched) allow that.
        // SAFETY: plain Win32 call without pointers.
        let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
        if send(&name, &bytes) {
            return Claim::Forwarded;
        }
        match create(&name, true) {
            Some(h) => Claim::Primary(Listener {
                inner: Inner {
                    name,
                    first: h.0 as isize,
                },
            }),
            // Another Kitab started at the same moment: give it the files.
            None if send(&name, &bytes) => Claim::Forwarded,
            // No pipe at all (unusual): run on our own.
            None => Claim::Primary(Listener {
                inner: Inner { name, first: 0 },
            }),
        }
    }

    fn read_all(h: HANDLE) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let mut read = 0u32;
            // SAFETY: `h` is a connected pipe; `buf` outlives the call.
            let ok = unsafe { ReadFile(h, Some(&mut buf), Some(&mut read), None) }.is_ok();
            if !ok || read == 0 {
                break;
            }
            out.extend_from_slice(&buf[..read as usize]);
            if out.len() > MAX_MESSAGE {
                break;
            }
        }
        out
    }

    pub fn serve(inner: Inner, on_paths: &dyn Fn(Vec<PathBuf>)) {
        let mut next = (inner.first != 0).then_some(HANDLE(inner.first as *mut core::ffi::c_void));
        loop {
            let Some(h) = next.take().or_else(|| create(&inner.name, false)) else {
                return;
            };
            // SAFETY: waits for a client on our pipe handle (blocking, no CPU).
            let connected = unsafe { ConnectNamedPipe(h, None) }.is_ok()
                // SAFETY: reads the calling thread's last error.
                || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
            if connected {
                let bytes = read_all(h);
                if let Some(paths) = parse(&bytes) {
                    on_paths(paths);
                }
            }
            // SAFETY: done with this client; disconnect and close our handle.
            unsafe {
                let _ = DisconnectNamedPipe(h);
                let _ = CloseHandle(h);
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::{Claim, Listener, MAX_MESSAGE, message, parse, user_key};
    use std::io::{Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;

    pub struct Inner {
        listener: Option<UnixListener>,
    }

    fn socket_path() -> PathBuf {
        let mut dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        // Flatpak gives each sandbox its own runtime folder; this one is
        // shared by every running copy of the app.
        if let Some(id) = std::env::var_os("FLATPAK_ID") {
            dir = dir.join("app").join(id);
        }
        dir.join(format!("kitab-{}.sock", user_key()))
    }

    fn send(path: &PathBuf, bytes: &[u8]) -> bool {
        match UnixStream::connect(path) {
            Ok(mut s) => s.write_all(bytes).is_ok(),
            Err(_) => false,
        }
    }

    pub fn claim(files: &[PathBuf]) -> Claim {
        let path = socket_path();
        let bytes = message(files);
        if send(&path, &bytes) {
            return Claim::Forwarded;
        }
        // A socket file left by a crashed Kitab: nobody answered, remove it.
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).ok();
        if let Some(l) = &listener {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            let _ = l;
        }
        Claim::Primary(Listener {
            inner: Inner { listener },
        })
    }

    pub fn serve(inner: Inner, on_paths: &dyn Fn(Vec<PathBuf>)) {
        let Some(listener) = inner.listener else {
            return;
        };
        for stream in listener.incoming().flatten() {
            let mut bytes = Vec::new();
            let _ = stream.take(MAX_MESSAGE as u64 + 1).read_to_end(&mut bytes);
            if let Some(paths) = parse(&bytes) {
                on_paths(paths);
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{Claim, Listener};
    use std::path::PathBuf;

    pub struct Inner;

    /// macOS routes file opens to the running app itself.
    pub fn claim(_files: &[PathBuf]) -> Claim {
        Claim::Primary(Listener { inner: Inner })
    }

    pub fn serve(_inner: Inner, _on_paths: &dyn Fn(Vec<PathBuf>)) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_and_reject_junk() {
        let abs = std::env::temp_dir().join("a.pdf");
        let msg = message(std::slice::from_ref(&abs));
        assert_eq!(parse(&msg), Some(vec![abs]));
        assert_eq!(parse(MAGIC.as_bytes()), Some(vec![]), "focus only");
        assert_eq!(parse(b"GET / HTTP/1.1\r\n"), None);
        assert_eq!(parse(&vec![b'a'; MAX_MESSAGE + 1]), None);
        // Relative paths are never accepted from the channel.
        assert_eq!(parse(b"KITAB1\nrelative.pdf"), Some(vec![]));
    }
}
