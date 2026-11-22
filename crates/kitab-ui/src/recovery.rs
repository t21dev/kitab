// SPDX-License-Identifier: AGPL-3.0-or-later

//! Unsaved work survives a crash.
//!
//! Marks are kept in the library as they are drawn, so they are safe
//! already. What lived only in memory until Save was a PDF's filled-in form
//! fields, the PDF's own annotations taken out, and Markdown being edited.
//! The windows publish that here every few seconds ([`publish`]); it is
//! written to `recovery/s<session>/unsaved.json` in Kitab's data folder when
//! it changes, and at once by the panic hook ([`install_panic_hook`]), so a
//! panic loses at most the last few seconds. A clean quit removes the file
//! ([`clear`]); a file found at start-up is a crash's, and Kitab offers to
//! restore it.
//!
//! Each Windows session has its own folder: one user signed in at the
//! screen and over Remote Desktop runs two Kitabs (the single-instance pipe
//! is per session too).

use kitab_core::{FieldEdit, FieldValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One document's unsaved work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unsaved {
    pub path: PathBuf,
    /// The Markdown being edited, when it differs from the file.
    #[serde(default)]
    pub markdown: Option<String>,
    /// Form fields filled in, oldest first.
    #[serde(default)]
    pub fields: Vec<Field>,
    /// The PDF's own annotations taken out: (page, index).
    #[serde(default)]
    pub deleted: Vec<(u32, u32)>,
}

impl Unsaved {
    pub fn is_empty(&self) -> bool {
        self.markdown.is_none() && self.fields.is_empty() && self.deleted.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub page: u32,
    pub index: u32,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Text(String),
    Checked(bool),
    Choice(String),
}

impl From<&FieldEdit> for Field {
    fn from(e: &FieldEdit) -> Self {
        Field {
            page: e.page,
            index: e.index,
            value: match &e.value {
                FieldValue::Text(t) => Value::Text(t.clone()),
                FieldValue::Checked(c) => Value::Checked(*c),
                FieldValue::Choice(c) => Value::Choice(c.clone()),
            },
        }
    }
}

impl From<&Field> for FieldEdit {
    fn from(f: &Field) -> Self {
        FieldEdit {
            page: f.page,
            index: f.index,
            value: match &f.value {
                Value::Text(t) => FieldValue::Text(t.clone()),
                Value::Checked(c) => FieldValue::Checked(*c),
                Value::Choice(c) => FieldValue::Choice(c.clone()),
            },
        }
    }
}

/// What each window last published, by window.
static LATEST: Mutex<BTreeMap<u64, Vec<Unsaved>>> = Mutex::new(BTreeMap::new());

const FILE: &str = "unsaved.json";
/// A crash's work, moved aside at start-up (so this session's own writes
/// don't replace it) until the user restores it or lets it go.
const LEFTOVER: &str = "leftover.json";
/// The last panic's message and place, for a bug report, in the data
/// folder's `crash` folder (PRIVACY.md). It goes nowhere by itself.
const PANIC_NOTE: &str = "last-panic.txt";
/// Leftovers larger than this aren't read (a damaged file).
const MAX_FILE: u64 = 64 * 1024 * 1024;

fn dir() -> Option<PathBuf> {
    Some(
        kitab_os::data_dir()?
            .join("recovery")
            .join(format!("s{}", kitab_os::session_id())),
    )
}

/// Window `window`'s unsaved work now. Returns whether it changed (then
/// call [`write`], off the UI thread).
pub fn publish(window: u64, work: Vec<Unsaved>) -> bool {
    let mut latest = LATEST.lock().unwrap_or_else(|e| e.into_inner());
    let work: Vec<Unsaved> = work.into_iter().filter(|u| !u.is_empty()).collect();
    if latest.get(&window).map_or(work.is_empty(), |w| *w == work) {
        return false;
    }
    if work.is_empty() {
        latest.remove(&window);
    } else {
        latest.insert(window, work);
    }
    true
}

/// Write what was published (or remove the file when there's nothing).
pub fn write() {
    // A panic may come while the lock is held: never wait for it there.
    let all: Vec<Unsaved> = match LATEST.try_lock() {
        Ok(l) => l.values().flatten().cloned().collect(),
        Err(std::sync::TryLockError::Poisoned(p)) => {
            p.into_inner().values().flatten().cloned().collect()
        }
        Err(std::sync::TryLockError::WouldBlock) => return,
    };
    let Some(dir) = dir() else { return };
    let _ = write_to(&dir, &all);
}

fn write_to(dir: &Path, all: &[Unsaved]) -> std::io::Result<()> {
    let file = dir.join(FILE);
    if all.is_empty() {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(dir)?;
    let json = serde_json::to_vec(all).map_err(std::io::Error::other)?;
    // Written beside it, then swapped in: never half a file.
    let tmp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &file)
}

/// A window closed cleanly (its work saved or let go on purpose).
pub fn forget(window: u64) {
    if publish(window, Vec::new()) {
        write();
    }
}

/// The work a crash left behind, if any (call once, at start-up, before
/// this session writes its own).
pub fn take_leftovers() -> Vec<Unsaved> {
    dir().map(|d| take_from(&d)).unwrap_or_default()
}

fn take_from(dir: &Path) -> Vec<Unsaved> {
    // An earlier start-up's leftovers not yet answered, and the last crash's.
    let mut all = read_file(&dir.join(LEFTOVER));
    for u in read_file(&dir.join(FILE)) {
        if !all.contains(&u) {
            all.push(u);
        }
    }
    let _ = std::fs::remove_file(dir.join(FILE));
    if !all.is_empty()
        && let Ok(json) = serde_json::to_vec(&all)
    {
        let _ = std::fs::write(dir.join(LEFTOVER), json);
    }
    all
}

#[cfg(test)]
fn read_from(dir: &Path) -> Vec<Unsaved> {
    read_file(&dir.join(FILE))
}

fn read_file(file: &Path) -> Vec<Unsaved> {
    if std::fs::metadata(file).map_or(true, |m| m.len() > MAX_FILE) {
        return Vec::new();
    }
    std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<Unsaved>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|u| !u.is_empty())
        .collect()
}

