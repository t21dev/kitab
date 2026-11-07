#!/usr/bin/env python3
"""Large-library memory scenario (Windows): what Kitab holds with hundreds of
documents on Home, and whether it gives memory back while you only read.

    python tools/bench_library.py [--exe target/release/kitab.exe] [--docs 400]

Makes N distinct PDFs from the test fixtures in a temp folder, starts an
isolated Kitab (own APPDATA, own single-instance name) so it creates its
library, adds the folder as watched, then starts it again under
kitab-bench (whose input steps drive it: the start-up scan finds the
documents, Home › All shows them, the list scrolls, a document opens) and
samples its memory from outside at each step:

    home_all       Home showing the All shelf (every document)
    home_scrolled  after scrolling Home down and back up
    doc_open       a document open in a tab
    reading_60s    a minute later, only reading

Prints a table; --json writes it too. Nothing outside the temp folders is
touched. Needs a release build of kitab-bench.
"""
import argparse
import ctypes
import ctypes.wintypes as W
import json
import os
import pathlib
import shutil
import sqlite3
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "tests" / "fixtures" / "pdf"
SOURCES = ["01-text-outline.pdf", "02-mixed-page-sizes.pdf", "03-forms.pdf",
           "10-scanned-and-images.pdf", "07-unicode-rtl.pdf"]

k32 = ctypes.windll.kernel32
psapi = ctypes.windll.psapi


class PMC(ctypes.Structure):
    _fields_ = [("cb", W.DWORD), ("PageFaultCount", W.DWORD)] + [
        (n, ctypes.c_size_t) for n in [
            "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
            "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage",
            "PagefileUsage", "PeakPagefileUsage", "PrivateUsage"]]


def memory(pid):
    h = k32.OpenProcess(0x0410, False, pid)
    pmc = PMC()
    pmc.cb = ctypes.sizeof(PMC)
    psapi.GetProcessMemoryInfo(h, ctypes.byref(pmc), pmc.cb)
    k32.CloseHandle(h)
    return pmc.WorkingSetSize / 2**20, pmc.PrivateUsage / 2**20


def child_kitab(parent, timeout=20):
    """The kitab.exe that kitab-bench started."""
    end = time.time() + timeout
    while time.time() < end:
        out = subprocess.run(
            ["powershell", "-NoProfile", "-Command",
             f"(Get-CimInstance Win32_Process -Filter \"ParentProcessId={parent} and Name='kitab.exe'\").ProcessId"],
            capture_output=True, text=True).stdout.split()
        if out:
            return int(out[0])
        time.sleep(0.3)
    raise SystemExit("kitab didn't start")


def make_library(folder, n):
    folder.mkdir(parents=True, exist_ok=True)
    data = [(FIXTURES / s).read_bytes() for s in SOURCES if (FIXTURES / s).exists()]
    for i in range(n):
        # A trailing comment after %%EOF: a different file, the same pages.
        (folder / f"doc-{i:04}.pdf").write_bytes(data[i % len(data)] + f"\n% kitab bench {i}\n".encode())


def run(args):
    work = pathlib.Path(tempfile.mkdtemp(prefix="kitab-lib-"))
    lib = work / "library"
    make_library(lib, args.docs)
    appdata = work / "appdata"
    (appdata / "t21" / "Kitab").mkdir(parents=True)
    env = dict(os.environ, APPDATA=str(appdata), USERNAME="kitabbench")
    if args.keep_trace:
        env["KITAB_TRACE_START"] = "1"
    exe = str(pathlib.Path(args.exe).resolve())
    bench = str(ROOT / "target" / "release" / "kitab-bench.exe")

    # First run: the library database. Then the watched folder.
    p = subprocess.Popen([exe], env=env)
    time.sleep(4)
    p.kill()
    p.wait()
    con = sqlite3.connect(appdata / "t21" / "Kitab" / "library.sqlite3")
    con.execute("INSERT OR IGNORE INTO watched_folders(path, added_at) VALUES (?, ?)",
                (str(lib), int(time.time() * 1000)))
    con.commit()
    con.close()

    # The timeline kitab-bench plays; memory is read at the marks.
    scroll = ";".join(["wheel:700,600,-15;wait:250"] * 6 + ["wheel:700,600,15;wait:250"] * 6)
    steps = (f"size:1400,900;wait:{int(args.scan * 1000)};click:22,22;wait:800;"
             f"click:128,145;wait:4500;{scroll};wait:90000")
    shot = work / "end.png"
    b = subprocess.Popen([bench, "shot", "--out", str(shot), "--wait", "2500", "--do", steps,
                          "--", exe], env=env, stdout=subprocess.DEVNULL)
    pid = child_kitab(b.pid)
    rows = []
    # 2.5 s start + scan + clicks + 3 s for All (and 150 ms per input step).
    time.sleep(2.5 + args.scan + 0.8 + 3.0 + 0.6)
    rows.append(("home_all", *memory(pid)))
    time.sleep(1.5 + 12 * (0.25 + 0.15) + 3.0)
    rows.append(("home_scrolled", *memory(pid)))
    # Open one: a second Kitab hands the file to this one.
    subprocess.run([exe, str(lib / "doc-0000.pdf")], env=env, timeout=20)
    time.sleep(4)
    rows.append(("doc_open", *memory(pid)))
    for i in range(12):
        time.sleep(5)
        if args.curve:
            ws, pv = memory(pid)
            print(f"  +{(i + 1) * 5:>2}s  working set {ws:.0f} MB  private {pv:.0f} MB", flush=True)
    rows.append(("reading_60s", *memory(pid)))
    subprocess.run(["taskkill", "/F", "/PID", str(pid)], capture_output=True)
    try:
        b.wait(timeout=30)
    except subprocess.TimeoutExpired:
        b.kill()
    if args.keep:
        print(f"kept: {work}")
    else:
        shutil.rmtree(work, ignore_errors=True)

    print(f"{args.docs} documents")
    print(f"{'step':<15}{'working set':>14}{'private':>12}")
    for name, ws, pv in rows:
        print(f"{name:<15}{ws:>11.0f} MB{pv:>9.0f} MB")
    if args.json:
        pathlib.Path(args.json).write_text(json.dumps(
            [{"step": n, "working_set_mb": round(a, 1), "private_mb": round(b2, 1)}
             for n, a, b2 in rows], indent=2))


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", default=str(ROOT / "target" / "release" / "kitab.exe"))
    ap.add_argument("--docs", type=int, default=400)
    ap.add_argument("--scan", type=float, default=8.0, help="seconds to wait for the start-up scan")
    ap.add_argument("--json")
    ap.add_argument("--curve", action="store_true", help="print memory every 5 s while reading")
    ap.add_argument("--keep-trace", action="store_true")
    ap.add_argument("--keep", action="store_true", help="keep the library and APPDATA folders")
    run(ap.parse_args())
