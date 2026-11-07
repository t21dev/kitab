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


def registry_copy() -> pathlib.Path:
    home = pathlib.Path(os.environ.get("CARGO_HOME", pathlib.Path.home() / ".cargo"))
    hits = glob.glob(str(home / "registry" / "src" / "*" / "gpui-0.2.2"))
    if not hits:
        sys.exit("gpui-0.2.2 is not in the cargo registry; run `cargo fetch` in this folder first")
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
    print(f"patched gpui copy: {DEST}")


if __name__ == "__main__":
    main()
