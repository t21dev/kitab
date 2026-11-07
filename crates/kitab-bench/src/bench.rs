// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use xcap::Window;

const TIMEOUT: Duration = Duration::from_secs(15);
/// How long to let the app settle after its page is on screen before reading memory and idle CPU.
const SETTLE: Duration = Duration::from_secs(2);
/// A pixel counts as content when it differs from the region's dominant colour by more than this (0–255 per channel).
const INK_DELTA: u8 = 40;
/// The region counts as non-blank when at least this share of its pixels is content.
const INK_SHARE: f64 = 0.002;

struct Opts {
    file: PathBuf,
    runs: usize,
    warmup: usize,
    kitab: PathBuf,
    kitab_args: Vec<String>,
    sumatra: PathBuf,
    only: Option<String>,
    out: PathBuf,
}

#[derive(Serialize, Clone, Copy)]
struct Run {
    window_ms: f64,
    /// The page was not yet on screen at this time (start of the last capture without it).
    page_lo_ms: f64,
    /// The page was on screen by this time (end of the first capture that showed it).
    page_hi_ms: f64,
    /// Midpoint of the bracket, used for the summary statistics.
    page_ms: f64,
    captures: u32,
    capture_ms_avg: f64,
    /// CPU time (user + kernel, all threads) used up to the moment the page was found.
    cpu_ms_to_page: f64,
    /// Working set and private bytes after settling, in MB.
    working_set_mb: f64,
    private_mb: f64,
    /// CPU used while idle during the settle period, as % of one core.
    idle_cpu_pct: f64,
}

#[derive(Serialize)]
struct Stats {
    median: f64,
    p90: f64,
    max: f64,
}

#[derive(Serialize)]
struct AppResult {
    app: String,
    exe: String,
    args: Vec<String>,
    runs: Vec<Run>,
    window_ms: Stats,
    page_ms: Stats,
    page_lo_ms: Stats,
    page_hi_ms: Stats,
    cpu_ms_to_page: Stats,
    working_set_mb: Stats,
    private_mb: Stats,
    idle_cpu_pct: Stats,
    capture_interval_ms: f64,
}

#[derive(Serialize)]
struct Report {
    scenario: &'static str,
    file: String,
    unix_time: u64,
    machine: String,
    results: Vec<AppResult>,
}

