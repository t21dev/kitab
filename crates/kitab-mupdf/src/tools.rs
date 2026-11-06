// SPDX-License-Identifier: AGPL-3.0-or-later

//! PDF tools that write new files (PLAN.md Phase 9): extract pages and merge.
//! Each opens its own copy of the source, so the open document a reader shows
//! is never changed. Files are written next to the target and renamed, so a
//! failure never leaves half a PDF behind.

use mupdf::pdf::{InsertPdfOptions, PageSelection, PdfDocument, PdfWriteOptions};
use std::path::{Path, PathBuf};

fn open(path: &Path, password: Option<&str>) -> Result<PdfDocument, String> {
    let name = path.to_str().ok_or("the file name is not valid Unicode")?;
    let mut doc = PdfDocument::open(name).map_err(|e| format!("could not open the PDF: {e}"))?;
    if doc.needs_password().map_err(|e| e.to_string())? {
        let ok = match password {
            Some(p) => doc.authenticate(p).map_err(|e| e.to_string())?,
            None => false,
        };
        if !ok {
            return Err("this PDF needs its password".into());
        }
    }
    Ok(doc)
}

fn save(doc: &PdfDocument, out: &Path) -> Result<(), String> {
    let mut options = PdfWriteOptions::default();
    // Drop objects only the removed pages used, and keep streams compressed.
    options.set_garbage(true).set_compress(true);
    let tmp: PathBuf = out.with_extension("pdf.kitab-tmp");
    let tmp_name = tmp.to_str().ok_or("the file name is not valid Unicode")?;
    doc.save_with_options(tmp_name, options)
        .map_err(|e| format!("could not write the PDF: {e}"))?;
    std::fs::rename(&tmp, out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("could not save the PDF: {e}")
    })
}

/// Write `pages` (0-based, in this order and without repeats) of `src` to a new PDF.
pub fn extract_pages(
    src: &Path,
    password: Option<&str>,
    pages: &[u32],
    out: &Path,
) -> Result<(), String> {
    if pages.is_empty() {
        return Err("no pages to save".into());
    }
    if same_file(src, out) {
        return Err("choose a different file name than the original".into());
    }
    let mut doc = open(src, password)?;
    let pages: Vec<usize> = pages.iter().map(|p| *p as usize).collect();
    doc.select_pages(PageSelection::Pages(pages))
        .map_err(|e| format!("could not select those pages: {e}"))?;
    save(&doc, out)
}

/// Write every page of `inputs`, in order, to a new PDF. Returns the page count.
pub fn merge(inputs: &[(PathBuf, Option<String>)], out: &Path) -> Result<u32, String> {
    if inputs.len() < 2 {
        return Err("choose at least two PDFs".into());
    }
    if inputs.iter().any(|(p, _)| same_file(p, out)) {
        return Err("choose a file name that isn't one of the PDFs being merged".into());
    }
    let mut dst = PdfDocument::new();
    let mut total = 0u32;
    for (path, password) in inputs {
        let src = open(path, password.as_deref())?;
        let result = dst
            .insert_pdf(&src, InsertPdfOptions::default())
            .map_err(|e| format!("could not merge {}: {e}", file_name(path)))?;
        total += result.page_count as u32;
    }
    save(&dst, out)?;
    Ok(total)
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pdf")
            .join(name)
    }

    fn page_count(p: &Path) -> usize {
        let name = p.to_str().expect("utf-8 temp path");
        PdfDocument::open(name)
            .expect("output opens")
            .page_count()
            .expect("count") as usize
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kitab-tools-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(name)
    }

    #[test]
    fn extracts_the_chosen_pages() {
        let out = temp("extract.pdf");
        extract_pages(&fixture("01-text-outline.pdf"), None, &[0, 4, 9], &out).expect("extracts");
        assert_eq!(page_count(&out), 3);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn merges_in_order() {
        let a = fixture("01-text-outline.pdf");
        let a_pages = page_count(&a);
        let out = temp("merge.pdf");
        let n = merge(&[(a.clone(), None), (a.clone(), None)], &out).expect("merges");
        assert_eq!(n as usize, a_pages * 2);
        assert_eq!(page_count(&out), a_pages * 2);
        let _ = std::fs::remove_file(&out);
    }

    /// Page counts and the password come from expected.json (AGENTS.md rule 21).
    fn expected(file: &str) -> serde_json::Value {
        let text = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/expected.json"),
        )
        .expect("expected.json");
        let all: serde_json::Value = serde_json::from_str(&text).expect("parses");
        all.pointer(&format!("/pdf/{file}"))
            .cloned()
            .unwrap_or_else(|| {
                // expected.json groups by format; fall back to a search.
                fn find(v: &serde_json::Value, key: &str) -> Option<serde_json::Value> {
                    match v {
                        serde_json::Value::Object(m) => m
                            .get(key)
                            .cloned()
                            .or_else(|| m.values().find_map(|c| find(c, key))),
                        _ => None,
                    }
                }
                find(&all, file).expect("fixture in expected.json")
            })
    }

    #[test]
    fn extracts_from_a_password_protected_pdf() {
        let name = "04a-encrypted-password-kitab.pdf";
        let want = expected(name);
        assert_eq!(want["unlocks_with_kitab"], true);
        let pages = want["pages"].as_u64().expect("pages") as u32;
        let src = fixture(name);
        let out = temp("encrypted-extract.pdf");
        assert!(
            extract_pages(&src, None, &[0], &out).is_err(),
            "needs the password"
        );
        extract_pages(&src, Some("kitab"), &[0, pages - 1], &out)
            .expect("extracts with the password");
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn refuses_to_overwrite_the_source() {
        let a = fixture("01-text-outline.pdf");
        assert!(extract_pages(&a, None, &[0], &a).is_err());
        assert!(merge(&[(a.clone(), None)], &temp("one.pdf")).is_err());
    }
}
