// SPDX-License-Identifier: AGPL-3.0-or-later

//! Signing (PLAN.md §3.7): a key and certificate from a `.pfx`/`.p12` file,
//! a CMS (PKCS #7, adbe.pkcs7.detached) signature over the PDF's byte
//! range, written into the placeholder the engine left. The key lives in
//! memory for one signature; its password is never kept.

use crate::algo::Hash;
use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{
    CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos,
};
use der::asn1::{ObjectIdentifier as Oid, OctetString, SetOfVec, UtcTime};
use der::{Any, Decode, Encode};
use spki::AlgorithmIdentifierOwned;
use x509_cert::Certificate;
use x509_cert::attr::Attribute;

const ID_DATA: Oid = Oid::new_unwrap("1.2.840.113549.1.7.1");
const CONTENT_TYPE: Oid = Oid::new_unwrap("1.2.840.113549.1.9.3");
const SHA256: Oid = Oid::new_unwrap("2.16.840.1.101.3.4.2.1");
const SHA384: Oid = Oid::new_unwrap("2.16.840.1.101.3.4.2.2");
const ECDSA_SHA256: Oid = Oid::new_unwrap("1.2.840.10045.4.3.2");
const ECDSA_SHA384: Oid = Oid::new_unwrap("1.2.840.10045.4.3.3");
const RSA_SHA256: Oid = Oid::new_unwrap("1.2.840.113549.1.1.11");

enum Key {
    P256(p256::ecdsa::SigningKey),
    P384(p384::ecdsa::SigningKey),
    Rsa(Box<rsa::RsaPrivateKey>),
}

/// A key ready to sign, with its certificate and the chain that came with it.
pub struct SigningKey {
    key: Key,
    cert: Certificate,
    chain: Vec<Certificate>,
}

impl std::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the key.
        f.debug_struct("SigningKey")
            .field("subject", &self.subject())
            .finish_non_exhaustive()
    }
}

/// Open a `.pfx`/`.p12` file with its password.
pub fn load_pfx(bytes: &[u8], password: &str) -> Result<SigningKey, String> {
    let store = p12_keystore::KeyStore::from_pkcs12(
        bytes,
        password,
        p12_keystore::Pkcs12ImportPolicy::Relaxed,
    )
    .map_err(|e| match e {
        p12_keystore::error::Error::MacError(_) => "the password is wrong".to_string(),
        other => format!("not a certificate file Kitab can read ({other})"),
    })?;
    let (_, chain) = store
        .private_key_chain()
        .ok_or("the file has no private key to sign with")?;
    let pkcs8 = chain.key().as_der();
    let key = {
        use p256::pkcs8::DecodePrivateKey;
        if let Ok(k) = p256::ecdsa::SigningKey::from_pkcs8_der(pkcs8) {
            Key::P256(k)
        } else if let Ok(k) = p384::ecdsa::SigningKey::from_pkcs8_der(pkcs8) {
            Key::P384(k)
        } else if let Ok(k) = rsa::RsaPrivateKey::from_pkcs8_der(pkcs8) {
            Key::Rsa(Box::new(k))
        } else {
            return Err(
                "a key type Kitab can't sign with yet (ECDSA P-256/P-384 and RSA work)".into(),
            );
        }
    };
    let certs: Vec<Certificate> = chain
        .certs()
        .iter()
        .filter_map(|c| Certificate::from_der(c.as_der()).ok())
        .collect();
    let public = public_key_raw(&key)?;
    let cert = certs
        .iter()
        .find(|c| {
            c.tbs_certificate
                .subject_public_key_info
                .subject_public_key
                .as_bytes()
                == Some(public.as_slice())
        })
        .cloned()
        .ok_or("no certificate in the file matches its key")?;
    let chain = certs.into_iter().filter(|c| *c != cert).collect();
    Ok(SigningKey { key, cert, chain })
}

/// The raw public key, as a certificate's subjectPublicKey holds it.
fn public_key_raw(key: &Key) -> Result<Vec<u8>, String> {
    Ok(match key {
        Key::P256(k) => k
            .verifying_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec(),
        Key::P384(k) => k
            .verifying_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec(),
        Key::Rsa(k) => {
            use rsa::pkcs1::EncodeRsaPublicKey;
            rsa::RsaPublicKey::from(k.as_ref())
                .to_pkcs1_der()
                .map_err(|e| e.to_string())?
                .as_bytes()
                .to_vec()
        }
    })
}

impl SigningKey {
    /// "Organization: …, Common Name: …".
    pub fn subject(&self) -> String {
        crate::subject_line(&self.cert.tbs_certificate.subject)
    }

    /// The common name, for the signature's look and `/Name`.
    pub fn name(&self) -> String {
        crate::common_name(&self.cert.tbs_certificate.subject).unwrap_or_else(|| self.subject())
    }

    fn hash(&self) -> Hash {
        match self.key {
            Key::P384(_) => Hash::Sha384,
            _ => Hash::Sha256,
        }
    }