pub fn run(args: Vec<String>) -> i32 {
    if args.first().map(String::as_str) == Some("shot") {
        return shot(&args[1..]);
    }
    let opts = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("kitab-bench: {e}");
            return 2;
        }
    };
    if !opts.file.is_file() {
        eprintln!("kitab-bench: file not found: {}", opts.file.display());
        return 2;
    }
    let file = match opts.file.canonicalize() {
        Ok(f) => strip_verbatim(&f),
        Err(e) => {
            eprintln!("kitab-bench: {e}");
            return 2;
        }
    };

    let mut apps: Vec<(String, PathBuf, Vec<String>)> = Vec::new();
    if opts.only.as_deref() != Some("sumatra") {
        let mut a = opts.kitab_args.clone();
        a.insert(0, file.clone());
        apps.push(("kitab".into(), opts.kitab.clone(), a));
    }
    if opts.only.as_deref() != Some("kitab") {
        apps.push(("sumatra".into(), opts.sumatra.clone(), vec![file.clone()]));
    }

    let mut results = Vec::new();
    for (name, exe, args) in apps {
        if !exe.is_file() {
            eprintln!(
                "kitab-bench: {name}: executable not found: {}",
                exe.display()
            );
            return 2;
        }
        if let Some(other) = existing_window_of(&exe) {
            eprintln!(
                "kitab-bench: {name} is already running (pid {other}); close it first so every run is a fresh start."
            );
            return 2;
        }
        eprintln!(
            "{name}: {} warm-up + {} measured runs",
            opts.warmup, opts.runs
        );
        let mut runs = Vec::new();
        let _ = std::fs::create_dir_all(&opts.out);
        let sample = opts.out.join(format!("last-region-{name}.png"));
        for i in 0..opts.warmup + opts.runs {
            match one_run(&exe, &args, Some(&sample)) {
                Ok(r) => {
                    let tag = if i < opts.warmup { "warm-up" } else { "run" };
                    eprintln!(
                        "  {tag:7} {:>2}: window {:>6.1} ms, page {:>6.1}–{:<6.1} ms, cpu {:>5.0} ms, ws {:>5.1} MB, private {:>5.1} MB, idle {:>4.1} %",
                        i + 1,
                        r.window_ms,
                        r.page_lo_ms,
                        r.page_hi_ms,
                        r.cpu_ms_to_page,
                        r.working_set_mb,
                        r.private_mb,
                        r.idle_cpu_pct
                    );
                    if i >= opts.warmup {
                        runs.push(r);
                    }
                }
                Err(e) => {
                    eprintln!("kitab-bench: {name}: run {} failed: {e}", i + 1);
                    return 1;
                }
            }
            std::thread::sleep(Duration::from_millis(600));
        }
        let interval =
            runs.iter().map(|r| r.capture_ms_avg).sum::<f64>() / runs.len().max(1) as f64;
        results.push(AppResult {
            app: name,
            exe: exe.display().to_string(),
            args,
            window_ms: stats(runs.iter().map(|r| r.window_ms).collect()),
            page_ms: stats(runs.iter().map(|r| r.page_ms).collect()),
            page_lo_ms: stats(runs.iter().map(|r| r.page_lo_ms).collect()),
            page_hi_ms: stats(runs.iter().map(|r| r.page_hi_ms).collect()),
            cpu_ms_to_page: stats(runs.iter().map(|r| r.cpu_ms_to_page).collect()),
            working_set_mb: stats(runs.iter().map(|r| r.working_set_mb).collect()),
            private_mb: stats(runs.iter().map(|r| r.private_mb).collect()),
            idle_cpu_pct: stats(runs.iter().map(|r| r.idle_cpu_pct).collect()),
            capture_interval_ms: interval,
            runs,
        });
    }

    print_table(&results);
    let report = Report {
        scenario: "S2",
        file,
        unix_time: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        machine: std::env::var("COMPUTERNAME").unwrap_or_default(),
        results,
    };
    if let Err(e) = write_report(&opts.out, &report) {
        eprintln!("kitab-bench: could not write the report: {e}");
        return 1;
    }
    0
}

fn parse(args: Vec<String>) -> Result<Opts, String> {
    let mut it = args.into_iter();
    match it.next().as_deref() {
        Some("s2") | Some("S2") => {}
        Some(other) => return Err(format!("unknown scenario {other:?}; only s2 exists so far")),
        None => return Err("usage: kitab-bench s2 [--file F] [--runs N] [--warmup N] [--kitab PATH] [--kitab-arg A]… [--sumatra PATH] [--only kitab|sumatra] [--out DIR]".into()),
    }
    let mut o = Opts {
        file: PathBuf::from("tests/fixtures/pdf/01-text-outline.pdf"),
        runs: 15,
        warmup: 2,
        kitab: PathBuf::from("spikes/spike-a/target/release/spike-a.exe"),
        kitab_args: Vec::new(),
        sumatra: PathBuf::from(r"C:\Program Files\SumatraPDF\SumatraPDF.exe"),
        only: None,
        out: PathBuf::from("docs/perf/runs"),
    };
    while let Some(flag) = it.next() {
        let mut val = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--file" => o.file = val()?.into(),
            "--runs" => o.runs = val()?.parse().map_err(|_| "--runs takes a number")?,
            "--warmup" => o.warmup = val()?.parse().map_err(|_| "--warmup takes a number")?,
            "--kitab" => o.kitab = val()?.into(),
            "--kitab-arg" => o.kitab_args.push(val()?),
            "--sumatra" => o.sumatra = val()?.into(),
            "--only" => o.only = Some(val()?),
            "--out" => o.out = val()?.into(),
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if o.runs == 0 {
        return Err("--runs must be at least 1".into());
    }
    Ok(o)
}

fn one_run(exe: &Path, args: &[String], save: Option<&Path>) -> Result<Run, String> {
    let start = Instant::now();
    let mut child = Command::new(exe)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", exe.display()))?;
    let pid = child.id();
    let result = measure(pid, start, save).and_then(|mut run| {
        let (cpu_at_page, _, _) =
            sample(pid).ok_or("the app exited before its page could be measured")?;
        std::thread::sleep(SETTLE);
        let (cpu_after, ws, private) = sample(pid).ok_or("the app exited while settling")?;
        run.cpu_ms_to_page = cpu_at_page;
        run.working_set_mb = ws;
        run.private_mb = private;
        run.idle_cpu_pct = (cpu_after - cpu_at_page) / ms(SETTLE) * 100.0;
        Ok(run)
    });
    stop(&mut child);
    result
}

/// (CPU ms, working set MB, private MB) for one process.
fn sample(pid: u32) -> Option<(f64, f64, f64)> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new().ok()?;
    let pid = Pid::from_u32(pid);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory().with_cpu(),
    );
    let p = sys.process(pid)?;
    let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
    Some((
        p.accumulated_cpu_time() as f64,
        mb(p.memory()),
        mb(p.virtual_memory()),
    ))
}

