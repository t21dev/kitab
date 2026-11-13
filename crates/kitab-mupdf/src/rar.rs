// SPDX-License-Identifier: AGPL-3.0-or-later

//! RAR archives, for comic books (`.cbr`): the list of files and their
//! contents. The archive container (RAR 2.9–4 and RAR 5 headers) is read
//! here, from RARLAB's public technote and the libarchive reader's format
//! notes; the compressed data is decoded by compcol's clean-room RAR 2, 3 and
//! 5 decoders. No RARLAB unRAR code is involved (its licence doesn't fit the
//! AGPL), and nothing here can write a RAR archive.
//!
//! Read only what a comic reader needs: whole files, in archive order, with
//! each file's CRC-32 checked. Encrypted and multi-volume archives are refused
//! with a plain message.

use compcol::Status;

/// Where an archive may get to before it's refused (a decompression bomb, or
/// a comic too big to hold in memory).
pub(crate) struct Limits {
    /// Bytes of all wanted files together, unpacked.
    pub total: u64,
    /// Files in the archive.
    pub files: usize,
}

impl Limits {
    pub(crate) const COMIC: Limits = Limits {
        total: 3 << 30,
        files: 100_000,
    };
}

/// One file taken out of the archive.
#[derive(Debug)]
pub(crate) struct Extracted {
    pub name: String,
    pub data: Vec<u8>,
}

/// One file as the headers describe it.
#[derive(Debug, Clone)]
struct Member {
    name: String,
    /// Where its packed data is in the archive.
    data: std::ops::Range<usize>,
    unpacked: u64,
    crc: Option<u32>,
    codec: Codec,
    /// Continues the compression history of the file before it.
    solid: bool,
    /// RAR 5 dictionary size in bytes.
    window: usize,
    dir: bool,
    /// Why it can't be read, when it can't (encrypted, split across volumes).
    refused: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Codec {
    Store,
    Rar2,
    Rar3,
    Rar5,
    Unknown,
}

const SIG4: &[u8] = b"Rar!\x1a\x07\x00";
const SIG5: &[u8] = b"Rar!\x1a\x07\x01\x00";

/// Whether `bytes` start like a RAR archive.
pub(crate) fn is_rar(bytes: &[u8]) -> bool {
    bytes.starts_with(SIG4) || bytes.starts_with(SIG5)
}

/// Every file's name (not folders or links), in archive order, without
/// unpacking anything.
pub(crate) fn names(bytes: &[u8], limits: &Limits) -> Result<Vec<String>, String> {
    let members = if bytes.starts_with(SIG5) {
        list5(bytes, limits)?
    } else if bytes.starts_with(SIG4) {
        list4(bytes, limits)?
    } else {
        return Err("not a RAR archive".into());
    };
    Ok(members
        .into_iter()
        .filter(|m| !m.dir)
        .map(|m| m.name)
        .collect())
}

/// The files `want` picks (by name), in archive order, unpacked and checked.
/// Files before a wanted one in a solid group are decoded too (their history
/// is needed) but not kept.
pub(crate) fn extract(
    bytes: &[u8],
    want: impl Fn(&str) -> bool,
    limits: &Limits,
) -> Result<Vec<Extracted>, String> {
    let members = if bytes.starts_with(SIG5) {
        list5(bytes, limits)?
    } else if bytes.starts_with(SIG4) {
        list4(bytes, limits)?
    } else {
        return Err("not a RAR archive".into());
    };
    let wanted: Vec<bool> = members.iter().map(|m| !m.dir && want(&m.name)).collect();
    if let Some(m) = members
        .iter()
        .zip(&wanted)
        .find(|(m, w)| **w && m.refused.is_some())
    {
        return Err(m.0.refused.unwrap_or("this archive can't be read").into());
    }
    let total = members
        .iter()
        .zip(&wanted)
        .filter(|(_, w)| **w)
        .fold(0u64, |t, (m, _)| t.saturating_add(m.unpacked));
    if total > limits.total {
        return Err("the archive is too large to open".into());
    }

    let compressed = |m: &Member| !m.dir && m.codec != Codec::Store;
    let mut out = Vec::new();
    for (i, m) in members.iter().enumerate() {
        if !compressed(m) {
            if wanted[i] {
                let data = slice(bytes, &m.data)?.to_vec();
                check(m, &data)?;
                out.push(Extracted {
                    name: m.name.clone(),
                    data,
                });
            }
            continue;
        }
        if m.solid && members[..i].iter().any(compressed) {
            // Decoded with the chain it continues.
            continue;
        }
        // A compression chain: this file and the solid compressed files
        // after it (stored files in between don't break it).
        let chain: Vec<usize> = std::iter::once(i)
            .chain(
                (i + 1..members.len())
                    .filter(|&k| compressed(&members[k]))
                    .take_while(|&k| members[k].solid),
            )
            .collect();
        // Decode only as far as the last wanted file of the chain.
        let Some(last) = chain.iter().rposition(|&k| wanted[k]) else {
            continue;
        };
        let group: Vec<Member> = chain[..=last].iter().map(|&k| members[k].clone()).collect();
        if let Some(r) = group.iter().find_map(|m| m.refused) {
            return Err(r.into());
        }
        let decoded = decode_group(bytes, &group)?;
        for (&k, data) in chain.iter().zip(decoded) {
            if wanted[k] {
                check(&members[k], &data)?;
                out.push(Extracted {
                    name: members[k].name.clone(),
                    data,
                });
            }
        }
    }
    Ok(out)
}

fn slice<'a>(bytes: &'a [u8], r: &std::ops::Range<usize>) -> Result<&'a [u8], String> {
    bytes
        .get(r.clone())
        .ok_or_else(|| "the archive is cut short".to_string())
}

