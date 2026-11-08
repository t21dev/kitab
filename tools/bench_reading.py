#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Reading scenario, Kitab against SumatraPDF (Windows): memory and CPU while
a document is open, read and left alone.

    python tools/bench_reading.py [--file tests/fixtures/pdf/01-text-outline.pdf]
                                  [--sumatra "C:/Program Files/SumatraPDF/SumatraPDF.exe"]
                                  [--only kitab|sumatra] [--json out.json]

Each app runs in isolation (Kitab: its own APPDATA and single-instance name;
SumatraPDF: -appdata in a temp folder), in a 1200×800 window, driven by
kitab-bench input steps posted to its own window:

    open        4 s after start, the first page shown
    idle_10s    10 s later, untouched
    scrolled    after 60 wheel notches down (about 3 s of reading)
    settled     15 s after the scrolling stopped

At each mark: working set, private bytes, and the CPU time used since the
previous mark (user + kernel, all threads), as a percentage of one core.
Needs a release build of kitab-bench. Nothing outside the temp folders is
touched.
"""
import argparse
import ctypes
import ctypes.wintypes as W
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
k32 = ctypes.windll.kernel32
psapi = ctypes.windll.psapi


class PMC(ctypes.Structure):
    _fields_ = [("cb", W.DWORD), ("PageFaultCount", W.DWORD)] + [
        (n, ctypes.c_size_t) for n in [
            "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
            "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage",
            "PagefileUsage", "PeakPagefileUsage", "PrivateUsage"]]


def sample(pid):
    h = k32.OpenProcess(0x0410, False, pid)
    pmc = PMC()
    pmc.cb = ctypes.sizeof(PMC)
    psapi.GetProcessMemoryInfo(h, ctypes.byref(pmc), pmc.cb)
    c, e, kt, ut = (W.FILETIME() for _ in range(4))
    k32.GetProcessTimes(h, ctypes.byref(c), ctypes.byref(e), ctypes.byref(kt), ctypes.byref(ut))
    k32.CloseHandle(h)
    ft = lambda f: (f.dwHighDateTime << 32 | f.dwLowDateTime) / 1e7
    return pmc.WorkingSetSize / 2**20, pmc.PrivateUsage / 2**20, ft(kt) + ft(ut)


def child(parent, name, timeout=20):
    end = time.time() + timeout
    while time.time() < end:
        out = subprocess.run(
            ["powershell", "-NoProfile", "-Command",
             f"(Get-CimInstance Win32_Process -Filter \"ParentProcessId={parent} and Name='{name}'\").ProcessId"],
            capture_output=True, text=True).stdout.split()
        if out:
            return int(out[0])
        time.sleep(0.2)
    raise SystemExit(f"{name} didn't start")


def run(app, exe, doc, work):
    env = dict(os.environ)
    args = [str(exe)]
    if app == "kitab":
        ad = work / "kitab-appdata"
        (ad / "t21" / "Kitab").mkdir(parents=True, exist_ok=True)
        env.update(APPDATA=str(ad), USERNAME="kitabreadbench")
    else:
        ad = work / "sumatra-appdata"
        ad.mkdir(exist_ok=True)
        args += ["-appdata", str(ad)]
    args.append(str(doc))
    bench = ROOT / "target" / "release" / "kitab-bench.exe"
    scroll = ";".join(["wheel:700,450,-1;wait:50"] * 60)
    steps = f"size:1200,800;wait:14000;{scroll};wait:22000"
    b = subprocess.Popen([str(bench), "shot", "--out", str(work / f"{app}.png"), "--wait", "4000",
                          "--do", steps, "--", *args], env=env,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    pid = child(b.pid, pathlib.Path(exe).name)
    t0 = time.time()
    rows = []
    last_cpu = 0.0
    last_t = t0

    def mark(name, at):
        nonlocal last_cpu, last_t
        time.sleep(max(0.0, at - (time.time() - t0)))
        ws, pv, cpu = sample(pid)
        now = time.time()
        pct = (cpu - last_cpu) / max(now - last_t, 1e-6) * 100
        rows.append((name, ws, pv, cpu, pct))
        last_cpu, last_t = cpu, now

    # The bench waits 4 s, sizes the window (+150 ms), waits 14 s, then
    # 60 × (wheel + 50 ms); wheel and wait steps add no pause of their own.
    mark("open", 4.5)
    mark("idle_10s", 14.5)
    scroll_end = 4.0 + 0.15 + 14.0 + 60 * 0.05
    mark("scrolled", scroll_end + 0.5)
    mark("settled", scroll_end + 15.5)
    try:
        b.wait(timeout=40)
    except subprocess.TimeoutExpired:
        b.kill()
    subprocess.run(["taskkill", "/F", "/PID", str(pid)], capture_output=True)
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--file", default=str(ROOT / "tests" / "fixtures" / "pdf" / "01-text-outline.pdf"))
    ap.add_argument("--kitab", default=str(ROOT / "target" / "release" / "kitab.exe"))
    ap.add_argument("--sumatra", default=r"C:\Program Files\SumatraPDF\SumatraPDF.exe")
    ap.add_argument("--only", choices=["kitab", "sumatra"])
    ap.add_argument("--json")
    a = ap.parse_args()
    work = pathlib.Path(tempfile.mkdtemp(prefix="kitab-read-"))
    out = {}
    try:
        for app, exe in [("kitab", a.kitab), ("sumatra", a.sumatra)]:
            if a.only and a.only != app:
                continue
            out[app] = run(app, exe, pathlib.Path(a.file).resolve(), work)
            time.sleep(2)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    print(f"{pathlib.Path(a.file).name}")
    print(f"{'app':<9}{'step':<10}{'working set':>13}{'private':>10}{'CPU':>8}")
    for app, rows in out.items():
        for name, ws, pv, _, pct in rows:
            print(f"{app:<9}{name:<10}{ws:>10.1f} MB{pv:>7.1f} MB{pct:>7.1f}%")
    if a.json:
        pathlib.Path(a.json).write_text(json.dumps(
            {app: [{"step": n, "working_set_mb": round(ws, 1), "private_mb": round(pv, 1),
                    "cpu_percent": round(p, 2)} for n, ws, pv, _, p in rows]
             for app, rows in out.items()}, indent=2))


if __name__ == "__main__":
    main()
