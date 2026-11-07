// SPDX-License-Identifier: AGPL-3.0-or-later

//! Digital signatures in a PDF (PLAN.md §3.7): what the engine reads from a
//! signature field, and what verifying it found.

/// How much of the file a signature's byte range covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigCoverage {
    /// Every byte but the signature itself: nothing was added after it.
    EntireFile,
    /// The file as it was when signed; later revisions were appended.
    EntireRevision,
    /// Gaps other than the signature: some bytes aren't protected.
    Partial,
}

/// What changed in the revisions after the signed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SigModification {
    None,
    /// Form fields filled or more signatures added.
    FormFilling,
    /// Comments and markup added or changed.
    Annotations,
    /// Anything else: pages, content, other objects.
    Other,
}

/// A signature field as the engine reads it, before any crypto.
#[derive(Debug, Clone, PartialEq)]
pub struct SignedField {
    /// Fully qualified field name ("Landlord", "Form.Sig1").
    pub field: String,
    /// 0-based page of its widget, if it has one.
    pub page: Option<u32>,
    /// `/ByteRange` as (offset, length) pairs.
    pub byte_range: Vec<(u64, u64)>,
    /// `/Contents`: the CMS/PKCS #7 blob (may carry zero padding).
    pub contents: Vec<u8>,
    /// `/SubFilter` (adbe.pkcs7.detached, ETSI.CAdES.detached, …).
    pub sub_filter: String,
    /// `/Name`, `/Reason`, `/Location`, `/M` as written by the signer.
    pub name: Option<String>,
    pub reason: Option<String>,
    pub location: Option<String>,
    pub when: Option<String>,
    pub coverage: SigCoverage,
    pub modification: SigModification,
}

/// What verifying one signature found.
#[derive(Debug, Clone, PartialEq)]
pub struct SignatureCheck {
    pub field: String,
    pub page: Option<u32>,
    /// The signer's subject, "Organization: …, Common Name: …".
    pub signer: Option<String>,
    /// The signer's common name alone, for short labels.
    pub signer_name: Option<String>,
    /// When it was signed (the signed attribute, else `/M`), as written.
    pub when: Option<String>,
    pub reason: Option<String>,
    pub location: Option<String>,
    /// The signed bytes still hash to the signed digest.
    pub intact: bool,
    /// The signature over the digest checks out with the signer's key.
    pub valid: bool,
    /// The signer's certificate chains to a certificate the user trusts.
    pub trusted: bool,
    /// The signer's certificate wasn't valid at the signing time.
    pub expired: bool,
    pub coverage: SigCoverage,
    pub modification: SigModification,
    /// Why it couldn't be checked, in words, when it couldn't.
    pub problem: Option<String>,
}

/// One word for a signature, as the status bar and panel show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SigStatus {
    /// Intact, valid, trusted, nothing but form filling since.
    Valid,
    /// Intact and valid, but the signer isn't trusted (or expired).
    Unknown,
    /// Intact and valid, but the document changed after signing.
    Changed,
    /// The bytes or the signature don't check out.
    Broken,
}

impl SignatureCheck {
    pub fn status(&self) -> SigStatus {
        if !self.intact || !self.valid || self.coverage == SigCoverage::Partial {
            SigStatus::Broken
        } else if self.modification > SigModification::FormFilling {
            SigStatus::Changed
        } else if !self.trusted || self.expired {
            SigStatus::Unknown
        } else {
            SigStatus::Valid
        }
    }
}

/// The document's overall signature status: its worst signature.
pub fn overall_status(checks: &[SignatureCheck]) -> Option<SigStatus> {
    checks.iter().map(SignatureCheck::status).max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check() -> SignatureCheck {
        SignatureCheck {
            field: "Sig".into(),
            page: Some(0),
            signer: None,
            signer_name: None,
            when: None,
            reason: None,
            location: None,
            intact: true,
            valid: true,
            trusted: true,
            expired: false,
            coverage: SigCoverage::EntireFile,
            modification: SigModification::None,
            problem: None,
        }
    }

    #[test]
    fn the_worst_finding_decides() {
        assert_eq!(check().status(), SigStatus::Valid);
        let filled = SignatureCheck {
            modification: SigModification::FormFilling,
            coverage: SigCoverage::EntireRevision,
            ..check()
        };
        assert_eq!(filled.status(), SigStatus::Valid);
        let untrusted = SignatureCheck {
            trusted: false,
            ..check()
        };
        assert_eq!(untrusted.status(), SigStatus::Unknown);
        let changed = SignatureCheck {
            modification: SigModification::Annotations,
            trusted: false,
            ..check()
        };
        assert_eq!(changed.status(), SigStatus::Changed);
        let tampered = SignatureCheck {
            intact: false,
            ..check()
        };
        assert_eq!(tampered.status(), SigStatus::Broken);
        assert_eq!(
            overall_status(&[check(), untrusted, tampered]),
            Some(SigStatus::Broken)
        );
        assert_eq!(overall_status(&[]), None);
    }
}
