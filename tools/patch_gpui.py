# SPDX-License-Identifier: AGPL-3.0-or-later
"""Make the patched copy of gpui 0.2.2 the workspace builds against (ADR 0002).

Copies gpui-0.2.2 from the local cargo registry into vendor/gpui-0.2.2 (git-ignored;
the workspace Cargo.toml points `[patch.crates-io] gpui` at it) and applies Kitab's
Windows patches, measured in docs/perf/baseline.md. They are ON by default; each has
an environment switch to turn it off so it can still be benchmarked:

  Fonts   DirectWrite skips re-checking every system font file at start-up
          (GetSystemFontCollection checkForUpdates = false). The not-found fallback still
          checks, so fonts installed later load. Off: KITAB_GPUI_FONT_UPDATE_CHECK=1.
          Source: stofu5678/ume_editor#23.
  Adapter No throwaway probe device per adapter. KITAB_GPU=low-power|high-performance|warp
          picks by preference (warp = software Direct3D); default is the first adapter.
  Lean    Single-sampled path texture instead of a window-sized 4x MSAA one, and 2
          swap-chain buffers instead of 3. Off: KITAB_GPUI_FULL=1.
  Heap    The manifest enables the Windows segment heap.
  Fallback When Direct3D 11 fails on the chosen adapter, every other one is tried,
          then WARP (software). KITAB_GPU=test-fallback goes straight to WARP.
  Frames  Windows are woken by the vsync thread only when something changed or
          an animation waits for a frame (idle CPU 0 %). KITAB_GPUI_FULL has no say.
  Scale   An interface size factor (Settings › Appearance) multiplies every
          window's scale; gpui::kitab_set_ui_scale sets it.
  Display A window reopened on a monitor with a different scale is placed with
          that monitor's scale.
  Select  TextLayout::kitab_bounds: bounds without the panic, for hit-testing.
  RTL     Right-to-left glyph runs (Arabic, Hebrew) are placed at DirectWrite's
          origin and advance leftwards, instead of left to right.
  Paths   The two window-sized textures paths are drawn through are 1x1 until
          a frame has a path to draw (ink, a drawn shape), and go back after 30
          frames without one. They were 8 bytes per window pixel, always: 29 MB
          in a 2400x1500 window, 66 MB at 4K.
  Touch   Windows touch screens. A finger pans (WM_GESTURE GID_PAN, with inertia)
          as fine scroll steps; a pinch (GID_ZOOM) arrives as a scroll with Ctrl
          held whose y is 200 x ln(ratio), which the app reads as a zoom factor.
          A wheel message's own Ctrl and Shift flags decide (a touchpad pinch says
          Ctrl; a key let go in another window can't leave Kitab zooming).
  Atlas   Linux: a texture freed before the frame that would fill it (a tab
          closed just after its pages were drawn) takes its queued upload and
          start-up with it. The frame looked the texture up afterwards and
          panicked, which took Kitab down (GitHub #19).
  Wayland A Wayland compositor without what gpui needs (xdg_wm_base 2+,
          wl_compositor 3+, wl_shm, wl_seat; WSLg's is too old) is passed over
          for X11 when there's a DISPLAY, instead of panicking at start-up.
  MenuKey macOS's menu bar shows a command's key only when it has Ctrl or Cmd
          in it. A key equivalent there is live: with a plain one (N for the
          next page) the menu could take the letter while it is being typed.
  Align   Centred and right-aligned text is hit-tested where it's drawn
          (TextLayout::index_for_position undoes each row's shift).
  Pixels  RenderImage::kitab_husk: an image that is only on the GPU. Once a page
          has been uploaded its pixels in RAM are let go and a husk (same id and
          size, no bytes) stands in for it. Painting a husk whose texture is
          gone (device lost) paints nothing and counts in gpui::kitab_image_misses,
          so the app renders the page again.

Run `python tools/patch_gpui.py` once after cloning (and after a gpui version bump).

gpui is Apache-2.0; its LICENSE files travel with the copy.
Run: python patch_gpui.py   (then cargo build --release in this folder)
"""
import glob
import os
import pathlib
import shutil
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DEST = ROOT / "vendor" / "gpui-0.2.2"


# SHA-256 of gpui-0.2.2.crate from crates.io (the file the workspace has
# always been built from). Cargo.lock has none: gpui is patched to a path.
GPUI_SHA256 = "979b45cfa6ec723b6f42330915a1b3769b930d02b2d505f9697f8ca602bee707"


def download_copy() -> pathlib.Path:
    """gpui-0.2.2 straight from crates.io, checked against GPUI_SHA256: a fresh
    clone (CI) can't `cargo fetch` while [patch] points at a missing folder."""
    import hashlib
    import tarfile
    import tempfile
    import urllib.request

    url = "https://static.crates.io/crates/gpui/gpui-0.2.2.crate"
    data = urllib.request.urlopen(url, timeout=120).read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != GPUI_SHA256:
        sys.exit(f"gpui-0.2.2.crate has SHA-256 {digest}, expected {GPUI_SHA256}")
    out = pathlib.Path(tempfile.mkdtemp(prefix="kitab-gpui-"))
    crate = out / "gpui-0.2.2.crate"
    crate.write_bytes(data)
    with tarfile.open(crate, "r:gz") as tar:
        tar.extractall(out, filter="data")
    return out / "gpui-0.2.2"


def registry_copy() -> pathlib.Path:
    home = pathlib.Path(os.environ.get("CARGO_HOME", pathlib.Path.home() / ".cargo"))
    hits = glob.glob(str(home / "registry" / "src" / "*" / "gpui-0.2.2"))
    if not hits:
        print("gpui-0.2.2 isn't in the cargo registry; downloading it from crates.io")
        return download_copy()
    return pathlib.Path(hits[0])


def edit(rel: str, pairs: list[tuple[str, str]]) -> None:
    p = DEST / rel
    s = p.read_text(encoding="utf8")
    for old, new in pairs:
        if old not in s:
            sys.exit(f"{rel}: patch anchor not found (gpui changed?): {old[:80]!r}")
        s = s.replace(old, new, 1)
    p.write_text(s, encoding="utf8")


