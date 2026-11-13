// SPDX-License-Identifier: AGPL-3.0-or-later

//! The RAR reader behind CBR comics, on libarchive's RAR test archives
//! (tests/fixtures/rar): the archives that should open give every file, each
//! checked against its CRC-32; encrypted, split and damaged ones are refused
//! with the right reason, and none panics. Expectations live in
//! tests/fixtures/rar/expected.json (AGENTS.md rule 21).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rar")
}

#[test]
fn every_rar_fixture_matches_expected_json() {
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir().join("expected.json")).unwrap())
            .unwrap();
    let files = expected.as_object().unwrap();
    assert!(files.len() >= 20, "rar fixtures are in expected.json");
    let mut problems = Vec::new();
    for (name, exp) in files {
        let bytes = std::fs::read(dir().join(name)).unwrap();
        let got = std::panic::catch_unwind(|| kitab_mupdf::rar::extract_for_tests(&bytes));
        let Ok(got) = got else {
            problems.push(format!("{name}: panicked"));
            continue;
        };
        match (exp["opens"].as_bool().unwrap(), got) {
            (true, Ok(files)) => {
                let want = exp["files"].as_u64().unwrap() as usize;
                if files.len() != want {
                    problems.push(format!("{name}: {} files, expected {want}", files.len()));
                }
            }
            (true, Err(e)) => problems.push(format!("{name}: does not open: {e}")),
            (false, Ok(_)) => problems.push(format!("{name}: should not open")),
            (false, Err(e)) => {
                let reason = exp["refused"].as_str().unwrap();
                let fits = match reason {
                    "password" => e.contains("password"),
                    "split" => e.contains("split"),
                    _ => !e.contains("password") && !e.contains("split"),
                };
                if !fits {
                    problems.push(format!("{name}: refused as \"{e}\", expected {reason}"));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
