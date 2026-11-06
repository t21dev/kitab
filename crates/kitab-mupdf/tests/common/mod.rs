// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared helpers for the engine tests. Expectations come from
//! `tests/fixtures/expected.json` and `tests/fixtures/MANIFEST.md`, never
//! from numbers retyped here (AGENTS.md rule 21).

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::OnceLock;

use kitab_mupdf::Engine;
use serde_json::Value;

/// `tests/fixtures`, or `KITAB_FIXTURES_DIR` to run against another corpus
/// checkout (for example a branch that adds fixtures).
pub fn fixtures_dir() -> PathBuf {
    match std::env::var_os("KITAB_FIXTURES_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures"),
    }
}

pub fn pdf(name: &str) -> PathBuf {
    fixtures_dir().join("pdf").join(name)
}

fn read_text(path: PathBuf) -> String {
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    text.trim_start_matches('\u{feff}').to_string()
}

pub fn expected() -> &'static Value {
    static EXPECTED: OnceLock<Value> = OnceLock::new();
    EXPECTED.get_or_init(|| {
        let text = read_text(fixtures_dir().join("expected.json"));
        serde_json::from_str(&text).expect("expected.json parses")
    })
}

pub fn expected_pdf(name: &str) -> &'static Value {
    &expected()["pdf"][name]
}

pub fn manifest() -> &'static str {
    static MANIFEST: OnceLock<String> = OnceLock::new();
    MANIFEST.get_or_init(|| read_text(fixtures_dir().join("MANIFEST.md")))
}

/// The MANIFEST table row for a fixture file.
pub fn manifest_row(file: &str) -> &'static str {
    let marker = format!("| `{file}` |");
    manifest()
        .lines()
        .find(|l| l.starts_with(&marker))
        .unwrap_or_else(|| panic!("no MANIFEST row for {file}"))
}

/// The number that follows `marker` in `text` (digits only, `**` allowed around it).
pub fn number_after(text: &str, marker: &str) -> u32 {
    let at = text
        .find(marker)
        .unwrap_or_else(|| panic!("MANIFEST has no `{marker}`"));
    let digits: String = text[at + marker.len()..]
        .trim_start_matches('*')
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("no number after `{marker}`"))
}

/// The user password a MANIFEST row names (`user password \`…\``).
pub fn manifest_password(file: &str) -> Option<String> {
    let row = manifest_row(file);
    let at = row.find("user password `")? + "user password `".len();
    let len = row[at..].find('`')?;
    Some(row[at..at + len].to_string())
}

/// The backticked strings after `marker` in a MANIFEST row, up to the end of the sentence.
pub fn backticked_after(row: &str, marker: &str) -> Vec<String> {
    let Some(at) = row.find(marker) else {
        return Vec::new();
    };
    let rest = &row[at + marker.len()..];
    let sentence = rest.split(". ").next().unwrap_or(rest);
    sentence
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// One engine for the whole test binary, like the app has.
pub fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| Engine::new(4))
}
