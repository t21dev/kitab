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

## Start-up fixes, measured (7 November 2026)

Same machine and method, now with CPU and memory. `kitab-bench s2 --runs 15 --warmup 2`, median. CPU to page = user + kernel time of all threads until the page was found; memory and idle CPU are read 2 s after the page.

| Configuration | Window appears | Page on screen | CPU to page | Working set | Private | Idle CPU |
|---|---|---|---|---|---|---|
| SumatraPDF 3.6.1 | 62 ms | 62–327 ms | 187 ms | 40.4 MB | 14.9 MB | 0.0 % |
| Spike, no fixes (40 pages) | 264 ms | 264–326 ms | 390 ms | 261.6 MB | 519.6 MB | 3.9 % |
| Spike, font + GPU fixes (40 pages) | 133 ms | 133–189 ms | 265 ms | 259.5 MB | 518.0 MB | 4.7 % |
| Spike, fixes + GDI splash (40 pages) | **21 ms** | **21–82 ms** | 171 ms | 259.3 MB | 516.4 MB | 8.6 % |
| Spike, fixes + GDI splash (2 pages) | **21 ms** | **21–83 ms** | 156 ms | 77.1 MB | 142.9 MB | 3.1 % |

- **Font fix:** `KITAB_FONT_NO_UPDATE_CHECK=1` makes GPUI's start-up `GetSystemFontCollection` skip re-checking every font file (from stofu5678/ume_editor#23).
- **GPU fix:** `KITAB_GPU=high-performance` picks the adapter with `EnumAdapterByGpuPreference` and skips GPUI's throwaway probe device. On this machine the RTX 5070 (awake) started faster than the AMD integrated GPU (`low-power`: window 162 ms with the font fix). A sleeping discrete GPU on a laptop would reverse that, so the default must be measured on more machines.
- **GDI splash:** the SumatraPDF approach. A plain Win32 window on its own thread paints page 1 with `SetDIBitsToDevice` 19–21 ms after `main`, while GPUI sets up; GPUI takes over at about 180 ms (`first_page_painted`). The benchmark's saved capture confirms the window showed the real page.
- **Memory:** the spike renders every page at full resolution up front (40 pages ≈ 520 MB private). With 2 pages it is 143 MB private, still about 10× SumatraPDF: GPU device and driver memory, GPUI's atlas and MuPDF. Phase 1 renders visible pages only, with a byte-bounded tile cache.
- **Idle CPU:** 3–9 % for the spike against 0 % for SumatraPDF. Something keeps running after the page is shown; Phase 1 must reach 0 % idle.

Patches: `spikes/spike-a/patch_gpui.py` (applied to a local copy of gpui 0.2.2). Splash: `spikes/spike-a/src/splash.rs`.

## Internal trace marks (Kitab only)

From ADR 0001 (milliseconds from `main`, 5 runs): window opened 262–275 ms, page 1 rendered by MuPDF 3–4 ms later, first page painted 281–290 ms. About 250 ms of that is GPUI's Windows platform set-up (D3D11 device created twice, DirectWrite) before the window exists.
