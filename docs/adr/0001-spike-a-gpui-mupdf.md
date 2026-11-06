# ADR 0001: Spike A, GPUI + MuPDF on Windows

- Status: **open, gate not passed yet** (start-up budget missed; plan below)
- Date: 2026-11-06
- Phase: 0 (PLAN.md §12, Spike A)
- Code: [`spikes/spike-a`](../../spikes/spike-a) (outside the workspace, so CI does not build GPUI or MuPDF yet)

## Question

Can a GPUI window show a MuPDF-rendered PDF page fast enough to meet the S1/S2 budgets (PLAN.md §6.2: median ≤ SumatraPDF, guardrails S1 ≤ 150 ms, S2 ≤ 250 ms)?

## What was built

`spike-a <file.pdf> [pages]` opens a 1000×1000 GPUI window, opens the PDF with the `mupdf` crate on a plain thread (never the UI thread), renders up to 40 pages at the window's scale factor, converts RGBA to BGRA, and shows each page as a `gpui::img(ImageSource::Render(..))` at its own size in a scrolling column on the Dark theme's `desk` colour from `kitab-theme`. It prints trace marks from process start to stderr.

Versions: `gpui` 0.2.2 (crates.io), `mupdf` 0.8.0 / `mupdf-sys` 0.8.0 (default features off), rustc 1.98.1 MSVC, Visual Studio 2022 Build Tools 14.44.

## Building it

It builds and runs on Windows x64. Two things were needed:

1. **libclang** for `mupdf-sys`'s bindgen step. LLVM is not installed on the dev machine; the `libclang` 18.1.1 wheel from PyPI was used (`LIBCLANG_PATH=<venv>/Lib/site-packages/clang/native`).
2. That libclang ships without clang's resource headers, so the MSVC C headers are used, and they don't define `max_align_t` in C mode. `mupdf` 0.8.0 then fails with `E0425: cannot find type max_align_t` (`src/device/native.rs:599`). Workaround: `BINDGEN_EXTRA_CLANG_ARGS="-include max_align_t.h"` with `typedef double max_align_t;` (clang's own definition for MSVC targets). A full LLVM install should not need this; CI must install LLVM (or set the same flags) before Phase 1 adds MuPDF to the workspace.

Clean release build of `mupdf-sys` + `gpui` + spike: about 2–3 minutes on this machine. `spike-a.exe` is 17.1 MB (release, symbols not stripped, base-14 fonts only). The installer size (budget 15 MB, compressed) was not measured.

## Measurements

Machine: the dev laptop (Windows 11 Pro 10.0.26300, AMD Radeon integrated GPU + NVIDIA GeForce RTX 5070). Warm runs. Fixture `tests/fixtures/pdf/01-text-outline.pdf` (40 pages). These are internal trace marks (milliseconds from `main`), not the black-box S1/S2 harness, which doesn't exist yet.

| Mark (7 runs) | Range |
|---|---|
| `Application::new()` returned | 237–250 ms |
| window opened | 262–275 ms |
| page 1 rendered by MuPDF (worker) | 265–279 ms (about 3–4 ms after the window) |
| first frame with page 1 painted | 281–290 ms |
| all 40 pages rendered (3 runs) | 335–359 ms (≈ 2 ms per page) |

Breakdown of `Application::new()` with timing prints added to a scratch copy of GPUI (3 runs):

| Step in `WindowsPlatform::new` | Cumulative |
|---|---|
| `OleInitialize` | ~1 ms |
| `DirectXDevices::new` (DXGI factory, adapter, D3D11 device) | 187–197 ms |
| `DirectWriteTextSystem::new` | 236–249 ms |
| rest of platform init | 241–254 ms |

Inside `DirectXDevices::new`, `get_adapter` alone took ~106 ms: it creates a throwaway D3D11 device to probe each adapter, then the real device is created again.

**Experiment:** in the scratch copy, choose the adapter with `EnumAdapterByGpuPreference(MINIMUM_POWER)` (the AMD iGPU here) and skip the probe device. `DirectXDevices::new` fell to 38–56 ms and platform init to ~90–105 ms, but opening the window grew from ~25 ms to ~115–125 ms, so first paint moved only from ~285 ms to **233–268 ms (5 runs)**. Part of the device cost moves into the first swap chain / composition setup rather than going away.

**SumatraPDF comparison (rough):** launch to a visible main window, same file, polled from PowerShell (`MainWindowHandle`, ~15 ms resolution), 7 runs after 2 warm-ups: **SumatraPDF 31 ms median, spike-a 342 ms median.** This is not the S2 metric (first page sharp) and the poll is coarse, but the gap is an order of magnitude.

Not measured: scroll frame times (S9), live resize (S13), tile upload cost at 4K (that's Spike B), memory.

## Findings

1. **MuPDF is not the problem.** Page 1 renders in ~3–4 ms off the UI thread, and 40 pages in ~75 ms.
2. **GPUI's Windows platform start-up is.** About 250 ms goes into D3D11 device creation (with a probe device per adapter) and DirectWrite set-up before any window exists. SumatraPDF uses GDI and shows a window in ~30 ms.
3. On this hybrid-GPU laptop, adapter choice matters (~40–50 ms), but it doesn't close the gap.

## Decision for now

D1 (GPUI) **stands, conditionally.** The gate "projected S1/S2 within budget, or a written reason and plan" is met by the plan below, not by numbers. If after the plan's step 3 the first paint is still above 150 ms on this machine, reopen D1 (PLAN.md §1, "Revisit if").

## Plan

1. **Build the black-box harness first** (`kitab-bench` S1/S2, PLAN.md §6.1), so every step below is measured the same way against SumatraPDF, and record `docs/perf/baseline.md`.
2. **Patch GPUI's Windows start-up** in a pinned fork (AGENTS.md rule 26), each change measured:
   - drop the per-adapter probe device; pick the adapter with `EnumAdapterByGpuPreference` (minimum power by default, a setting for high performance);
   - create the D3D device and DirectWrite factory on a background thread while the Win32 window is created and shown;
   - paint the first frame (chrome on `bg`) with the cheapest path available, before the font collection and atlas are warm.
3. **Show something before GPUI is ready:** a plain Win32 window in the theme's `bg` colour shown at once, which GPUI then adopts or replaces. This is what makes S1 comparable with Sumatra's GDI start.
4. Upstream what's general (probe device, background device creation) to Zed.

## Reproduce

```powershell
# libclang: full LLVM, or the PyPI wheel
python -m venv .venv-clang; .venv-clang\Scripts\pip install libclang
$env:LIBCLANG_PATH = "$PWD\.venv-clang\Lib\site-packages\clang\native"
# only with the wheel (no clang resource headers):
Set-Content max_align_t.h "typedef double max_align_t;"
$env:BINDGEN_EXTRA_CLANG_ARGS = "-include $PWD\max_align_t.h"

cd spikes\spike-a
cargo build --release
.\target\release\spike-a.exe ..\..\tests\fixtures\pdf\01-text-outline.pdf 40 --exit-after-first-paint
```