fn measure(pid: u32, start: Instant, save: Option<&Path>) -> Result<Run, String> {
    let mut window_ms = None;
    let mut win: Option<Window> = None;
    // (hash, start of that capture, end of that capture, start of the capture before it)
    let mut last: Option<(u64, f64, f64, f64)> = None;
    let mut prev_start: Option<f64> = None;
    let mut captures = 0u32;
    let mut capture_total = Duration::ZERO;
    while start.elapsed() < TIMEOUT {
        if win.is_none() {
            win = window_of(pid);
            if win.is_none() {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            window_ms.get_or_insert(ms(start.elapsed()));
        }
        let (Some(w), Some(window_ms)) = (win.as_ref(), window_ms) else {
            continue;
        };
        let before = Instant::now();
        // The window's own pixels only (Windows Graphics Capture), never the screen.
        let img = match w.capture_image() {
            Ok(img) => img,
            Err(e) => {
                if std::env::var_os("KITAB_BENCH_DEBUG").is_some() {
                    eprintln!("    capture failed at {:.0} ms: {e}", ms(start.elapsed()));
                }
                // The window may have closed (a splash handing over to the main
                // window): find the app's current window again. "Window shown"
                // keeps its first time.
                win = None;
                last = None;
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
        };
        capture_total += before.elapsed();
        captures += 1;
        let (w, h) = img.dimensions();
        // The middle of the window: the page area for a reader opened on a file.
        let (x0, x1, y0, y1) = (w / 4, w * 3 / 4, h * 3 / 10, h * 8 / 10);
        let mut hasher = DefaultHasher::new();
        let mut counts = std::collections::HashMap::<[u8; 3], u32>::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let p = img.get_pixel(x, y).0;
                let rgb = [p[0], p[1], p[2]];
                rgb.hash(&mut hasher);
                *counts.entry(rgb).or_default() += 1;
            }
        }
        let total = f64::from((x1 - x0) * (y1 - y0)).max(1.0);
        let dominant = counts
            .iter()
            .max_by_key(|(_, n)| **n)
            .map(|(c, _)| *c)
            .unwrap_or([0, 0, 0]);
        let ink: u32 = counts
            .iter()
            .filter(|(c, _)| {
                c.iter()
                    .zip(dominant)
                    .any(|(a, b)| a.abs_diff(b) > INK_DELTA)
            })
            .map(|(_, n)| *n)
            .sum();
        let hash = hasher.finish();
        let started = ms(before.duration_since(start));
        let ended = ms(start.elapsed());
        if f64::from(ink) / total >= INK_SHARE {
            if let Some((prev, _prev_start, prev_end, lo)) = last {
                if prev == hash {
                    if let Some(path) = save {
                        let _ = img.save(path);
                    }
                    return Ok(Run {
                        window_ms,
                        page_lo_ms: lo,
                        page_hi_ms: prev_end,
                        page_ms: (lo + prev_end) / 2.0,
                        cpu_ms_to_page: 0.0,
                        working_set_mb: 0.0,
                        private_mb: 0.0,
                        idle_cpu_pct: 0.0,
                        captures,
                        capture_ms_avg: ms(capture_total) / f64::from(captures),
                    });
                }
            }
            last = Some((hash, started, ended, prev_start.unwrap_or(window_ms)));
        } else {
            last = None;
        }
        prev_start = Some(started);
    }
    Err(format!("no stable page within {} s", TIMEOUT.as_secs()))
}

