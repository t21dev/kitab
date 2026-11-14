// SPDX-License-Identifier: AGPL-3.0-or-later

//! Engine tests against the fixture corpus (PLAN.md §7.2, layer 2).

// Tests may unwrap (AGENTS.md rule 9).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use kitab_core::{FindOptions, LinkTarget, OpenError, OutlineItem, RenderRequest, TilePx};
use kitab_mupdf::{DocId, Engine, ink_ratio};

fn open(name: &str) -> (DocId, kitab_core::DocInfo) {
    let password = manifest_password(name);
    engine()
        .open(&pdf(name), password.as_deref())
        .unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn count_outline(items: &[OutlineItem]) -> usize {
    items.iter().map(|i| 1 + count_outline(&i.children)).sum()
}

fn request(page: u32, scale: f32) -> RenderRequest {
    RenderRequest {
        page,
        scale,
        rotation: 0,
        tile: None,
        generation: 0,
    }
}

/// Every PDF in expected.json opens (or fails) as recorded, with the recorded
/// page count, page sizes, title, outline size, permissions, encryption,
/// repair flag, link count and text layer on page 1.
#[test]
fn every_pdf_matches_expected_json() {
    let engine = engine();
    let mut problems = Vec::new();
    let mut checked = 0;
    let pdfs = expected()["pdf"].as_object().expect("pdf section");
    for (name, exp) in pdfs {
        let path = pdf(name);
        if !path.exists() {
            eprintln!("skip {name}: not in this checkout");
            continue;
        }
        checked += 1;
        let mut fail = |what: String| problems.push(format!("{name}: {what}"));

        if exp["opens"] == false {
            match engine.open(&path, None) {
                Err(OpenError::Unsupported | OpenError::Failed(_)) => {}
                other => fail(format!("should not open, got {other:?}")),
            }
            continue;
        }

        let password = manifest_password(name);
        if exp["needs_pass"] == true {
            match engine.open(&path, None) {
                Err(OpenError::NeedsPassword) => {}
                other => fail(format!("without a password: {:?}", other.map(|r| r.0))),
            }
            match engine.open(&path, Some("not the password")) {
                Err(OpenError::WrongPassword) => {}
                other => fail(format!("wrong password: {:?}", other.map(|r| r.0))),
            }
            if exp["unlocks_with_kitab"] == false {
                match engine.open(&path, Some("kitab")) {
                    Err(OpenError::WrongPassword) => {}
                    other => fail(format!(
                        "`kitab` should be rejected: {:?}",
                        other.map(|r| r.0)
                    )),
                }
            }
            if password.is_none() {
                fail("MANIFEST names no password".into());
                continue;
            }
        }

        let (id, info) = match engine.open(&path, password.as_deref()) {
            Ok(r) => r,
            Err(e) => {
                fail(format!("open: {e:?}"));
                continue;
            }
        };
        if let Some(n) = exp["pages"].as_u64()
            && u64::from(info.page_count) != n
        {
            fail(format!("pages {} != {n}", info.page_count));
        }
        if info.page_sizes.len() != info.page_count as usize {
            fail("one size per page".into());
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
            match info.page_sizes.get(i) {
                Some(s)
                    if (f64::from(s.w) - w).abs() <= 0.5 && (f64::from(s.h) - h).abs() <= 0.5 => {}
                other => fail(format!("page {i} size {other:?} != {w} x {h}")),
            }
        }
        if let Some(t) = exp["title"].as_str()
            && info.title.as_deref() != Some(t)
        {
            fail(format!("title {:?}", info.title));
        }
        if let Some(n) = exp["outline"].as_u64()
            && count_outline(&info.outline) as u64 != n
        {
            fail(format!(
                "outline {} entries != {n}",
                count_outline(&info.outline)
            ));
        }
        let perms = [
            ("permissions_print", info.permissions.print),
            ("permissions_copy", info.permissions.copy),
            ("permissions_fill_forms", info.permissions.fill_forms),
            ("permissions_modify", info.permissions.modify),
        ];
        for (key, got) in perms {
            if let Some(want) = exp[key].as_bool()
                && want != got
            {
                fail(format!("{key} {got} != {want}"));
            }
        }
        if let Some(want) = exp["is_repaired"].as_bool()
            && want != info.repaired
        {
            fail(format!("repaired {} != {want}", info.repaired));
        }
        if let Some(enc) = exp.get("encryption") {
            let want = enc.as_str();
            if info.encryption.as_deref() != want {
                fail(format!("encryption {:?} != {want:?}", info.encryption));
            }
        } else if exp["needs_pass"] == true && info.encryption.is_none() {
            fail("encrypted but no encryption description".into());
        }
        if let Some(n) = exp["links_page1"].as_u64() {
            match engine.links(id, 0) {
                Ok(links) if links.len() as u64 == n => {}
                other => fail(format!("links on page 1: {other:?}, want {n}")),
            }
        }
        if let Some(chars) = exp["text_chars_page1"].as_u64() {
            match engine.page_has_text(id, 0) {
                Ok(has) if has == (chars > 0) => {}
                other => fail(format!("page 1 has text: {other:?}, chars {chars}")),
            }
        }
        engine.close(id);
    }
    assert!(checked >= 10, "only {checked} fixtures found");
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn outline_of_01_has_chapters_and_closed_state() {
    let row = manifest_row("01-text-outline.pdf");
    let chapters = number_after(row, "2-level outline (") as usize;
    let first_closed = number_after(row, "closed state from chapter ") as usize;
    let (id, info) = open("01-text-outline.pdf");
    assert_eq!(info.outline.len(), chapters);
    for (i, chapter) in info.outline.iter().enumerate() {
        assert!(chapter.page.is_some(), "chapter {i} has a page target");
        assert!(!chapter.children.is_empty(), "chapter {i} has sections");
        let should_be_open = i + 1 < first_closed;
        assert_eq!(chapter.open, should_be_open, "chapter {} open state", i + 1);
    }
    let pages: Vec<u32> = info.outline.iter().filter_map(|c| c.page).collect();
    assert!(
        pages.windows(2).all(|w| w[0] < w[1]),
        "chapters in page order: {pages:?}"
    );
    engine().close(id);
}

#[test]
fn page_sizes_and_rotation_of_02() {
    let exp = expected_pdf("02-mixed-page-sizes.pdf");
    let (id, info) = open("02-mixed-page-sizes.pdf");
    let sizes = exp["page_sizes_pt"].as_array().expect("sizes");
    let rotated = sizes
        .iter()
        .position(|s| s[2].as_u64() == Some(90))
        .expect("02 has a /Rotate 90 page");
    // The rotated page is landscape: its size already has /Rotate applied.
    let size = info.page_sizes[rotated];
    assert!(size.w > size.h, "rotated page shows landscape: {size:?}");
    // Distinct sizes stay distinct (true page sizes, PLAN.md §3.3).
    let mut distinct: Vec<(i32, i32)> = info
        .page_sizes
        .iter()
        .map(|s| (s.w.round() as i32, s.h.round() as i32))
        .collect();
    distinct.sort();
    distinct.dedup();
    assert!(distinct.len() >= 4, "{distinct:?}");

    // Rendering the rotated page follows its own orientation, and an extra
    // 90 degrees turns it back.
    let scale = 0.5;
    let bmp = engine()
        .render(id, request(rotated as u32, scale))
        .unwrap()
        .unwrap();
    assert!(bmp.width > bmp.height);
    let turned = engine()
        .render(
            id,
            RenderRequest {
                rotation: 90,
                ..request(rotated as u32, scale)
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!((turned.width, turned.height), (bmp.height, bmp.width));
    engine().close(id);
}

/// The pages chosen for printing come out as a PDF of just those pages, at
/// their own sizes (what macOS and Linux hand to CUPS).
#[test]
fn write_pdf_keeps_the_chosen_pages_and_sizes() {
    let (id, info) = open("02-mixed-page-sizes.pdf");
    let dir = std::env::temp_dir().join(format!("kitab-write-pdf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("chosen.pdf");
    let pages = vec![2, 0];
    assert_eq!(engine().write_pdf(id, pages.clone(), out.clone()), Ok(2));
    let (copy, copied) = engine().open(&out, None).unwrap();
    assert_eq!(copied.page_sizes.len(), 2);
    for (n, &p) in pages.iter().enumerate() {
        let (a, b) = (info.page_sizes[p as usize], copied.page_sizes[n]);
        assert!(
            (a.w - b.w).abs() < 1.0 && (a.h - b.h).abs() < 1.0,
            "{a:?} {b:?}"
        );
    }
    // A page the document doesn't have is an error, not a panic.
    assert!(
        engine()
            .write_pdf(id, vec![9_999], dir.join("bad.pdf"))
            .is_err()
    );
    engine().close(copy);
    engine().close(id);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn find_counts_in_01() {
    let exp = &expected_pdf("01-text-outline.pdf")["search"];
    let row = manifest_row("01-text-outline.pdf");
    let (id, _) = open("01-text-outline.pdf");
    let count = |query: &str, options: FindOptions| {
        engine()
            .find(id, query, options, |_| true)
            .unwrap_or_else(|e| panic!("find {query}: {e}"))
    };
    let caseless = FindOptions {
        match_case: false,
        whole_word: false,
        ignore_diacritics: true,
    };
    let whole_word = FindOptions {
        whole_word: true,
        ..caseless
    };
    let cased = FindOptions {
        match_case: true,
        ..caseless
    };
    let want = |key: &str| exp[key].as_u64().unwrap_or_else(|| panic!("no {key}")) as usize;
    assert_eq!(
        count("lantern", caseless),
        want("lantern (case-insensitive, substring)")
    );
    assert_eq!(
        count("lantern", whole_word),
        number_after(row, "whole word, case-insensitive = ") as usize
    );
    assert_eq!(count("Lantern", cased), want("Lantern (case-sensitive)"));

    let mut pages = Vec::new();
    let n = engine()
        .find(id, "kitab-needle-17", caseless, |hit| {
            assert!(!hit.quads.is_empty());
            assert!(hit.context.contains("kitab-needle-17"), "{}", hit.context);
            pages.push(hit.page);
            true
        })
        .unwrap();
    assert_eq!(n, want("kitab-needle-17"));
    let page = number_after(row, "`kitab-needle-17` = **1**, on page ");
    assert_eq!(pages, vec![page - 1]);

    // `false` stops the search after the first hit.
    let mut seen = 0;
    let delivered = engine()
        .find(id, "lantern", caseless, |_| {
            seen += 1;
            false
        })
        .unwrap();
    assert_eq!((seen, delivered), (1, 1));
    engine().close(id);
}

#[test]
fn unicode_search_in_07() {
    let row = manifest_row("07-unicode-rtl.pdf");
    let queries = backticked_after(row, "Search finds ");
    assert!(!queries.is_empty(), "MANIFEST lists queries for 07");
    let (id, _) = open("07-unicode-rtl.pdf");
    assert!(engine().page_has_text(id, 0).unwrap());
    let options = FindOptions {
        match_case: false,
        whole_word: false,
        ignore_diacritics: true,
    };
    for q in queries {
        let n = engine().find(id, &q, options, |_| true).unwrap();
        assert!(n >= 1, "07 finds {q}");
    }
    engine().close(id);
}

#[test]
fn scanned_page_of_10_has_no_text() {
    let exp = expected_pdf("10-scanned-and-images.pdf");
    assert_eq!(exp["text_chars_page1"].as_u64(), Some(0));
    let (id, _) = open("10-scanned-and-images.pdf");
    assert!(!engine().page_has_text(id, 0).unwrap());
    // It is still an image, not a blank page.
    let bmp = engine().render(id, request(0, 0.5)).unwrap().unwrap();
    assert!(ink_ratio(&bmp) > 0.01, "ink {}", ink_ratio(&bmp));
    engine().close(id);
}

#[test]
fn links_on_page_1_of_01() {
    let exp = expected_pdf("01-text-outline.pdf");
    let row = manifest_row("01-text-outline.pdf");
    let target = number_after(row, "lands on page ");
    let (id, info) = open("01-text-outline.pdf");
    let links = engine().links(id, 0).unwrap();
    assert_eq!(Some(links.len() as u64), exp["links_page1"].as_u64());
    let internal: Vec<u32> = links
        .iter()
        .filter_map(|l| match l.target {
            LinkTarget::Page { page, .. } => Some(page),
            LinkTarget::Uri(_) => None,
        })
        .collect();
    assert_eq!(internal, vec![target - 1], "internal link to page {target}");
    assert!(
        links
            .iter()
            .any(|l| matches!(&l.target, LinkTarget::Uri(u) if u.contains("://"))),
        "an external link"
    );
    let page = info.page_sizes[0];
    for l in &links {
        assert!(l.rect.w > 0.0 && l.rect.h > 0.0);
        assert!(l.rect.x >= 0.0 && l.rect.x + l.rect.w <= page.w + 0.5);
        assert!(l.rect.y >= 0.0 && l.rect.y + l.rect.h <= page.h + 0.5);
    }
    engine().close(id);
}

/// An earlier spike rendered blank pages: insist on real ink.
#[test]
fn renders_page_1_of_01_with_ink() {
    let (id, info) = open("01-text-outline.pdf");
    let scale = 1.5;
    let bmp = engine().render(id, request(0, scale)).unwrap().unwrap();
    let size = info.page_sizes[0];
    assert!((f64::from(bmp.width) - f64::from(size.w * scale)).abs() <= 1.0);
    assert!((f64::from(bmp.height) - f64::from(size.h * scale)).abs() <= 1.0);
    assert_eq!(bmp.bgra.len(), bmp.width as usize * bmp.height as usize * 4);
    let ink = ink_ratio(&bmp);
    assert!(ink > 0.01, "page looks blank: ink {ink}");
    assert!(ink < 0.6, "page looks inverted or black: ink {ink}");
    // Opaque and on white: corners are paper.
    assert!(bmp.bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    assert_eq!(&bmp.bgra[..4], &[255, 255, 255, 255]);

    // A tile is exactly the matching region of the full page.
    let tile = TilePx {
        x: 100,
        y: 120,
        w: 300,
        h: 200,
    };
    let part = engine()
        .render(
            id,
            RenderRequest {
                tile: Some(tile),
                ..request(0, scale)
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!((part.width, part.height), (tile.w, tile.h));
    for row in 0..tile.h as usize {
        let full_start = ((tile.y as usize + row) * bmp.width as usize + tile.x as usize) * 4;
        let full = &bmp.bgra[full_start..full_start + tile.w as usize * 4];
        let tiled = &part.bgra[row * tile.w as usize * 4..(row + 1) * tile.w as usize * 4];
        assert_eq!(full, tiled, "tile row {row} differs");
    }
    engine().close(id);
}

#[test]
fn stale_generation_is_dropped() {
    let (id, _) = open("01-text-outline.pdf");
    let engine = engine();
    engine.set_generation(id, 5);
    let stale = RenderRequest {
        generation: 4,
        ..request(0, 0.25)
    };
    assert_eq!(engine.render(id, stale).unwrap(), None);
    let current = RenderRequest {
        generation: 5,
        ..request(0, 0.25)
    };
    assert!(engine.render(id, current).unwrap().is_some());
    // Generations never go back.
    engine.set_generation(id, 1);
    assert_eq!(engine.render(id, stale).unwrap(), None);
    // Printing asks with u64::MAX, so no view change can drop its pages.
    let print = RenderRequest {
        generation: u64::MAX,
        ..request(0, 0.25)
    };
    assert!(engine.render(id, print).unwrap().is_some());
    engine.close(id);
}

#[test]
fn bad_requests_are_errors_not_panics() {
    let (id, info) = open("01-text-outline.pdf");
    let engine = engine();
    assert!(engine.render(id, request(info.page_count, 1.0)).is_err());
    assert!(engine.render(id, request(0, 0.0)).is_err());
    assert!(engine.render(id, request(0, f32::NAN)).is_err());
    assert!(
        engine.render(id, request(0, 1000.0)).is_err(),
        "over the bitmap limit"
    );
    let odd = RenderRequest {
        rotation: 45,
        ..request(0, 1.0)
    };
    assert!(engine.render(id, odd).is_err());
    let outside = RenderRequest {
        tile: Some(TilePx {
            x: 100_000,
            y: 0,
            w: 10,
            h: 10,
        }),
        ..request(0, 1.0)
    };
    assert!(engine.render(id, outside).is_err());
    assert!(engine.links(id, info.page_count).is_err());
    engine.close(id);
    assert!(engine.render(id, request(0, 1.0)).is_err(), "closed");
    assert!(engine.links(id, 0).is_err(), "closed");
}

#[test]
fn missing_file_is_not_found() {
    let missing = pdf("does-not-exist.pdf");
    assert_eq!(
        engine().open(&missing, None).err(),
        Some(OpenError::NotFound)
    );
}

/// Several documents at once, each pinned to its own worker, rendered from
/// several caller threads.
#[test]
fn several_documents_in_parallel() {
    let engine = Engine::new(3);
    assert_eq!(
        engine.started_threads(),
        0,
        "no thread before the first job"
    );
    let names = [
        "01-text-outline.pdf",
        "02-mixed-page-sizes.pdf",
        "09-annotations.pdf",
    ];
    let ids: Vec<DocId> = names
        .iter()
        .map(|n| engine.open(&pdf(n), None).unwrap().0)
        .collect();
    assert_eq!(engine.started_threads(), 3);
    std::thread::scope(|s| {
        for &id in &ids {
            let engine = &engine;
            s.spawn(move || {
                for _ in 0..3 {
                    let bmp = engine.render(id, request(0, 0.5)).unwrap().unwrap();
                    assert!(ink_ratio(&bmp) > 0.0);
                }
            });
        }
    });
    assert!(engine.shrink_caches(0));
    for id in ids {
        engine.close(id);
    }
    assert_eq!(engine.open_documents(), 0);
}

/// 04f's password is non-ASCII; it opens however the text was normalised
/// (typed composed, pasted decomposed), and plain `kitab` does not.
#[test]
fn unicode_password_in_any_normalisation() {
    use unicode_normalization::UnicodeNormalization;
    let name = "04f-encrypted-aes-256-unicode.pdf";
    if !pdf(name).exists() {
        eprintln!("skip {name}: not in this checkout");
        return;
    }
    let password = manifest_password(name).expect("MANIFEST names the password");
    assert!(!password.is_ascii());
    for form in [
        password.nfc().collect::<String>(),
        password.nfd().collect::<String>(),
    ] {
        let (id, _) = engine()
            .open(&pdf(name), Some(&form))
            .unwrap_or_else(|e| panic!("{form:?}: {e:?}"));
        engine().close(id);
    }
    assert_eq!(
        engine().open(&pdf(name), Some("kitab")).err(),
        Some(OpenError::WrongPassword)
    );
}

/// Renders of one document from several threads go to helper workers that
/// open their own handle; the pixels must match one-at-a-time renders, for a
/// plain and a password-protected file, and closing must still work.
#[test]
fn parallel_renders_of_one_document_match_serial_ones() {
    for name in ["01-text-outline.pdf", "04a-encrypted-password-kitab.pdf"] {
        let engine = std::sync::Arc::new(Engine::new(4));
        let password = manifest_password(name);
        let (doc, info) = engine.open(&pdf(name), password.as_deref()).expect("opens");
        let pages: Vec<u32> = (0..info.page_count.min(8)).collect();
        let serial: Vec<_> = pages
            .iter()
            .map(|p| {
                engine
                    .render(doc, request(*p, 0.5))
                    .expect("renders")
                    .expect("fresh")
            })
            .collect();
        let handles: Vec<_> = pages
            .iter()
            .map(|p| {
                let (engine, p) = (engine.clone(), *p);
                std::thread::spawn(move || {
                    engine
                        .render(doc, request(p, 0.5))
                        .expect("renders")
                        .expect("fresh")
                })
            })
            .collect();
        for (h, want) in handles.into_iter().zip(serial) {
            assert_eq!(
                h.join().expect("thread"),
                want,
                "{name}: parallel render differs"
            );
        }
        assert!(engine.started_threads() > 1, "{name}: helpers were used");
        engine.close(doc);
        assert!(
            engine.render(doc, request(0, 0.5)).is_err(),
            "{name}: closed"
        );
    }
}

/// The annotations already in 09 are listed with their kinds and notes, in
/// order (expected.json `annot_kinds_page1`, `annot_notes_page1`); a PDF
/// without any lists none.
#[test]
fn existing_annotations_are_listed() {
    let exp = expected_pdf("09-annotations.pdf");
    let (id, _) = open("09-annotations.pdf");
    let annots = engine().annotations(id).unwrap();
    let kinds: Vec<&str> = annots
        .iter()
        .filter(|a| a.page == 0)
        .map(|a| a.kind.as_str())
        .collect();
    let want: Vec<&str> = exp["annot_kinds_page1"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(kinds, want);
    let notes: Vec<String> = annots
        .iter()
        .filter(|a| a.page == 0)
        .map(|a| a.text.clone().unwrap_or_default())
        .collect();
    let want_notes: Vec<String> = exp["annot_notes_page1"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().trim().to_string())
        .collect();
    assert_eq!(notes, want_notes);
    assert!(annots.iter().all(|a| a.rect.w >= 0.0 && a.rect.h >= 0.0));
    engine().close(id);

    let (plain, _) = open("01-text-outline.pdf");
    assert!(engine().annotations(plain).unwrap().is_empty());
    engine().close(plain);
}