fn check(m: &Member, data: &[u8]) -> Result<(), String> {
    if data.len() as u64 != m.unpacked {
        return Err(format!("{} is damaged (wrong size)", m.name));
    }
    if let Some(want) = m.crc {
        let mut c = compcol::checksum::Crc32::new();
        c.update(data);
        if c.finalize() != want {
            return Err(format!("{} is damaged (checksum)", m.name));
        }
    }
    Ok(())
}

/// Decode a solid group (or one file): every member's data, in order.
fn decode_group(bytes: &[u8], group: &[Member]) -> Result<Vec<Vec<u8>>, String> {
    let codec = group[0].codec;
    if group.iter().any(|m| m.codec != codec) {
        return Err("this archive mixes compression versions".into());
    }
    // No real archive packs more than ~1000:1 (and pictures barely pack at
    // all): a header that claims more is damaged or a decompression bomb.
    let unpacked = group.iter().fold(0u64, |t, m| t.saturating_add(m.unpacked));
    let packed = group
        .iter()
        .fold(0u64, |t, m| t.saturating_add(m.data.len() as u64));
    if unpacked > packed.saturating_mul(1000).saturating_add(1 << 20) {
        return Err("the archive is damaged (sizes don't add up)".into());
    }
    match codec {
        Codec::Rar5 => {
            // One continuous stream: the members' runs back to back.
            let total: u64 = group.iter().map(|m| m.unpacked).sum();
            let mut packed = Vec::new();
            for m in group {
                packed.extend_from_slice(slice(bytes, &m.data)?);
            }
            let window = group.iter().map(|m| m.window).max().unwrap_or(0);
            let mut dec = compcol::rar5::Decoder::with_unpack_size_and_window(total, window);
            let mut at = 0;
            for m in group {
                dec.add_file_boundary(at);
                at += m.unpacked;
            }
            let all = drive(&mut dec, &packed, total)?;
            Ok(split(all, group))
        }
        Codec::Rar3 => {
            let mut dec = compcol::rar3::Decoder::with_unpack_size(group[0].unpacked);
            if group.len() > 1 {
                dec = dec.with_solid();
            }
            let mut out = Vec::with_capacity(group.len());
            for (k, m) in group.iter().enumerate() {
                if k > 0 {
                    dec.begin_solid_member(m.unpacked).map_err(rar_err)?;
                }
                out.push(drive(&mut dec, slice(bytes, &m.data)?, m.unpacked)?);
            }
            Ok(out)
        }
        Codec::Rar2 => {
            if group.len() > 1 {
                return Err("solid RAR 2 archives aren't supported".into());
            }
            let m = &group[0];
            let mut dec = compcol::rar2::Decoder::with_unpack_size(m.unpacked);
            Ok(vec![drive(&mut dec, slice(bytes, &m.data)?, m.unpacked)?])
        }
        Codec::Store => Ok(group
            .iter()
            .map(|m| slice(bytes, &m.data).map(<[u8]>::to_vec))
            .collect::<Result<_, _>>()?),
        Codec::Unknown => Err("this archive uses a compression Kitab can't read".into()),
    }
}

