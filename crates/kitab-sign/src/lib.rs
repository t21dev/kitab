// SPDX-License-Identifier: AGPL-3.0-or-later

//! Signature crypto for Kitab Reader (PLAN.md §3.7), in pure Rust: the CMS
//! (PKCS #7) blob of a PDF signature is parsed, the signed bytes are hashed
//! and compared with the signed digest, the signature over the signed
//! attributes is checked with the signer's key (ECDSA P-256/P-384, RSA
//! PKCS #1 v1.5 and PSS), and the signer's certificate is chained to the
//! certificates the user trusts. Nothing here touches the network.

mod algo;
mod names;
pub mod store;

pub use names::{common_name, subject_line};

use cms::cert::CertificateChoices;
use cms::content_info::ContentInfo;
use cms::signed_data::{SignedData, SignerIdentifier, SignerInfo};
use der::asn1::OctetString;
use der::{Decode, Encode};
use kitab_core::{SignatureCheck, SignedField};
use x509_cert::Certificate;

/// The certificates the user trusts (Settings › Signing), and the time to
/// check against when a signature carries none.
#[derive(Default, Clone)]
pub struct Trust {
    roots: Vec<Certificate>,
}

impl Trust {
    /// Trust these PEM or DER certificates (one file may hold several).
    pub fn add(&mut self, bytes: &[u8]) -> Result<usize, String> {
        let certs = parse_certs(bytes)?;
        let n = certs.len();
        self.roots.extend(certs);
        Ok(n)
    }

