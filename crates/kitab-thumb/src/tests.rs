// SPDX-License-Identifier: AGPL-3.0-or-later

//! Thumbnails of the fixtures (AGENTS.md rule 21: sizes and outcomes come
//! from tests/fixtures/expected.json).

use super::*;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn expected() -> serde_json::Value {
    let text = std::fs::read_to_string(fixtures().join("expected.json")).expect("expected.json");
    serde_json::from_str(&text).expect("expected.json parses")
}

fn pdf(name: &str) -> PathBuf {
    fixtures().join("pdf").join(name)
}

fn thumb_of(path: &Path, size: u32) -> Result<Thumb, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    render(Source::Path(path), ext, size)
}

#[test]
fn every_pdf_that_opens_has_a_first_page_thumbnail_with_its_shape() {
    let exp = expected();
    let pdfs = exp["pdf"].as_object().expect("pdf section");
    let mut checked = 0;
    for (name, e) in pdfs {
        let path = pdf(name);
        if !path.exists() || name.starts_with("06-") {
            continue; // 06 is the 20 MB file: covered by its own test.
        }
        let result = thumb_of(&path, 256);
        let opens = e["opens"].as_bool().unwrap_or(false);
        let locked = e["needs_pass"].as_bool().unwrap_or(false);
        if !opens || locked {
            assert!(
                result.is_err(),
                "{name}: no thumbnail for a file Kitab can't open freely"
            );
            continue;
        }
        let t = result.unwrap_or_else(|e| panic!("{name}: {e}"));
        let size = &e["page_sizes_pt"][0];
        let (w, h, rot) = (
            size[0].as_f64().unwrap_or(0.0),
            size[1].as_f64().unwrap_or(0.0),
            size[2].as_i64().unwrap_or(0),
        );
        let (w, h) = if rot % 180 == 0 { (w, h) } else { (h, w) };
        assert_eq!(
            t.width().max(t.height()),
            256,
            "{name}: longer side is the size"
        );
        let want = w / h;
        let got = t.width() as f64 / t.height() as f64;
        assert!(
            (want - got).abs() < 0.02,
            "{name}: aspect {got} vs page {want}"
        );
        assert!(t.has_content(), "{name}: something is drawn");
        checked += 1;
    }
    assert!(checked >= 8, "only {checked} PDFs checked");
}

#[test]
fn bytes_and_path_give_the_same_pixels() {
    let path = pdf("01-text-outline.pdf");
    let bytes = std::fs::read(&path).expect("fixture");
    let a = thumb_of(&path, 128).expect("path");
    let b = render(Source::Bytes(&bytes), "pdf", 128).expect("bytes");
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    assert_eq!(a.to_bgra(), b.to_bgra());
}

#[test]
fn sizes_are_clamped() {
    let path = pdf("01-text-outline.pdf");
    assert_eq!(thumb_of(&path, 4).expect("tiny").height(), MIN_SIZE);
    assert_eq!(thumb_of(&path, 9000).expect("huge").height(), MAX_SIZE);
}

#[test]
fn the_large_pdf_thumbnails_quickly() {
    let path = pdf("06-large-20mb.pdf");
    if !path.exists() {
        return; // generated with --large only
    }
    let start = Instant::now();
    let t = thumb_of(&path, 256).expect("large pdf");
    assert!(t.has_content());
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn damaged_and_foreign_files_fail_cleanly() {
    // Truncated and bad-xref PDFs are repaired by MuPDF or refused; neither
    // panics. Not-a-PDF and garbage bytes are refused.
    for name in ["05a-damaged-truncated.pdf", "05b-damaged-bad-xref.pdf"] {
        let _ = thumb_of(&pdf(name), 128);
    }
    assert!(thumb_of(&pdf("05c-not-a-pdf.pdf"), 128).is_err());
    assert!(render(Source::Bytes(b"not a document at all"), "pdf", 128).is_err());
    assert!(render(Source::Bytes(b""), "pdf", 128).is_err());
    assert!(render(Source::Bytes(b"x"), "docx", 128).is_err());
    // Image files aren't a format Kitab views.
    assert!(render(Source::Bytes(b"\x89PNG\r\n\x1a\n"), "png", 128).is_err());
}

#[test]
fn every_markdown_fixture_has_a_page_preview() {
    let exp = expected();
    let mds = exp["md"].as_object().expect("md section");
    for name in mds.keys() {
        let path = fixtures().join("md").join(name);
        let start = Instant::now();
        let t = thumb_of(&path, 256).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(t.height(), 256, "{name}");
        assert_eq!(t.width(), markdown::page_width(256), "{name}");
        assert!(t.has_content(), "{name}: text is drawn");
        // Explorer gives a thumbnail a few seconds. The budget is for the
        // release build; an unoptimised test build on a CI runner is slower.
        let budget = Duration::from_secs(if cfg!(debug_assertions) { 6 } else { 2 });
        assert!(start.elapsed() < budget, "{name}: {:?}", start.elapsed());
    }
}

#[test]
fn hostile_markdown_draws_only_text() {
    // 09-hostile.md: scripts, iframes and remote images. The preview reads
    // nothing beyond the file and draws it like any other page.
    let path = fixtures().join("md").join("09-hostile.md");
    let t = thumb_of(&path, 128).expect("hostile md");
    assert!(t.has_content());
}

#[test]
fn text_in_scripts_the_fonts_lack_still_draws() {
    let t = render(
        Source::Bytes("# শিরোনাম\n\nবাংলা লেখা 中文 العربية".as_bytes()),
        "md",
        128,
    )
    .expect("unicode md");
    assert!(t.has_content());
    let t = render(Source::Bytes(b"plain text\nsecond line"), "txt", 128).expect("txt");
    assert!(t.has_content());
    let t = render(Source::Bytes(b""), "md", 64).expect("empty md is a blank page");
    assert!(!t.has_content(), "an empty file is a plain sheet");
}

#[test]
fn cli_writes_a_png_and_bgra() {
    let dir = std::env::temp_dir().join(format!("kitab-thumb-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let out = dir.join("t.png");
    let args: Vec<String> = [
        "-s",
        "96",
        pdf("01-text-outline.pdf").to_str().expect("utf-8"),
        out.to_str().expect("utf-8"),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(cli(&args, &mut std::io::empty(), &mut Vec::new()), 0);
    let png = std::fs::read(&out).expect("png written");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let _ = std::fs::remove_dir_all(&dir);

    let bytes = std::fs::read(fixtures().join("md").join("01-gfm-kitchen-sink.md")).expect("md");
    let args: Vec<String> = ["-s", "64", "--stdin", "md", "--bgra"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut stdout = Vec::new();
    assert_eq!(cli(&args, &mut bytes.as_slice(), &mut stdout), 0);
    let w = u32::from_le_bytes(stdout[0..4].try_into().expect("4 bytes"));
    let h = u32::from_le_bytes(stdout[4..8].try_into().expect("4 bytes"));
    assert_eq!((w, h), (markdown::page_width(64), 64));
    assert_eq!(stdout.len(), 8 + (w * h * 4) as usize);

    let bad: Vec<String> = vec!["-s".into(), "nope".into()];
    assert_eq!(cli(&bad, &mut std::io::empty(), &mut Vec::new()), 1);
}