fn split(mut all: Vec<u8>, group: &[Member]) -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(group.len());
    // From the back, so each split_off is one copy of that member only.
    for m in group.iter().rev() {
        let at = all.len().saturating_sub(m.unpacked as usize);
        out.push(all.split_off(at));
    }
    out.reverse();
    out
}

fn rar_err(e: compcol::Error) -> String {
    format!("the archive is damaged ({e})")
}

/// Run a decoder over `input` until it has produced `unpacked` bytes or
/// stops making progress.
fn drive(dec: &mut impl compcol::Decoder, input: &[u8], unpacked: u64) -> Result<Vec<u8>, String> {
    let len = usize::try_from(unpacked).map_err(|_| "a file is too large".to_string())?;
    // Grown as output arrives, so a header's claimed size alone allocates
    // nothing.
    const CHUNK: usize = 8 << 20;
    let mut out = Vec::new();
    let mut total = 0usize;
    let mut fed = 0usize;
    let mut stalls = 0;
    while total < len {
        if out.len() == total {
            out.resize(total + (len - total).min(CHUNK), 0);
        }
        let (p, status) = dec
            .decode(&input[fed..], &mut out[total..])
            .map_err(rar_err)?;
        fed += p.consumed;
        total += p.written;
        if matches!(status, Status::StreamEnd) {
            break;
        }
        if p.consumed == 0 && p.written == 0 {
            stalls += 1;
            if stalls > 2 {
                break;
            }
        } else {
            stalls = 0;
        }
    }
    while total < len {
        if out.len() == total {
            out.resize(total + (len - total).min(CHUNK), 0);
        }
        let (p, status) = dec.finish(&mut out[total..]).map_err(rar_err)?;
        total += p.written;
        if matches!(status, Status::StreamEnd) || p.written == 0 {
            break;
        }
    }
    out.truncate(total);
    Ok(out)
}

// ─── RAR 5 ──────────────────────────────────────────────────────────────

struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn u8(&mut self) -> Result<u8, String> {
        let v = *self.b.get(self.at).ok_or(CUT)?;
        self.at += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().map_err(|_| CUT)?,
        ))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| CUT)?,
        ))
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or(CUT)?;
        let s = self.b.get(self.at..end).ok_or(CUT)?;
        self.at = end;
        Ok(s)
    }
    /// RAR 5's variable-length integer: 7 bits a byte, low bits first.
    fn vint(&mut self) -> Result<u64, String> {
        let mut v = 0u64;
        for shift in (0..70).step_by(7) {
            let b = self.u8()?;
            if shift < 64 {
                v |= u64::from(b & 0x7f) << shift;
            }
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err("the archive is damaged (a number is too long)".into())
    }
    fn size(&mut self) -> Result<usize, String> {
        usize::try_from(self.vint()?).map_err(|_| CUT.to_string())
    }
}

const CUT: &str = "the archive is cut short";

fn crc32(data: &[u8]) -> u32 {
    let mut c = compcol::checksum::Crc32::new();
    c.update(data);
    c.finalize()
}

