// SPDX-License-Identifier: AGPL-3.0-or-later

//! Start-up trace for performance work (docs/perf): with `KITAB_TRACE_START`
//! set, each mark appends "wall ms, process CPU ms, label" to
//! `%TEMP%/kitab-trace.txt`. Without it, a mark is one cached check.

use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Option<(Instant, std::sync::Mutex<std::fs::File>)>> = OnceLock::new();

fn sink() -> &'static Option<(Instant, std::sync::Mutex<std::fs::File>)> {
    START.get_or_init(|| {
        std::env::var_os("KITAB_TRACE_START")?;
        let path = std::env::temp_dir().join("kitab-trace.txt");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        Some((Instant::now(), std::sync::Mutex::new(file)))
    })
}

/// CPU time of the whole process so far, in ms (Windows; 0 elsewhere).
fn cpu_ms() -> f64 {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::FILETIME;
        use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
        let (mut c, mut e, mut k, mut u) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // SAFETY: the current-process pseudo handle is always valid; the
        // call only writes the four structs it is given.
        let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) };
        if ok.is_err() {
            return 0.0;
        }
        let t =
            |f: FILETIME| ((u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)) as f64;
        (t(k) + t(u)) / 10_000.0
    }
    #[cfg(not(windows))]
    {
        0.0
    }
}

/// Note a start-up step (no-op unless `KITAB_TRACE_START` is set).
pub fn mark(label: &str) {
    if let Some((start, file)) = sink() {
        if let Ok(mut f) = file.lock() {
            let _ = writeln!(
                f,
                "{:8.1} ms  cpu {:7.1} ms  {label}",
                start.elapsed().as_secs_f64() * 1000.0,
                cpu_ms()
            );
        }
    }
}
