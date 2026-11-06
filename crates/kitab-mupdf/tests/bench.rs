// SPDX-License-Identifier: AGPL-3.0-or-later

//! A quick timing of the engine's first-page path. Not a CI gate (kitab-bench
//! owns the scenarios in PLAN.md §6); run it by hand, in release:
//!
//! ```text
//! cargo test -p kitab-mupdf --release --test bench -- --ignored --nocapture
//! ```

// Tests may unwrap (AGENTS.md rule 9).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use kitab_core::RenderRequest;
use kitab_mupdf::Engine;

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

#[test]
#[ignore = "timing; run by hand with --release --ignored --nocapture"]
fn open_and_render_01_page_1() {
    let path = common::pdf("01-text-outline.pdf");
    let req = RenderRequest {
        page: 0,
        scale: 1.5,
        rotation: 0,
        tile: None,
        generation: 0,
    };

    // Cold: a new engine, so the worker thread and MuPDF context start too.
    let engine = Engine::new(2);
    let t = Instant::now();
    let (doc, _) = engine.open(&path, None).expect("opens");
    let cold_open = t.elapsed();
    let t = Instant::now();
    let bmp = engine.render(doc, req).expect("renders").expect("current");
    let cold_render = t.elapsed();
    engine.close(doc);

    // Warm: the same engine, ten times each.
    let mut opens = Vec::new();
    let mut renders = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        let (doc, _) = engine.open(&path, None).expect("opens");
        opens.push(t.elapsed());
        let t = Instant::now();
        engine.render(doc, req).expect("renders").expect("current");
        renders.push(t.elapsed());
        engine.close(doc);
    }

    println!(
        "01 page 1 at 1.5x ({}x{} px): cold open {:.2} ms, cold render {:.2} ms; \
         warm median open {:.2} ms, render {:.2} ms",
        bmp.width,
        bmp.height,
        ms(cold_open),
        ms(cold_render),
        ms(median(opens)),
        ms(median(renders)),
    );
}
