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

## Memory and idle CPU deep dive (7 November 2026)

Owner's targets: idle CPU 0 % (1–2 % at most), memory under 50 MB (100–120 MB at worst).

### Which number is "memory"

- **Task Manager memory** = private working set: memory only this app holds in RAM right now. What users see.
- **Working set** = all RAM the process touches, including shared DLL pages.
- **Private bytes** = commit charge: memory the process has committed, in RAM or the page file. The strictest.

SumatraPDF keeps rendered pages in page-file-backed mapped sections (`CreateFileMapping` + `CreateDIBSection`, `EngineMupdf.cpp` at tag 3.6.1rel), which count in its working set but not its private bytes, so compare all three.

### Where Kitab's memory goes (spike, 6 s after start, this machine)

| Configuration | Task Manager | Working set | Private bytes | Threads | Idle CPU |
|---|---|---|---|---|---|
| SumatraPDF 3.6.1 | 5.6 MB | 40.2 MB | 14.6 MB | | 0 % |
| GDI + MuPDF only (`--splash-only`), page shown, no GPUI | | 21.5 MB | **9.0 MB** | 5 | 0 % |
| GPUI only, no MuPDF, NVIDIA | | 60.6 MB | 113.8 MB | 91 | 0.9 % |
| GPUI only, no MuPDF, AMD iGPU | | 130.4 MB | 202.1 MB | 68 | 0.6 % |
| Kitab no fixes, 2 pages | 36.1 MB | 66.8 MB | 129.7 MB | 91 | |
| **Kitab NVIDIA lean, 2 pages** | **28.8 MB** | 68.9 MB | **107.1 MB** | 91 | 0.3 % |
| Kitab AMD lean, 2 pages | 50.8 MB | 130.9 MB | 186.0 MB | 68 | 0.6 % |
| Kitab WARP lean, 2 pages | 33.1 MB | 77.4 MB | 59.0 MB | 33 | 0.6 % |

Findings:

1. **MuPDF and GDI are lean**: page 1 on screen for 9 MB private, less than SumatraPDF. The cost is GPUI's Direct3D 11 device, the vendor driver and window-sized GPU surfaces.
2. **The GPU choice decides most of it.** On the AMD iGPU textures live in system memory and count as Kitab's; on the NVIDIA dGPU they are in VRAM. WARP (software Direct3D) loads no vendor driver: 39 MB private with no pages, but it burns about 14 cores for the first second (shader compilation) and costs CPU on every frame, so it is the "hardware acceleration off" fallback, not the default.
3. **`KITAB_LEAN=1`** (single-sampled path texture instead of a window-sized 4× MSAA one, 2 swap-chain buffers instead of 3): GPUI-only floor 113 → 87 MB on NVIDIA, 198 → 149 MB on AMD, 65 → 39 MB on WARP.
4. **Segment heap** in the manifest: about 1 MB here, because the remaining memory is driver and GPU memory, not the process heap. Kept, as the app's own allocations will grow.
5. **Each full-resolution page costs about 9–10 MB** (a CPU copy kept by GPUI's `RenderImage` plus the GPU texture). Phase 1 renders visible pages only, into a byte-bounded cache, and drops the CPU copy after upload.
6. **Idle CPU reaches 0 % within about 5 s** on NVIDIA and AMD. Before that, frames after start-up cost 2–5 %. GPUI's vsync thread invalidates every window on every refresh; making it demand-driven (zed PR #63182 measured 4.7 % → 0.02 %) removes the remaining wake-ups.

### Next fixes, in order

1. Render only visible pages or tiles, keep no CPU copy after upload, and free atlas textures on eviction (`drop_image`).
2. Demand-driven frames: patch `begin_vsync_thread` (`platform/windows/platform.rs`) to wait until a window asks for a frame, and skip minimised windows.
3. Create path textures only on frames that draw paths.
4. On minimise or long idle: `ClearState` + `IDXGIDevice3::Trim`, drop atlas pages.
5. Cap MuPDF's store (256 MB by default; one-line `mupdf-sys` patch) and call `fz_shrink_store` when idle.
6. Choose the default adapter by measurement on more machines: dGPU (less private memory, may wake a sleeping GPU) against iGPU (more private memory, lower power).

## Internal trace marks (Kitab only)

From ADR 0001 (milliseconds from `main`, 5 runs): window opened 262–275 ms, page 1 rendered by MuPDF 3–4 ms later, first page painted 281–290 ms. About 250 ms of that is GPUI's Windows platform set-up (D3D11 device created twice, DirectWrite) before the window exists.

## Kitab 0.1 against SumatraPDF (8 November 2026)

Same machine. `kitab-bench s2 --runs 15 --warmup 2`, medians. Kitab: release build with the static C runtime, no stand-in window (ADR 0002 update). The bench now ignores cloaked windows and also reports when page content is first seen in a capture (stable or not), because Kitab's window appears with the page already drawn and Windows' open animation changes its pixels for about 150 ms, which the "two identical captures" rule waits out. SumatraPDF's captures cost about 150 ms each, so its page times are coarse.

| File | App | Window on screen | Page first seen | Page stable | CPU to page | Working set | Private | CPU in the 2 s after |
|---|---|---|---|---|---|---|---|---|
| 01-text-outline.pdf (40 pages) | Kitab | 201 ms | 231 ms | 326–385 ms | 312 ms | 106.6 MB | 155.0 MB | 3.9 % |
| | SumatraPDF 3.6.1 | 76 ms (blank) | ≤ 285 ms | 76–298 ms | 218 ms | 42.7 MB | 15.0 MB | 0.0 % |
| 06-large-20mb.pdf (300 images) | Kitab | 203 ms | 243 ms | 321–393 ms | 296 ms | 106.7 MB | 155.2 MB | 4.0 % |
| | SumatraPDF 3.6.1 | 80 ms (blank) | ≤ 449 ms | 80–449 ms | 234 ms | 87.1 MB | 37.5 MB | 0.0 % |

- Steady idle (after the first seconds) is 0 % with occasional single timer ticks; the "2 s after" column is start-up work settling (outline, bookmarks, the reader's neighbouring pages).
- Start-up trace (`KITAB_TRACE_START=1`, `kitab_os::trace`): GPUI's `Application::new` takes about 110 ms of wall and CPU time before any window exists (Direct3D device and DirectWrite set-up); MuPDF opens the file in about 2 ms and renders page 1 in about 20 ms.
- Fixed in this round: each visible page was rendered two or three times on open (a relayout at start-up cancelled renders in flight, and a soft preview ran before any page had been timed). Before: about 300 ms CPU at first paint, 129 MB working set, 186 MB private. After: about 205–235 ms CPU at first paint (trace), 107 MB, 155 MB.

### After the memory and CPU round (8 November 2026)

| File | App | Window on screen | Page stable | CPU to page | Working set | Private | CPU in the 2 s after |
|---|---|---|---|---|---|---|---|
| 01-text-outline.pdf | Kitab | 199 ms (with the page) | 199–225 ms | 203 ms | 96.8 MB | 144.9 MB | 4.7 % |
| | SumatraPDF 3.6.1 | 90 ms (blank) | 90–312 ms | 203 ms | 42.6 MB | 14.9 MB | 0.0 % |
| 06-large-20mb.pdf | Kitab | 196 ms (with the page) | 196–228 ms | 203 ms | 96.1 MB | 144.5 MB | 5.5 % |
| | SumatraPDF 3.6.1 | 88 ms (blank) | 88–438 ms | 218 ms | 87.0 MB | 37.4 MB | 0.0 % |

- The 150 ms between the window showing and the page being stable was Kitab's own tab and dock entrances resuming after the reveal (GPUI times an animation from its first frame with whatever duration it's given later), not Windows' open animation. Entrances begun off screen now stay finished (`motion.rs`): 2 frames after the reveal instead of about 30.
- 120 ms of the CPU before any window is Direct3D device creation (the GPU driver starting) inside GPUI's `Application::new`, measured with temporary marks in gpui's platform set-up. SumatraPDF draws with GDI and doesn't pay it.
- Memory: the page ahead renders only after the first scroll (about 11 MB at open), the page cache budget is 64 MB, and inactive tabs release pages after 20 s in the background.

## Reading against SumatraPDF, and the idle-CPU round (8 November 2026)

`python tools/bench_reading.py` (new): both apps in isolation in a 1200×800 window, `01-text-outline.pdf`, sampled from outside 4 s after start, 10 s later, after 60 wheel notches (3 s) and 15 s after that. CPU is the share of one core since the previous sample. NVIDIA RTX 5070 (Kitab's default adapter here; the AMD iGPU costs 60–100 MB more private, its textures live in system memory).

| | Working set | Private | CPU idle | CPU scrolling | CPU idle after |
|---|---|---|---|---|---|
| SumatraPDF 3.6.1 | 34 MB | 15 MB | 0.0 % | 2.0 % | 0.0 % |
| Kitab before | 118 MB (133 after scrolling) | 137 MB (172 after scrolling) | 2.2 % | 8.7 % | — |
| Kitab after | 109 MB (114 after scrolling) | 129 MB (156 after scrolling) | 0.0–0.5 % | 2.4–7 % | 0.2 % |

- **Frames on demand** (gpui patch 9): GPUI's vsync thread woke every window on every display refresh, idle or not. It now wakes them only when a window changed or an animation waits for a frame, and rests otherwise. Idle CPU 2.2 % → about 0 %, and scrolling costs a third of what it did.
- **Page cache floor 96 → 32 MB:** the cache still always holds the pages on screen and the next three (the flicker fix); only how many pages further away it keeps went down. −18 MB private after scrolling.
- **MuPDF store trimmed to 48 MB after each render** (its default limit is 256 MB). It made no difference on these files; it bounds image-heavy documents.
- **What's left:** with nothing open Kitab holds about 88 MB private and 60 MB working set: GPUI's Direct3D 11 device, the NVIDIA user-mode driver and DirectWrite. SumatraPDF draws with GDI and has no such floor. A document adds about 44 MB, mostly rendered pages (each kept both as pixels in RAM and as a GPU texture).
