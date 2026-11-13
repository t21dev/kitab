// SPDX-License-Identifier: AGPL-3.0-or-later

//! Signature fields in a PDF (PLAN.md §3.7): each one's byte range and CMS
//! blob, how much of the file it covers, and what the revisions after it
//! changed. The crypto is kitab-sign's; this only reads the file.
//!
//! What changed is found by opening the signed revision (the file up to the
//! end of the byte range) beside the whole file and comparing every object:
//! new or changed signatures, widgets and form fields are form filling, new
//! or changed annotations are annotations, and anything else (a page's
//! content, a changed stream, other objects) is "other".

use kitab_core::{SigCoverage, SigModification, SignedField};
use mupdf::pdf::{PdfDocument, PdfObject};
use std::path::Path;

/// Every signed signature field in the PDF at `path`, in field order.
pub fn signed_fields(path: &Path, password: Option<&str>) -> Result<Vec<SignedField>, String> {
    // Most PDFs have no signature: look for one with MuPDF reading the file
    // as it needs to, and only read it all into memory when there is one.
    if !has_signature(path, password) {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("could not read the file: {e}"))?;
    let doc = open(&bytes, password)?;
    let mut found = Vec::new();
    let fields = doc
        .catalog()
        .ok()
        .and_then(|c| c.get_dict("AcroForm").ok().flatten())
        .and_then(|f| f.get_dict("Fields").ok().flatten());
    if let Some(fields) = fields {
        walk(&fields, "", &mut found, 0);
    }
    let mut out = Vec::with_capacity(found.len());
    for (field, v, widget) in found {
        out.push(read_field(
            &doc,
            &bytes,
            password,
            field,
            &v,
            widget.as_ref(),
        ));
    }
    Ok(out)
}

/// Whether the PDF has at least one signed signature field (a cheap look:
/// the file is opened in place, not read whole).
fn has_signature(path: &Path, password: Option<&str>) -> bool {
    let Some(name) = path.to_str() else {
        return true; // can't tell cheaply: take the full path
    };
    let Ok(mut doc) = PdfDocument::open(name) else {
        return false;
    };
    if doc.needs_password().unwrap_or(false)
        && !password.is_some_and(|p| doc.authenticate(p).unwrap_or(false))
    {
        return false;
    }
    let fields = doc
        .catalog()
        .ok()
        .and_then(|c| c.get_dict("AcroForm").ok().flatten())
        .and_then(|f| f.get_dict("Fields").ok().flatten());
    let mut found = Vec::new();
    if let Some(fields) = fields {
        walk(&fields, "", &mut found, 0);
    }
    !found.is_empty()
}

fn open(bytes: &[u8], password: Option<&str>) -> Result<PdfDocument, String> {
    let mut doc = PdfDocument::from_bytes(bytes).map_err(|e| format!("could not open: {e}"))?;
    if doc.needs_password().unwrap_or(false) {
        let ok = password.is_some_and(|p| doc.authenticate(p).unwrap_or(false));
        if !ok {
            return Err("this PDF needs its password".into());
        }
    }
    Ok(doc)
}

/// Fields with `/FT /Sig` and a signature value: (name, /V, the widget).
type Found = (String, PdfObject, Option<PdfObject>);

fn walk(list: &PdfObject, prefix: &str, out: &mut Vec<Found>, depth: u32) {
    if depth > 16 {
        return;
    }
    let n = list.len().unwrap_or(0);
    for i in 0..n {
        let Some(field) = list.get_array(i as i32).ok().flatten() else {
            continue;
        };
        let part = field
            .get_dict("T")
            .ok()
            .flatten()
            .and_then(|t| t.as_string().ok())
            .unwrap_or_default();
        let name = match (prefix.is_empty(), part.is_empty()) {
            (_, true) => prefix.to_string(),
            (true, false) => part,
            (false, false) => format!("{prefix}.{part}"),
        };
        let kids = field.get_dict("Kids").ok().flatten();
        let is_sig = field
            .get_dict_inheritable("FT")
            .ok()
            .flatten()
            .and_then(|t| t.as_name().ok())
            .is_some_and(|t| t == b"Sig");
        if is_sig {
            let value = field.get_dict_inheritable("V").ok().flatten();
            if let Some(v) = value.filter(|v| v.is_dict().unwrap_or(false)) {
                // The widget is the field itself, or its first kid.
                let widget = if field.get_dict("Rect").ok().flatten().is_some() {
                    Some(field)
                } else {
                    kids.as_ref().and_then(|k| k.get_array(0).ok().flatten())
                };
                out.push((name, v, widget));
                continue;
            }
        }
        if let Some(kids) = kids {
            // Kids that are fields carry /T; widgets of this field don't.
            walk(&kids, &name, out, depth + 1);
        }
    }
}

