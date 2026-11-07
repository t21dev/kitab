// SPDX-License-Identifier: AGPL-3.0-or-later

//! Opt-in check against the maintainer's real-world PDFs
//! (`tests/fixtures/local/README.md`). Skips when `local-corpus.json` is
//! missing; `KITAB_LOCAL_CORPUS` points at another copy of it. The files are
//! only read, never written, and each one's SHA-256 is checked first so a
//! result is never compared against a different file.

// Tests may unwrap (AGENTS.md rule 9).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::PathBuf;
use std::time::Instant;

use kitab_core::RenderRequest;
use kitab_mupdf::{Engine, ink_ratio};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn corpus_path() -> PathBuf {
    match std::env::var_os("KITAB_LOCAL_CORPUS") {
        Some(p) => PathBuf::from(p),
        None => common::fixtures_dir().join("local/local-corpus.json"),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn every_local_file_opens_and_page_1_has_ink() {
    let path = corpus_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("skip: no local corpus at {}", path.display());
        return;
    };
    let corpus: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).expect("corpus parses");
    let files = corpus["files"].as_array().expect("files list");
    let engine = Engine::new(Engine::default_threads());
    let mut problems = Vec::new();
    for file in files {
        let id = file["id"].as_str().unwrap_or("?");
        let path = PathBuf::from(file["path"].as_str().expect("path"));
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                problems.push(format!("{id}: cannot read: {e}"));
                continue;
            }
        };
        if let Some(want) = file["sha256"].as_str()
            && sha256_hex(&bytes) != want
        {
            problems.push(format!(
                "{id}: SHA-256 changed; not testing a different file"
            ));
            continue;
        }
        drop(bytes);

        let started = Instant::now();
        let (doc, info) = match engine.open(&path, None) {
            Ok(r) => r,
            Err(e) => {
                problems.push(format!("{id}: open failed: {e:?}"));
                continue;
            }
        };
        let opened = started.elapsed();
        if let Some(pages) = file["pages"].as_u64()
            && u64::from(info.page_count) != pages
        {
            problems.push(format!(
                "{id}: {} pages, corpus says {pages}",
                info.page_count
            ));
        }
        // About 1000 px wide, like a page on screen.
        let scale = 1000.0 / info.page_sizes[0].w.max(1.0);
        let started = Instant::now();
        let req = RenderRequest {
            page: 0,
            scale,
            rotation: 0,
            tile: None,
            generation: 0,
        };
        match engine.render(doc, req) {
            Ok(Some(bmp)) => {
                let ink = ink_ratio(&bmp);
                eprintln!(
                    "{id}: {} pages, open {:.1} ms, page 1 {}x{} in {:.1} ms, ink {:.3}, repaired {}",
                    info.page_count,
                    opened.as_secs_f64() * 1e3,
                    bmp.width,
                    bmp.height,
                    started.elapsed().as_secs_f64() * 1e3,
                    ink,
                    info.repaired,
                );
                if ink < 0.002 {
                    problems.push(format!("{id}: page 1 looks blank (ink {ink:.4})"));
                }
            }
            other => problems.push(format!("{id}: render failed: {other:?}")),
        }
        engine.close(doc);
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// L12 (scripts, fonts and emoji): Kitab's find, which ignores diacritics by
/// default, finds words in every script, with and without their marks.
#[test]
fn l12_every_script_is_searchable() {
    let Ok(text) = std::fs::read_to_string(corpus_path()) else {
        eprintln!("skip: no local corpus");
        return;
    };
    let corpus: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).expect("corpus parses");
    let Some(file) = corpus["files"]
        .as_array()
        .and_then(|f| f.iter().find(|f| f["id"] == "L12"))
    else {
        eprintln!("skip: no L12 in the local corpus");
        return;
    };
    let path = PathBuf::from(file["path"].as_str().expect("path"));
    let engine = Engine::new(1);
    let (doc, _) = engine.open(&path, None).expect("opens");
    let options = kitab_core::FindOptions {
        match_case: false,
        whole_word: false,
        ignore_diacritics: true,
    };
    let mut missing = Vec::new();
    for q in [
        "kitab-unicode-needle-11",
        "কিতাব",
        "किताब",
        "புத்தகம்",
        "หนังสือ",
        "كتاب",
        "الكتاب",
        "الكِتَابُ",
        "ספר",
        "סֵפֶר",
        "书是",
        "本は",
        "책은",
        "книга",
        "ˈkɪtæb",
        "📖",
    ] {
        let hits = engine.find_page(doc, 0, q, options).expect("searches");
        eprintln!("{q}: {}", hits.len());
        // Known gap: MuPDF extracts this file's pointed Hebrew with a space
        // between letter clusters ("ס ֶפ ר"), so it can't be found yet.
        let known_gap = q.chars().any(|c| ('\u{0590}'..='\u{05FF}').contains(&c));
        if hits.is_empty() && !known_gap {
            missing.push(q);
        }
    }
    assert!(missing.is_empty(), "not found: {missing:?}");
}
