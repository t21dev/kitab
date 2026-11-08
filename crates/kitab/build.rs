// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows: embed the app and document icons (packaging/windows/kitab.rc)
//! and the version block (packaging/windows/version.rc), whose
//! FileDescription is the name Windows shows for the app.

fn main() {
    let rc = "../../packaging/windows/kitab.rc";
    let ver = "../../packaging/windows/version.rc";
    println!("cargo:rerun-if-changed={rc}");
    println!("cargo:rerun-if-changed={ver}");
    println!("cargo:rerun-if-changed=../../packaging/windows");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile(rc, embed_resource::NONE)
            .manifest_optional()
            .unwrap_or_else(|e| panic!("embedding the icons failed: {e}"));
        let v = |k: &str| std::env::var(k).unwrap_or_else(|_| "0".into());
        let defines = [
            format!("KITAB_V1={}", v("CARGO_PKG_VERSION_MAJOR")),
            format!("KITAB_V2={}", v("CARGO_PKG_VERSION_MINOR")),
            format!("KITAB_V3={}", v("CARGO_PKG_VERSION_PATCH")),
            format!("KITAB_VERSION=\"{}\"", v("CARGO_PKG_VERSION")),
        ];
        embed_resource::compile(ver, &defines)
            .manifest_optional()
            .unwrap_or_else(|e| panic!("embedding the version failed: {e}"));
    }
}