def main() -> None:
    src = registry_copy()
    if DEST.exists():
        shutil.rmtree(DEST)
    shutil.copytree(src, DEST, ignore=shutil.ignore_patterns("examples", "docs", "tests", ".cargo-ok"))

    # 1. Fonts: the start-up system font collection skips the per-file update check.
    edit("src/platform/windows/direct_write.rs", [(
        """                .GetSystemFontCollection(false, &mut result, true)?;""",
        """                // Kitab patch (ADR 0002): checkForUpdates = true re-reads every system
                // font file on each launch. The not-found fallback below still checks.
                .GetSystemFontCollection(
                    false,
                    &mut result,
                    std::env::var_os("KITAB_GPUI_FONT_UPDATE_CHECK").is_some(),
                )?;""",
    )])

    # 2. GPU: choose the adapter by preference and skip the probe device.
    edit("src/platform/windows/directx_devices.rs", [(
        """fn get_adapter(dxgi_factory: &IDXGIFactory6, debug_layer_available: bool) -> Result<IDXGIAdapter1> {
    for adapter_index in 0.. {""",
        """fn get_adapter(dxgi_factory: &IDXGIFactory6, debug_layer_available: bool) -> Result<IDXGIAdapter1> {
    // Kitab patch (ADR 0002): ask DXGI for the preferred adapter and don't create a
    // throwaway D3D11 device on it. Creating the first device initialises the driver,
    // which is the slow part; doing it twice doubles the cost.
    if let Ok(pref) = std::env::var("KITAB_GPU") {
        // Software rendering (WARP): no vendor driver in the process. Its memory and
        // CPU cost are measured against the hardware adapters (ADR 0002).
        if pref == "warp" {
            if let Ok(adapter) = unsafe { dxgi_factory.EnumWarpAdapter::<IDXGIAdapter1>() } {
                log::info!("Using GPU: Microsoft Basic Render Driver (WARP)");
                return Ok(adapter);
            }
        }
        use windows::Win32::Graphics::Dxgi::{
            DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, DXGI_GPU_PREFERENCE_MINIMUM_POWER,
        };
        let preference = if pref == "high-performance" {
            DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE
        } else {
            DXGI_GPU_PREFERENCE_MINIMUM_POWER
        };
        if let Ok(adapter) =
            unsafe { dxgi_factory.EnumAdapterByGpuPreference::<IDXGIAdapter1>(0, preference) }
        {
            if let Ok(desc) = unsafe { adapter.GetDesc1() } {
                let gpu_name = String::from_utf16_lossy(&desc.Description)
                    .trim_matches(char::from(0))
                    .to_string();
                log::info!("Using GPU (by preference {pref}): {}", gpu_name);
            }
            return Ok(adapter);
        }
    }
    // Default: the first adapter, without creating a probe device on it.
    if let Ok(adapter) = unsafe { dxgi_factory.EnumAdapters(0) }.and_then(|a| a.cast::<IDXGIAdapter1>()) {
        return Ok(adapter);
    }
    for adapter_index in 0.. {""",
    )])
    # 3. Lean GPU memory (KITAB_LEAN=1): a single-sampled path texture instead of a
    #    full-window 4x MSAA one (resolved with a plain copy), and 2 swap-chain
    #    buffers instead of 3. A reader draws few vector paths in its chrome.
    lean_fn = """

/// Kitab patch (ADR 0002): lean mode (default; KITAB_GPUI_FULL=1 turns it off) trades path anti-aliasing quality and one
/// back buffer for less GPU memory.
fn kitab_lean() -> bool {
    std::env::var_os("KITAB_GPUI_FULL").is_none()
}

fn kitab_buffer_count() -> u32 {
    if kitab_lean() { 2 } else { BUFFER_COUNT as u32 }
}
"""
    p = DEST / "src/platform/windows/directx_renderer.rs"
    s = p.read_text(encoding="utf8")
    if s.count("BUFFER_COUNT as u32") != 3:
        sys.exit("directx_renderer.rs: expected 3 uses of BUFFER_COUNT as u32")
    s = s.replace("BUFFER_COUNT as u32", "kitab_buffer_count()")
    p.write_text(s + lean_fn, encoding="utf8")
    edit("src/platform/windows/directx_renderer.rs", [
        ("""            self.devices.device_context.ResolveSubresource(
                &self.resources.path_intermediate_texture,
                0,
                &self.resources.path_intermediate_msaa_texture,
                0,
                RENDER_TARGET_FORMAT,
            );""",
         """            if kitab_lean() {
                self.devices.device_context.CopyResource(
                    &self.resources.path_intermediate_texture,
                    &self.resources.path_intermediate_msaa_texture,
                );
            } else {
                self.devices.device_context.ResolveSubresource(
                    &self.resources.path_intermediate_texture,
                    0,
                    &self.resources.path_intermediate_msaa_texture,
                    0,
                    RENDER_TARGET_FORMAT,
                );
            }"""),
        ("""                Count: PATH_MULTISAMPLE_COUNT,
                Quality: D3D11_STANDARD_MULTISAMPLE_PATTERN.0 as u32,""",
         """                Count: if kitab_lean() { 1 } else { PATH_MULTISAMPLE_COUNT },
                Quality: if kitab_lean() { 0 } else { D3D11_STANDARD_MULTISAMPLE_PATTERN.0 as u32 },"""),
    ])
    # 4. Segment heap (Windows 10 2004+): the process heap (Rust's System allocator)
    #    commits less and fragments less. zed#54538 measured 124 -> 91 MB.
    edit("resources/windows/gpui.manifest.xml", [(
        """            <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>""",
        """            <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
            <heapType xmlns="http://schemas.microsoft.com/SMI/2020/WindowsSettings">SegmentHeap</heapType>""",
    )])
    # 5. Right-to-left text: DirectWrite shapes Arabic and Hebrew and runs the
    #    bidi algorithm, then hands each glyph run with its origin and bidi
    #    level. gpui ignored both and laid every run left to right after the
    #    previous one, so RTL words came out backwards. Place each run at its
    #    origin; odd-level runs advance leftwards.
    edit("src/platform/windows/direct_write.rs", [
        ("""        _baselineoriginx: f32,
        _baselineoriginy: f32,
        _measuringmode: DWRITE_MEASURING_MODE,
        glyphrun: *const DWRITE_GLYPH_RUN,""",
         """        baselineoriginx: f32,
        _baselineoriginy: f32,
        _measuringmode: DWRITE_MEASURING_MODE,
        glyphrun: *const DWRITE_GLYPH_RUN,"""),
        ("""        let mut cluster_analyzer = ClusterAnalyzer::new(cluster_map, glyph_count);""",
         """        // Kitab: runs sit at DirectWrite's origin; RTL runs advance leftwards.
        let rtl = glyphrun.bidiLevel % 2 == 1;
        let mut pen = baselineoriginx;
        let mut cluster_analyzer = ClusterAnalyzer::new(cluster_map, glyph_count);"""),
        ("""                let this_glyph_idx = glyph_idx + cluster_glyph_idx;
                glyphs.push(ShapedGlyph {
                    id,
                    position: point(
                        px(context.width + glyph_offsets[this_glyph_idx].advanceOffset),
                        px(0.0),
                    ),""",
         """                let this_glyph_idx = glyph_idx + cluster_glyph_idx;
                let advance = glyph_advances[this_glyph_idx];
                let offset = glyph_offsets[this_glyph_idx].advanceOffset;
                let x = if rtl { pen - advance - offset } else { pen + offset };
                pen = if rtl { pen - advance } else { pen + advance };
                glyphs.push(ShapedGlyph {
                    id,
                    position: point(px(x), px(0.0)),"""),
        ("""                context.width += glyph_advances[this_glyph_idx];
            }""",
         """            }"""),
    ])
    # The line's width is the right edge of the rightmost run so far (runs come
    # in visual order).
    edit("src/platform/windows/direct_write.rs", [
        ("""            glyph_idx += cluster_glyph_count;
        }""",
         """            glyph_idx += cluster_glyph_count;
        }
        context.width = context.width.max(if rtl { baselineoriginx } else { pen });"""),
    ])
    # 6. The title bar: dragging the window, and its minimise, maximise and
    #    close buttons. gpui sends a press there through the app first and,
    #    when something prevented the default, swallows it. Any focusable
    #    ancestor does that (click-to-focus calls prevent_default), so a drag
    #    never reached Windows and the buttons never recorded their press.
    #    The app still sees the press; Windows always gets it on these areas.
    edit("src/platform/windows/events.rs", [
        ("""            let result = func(input);
            let handled = !result.propagate || result.default_prevented;
            self.state.borrow_mut().callbacks.input = Some(func);

            if handled {
                return Some(0);
            }
        } else {
            drop(lock);
        };

        // Since these are handled in handle_nc_mouse_up_msg we must prevent the default window proc""",
         """            let result = func(input);
            let handled = !result.propagate || result.default_prevented;
            self.state.borrow_mut().callbacks.input = Some(func);

            let caption = matches!(
                wparam.0 as u32,
                HTCAPTION | HTMINBUTTON | HTMAXBUTTON | HTCLOSE
            );
            if handled && !caption {
                return Some(0);
            }
        } else {
            drop(lock);
        };

        // Since these are handled in handle_nc_mouse_up_msg we must prevent the default window proc"""),
    ])
    # 7. A GPU that can't run gpui: when Direct3D 11 can't be created on the
    #    chosen adapter (an old GPU, a broken driver, some virtual machines and
    #    remote sessions), try every other adapter, then WARP (software
    #    Direct3D, part of Windows) instead of failing to open a window.
    #    KITAB_GPU=test-fallback skips the hardware adapters, to test this.
    edit("src/platform/windows/directx_devices.rs", [
        ("""        let adapter =
            get_adapter(&dxgi_factory, debug_layer_available).context("Getting DXGI adapter")?;
        let (device, device_context) = {
            let mut context: Option<ID3D11DeviceContext> = None;
            let mut feature_level = D3D_FEATURE_LEVEL::default();
            let device = get_device(
                &adapter,
                Some(&mut context),
                Some(&mut feature_level),
                debug_layer_available,
            )
            .context("Creating Direct3D device")?;
            match feature_level {""",
         """        let first = get_adapter(&dxgi_factory, debug_layer_available)
            .context("Getting DXGI adapter")
            .log_err();
        let (adapter, device, context, feature_level) =
            kitab_open_device(&dxgi_factory, first, debug_layer_available)?;
        let (device, device_context) = {
            match feature_level {"""),
        ("""            (device, context.unwrap())
        };""",
         """            (device, context)
        };"""),
        ("""#[inline]
fn check_debug_layer_available() -> bool {""",
         """/// Kitab patch: Direct3D 11 on the first adapter that can run it: the chosen
/// one, then every other adapter, then WARP (software, always present).
fn kitab_open_device(
    factory: &IDXGIFactory6,
    first: Option<IDXGIAdapter1>,
    debug_layer_available: bool,
) -> Result<(IDXGIAdapter1, ID3D11Device, ID3D11DeviceContext, D3D_FEATURE_LEVEL)> {
    let skip_hardware = std::env::var("KITAB_GPU").is_ok_and(|v| v == "test-fallback");
    // Adapters are told apart by LUID: each enumeration returns new objects.
    let mut tried: Vec<(u32, i32)> = Vec::new();
    let mut candidates: Vec<IDXGIAdapter1> = Vec::new();
    if !skip_hardware {
        candidates.extend(first);
        for i in 0..16 {
            match unsafe { factory.EnumAdapters(i) }.and_then(|a| a.cast::<IDXGIAdapter1>()) {
                Ok(a) => candidates.push(a),
                Err(_) => break,
            }
        }
    }
    if let Ok(warp) = unsafe { factory.EnumWarpAdapter::<IDXGIAdapter1>() } {
        candidates.push(warp);
    }
    let mut last = anyhow::anyhow!("No graphics adapter");
    for adapter in candidates {
        let desc = unsafe { adapter.GetDesc1() }.unwrap_or_default();
        let luid = (desc.AdapterLuid.LowPart, desc.AdapterLuid.HighPart);
        if tried.contains(&luid) {
            continue;
        }
        tried.push(luid);
        let name = String::from_utf16_lossy(&desc.Description)
            .trim_matches(char::from(0))
            .to_string();
        let mut context: Option<ID3D11DeviceContext> = None;
        let mut feature_level = D3D_FEATURE_LEVEL::default();
        match get_device(
            &adapter,
            Some(&mut context),
            Some(&mut feature_level),
            debug_layer_available,
        ) {
            Ok(device) => {
                if let Some(context) = context {
                    if tried.len() > 1 {
                        log::warn!("Using GPU {name} after the first one failed");
                    }
                    return Ok((adapter, device, context, feature_level));
                }
            }
            Err(e) => {
                log::warn!("Direct3D 11 failed on {name}: {e:#}");
                last = e;
            }
        }
    }
    Err(last.context("Creating Direct3D device"))
}

#[inline]
fn check_debug_layer_available() -> bool {"""),
    ])
    # 8. A window reopened on another monitor: the saved bounds are in that
    #    monitor's logical pixels, and the check for which monitor they're
    #    on uses its scale; place the window with that scale too, not the
    #    primary monitor's, or it lands elsewhere when the two differ.
    edit("src/platform/windows/window.rs", [
        ("""    let bounds = bounds.to_device_pixels(scale_factor);
    placement.rcNormalPosition = calculate_window_rect(bounds, border_offset);""",
         """    // Kitab patch: the display's own scale (it's where the window goes).
    let _ = scale_factor;
    let bounds = bounds.to_device_pixels(display.scale_factor);
    placement.rcNormalPosition = calculate_window_rect(bounds, border_offset);"""),
    ])
    edit("src/platform/windows/display.rs", [
        ("""    pub display_id: DisplayId,
    scale_factor: f32,""", """    pub display_id: DisplayId,
    pub(crate) scale_factor: f32,"""),
    ])
    # 9. Frames on demand: the vsync thread woke every window on every display
    #    refresh (60-144 times a second), costing about 2 % of a core while
    #    nothing changed. A window that changed, or waits for a frame
    #    callback (animations), raises a flag; the vsync thread wakes windows
    #    only while it's up, and rests in between. GPUI's one second of
    #    presents after input (it keeps the display's refresh rate up) keeps
    #    the flag up. Same idea as zed#63182.
    edit("src/window.rs", [
        ("""struct WindowInvalidatorInner {""",
         """/// Kitab patch: a window changed or waits for a frame; the vsync thread
/// wakes windows only while this is set.
pub(crate) static KITAB_FRAME_WANTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);
/// The vsync thread, woken when a frame is wanted while it rests.
pub(crate) static KITAB_VSYNC_THREAD: std::sync::OnceLock<std::thread::Thread> =
    std::sync::OnceLock::new();

/// Kitab patch: ask the vsync thread for a frame.
pub(crate) fn kitab_want_frame() {
    if !KITAB_FRAME_WANTED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        if let Some(t) = KITAB_VSYNC_THREAD.get() {
            t.unpark();
        }
    }
}

struct WindowInvalidatorInner {"""),
        ("""        if inner.draw_phase == DrawPhase::None {
            inner.dirty = true;
            cx.push_effect(Effect::Notify { emitter: entity });""",
         """        if inner.draw_phase == DrawPhase::None {
            inner.dirty = true;
            kitab_want_frame();
            cx.push_effect(Effect::Notify { emitter: entity });"""),
        ("""    pub fn set_dirty(&self, dirty: bool) {
        self.inner.borrow_mut().dirty = dirty
    }""",
         """    pub fn set_dirty(&self, dirty: bool) {
        if dirty {
            kitab_want_frame();
        }
        self.inner.borrow_mut().dirty = dirty
    }"""),
        ("""    pub fn on_next_frame(&self, callback: impl FnOnce(&mut Window, &mut App) + 'static) {
        RefCell::borrow_mut(&self.next_frame_callbacks).push(Box::new(callback));""",
         """    pub fn on_next_frame(&self, callback: impl FnOnce(&mut Window, &mut App) + 'static) {
        kitab_want_frame();
        RefCell::borrow_mut(&self.next_frame_callbacks).push(Box::new(callback));"""),
        ("""                let needs_present = request_frame_options.require_presentation
                    || needs_present.get()
                    || (active.get()
                        && last_input_timestamp.get().elapsed() < Duration::from_secs(1));""",
         """                let needs_present = request_frame_options.require_presentation
                    || needs_present.get()
                    || (active.get()
                        && last_input_timestamp.get().elapsed() < Duration::from_secs(1));
                // Kitab patch: presents still to come need frames too.
                if needs_present {
                    kitab_want_frame();
                }"""),
    ])
    edit("src/platform/windows/platform.rs", [
        ("""                let vsync_provider = VSyncProvider::new();
                loop {
                    vsync_provider.wait_for_vsync();""",
         """                let vsync_provider = VSyncProvider::new();
                let _ = crate::window::KITAB_VSYNC_THREAD.set(std::thread::current());
                loop {
                    // Kitab patch: rest until a frame is wanted (and check the
                    // device a few times a second meanwhile).
                    if !crate::window::KITAB_FRAME_WANTED.load(std::sync::atomic::Ordering::Acquire) {
                        std::thread::park_timeout(std::time::Duration::from_millis(250));
                    }
                    vsync_provider.wait_for_vsync();"""),
        ("""                    for hwnd in all_windows.read().iter() {
                        unsafe {
                            let _ = RedrawWindow(Some(hwnd.as_raw()), None, None, RDW_INVALIDATE);
                        }
                    }""",
         """                    if !crate::window::KITAB_FRAME_WANTED
                        .swap(false, std::sync::atomic::Ordering::AcqRel)
                    {
                        continue;
                    }
                    for hwnd in all_windows.read().iter() {
                        unsafe {
                            let _ = RedrawWindow(Some(hwnd.as_raw()), None, None, RDW_INVALIDATE);
                        }
                    }"""),
    ])
    #    The window is marked painted before its frame is drawn, not after: a
    #    repaint the vsync thread asks for while a slow frame is drawing (the
    #    next step of an animation) was wiped out by the late ValidateRect,
    #    and with the flag already spent nothing drew again until the mouse
    #    moved: dialogs stuck half faded in.
    edit("src/platform/windows/events.rs", [
        ("""        let mut request_frame = self.state.borrow_mut().callbacks.request_frame.take()?;
        request_frame(RequestFrameOptions {
            require_presentation: false,
            force_render,
        });
        self.state.borrow_mut().callbacks.request_frame = Some(request_frame);
        unsafe { ValidateRect(Some(handle), None).ok().log_err() };
        Some(0)""",
         """        let mut request_frame = self.state.borrow_mut().callbacks.request_frame.take()?;
        // Kitab patch: painted first, so a repaint asked for meanwhile stands.
        unsafe { ValidateRect(Some(handle), None).ok().log_err() };
        request_frame(RequestFrameOptions {
            require_presentation: false,
            force_render,
        });
        self.state.borrow_mut().callbacks.request_frame = Some(request_frame);
        Some(0)"""),
    ])
    #    Paths can be moved: a stroke (a pen mark) is tessellated once at
    #    the page's corner and placed where the page is each frame, instead
    #    of being tessellated again on every frame of a scroll.
    edit("src/scene.rs", [
        ("""    /// Scale this path by the given factor.
    pub fn scale(&self, factor: f32) -> Path<ScaledPixels> {""",
         """    /// Kitab patch: this path moved by `by` (moving vertices costs far
    /// less than tessellating the stroke again).
    pub fn kitab_offset(&self, by: Point<Pixels>) -> Path<Pixels> {
        let mut p = self.clone();
        p.bounds.origin = p.bounds.origin + by;
        p.start = p.start + by;
        p.current = p.current + by;
        for v in &mut p.vertices {
            v.xy_position = v.xy_position + by;
        }
        p
    }

    /// Scale this path by the given factor.
    pub fn scale(&self, factor: f32) -> Path<ScaledPixels> {"""),
    ])
    #    And a frame that ends still wanting another (a notify that came in
    #    while it drew, an animation's next step) asks for it again.
    edit("src/window.rs", [
        ("""            let last_input_timestamp = last_input_timestamp.clone();
            move |request_frame_options| {""",
         """            let last_input_timestamp = last_input_timestamp.clone();
            let kitab_callbacks = next_frame_callbacks.clone();
            move |request_frame_options| {"""),
        ("""                handle
                    .update(&mut cx, |_, window, _| {
                        window.complete_frame();
                    })
                    .log_err();
            }
        }));""",
         """                handle
                    .update(&mut cx, |_, window, _| {
                        window.complete_frame();
                    })
                    .log_err();
                // Kitab patch: more to draw already: ask for the next frame.
                if invalidator.is_dirty() || !kitab_callbacks.borrow().is_empty() {
                    kitab_want_frame();
                }
            }
        }));"""),
    ])
    # 10. Interface size (Kitab's Settings › Appearance): every window's scale
    #     is the monitor's times the user's factor, like a browser's zoom, so
    #     text, icons, spacing and pages all grow together. A DPI message with
    #     the window's own DPI and rectangle applies a new factor at once; the
    #     handler now always lays the window out again (a same-size
    #     SetWindowPos sends no WM_SIZE).
    edit("src/window.rs", [
        ("""struct WindowInvalidatorInner {""",
         """/// Kitab patch: the interface size the user chose (1.0 = the system's),
/// as f32 bits. Windows multiplies every window's scale by it.
static KITAB_UI_SCALE: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0x3f80_0000);

/// Kitab patch: set the interface size (0.5 to 3.0 times the system's).
/// New windows use it; existing ones on their next DPI message.
pub fn kitab_set_ui_scale(scale: f32) {
    let scale = if scale.is_finite() { scale.clamp(0.5, 3.0) } else { 1.0 };
    KITAB_UI_SCALE.store(scale.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// Kitab patch: the interface size factor.
pub fn kitab_ui_scale() -> f32 {
    f32::from_bits(KITAB_UI_SCALE.load(std::sync::atomic::Ordering::Relaxed))
}

struct WindowInvalidatorInner {"""),
    ])
    edit("src/platform/windows/window.rs", [
        ("""            let monitor_dpi = unsafe { GetDpiForWindow(hwnd) } as f32;
            monitor_dpi / USER_DEFAULT_SCREEN_DPI as f32
        };""",
         """            let monitor_dpi = unsafe { GetDpiForWindow(hwnd) } as f32;
            // Kitab patch: times the interface size.
            monitor_dpi / USER_DEFAULT_SCREEN_DPI as f32 * crate::kitab_ui_scale()
        };"""),
    ])
    edit("src/platform/windows/events.rs", [
        ("""        let new_scale_factor = new_dpi / USER_DEFAULT_SCREEN_DPI as f32;""",
         """        // Kitab patch: times the interface size.
        let new_scale_factor = new_dpi / USER_DEFAULT_SCREEN_DPI as f32 * crate::kitab_ui_scale();"""),
        ("""        // When maximized, SetWindowPos doesn't send WM_SIZE, so we need to manually
        // update the size and call the resize callback
        if is_maximized {
            let device_size = size(DevicePixels(width), DevicePixels(height));
            self.handle_size_change(device_size, new_scale_factor, true);
        }""",
         """        // When maximized, SetWindowPos doesn't send WM_SIZE, so we need to manually
        // update the size and call the resize callback. Kitab patch: nor does
        // a move to the same rectangle (a new interface size), so always.
        // The client area as it ended up (a minimum size may have grown it).
        let _ = (is_maximized, width, height);
        let mut client = RECT::default();
        unsafe { GetClientRect(handle, &mut client) }.log_err();
        let device_size = size(
            DevicePixels(client.right - client.left),
            DevicePixels(client.bottom - client.top),
        );
        self.handle_size_change(device_size, new_scale_factor, true);"""),
        ("""        let min_size = lock.min_size?;
        let scale_factor = lock.scale_factor;""",
         """        let min_size = lock.min_size?;
        // Kitab patch: the smallest window is set in screen terms, not grown
        // with the interface size (at 200 % it wouldn't fit the screen).
        let scale_factor = lock.scale_factor / crate::kitab_ui_scale();"""),
    ])
    # Displays measure in the same scaled points as windows, so positions,
    # sizes and "which monitor is this on" agree at any interface size.
    edit("src/platform/windows/display.rs", [
        ("""    Ok(dpi_x as f32 / USER_DEFAULT_SCREEN_DPI as f32)
}""",
         """    // Kitab patch: times the interface size, like every window's scale.
    Ok(dpi_x as f32 / USER_DEFAULT_SCREEN_DPI as f32 * crate::kitab_ui_scale())
}"""),
    ])
    # 11. Text selection in Kitab's Markdown view hit-tests laid-out text; the
    #     stock accessors panic before layout, so a checked one.
    edit("src/elements/text.rs", [
        ("""    /// The bounds of this layout.
    pub fn bounds(&self) -> Bounds<Pixels> {""",
         """    /// Kitab patch: the bounds once laid out and painted, else `None`.
    pub fn kitab_bounds(&self) -> Option<Bounds<Pixels>> {
        self.0.borrow().as_ref().and_then(|s| s.bounds)
    }

    /// The bounds of this layout.
    pub fn bounds(&self) -> Bounds<Pixels> {"""),
    ])
    husk_patch()
    path_patch()
    touch_patch()
    blade_atlas_pending_fix()
    wayland_fallback_patch()
    menu_key_patch()
    aligned_hit_test_patch()
    zed_backports()
    print(f"patched gpui copy: {DEST}")
    xim_ctext_fix()


