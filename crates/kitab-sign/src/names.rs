// SPDX-License-Identifier: AGPL-3.0-or-later

//! Distinguished names in words: "Organization: Kitab Test Suite, Common
//! Name: R. Haque".

use der::{Tag, Tagged};
use x509_cert::name::Name;

fn label(oid: &str) -> Option<&'static str> {
    Some(match oid {
        "2.5.4.3" => "Common Name",
        "2.5.4.10" => "Organization",
        "2.5.4.11" => "Organizational Unit",
        "2.5.4.6" => "Country",
        "2.5.4.7" => "Locality",
        "2.5.4.8" => "State",
        "2.5.4.5" => "Serial Number",
        "1.2.840.113549.1.9.1" => "Email",
        _ => return None,
    })
}

/// The (label, value) pairs of a name, outermost first.
fn parts(name: &Name) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for rdn in name.0.iter() {
        for atv in rdn.0.iter() {
            let Some(l) = label(&atv.oid.to_string()) else {
                continue;
            };
            let v = &atv.value;
            let text = match v.tag() {
                Tag::Utf8String | Tag::PrintableString | Tag::Ia5String | Tag::TeletexString => {
                    String::from_utf8_lossy(v.value()).to_string()
                }
                Tag::BmpString => {
                    let units: Vec<u16> = v
                        .value()
                        .chunks(2)
                        .filter(|c| c.len() == 2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .collect();
                    String::from_utf16_lossy(&units)
                }
                _ => continue,
            };
            out.push((l, text));
        }
    }
    out
}

/// The whole subject, broadest last as RFC 4514 writes it.
pub fn subject_line(name: &Name) -> String {
    parts(name)
        .into_iter()
        .rev()
        .map(|(l, v)| format!("{l}: {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The common name alone.
pub fn common_name(name: &Name) -> Option<String> {
    parts(name)
        .into_iter()
        .find(|(l, _)| *l == "Common Name")
        .map(|(_, v)| v)
}
