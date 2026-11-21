// SPDX-License-Identifier: AGPL-3.0-or-later

//! What every command shares: its parsed arguments, how to print, the
//! engine, passwords, opening inputs (with the right exit code for each way
//! that fails) and checking outputs (never an input, never an existing file
//! without --force).

use crate::args::Parsed;
use crate::out::{CliError, Out, Result};
use kitab_core::{DocInfo, Format, OpenError};
use kitab_mupdf::{DocId, Engine};
use std::collections::VecDeque;
use std::io::BufRead;
use std::path::{Path, PathBuf};

pub struct Cx {
    /// The command's name as shown in JSON ("pages keep").
    pub command: String,
    pub args: Parsed,
    pub out: Out,
    engine: Option<Engine>,
    /// Stdin's lines, read once on first use: `--password-stdin` takes the
    /// first, `--new-password-stdin` (or `--key-password-stdin`) the next.
    stdin: Option<VecDeque<String>>,
    password: Option<Option<String>>,
}

impl Cx {
    pub fn new(command: &str, args: Parsed) -> Self {
        let out = Out {
            json: args.has("json"),
            quiet: args.has("quiet"),
        };
        Self {
            command: command.to_string(),
            args,
            out,
            engine: None,
            stdin: None,
            password: None,
        }
    }

    pub fn force(&self) -> bool {
        self.args.has("force")
    }

    /// Report success (see [`Out::done`]).
    pub fn done(&self, fields: serde_json::Value, human: &str) {
        self.out.done(&self.command, fields, human);
    }

    /// One engine per run; one worker is plenty for a single command.
    pub fn engine(&mut self) -> &Engine {
        self.engine.get_or_insert_with(|| Engine::new(1))
    }

    /// The next line of stdin, without its line ending.
    pub fn stdin_line(&mut self, what: &str) -> Result<String> {
        let lines = self.stdin.get_or_insert_with(|| {
            std::io::stdin()
                .lock()
                .lines()
                .map_while(std::result::Result::ok)
                .collect()
        });
        lines
            .pop_front()
            .map(|l| l.trim_end_matches(['\r', '\n']).to_string())
            .ok_or_else(|| CliError::usage(format!("expected {what} on stdin, but it was empty")))
    }

    /// A secret from `--NAME-env VAR` or `--NAME-stdin` (never a plain argument).
    pub fn secret(&mut self, name: &str, what: &str) -> Result<Option<String>> {
        let env_flag = format!("{name}-env");
        let stdin_flag = format!("{name}-stdin");
        if let Some(var) = self.args.value(&env_flag).map(str::to_string) {
            return std::env::var(&var).map(Some).map_err(|_| {
                CliError::usage(format!(
                    "--{env_flag}: environment variable {var} is not set"
                ))
            });
        }
        if self.args.has(&stdin_flag) {
            return self.stdin_line(what).map(Some);
        }
        Ok(None)
    }

    /// The password for encrypted inputs: --password, --password-env or
    /// --password-stdin. Read once.
    pub fn password(&mut self) -> Result<Option<String>> {
        if let Some(p) = &self.password {
            return Ok(p.clone());
        }
        let p = match self.args.value("password") {
            Some(pw) => Some(pw.to_string()),
            None => self.secret("password", "the password")?,
        };
        self.password = Some(p.clone());
        Ok(p)
    }

    /// The `index`th positional argument, or a usage error naming it.
    pub fn arg(&self, index: usize, name: &str) -> Result<String> {
        self.args.positional.get(index).cloned().ok_or_else(|| {
            CliError::usage(format!(
                "missing {name} (see: kitab help {})",
                self.command.split(' ').next().unwrap_or_default()
            ))
        })
    }

    /// No positional arguments past `count`.
    pub fn no_more_args(&self, count: usize) -> Result<()> {
        match self.args.positional.get(count) {
            Some(extra) => Err(CliError::usage(format!(
                "unexpected argument '{extra}' (see: kitab help {})",
                self.command.split(' ').next().unwrap_or_default()
            ))),
            None => Ok(()),
        }
    }

