# ADR 0002: Keep GPUI and fix its Windows start-up

- Status: accepted (2026-11-07, by the owner)
- Context: [ADR 0001](0001-spike-a-gpui-mupdf.md), and a survey of eleven UI stacks for this decision

## Question

Spike A put a GPUI window with a PDF page on screen in about 280 ms. The S1 guardrail is 150 ms and the S2 goal is to match SumatraPDF on the same machine. Do we keep GPUI (D1) or switch?

## What the survey found

We compared GPUI with plain Win32 + Direct2D, egui, Iced, Slint, Xilem + Vello, Makepad, Freya, Floem, Tauri 2 / WebView2 and Qt via cxx-qt.

- **No framework has a published Windows start-up measurement better than ours.** Every GPU-drawn framework creates a GPU device (and often compiles shaders) before its first frame. The figures that do exist range from about 110 ms to several seconds, on other machines and methods. A webview starts slower still.
- **GPUI already has what the app needs on Windows:** DirectWrite text, GPU textures for page bitmaps, AccessKit on Zed's main branch, and IME as Zed ships it. Most alternatives lack one of these (Iced: no released accessibility; Xilem: Windows not supported; Tauri: slow cold start and awkward large bitmaps).
- **GPUI's cost is in when it does the work, not what it does.** 0.2.2 creates the D3D11 device twice (a probe per adapter, then the real one). Zed's main branch creates it once. Device and DirectWrite set-up still run synchronously before the window exists.
- **crates.io `gpui` is frozen at 0.2.2** (October 2025); Zed's main branch is about a year ahead and split into `gpui`, `gpui_platform` and `gpui_windows`.

## Decision

**Keep GPUI (D1 stands).** Depend on a **pinned Zed git revision** instead of crates.io, and carry a small Windows start-up patch set (AGENTS.md rule 26), offered upstream where general.

## Plan, each step measured with `kitab-bench`

1. **Black-box benchmark first:** `kitab-bench` S1/S2 against SumatraPDF on this machine, and `docs/perf/baseline.md`.
2. **Pinned Zed revision:** single device creation comes for free. Measure.
3. **Show the window first:** create the HWND and paint the first page with GDI from MuPDF's bitmap, the way SumatraPDF draws, while the D3D11 device and DirectWrite are created on a background thread. GPUI's renderer takes over when ready. Measure the handover for flicker and colour or DPI mismatch.
4. **Gate:** S1 and S2 within budget (PLAN.md §6.2) on this laptop and one more machine, cold and warm.

If the gate fails after step 3, the fallback is a Windows-only Win32 + Direct2D shell with AccessKit used on its own (ADR to follow), with macOS and Linux on GPUI.

## Risks

- Keeping a fork of a pre-1.0 framework current: pin, keep patches small, upstream them.
- Driver variance on hybrid-GPU laptops (waking the discrete GPU): measure on several machines.
- A visible seam when the GDI first frame hands over to the GPU surface.
- AccessKit in GPUI is new.