/// Remove the leftovers (restored, or the user let them go).
pub fn clear_leftovers() {
    if let Some(d) = dir() {
        let _ = std::fs::remove_file(d.join(LEFTOVER));
    }
}

/// The most the panic note keeps: the first panics of a run, which are the
/// cause; a crash's later ones only follow from them.
const PANIC_NOTE_MAX: usize = 64 << 10;

/// A panic noted, with where it happened and how it got there.
fn panic_note(info: &std::panic::PanicHookInfo<'_>) -> String {
    let thread = std::thread::current();
    format!(
        "Kitab {} panicked on thread '{}': {info}\n{}\n",
        env!("CARGO_PKG_VERSION"),
        thread.name().unwrap_or("unnamed"),
        std::backtrace::Backtrace::force_capture()
    )
}

/// Add `note` to the panic note: the first of this run starts it afresh, the
/// rest follow it. A panic while another unwinds ends in "panic in a
/// destructor during cleanup", which says nothing; the first one says what
/// went wrong (GitHub #19 reported only the second).
fn add_panic_note(path: &Path, note: &str, first: bool) {
    use std::io::Write;
    let file = if first {
        std::fs::File::create(path)
    } else {
        if std::fs::metadata(path).is_ok_and(|m| m.len() as usize >= PANIC_NOTE_MAX) {
            return;
        }
        std::fs::OpenOptions::new().append(true).open(path)
    };
    if let Ok(mut f) = file {
        let _ = f.write_all(note.as_bytes());
    }
}

/// On any panic, write the unsaved work at once, then note the panic, then
/// carry on as before (the default report, then unwinding).
pub fn install_panic_hook() {
    static NOTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write();
        if let Some(d) = kitab_os::data_dir().map(|d| d.join("crash")) {
            let first = !NOTED.swap(true, std::sync::atomic::Ordering::SeqCst);
            let _ = std::fs::create_dir_all(&d);
            add_panic_note(&d.join(PANIC_NOTE), &panic_note(info), first);
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(path: &str) -> Unsaved {
        Unsaved {
            path: path.into(),
            markdown: Some("# Draft".into()),
            fields: vec![Field {
                page: 0,
                index: 2,
                value: Value::Checked(true),
            }],
            deleted: vec![(1, 0)],
        }
    }

    /// GitHub #19: the panic that started a crash isn't overwritten by the
    /// ones that follow it; the next run starts the note afresh.
    #[test]
    fn the_first_panic_of_a_run_is_kept() {
        let dir = std::env::temp_dir().join(format!("kitab-panic-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let note = dir.join(PANIC_NOTE);
        add_panic_note(&note, "cause\n", true);
        add_panic_note(&note, "panic in a destructor during cleanup\n", false);
        assert_eq!(
            std::fs::read_to_string(&note).unwrap(),
            "cause\npanic in a destructor during cleanup\n"
        );
        add_panic_note(&note, "next run\n", true);
        assert_eq!(std::fs::read_to_string(&note).unwrap(), "next run\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn work_goes_to_disk_and_comes_back() {
        let dir = std::env::temp_dir().join(format!("kitab-recovery-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let all = vec![work("a.md"), work("b.pdf")];
        write_to(&dir, &all).unwrap();
        assert_eq!(read_from(&dir), all);
        // Nothing left: the file goes.
        write_to(&dir, &[]).unwrap();
        assert!(read_from(&dir).is_empty());
        assert!(!dir.join(FILE).exists());
        // A damaged file is no leftovers, not an error.
        std::fs::write(dir.join(FILE), b"{not json").unwrap();
        assert!(read_from(&dir).is_empty());
        // A crash's work is moved aside at start-up and kept until answered.
        write_to(&dir, &all).unwrap();
        assert_eq!(take_from(&dir), all);
        assert!(!dir.join(FILE).exists());
        assert_eq!(take_from(&dir), all, "still there until restored or let go");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_changes_count() {
        assert!(publish(901, vec![work("a.md")]));
        assert!(!publish(901, vec![work("a.md")]));
        assert!(publish(901, Vec::new()));
        assert!(!publish(
            901,
            vec![Unsaved {
                path: "c.pdf".into(),
                markdown: None,
                fields: Vec::new(),
                deleted: Vec::new(),
            }]
        ));
    }

    #[test]
    fn fields_round_trip() {
        let e = FieldEdit {
            page: 3,
            index: 1,
            value: FieldValue::Choice("Dhaka".into()),
        };
        assert_eq!(FieldEdit::from(&Field::from(&e)), e);
    }
}