    /// Check an input path exists and is a file (exit 3 otherwise).
    pub fn input_path(&self, path: &str) -> Result<PathBuf> {
        let p = PathBuf::from(path);
        if !p.exists() {
            return Err(CliError::not_found(format!("{path}: no such file")));
        }
        if p.is_dir() {
            return Err(CliError::unreadable(format!(
                "{path} is a folder, not a document"
            )));
        }
        if let Err(e) = std::fs::File::open(&p) {
            return Err(CliError::unreadable(format!("{path}: can't read it: {e}")));
        }
        Ok(p)
    }

    /// Open a document with the engine, sorting failures into exit codes.
    pub fn open(&mut self, path: &Path) -> Result<(DocId, DocInfo)> {
        let password = self.password()?;
        let shown = path.display().to_string();
        self.engine()
            .open(path, password.as_deref())
            .map_err(|e| open_error(&shown, e, password.is_some()))
    }

    /// Open `path`, which must be a PDF, and return its page count. The PDF
    /// tools open the file themselves; this first look gives clear errors
    /// (not found, needs a password, not a PDF) and the count for ranges.
    pub fn open_pdf(&mut self, path: &Path) -> Result<u32> {
        let (id, info) = self.open(path)?;
        self.engine().close(id);
        if info.format != Format::Pdf {
            return Err(CliError::failed(format!(
                "{} is {}, but this command works on PDF files only",
                path.display(),
                article(format_name(info.format))
            )));
        }
        Ok(info.page_count)
    }

    /// The `-o` path, required.
    pub fn out_path(&self) -> Result<PathBuf> {
        self.args.value("out").map(PathBuf::from).ok_or_else(|| {
            CliError::usage(format!(
                "missing -o OUT: where to write the result (see: kitab help {})",
                self.command.split(' ').next().unwrap_or_default()
            ))
        })
    }

    /// Check `out` may be written: not one of `inputs` (even with --force),
    /// not an existing file unless --force. Creates its folder if missing.
    pub fn check_out(&self, out: &Path, inputs: &[&Path]) -> Result<()> {
        if inputs.iter().any(|i| same_file(i, out)) {
            return Err(CliError::usage(format!(
                "{} is an input; kitab never overwrites its inputs, choose another -o",
                out.display()
            )));
        }
        if out.is_dir() {
            return Err(CliError::usage(format!(
                "{} is a folder; give a file name with -o",
                out.display()
            )));
        }
        if out.exists() && !self.force() {
            return Err(CliError::exists(format!(
                "{} already exists (add --force to overwrite it)",
                out.display()
            )));
        }
        make_parent(out)
    }

    /// Check a folder output: created if missing; if it holds files, only
    /// with --force.
    pub fn check_out_dir(&self, dir: &Path) -> Result<()> {
        if dir.is_file() {
            return Err(CliError::usage(format!(
                "{} is a file; give a folder with -o",
                dir.display()
            )));
        }
        let busy = std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some());
        if busy && !self.force() {
            return Err(CliError::exists(format!(
                "{} is not empty (add --force to write into it)",
                dir.display()
            )));
        }
        std::fs::create_dir_all(dir)
            .map_err(|e| CliError::failed(format!("could not create {}: {e}", dir.display())))
    }
}

pub fn make_parent(out: &Path) -> Result<()> {
    match out.parent() {
        Some(parent) if !parent.as_os_str().is_empty() && !parent.exists() => {
            std::fs::create_dir_all(parent).map_err(|e| {
                CliError::failed(format!("could not create {}: {e}", parent.display()))
            })
        }
        _ => Ok(()),
    }
}