    /// A detached CMS signature over `parts`, signed at `unix` seconds.
    pub fn cms(&self, parts: &[&[u8]], unix: u64) -> Result<Vec<u8>, String> {
        let hash = self.hash();
        let digest = hash.digest(parts);
        let attr = |oid: Oid, v: Any| -> Result<Attribute, String> {
            Ok(Attribute {
                oid,
                values: SetOfVec::try_from(vec![v]).map_err(|e| e.to_string())?,
            })
        };
        let time = UtcTime::from_unix_duration(std::time::Duration::from_secs(unix))
            .map_err(|e| e.to_string())?;
        let attrs = SetOfVec::try_from(vec![
            attr(CONTENT_TYPE, any(&ID_DATA)?)?,
            attr(crate::SIGNING_TIME, any(&time)?)?,
            attr(
                crate::MESSAGE_DIGEST,
                any(&OctetString::new(digest).map_err(|e| e.to_string())?)?,
            )?,
        ])
        .map_err(|e| e.to_string())?;
        let signed = attrs.to_der().map_err(|e| e.to_string())?;
        let (sig, sig_alg) = match &self.key {
            Key::P256(k) => {
                use p256::ecdsa::signature::Signer;
                let s: p256::ecdsa::DerSignature = k.sign(&signed);
                (s.as_bytes().to_vec(), ECDSA_SHA256)
            }
            Key::P384(k) => {
                use p384::ecdsa::signature::Signer;
                let s: p384::ecdsa::DerSignature = k.sign(&signed);
                (s.as_bytes().to_vec(), ECDSA_SHA384)
            }
            Key::Rsa(k) => {
                let d = Hash::Sha256.digest(&[&signed]);
                let s = k
                    .sign_with_rng(
                        &mut rand_core::OsRng,
                        rsa::Pkcs1v15Sign::new::<sha2::Sha256>(),
                        &d,
                    )
                    .map_err(|e| e.to_string())?;
                (s, RSA_SHA256)
            }
        };
        let digest_alg = AlgorithmIdentifierOwned {
            oid: if hash == Hash::Sha384 { SHA384 } else { SHA256 },
            parameters: None,
        };
        let signer = SignerInfo {
            version: CmsVersion::V1,
            sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
                issuer: self.cert.tbs_certificate.issuer.clone(),
                serial_number: self.cert.tbs_certificate.serial_number.clone(),
            }),
            digest_alg: digest_alg.clone(),
            signed_attrs: Some(attrs),
            signature_algorithm: AlgorithmIdentifierOwned {
                oid: sig_alg,
                parameters: if sig_alg == RSA_SHA256 {
                    Some(Any::null())
                } else {
                    None
                },
            },
            signature: OctetString::new(sig).map_err(|e| e.to_string())?,
            unsigned_attrs: None,
        };
        let certs: Vec<CertificateChoices> = std::iter::once(&self.cert)
            .chain(&self.chain)
            .map(|c| CertificateChoices::Certificate(c.clone()))
            .collect();
        let sd = SignedData {
            version: CmsVersion::V1,
            digest_algorithms: SetOfVec::try_from(vec![digest_alg]).map_err(|e| e.to_string())?,
            encap_content_info: EncapsulatedContentInfo {
                econtent_type: ID_DATA,
                econtent: None,
            },
            certificates: Some(CertificateSet(
                SetOfVec::try_from(certs).map_err(|e| e.to_string())?,
            )),
            crls: None,
            signer_infos: SignerInfos(SetOfVec::try_from(vec![signer]).map_err(|e| e.to_string())?),
        };
        let info = ContentInfo {
            content_type: crate::SIGNED_DATA,
            content: any(&sd)?,
        };
        info.to_der().map_err(|e| e.to_string())
    }
}

/// Any value as `Any`, for attribute values and the content.
fn any<T: Encode>(v: &T) -> Result<Any, String> {
    let der = v.to_der().map_err(|e| e.to_string())?;
    Any::from_der(&der).map_err(|e| e.to_string())
}

/// Finish a prepared PDF: write the byte range over its placeholder, sign
/// those bytes and write the signature into the zeroed `/Contents`.
pub fn fill(
    pdf: &mut [u8],
    range_mark: &str,
    room: usize,
    key: &SigningKey,
    unix: u64,
) -> Result<(), String> {
    let find_last =
        |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).rposition(|w| w == needle);
    let mark_at = find_last(pdf, range_mark.as_bytes()).ok_or("no byte-range placeholder")?;
    let open = pdf[..mark_at]
        .iter()
        .rposition(|b| *b == b'[')
        .ok_or("no byte-range placeholder")?;
    let close = mark_at
        + pdf[mark_at..]
            .iter()
            .position(|b| *b == b']')
            .ok_or("no byte-range placeholder")?;
    let zeros = vec![b'0'; room * 2];
    let hex_at = find_last(pdf, &zeros).ok_or("no signature placeholder")?;
    let start = hex_at
        .checked_sub(1)
        .filter(|i| pdf[*i] == b'<')
        .ok_or("no signature placeholder")?;
    let end = hex_at + zeros.len();
    if pdf.get(end) != Some(&b'>') {
        return Err("no signature placeholder".into());
    }
    let end = end + 1;
    let len = pdf.len();
    let range = format!("[0 {start} {end} {}", len - end);
    let width = close - open;
    if range.len() > width {
        return Err("the byte range doesn't fit its placeholder".into());
    }
    let padded = format!("{range:<width$}");
    pdf[open..close].copy_from_slice(padded.as_bytes());
    let cms = key.cms(&[&pdf[..start], &pdf[end..]], unix)?;
    if cms.len() > room {
        return Err("the signature is bigger than the room left for it".into());
    }
    let hex: String = cms.iter().map(|b| format!("{b:02X}")).collect();
    pdf[hex_at..hex_at + hex.len()].copy_from_slice(hex.as_bytes());
    Ok(())
}
