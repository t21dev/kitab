// SPDX-License-Identifier: AGPL-3.0-or-later

//! GitHub #9: going on past the last page offers the next file in the same
//! folder (name order, numbers counted: "Vol 2" before "Vol 10"), and going
//! on once more opens it in the same tab. Settings › Reading turns it off.

use super::Workspace;
use crate::reader::Reader;
use crate::style::Style;
use gpui::{AppContext, Context, Entity, EntityId, Window};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The offer stands this long.
const OFFER_FOR: Duration = Duration::from_secs(8);
/// A held key repeats; only a press after a pause this long takes the offer.
const PAUSE: Duration = Duration::from_millis(250);

pub(super) struct Offer {
    reader: EntityId,
    /// `None`: the folder had no next book (said once, not again).
    next: Option<PathBuf>,
    first: Instant,
    last: Instant,
    /// The toast that made the offer, closed when the book opens.
    toast: u64,
}

impl Workspace {
    pub(super) fn past_end(
        &mut self,
        reader: Entity<Reader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !Style::get(cx).next_book {
            return;
        }
        let id = reader.entity_id();
        let now = Instant::now();
        if let Some(offer) = self.next_book.as_mut()
            && offer.reader == id
            && now.duration_since(offer.first) < OFFER_FOR
        {
            let paused = now.duration_since(offer.last) >= PAUSE;
            offer.last = now;
            if paused && let Some(next) = offer.next.clone() {
                let toast = offer.toast;
                self.next_book = None;
                self.dismiss_toast(toast, cx);
                let presenting = reader.read(cx).presenting();
                let ix = self.tabs.iter().position(|t| *t == reader);
                self.insert_tab(next.clone(), ix.map(|i| i + 1), true, window, cx);
                // Presenting goes on into the next book, still full screen.
                if presenting && let Some(new) = self.tabs.iter().find(|t| t.read(cx).path == next)
                {
                    new.update(cx, |r, cx| r.present_when_open(cx));
                    self.present_carried = true;
                }
                if let Some(ix) = ix {
                    self.close_tab(ix, window, cx);
                }
            }
            return;
        }
        // Asked for now; a second press while the folder is read finds the
        // offer when it comes.
        self.next_book = Some(Offer {
            reader: id,
            next: None,
            first: now,
            last: now,
            toast: 0,
        });
        let current = reader.read(cx).path.clone();
        cx.spawn(async move |this, cx| {
            let next = cx
                .background_spawn(async move { next_in_folder(&current) })
                .await;
            let _ = this.update(cx, |ws, cx| {
                let Some(offer) = ws.next_book.as_mut().filter(|o| o.reader == id) else {
                    return;
                };
                offer.next = next.clone();
                // The offer runs from when it shows.
                offer.first = Instant::now();
                let text = match &next {
                    Some(p) => format!("End of the book. Go on again to open {}.", file_name(p)),
                    None => "That was the last book in this folder.".to_string(),
                };
                ws.toast(text, false, None, cx);
                let serial = ws.tools.toast_serial;
                if let Some(offer) = ws.next_book.as_mut() {
                    offer.toast = serial;
                }
            });
        })
        .detach();
    }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Whether Kitab would open this file as a book from a folder: a format it
/// reads; plain archives only next to another plain archive.
fn readable(path: &Path, archives: bool) -> bool {
    let Some(ext) = path.extension().map(|e| e.to_string_lossy()) else {
        return false;
    };
    if kitab_core::Format::is_plain_archive(&ext) && !archives {
        return false;
    }
    kitab_core::Format::from_extension(&ext).is_some_and(|f| f != kitab_core::Format::Image)
}

/// The file after `current` in its folder, in name order.
pub(crate) fn next_in_folder(current: &Path) -> Option<PathBuf> {
    let dir = current.parent()?;
    let archives = current
        .extension()
        .is_some_and(|e| kitab_core::Format::is_plain_archive(&e.to_string_lossy()));
    let name = current.file_name()?.to_string_lossy().into_owned();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| !file_name(p).starts_with('.') && readable(p, archives))
        .filter(|p| natural_cmp(&file_name(p), &name) == Ordering::Greater)
        .min_by(|a, b| natural_cmp(&file_name(a), &file_name(b)))
}

/// File names in the order people expect: case aside, and runs of digits
/// by their value ("2" before "10").
pub(crate) fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s
                };
                let (x, y) = (take(&mut a), take(&mut b));
                let (tx, ty) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let o = tx.len().cmp(&ty.len()).then_with(|| tx.cmp(ty));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                a.next();
                b.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_sort_by_their_numbers() {
        assert_eq!(natural_cmp("Vol 2.cbz", "Vol 10.cbz"), Ordering::Less);
        assert_eq!(natural_cmp("vol 10", "Vol 9"), Ordering::Greater);
        assert_eq!(natural_cmp("a", "B"), Ordering::Less);
        assert_eq!(natural_cmp("ch01", "ch1"), Ordering::Equal);
    }

    #[test]
    fn the_next_book_is_the_next_readable_file() {
        let dir = std::env::temp_dir().join(format!("kitab-next-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in [
            "Vol 1.cbz",
            "Vol 2.cbz",
            "Vol 10.cbz",
            "Vol 3.jpg",
            "Vol 4.zip",
            ".Vol 5.pdf",
            "notes.docx",
        ] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let next = |f: &str| next_in_folder(&dir.join(f)).map(|p| file_name(&p));
        assert_eq!(next("Vol 1.cbz").as_deref(), Some("Vol 2.cbz"));
        // Pictures, plain archives, hidden and unread files are skipped.
        assert_eq!(next("Vol 2.cbz").as_deref(), Some("Vol 10.cbz"));
        assert_eq!(next("Vol 10.cbz"), None);
        // From a plain archive, archives count.
        std::fs::write(dir.join("Vol 0.zip"), b"x").unwrap();
        assert_eq!(next("Vol 0.zip").as_deref(), Some("Vol 1.cbz"));
        assert_eq!(next("Vol 3.zip").as_deref(), Some("Vol 4.zip"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
