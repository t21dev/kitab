# Performance baseline

The reference numbers every start-up change is compared against (PLAN.md §6). Each row says exactly how it was measured. Nothing here is an estimate.

## Machine

| | |
|---|---|
| CPU | AMD Ryzen 9 9900X (12 cores) |
| GPU | AMD Radeon (integrated) + NVIDIA GeForce RTX 5070 |
| RAM | 62 GB |
| OS | Windows 11 Pro, build 26300 |
| SumatraPDF | 3.6.1, installed (`C:\Program Files\SumatraPDF`) |
| Kitab | Spike A (`spikes/spike-a`, GPUI 0.2.2 from crates.io, mupdf 0.8.0, release build) |

## S2 · process start → first page on screen

`kitab-bench s2 --runs 15 --warmup 2`, file `tests/fixtures/pdf/01-text-outline.pdf`, 7 November 2026. Warm starts (both apps had run before). Median of 15 runs.

| App | Window appears | First page on screen | Capture cost |
|---|---|---|---|
| Kitab (Spike A) | **263 ms** | between 263 and 325 ms | 25 ms |
| SumatraPDF 3.6.1 | **61 ms** | between 61 and 319 ms | 118 ms |

How `kitab-bench` measures: it starts the app with the file, finds the app's own top-level window, and captures that window (PrintWindow, full content: the window's own pixels, never the screen). The clock stops when the middle of the window shows content that is identical in two captures in a row. A page time is a bracket: the last capture without the page and the first with it.

### What this does and does not show

- **Window appears** is precise to about 1 ms (a window poll, no capture). Kitab shows its window **about 4× later** than SumatraPDF.
- **First page** cannot be ranked yet. Capturing SumatraPDF's window makes it repaint everything, which costs about 118 ms per capture, so its page bracket is wide. Both brackets overlap.
- Windows Graphics Capture was tried to get finer resolution. It works on SumatraPDF but never delivers a frame for a GPUI window that draws once, so it can't measure Kitab.
- Capturing the screen instead of the window was tried and rejected: a newly started window is often not in front, so screen pixels show other apps (and their private content) and produce false timings.

**Next improvement to the harness:** a cheaper per-window paint signal (for example a DWM frame counter or an accessibility event per window) to tighten the page brackets for both apps. Until then, S2 comparisons with SumatraPDF use the window-appears column and Kitab's internal trace marks (ADR 0001), never a page midpoint.

## Internal trace marks (Kitab only)

From ADR 0001 (milliseconds from `main`, 5 runs): window opened 262–275 ms, page 1 rendered by MuPDF 3–4 ms later, first page painted 281–290 ms. About 250 ms of that is GPUI's Windows platform set-up (D3D11 device created twice, DirectWrite) before the window exists.