def aligned_hit_test_patch() -> None:
    """16. Align: text centred or right-aligned (`text_align`, a Markdown
    `<div align="center">`) is drawn with each wrapped row shifted, but
    `TextLayout::index_for_position` and `position_for_index` mapped as if
    it were left-aligned, so selecting centred text picked the wrong
    letters. The alignment is kept when the text paints, and each row's
    shift (the one `aligned_origin_x` paints with) is taken off the
    pointer, or put on the position."""
    edit("src/elements/text.rs", [
        ("""    size: Option<Size<Pixels>>,
    bounds: Option<Bounds<Pixels>>,
}""",
         """    size: Option<Size<Pixels>>,
    bounds: Option<Bounds<Pixels>>,
    // Kitab patch: the alignment it painted with.
    kitab_align: crate::TextAlign,
}

/// Kitab patch: how far row `row` of `line` is shifted right by `align`
/// in `width` (as `aligned_origin_x` paints it).
fn kitab_row_shift(line: &WrappedLine, row: usize, align: crate::TextAlign, width: Pixels) -> Pixels {
    if align == crate::TextAlign::Left {
        return Pixels::ZERO;
    }
    let layout = &line.layout.unwrapped_layout;
    let x_at = |b: &crate::WrapBoundary| layout.runs[b.run_ix].glyphs[b.glyph_ix].position.x;
    let bounds = &line.layout.wrap_boundaries;
    let start = if row == 0 {
        Pixels::ZERO
    } else {
        bounds.get(row - 1).map(x_at).unwrap_or(Pixels::ZERO)
    };
    let end = bounds.get(row).map(x_at).unwrap_or(layout.width);
    let row_width = end - start;
    match align {
        crate::TextAlign::Left => Pixels::ZERO,
        crate::TextAlign::Center => (width - row_width) / 2.0,
        crate::TextAlign::Right => width - row_width,
    }
}"""),
        ("""                        size: Some(Size::default()),
                        bounds: None,
                    });""",
         """                        size: Some(Size::default()),
                        bounds: None,
                        kitab_align: crate::TextAlign::Left,
                    });"""),
        ("""                    size: Some(size),
                    bounds: None,
                });""",
         """                    size: Some(size),
                    bounds: None,
                    kitab_align: crate::TextAlign::Left,
                });"""),
        ("""    fn paint(&self, text: &str, window: &mut Window, cx: &mut App) {
        let element_state = self.0.borrow();""",
         """    fn paint(&self, text: &str, window: &mut Window, cx: &mut App) {
        // Kitab patch: hit-testing needs the alignment it paints with.
        if let Some(state) = self.0.borrow_mut().as_mut() {
            state.kitab_align = window.text_style().text_align;
        }
        let element_state = self.0.borrow();"""),
        ("""            } else {
                let position_within_line = position - line_origin;
                match line.index_for_position(position_within_line, line_height) {""",
         """            } else {
                let mut position_within_line = position - line_origin;
                // Kitab patch: undo the row's alignment shift.
                if element_state.kitab_align != crate::TextAlign::Left {
                    let row = (position_within_line.y / line_height).floor().max(0.0) as usize;
                    position_within_line.x -=
                        kitab_row_shift(line, row, element_state.kitab_align, bounds.size.width);
                }
                match line.index_for_position(position_within_line, line_height) {"""),
        ("""                let ix_within_line = index - line_start_ix;
                return Some(line_origin + line.position_for_index(ix_within_line, line_height)?);""",
         """                let ix_within_line = index - line_start_ix;
                let mut p = line.position_for_index(ix_within_line, line_height)?;
                // Kitab patch: and put it on for a position.
                if element_state.kitab_align != crate::TextAlign::Left {
                    let row = (p.y / line_height).floor().max(0.0) as usize;
                    p.x += kitab_row_shift(line, row, element_state.kitab_align, bounds.size.width);
                }
                return Some(line_origin + p);"""),
    ])


