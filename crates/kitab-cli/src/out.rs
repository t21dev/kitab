// SPDX-License-Identifier: AGPL-3.0-or-later

//! What the command prints and the exit codes it returns. Agents and
//! scripts read these, so they are a contract: with `--json` stdout holds one
//! JSON object and nothing else; errors go to stderr prefixed "kitab: ".

use serde_json::{Map, Value, json};
use std::io::Write;

/// The exit codes (documented in `kitab --help` and SKILL.md).
pub const EXIT_OK: i32 = 0;
pub const EXIT_FAILED: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_INPUT: i32 = 3;
pub const EXIT_PASSWORD: i32 = 4;
pub const EXIT_EXISTS: i32 = 5;

/// A failure, with the exit code and a short machine-readable code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    pub exit: i32,
    pub code: &'static str,
    pub message: String,
}

impl CliError {
    pub fn new(exit: i32, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            exit,
            code,
            message: message.into(),
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(EXIT_USAGE, "usage", message)
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self::new(EXIT_FAILED, "failed", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(EXIT_INPUT, "not_found", message)
    }

    pub fn unreadable(message: impl Into<String>) -> Self {
        Self::new(EXIT_INPUT, "unreadable", message)
    }

    pub fn exists(message: impl Into<String>) -> Self {
        Self::new(EXIT_EXISTS, "output_exists", message)
    }

    /// An engine error string, sorted into the right exit code: the tools
    /// say "needs its password" when a password is missing or wrong.
    pub fn engine(message: String) -> Self {
        if message.contains("needs its password") || message.contains("with a password") {
            Self::new(EXIT_PASSWORD, "needs_password", message)
        } else {
            Self::failed(message)
        }
    }
}

pub type Result<T> = std::result::Result<T, CliError>;

/// How to print: as JSON, quietly, or as short sentences.
#[derive(Debug, Clone, Copy, Default)]
pub struct Out {
    pub json: bool,
    pub quiet: bool,
}

impl Out {
    /// Report success: `fields` become the JSON object (with `ok` and
    /// `command` first); `human` is printed unless `--quiet`.
    pub fn done(&self, command: &str, fields: Value, human: &str) {
        if self.json {
            let mut obj = Map::new();
            if let Value::Object(m) = fields {
                obj.extend(m);
            }
            obj.remove("ok");
            obj.remove("command");
            print_json(&head(true, command), &obj);
        } else if !self.quiet && !human.is_empty() {
            print_text(human);
        }
    }

    /// Print a document's text or other content to stdout (not with
    /// `--json`, where it goes inside the object instead). `--quiet` doesn't
    /// hide it: it is what was asked for.
    pub fn content(&self, text: &str) {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        if !text.ends_with('\n') {
            let _ = stdout.write_all(b"\n");
        }
        let _ = stdout.flush();
    }

    /// A note on stderr that isn't an error ("2 pages have no text").
    pub fn note(&self, text: &str) {
        if !self.quiet && !self.json {
            eprintln!("kitab: {text}");
        }
    }

    pub fn error(&self, command: &str, e: &CliError) {
        if self.json {
            let rest = json!({"error": e.message, "code": e.code, "exit": e.exit});
            let rest = match rest {
                Value::Object(m) => m,
                _ => Map::new(),
            };
            print_json(&head(false, command), &rest);
        } else {
            eprintln!("kitab: {}", e.message);
        }
    }
}

fn print_text(text: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", text.trim_end());
    let _ = stdout.flush();
}

/// `"ok":…,"command":…`, which lead every object (serde_json's maps sort
/// their keys, so these two are written by hand to come first).
fn head(ok: bool, command: &str) -> String {
    format!(
        "\"ok\":{ok},\"command\":{}",
        serde_json::to_string(command).unwrap_or_else(|_| "\"\"".into())
    )
}

fn print_json(head: &str, rest: &Map<String, Value>) {
    let mut text = format!("{{{head}");
    for (k, v) in rest {
        let (Ok(k), Ok(v)) = (serde_json::to_string(k), serde_json::to_string(v)) else {
            continue;
        };
        text.push(',');
        text.push_str(&k);
        text.push(':');
        text.push_str(&v);
    }
    text.push('}');
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{text}");
    let _ = stdout.flush();
}

/// "1 page", "3 pages".
pub fn count(n: u64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Bytes as people read them: "812 KB", "4.2 MB".
pub fn size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} bytes")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / KB / KB)
    } else {
        format!("{:.2} GB", b / KB / KB / KB)
    }
}