fn list5(bytes: &[u8], limits: &Limits) -> Result<Vec<Member>, String> {
    let mut members = Vec::new();
    let mut at = SIG5.len();
    while at < bytes.len() {
        let mut c = Cursor { b: bytes, at };
        let crc = c.u32()?;
        let size_at = c.at;
        let head_size = c.size()?;
        let body = c.at;
        let end = body.checked_add(head_size).ok_or(CUT)?;
        if head_size == 0 || end > bytes.len() {
            return Err(CUT.into());
        }
        if crc32(&bytes[size_at..end]) != crc {
            return Err("the archive is damaged (a header's checksum)".into());
        }
        let kind = c.vint()?;
        let flags = c.vint()?;
        let extra = if flags & 0x01 != 0 { c.size()? } else { 0 };
        let data = if flags & 0x02 != 0 { c.size()? } else { 0 };
        let data_end = end.checked_add(data).ok_or(CUT)?;
        match kind {
            // Main archive header.
            1 => {
                let archive_flags = c.vint()?;
                if archive_flags & 0x01 != 0 {
                    // A volume of a multi-part archive: the parts aren't here.
                    return Err("archives split into parts aren't supported".into());
                }
            }
            // File header.
            2 => {
                let file_flags = c.vint()?;
                let unpacked = c.vint()?;
                let _attributes = c.vint()?;
                if file_flags & 0x02 != 0 {
                    c.u32()?;
                }
                let crc = if file_flags & 0x04 != 0 {
                    Some(c.u32()?)
                } else {
                    None
                };
                let info = c.vint()?;
                let _host = c.vint()?;
                let name_len = c.size()?;
                let name = String::from_utf8_lossy(c.take(name_len)?).replace('\\', "/");
                let extra_start = end.checked_sub(extra).ok_or(CUT)?;
                let extras = extra_records(bytes.get(extra_start..end).ok_or(CUT)?)?;
                let version = info & 0x3f;
                let method = (info >> 7) & 0x07;
                let dict_bits = (info >> 10) & 0x1f;
                let split = flags & (0x08 | 0x10) != 0;
                let window = (0x20000usize)
                    .checked_shl(u32::try_from(dict_bits).unwrap_or(31))
                    .unwrap_or(usize::MAX)
                    .next_power_of_two();
                members.push(Member {
                    name,
                    data: end..data_end,
                    unpacked: if file_flags & 0x08 != 0 {
                        u64::MAX
                    } else {
                        unpacked
                    },
                    crc,
                    codec: match (version, method) {
                        (_, 0) => Codec::Store,
                        (0 | 1, _) => Codec::Rar5,
                        _ => Codec::Unknown,
                    },
                    solid: info & 0x40 != 0,
                    window,
                    // A link (hard or symbolic) has no data of its own: not a page.
                    dir: file_flags & 0x01 != 0 || extras.link,
                    refused: if extras.encrypted {
                        Some("password-protected archives aren't supported")
                    } else if split {
                        Some("archives split into parts aren't supported")
                    } else if file_flags & 0x08 != 0 {
                        Some("this archive can't be read (a file of unknown size)")
                    } else if window > 1 << 30 {
                        Some("this archive's dictionary is too large to open")
                    } else {
                        None
                    },
                });
                if members.len() > limits.files {
                    return Err("the archive has too many files".into());
                }
            }
            // Archive encryption header: every later header is encrypted.
            4 => return Err("password-protected archives aren't supported".into()),
            // End of archive.
            5 => break,
            // Service headers (comments, recovery records) and the rest.
            _ => {}
        }
        if data_end > bytes.len() {
            return Err(CUT.into());
        }
        at = data_end;
    }
    Ok(members)
}

#[derive(Default)]
struct Extras {
    encrypted: bool,
    link: bool,
}

/// What a RAR 5 file's extra area says: encrypted (record 1), or a link to
/// another file (record 5, "file system redirection").
fn extra_records(area: &[u8]) -> Result<Extras, String> {
    let mut found = Extras::default();
    let mut c = Cursor { b: area, at: 0 };
    while c.at < area.len() {
        let size = c.size()?;
        let start = c.at;
        match c.vint()? {
            0x01 => found.encrypted = true,
            0x05 => found.link = true,
            _ => {}
        }
        c.at = start.checked_add(size).ok_or(CUT)?;
    }
    Ok(found)
}

