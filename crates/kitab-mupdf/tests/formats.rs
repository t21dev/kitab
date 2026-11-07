// SPDX-License-Identifier: AGPL-3.0-or-later

//! Phase 4 formats against the fixture corpus: every EPUB in expected.json
//! opens (or fails) as recorded, with the recorded page count at the
//! 400 × 600 pt, 11 pt layout, contents size and title; page 1 has ink; the
//! MANIFEST needles are found. Image files are refused (Kitab doesn't view
//! them). AGENTS.md rule 21: no expectations retyped here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use kitab_core::{FindOptions, Format, OpenError, OutlineItem, RenderRequest};
use kitab_mupdf::{REFLOW_EM, REFLOW_H, REFLOW_W, ink_ratio};
use std::time::{Duration, Instant};

fn count_outline(items: &[OutlineItem]) -> usize {
    items.iter().map(|i| 1 + count_outline(&i.children)).sum()
}

fn page_one(id: kitab_mupdf::DocId) -> kitab_core::Bitmap {
    engine()
        .render(
            id,
            RenderRequest {
                page: 0,
                scale: 1.0,
                rotation: 0,
                tile: None,
                generation: 0,
            },
        )
        .unwrap()
        .unwrap()
}

const CASELESS: FindOptions = FindOptions {
    match_case: false,
    whole_word: false,
    ignore_diacritics: true,
};

#[test]
fn the_layout_is_the_one_the_oracle_counts_pages_at() {
    // expected.json's key names the layout; the engine uses the same one.
    let key = format!(
        "pages_at_{}x{}_{}pt",
        REFLOW_W as u32, REFLOW_H as u32, REFLOW_EM as u32
    );
    assert!(
        expected()["epub"]["01-epub3-basic.epub"]
            .get(&key)
            .is_some(),
        "{key}"
    );
}