def zed_backports() -> None:
    """Fixes from Zed's main branch since gpui 0.2.2 (crates.io has nothing
    newer), each kept as Zed wrote it where the code still matches. Each was
    measured or tested before it stayed (docs/perf/baseline.md)."""
    input_rate_backport()
    thread_priority_backport()
    atlas_free_backport()
    task_budget_backport()
    scene_backport()
    # Round 2: Linux and macOS bugs.
    glyph_offsets_backport()
    wrapped_hit_test_backport()
    wayland_backports()
    x11_scroll_focus_backport()
    prompt_click_backport()
    open_uri_backport()
    mac_backports()


def input_rate_backport() -> None:
    """zed#45369: keep presenting for a second after input only when input
    comes fast (60 a second or more: a scroll, a drag). Any input did it
    before, and with the Frames patch a single click or mouse move asked for
    a frame on every display refresh for a whole second."""
    edit("src/window.rs", [
        ("""    pub(crate) last_input_timestamp: Rc<Cell<Instant>>,""",
         """    // Kitab backport (zed#45369): input rate, not the last input's time.
    pub(crate) last_input_timestamp: Rc<RefCell<InputRateTracker>>,"""),
        ("""        let last_input_timestamp = Rc::new(Cell::new(Instant::now()));""",
         """        let last_input_timestamp = Rc::new(RefCell::new(InputRateTracker::default()));"""),
        ("""                    || (active.get()
                        && last_input_timestamp.get().elapsed() < Duration::from_secs(1));""",
         """                    || (active.get() && last_input_timestamp.borrow().is_high_rate());"""),
        ("""    pub fn dispatch_event(&mut self, event: PlatformInput, cx: &mut App) -> DispatchEventResult {
        self.last_input_timestamp.set(Instant::now());
""",
         """    pub fn dispatch_event(&mut self, event: PlatformInput, cx: &mut App) -> DispatchEventResult {
"""),
        ("""            self.dispatch_key_event(any_key_event, cx);
        }

        DispatchEventResult {
            propagate: cx.propagate_event,""",
         """            self.dispatch_key_event(any_key_event, cx);
        }

        // Kitab backport (zed#45369): input that changed something counts
        // towards the rate.
        if self.invalidator.is_dirty() {
            self.last_input_timestamp.borrow_mut().record_input();
        }

        DispatchEventResult {
            propagate: cx.propagate_event,"""),
        ("""#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DrawPhase {""",
         """/// Kitab backport (zed#45369): whether input is arriving at a high rate
/// (60 a second or more over the last 100 ms), sustained for a second after.
#[derive(Clone, Debug)]
pub(crate) struct InputRateTracker {
    timestamps: Vec<Instant>,
    window: Duration,
    inputs_per_second: u32,
    sustain_until: Instant,
    sustain_duration: Duration,
}

impl Default for InputRateTracker {
    fn default() -> Self {
        Self {
            timestamps: Vec::new(),
            window: Duration::from_millis(100),
            inputs_per_second: 60,
            sustain_until: Instant::now(),
            sustain_duration: Duration::from_secs(1),
        }
    }
}

impl InputRateTracker {
    pub fn record_input(&mut self) {
        let now = Instant::now();
        self.timestamps.push(now);
        let window = self.window;
        self.timestamps.retain(|&t| now.duration_since(t) <= window);
        let min_events = self.inputs_per_second as u128 * self.window.as_millis() / 1000;
        if self.timestamps.len() as u128 >= min_events {
            self.sustain_until = now + self.sustain_duration;
        }
    }

    pub fn is_high_rate(&self) -> bool {
        Instant::now() < self.sustain_until
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DrawPhase {"""),
    ])


def thread_priority_backport() -> None:
    """zed#44121: background work at normal priority, not High: a long
    search or thumbnail run no longer competes with every other program."""
    edit("src/platform/windows/dispatcher.rs", [
        ("""        ThreadPool, ThreadPoolTimer, TimerElapsedHandler, WorkItemHandler, WorkItemPriority,
""",
         """        ThreadPool, ThreadPoolTimer, TimerElapsedHandler, WorkItemHandler,
"""),
        ("""ThreadPool::RunWithPriorityAsync(&handler, WorkItemPriority::High).log_err();""",
         """ThreadPool::RunAsync(&handler).log_err(); // Kitab backport (zed#44121)"""),
    ])


