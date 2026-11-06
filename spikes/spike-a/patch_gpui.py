# SPDX-License-Identifier: AGPL-3.0-or-later
"""Make a patched copy of gpui 0.2.2 for start-up experiments (ADR 0002).

Copies gpui-0.2.2 from the local cargo registry into vendor/gpui-0.2.2 (git-ignored)
and applies Kitab's Windows start-up patches. Each patch is off unless its
environment variable is set, so one build can be benchmarked with and without it:

  KITAB_FONT_NO_UPDATE_CHECK=1   DirectWrite: don't re-check every system font file
                                 at start-up (GetSystemFontCollection checkForUpdates =
                                 false). The fallback lookup for fonts that aren't found
                                 still checks for updates, so fonts installed later load.
                                 Source: stofu5678/ume_editor#23 (~145 ms -> ~2 ms there).
  KITAB_GPU=low-power|high-performance
                                 Pick the adapter with EnumAdapterByGpuPreference and
                                 skip the throwaway probe device gpui creates per adapter.

gpui is Apache-2.0; its LICENSE files travel with the copy.
Run: python patch_gpui.py   (then cargo build --release in this folder)
"""
import glob
import os
import pathlib
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
DEST = HERE / "vendor" / "gpui-0.2.2"


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
                    std::env::var_os("KITAB_FONT_NO_UPDATE_CHECK").is_none(),
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
    for adapter_index in 0.. {""",
    )])
    print(f"patched gpui copy: {DEST}")


if __name__ == "__main__":
    main()
