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


## 0.6.0 (main) against SumatraPDF, re-run (15 November 2026)

Same machine (NVIDIA driver 621.24). Kitab: `main` at 3e99a61, built like a release (`tools/build_release.py`, static C runtime), each app in its own temporary profile. SumatraPDF 3.6.1.

### Start-up: `kitab-bench s2 --runs 15 --warmup 2`, medians

| File | App | Window on screen | Page first seen | CPU to page | Working set | Private | CPU in the 2 s after |
|---|---|---|---|---|---|---|---|
| 01-text-outline.pdf | Kitab | 241 ms (with the page) | 268 ms | 234 ms | 98.0 MB | 109.4 MB | 2.4 % |
| | SumatraPDF 3.6.1 | 68 ms (blank) | ≤ 359 ms | 187 ms | 44.1 MB | 15.0 MB | 0.0 % |
| 06-large-20mb.pdf | Kitab | 247 ms (with the page) | 279 ms | 265 ms | 98.6 MB | 110.4 MB | 4.7 % |
| | SumatraPDF 3.6.1 | 68 ms (blank) | ≤ 374 ms | 171 ms | 74.2 MB | 37.2 MB | 0.0 % |

- **Start-up is about 40 ms later than on 8 November, under every version, not only this one.** The released 0.5.0 (taken from its Store package) measured 232–233 ms to the window in two runs today, and `main` 239 ms, in fresh profiles: the same within noise. The start-up trace (`KITAB_TRACE_START=1`) puts the difference before any Kitab code: GPUI's `Application::new` (Direct3D device, DirectWrite) took 155–158 ms today against 110–118 ms in November's traces, and the process starts with 30–47 ms of CPU already used before `main` (0 then). Kitab's own part, from GPUI up to the window, is about 70 ms, with larger pages drawn than in November's runs.
- **Memory is lower:** 109 MB private at open, against 145 MB on 8 November.

### Reading: `python tools/bench_reading.py`, 01-text-outline.pdf, 1200×800

| | Working set | Private | CPU idle (10 s) | CPU scrolling | CPU settled |
|---|---|---|---|---|---|
| SumatraPDF 3.6.1 | 32 MB | 15 MB | 0.0 % | 1.3 % | 0.0 % |
| Kitab 0.6.0 (three runs) | 89 MB (84–92 after scrolling) | 100 MB (114 after scrolling, 119–123 at the peak) | 0.0–0.8 % | 9.6–13.1 % | 0.0–0.3 % |
| Kitab, 8 November | 109 MB (114 after scrolling) | 129 MB (156 after scrolling) | 0.0–0.5 % | 2.4–7 % | 0.2 % |

- Memory is 20–40 MB lower than in November, before and after scrolling.
- Idle stays at 0 %: frames on demand still hold.

### Scrolling, traced and fixed (same day)

The scrolling numbers above weren't scrolling. The benchmark posts wheel messages to a window that has never had the keyboard, and GPUI took Ctrl from the window thread's key state (`GetKeyState`), which reported Ctrl held for the first 11 of 60 notches: they zoomed the page out to 10 %, and the rest scrolled 40 tiny pages that barely moved. The same can happen to a person: let go of Ctrl while another window has the keyboard, then wheel over Kitab (Windows wheels the window under the pointer). Fixed in gpui patch 14: Ctrl and Shift come from the wheel message's own flags (`MK_CONTROL`, `MK_SHIFT`). SumatraPDF reads the key state too, so its earlier 1.3 % was likely the same zoom.

With real scrolling (61 % zoom, page 1 to 6 in 3.8 s), per-thread CPU showed the UI thread doing nearly all of it: about 660 ms of the 3.8 s against 80–140 ms on the render threads. The screens here refresh at 160–164 Hz and smooth scrolling draws a frame per refresh; GPUI builds the whole window each frame. Temporary timers put Kitab's own render functions at about a third of a frame and GPUI's layout at most of the rest; closing the sidebar halved the UI thread (about 370 ms). The contents list was rebuilt, every row of it, on every frame.

- **The contents list is a cached view** (`TocView`): reused while the outline, the page, folds and the look are unchanged. Over a 649-frame scroll it was built 16 times (the page changing). UI thread during the scroll: about 660 ms → 344–547 ms.

`python tools/bench_reading.py`, after both fixes, 01-text-outline.pdf, 1200×800:

| | Working set | Private | CPU idle (10 s) | CPU scrolling | CPU settled |
|---|---|---|---|---|---|
| SumatraPDF 3.6.1 | 32 MB | 16 MB | 0.0 % | 6.3 % | 0.0 % |
| Kitab 0.6.0 (three runs) | 90–93 MB (81–94 after scrolling) | 102–105 MB (116–117 after scrolling, 121–123 at the peak) | 0.2–0.5 % | 7.6–8.3 % | 0.1–0.7 % |


## Speed and memory round after 0.6.0 (15 November 2026)

Fifteen candidates from a code read; each was measured against the build before it and kept only if its number improved (Windows 11, release build, 1200×800 window, fresh profile per run, 160–164 Hz screens). UI thread = CPU time of the main thread over the scenario.

| Change | Scenario | Before | After | Kept |
|---|---|---|---|---|
| PDF page sizes from the page tree (no page loads at open) | Open a 1,251-page textbook, 3 runs | open 51–56 ms, 285 MB private | open 18–21 ms, 257 MB | yes (`81cf2d6`) |
| Link tooltips and footnote checks worked out once per page | Pointer sweep over a 255-link contents page, 3 s, 5 runs | UI median 469 ms | 328 ms | yes |
| Deep-zoom tiles keep only their GPU copy | 640 % zoom and a short scroll, 3 runs | ~241 MB private, ~158 MB working set | ~222 MB, ~138 MB | yes |
| Home covers keep only their GPU copy | 3,000-book All shelf, 40 notches, 3 runs | 97–98 MB private | 92 MB | yes |
| Find highlights painted by one element per page | Scroll with Find "e" (8,245 hits), 5 runs | UI median 578 ms | 469 ms (406 with Find closed) | yes (`b151a56`) |
| Pen strokes tessellated once (gpui `Path::kitab_offset`) | 60 strokes on a page, scroll wiggle, 9 runs | UI median 344 ms | 297 ms | yes (`8032b2b`) |
| Shared themes in Style | Clone timing | 336 ns per clone | 65 ns (≈3 µs per frame) | no: too small to see |
| Contents cache keyed by a counter, not an outline hash | Hash timing | 0.5 µs (30 headings) to 55 µs (3,000) per frame | — | no: 5 of 2,172 real books exceed 500 headings |
| Linear unsaved-edit count | 50–1,000 edits | 1.4–29 µs | 1.7–24 µs | no |
| Window chrome as cached views | Scroll with each part off (sidebar closed), 3 runs | all on 359 ms | status bar off 375, dock tucked 328, title bar off 375 | not built: no part stands out of the noise (Focus mode, all off at once: 383 → 266 ms) |
| Thumbnails in their own cache | Scroll 25 pages and back, count page redraws | 5 with Contents | 5 with Pages | not built: thumbnails don't evict pages |
| Open the file before GPUI starts | Start-up trace | engine open 1.3 ms (small PDF), 19 ms (1,251 pages) | — | not built: at most 19 ms, only on double-click |

Measured, nothing to fix:
- **CPU before `main`:** 0 or 15.6 ms, one tick of Windows' 15.6 ms process clock. The 30–47 ms seen once came from fresh builds' first launches.
- **Page uploads at 160 Hz:** no frame over 6.25 ms while scrolling, at 61 % (722×1022 pages, slowest frame 5.1 ms) or 200 % (1786×2526, slowest 4.8 ms).
- **Small images pinning page textures:** the atlas does pack small images into page textures' spare room (`directx_atlas.rs` `allocate`), but scrolling 40 pages with the Pages pane open holds 12 MB more GPU memory than without, about 9 MB of it the thumbnails themselves.

## Rendering round from PdfCraft's ideas (17 November 2026)

Three changes, measured against the released 0.8.1 (its portable build, `portable` marker removed so both run in a fresh profile) on the same machine:

- **Renders nobody wants stop part-way** (`kitab-mupdf/src/abort.rs`): each render carries a MuPDF cookie, and a watchdog sets its abort flag once the page has left the view (± 2 pages) or the zoom changed.
- **Only slow pages wait out a zoom**: a page whose sharp render took 250 ms or more keeps its stretched picture until zooming has paused 350 ms; a pinch settles after 60 ms (was 140 ms) for the rest.
- **The pages on screen first**: thumbnails, link and field loading and the page ahead wait while a render for a page on screen is still out.

### Heavy pages: `heavy.pdf`, 40 pages of 12,000 curves each (about 1.4 s to draw a page at 107 %), 1765×1294, `KITAB_TRACE_RENDER=1`

Render work is the sum of the finished renders' times after the input started; "sharp at" is when the last sharp render finished, from the first input.

