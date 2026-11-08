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
    print(f"patched gpui copy: {DEST}")


if __name__ == "__main__":
    main()
