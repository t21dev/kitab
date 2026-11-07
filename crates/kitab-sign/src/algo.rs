// SPDX-License-Identifier: AGPL-3.0-or-later

//! Hashes and signature algorithms by OID.

use der::asn1::ObjectIdentifier as Oid;
use sha2::Digest;
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hash {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

const fn oid(s: &str) -> Oid {
    Oid::new_unwrap(s)
}

const SHA1: Oid = oid("1.3.14.3.2.26");
const SHA256: Oid = oid("2.16.840.1.101.3.4.2.1");
const SHA384: Oid = oid("2.16.840.1.101.3.4.2.2");
const SHA512: Oid = oid("2.16.840.1.101.3.4.2.3");

const RSA: Oid = oid("1.2.840.113549.1.1.1");
const RSA_SHA1: Oid = oid("1.2.840.113549.1.1.5");
const RSA_SHA256: Oid = oid("1.2.840.113549.1.1.11");
const RSA_SHA384: Oid = oid("1.2.840.113549.1.1.12");
const RSA_SHA512: Oid = oid("1.2.840.113549.1.1.13");
const RSA_PSS: Oid = oid("1.2.840.113549.1.1.10");
const EC_KEY: Oid = oid("1.2.840.10045.2.1");
const ECDSA_SHA1: Oid = oid("1.2.840.10045.4.1");
const ECDSA_SHA256: Oid = oid("1.2.840.10045.4.3.2");
const ECDSA_SHA384: Oid = oid("1.2.840.10045.4.3.3");
const ECDSA_SHA512: Oid = oid("1.2.840.10045.4.3.4");
const P256: Oid = oid("1.2.840.10045.3.1.7");
const P384: Oid = oid("1.3.132.0.34");

impl Hash {
    pub fn from_oid(o: &Oid) -> Option<Hash> {
        Some(match *o {
            SHA1 => Hash::Sha1,
            SHA256 => Hash::Sha256,
            SHA384 => Hash::Sha384,
            SHA512 => Hash::Sha512,
            _ => return None,
        })
    }

    /// The hash a combined signature algorithm names (sha256WithRSA…).
    pub fn for_signature(o: &Oid) -> Option<Hash> {
        Some(match *o {
            RSA_SHA1 | ECDSA_SHA1 => Hash::Sha1,
            RSA_SHA256 | ECDSA_SHA256 => Hash::Sha256,
            RSA_SHA384 | ECDSA_SHA384 => Hash::Sha384,
            RSA_SHA512 | ECDSA_SHA512 => Hash::Sha512,
            _ => return None,
        })
    }

    pub fn digest(self, parts: &[&[u8]]) -> Vec<u8> {
        fn run<D: Digest>(parts: &[&[u8]]) -> Vec<u8> {
            let mut h = D::new();
            for p in parts {
                h.update(p);
            }
            h.finalize().to_vec()
        }
        match self {
            Hash::Sha1 => run::<sha1::Sha1>(parts),
            Hash::Sha256 => run::<sha2::Sha256>(parts),
            Hash::Sha384 => run::<sha2::Sha384>(parts),
            Hash::Sha512 => run::<sha2::Sha512>(parts),
        }
    }
}

/// Check `sig` over `message` (in parts) with `key`. `hash` is the digest
/// algorithm, unless the signature algorithm names its own.
pub fn verify(
    alg: &AlgorithmIdentifierOwned,
    hash: Hash,
    key: &SubjectPublicKeyInfoOwned,
    message: &[&[u8]],
    sig: &[u8],
) -> Result<bool, String> {
    let hash = Hash::for_signature(&alg.oid).unwrap_or(hash);
    let digest = hash.digest(message);
    let key_bytes = key
        .subject_public_key
        .as_bytes()
        .ok_or("the public key isn't whole bytes")?;
    match key.algorithm.oid {
        EC_KEY => {
            let curve = key
                .algorithm
                .parameters
                .as_ref()
                .and_then(|p| p.decode_as::<Oid>().ok())
                .ok_or("an EC key without a curve")?;
            use p256::ecdsa::signature::hazmat::PrehashVerifier;
            match curve {
                P256 => {
                    let vk = p256::ecdsa::VerifyingKey::from_sec1_bytes(key_bytes)
                        .map_err(|e| e.to_string())?;
                    let s = p256::ecdsa::Signature::from_der(sig).map_err(|e| e.to_string())?;
                    Ok(vk.verify_prehash(&digest, &s).is_ok())
                }
                P384 => {
                    let vk = p384::ecdsa::VerifyingKey::from_sec1_bytes(key_bytes)
                        .map_err(|e| e.to_string())?;
                    let s = p384::ecdsa::Signature::from_der(sig).map_err(|e| e.to_string())?;
                    Ok(vk.verify_prehash(&digest, &s).is_ok())
                }
                _ => Err("an elliptic curve Kitab doesn't check yet".into()),
            }
        }
        RSA => {
            use rsa::pkcs1::DecodeRsaPublicKey;
            let pk = rsa::RsaPublicKey::from_pkcs1_der(key_bytes).map_err(|e| e.to_string())?;
            let ok = if alg.oid == RSA_PSS {
                let pss_hash = pss_hash(alg).unwrap_or(hash);
                let digest = pss_hash.digest(message);
                match pss_hash {
                    Hash::Sha1 => pk.verify(rsa::Pss::new::<sha1::Sha1>(), &digest, sig),
                    Hash::Sha256 => pk.verify(rsa::Pss::new::<sha2::Sha256>(), &digest, sig),
                    Hash::Sha384 => pk.verify(rsa::Pss::new::<sha2::Sha384>(), &digest, sig),
                    Hash::Sha512 => pk.verify(rsa::Pss::new::<sha2::Sha512>(), &digest, sig),
                }
            } else {
                let scheme = match hash {
                    Hash::Sha1 => rsa::Pkcs1v15Sign::new::<sha1::Sha1>(),
                    Hash::Sha256 => rsa::Pkcs1v15Sign::new::<sha2::Sha256>(),
                    Hash::Sha384 => rsa::Pkcs1v15Sign::new::<sha2::Sha384>(),
                    Hash::Sha512 => rsa::Pkcs1v15Sign::new::<sha2::Sha512>(),
                };
                pk.verify(scheme, &digest, sig)
            };
            Ok(ok.is_ok())
        }
        _ => Err("a key type Kitab doesn't check yet".into()),
    }
}

/// RSASSA-PSS parameters: the hash, from `[0] hashAlgorithm`.
fn pss_hash(alg: &AlgorithmIdentifierOwned) -> Option<Hash> {
    let params = alg.parameters.as_ref()?;
    // SEQUENCE { [0] EXPLICIT AlgorithmIdentifier, … }; find the first OID.
    let bytes = params.value();
    let at = bytes.windows(2).position(|w| w[0] == 0x06)?;
    let len = *bytes.get(at + 1)? as usize;
    let o = Oid::from_bytes(bytes.get(at + 2..at + 2 + len)?).ok()?;
    Hash::from_oid(&o)
}