def task_budget_backport() -> None:
    """zed#43678, #43682, #43883, #43886, #46609: the main thread runs its
    queued tasks in 10 ms turns; between turns it takes the window's paint
    and input messages, so a burst of finished work (renders, search hits)
    can't hold input and painting back. Before, every queued task ran in
    one go. Messages are dispatched as GPUI's own loop does (no
    TranslateMessage here: 0.2.2 doesn't use it)."""
    edit("src/platform/windows/platform.rs", [
        ("""    fn run_foreground_task(&self) -> Option<isize> {
        for runnable in self.main_receiver.drain() {
            runnable.run();
        }
        Some(0)
    }""",
         """    fn run_foreground_task(&self) -> Option<isize> {
        // Kitab backport (zed#43678 and follow-ups): a 10 ms budget.
        const MAIN_TASK_TIMEOUT: u128 = 10;
        let mut start = std::time::Instant::now();
        loop {
            if start.elapsed().as_millis() >= MAIN_TASK_TIMEOUT {
                let mut msg = MSG::default();
                let process_message = |msg: &MSG| unsafe {
                    DispatchMessageW(msg);
                };
                let peek_msg = |msg: &mut MSG, kind| unsafe {
                    PeekMessageW(msg, None, 0, 0, PM_REMOVE | kind).as_bool()
                };
                // A paint first and last only, so animations can't loop here.
                if peek_msg(&mut msg, PM_QS_PAINT) {
                    process_message(&msg);
                }
                while peek_msg(&mut msg, PM_QS_INPUT) {
                    process_message(&msg);
                }
                if peek_msg(&mut msg, PM_QS_PAINT) {
                    process_message(&msg);
                }
                start = std::time::Instant::now();
            }
            match self.main_receiver.try_recv() {
                Err(_) => break,
                Ok(runnable) => {
                    runnable.run();
                }
            }
        }
        Some(0)
    }"""),
    ])


def scene_backport() -> None:
    """Building the scene each frame: zed#44655's bounds tree (finding each
    primitive's draw order; an R-tree with 12 children a node instead of a
    binary tree), taken whole from Zed (tools/gpui_backports/bounds_tree.rs,
    same interface), and zed#66898's sort: small (key, index) pairs sorted,
    then each primitive moved once. 0.2.2's primitives aren't Copy, so they
    are swapped along each cycle instead of copied (the same order, ties
    still in insertion order)."""
    tree = pathlib.Path(__file__).resolve().parent / "gpui_backports" / "bounds_tree.rs"
    (DEST / "src" / "bounds_tree.rs").write_text(tree.read_text(encoding="utf8"), encoding="utf8")
    edit("src/scene.rs", [
        ("""    pub fn finish(&mut self) {
        self.shadows.sort_by_key(|shadow| shadow.order);
        self.quads.sort_by_key(|quad| quad.order);
        self.paths.sort_by_key(|path| path.order);
        self.underlines.sort_by_key(|underline| underline.order);
        self.monochrome_sprites
            .sort_by_key(|sprite| (sprite.order, sprite.tile.tile_id));
        self.polychrome_sprites
            .sort_by_key(|sprite| (sprite.order, sprite.tile.tile_id));
        self.surfaces.sort_by_key(|surface| surface.order);
    }
""",
         """    pub fn finish(&mut self) {
        // Kitab backport (zed#66898): sort keys, then move each primitive once.
        let keys = &mut Vec::new();
        kitab_sort_in_place(&mut self.shadows, keys, |shadow| (shadow.order, 0));
        kitab_sort_in_place(&mut self.quads, keys, |quad| (quad.order, 0));
        self.paths.sort_by_key(|path| path.order);
        kitab_sort_in_place(&mut self.underlines, keys, |underline| (underline.order, 0));
        kitab_sort_in_place(&mut self.monochrome_sprites, keys, |sprite| {
            (sprite.order, sprite.tile.tile_id.0)
        });
        kitab_sort_in_place(&mut self.polychrome_sprites, keys, |sprite| {
            (sprite.order, sprite.tile.tile_id.0)
        });
        self.surfaces.sort_by_key(|surface| surface.order);
    }
"""),
    ])
    scene = DEST / "src" / "scene.rs"
    scene.write_text(scene.read_text(encoding="utf8") + SORT_IN_PLACE, encoding="utf8")


SORT_IN_PLACE = """
/// Kitab backport (zed#66898): sort small `(key, index)` pairs instead of
/// the primitives, then put each primitive in place. Ties keep insertion
/// order (the index is part of the key), as `sort_by_key` did.
pub(crate) fn kitab_sort_in_place<T>(
    primitives: &mut [T],
    keys: &mut Vec<(u64, u32)>,
    key: impl Fn(&T) -> (u32, u32),
) {
    if primitives.is_sorted_by_key(&key) {
        return;
    }
    keys.clear();
    keys.extend(primitives.iter().enumerate().map(|(index, primitive)| {
        let (major, minor) = key(primitive);
        (((major as u64) << 32) | minor as u64, index as u32)
    }));
    keys.sort_unstable();
    // `keys[target].1` is where `target`'s primitive comes from. Follow each
    // cycle of that permutation, swapping (0.2.2's primitives aren't Copy),
    // and mark visited slots by pointing them at themselves.
    for start in 0..primitives.len() {
        if keys[start].1 as usize == start {
            continue;
        }
        let mut target = start;
        loop {
            let source = keys[target].1 as usize;
            keys[target].1 = target as u32;
            if source == start {
                break;
            }
            primitives.swap(target, source);
            target = source;
        }
    }
}

#[cfg(test)]
mod kitab_sort_tests {
    use super::kitab_sort_in_place;

    #[test]
    fn sorts_like_a_stable_sort() {
        let mut seed = 12345u64;
        for len in [0usize, 1, 2, 3, 10, 100, 1000] {
            let mut v: Vec<(u32, u32, usize)> = (0..len)
                .map(|i| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    (((seed >> 33) % 20) as u32, ((seed >> 13) % 3) as u32, i)
                })
                .collect();
            let mut expected = v.clone();
            expected.sort_by_key(|p| (p.0, p.1));
            let keys = &mut Vec::new();
            kitab_sort_in_place(&mut v, keys, |p| (p.0, p.1));
            assert_eq!(v, expected, "len {len}");
        }
    }
}
"""


XIM_CTEXT_SHA256 = "2ac61a7062c40f3c37b6e82eeeef835d5cc7824b632a72784a89b3963c33284c"


def xim_ctext_fix() -> None:
    """zed issue #66087: on X11 with an input method (IBus, fcitx), Japanese
    and other multi-byte preedit text repeated ("nihongo" showed にほんにほんご).
    The bug is in xim-ctext 0.3.0, which gpui's X11 input uses; its fix
    (Riey/xim-rs@9ec8406, MIT) was never released, so the workspace patches
    xim-ctext to this fixed copy, made from the crates.io crate (checked
    against Cargo.lock's checksum) beside the gpui copy."""
    import hashlib
    import io
    import tarfile
    import urllib.request

    dest = ROOT / "vendor" / "xim-ctext-0.3.0"
    url = "https://static.crates.io/crates/xim-ctext/xim-ctext-0.3.0.crate"
    data = urllib.request.urlopen(url, timeout=120).read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != XIM_CTEXT_SHA256:
        sys.exit(f"xim-ctext-0.3.0.crate has SHA-256 {digest}, expected {XIM_CTEXT_SHA256}")
    if dest.exists():
        shutil.rmtree(dest)
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
        tar.extractall(dest.parent, filter="data")
    lib = dest / "src" / "lib.rs"
    s = lib.read_text(encoding="utf8")
    for old, new in [
        ("""    ($decoder:expr, $out:expr, $bytes:expr, $last:expr) => {
        loop {
            let (ret, _, _) = $decoder.decode_to_string($bytes, $out, $last);
""",
         """    ($decoder:expr, $out:expr, $bytes:expr, $last:expr) => {
        // Kitab: Riey/xim-rs@9ec8406, decode each byte once.
        let mut _current_bytes: &[u8] = $bytes;
        loop {
            let (ret, nread, _) = $decoder.decode_to_string(_current_bytes, $out, $last);
"""),
        ("""                            .unwrap_or_default(),
                    );
                }""",
         """                            .unwrap_or_default(),
                    );
                    _current_bytes = &_current_bytes[nread..];
                }"""),
    ]:
        if s.count(old) != 1:
            sys.exit(f"xim-ctext: patch anchor not found: {old[:60]!r}")
        s = s.replace(old, new)
    # Upstream's test for it: a long ISO-2022-JP string decodes once.
    marker = "\n#[cfg(test)]\nmod tests {"
    if marker not in s:
        sys.exit("xim-ctext: tests module not found")
    s = s.replace(marker, marker + """
    #[test]
    fn kitab_iso_2022_jp_long() {
        const UTF8: &str = "知ってるつもり";
        const COMP: &[u8] = &[
            27, 36, 40, 66, 67, 78, 36, 67, 36, 70, 36, 107, 36, 68, 36, 98, 36, 106, 27, 40, 66,
        ];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }
""", 1)
    lib.write_text(s, encoding="utf8")
    print(f"patched xim-ctext copy: {dest}")


def glyph_offsets_backport() -> None:
    """zed#66799: Linux shaping kept cosmic-text's glyph positions but
    dropped each glyph's mark offset, so combining marks (Bangla vowel
    signs, Arabic and Thai marks, accents) drew beside their letter. The
    offsets are in ems; cosmic-text 0.14's own `physical()` applies them
    the same way."""
    edit("src/platform/linux/text_system.rs", [
        ("""                position: point(glyph.x.into(), glyph.y.into()),""",
         """                // Kitab backport (zed#66799): the mark offsets too.
                position: point(
                    (glyph.x + glyph.x_offset * f32::from(font_size)).into(),
                    (glyph.y - glyph.y_offset * f32::from(font_size)).into(),
                ),"""),
    ])


