// SPDX-License-Identifier: AGPL-3.0-or-later

//! The certificates the user trusts (Settings › Signing): one PEM file per
//! certificate in a folder of Kitab's data, named by its fingerprint.
//! Nothing is trusted until the user adds it; the system's own certificate
//! stores aren't read.

use crate::{Trust, describe, parse_certs};
use der::EncodePem;
use std::path::{Path, PathBuf};

/// A trusted certificate as Settings lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trusted {
    pub subject: String,
    /// SHA-256, hex pairs with colons.
    pub fingerprint: String,
    pub path: PathBuf,
}

/// Everything in `dir` that parses as a certificate.
pub fn list(dir: &Path) -> Vec<Trusted> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Trusted> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "pem"))
        .filter_map(|p| {
            let bytes = std::fs::read(&p).ok()?;
            let cert = parse_certs(&bytes).ok()?.into_iter().next()?;
            let (subject, fingerprint) = describe(&cert);
            Some(Trusted {
                subject,
                fingerprint,
                path: p,
            })
        })
        .collect();
    out.sort_by(|a, b| a.subject.cmp(&b.subject));
    out
}

/// The trust for verifying: every certificate in `dir`.
pub fn load(dir: &Path) -> Trust {
    let mut t = Trust::default();
    for c in list(dir) {
        if let Ok(bytes) = std::fs::read(&c.path) {
            let _ = t.add(&bytes);
        }
    }
    t
}

/// Trust the certificates in a PEM or DER file: each is written to `dir`.
/// Returns the subjects added.
pub fn add(dir: &Path, bytes: &[u8]) -> Result<Vec<String>, String> {
    let certs = parse_certs(bytes)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut added = Vec::new();
    for cert in certs {
        let (subject, fingerprint) = describe(&cert);
        let name: String = fingerprint.chars().filter(|c| *c != ':').take(16).collect();
        let pem = cert
            .to_pem(der::pem::LineEnding::LF)
            .map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.pem", name.to_lowercase()));
        std::fs::write(&path, pem).map_err(|e| format!("{}: {e}", path.display()))?;
        added.push(subject);
    }
    Ok(added)
}

/// Stop trusting one certificate (a file `list` returned).
pub fn remove(dir: &Path, path: &Path) -> Result<(), String> {
    // Only files inside the trust folder.
    if path.parent() != Some(dir) {
        return Err("that certificate isn't in Kitab's trust list".into());
    }
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificates_are_added_listed_and_removed() {
        let dir = std::env::temp_dir().join(format!("kitab-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/pdf/certs/kitab-test-root-ca.pem"),
        )
        .unwrap_or_default();
        assert!(list(&dir).is_empty());
        let added = add(&dir, &root).unwrap_or_default();
        assert_eq!(added.len(), 1);
        let listed = list(&dir);
        assert_eq!(listed.len(), 1);
        assert!(
            listed[0].subject.contains("Kitab Test"),
            "{}",
            listed[0].subject
        );
        assert_eq!(listed[0].fingerprint.len(), 32 * 3 - 1);
        assert_eq!(load(&dir).len(), 1);
        assert!(add(&dir, b"not a certificate").is_err());
        assert!(remove(&dir, Path::new("elsewhere.pem")).is_err());
        remove(&dir, &listed[0].path).unwrap_or_default();
        assert!(list(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
