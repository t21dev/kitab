// SPDX-License-Identifier: AGPL-3.0-or-later

//! Embed a per-monitor DPI-aware manifest, so window bounds and captures are in
//! physical pixels and screenshots are not cropped on scaled displays.

fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=res/kitab-bench.manifest.xml");
        println!("cargo:rerun-if-changed=res/kitab-bench.rc");
        let _ = embed_resource::compile("res/kitab-bench.rc", embed_resource::NONE);
    }
}