def wrapped_hit_test_backport() -> None:
    """zed#64672: hit-testing just past a wrapped line's last glyph (a
    zero-width glyph at a wrap) panicked on `unwrap`; it now answers the
    row's end."""
    edit("src/text_system/line_layout.rs", [
        ("""                Ok(self
                    .unwrapped_layout
                    .index_for_x(position_in_unwrapped_line.x)
                    .unwrap())""",
         """                // Kitab backport (zed#64672): past the last glyph, the row's end.
                self.unwrapped_layout
                    .index_for_x(position_in_unwrapped_line.x)
                    .ok_or(wrapped_line_end_index)"""),
    ])


def wayland_backports() -> None:
    """Wayland: zed#50609, window controls start as none (the compositor
    lists the ones it has), so unsupported buttons don't show; zed#39313,
    a compositor without server-side decorations (GNOME) gets client-side
    ones instead of none; zed#46560, a seat announced again doesn't leave a
    second text input behind (characters typed twice with an IME)."""
    edit("src/platform/linux/wayland/window.rs", [
        ("""                let mut window_controls = WindowControls::default();

                let states = extract_states::<xdg_toplevel::WmCapabilities>(&capabilities);""",
         """                // Kitab backport (zed#50609): only what the compositor lists.
                let mut window_controls = WindowControls {
                    maximize: false,
                    minimize: false,
                    fullscreen: false,
                    window_menu: false,
                };

                let states = extract_states::<xdg_toplevel::WmCapabilities>(&capabilities);"""),
        ("""        state.decorations = decorations;
        if let Some(decoration) = state.decoration.as_ref() {
            decoration.set_mode(decorations.to_xdg());
            update_window(state);
        }""",
         """        // Kitab backport (zed#39313): no server-side decorations, client-side ones.
        match state.decoration.as_ref() {
            Some(decoration) => {
                decoration.set_mode(decorations.to_xdg());
                state.decorations = decorations;
                update_window(state);
            }
            None => {
                if matches!(decorations, WindowDecorations::Server) {
                    log::info!(
                        "Server-side decorations requested, but the Wayland server does not support them. Falling back to client-side decorations."
                    );
                }
                state.decorations = WindowDecorations::Client;
                update_window(state);
            }
        }"""),
    ])
    edit("src/platform/linux/wayland/client.rs", [
        ("""                let keyboard = seat.get_keyboard(qh, ());

                state.text_input = state""",
         """                let keyboard = seat.get_keyboard(qh, ());

                // Kitab backport (zed#46560): one text input per seat.
                if let Some(text_input) = state.text_input.take() {
                    text_input.destroy();
                    state.ime_pre_edit = None;
                    state.composing = false;
                }

                state.text_input = state"""),
        ("""                let pointer = seat.get_pointer(qh, ());
                state.cursor_shape_device = state""",
         """                let pointer = seat.get_pointer(qh, ());
                // Kitab backport (zed#46560): and one cursor shape device.
                if let Some(cursor_shape_device) = state.cursor_shape_device.take() {
                    cursor_shape_device.destroy();
                }
                state.cursor_shape_device = state"""),
    ])


def x11_scroll_focus_backport() -> None:
    """zed#43841: X11's scroll valuator is global; after scrolling in another
    window, the first scroll back in Kitab jumped by all of it. Focus out
    forgets the last positions."""
    edit("src/platform/linux/x11/client.rs", [
        ("""            Event::FocusOut(event) => {
                let window = self.get_window(event.event)?;
                window.set_active(false);
                let mut state = self.0.borrow_mut();
                state.keyboard_focused_window = None;""",
         """            Event::FocusOut(event) => {
                let window = self.get_window(event.event)?;
                window.set_active(false);
                let mut state = self.0.borrow_mut();
                // Kitab backport (zed#43841): no huge delta on the next scroll.
                reset_all_pointer_device_scroll_positions(&mut state.pointer_device_states);
                state.keyboard_focused_window = None;"""),
    ])


def prompt_click_backport() -> None:
    """zed#41442: GPUI's own prompt (Linux without a native one) let a click
    on its button through to the window below."""
    edit("src/window/prompts.rs", [
        ("""                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(PromptResponse(ix));
                    }))""",
         """                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(PromptResponse(ix));
                        cx.stop_propagation(); // Kitab backport (zed#41442)
                    }))"""),
    ])


def open_uri_backport() -> None:
    """zed#49752: links open with xdg-open first and the desktop portal only
    if that fails. The portal first showed "No apps available" where it has
    no backend for the type (the same missing-portal case as the file
    picker). The portal call is 0.2.2's (its ashpd version)."""
    edit("src/platform/linux/platform.rs", [
        ("""                match ashpd::desktop::open_uri::OpenFileRequest::default()
                    .activation_token(activation_token.clone().map(ashpd::ActivationToken::from))
                    .send_uri(&uri)
                    .await
                {
                    Ok(_) => return,
                    Err(e) => log::error!("Failed to open with dbus: {}", e),
                }

                for mut command in open::commands(uri.to_string()) {
                    if let Some(token) = activation_token.as_ref() {
                        command.env("XDG_ACTIVATION_TOKEN", token);
                    }
                    let program = format!("{:?}", command.get_program());
                    match smol::process::Command::from(command).spawn() {
                        Ok(mut cmd) => {
                            cmd.status().await.log_err();
                            return;
                        }
                        Err(e) => {
                            log::error!("Failed to open with {}: {}", program, e)
                        }
                    }
                }""",
         """                // Kitab backport (zed#49752): xdg-open first, the portal after.
                for mut command in open::commands(uri.to_string()) {
                    if let Some(token) = activation_token.as_ref() {
                        command.env("XDG_ACTIVATION_TOKEN", token);
                    }
                    let program = format!("{:?}", command.get_program());
                    match smol::process::Command::from(command).spawn() {
                        Ok(mut cmd) => match cmd.status().await {
                            Ok(status) if status.success() => return,
                            Ok(status) => {
                                log::error!("Command {} exited with status: {}", program, status)
                            }
                            Err(e) => log::error!("Failed to get status from {}: {}", program, e),
                        },
                        Err(e) => {
                            log::error!("Failed to open with {}: {}", program, e)
                        }
                    }
                }
                if let Err(e) = ashpd::desktop::open_uri::OpenFileRequest::default()
                    .activation_token(activation_token.map(ashpd::ActivationToken::from))
                    .send_uri(&uri)
                    .await
                {
                    log::error!("Failed to open with dbus: {}", e);
                }"""),
    ])


def mac_backports() -> None:
    """macOS: zed#51394, Cmd+key isn't handed to the input method (it ate
    shortcuts on non-US and dead-key layouts); zed#51035, no deadlock when
    resigning key status re-enters the window; zed#65744, Backspace and
    Delete show their own symbols in the menu bar (0.2.2 uses key_to_native
    only for menu key equivalents)."""
    edit("src/platform/mac/window.rs", [
        ("""                || (key_down_event.keystroke.key_char.is_none()
                    && !key_down_event.keystroke.modifiers.control
                    && !key_down_event.keystroke.modifiers.function)""",
         """                || (key_down_event.keystroke.key_char.is_none()
                    && !key_down_event.keystroke.modifiers.control
                    && !key_down_event.keystroke.modifiers.function
                    // Kitab backport (zed#51394): Cmd+key skips the IME.
                    && !key_down_event.keystroke.modifiers.platform)"""),
        ("""    if selector == sel!(windowDidBecomeKey:) && !is_active {
        unsafe {
            let _: () = msg_send![lock.native_window, resignKeyWindow];
            return;
        }
    }""",
         """    if selector == sel!(windowDidBecomeKey:) && !is_active {
        // Kitab backport (zed#51035): not under the lock (it re-enters).
        let native_window = lock.native_window;
        drop(lock);
        unsafe {
            let _: () = msg_send![native_window, resignKeyWindow];
        }
        return;
    }"""),
    ])
    edit("src/platform/mac/events.rs", [
        ("""        "backspace" => BACKSPACE_KEY,""",
         """        // Kitab backport (zed#65744): menu key equivalents' own codes.
        "backspace" => 0x08,"""),
        ("""        "delete" => NSDeleteFunctionKey,""",
         """        "delete" => 0x7f,"""),
    ])


def atlas_free_backport() -> None:
    """zed#58874: removing an image from the atlas gives its space back. The
    allocator kept it before, so thumbnails and small images scrolled past
    kept growing the shared textures."""
    edit("src/platform/windows/directx_atlas.rs", [
        ("""        let Some(id) = lock.tiles_by_key.remove(key).map(|tile| tile.texture_id) else {
            return;
        };
""",
         """        // Kitab backport (zed#58874): keep the tile to free its space.
        let Some(tile) = lock.tiles_by_key.remove(key) else {
            return;
        };
        let id = tile.texture_id;
"""),
        ("""        if let Some(mut texture) = texture_slot.take() {
            texture.decrement_ref_count();
            if texture.is_unreferenced() {
                textures.free_list.push(texture.id.index as usize);""",
         """        if let Some(mut texture) = texture_slot.take() {
            texture.allocator.deallocate(tile.tile_id.into());
            texture.decrement_ref_count();
            if texture.is_unreferenced() {
                textures.free_list.push(texture.id.index as usize);"""),
    ])
    # zed#58874's test: a big image removed leaves room for the next one in
    # the same texture (before, the next went to a new texture).
    atlas = DEST / "src/platform/windows/directx_atlas.rs"
    atlas.write_text(atlas.read_text(encoding="utf8") + ATLAS_TEST, encoding="utf8")