    pub fn len(&self) -> usize {
        self.roots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
}

/// One or more certificates from PEM text or a DER file.
pub fn parse_certs(bytes: &[u8]) -> Result<Vec<Certificate>, String> {
    if bytes.starts_with(b"-----") || bytes.windows(11).any(|w| w == b"-----BEGIN ") {
        let certs =
            Certificate::load_pem_chain(bytes).map_err(|e| format!("not a certificate: {e}"))?;
        if certs.is_empty() {
            return Err("no certificate in the file".into());
        }
        return Ok(certs);
    }
    Certificate::from_der(bytes)
        .map(|c| vec![c])
        .map_err(|e| format!("not a certificate: {e}"))
}

/// A certificate's subject line and SHA-256 fingerprint (hex), for lists.
pub fn describe(cert: &Certificate) -> (String, String) {
    let der = cert.to_der().unwrap_or_default();
    let fp = algo::Hash::Sha256.digest(&[&der]);
    let hex: Vec<String> = fp.iter().map(|b| format!("{b:02X}")).collect();
    (subject_line(&cert.tbs_certificate.subject), hex.join(":"))
}

/// The length of the DER value at the start of `b`, so the zero padding
/// PDFs put after the CMS blob is left out.
fn der_extent(b: &[u8]) -> Option<usize> {
    let first = *b.get(1)?;
    if first < 0x80 {
        return Some(2 + first as usize);
    }
    let n = (first & 0x7f) as usize;
    if n == 0 || n > 4 {
        return None;
    }
    let mut len = 0usize;
    for i in 0..n {
        len = (len << 8) | *b.get(2 + i)? as usize;
    }
    Some(2 + n + len)
}

const SIGNED_DATA: der::asn1::ObjectIdentifier =
    der::asn1::ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const MESSAGE_DIGEST: der::asn1::ObjectIdentifier =
    der::asn1::ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const SIGNING_TIME: der::asn1::ObjectIdentifier =
    der::asn1::ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");
const SUBJECT_KEY_ID: der::asn1::ObjectIdentifier =
    der::asn1::ObjectIdentifier::new_unwrap("2.5.29.14");

/// Verify one signature field against the whole file's bytes.
pub fn verify(file: &[u8], field: &SignedField, trust: &Trust, now_unix: u64) -> SignatureCheck {
    let mut check = SignatureCheck {
        field: field.field.clone(),
        page: field.page,
        signer: field.name.clone(),
        signer_name: field.name.clone(),
        when: field.when.as_deref().map(pdf_date),
        reason: field.reason.clone(),
        location: field.location.clone(),
        intact: false,
        valid: false,
        trusted: false,
        expired: false,
        coverage: field.coverage,
        modification: field.modification,
        problem: None,
    };
    if let Err(why) = check_crypto(file, field, trust, now_unix, &mut check) {
        check.problem = Some(why);
    }
    check
}

fn check_crypto(
    file: &[u8],
    field: &SignedField,
    trust: &Trust,
    now_unix: u64,
    check: &mut SignatureCheck,
) -> Result<(), String> {
    // The signed bytes, in order, all inside the file.
    let mut parts: Vec<&[u8]> = Vec::new();
    for &(at, len) in &field.byte_range {
        let (at, len) = (at as usize, len as usize);
        let part = file
            .get(at..at.checked_add(len).ok_or("bad byte range")?)
            .ok_or("the byte range runs past the end of the file")?;
        parts.push(part);
    }
    if parts.is_empty() {
        return Err("the signature has no byte range".into());
    }
    let blob = &field.contents;
    let end = der_extent(blob).ok_or("the signature is empty")?;
    let blob = blob.get(..end).ok_or("the signature is cut short")?;
    let info = ContentInfo::from_der(blob).map_err(|e| format!("unreadable signature: {e}"))?;
    if info.content_type != SIGNED_DATA {
        return Err("not a CMS signed-data signature".into());
    }
    let sd: SignedData = info
        .content
        .decode_as()
        .map_err(|e| format!("unreadable signed data: {e}"))?;
    let certs: Vec<Certificate> = sd
        .certificates
        .as_ref()
        .map(|set| {
            set.0
                .iter()
                .filter_map(|c| match c {
                    CertificateChoices::Certificate(c) => Some(c.clone()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let si = sd
        .signer_infos
        .0
        .iter()
        .next()
        .ok_or("the signature has no signer")?;
    let signer =
        find_signer(si, &certs).ok_or("the signer's certificate isn't in the signature")?;
    let subject = &signer.tbs_certificate.subject;
    check.signer = Some(subject_line(subject));
    check.signer_name = common_name(subject).or_else(|| field.name.clone());

    let hash = algo::Hash::from_oid(&si.digest_alg.oid).ok_or("an unknown digest algorithm")?;
    let digest = hash.digest(&parts);
    // What was signed: the signed attributes, which carry the digest; or,
    // without them, the digest itself.
    let (message, signing_time) = match si.signed_attrs.as_ref() {
        Some(attrs) => {
            let mut signed_digest = None;
            let mut time = None;
            for a in attrs.iter() {
                let Some(v) = a.values.iter().next() else {
                    continue;
                };
                if a.oid == MESSAGE_DIGEST {
                    signed_digest = v
                        .decode_as::<OctetString>()
                        .ok()
                        .map(|o| o.as_bytes().to_vec());
                } else if a.oid == SIGNING_TIME {
                    time = v
                        .to_der()
                        .ok()
                        .and_then(|d| x509_cert::time::Time::from_der(&d).ok());
                }
            }
            check.intact = signed_digest.as_deref() == Some(digest.as_slice());
            let der = attrs.to_der().map_err(|e| e.to_string())?;
            (der, time)
        }
        None => {
            // adbe.pkcs7.detached without attributes signs the data itself.
            check.intact = true;
            (parts.concat(), None)
        }
    };
    if let Some(t) = signing_time {
        check.when = Some(format_unix(t.to_unix_duration().as_secs()));
    }
    let spki = &signer.tbs_certificate.subject_public_key_info;
    check.valid = algo::verify(
        &si.signature_algorithm,
        hash,
        spki,
        &[&message],
        si.signature.as_bytes(),
    )
    .unwrap_or(false);
    // The certificate was valid when signed (or now, with no time signed).
    let at = signing_time.map_or(now_unix, |t| t.to_unix_duration().as_secs());
    let v = &signer.tbs_certificate.validity;
    check.expired = at < v.not_before.to_unix_duration().as_secs()
        || at > v.not_after.to_unix_duration().as_secs();
    check.trusted = chains_to_trust(&signer, &certs, trust);
    Ok(())
}

fn find_signer(si: &SignerInfo, certs: &[Certificate]) -> Option<Certificate> {
    certs
        .iter()
        .find(|c| match &si.sid {
            SignerIdentifier::IssuerAndSerialNumber(id) => {
                c.tbs_certificate.issuer == id.issuer
                    && c.tbs_certificate.serial_number == id.serial_number
            }
            SignerIdentifier::SubjectKeyIdentifier(ski) => {
                key_id(c).is_some_and(|k| k == ski.0.as_bytes())
            }
        })
        .cloned()
}

fn key_id(c: &Certificate) -> Option<Vec<u8>> {
    let ext = c
        .tbs_certificate
        .extensions
        .as_ref()?
        .iter()
        .find(|e| e.extn_id == SUBJECT_KEY_ID)?;
    OctetString::from_der(ext.extn_value.as_bytes())
        .ok()
        .map(|o| o.as_bytes().to_vec())
}

/// Whether `cert` is trusted, or is signed by a chain (through the
/// signature's own certificates) that ends at a trusted certificate.
fn chains_to_trust(cert: &Certificate, pool: &[Certificate], trust: &Trust) -> bool {
    let mut current = cert.clone();
    for _ in 0..8 {
        let der = current.to_der().unwrap_or_default();
        if trust
            .roots
            .iter()
            .any(|r| r.to_der().ok().as_deref() == Some(der.as_slice()))
        {
            return true;
        }
        let issuer = &current.tbs_certificate.issuer;
        // Signed by a trusted certificate?
        if trust
            .roots
            .iter()
            .any(|r| &r.tbs_certificate.subject == issuer && signed_by(&current, r))
        {
            return true;
        }
        // Up one step through the signature's own certificates.
        let next = pool.iter().find(|c| {
            &c.tbs_certificate.subject == issuer
                && c.tbs_certificate.subject != current.tbs_certificate.subject
                && signed_by(&current, c)
        });
        match next {
            Some(n) => current = n.clone(),
            None => return false,
        }
    }
    false
}

fn signed_by(cert: &Certificate, issuer: &Certificate) -> bool {
    let Ok(tbs) = cert.tbs_certificate.to_der() else {
        return false;
    };
    let Some(sig) = cert.signature.as_bytes() else {
        return false;
    };
    let hash =
        algo::Hash::for_signature(&cert.signature_algorithm.oid).unwrap_or(algo::Hash::Sha256);
    algo::verify(
        &cert.signature_algorithm,
        hash,
        &issuer.tbs_certificate.subject_public_key_info,
        &[&tbs],
        sig,
    )
    .unwrap_or(false)
}

/// A PDF date ("D:20261107093000+06'00'") as "2026-11-07 09:30 +06:00".
pub fn pdf_date(d: &str) -> String {
    let s = d.trim().trim_start_matches("D:");
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 8 {
        return d.to_string();
    }
    let part = |a: usize, b: usize| digits.get(a..b).unwrap_or("00");
    let mut out = format!("{}-{}-{}", part(0, 4), part(4, 6), part(6, 8));
    if digits.len() >= 12 {
        out.push_str(&format!(" {}:{}", part(8, 10), part(10, 12)));
    }
    let zone = &s[digits.len()..];
    if zone.starts_with('Z') {
        out.push_str(" UTC");
    } else if let Some(sign) = zone.chars().next().filter(|c| *c == '+' || *c == '-') {
        let z: String = zone.chars().filter(char::is_ascii_digit).collect();
        if z.len() >= 4 {
            out.push_str(&format!(" {sign}{}:{}", &z[..2], &z[2..4]));
        }
    }
    out
}

/// Seconds since 1970 as "2026-11-07 09:30 UTC".
pub fn format_unix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_plainly() {
        assert_eq!(
            pdf_date("D:20261107093000+06'00'"),
            "2026-11-07 09:30 +06:00"
        );
        assert_eq!(pdf_date("D:20240102Z"), "2024-01-02 UTC");
        assert_eq!(pdf_date("junk"), "junk");
        assert_eq!(format_unix(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_unix(1_793_000_000), "2026-10-26 07:33 UTC");
    }

    #[test]
    fn der_extent_skips_padding() {
        assert_eq!(der_extent(&[0x30, 0x03, 1, 2, 3, 0, 0]), Some(5));
        assert_eq!(der_extent(&[0x30, 0x82, 0x01, 0x00]), Some(260));
        assert_eq!(der_extent(&[0x30]), None);
    }
}
