// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows: embed the app and document icons (packaging/windows/kitab.rc).

fn main() {
    let rc = "../../packaging/windows/kitab.rc";
    println!("cargo:rerun-if-changed={rc}");
    println!("cargo:rerun-if-changed=../../packaging/windows");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile(rc, embed_resource::NONE)
            .manifest_optional()
            .unwrap_or_else(|e| panic!("embedding the icons failed: {e}"));
    }
}