ATLAS_TEST = """
#[cfg(test)]
mod kitab_atlas_tests {
    use super::*;
    use crate::{ImageId, RenderImageParams};
    use std::borrow::Cow;
    use windows::Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_WARP,
            Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice},
        },
    };

    fn create_atlas() -> Option<DirectXAtlas> {
        let mut device: Option<ID3D11Device> = None;
        let mut device_context: Option<ID3D11DeviceContext> = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut device_context),
            )
        }
        .ok()?;
        Some(DirectXAtlas::new(&device?, &device_context?))
    }

    fn key(image_id: usize) -> AtlasKey {
        AtlasKey::Image(RenderImageParams {
            image_id: ImageId(image_id),
            frame_index: 0,
        })
    }

    fn insert(atlas: &DirectXAtlas, key: &AtlasKey, size: Size<DevicePixels>) -> AtlasTile {
        atlas
            .get_or_insert_with(key, &mut || {
                let n = (size.width.0 as usize) * (size.height.0 as usize) * 4;
                Ok(Some((size, Cow::Owned(vec![0u8; n]))))
            })
            .expect("allocation should succeed")
            .expect("callback returns Some")
    }

    #[test]
    fn removed_tile_space_is_reused() {
        let Some(atlas) = create_atlas() else { return };
        let small = Size { width: DevicePixels(64), height: DevicePixels(64) };
        let big = Size { width: DevicePixels(700), height: DevicePixels(700) };
        let keeper = insert(&atlas, &key(1), small);
        let a = insert(&atlas, &key(2), big);
        assert_eq!(keeper.texture_id, a.texture_id);
        atlas.remove(&key(2));
        let b = insert(&atlas, &key(3), big);
        assert_eq!(b.texture_id, keeper.texture_id);
    }
}
"""


def husk_patch() -> None:
    """12. Pixels: images that live only on the GPU (see the module docs)."""
    edit("src/assets.rs", [
        ("""    pub(crate) scale_factor: f32,
    data: SmallVec<[Frame; 1]>,
}
""",
         """    pub(crate) scale_factor: f32,
    data: SmallVec<[Frame; 1]>,
    /// Kitab patch: set on a husk, an image whose pixels are only on the GPU.
    kitab_husk: Option<Size<DevicePixels>>,
}

static KITAB_IMAGE_MISSES: AtomicUsize = AtomicUsize::new(0);

/// Kitab patch: how many times a husk was painted after its texture had gone
/// (a lost device, an atlas that was cleared). It only ever grows.
pub fn kitab_image_misses() -> usize {
    KITAB_IMAGE_MISSES.load(SeqCst)
}

pub(crate) fn kitab_note_image_miss() {
    KITAB_IMAGE_MISSES.fetch_add(1, SeqCst);
}
"""),
        ("""            scale_factor: 1.0,
            data: data.into(),
        }
    }
""",
         """            scale_factor: 1.0,
            data: data.into(),
            kitab_husk: None,
        }
    }

    /// Kitab patch: stands in for `of` once its pixels are on the GPU: the
    /// same id and size and no bytes, so `of` (and its memory) can be dropped.
    pub fn kitab_husk(of: &RenderImage) -> Self {
        Self {
            id: of.id,
            scale_factor: of.scale_factor,
            data: SmallVec::new(),
            kitab_husk: Some(of.size(0)),
        }
    }

    /// Kitab patch: whether this is a husk (no pixels in memory).
    pub fn kitab_is_husk(&self) -> bool {
        self.kitab_husk.is_some()
    }
"""),
        ("""    pub fn size(&self, frame_index: usize) -> Size<DevicePixels> {
""",
         """    pub fn size(&self, frame_index: usize) -> Size<DevicePixels> {
        if let Some(size) = self.kitab_husk {
            return size;
        }
"""),
        ("""    pub fn delay(&self, frame_index: usize) -> Delay {
""",
         """    pub fn delay(&self, frame_index: usize) -> Delay {
        if self.kitab_husk.is_some() {
            return Delay::from_numer_denom_ms(0, 1);
        }
"""),
        ("""    pub fn frame_count(&self) -> usize {
""",
         """    pub fn frame_count(&self) -> usize {
        if self.kitab_husk.is_some() {
            return 1;
        }
"""),
    ])
    edit("src/window.rs", [
        ("""        let tile = self
            .sprite_atlas
            .get_or_insert_with(&params.into(), &mut || {
                Ok(Some((
                    data.size(frame_index),
                    Cow::Borrowed(
                        data.as_bytes(frame_index)
                            .expect("It's the caller's job to pass a valid frame index"),
                    ),
                )))
            })?
            .expect("Callback above only returns Some");
""",
         """        // Kitab patch: a husk has no bytes. Its texture is normally in the
        // atlas already; if it is gone, paint nothing and say so.
        let Some(tile) = self
            .sprite_atlas
            .get_or_insert_with(&params.into(), &mut || {
                Ok(data
                    .as_bytes(frame_index)
                    .map(|bytes| (data.size(frame_index), Cow::Borrowed(bytes))))
            })?
        else {
            crate::kitab_note_image_miss();
            return Ok(());
        };
"""),
    ])


def path_patch() -> None:
    """13. Paths: the path textures exist at window size only while paths are drawn."""
    edit("src/platform/windows/directx_renderer.rs", [
        ("""    path_intermediate_msaa_view: [Option<ID3D11RenderTargetView>; 1],

    // Cached window size and viewport
""",
         """    path_intermediate_msaa_view: [Option<ID3D11RenderTargetView>; 1],
    // Kitab patch: the path textures are 1x1 until a frame draws a path.
    kitab_path_full: bool,
    kitab_pathless_frames: u32,

    // Cached window size and viewport
"""),
        ("""            path_intermediate_srv,
            viewport,
            width,
            height,
        }))
""",
         """            path_intermediate_srv,
            kitab_path_full: false,
            kitab_pathless_frames: 0,
            viewport,
            width,
            height,
        }))
"""),
        ("""        self.path_intermediate_srv = path_intermediate_srv;
        self.viewport = viewport;
        Ok(())
    }
""",
         """        self.path_intermediate_srv = path_intermediate_srv;
        self.kitab_path_full = false;
        self.kitab_pathless_frames = 0;
        self.viewport = viewport;
        Ok(())
    }

    /// Kitab patch: the path textures at window size while frames draw
    /// paths, 1x1 otherwise (they cost 8 bytes per window pixel).
    fn kitab_fit_path_textures(
        &mut self,
        devices: &DirectXRendererDevices,
        has_paths: bool,
    ) -> Result<()> {
        let (width, height) = if has_paths {
            self.kitab_pathless_frames = 0;
            if self.kitab_path_full {
                return Ok(());
            }
            (self.width, self.height)
        } else {
            if !self.kitab_path_full {
                return Ok(());
            }
            self.kitab_pathless_frames += 1;
            if self.kitab_pathless_frames < 30 {
                return Ok(());
            }
            (1, 1)
        };
        let (texture, srv) = create_path_intermediate_texture(&devices.device, width, height)?;
        let (msaa_texture, msaa_view) =
            create_path_intermediate_msaa_texture_and_view(&devices.device, width, height)?;
        self.path_intermediate_texture = texture;
        self.path_intermediate_srv = srv;
        self.path_intermediate_msaa_texture = msaa_texture;
        self.path_intermediate_msaa_view = msaa_view;
        self.kitab_path_full = has_paths;
        Ok(())
    }
"""),
        ("""    let (path_intermediate_texture, path_intermediate_srv) =
        create_path_intermediate_texture(&devices.device, width, height)?;
    let (path_intermediate_msaa_texture, path_intermediate_msaa_view) =
        create_path_intermediate_msaa_texture_and_view(&devices.device, width, height)?;
""",
         """    // Kitab patch: 1x1 until a frame draws a path (kitab_fit_path_textures).
    let (path_intermediate_texture, path_intermediate_srv) =
        create_path_intermediate_texture(&devices.device, 1, 1)?;
    let (path_intermediate_msaa_texture, path_intermediate_msaa_view) =
        create_path_intermediate_msaa_texture_and_view(&devices.device, 1, 1)?;
"""),
        ("""    pub(crate) fn draw(&mut self, scene: &Scene) -> Result<()> {
        self.pre_draw()?;
""",
         """    pub(crate) fn draw(&mut self, scene: &Scene) -> Result<()> {
        // Kitab patch: path textures only while there are paths to draw.
        self.resources
            .kitab_fit_path_textures(&self.devices, !scene.paths.is_empty())?;
        self.pre_draw()?;
"""),
    ])


def menu_key_patch() -> None:
    """15. MenuKey: no plain keys as key equivalents in macOS's menu bar."""
    edit("src/platform/mac/platform.rs", [
        ("""                    if let Some(keystrokes) = keystrokes {
                        if keystrokes.len() == 1 {
""",
         """                    if let Some(keystrokes) = keystrokes {
                        // Kitab patch: only a key with Ctrl or Cmd in it.
                        if keystrokes.len() == 1
                            && (keystrokes[0].modifiers().control
                                || keystrokes[0].modifiers().platform)
                        {
"""),
    ])


def blade_atlas_pending_fix() -> None:
    """18. Atlas: GitHub #19, a crash closing tabs quickly on Linux.

    The Blade atlas (Linux) queues a new texture's start-up and its pixel
    uploads for the next frame. `remove` destroys a texture as soon as its
    last picture goes and empties its slot, but left what was queued for it:
    a tab closed before the next frame (several closed in a row) left the
    frame to look up an empty slot, which panicked ("unwrap on None" in
    flush_initializations), and the panic while unwinding aborted Kitab.
    Now the queued work for a texture goes with it."""
    edit("src/platform/blade/blade_atlas.rs", [
        ("""            if texture.is_unreferenced() {
                lock.storage[id.kind]
                    .free_list
                    .push(texture.id.index as usize);
                texture.destroy(&lock.gpu);""",
         """            if texture.is_unreferenced() {
                // Kitab patch (GitHub #19): what was queued for it this frame
                // goes too, or the frame finds its slot empty and panics.
                let gone = texture.id;
                lock.initializations.retain(|queued| *queued != gone);
                lock.uploads.retain(|upload| upload.id != gone);
                lock.storage[id.kind]
                    .free_list
                    .push(texture.id.index as usize);
                texture.destroy(&lock.gpu);"""),
    ])


