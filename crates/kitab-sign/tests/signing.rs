// SPDX-License-Identifier: AGPL-3.0-or-later

//! I3–I4 (PLAN.md §7.2): Kitab signs a PDF with a .p12 key, its own
//! verifier finds the signature valid and trusted, and signing a file that
//! is already signed keeps the earlier signature valid (incremental).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kitab_core::{SigCoverage, SigModification, SigStatus};
use kitab_mupdf::sigs::{NewSignature, RANGE_MARK, SIG_ROOM, prepare_signature, signed_fields};
use kitab_sign::sign::{fill, load_pfx};
use kitab_sign::{Trust, verify};
use std::path::{Path, PathBuf};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdf")
}

const NOW: u64 = 1_794_000_000;

fn sign(src: &Path, out: &Path, p12: &str, field: &str, rect: [f32; 4]) {
    let key = load_pfx(
        &std::fs::read(dir().join("certs").join(p12)).unwrap(),
        "kitab",
    )
    .unwrap();
    let new = NewSignature {
        field: field.into(),
        page: 0,
        rect,
        lines: vec![
            format!("Digitally signed by {}", key.name()),
            "2026-11-07".into(),
        ],
        name: Some(key.name()),
        reason: Some("I agree".into()),
        location: Some("Dhaka".into()),
        when: "D:20261107120000Z".into(),
    };
    let mut pdf = prepare_signature(src, &new).unwrap();
    fill(&mut pdf, RANGE_MARK, SIG_ROOM, &key, NOW).unwrap();
    std::fs::write(out, pdf).unwrap();
}

fn trust() -> Trust {
    let mut t = Trust::default();
    t.add(&std::fs::read(dir().join("certs/kitab-test-root-ca.pem")).unwrap())
        .unwrap();
    t
}

#[test]
fn signs_and_signs_again() {
    let tmp = std::env::temp_dir().join(format!("kitab-signing-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let one = tmp.join("one.pdf");
    let two = tmp.join("two.pdf");
    sign(
        &dir().join("08-lease-unsigned.pdf"),
        &one,
        "landlord.p12",
        "Kitab1",
        [72.0, 240.0, 272.0, 300.0],
    );
    let bytes = std::fs::read(&one).unwrap();
    let fields = signed_fields(&one, None).unwrap();
    assert_eq!(fields.len(), 1);
    let c = verify(&bytes, &fields[0], &trust(), NOW);
    assert_eq!(c.problem, None);
    assert!(c.intact && c.valid && c.trusted && !c.expired, "{c:?}");
    assert_eq!(c.coverage, SigCoverage::EntireFile);
    assert_eq!(c.status(), SigStatus::Valid);
    assert_eq!(c.reason.as_deref(), Some("I agree"));
    assert_eq!(c.signer_name.as_deref(), Some("R. Haque (test landlord)"));

    // A second signer: the first signature stays valid.
    sign(
        &one,
        &two,
        "tenant.p12",
        "Kitab2",
        [320.0, 240.0, 520.0, 300.0],
    );
    let bytes = std::fs::read(&two).unwrap();
    assert!(bytes.starts_with(&std::fs::read(&one).unwrap()));
    let fields = signed_fields(&two, None).unwrap();
    assert_eq!(fields.len(), 2);
    let checks: Vec<_> = fields
        .iter()
        .map(|f| verify(&bytes, f, &trust(), NOW))
        .collect();
    assert_eq!(checks[0].modification, SigModification::FormFilling);
    assert!(
        checks.iter().all(|c| c.status() == SigStatus::Valid),
        "{checks:?}"
    );
    // Kept for checking with other verifiers by hand.
    let _ = std::fs::copy(&two, std::env::temp_dir().join("kitab-signed-sample.pdf"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn a_wrong_password_says_so() {
    let p12 = std::fs::read(dir().join("certs/landlord.p12")).unwrap();
    let e = load_pfx(&p12, "nope").unwrap_err();
    assert!(e.contains("password"), "{e}");
}