pub fn open_error(path: &str, e: OpenError, had_password: bool) -> CliError {
    match e {
        OpenError::NotFound => CliError::not_found(format!("{path}: no such file")),
        OpenError::NeedsPassword if had_password => wrong_password(path),
        OpenError::NeedsPassword => CliError::new(
            crate::out::EXIT_PASSWORD,
            "needs_password",
            format!("{path} needs a password: pass it with --password-env VAR or --password-stdin"),
        ),
        OpenError::WrongPassword => wrong_password(path),
        OpenError::Unsupported => CliError::unreadable(format!(
            "{path} isn't a document Kitab can read (PDF, EPUB, MOBI, FB2, XPS, CBZ, CBR, DjVu...)"
        )),
        OpenError::Failed(m) => CliError::unreadable(format!("{path} could not be opened: {m}")),
    }
}

fn wrong_password(path: &str) -> CliError {
    CliError::new(
        crate::out::EXIT_PASSWORD,
        "wrong_password",
        format!("the password for {path} is wrong"),
    )
}

/// Whether two paths name the same file (also when one doesn't exist yet).
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

pub fn format_name(f: Format) -> &'static str {
    match f {
        Format::Pdf => "pdf",
        Format::Xps => "xps",
        Format::Epub => "epub",
        Format::Mobi => "mobi",
        Format::Fb2 => "fb2",
        Format::Cbz => "cbz",
        Format::Cbr => "cbr",
        Format::Cb7 => "cb7",
        Format::Djvu => "djvu",
        Format::Image => "image",
        Format::Txt => "txt",
        Format::Markdown => "markdown",
    }
}

fn article(name: &str) -> String {
    let upper = name.to_ascii_uppercase();
    if upper.starts_with(['A', 'E', 'I', 'O', 'U', 'F', 'X', 'M']) {
        format!("an {upper} file")
    } else {
        format!("a {upper} file")
    }
}

/// Page ranges ("1-3,7,10-") for a document of `count` pages, 0-based.
pub fn ranges(text: &str, count: u32) -> Result<Vec<u32>> {
    kitab_core::parse_page_ranges(text, count).map_err(|e| CliError::failed(format!("pages: {e}")))
}

/// A 1-based page number, checked against `count`; returns it 0-based.
pub fn page_number(text: &str, count: u32, flag: &str) -> Result<u32> {
    let n: u32 = text
        .trim()
        .parse()
        .map_err(|_| CliError::usage(format!("{flag}: \"{text}\" isn't a page number")))?;
    if n == 0 || n > count {
        return Err(CliError::failed(format!(
            "{flag}: page {n} is outside 1–{count}"
        )));
    }
    Ok(n - 1)
}

/// A place to put pages: before page N (1-based; count + 1 or "end" is
/// after the last). Returns it 0-based.
pub fn before_page(text: &str, count: u32) -> Result<u32> {
    if text.eq_ignore_ascii_case("end") {
        return Ok(count);
    }
    page_number(text, count.saturating_add(1), "--before")
}

/// A number list such as "72,600" or "72,600,250,30".
pub fn numbers(text: &str, want: usize, flag: &str, shape: &str) -> Result<Vec<f32>> {
    let v: Vec<f32> = text
        .split(',')
        .map(|s| s.trim().parse::<f32>())
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| CliError::usage(format!("{flag} takes {shape}, e.g. {}", example(want))))?;
    if v.len() != want || v.iter().any(|n| !n.is_finite()) {
        return Err(CliError::usage(format!(
            "{flag} takes {shape}, e.g. {}",
            example(want)
        )));
    }
    Ok(v)
}

fn example(n: usize) -> &'static str {
    if n == 2 { "72,600" } else { "72,600,250,30" }
}

/// Write `data` to `out` through a temporary file beside it.
pub fn write_atomic(out: &Path, data: &[u8]) -> Result<()> {
    let mut name = out.file_name().unwrap_or_default().to_os_string();
    name.push(".kitab-tmp");
    let tmp = out.with_file_name(name);
    std::fs::write(&tmp, data)
        .map_err(|e| CliError::failed(format!("could not write {}: {e}", out.display())))?;
    std::fs::rename(&tmp, out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        CliError::failed(format!("could not save {}: {e}", out.display()))
    })
}

/// A path for JSON and messages, with forward slashes so it reads the same
/// on every system.
pub fn shown(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}
