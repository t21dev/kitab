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
        // Fixed layout: one page per spine document, at the book's viewport,
        // and no re-flow. Otherwise the oracle's count at the default layout.
        let fixed = exp["fixed_layout"] == true;
        let pages = if fixed {
            exp["spine_items"].as_u64().unwrap_or(0) as u32
        } else {
            exp["pages_at_400x600_11pt"].as_u64().unwrap_or(0) as u32
        };
        if info.page_count != pages {
            fail(format!("{} pages, expected {pages}", info.page_count));
        }
        if info.reflowable == fixed {
            fail(format!(
                "reflowable {} for fixed layout {fixed}",
                info.reflowable
            ));
        }
        if let Some(vp) = exp["viewport"].as_array() {
            let w = vp[0].as_f64().unwrap_or(0.0) as f32;
            if info.page_sizes.iter().any(|s| s.w < w) {
                fail(format!("pages narrower than the {w} px viewport"));
            }
        }
        if info.right_to_left != (exp["right_to_left"] == true) {
            fail(format!("right_to_left {}", info.right_to_left));
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
    // Debug builds on CI runners are several times slower than release.
    let budget = if cfg!(debug_assertions) { 10 } else { 3 };
    assert!(took < Duration::from_secs(budget), "{took:?}");
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

/// TXT, FB2, comics, XPS and DjVu in `other/` against expected.json: opens or not,
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
            Some("cbr") => Format::Cbr,
            Some("cb7") => Format::Cb7,
            Some("djvu") => Format::Djvu,
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
        ..BookLayout::DEFAULT
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

    // Typography opens the book again under the reader's style sheet: roomier
    // lines and margins take more pages; the default brings the oracle back.
    let roomy = BookLayout {
        style: kitab_core::BookStyle {
            face: kitab_core::BookFace::Sans,
            line_spacing: Some(2.4),
            margin: Some(4.0),
            ..kitab_core::BookStyle::DEFAULT
        },
        ..BookLayout::DEFAULT
    };
    let r = engine().relayout(id, roomy).unwrap();
    assert!(r.page_count > oracle, "roomy: {} vs {oracle}", r.page_count);
    let plain = engine().relayout(id, BookLayout::DEFAULT).unwrap();
    assert_eq!(plain.page_count, oracle);
    let bad_style = BookLayout {
        style: kitab_core::BookStyle {
            line_spacing: Some(9.0),
            ..kitab_core::BookStyle::DEFAULT
        },
        ..BookLayout::DEFAULT
    };
    assert!(engine().relayout(id, bad_style).is_err());

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

/// MANIFEST epub/03: the note marks are links whose own text is a mark, and
/// the text at their targets is the note (shown as a popover, not a jump).
#[test]
fn footnote_links_lead_to_their_notes() {
    use kitab_core::LinkTarget;
    use kitab_core::notes::{is_note_mark, note_text, text_in};
    let (id, _) = engine()
        .open(&epub("03-footnotes-images.epub"), None)
        .unwrap();
    let mut notes = Vec::new();
    for page in 0..3 {
        let chars = engine().page_chars(id, page).unwrap();
        for link in engine().links(id, page).unwrap() {
            let mark = text_in(&chars, link.rect);
            let LinkTarget::Page { page: to, y } = link.target else {
                continue;
            };
            if !is_note_mark(&mark) {
                continue;
            }
            let target = engine().page_chars(id, to).unwrap();
            notes.push((
                mark.trim().to_string(),
                note_text(&target, y, Some(&mark), 600),
            ));
        }
    }
    engine().close(id);
    assert_eq!(notes.len(), 2, "{notes:?}");
    notes.sort();
    assert_eq!(notes[0].0, "1");
    assert_eq!(notes[0].1, "1. The first footnote, shown as a popover.");
    assert_eq!(notes[1].1, "2. The second footnote.");
}

/// Printing Markdown: Kitab writes the document as HTML (kitab-md's
/// print module) and MuPDF lays it out on A4, like an EPUB chapter.
#[test]
fn printable_html_lays_out_on_a4() {
    let dir = std::env::temp_dir().join(format!("kitab-print-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("kitab-print-test.html");
    let mut body = String::from("<!DOCTYPE html><html><body><h1>Title</h1>");
    for i in 0..120 {
        body.push_str(&format!(
            "<p>Paragraph {i} of plain sample text for the printer.</p>"
        ));
    }
    body.push_str("<pre>fn main() {}</pre></body></html>");
    std::fs::write(&file, body).unwrap();
    let layout = kitab_core::BookLayout {
        width: 595.0,
        height: 842.0,
        em: 10.5,
        ..kitab_core::BookLayout::DEFAULT
    };
    let (id, info) = engine().open_with(&file, None, layout).unwrap();
    assert!(info.page_count >= 2, "{} pages", info.page_count);
    let size = info.page_sizes[0];
    assert!(
        (size.w - 595.0).abs() < 1.0 && (size.h - 842.0).abs() < 1.0,
        "{size:?}"
    );
    assert!(ink_ratio(&page_one(id)) > 0.001);
    engine().close(id);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn other_html_files_stay_unsupported() {
    let dir = std::env::temp_dir().join(format!("kitab-html-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("page.html");
    std::fs::write(&file, "<html><body><p>hi</p></body></html>").unwrap();
    assert!(matches!(
        engine().open(&file, None),
        Err(OpenError::Unsupported)
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

fn render(
    id: kitab_mupdf::DocId,
    page: u32,
    scale: f32,
    tile: Option<kitab_core::TilePx>,
) -> kitab_core::Bitmap {
    engine()
        .render(
            id,
            RenderRequest {
                page,
                scale,
                rotation: 0,
                tile,
                generation: 0,
            },
        )
        .unwrap()
        .unwrap()
}

/// Comics in RAR and 7z and DjVu draw through Kitab's own page source: every
/// page draws (WebP too, which MuPDF can't decode), at any zoom and in tiles;
/// DjVu text reads; pages save to a PDF; two opens of a file share it.
#[test]
fn comics_and_djvu_draw_through_kitabs_page_source() {
    let other = fixtures_dir().join("other");
    let (cb7, info) = engine().open(&other.join("07-comic.cb7"), None).unwrap();
    for page in 0..info.page_count {
        assert!(
            ink_ratio(&render(cb7, page, 0.5, None)) > 0.001,
            "cb7 page {}",
            page + 1
        );
    }
    engine().close(cb7);
    for name in ["08-stored-rar5.cbr", "09-stored-rar4.cbr"] {
        let (id, info) = engine().open(&other.join(name), None).unwrap();
        let last = render(id, info.page_count - 1, 0.5, None);
        assert!(ink_ratio(&last) > 0.001, "{name}");
        engine().close(id);
    }

    let djvu = other.join("11-scan-cyrillic.djvu");
    let (a, info) = engine().open(&djvu, None).unwrap();
    let (b, _) = engine().open(&djvu, None).unwrap();
    // A thumbnail, the page, and a tile at 300 %.
    let small = render(a, 5, 0.2, None);
    let page = render(a, 5, 1.5, None);
    assert!(ink_ratio(&small) > 0.001 && ink_ratio(&page) > 0.001);
    let size = info.page_sizes[5];
    assert_eq!(page.width, (size.w * 1.5).round() as u32);
    let tile = kitab_core::TilePx {
        x: 256,
        y: 256,
        w: 256,
        h: 256,
    };
    assert_eq!(render(a, 5, 3.0, Some(tile)).width, 256);
    assert!(engine().page_has_text(a, 5).unwrap());
    assert!(!engine().page_chars(a, 5).unwrap().is_empty());
    // The second open still draws once the first is closed.
    engine().close(a);
    assert!(ink_ratio(&render(b, 0, 1.0, None)) > 0.001);

    // Save two pages as a PDF: they draw there at full size.
    let out = std::env::temp_dir().join(format!("kitab-djvu-pages-{}.pdf", std::process::id()));
    assert_eq!(engine().write_pdf(b, vec![0, 5], out.clone()).unwrap(), 2);
    engine().close(b);
    let (pdf, pdf_info) = engine().open(&out, None).unwrap();
    assert_eq!(pdf_info.page_count, 2);
    assert!(ink_ratio(&page_one(pdf)) > 0.001);
    engine().close(pdf);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn djvu_links_go_to_pages_and_web_addresses() {
    use djvu_rs::annotation::{Annotation, MapArea, Rect, Shape};
    use djvu_rs::djvu_mut::DjVuDocumentMut;
    use kitab_core::LinkTarget;

    // The scan has no links: give page 1 three, in DjVu's own pixels
    // (129 dpi, counted from the bottom).
    let src = std::fs::read(fixtures_dir().join("other/11-scan-cyrillic.djvu")).unwrap();
    let area = |url: &str, shape: Shape| MapArea {
        url: url.into(),
        target: None,
        description: String::new(),
        shape,
        border: None,
        highlight: None,
        extra: Vec::new(),
    };
    let areas = [
        area(
            "#+2",
            Shape::Rect(Rect {
                x: 129,
                y: 258,
                width: 258,
                height: 129,
            }),
        ),
        area(
            "https://example.org/",
            Shape::Poly(vec![(0, 0), (129, 0), (129, 129)]),
        ),
        // Never followed: scripts, files.
        area(
            "javascript:alert(1)",
            Shape::Rect(Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            }),
        ),
    ];
    let mut doc = DjVuDocumentMut::from_bytes(&src).unwrap();
    doc.page_mut(0)
        .unwrap()
        .set_annotations(&Annotation::default(), &areas);
    let path = std::env::temp_dir().join(format!("kitab-djvu-links-{}.djvu", std::process::id()));
    std::fs::write(&path, doc.into_bytes()).unwrap();

    let (id, info) = engine().open(&path, None).unwrap();
    let links = engine().links(id, 0).unwrap();
    let h = info.page_sizes[0].h;
    assert_eq!(links.len(), 2, "{links:?}");
    // 129 px at 129 dpi is 72 pt.
    let near = |a: f32, b: f32| (a - b).abs() < 0.5;
    let r = links[0].rect;
    assert_eq!(links[0].target, LinkTarget::Page { page: 2, y: None });
    assert!(
        near(r.x, 72.0) && near(r.w, 144.0) && near(r.h, 72.0),
        "{r:?}"
    );
    assert!(near(r.y, h - 216.0), "{r:?}");
    let r = links[1].rect;
    assert_eq!(
        links[1].target,
        LinkTarget::Uri("https://example.org/".into())
    );
    assert!(
        near(r.x, 0.0) && near(r.y, h - 72.0) && near(r.w, 72.0),
        "{r:?}"
    );
    // Pages without links have none.
    assert!(engine().links(id, 1).unwrap().is_empty());
    engine().close(id);
    let _ = std::fs::remove_file(&path);
}
