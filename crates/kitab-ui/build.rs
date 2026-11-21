// SPDX-License-Identifier: AGPL-3.0-or-later

//! Every translation in `assets/locales/` is built in (GitHub #12): adding a
//! language is adding its `<code>.json` there, with no code to change.
//! Writes `locales.rs` in OUT_DIR: `CATALOGS`, (code, file contents) pairs
//! sorted by code.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    // Cargo always sets both; without them there is nothing sensible to do.
    let var =
        |name: &str| std::env::var(name).unwrap_or_else(|e| panic!("{name} not set by Cargo: {e}"));
    let manifest = PathBuf::from(var("CARGO_MANIFEST_DIR"));
    let dir = manifest.join("../../assets/locales");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .filter_map(|p| {
                    let code = p.file_stem()?.to_str()?.to_string();
                    // `en` is the code's own text; `_`-files are scratch.
                    (code != "en" && !code.starts_with('_')).then_some((code, p))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    let mut out = String::from("pub(crate) const CATALOGS: &[(&str, &str)] = &[\n");
    for (code, path) in &found {
        println!("cargo:rerun-if-changed={}", path.display());
        let _ = writeln!(
            out,
            "    ({code:?}, include_str!({:?})),",
            path.display().to_string()
        );
    }
    out.push_str("];\n");
    let dest = PathBuf::from(var("OUT_DIR")).join("locales.rs");
    std::fs::write(&dest, out).unwrap_or_else(|e| panic!("writing {} failed: {e}", dest.display()));
}
