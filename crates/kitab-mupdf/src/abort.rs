// SPDX-License-Identifier: AGPL-3.0-or-later

//! Stop renders nobody wants any more, part-way through a page.
//!
//! A render already checks, before it starts and after it ends, whether its
//! page is still wanted (`Engine::render_when`). A heavy page (a scan, a map,
//! a CAD drawing) can take hundreds of milliseconds, though, and flicking
//! past it kept the worker busy drawing a page already gone from the screen.
//! Each render now carries a MuPDF cookie; while any render runs, one small
//! thread checks every [`TICK`] whether each is still wanted and, if not,
//! sets its cookie's abort flag. MuPDF stops between drawing operations and
//! the render returns as stale.
//!
//! MuPDF documents the cookie as written by another thread while a page
//! runs (an unsynchronised int). The cookie is only touched under the
//! registry's lock, and a job is taken out of the registry before its cookie
//! is freed, so the flag is never written after the free. That one write
//! is the only `unsafe` here.

#![allow(unsafe_code)]

use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

/// How often running renders are checked: a frame at 60 Hz.
const TICK: Duration = Duration::from_millis(16);

type Stale = Box<dyn Fn() -> bool + Send>;

struct Job {
    id: u64,
    /// The `fz_cookie` of the running render, as an address.
    cookie: usize,
    stale: Stale,
    aborted: bool,
}

#[derive(Default)]
struct Registry {
    jobs: Vec<Job>,
    next: u64,
}

struct Shared {
    reg: Mutex<Registry>,
    wake: Condvar,
}

fn shared() -> &'static Arc<Shared> {
    static S: OnceLock<Arc<Shared>> = OnceLock::new();
    S.get_or_init(|| {
        let s = Arc::new(Shared {
            reg: Mutex::new(Registry::default()),
            wake: Condvar::new(),
        });
        let t = s.clone();
        let _ = std::thread::Builder::new()
            .name("kitab-render-abort".into())
            .spawn(move || watch(&t));
        s
    })
}

fn watch(s: &Shared) {
    let mut reg = s.reg.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if reg.jobs.is_empty() {
            // Nothing running: sleep until a render registers.
            reg = s.wake.wait(reg).unwrap_or_else(|e| e.into_inner());
            continue;
        }
        for job in reg.jobs.iter_mut().filter(|j| !j.aborted) {
            if (job.stale)() {
                job.aborted = true;
                // SAFETY: the job is in the registry, so its cookie is alive
                // (it is removed, under this lock, before the cookie is freed).
                unsafe {
                    (*(job.cookie as *mut mupdf_sys::fz_cookie)).abort = 1;
                }
            }
        }
        reg = s
            .wake
            .wait_timeout(reg, TICK)
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
}

/// A render being watched; dropping it stops the watching. Drop it before
/// the cookie.
pub(crate) struct Watch {
    id: u64,
}

/// Watch the render using `cookie` (an `fz_cookie` address), aborting it
/// once `stale` says so.
pub(crate) fn watch_render(cookie: usize, stale: Stale) -> Watch {
    let s = shared();
    let mut reg = s.reg.lock().unwrap_or_else(|e| e.into_inner());
    reg.next += 1;
    let id = reg.next;
    reg.jobs.push(Job {
        id,
        cookie,
        stale,
        aborted: false,
    });
    s.wake.notify_one();
    Watch { id }
}

impl Watch {
    /// Whether the render was told to stop.
    pub(crate) fn aborted(&self) -> bool {
        let reg = shared().reg.lock().unwrap_or_else(|e| e.into_inner());
        reg.jobs.iter().any(|j| j.id == self.id && j.aborted)
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let mut reg = shared().reg.lock().unwrap_or_else(|e| e.into_inner());
        reg.jobs.retain(|j| j.id != self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn a_stale_render_is_told_to_stop() {
        // SAFETY: fz_cookie is plain C ints; all zero is its start state.
        let mut cookie: mupdf_sys::fz_cookie = unsafe { std::mem::zeroed() };
        let gone = Arc::new(AtomicBool::new(false));
        let g = gone.clone();
        let w = watch_render(
            std::ptr::addr_of_mut!(cookie) as usize,
            Box::new(move || g.load(Ordering::Relaxed)),
        );
        std::thread::sleep(TICK * 3);
        assert!(!w.aborted());
        gone.store(true, Ordering::Relaxed);
        let start = std::time::Instant::now();
        while !w.aborted() && start.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(w.aborted());
        drop(w);
        // SAFETY: the watch is gone, so nothing else writes the cookie.
        assert_eq!(unsafe { std::ptr::read_volatile(&cookie.abort) }, 1);
    }
}
