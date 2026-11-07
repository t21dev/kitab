// SPDX-License-Identifier: AGPL-3.0-or-later

//! I1–I2 (PLAN.md §7.2): the four signed fixtures verify as expected.json
//! says, and trusting the test root makes their signers trusted.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kitab_core::{SigStatus, SignatureCheck};
use kitab_sign::{Trust, verify};
use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdf")
}

fn checks(name: &str, trust: &Trust) -> Vec<SignatureCheck> {
    let path = dir().join(name);
    let bytes = std::fs::read(&path).unwrap();
    let fields = kitab_mupdf::sigs::signed_fields(&path, None).unwrap();
    // 2026-11-07, inside the test certificates' validity.
    fields
        .iter()
        .map(|f| verify(&bytes, f, trust, 1_794_000_000))
        .collect()
}

fn test_root() -> Trust {
    let mut t = Trust::default();
    let pem = std::fs::read(dir().join("certs/kitab-test-root-ca.pem")).unwrap();
    assert_eq!(t.add(&pem).unwrap(), 1);
    t
}

#[test]
fn each_fixture_matches_expected_json() {
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir().join("../expected.json")).unwrap()).unwrap();
    let pdfs = expected.get("pdf").unwrap_or(&expected);
    let trust = test_root();
    for name in [
        "08a-signed-valid.pdf",
        "08b-signed-two-signers.pdf",
        "08c-signed-then-modified.pdf",
        "08d-signed-tampered.pdf",
    ] {
        let want = pdfs[name]["signatures"].as_array().unwrap();
        let got = checks(name, &trust);
        assert_eq!(got.len(), want.len(), "{name}");
        for (g, w) in got.iter().zip(want) {
            assert_eq!(g.field, w["field"].as_str().unwrap(), "{name}");
            assert_eq!(
                g.intact,
                w["intact"].as_bool().unwrap(),
                "{name} intact {g:?}"
            );
            assert_eq!(g.valid, w["valid"].as_bool().unwrap(), "{name} valid {g:?}");
            assert_eq!(g.signer.as_deref(), w["signer"].as_str(), "{name}");
            assert!(g.problem.is_none(), "{name}: {:?}", g.problem);
            assert!(!g.expired, "{name}");
            // Tampered bytes don't count as trusted, whatever the chain.
            if g.intact {
                assert_eq!(g.trusted, w["trusted"].as_bool().unwrap(), "{name}");
            }
        }
    }
}

#[test]
fn statuses_with_and_without_the_test_root() {
    let trust = test_root();
    let status = |name: &str, t: &Trust| {
        checks(name, t)
            .iter()
            .map(|c| c.status())
            .collect::<Vec<_>>()
    };
    assert_eq!(status("08a-signed-valid.pdf", &trust), [SigStatus::Valid]);
    assert_eq!(
        status("08b-signed-two-signers.pdf", &trust),
        [SigStatus::Valid, SigStatus::Valid]
    );
    assert_eq!(
        status("08c-signed-then-modified.pdf", &trust),
        [SigStatus::Changed]
    );
    assert_eq!(
        status("08d-signed-tampered.pdf", &trust),
        [SigStatus::Broken]
    );
    // Nobody trusted: the signer is unknown, the rest stands.
    let none = Trust::default();
    assert_eq!(status("08a-signed-valid.pdf", &none), [SigStatus::Unknown]);
    assert_eq!(
        status("08d-signed-tampered.pdf", &none),
        [SigStatus::Broken]
    );
    let a = &checks("08a-signed-valid.pdf", &none)[0];
    assert_eq!(a.signer_name.as_deref(), Some("R. Haque (test landlord)"));
    assert!(a.when.is_some());
}