#[test]
fn every_epub_matches_expected_json() {
    let mut problems = Vec::new();
    let epubs = expected()["epub"].as_object().expect("epub section");
    for (name, exp) in epubs {
        let path = epub(name);
        let mut fail = |what: String| problems.push(format!("{name}: {what}"));
        if exp["opens"] == false {
            match engine().open(&path, None) {
                Err(OpenError::Unsupported | OpenError::Failed(_)) => {}
                other => fail(format!(
                    "should not open, got {:?}",
                    other.map(|(_, i)| i.page_count)
                )),
            }
            continue;
        }
        let start = Instant::now();
        let (id, info) = match engine().open(&path, None) {
            Ok(v) => v,
            Err(e) => {
                fail(format!("does not open: {e:?}"));
                continue;
            }
        };
        let opened = start.elapsed();
        if info.format != Format::Epub {
            fail(format!("format {:?}", info.format));
        }
        let pages = exp["pages_at_400x600_11pt"].as_u64().unwrap_or(0) as u32;
        if info.page_count != pages {
            fail(format!("{} pages, expected {pages}", info.page_count));
        }
        let outline = exp["outline"].as_u64().unwrap_or(0) as usize;
        if count_outline(&info.outline) != outline {
            fail(format!(
                "{} contents entries, expected {outline}",
                count_outline(&info.outline)
            ));
        }
        if info.title.as_deref() != exp["title"].as_str() {
            fail(format!("title {:?}", info.title));
        }
        // Not blank. (No upper bound: 08's pages are full-bleed colour boxes.)
        let ink = ink_ratio(&page_one(id));
        if ink < 0.001 {
            fail(format!("page 1 is blank (ink {ink})"));
        }
        eprintln!("{name}: {} pages, opened in {opened:?}", info.page_count);
        engine().close(id);
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn needles_are_found_in_books() {
    // 01: `kitab-epub-needle-N` per chapter (MANIFEST); 06: the last chapter's.
    let (id, _) = engine().open(&epub("01-epub3-basic.epub"), None).unwrap();
    let n = engine()
        .find(id, "kitab-epub-needle-", CASELESS, |_| true)
        .unwrap();
    assert!(n >= 1, "01: no needles");
    engine().close(id);

    let row = manifest_row("06-large-300-chapters.epub");
    let needle = backticked_after(row, "search finds ");
    let needle = needle.first().expect("06 names its needle");
    let (id, _) = engine()
        .open(&epub("06-large-300-chapters.epub"), None)
        .unwrap();
    let mut pages = Vec::new();
    let n = engine()
        .find(id, needle, CASELESS, |hit| {
            pages.push(hit.page);
            true
        })
        .unwrap();
    assert_eq!(n, 1, "06: {needle}");
    engine().close(id);
}

#[test]
fn the_large_book_opens_within_budget() {
    // S4 (PLAN.md §6) budgets the whole open-to-first-page; the engine part
    // must stay well inside it.
    let start = Instant::now();
    let (id, info) = engine()
        .open(&epub("06-large-300-chapters.epub"), None)
        .unwrap();
    let _ = page_one(id);
    let took = start.elapsed();
    eprintln!("06: {} pages, open + page 1 in {took:?}", info.page_count);
    assert!(took < Duration::from_secs(3), "{took:?}");
    engine().close(id);
}

#[test]
fn image_files_and_markdown_are_not_engine_documents() {
    let dir = std::env::temp_dir().join(format!("kitab-formats-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A real PNG header: MuPDF could read it if image documents were on.
    let png = dir.join("picture.png");
    std::fs::write(
        &png,
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x02\0\0\0",
    )
    .unwrap();
    assert!(matches!(
        engine().open(&png, None),
        Err(OpenError::Unsupported)
    ));
    let md = dir.join("notes.md");
    std::fs::write(&md, "# Notes").unwrap();
    assert!(matches!(
        engine().open(&md, None),
        Err(OpenError::Unsupported)
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plain_text_opens_as_a_book() {
    let dir = std::env::temp_dir().join(format!("kitab-txt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let txt = dir.join("story.txt");
    let body = "Kitab plain text. kitab-txt-needle\n".repeat(400);
    std::fs::write(&txt, &body).unwrap();
    let (id, info) = engine().open(&txt, None).unwrap();
    assert_eq!(info.format, Format::Txt);
    assert!(info.page_count > 1, "400 lines fill more than a page");
    assert!(ink_ratio(&page_one(id)) > 0.001);
    let n = engine()
        .find(id, "kitab-txt-needle", CASELESS, |_| true)
        .unwrap();
    assert_eq!(n, 400);
    engine().close(id);
    let _ = std::fs::remove_dir_all(&dir);
}

/// TXT, FB2, CBZ and XPS in `other/` against expected.json: opens or not,
/// page count, page sizes of fixed pages, title, contents and needle hits.
#[test]
fn every_other_format_matches_expected_json() {
    let mut problems = Vec::new();
    let files = expected()["other"].as_object().expect("other section");
    assert!(files.len() >= 6, "other/ fixtures are in expected.json");
    for (name, exp) in files {
        let path = fixtures_dir().join("other").join(name);
        let mut fail = |what: String| problems.push(format!("{name}: {what}"));
        if exp["opens"] == false {
            if engine().open(&path, None).is_ok() {
                fail("should not open".into());
            }
            continue;
        }
        let (id, info) = match engine().open(&path, None) {
            Ok(v) => v,
            Err(e) => {
                fail(format!("does not open: {e:?}"));
                continue;
            }
        };
        let want_format = match path.extension().and_then(|e| e.to_str()) {
            Some("txt") => Format::Txt,
            Some("fb2") => Format::Fb2,
            Some("cbz") => Format::Cbz,
            Some("xps") => Format::Xps,
            other => panic!("unexpected fixture extension {other:?}"),
        };
        if info.format != want_format {
            fail(format!("format {:?}", info.format));
        }
        if u64::from(info.page_count) != exp["pages"].as_u64().unwrap_or(0) {
            fail(format!(
                "{} pages, expected {}",
                info.page_count, exp["pages"]
            ));
        }
        for (i, size) in exp["page_sizes_pt"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let (w, h) = (
                size[0].as_f64().unwrap_or(0.0),
                size[1].as_f64().unwrap_or(0.0),
            );
            let got = info.page_sizes[i];
            if (f64::from(got.w) - w).abs() > 1.0 || (f64::from(got.h) - h).abs() > 1.0 {
                fail(format!(
                    "page {} is {}x{}, expected {w}x{h}",
                    i + 1,
                    got.w,
                    got.h
                ));
            }
        }
        if info.title.as_deref() != exp["title"].as_str() {
            fail(format!("title {:?}", info.title));
        }
        let outline = exp["outline"].as_u64().unwrap_or(0) as usize;
        if count_outline(&info.outline) != outline {
            fail(format!(
                "{} contents entries, expected {outline}",
                count_outline(&info.outline)
            ));
        }
        if let (Some(needle), Some(hits)) = (exp["needle"].as_str(), exp["needle_hits"].as_u64()) {
            let n = engine().find(id, needle, CASELESS, |_| true).unwrap() as u64;
            if n != hits {
                fail(format!("{n} hits for {needle}, expected {hits}"));
            }
        }
        // A broken file opens with what MuPDF could read, which may be nothing.
        if !name.contains("broken") && ink_ratio(&page_one(id)) < 0.001 {
            fail("page 1 is blank".into());
        }
        engine().close(id);
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Project Gutenberg's Kindle editions of Alice (fetch_real_world.py; skipped
/// when not fetched). MOBI reads as a book; KF8 (.azw3) is refused, since
/// MuPDF would show only its cover.
#[test]
fn kindle_books_from_gutenberg() {
    let real = fixtures_dir().join("real");
    let mobi = real.join("alice-gutenberg-11.mobi");
    if mobi.exists() {
        let (id, info) = engine().open(&mobi, None).unwrap();
        assert_eq!(info.format, Format::Mobi);
        assert!(info.page_count > 50, "{} pages", info.page_count);
        assert!(ink_ratio(&page_one(id)) > 0.001);
        let hits = engine().find(id, "Alice", CASELESS, |_| true).unwrap();
        assert!(hits > 100, "{hits} hits for Alice");
        engine().close(id);
    } else {
        eprintln!("skip: {} not fetched", mobi.display());
    }
    let azw3 = real.join("alice-gutenberg-11.azw3");
    if azw3.exists() {
        assert!(matches!(
            engine().open(&azw3, None),
            Err(OpenError::Unsupported)
        ));
    }
}

/// Books lay out again at a new page or text size: page count and sizes
/// change, the default layout gives the oracle's count back, bad layouts and
/// PDFs are refused, and pages drawn afterwards (on any worker) have the new
/// size.
#[test]
fn books_lay_out_again() {
    use kitab_core::BookLayout;
    let name = "01-epub3-basic.epub";
    let oracle = expected()["epub"][name]["pages_at_400x600_11pt"]
        .as_u64()
        .unwrap() as u32;
    let (id, info) = engine().open(&epub(name), None).unwrap();
    assert!(info.reflowable);
    assert_eq!(info.page_count, oracle);

    let wide = BookLayout {
        width: 800.0,
        height: 600.0,
        em: 11.0,
    };
    let w = engine().relayout(id, wide).unwrap();
    assert!(
        w.page_count < oracle,
        "wider pages: {} vs {oracle}",
        w.page_count
    );
    assert!(
        w.page_sizes
            .iter()
            .all(|s| (s.w - 800.0).abs() < 1.0 && (s.h - 600.0).abs() < 1.0)
    );
    assert!(
        w.outline
            .iter()
            .all(|o| o.page.is_none_or(|p| p < w.page_count))
    );

    let big = BookLayout { em: 18.0, ..wide };
    let b = engine().relayout(id, big).unwrap();
    assert!(
        b.page_count > w.page_count,
        "bigger text: {} vs {}",
        b.page_count,
        w.page_count
    );

    // Every page after the change is drawn at the new size, whichever worker
    // draws it (helpers reopen the book at the new layout).
    engine().set_generation(id, 1);
    let pages: Vec<u32> = (0..b.page_count.min(8)).collect();
    let handles: Vec<_> = pages
        .into_iter()
        .map(|page| {
            std::thread::spawn(move || {
                engine()
                    .render(
                        id,
                        RenderRequest {
                            page,
                            scale: 0.5,
                            rotation: 0,
                            tile: None,
                            generation: 1,
                        },
                    )
                    .unwrap()
                    .unwrap()
            })
        })
        .collect();
    for h in handles {
        let bmp = h.join().unwrap();
        assert!(
            (bmp.width as f32 - 400.0).abs() <= 1.0,
            "width {}",
            bmp.width
        );
        assert!(
            (bmp.height as f32 - 300.0).abs() <= 1.0,
            "height {}",
            bmp.height
        );
    }

    let back = engine().relayout(id, BookLayout::DEFAULT).unwrap();
    assert_eq!(back.page_count, oracle);

    for bad in [
        BookLayout {
            width: f32::NAN,
            ..BookLayout::DEFAULT
        },
        BookLayout {
            width: 50.0,
            ..BookLayout::DEFAULT
        },
        BookLayout {
            em: 200.0,
            ..BookLayout::DEFAULT
        },
    ] {
        assert!(engine().relayout(id, bad).is_err(), "{bad:?}");
    }
    engine().close(id);

    let (pdf_id, _) = engine().open(&pdf("01-text-outline.pdf"), None).unwrap();
    assert!(engine().relayout(pdf_id, BookLayout::DEFAULT).is_err());
    engine().close(pdf_id);
}