fn text(d: &PdfObject, key: &str) -> Option<String> {
    let s = d.get_dict(key).ok().flatten()?;
    let s = if s.is_name().unwrap_or(false) {
        String::from_utf8_lossy(&s.as_name().ok()?).to_string()
    } else {
        s.as_string().ok()?
    };
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn read_field(
    doc: &PdfDocument,
    bytes: &[u8],
    password: Option<&str>,
    field: String,
    v: &PdfObject,
    widget: Option<&PdfObject>,
) -> SignedField {
    let mut nums = Vec::new();
    if let Some(br) = v.get_dict("ByteRange").ok().flatten() {
        for i in 0..br.len().unwrap_or(0) {
            let n = br
                .get_array(i as i32)
                .ok()
                .flatten()
                .and_then(|n| n.as_int().ok())
                .unwrap_or(-1);
            nums.push(n);
        }
    }
    let byte_range: Vec<(u64, u64)> = if nums.len() % 2 == 0 && nums.iter().all(|n| *n >= 0) {
        nums.chunks(2).map(|c| (c[0] as u64, c[1] as u64)).collect()
    } else {
        Vec::new()
    };
    let contents = v
        .get_dict("Contents")
        .ok()
        .flatten()
        .and_then(|c| c.as_bytes().ok())
        .unwrap_or_default();
    let page = widget
        .and_then(|w| w.get_dict("P").ok().flatten())
        .and_then(|p| doc.lookup_page_number(&p).ok())
        .and_then(|n| u32::try_from(n).ok());
    let len = bytes.len() as u64;
    let end = byte_range.last().map_or(0, |(o, l)| o + l);
    let coverage = coverage(&byte_range, len);
    let modification = if byte_range.is_empty() || end >= len || end == 0 {
        SigModification::None
    } else {
        changes_since(bytes, end as usize, password)
    };
    SignedField {
        field,
        page,
        byte_range,
        contents,
        sub_filter: text(v, "SubFilter").unwrap_or_default(),
        name: text(v, "Name"),
        reason: text(v, "Reason"),
        location: text(v, "Location"),
        when: text(v, "M"),
        coverage,
        modification,
    }
}

/// Coverage from the byte range: two ranges from the start, the gap between
/// them holding only the signature, ending at the end of the file or of an
/// earlier revision.
pub fn coverage(ranges: &[(u64, u64)], file_len: u64) -> SigCoverage {
    match ranges {
        [(0, a), (b, c)] if a <= b && b + c <= file_len => {
            if b + c == file_len {
                SigCoverage::EntireFile
            } else {
                SigCoverage::EntireRevision
            }
        }
        _ => SigCoverage::Partial,
    }
}

/// Classify every object that differs between the signed revision (the
/// first `end` bytes) and the whole file.
fn changes_since(bytes: &[u8], end: usize, password: Option<&str>) -> SigModification {
    let (Ok(full), Ok(old)) = (open(bytes, password), open(&bytes[..end], password)) else {
        return SigModification::Other;
    };
    let info = full
        .trailer()
        .ok()
        .and_then(|t| t.get_dict("Info").ok().flatten())
        .and_then(|i| i.as_indirect().ok());
    let old_len = old.xref_len().unwrap_or(0);
    let mut worst = SigModification::None;
    for num in 1..full.xref_len().unwrap_or(0) as i32 {
        let Some(now) = full.xref_object(num).ok().flatten() else {
            continue;
        };
        if now.is_null().unwrap_or(true) {
            continue;
        }
        let before = if (num as usize) < old_len {
            old.xref_object(num)
                .ok()
                .flatten()
                .filter(|o| !o.is_null().unwrap_or(true))
        } else {
            None
        };
        if let Some(before) = before.as_ref() {
            if same(&full, &old, num, &now, before) {
                continue;
            }
        }
        if Some(num) == info {
            continue; // the document info's dates and producer
        }
        let level = classify(&now, before.as_ref());
        worst = worst.max(level);
        if worst == SigModification::Other {
            break;
        }
    }
    worst
}

fn same(full: &PdfDocument, old: &PdfDocument, num: i32, a: &PdfObject, b: &PdfObject) -> bool {
    if a.to_string() != b.to_string() {
        return false;
    }
    if a.is_stream().unwrap_or(false) {
        return full.xref_raw_stream(num).ok() == old.xref_raw_stream(num).ok();
    }
    true
}

fn name(d: &PdfObject, key: &str) -> Option<Vec<u8>> {
    d.get_dict(key)
        .ok()
        .flatten()
        .and_then(|n| n.as_name().ok())
}

/// A dict printed without some of its keys, to compare the rest.
fn without(d: &PdfObject, skip: &[&[u8]]) -> String {
    let mut parts = Vec::new();
    for i in 0..d.dict_len().unwrap_or(0) as i32 {
        let key = d
            .get_dict_key(i)
            .ok()
            .flatten()
            .and_then(|k| k.as_name().ok())
            .unwrap_or_default();
        if skip.contains(&key.as_slice()) {
            continue;
        }
        let val = d
            .get_dict_val(i)
            .ok()
            .flatten()
            .map(|v| v.to_string())
            .unwrap_or_default();
        parts.push(format!("{}={val}", String::from_utf8_lossy(&key)));
    }
    parts.sort();
    parts.join(" ")
}

const ANNOTS: &[&[u8]] = &[
    b"Text",
    b"Link",
    b"FreeText",
    b"Line",
    b"Square",
    b"Circle",
    b"Polygon",
    b"PolyLine",
    b"Highlight",
    b"Underline",
    b"Squiggly",
    b"StrikeOut",
    b"Stamp",
    b"Caret",
    b"Ink",
    b"Popup",
    b"FileAttachment",
    b"Sound",
    b"Redact",
];

fn classify(now: &PdfObject, before: Option<&PdfObject>) -> SigModification {
    use SigModification::*;
    if !now.is_dict().unwrap_or(false) {
        // Arrays (/Annots, /Fields, /Kids) only point at objects that are
        // classified on their own; other changed values are content.
        return if now.is_array().unwrap_or(false) {
            None
        } else {
            Other
        };
    }
    let ty = name(now, "Type");
    let sub = name(now, "Subtype");
    match ty.as_deref() {
        Some(b"XRef" | b"ObjStm" | b"Metadata") => return None,
        Some(b"Sig" | b"DocTimeStamp" | b"DSS" | b"VRI") => return FormFilling,
        Some(b"Catalog") => {
            let skip: &[&[u8]] = &[b"AcroForm", b"DSS", b"Perms", b"Metadata"];
            return match before {
                Some(b) if without(now, skip) == without(b, skip) => FormFilling,
                _ => Other,
            };
        }
        Some(b"Page") => {
            let skip: &[&[u8]] = &[b"Annots"];
            return match before {
                Some(b) if without(now, skip) == without(b, skip) => None,
                _ => Other,
            };
        }
        _ => {}
    }
    if sub.as_deref() == Some(b"Widget") || now.get_dict("FT").ok().flatten().is_some() {
        return FormFilling;
    }
    if now.get_dict("Fields").ok().flatten().is_some() {
        return FormFilling; // the AcroForm dictionary
    }
    if sub.as_deref().is_some_and(|s| ANNOTS.contains(&s)) || ty.as_deref() == Some(b"Annot") {
        return Annotations;
    }
    // New appearance streams, fonts and their files come with signatures
    // and annotations; a changed existing one is a content change.
    let resource = matches!(
        ty.as_deref(),
        Some(b"XObject" | b"Font" | b"FontDescriptor")
    ) || (ty.is_none() && now.is_stream().unwrap_or(false));
    if before.is_none() && resource {
        return None;
    }
    Other
}

/// Room for the CMS blob, in bytes: enough for a chain of a few RSA-4096
/// certificates.
pub const SIG_ROOM: usize = 16_384;

/// The byte-range placeholder kitab-sign looks for and rewrites in place.
pub const RANGE_MARK: &str = "1111111111 1222222222 1333333333";

/// What a new signature looks like and says.
#[derive(Debug, Clone, Default)]
pub struct NewSignature {
    pub field: String,
    pub page: u32,
    /// Where on the page, in page points as Kitab shows it (from the
    /// top-left, the page's /Rotate applied): x0, y0, x1, y1.
    pub rect: [f32; 4],
    /// The lines drawn in the box ("Digitally signed by …", the date…).
    pub lines: Vec<String>,
    pub name: Option<String>,
    pub reason: Option<String>,
    pub location: Option<String>,
    /// `/M`, a PDF date ("D:20261107093000Z").
    pub when: String,
}

/// A PDF string literal: parentheses and backslashes escaped, anything
/// outside printable ASCII replaced, so it is safe in content streams too.
fn literal(s: &str) -> String {
    let mut out = String::from("(");
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            ' '..='~' => out.push(c),
            _ => out.push('?'),
        }
    }
    out.push(')');
    out
}

