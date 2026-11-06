// SPDX-License-Identifier: AGPL-3.0-or-later

//! Black-box start-up benchmark (PLAN.md §6.1), measured the same way for Kitab
//! and SumatraPDF:
//!
//! 1. start the app with a file,
//! 2. poll for its top-level window and capture it (PrintWindow, full content),
//! 3. stop the clock when the middle of the window shows non-blank content that
//!    is identical in two captures in a row ("first page sharp", S2).
//!
//! It also records when the window first appeared ("window shown"). The timing
//! resolution is the capture interval, which the report prints so results are
//! never read as more precise than they are.
//!
//! Usage:
//!   kitab-bench s2 [--file F] [--runs 15] [--warmup 2]
//!                  [--kitab PATH] [--kitab-arg ARG]… [--sumatra PATH] [--only kitab|sumatra]
//!                  [--out DIR]

#[cfg(windows)]
mod bench;

#[cfg(windows)]
fn main() {
    std::process::exit(bench::run(std::env::args().skip(1).collect()));
}

#[cfg(not(windows))]
fn main() {
    eprintln!(
        "kitab-bench: the black-box start-up benchmark runs on Windows only for now (PLAN.md §6.4)."
    );
    std::process::exit(2);
}