/// The largest visible top-level window owned by `pid`.
fn window_of(pid: u32) -> Option<Window> {
    let all = Window::all().ok()?;
    all.into_iter()
        .filter(|w| w.pid().ok() == Some(pid))
        .filter(|w| !w.is_minimized().unwrap_or(true))
        .filter(|w| w.width().unwrap_or(0) >= 200 && w.height().unwrap_or(0) >= 200)
        .max_by_key(|w| u64::from(w.width().unwrap_or(0)) * u64::from(w.height().unwrap_or(0)))
}

/// A window already open from the same executable name (single-instance apps would hand the file to it).
fn existing_window_of(exe: &Path) -> Option<u32> {
    let stem = exe.file_stem()?.to_string_lossy().to_lowercase();
    Window::all()
        .ok()?
        .into_iter()
        .find(|w| {
            w.app_name()
                .map(|n| n.to_lowercase().contains(&stem))
                .unwrap_or(false)
        })
        .and_then(|w| w.pid().ok())
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn stats(mut v: Vec<f64>) -> Stats {
    v.sort_by(|a, b| a.total_cmp(b));
    let at = |q: f64| -> f64 {
        if v.is_empty() {
            return f64::NAN;
        }
        let i = ((v.len() as f64 - 1.0) * q).round() as usize;
        v[i.min(v.len() - 1)]
    };
    Stats {
        median: at(0.5),
        p90: at(0.9),
        max: v.last().copied().unwrap_or(f64::NAN),
    }
}

fn print_table(results: &[AppResult]) {
    println!();
    println!("S2: process start → first page on screen, ms (median over runs)");
    println!(
        "{:<10} {:>24} {:>14} {:>16} {:>14}",
        "app", "page between (lo – hi)", "page (mid)", "window shown", "capture cost"
    );
    for r in results {
        println!(
            "{:<10} {:>11.0} – {:<10.0} {:>14.0} {:>16.0} {:>11.1} ms",
            r.app,
            r.page_lo_ms.median,
            r.page_hi_ms.median,
            r.page_ms.median,
            r.window_ms.median,
            r.capture_interval_ms
        );
    }
    println!(
        "A page time is a bracket: the last capture without the page, and the first one with it."
    );
    println!();
    println!(
        "Resources (median). CPU to page: user + kernel time of all threads until the page was found."
    );
    println!(
        "Memory and idle CPU are read {} s after the page.",
        SETTLE.as_secs()
    );
    println!(
        "{:<10} {:>14} {:>16} {:>14} {:>12}",
        "app", "CPU to page", "working set", "private", "idle CPU"
    );
    for r in results {
        println!(
            "{:<10} {:>11.0} ms {:>13.1} MB {:>11.1} MB {:>10.1} %",
            r.app,
            r.cpu_ms_to_page.median,
            r.working_set_mb.median,
            r.private_mb.median,
            r.idle_cpu_pct.median
        );
    }
}

fn write_report(dir: &Path, report: &Report) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}-s2.json", report.unix_time));
    let json = serde_json::to_string_pretty(report).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)?;
    eprintln!("report: {}", path.display());
    Ok(())
}