// ─── RAR 2.9 – 4 ────────────────────────────────────────────────────────

fn list4(bytes: &[u8], limits: &Limits) -> Result<Vec<Member>, String> {
    let mut members = Vec::new();
    let mut at = SIG4.len();
    while at + 7 <= bytes.len() {
        let mut c = Cursor { b: bytes, at };
        let crc = c.u16()?;
        let kind = c.u8()?;
        let flags = c.u16()?;
        let head_size = usize::from(c.u16()?);
        let head_end = at.checked_add(head_size).ok_or(CUT)?;
        if head_size < 7 || head_end > bytes.len() {
            return Err(CUT.into());
        }
        // Packed data after the header: PACK_SIZE for files, ADD_SIZE for
        // other long blocks.
        let mut add: u64 = 0;
        match kind {
            // Main header.
            0x73 => {
                check4(bytes, at, head_end, crc)?;
                if flags & 0x0080 != 0 {
                    return Err("password-protected archives aren't supported".into());
                }
                if flags & 0x0001 != 0 {
                    return Err("archives split into parts aren't supported".into());
                }
            }
            // File header.
            0x74 => {
                check4(bytes, at, head_end, crc)?;
                let pack = u64::from(c.u32()?);
                let unp = u64::from(c.u32()?);
                let _host = c.u8()?;
                let file_crc = c.u32()?;
                let _time = c.u32()?;
                let version = c.u8()?;
                let method = c.u8()?;
                let name_size = usize::from(c.u16()?);
                let _attr = c.u32()?;
                let (pack, unp) = if flags & 0x0100 != 0 {
                    let hp = u64::from(c.u32()?);
                    let hu = u64::from(c.u32()?);
                    (pack | (hp << 32), unp | (hu << 32))
                } else {
                    (pack, unp)
                };
                let raw = c.take(name_size)?;
                let name = if flags & 0x0200 != 0 {
                    unicode_name(raw)
                } else {
                    // Older archives store names in the OEM/ANSI code page;
                    // ASCII, the common case, reads the same either way.
                    String::from_utf8_lossy(raw).into_owned()
                }
                .replace('\\', "/");
                add = pack;
                let start = head_end;
                let end = usize::try_from(pack)
                    .ok()
                    .and_then(|p| start.checked_add(p))
                    .ok_or(CUT)?;
                members.push(Member {
                    name,
                    data: start..end,
                    unpacked: unp,
                    crc: Some(file_crc),
                    codec: match (method, version) {
                        (0x30, _) => Codec::Store,
                        (_, 29 | 36) => Codec::Rar3,
                        (_, 20 | 26) => Codec::Rar2,
                        _ => Codec::Unknown,
                    },
                    solid: flags & 0x0010 != 0,
                    window: 0,
                    dir: flags & 0x00e0 == 0x00e0,
                    refused: if flags & 0x0004 != 0 {
                        Some("password-protected archives aren't supported")
                    } else if flags & 0x0003 != 0 {
                        Some("archives split into parts aren't supported")
                    } else {
                        None
                    },
                });
                if members.len() > limits.files {
                    return Err("the archive has too many files".into());
                }
            }
            // End of archive.
            0x7b => break,
            // Comments, sub-blocks, recovery records: skipped.
            _ => {
                if flags & 0x8000 != 0 {
                    add = u64::from(c.u32()?);
                }
            }
        }
        let next = usize::try_from(add)
            .ok()
            .and_then(|a| head_end.checked_add(a))
            .ok_or(CUT)?;
        if next > bytes.len() {
            return Err(CUT.into());
        }
        at = next;
    }
    Ok(members)
}