/// The file at `src` with a new signature field appended as an incremental
/// update, its `/Contents` all zeros and `/ByteRange` a placeholder: what
/// kitab-sign fills in. The earlier bytes are untouched, so signatures
/// already in the file stay valid.
pub fn prepare_signature(src: &Path, new: &NewSignature) -> Result<Vec<u8>, String> {
    let original = std::fs::read(src).map_err(|e| format!("could not read the file: {e}"))?;
    let name = src.to_str().ok_or("the file name is not valid Unicode")?;
    let mut doc = PdfDocument::open(name).map_err(|e| format!("could not open: {e}"))?;
    if doc.needs_password().unwrap_or(false) {
        return Err("a PDF with a password can't be signed yet; remove the password first".into());
    }
    if !doc.can_be_saved_incrementally() {
        return Err(
            "this PDF had to be repaired to open, so it can't be signed without breaking it".into(),
        );
    }
    let err = |e: mupdf::Error| e.to_string();
    let page = doc
        .find_page(i32::try_from(new.page).map_err(|e| e.to_string())?)
        .map_err(err)?;
    let page_num = page
        .as_indirect()
        .map_err(|_| "the page isn't an object of its own".to_string())?;
    // The box in the page's own PDF space (bottom-left origin, unrotated).
    let to_pdf = doc
        .load_pdf_page(i32::try_from(new.page).map_err(|e| e.to_string())?)
        .and_then(|p| p.ctm())
        .map_err(err)?
        .invert()
        .ok_or("the page can't be mapped")?;
    let (ax, ay) = to_pdf.transform_xy(new.rect[0], new.rect[1]);
    let (bx, by) = to_pdf.transform_xy(new.rect[2], new.rect[3]);
    let (x0, y0, x1, y1) = (ax.min(bx), ay.min(by), ax.max(bx), ay.max(by));
    let (w, h) = ((x1 - x0).max(20.0), (y1 - y0).max(12.0));

    // The look: a light box, the lines in Helvetica.
    let size = (h / (new.lines.len().max(1) as f32 + 1.0)).clamp(5.0, 10.0);
    let mut ap = format!(
        "q 0.5 0.5 0.5 RG 0.75 w 0.5 0.5 {:.2} {:.2} re S Q\nBT /Helv {size:.2} Tf ",
        w - 1.0,
        h - 1.0
    );
    let y = h - size - 4.0;
    ap.push_str(&format!("6 {y:.2} Td "));
    for (i, line) in new.lines.iter().enumerate() {
        if i > 0 {
            ap.push_str(&format!("0 {:.2} Td ", -(size + 3.0)));
        }
        ap.push_str(&format!("{} Tj ", literal(line)));
    }
    ap.push_str("ET\n");
    let ap_dict = doc
        .new_object_from_str(&format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 {w:.2} {h:.2}] /Resources << /Font << /Helv << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> >> >> >>"
        ))
        .map_err(err)?;
    let mut buf = mupdf::Buffer::with_capacity(ap.len());
    std::io::Write::write_all(&mut buf, ap.as_bytes()).map_err(|e| e.to_string())?;
    let ap_ref = doc.add_stream(&buf, Some(&ap_dict), false).map_err(err)?;
    let ap_num = ap_ref.as_indirect().map_err(err)?;

    let mut sig = format!(
        "<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /M {} /ByteRange [0 {RANGE_MARK}] /Contents <{}>",
        literal(&new.when),
        "0".repeat(SIG_ROOM * 2)
    );
    for (key, value) in [
        ("Name", &new.name),
        ("Reason", &new.reason),
        ("Location", &new.location),
    ] {
        if let Some(v) = value {
            sig.push_str(&format!(" /{key} {}", literal(v)));
        }
    }
    sig.push_str(" >>");
    let sig_obj = doc.new_object_from_str(&sig).map_err(err)?;
    let sig_num = doc
        .add_object(&sig_obj)
        .map_err(err)?
        .as_indirect()
        .map_err(err)?;

    let widget = doc
        .new_object_from_str(&format!(
            "<< /Type /Annot /Subtype /Widget /FT /Sig /F 4 /T {} /Rect [{x0:.2} {y0:.2} {x1:.2} {y1:.2}] /P {page_num} 0 R /V {sig_num} 0 R /AP << /N {ap_num} 0 R >> >>",
            literal(&new.field)
        ))
        .map_err(err)?;
    let widget_ref = doc.add_object(&widget).map_err(err)?;

    // On the page, and in the form.
    let mut page = page;
    match page.get_dict("Annots").map_err(err)? {
        Some(mut annots) => annots
            .array_push(widget_ref.try_clone().map_err(err)?)
            .map_err(err)?,
        None => {
            let mut annots = doc.new_array().map_err(err)?;
            annots
                .array_push(widget_ref.try_clone().map_err(err)?)
                .map_err(err)?;
            page.dict_put("Annots", annots).map_err(err)?;
        }
    }
    let mut catalog = doc.catalog().map_err(err)?;
    let form = match catalog.get_dict("AcroForm").map_err(err)? {
        Some(f) => f,
        None => {
            let f = doc.new_object_from_str("<< /Fields [] >>").map_err(err)?;
            let f = doc.add_object(&f).map_err(err)?;
            catalog
                .dict_put("AcroForm", f.try_clone().map_err(err)?)
                .map_err(err)?;
            f
        }
    };
    let mut form = form;
    match form.get_dict("Fields").map_err(err)? {
        Some(mut fields) => fields.array_push(widget_ref).map_err(err)?,
        None => {
            let mut fields = doc.new_array().map_err(err)?;
            fields.array_push(widget_ref).map_err(err)?;
            form.dict_put("Fields", fields).map_err(err)?;
        }
    }
    form.dict_put("SigFlags", doc.new_int(3).map_err(err)?)
        .map_err(err)?;

    // Append the update after the original bytes, in a temporary file of
    // this call's own: the clock alone repeats (Windows ticks in 100 ns), so
    // two signings at once shared a file and one lost it mid-save.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tmp = std::env::temp_dir().join(format!(
        "kitab-sign-{}-{}-{}.pdf",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::write(&tmp, &original).map_err(|e| e.to_string())?;
    let mut options = mupdf::pdf::PdfWriteOptions::default();
    options.set_incremental(true);
    let tmp_name = tmp
        .to_str()
        .ok_or("the temporary folder's name is not valid Unicode")?;
    let saved = doc.save_with_options(tmp_name, options).map_err(err);
    drop(doc);
    let out = saved.and_then(|_| std::fs::read(&tmp).map_err(|e| e.to_string()));
    let _ = std::fs::remove_file(&tmp);
    let out = out?;
    if !out.starts_with(&original) {
        return Err("the update didn't keep the original bytes".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pdf")
            .join(name)
    }

    #[test]
    fn byte_ranges_give_coverage() {
        assert_eq!(coverage(&[(0, 10), (30, 70)], 100), SigCoverage::EntireFile);
        assert_eq!(
            coverage(&[(0, 10), (30, 50)], 100),
            SigCoverage::EntireRevision
        );
        assert_eq!(coverage(&[(5, 10), (30, 65)], 100), SigCoverage::Partial);
        assert_eq!(coverage(&[(0, 10)], 100), SigCoverage::Partial);
        assert_eq!(coverage(&[(0, 40), (30, 70)], 100), SigCoverage::Partial);
    }

    #[test]
    fn signature_fields_of_the_fixtures() {
        assert!(
            signed_fields(&fixture("08-lease-unsigned.pdf"), None)
                .unwrap()
                .is_empty()
        );
        let a = signed_fields(&fixture("08a-signed-valid.pdf"), None).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].field, "Landlord");
        assert_eq!(a[0].page, Some(0));
        assert_eq!(a[0].coverage, SigCoverage::EntireFile);
        assert_eq!(a[0].modification, SigModification::None);
        assert!(a[0].contents.len() > 500 && a[0].byte_range.len() == 2);

        let b = signed_fields(&fixture("08b-signed-two-signers.pdf"), None).unwrap();
        let names: Vec<&str> = b.iter().map(|f| f.field.as_str()).collect();
        assert_eq!(names, ["Landlord", "Tenant"]);
        assert_eq!(b[0].coverage, SigCoverage::EntireRevision);
        assert_eq!(b[0].modification, SigModification::FormFilling);
        assert_eq!(b[1].coverage, SigCoverage::EntireFile);

        let c = signed_fields(&fixture("08c-signed-then-modified.pdf"), None).unwrap();
        assert_eq!(c[0].coverage, SigCoverage::EntireRevision);
        assert!(
            c[0].modification > SigModification::FormFilling,
            "{:?}",
            c[0].modification
        );
    }
}