fn run_steps(hwnd: isize, steps: &str) -> Result<(), String> {
    use kitab_os::input::{Button, click, hover, resize, wheel};
    for step in steps.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (verb, arg) = step.split_once(':').ok_or(format!("bad step {step}"))?;
        let xy = || -> Result<(i32, i32), String> {
            let (x, y) = arg.split_once(',').ok_or(format!("bad point in {step}"))?;
            Ok((
                x.trim().parse().map_err(|_| format!("bad x in {step}"))?,
                y.trim().parse().map_err(|_| format!("bad y in {step}"))?,
            ))
        };
        let ok = match verb {
            "wait" => {
                std::thread::sleep(Duration::from_millis(
                    arg.parse().map_err(|_| format!("bad wait {step}"))?,
                ));
                true
            }
            "click" => xy().map(|(x, y)| click(hwnd, x, y, Button::Left))?,
            "rclick" => xy().map(|(x, y)| click(hwnd, x, y, Button::Right))?,
            "mclick" => xy().map(|(x, y)| click(hwnd, x, y, Button::Middle))?,
            "hover" => xy().map(|(x, y)| hover(hwnd, x, y))?,
            // size:W,H resizes the window (short-screen checks).
            "size" => xy().map(|(w, h)| resize(hwnd, w, h))?,
            // wheel:X,Y,N turns the wheel N notches (negative scrolls down).
            "wheel" => {
                let mut it = arg.split(',').map(|v| v.trim().parse::<i32>());
                match (it.next(), it.next(), it.next()) {
                    (Some(Ok(x)), Some(Ok(y)), Some(Ok(n))) => wheel(hwnd, x, y, n),
                    _ => return Err(format!("bad wheel step {step}")),
                }
            }
            other => return Err(format!("unknown step {other}")),
        };
        if !ok {
            return Err(format!("could not post {step}"));
        }
        // Let the app handle the message and draw before the next step.
        std::thread::sleep(Duration::from_millis(150));
    }
    Ok(())
}

/// `canonicalize` on Windows returns `\\?\C:\…`, which some apps don't accept as an argument.
fn strip_verbatim(p: &Path) -> String {
    let s = p.display().to_string();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

/// `kitab-bench shot --out FILE.png [--wait MS] [--do STEPS] -- EXE [ARGS…]`: start an
/// app, wait, and save a capture of **its own window only** (PrintWindow), never
/// the screen, so other apps' content can't end up in a screenshot.
///
/// `--do` runs steps on that window before the capture, separated by `;`:
/// `click:X,Y`, `rclick:X,Y`, `mclick:X,Y`, `hover:X,Y`, `size:W,H`, `wheel:X,Y,N` (client pixels, as in the
/// capture) and `wait:MS`. Clicks are posted to the window, not the desktop.
fn shot(args: &[String]) -> i32 {
    let mut out = None;
    let mut wait = 2500u64;
    let mut steps = String::new();
    let mut cmd: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out = it.next().cloned(),
            "--wait" => wait = it.next().and_then(|w| w.parse().ok()).unwrap_or(wait),
            "--do" => steps = it.next().cloned().unwrap_or_default(),
            "--" => {
                cmd = it.by_ref().cloned().collect();
                break;
            }
            other => {
                eprintln!("kitab-bench shot: unknown flag {other}");
                return 2;
            }
        }
    }
    let (Some(out), Some((exe, rest))) = (out, cmd.split_first()) else {
        eprintln!("usage: kitab-bench shot --out FILE.png [--wait MS] -- EXE [ARGS…]");
        return 2;
    };
    let mut child = match Command::new(exe)
        .args(rest)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("kitab-bench shot: could not start {exe}: {e}");
            return 1;
        }
    };
    std::thread::sleep(Duration::from_millis(wait));
    let result = window_of(child.id())
        .ok_or_else(|| "no window".to_string())
        .and_then(|w| {
            let hwnd = w.id().map_err(|e| e.to_string())? as isize;
            run_steps(hwnd, &steps)?;
            w.capture_image().map_err(|e| e.to_string())
        })
        .and_then(|img| img.save(&out).map_err(|e| e.to_string()));
    stop(&mut child);
    match result {
        Ok(()) => {
            eprintln!("saved {out}");
            0
        }
        Err(e) => {
            eprintln!("kitab-bench shot: {e}");
            1
        }
    }
}