def wayland_fallback_patch() -> None:
    """19. Wayland: an old compositor falls back to X11.

    gpui picks Wayland whenever WAYLAND_DISPLAY is set, then unwraps the
    globals it needs; a compositor without them (WSLg's has xdg_wm_base 1)
    panicked before the window opened. The compositor is asked first, and
    without them Kitab uses X11 (XWayland) when there's a DISPLAY."""
    edit("src/platform.rs", [
        ("""        #[cfg(feature = "wayland")]
        "Wayland" => Rc::new(WaylandClient::new()),
""",
         """        #[cfg(feature = "wayland")]
        "Wayland" if kitab_wayland_usable() => Rc::new(WaylandClient::new()),

        // Kitab patch: a compositor without what gpui needs; X11 instead.
        #[cfg(all(feature = "wayland", feature = "x11"))]
        "Wayland" if std::env::var_os("DISPLAY").is_some_and(|d| !d.is_empty()) => {
            log::warn!("the Wayland compositor lacks what Kitab needs; using X11");
            Rc::new(
                X11Client::new()
                    .context("Failed to initialize X11 client.")
                    .unwrap(),
            )
        }

        #[cfg(feature = "wayland")]
        "Wayland" => Rc::new(WaylandClient::new()),
"""),
        ("""/// Return which compositor we're guessing we'll use.""",
         """/// Kitab patch: the Wayland compositor offers what gpui's client binds
/// without fallback: xdg_wm_base 2+, wl_compositor 3+, wl_shm and wl_seat.
#[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "wayland"))]
fn kitab_wayland_usable() -> bool {
    use wayland_client::{
        Connection, Dispatch, QueueHandle,
        globals::{GlobalListContents, registry_queue_init},
        protocol::wl_registry,
    };
    struct Probe;
    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Probe {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
    let Ok(conn) = Connection::connect_to_env() else {
        return false;
    };
    let Ok((globals, _queue)) = registry_queue_init::<Probe>(&conn) else {
        return false;
    };
    globals.contents().with_list(|list| {
        let has = |name: &str, version: u32| {
            list.iter().any(|g| g.interface == name && g.version >= version)
        };
        has("xdg_wm_base", 2) && has("wl_compositor", 3) && has("wl_shm", 1) && has("wl_seat", 1)
    })
}

/// Return which compositor we're guessing we'll use."""),
    ])


def touch_patch() -> None:
    """14. Touch: pan and pinch on Windows touch screens."""
    edit("src/platform/windows/events.rs", [
        ("""            WM_MOUSEWHEEL => self.handle_mouse_wheel_msg(handle, wparam, lparam),
""",
         """            // Kitab patch: touch screens.
            KITAB_WM_GESTURENOTIFY => self.kitab_gesture_notify(handle),
            KITAB_WM_GESTURE => self.kitab_gesture(handle, lparam),
            WM_MOUSEWHEEL => self.handle_mouse_wheel_msg(handle, wparam, lparam),
"""),
        ("""    ) -> Option<isize> {
        let modifiers = current_modifiers();
        let mut lock = self.state.borrow_mut();
        let Some(mut func) = lock.callbacks.input.take() else {
            return Some(1);
        };
        let scale_factor = lock.scale_factor;
        let wheel_scroll_amount = match modifiers.shift {
""",
         """    ) -> Option<isize> {
        let mut modifiers = current_modifiers();
        // Kitab patch: Ctrl and Shift as the wheel message itself says
        // (MK_CONTROL, MK_SHIFT), not the window thread's key state. That
        // state goes stale when a key is let go while another window has
        // the keyboard (Ctrl held while switching away), and Windows wheels
        // the window under the pointer even unfocused: every notch zoomed
        // instead of scrolling until a key was pressed in Kitab. A pinch on
        // a touchpad is a wheel message with MK_CONTROL while the key itself
        // is up, which this keeps.
        modifiers.control = wparam.0 & 0x0008 != 0;
        modifiers.shift = wparam.0 & 0x0004 != 0;
        let mut lock = self.state.borrow_mut();
        let Some(mut func) = lock.callbacks.input.take() else {
            return Some(1);
        };
        let scale_factor = lock.scale_factor;
        let wheel_scroll_amount = match modifiers.shift {
"""),
        ("""    fn handle_mouse_wheel_msg(
""",
         """    /// Kitab patch: ask for every pan direction with inertia, and pinch.
    fn kitab_gesture_notify(&self, handle: HWND) -> Option<isize> {
        let config = [
            KitabGestureConfig {
                id: KITAB_GID_PAN,
                // GC_PAN, with one finger up and down and sideways, with inertia.
                want: 0x01 | 0x02 | 0x04 | 0x10,
                // GC_PAN_WITH_GUTTER: off, so a pan can go diagonally.
                block: 0x08,
            },
            KitabGestureConfig {
                id: KITAB_GID_ZOOM,
                want: 0x01,
                block: 0,
            },
        ];
        unsafe {
            SetGestureConfig(
                handle,
                0,
                config.len() as u32,
                config.as_ptr(),
                std::mem::size_of::<KitabGestureConfig>() as u32,
            );
        }
        // The system still has to see the message.
        None
    }

    /// Kitab patch: a finger pans, two fingers pinch.
    fn kitab_gesture(&self, handle: HWND, lparam: LPARAM) -> Option<isize> {
        let mut info: KitabGestureInfo = unsafe { std::mem::zeroed() };
        info.cb_size = std::mem::size_of::<KitabGestureInfo>() as u32;
        if unsafe { GetGestureInfo(lparam.0, &mut info) } == 0 {
            return None;
        }
        let (x, y) = (info.location_x as i32, info.location_y as i32);
        let (begin, end) = (info.flags & 0x1 != 0, info.flags & 0x4 != 0);
        let phase = if begin {
            TouchPhase::Started
        } else if end {
            TouchPhase::Ended
        } else {
            TouchPhase::Moved
        };
        let handled = match info.id {
            KITAB_GID_PAN => {
                let last = KITAB_GESTURE.with(|g| g.replace((x, y, 0)));
                if begin {
                    Some(0)
                } else {
                    // The page follows the finger.
                    let by = ((x - last.0) as f32, (y - last.1) as f32);
                    self.kitab_send_scroll(handle, (x, y), by, false, phase)
                }
            }
            KITAB_GID_ZOOM => {
                // The distance between the two fingers.
                let apart = info.arguments & 0xFFFF_FFFF;
                let last = KITAB_GESTURE.with(|g| g.replace((x, y, apart)));
                if begin || last.2 == 0 || apart == 0 {
                    Some(0)
                } else {
                    let ratio = apart as f32 / last.2 as f32;
                    self.kitab_send_scroll(handle, (x, y), (0.0, ratio.ln() * 200.0), true, phase)
                }
            }
            _ => None,
        };
        if handled.is_some() {
            unsafe { CloseGestureInfoHandle(lparam.0) };
        }
        handled
    }

    /// Kitab patch: a touch gesture as a scroll event at screen point `at`.
    /// A pan is in device pixels; a pinch (`pinch`) is 200 x ln(ratio) with
    /// Ctrl held, as it is.
    fn kitab_send_scroll(
        &self,
        handle: HWND,
        at: (i32, i32),
        by: (f32, f32),
        pinch: bool,
        phase: TouchPhase,
    ) -> Option<isize> {
        let mut modifiers = current_modifiers();
        if pinch {
            modifiers.control = true;
        }
        let mut lock = self.state.borrow_mut();
        let Some(mut func) = lock.callbacks.input.take() else {
            return Some(0);
        };
        let scale_factor = lock.scale_factor;
        drop(lock);
        let mut point = POINT { x: at.0, y: at.1 };
        unsafe { ScreenToClient(handle, &mut point).ok().log_err() };
        let scale = if pinch { 1.0 } else { scale_factor };
        let input = PlatformInput::ScrollWheel(ScrollWheelEvent {
            position: logical_point(point.x as f32, point.y as f32, scale_factor),
            delta: ScrollDelta::Pixels(Point {
                x: px(by.0 / scale),
                y: px(by.1 / scale),
            }),
            modifiers,
            touch_phase: phase,
        });
        func(input);
        self.state.borrow_mut().callbacks.input = Some(func);
        Some(0)
    }

    fn handle_mouse_wheel_msg(
"""),
    ])
    p = DEST / "src/platform/windows/events.rs"
    p.write_text(p.read_text(encoding="utf8").rstrip("\n") + """

// Kitab patch: touch gestures (WM_GESTURE). Declared here so the `windows`
// crate needs no further features.
const KITAB_WM_GESTURE: u32 = 0x0119;
const KITAB_WM_GESTURENOTIFY: u32 = 0x011A;
const KITAB_GID_ZOOM: u32 = 3;
const KITAB_GID_PAN: u32 = 4;

/// Win32 `GESTUREINFO`.
#[repr(C)]
struct KitabGestureInfo {
    cb_size: u32,
    flags: u32,
    id: u32,
    hwnd_target: *mut std::ffi::c_void,
    location_x: i16,
    location_y: i16,
    instance_id: u32,
    sequence_id: u32,
    arguments: u64,
    cb_extra_args: u32,
}

/// Win32 `GESTURECONFIG`.
#[repr(C)]
struct KitabGestureConfig {
    id: u32,
    want: u32,
    block: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetGestureInfo(gesture: isize, info: *mut KitabGestureInfo) -> i32;
    fn CloseGestureInfoHandle(gesture: isize) -> i32;
    fn SetGestureConfig(
        hwnd: HWND,
        reserved: u32,
        count: u32,
        config: *const KitabGestureConfig,
        size: u32,
    ) -> i32;
}

thread_local! {
    /// Where the gesture under way last was (screen x and y) and, for a
    /// pinch, how far apart the fingers were.
    static KITAB_GESTURE: std::cell::Cell<(i32, i32, u64)> = const { std::cell::Cell::new((0, 0, 0)) };
}
""", encoding="utf8")


if __name__ == "__main__":
    main()