/// A RAR 4 header's CRC: the low 16 bits of the CRC-32 of the header after
/// the CRC field.
fn check4(bytes: &[u8], at: usize, end: usize, crc: u16) -> Result<(), String> {
    let got = crc32(bytes.get(at + 2..end).ok_or(CUT)?) & 0xffff;
    if got != u32::from(crc) {
        return Err("the archive is damaged (a header's checksum)".into());
    }
    Ok(())
}

/// A RAR 4 Unicode file name: the ASCII form, a zero byte, then the Unicode
/// form packed against it. Falls back to the ASCII form when the packed part
/// is missing or doesn't decode.
fn unicode_name(raw: &[u8]) -> String {
    let Some(zero) = raw.iter().position(|&b| b == 0) else {
        return String::from_utf8_lossy(raw).into_owned();
    };
    let (ascii, enc) = (&raw[..zero], &raw[zero + 1..]);
    decode_packed_name(ascii, enc).unwrap_or_else(|| String::from_utf8_lossy(ascii).into_owned())
}

fn decode_packed_name(ascii: &[u8], enc: &[u8]) -> Option<String> {
    let mut out: Vec<u16> = Vec::new();
    let high = u16::from(*enc.first()?);
    let mut pos = 1;
    let mut flags = 0u8;
    let mut bits = 0;
    while pos < enc.len() {
        if bits == 0 {
            flags = enc[pos];
            pos += 1;
            bits = 8;
        }
        bits -= 2;
        match (flags >> bits) & 3 {
            0 => {
                out.push(u16::from(*enc.get(pos)?));
                pos += 1;
            }
            1 => {
                out.push(u16::from(*enc.get(pos)?) | (high << 8));
                pos += 1;
            }
            2 => {
                out.push(u16::from(*enc.get(pos)?) | (u16::from(*enc.get(pos + 1)?) << 8));
                pos += 2;
            }
            _ => {
                let len = *enc.get(pos)?;
                pos += 1;
                if len & 0x80 != 0 {
                    let correction = *enc.get(pos)?;
                    pos += 1;
                    for _ in 0..(usize::from(len & 0x7f) + 2) {
                        let a = *ascii.get(out.len())?;
                        out.push(u16::from(a.wrapping_add(correction)) | (high << 8));
                    }
                } else {
                    for _ in 0..(usize::from(len) + 2) {
                        out.push(u16::from(*ascii.get(out.len())?));
                    }
                }
            }
        }
        if out.len() > 4096 {
            return None;
        }
    }
    String::from_utf16(&out).ok()
}

/// Every file's name and size, for tests that read archives from outside.
#[doc(hidden)]
pub fn extract_for_tests(bytes: &[u8]) -> Result<Vec<(String, usize)>, String> {
    extract(bytes, |_| true, &Limits::COMIC)
        .map(|v| v.into_iter().map(|e| (e.name, e.data.len())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_integers() {
        let mut c = Cursor {
            b: &[0x7f, 0x80, 0x01, 0xff, 0xff, 0x03],
            at: 0,
        };
        assert_eq!(c.vint(), Ok(0x7f));
        assert_eq!(c.vint(), Ok(0x80));
        assert_eq!(c.vint(), Ok(0xffff));
        assert!(c.vint().is_err());
    }

    #[test]
    fn not_rar_is_an_error() {
        assert!(!is_rar(b"PK\x03\x04"));
        assert!(extract(b"PK\x03\x04", |_| true, &Limits::COMIC).is_err());
        assert!(extract(b"Rar!\x1a\x07\x01\x00", |_| true, &Limits::COMIC).is_ok());
        assert!(extract(b"Rar!\x1a\x07\x01\x00\xff\xff", |_| true, &Limits::COMIC).is_err());
    }

    #[test]
    fn packed_unicode_name() {
        // "ab" stored as two plain bytes (flag pair 00 00).
        assert_eq!(
            decode_packed_name(b"ab", &[0, 0x00, b'a', b'b']),
            Some("ab".into())
        );
        // A run copied from the ASCII form (flag 11, length 0 = 2 chars).
        assert_eq!(
            decode_packed_name(b"xy", &[0, 0xc0, 0x00]),
            Some("xy".into())
        );
    }
}