| Scenario | Version | Render work (runs) | Renders stopped | Final page sharp at (runs) |
|---|---|---|---|---|
| Page Down × 12, 150 ms apart | 0.8.1 | 15.7, 21.6, 22.2, 22.2 s (median 21.9) | 2 | 4.4, 5.7, 5.7, 5.7 s (median 5.7) |
| | new | 12.9, 13.0, 13.0, 16.4, 17.3 s (median 13.0) | 3–4 | 5.2, 5.3, 5.3, 5.6, 6.4 s (median 5.3) |
| Zoom in × 6 (dock +), 150 ms apart | 0.8.1 | 6.7, 6.9, 9.7 s | 12 dropped before starting | 4.1, 4.4, 5.7 s |
| | new | 5.0, 5.0, 6.7 s | 7 (stopped part-way) | 3.7, 3.7, 4.5 s |

- Paging through heavy pages: about 40 % less render work (median 21.9 → 13.0 s), the final page sharp about 0.4 s sooner (median).
- Zooming a heavy page: about 25–30 % less work, sharp 0.4–1.2 s sooner.

### Ordinary reading: `python tools/bench_reading.py`, 1200×800, three runs each, interleaved

| File | Version | Working set at open | Private at open | CPU scrolling | Private peak | Private settled |
|---|---|---|---|---|---|---|
| 01-text-outline.pdf | 0.8.1 | 95.9–96.9 MB | 107.2–107.8 MB | 9.4–10.1 % | 120.1–121.1 MB | 111.6–112.2 MB |
| | new | 92.9–93.1 MB | 104.1–105.5 MB | 7.9–9.6 % | 117.7–119.3 MB | 108.7–110.5 MB |
| 06-large-20mb.pdf | 0.8.1 | 112.1–116.2 MB | 132.6–136.3 MB | 6.3–7.7 % (7 earlier runs: median 7.7) | 146.7–150.8 MB | 110.9–138.2 MB |
| | new | 112.1–122.6 MB | 129.7–151.7 MB | 6.8–9.2 % (7 earlier runs: median 7.4) | 143.7–146.8 MB | 107.6–110.3 MB |
| either | SumatraPDF 3.6.1 | 33 / 56 MB | 15 / 37 MB | 1.8 / 2.6 % | 16 / 40 MB | 15 / 39 MB |

- No regression in ordinary reading: idle stays 0–0.3 %, scrolling CPU is within run-to-run noise, memory is a few MB lower on 01 and the peak lower on 06. On 06, two of three new runs held 15–19 MB more 4 s after opening (the page ahead now renders after the visible one rather than beside it); peak and settled memory are lower.
- Against SumatraPDF the gap is unchanged: about 90 MB more private memory, and more CPU while scrolling (GPUI draws the whole window each frame).

### Found while measuring

The first build of "pages on screen first" kept a page that was laid out blank and then scrolled away marked as waiting, so the page ahead, links and fields never loaded after a flick. Fixed before the numbers above: it now waits on the on-screen renders still out, which always come back (drawn, skipped or stopped).

### Start-up: `kitab-bench s2 --runs 15 --warmup 2`, medians

| File | App | Window on screen | Page on screen | CPU to page | Working set | Private | CPU in the 2 s after |
|---|---|---|---|---|---|---|---|
| 01-text-outline.pdf | Kitab 0.8.1 | 246 ms (with the page) | 259 ms | 265 ms | 106.6 MB | 145.2 MB | 6.2 % |
| | Kitab new | 240 ms (with the page) | 253 ms | 234 ms | 107.1 MB | 144.9 MB | 6.2 % |
| | SumatraPDF 3.6.1 | 84 ms (blank) | ≤ 401 ms | 187 ms | 46.7 MB | 15.4 MB | 0.0 % |
| 06-large-20mb.pdf | Kitab 0.8.1 | 251 ms (with the page) | 265 ms | 281 ms | 109.1 MB | 127.0 MB | 7.0 % |
| | Kitab new | 258 ms (with the page) | 272 ms | 250 ms | 106.9 MB | 125.5 MB | 7.0 % |
| | SumatraPDF 3.6.1 | 110 ms (blank) | ≤ 413 ms | 187 ms | 74.6 MB | 37.4 MB | 0.0 % |

- Start-up is unchanged within noise (±7 ms to the window); CPU to the first page is 30 ms lower in both files (the page ahead and the link loading now wait for the first page).
- SumatraPDF shows its window 150 ms sooner but blank; Kitab's window arrives with the page in it, and the page is on screen sooner than SumatraPDF's (253–272 ms against up to 401–413 ms).
