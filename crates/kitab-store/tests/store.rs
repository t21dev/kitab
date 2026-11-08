// SPDX-License-Identifier: AGPL-3.0-or-later

//! Behaviour tests for the store: the library, moved files, reading state,
//! recents, sessions, closed tabs and bookmarks.

// Test helpers outside #[test] fns panic on purpose: a failed setup is a failed test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use kitab_core::{Format, Locator, ReadingState, ZoomMode};
use kitab_store::{
    DocMeta, DocumentId, FileIdentity, MAX_CLOSED_TABS, MAX_SESSIONS, Store, TabRecord, identify,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/pdf")
        .join(name)
}

fn pdf_meta(title: &str) -> DocMeta {
    DocMeta {
        format: Format::Pdf,
        title: Some(title.to_string()),
        author: Some("Kitab".to_string()),
        pages: Some(3),
    }
}

fn state(page: u32, zoom: ZoomMode) -> ReadingState {
    ReadingState {
        locator: Locator::Fixed {
            page,
            x: 0.0,
            y: 123.5,
        },
        zoom,
        rotation: 0,
        progress: 0.5,
    }
}

/// Add a document at `path` (which must exist) and open it.
fn add(store: &mut Store, path: &Path, title: &str) -> DocumentId {
    let ident = identify(path).unwrap();
    let id = store
        .upsert_document(&ident, path, &pdf_meta(title))
        .unwrap();
    store.mark_opened(id).unwrap();
    id
}

/// A synthetic identity for documents that do not exist on disk.
fn fake_ident(n: u64) -> FileIdentity {
    FileIdentity {
        file_key: Some(format!("u:1:{n:x}")),
        quick_hash: n.wrapping_mul(0x9e37_79b9_7f4a_7c15),
        size: 1000 + n,
        mtime_ms: Some(0),
    }
}

#[test]
fn rename_keeps_reading_state() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("book.pdf");
    std::fs::copy(fixture("01-text-outline.pdf"), &original).unwrap();

    let mut store = Store::open(&dir.path().join("kitab.db")).unwrap();
    let id = add(&mut store, &original, "Book");
    let saved = state(4, ZoomMode::FitWidth);
    store.save_reading_state(id, &saved).unwrap();
    store
        .add_bookmark(id, &saved.locator, "Where I was")
        .unwrap();

    let renamed = dir.path().join("sub").join("renamed book.pdf");
    std::fs::create_dir_all(renamed.parent().unwrap()).unwrap();
    std::fs::rename(&original, &renamed).unwrap();

    let ident = identify(&renamed).unwrap();
    let again = store
        .upsert_document(&ident, &renamed, &DocMeta::new(Format::Pdf))
        .unwrap();
    assert_eq!(again, id, "renamed file is the same document");
    assert_eq!(store.reading_state(again).unwrap(), Some(saved));
    assert_eq!(store.bookmarks(again).unwrap().len(), 1);

    let doc = store.document(id).unwrap().unwrap();
    assert_eq!(doc.path, renamed);
    assert_eq!(
        doc.title.as_deref(),
        Some("Book"),
        "absent metadata keeps the old title"
    );
    assert!(store.document_by_path(&original).unwrap().is_none());
}

#[test]
fn move_across_volumes_is_found_by_quick_hash() {
    // A cross-volume move changes the file id; the quick hash still matches.
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("a.pdf");
    std::fs::copy(fixture("01-text-outline.pdf"), &original).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    let id = add(&mut store, &original, "A");
    store
        .save_reading_state(id, &state(2, ZoomMode::FitPage))
        .unwrap();

    let moved = dir.path().join("b.pdf");
    std::fs::copy(&original, &moved).unwrap(); // new file id, same bytes
    std::fs::remove_file(&original).unwrap();

    let ident = identify(&moved).unwrap();
    let again = store
        .upsert_document(&ident, &moved, &DocMeta::new(Format::Pdf))
        .unwrap();
    assert_eq!(again, id);
    assert_eq!(
        store.reading_state(id).unwrap(),
        Some(state(2, ZoomMode::FitPage))
    );
}

#[test]
fn a_copy_of_an_existing_file_is_a_new_document() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("a.pdf");
    std::fs::copy(fixture("01-text-outline.pdf"), &original).unwrap();
    let copy = dir.path().join("copy.pdf");
    std::fs::copy(&original, &copy).unwrap();

    let mut store = Store::open_in_memory().unwrap();
    let a = add(&mut store, &original, "A");
    let b = add(&mut store, &copy, "A copy");
    assert_ne!(a, b);
}

#[test]
fn reopening_the_same_path_updates_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.pdf");
    std::fs::copy(fixture("01-text-outline.pdf"), &path).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    let a = add(&mut store, &path, "A");
    // Edited in place: different bytes, same path.
    std::fs::write(&path, b"%PDF-1.7 edited").unwrap();
    let b = add(&mut store, &path, "A, edited");
    assert_eq!(a, b);
    let doc = store.document(a).unwrap().unwrap();
    assert_eq!(doc.title.as_deref(), Some("A, edited"));
    assert_eq!(doc.size_bytes, 15);
}

#[test]
fn missing_file_is_marked_missing_and_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gone.pdf");
    std::fs::copy(fixture("01-text-outline.pdf"), &path).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    let id = add(&mut store, &path, "Gone");
    assert!(store.refresh_missing(id).unwrap());

    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(!store.refresh_missing(id).unwrap());
    let recent = store.recent(10).unwrap();
    assert!(recent[0].missing, "missing files stay in Recent, flagged");
    assert!(store.document(id).unwrap().unwrap().missing);

    // "Locateâ€¦": the file comes back and is upserted again.
    std::fs::write(&path, bytes).unwrap();
    let again = add(&mut store, &path, "Gone");
    assert_eq!(again, id);
    assert!(!store.document(id).unwrap().unwrap().missing);

    store.mark_missing(id).unwrap();
    assert!(store.document(id).unwrap().unwrap().missing);
}

#[test]
fn recent_is_newest_first_with_progress() {
    let mut store = Store::open_in_memory().unwrap();
    let ids: Vec<DocumentId> = (0..5)
        .map(|n| {
            let path = PathBuf::from(format!("/library/{n}.pdf"));
            store
                .upsert_document(&fake_ident(n), &path, &pdf_meta(&format!("Book {n}")))
                .unwrap()
        })
        .collect();
    // Never opened: not in Recent.
    assert!(store.recent(10).unwrap().is_empty());

    for id in &ids {
        store.mark_opened(*id).unwrap();
    }
    store.mark_opened(ids[1]).unwrap(); // reopened: back on top
    store
        .save_reading_state(ids[1], &state(1, ZoomMode::FitPage))
        .unwrap();

    let recent = store.recent(3).unwrap();
    let order: Vec<DocumentId> = recent.iter().map(|r| r.id).collect();
    assert_eq!(order, vec![ids[1], ids[4], ids[3]]);
    assert_eq!(recent[0].progress, Some(0.5));
    assert_eq!(recent[1].progress, None);
    assert_eq!(recent[0].title.as_deref(), Some("Book 1"));
    assert_eq!(recent[0].format, Format::Pdf);
    assert_eq!(recent[0].path, Path::new("/library/1.pdf"));
    assert!(recent[0].last_opened_at > recent[1].last_opened_at);

    store.forget_document(ids[1]).unwrap();
    assert_eq!(store.recent(1).unwrap()[0].id, ids[4]);
    assert!(store.reading_state(ids[1]).unwrap().is_none());
}

#[test]
fn reading_state_round_trips_every_zoom_mode() {
    let mut store = Store::open_in_memory().unwrap();
    let id = store
        .upsert_document(&fake_ident(1), Path::new("/a.pdf"), &pdf_meta("A"))
        .unwrap();
    assert!(store.reading_state(id).unwrap().is_none());
    for (i, zoom) in [
        ZoomMode::FitPage,
        ZoomMode::FitWidth,
        ZoomMode::ActualSize,
        ZoomMode::Custom(1.25),
        ZoomMode::Custom(0.333_333_34),
        ZoomMode::Custom(64.0),
    ]
    .into_iter()
    .enumerate()
    {
        let s = ReadingState {
            locator: Locator::Fixed {
                page: i as u32 * 100,
                x: 1.0 / 3.0,
                y: 842.000_06,
            },
            zoom,
            rotation: [0, 90, 180, 270][i % 4],
            progress: 0.1 + 0.2,
        };
        store.save_reading_state(id, &s).unwrap();
        assert_eq!(store.reading_state(id).unwrap(), Some(s), "{zoom:?}");
    }
}

#[test]
fn hostile_reading_state_values_are_sanitised() {
    let mut store = Store::open_in_memory().unwrap();
    let id = store
        .upsert_document(&fake_ident(1), Path::new("/a.pdf"), &pdf_meta("A"))
        .unwrap();
    let s = ReadingState {
        locator: Locator::Fixed {
            page: u32::MAX,
            x: f32::NAN,
            y: f32::NEG_INFINITY,
        },
        zoom: ZoomMode::Custom(-3.0),
        rotation: 0,
        progress: 7.0,
    };
    store.save_reading_state(id, &s).unwrap();
    let back = store.reading_state(id).unwrap().unwrap();
    assert_eq!(
        back.locator,
        Locator::Fixed {
            page: u32::MAX,
            x: 0.0,
            y: 0.0
        }
    );
    assert_eq!(back.zoom, ZoomMode::Custom(1.0));
    assert_eq!(back.progress, 1.0);
}

#[test]
fn oversized_metadata_is_clipped() {
    let mut store = Store::open_in_memory().unwrap();
    let long = "x".repeat(100_000);
    let meta = DocMeta {
        format: Format::Pdf,
        title: Some(long.clone()),
        author: Some("   ".into()),
        pages: Some(1),
    };
    let id = store
        .upsert_document(&fake_ident(1), Path::new("/a.pdf"), &meta)
        .unwrap();
    let doc = store.document(id).unwrap().unwrap();
    assert_eq!(
        doc.title.map(|t| t.chars().count()),
        Some(kitab_store::MAX_META_CHARS)
    );
    assert_eq!(doc.author, None);
}

#[test]
fn session_round_trip_clean_and_crashed() {
    let mut store = Store::open_in_memory().unwrap();
    let a = store
        .upsert_document(&fake_ident(1), Path::new("/a.pdf"), &pdf_meta("A"))
        .unwrap();
    let b = store
        .upsert_document(&fake_ident(2), Path::new("/b.pdf"), &pdf_meta("B"))
        .unwrap();
    assert!(store.last_session().unwrap().is_none());

    // First run: two tabs, quit normally.
    let s1 = store.begin_session().unwrap();
    let tabs = [
        TabRecord {
            document_id: a,
            pinned: true,
            active: false,
            scroll: None,
        },
        TabRecord {
            document_id: b,
            pinned: false,
            active: true,
            scroll: Some(state(9, ZoomMode::Custom(2.0))),
        },
    ];
    store.save_session_tabs(s1, &tabs).unwrap();
    store.end_session_cleanly(s1).unwrap();

    let last = store.last_session().unwrap().unwrap();
    assert_eq!(last.id, s1);
    assert!(last.closed_cleanly);
    let got: Vec<TabRecord> = last.tabs.iter().map(|t| t.tab).collect();
    assert_eq!(got, tabs);
    assert_eq!(last.tabs[0].path, Path::new("/a.pdf"));
    assert_eq!(last.tabs[1].title.as_deref(), Some("B"));
    assert_eq!(last.tabs[1].format, Format::Pdf);

    // Second run: crash after saving one tab (no end_session_cleanly).
    let s2 = store.begin_session().unwrap();
    assert!(s2 > s1);
    store.save_session_tabs(s2, &tabs[1..]).unwrap();
    // Saving again replaces, not appends.
    store.save_session_tabs(s2, &tabs[1..]).unwrap();

    let last = store.last_session().unwrap().unwrap();
    assert_eq!(last.id, s2);
    assert!(!last.closed_cleanly, "a crash leaves the session unclean");
    assert_eq!(last.tabs.len(), 1);
    assert_eq!(last.tabs[0].tab.document_id, b);

    // A session with no tabs still reports itself.
    let s3 = store.begin_session().unwrap();
    let last = store.last_session().unwrap().unwrap();
    assert_eq!(last.id, s3);
    assert!(last.tabs.is_empty());
}

#[test]
fn sessions_are_bounded() {
    let db = tempfile::tempdir().unwrap();
    let path = db.path().join("k.db");
    let mut store = Store::open(&path).unwrap();
    let mut newest = None;
    for _ in 0..(MAX_SESSIONS * 3) {
        newest = Some(store.begin_session().unwrap());
    }
    assert_eq!(store.last_session().unwrap().map(|s| s.id), newest);
    drop(store);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let n: i64 = conn
        .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n as usize, MAX_SESSIONS);
}

#[test]
fn closed_tab_stack_is_lifo_and_bounded() {
    let mut store = Store::open_in_memory().unwrap();
    let a = store
        .upsert_document(&fake_ident(1), Path::new("/a.pdf"), &pdf_meta("A"))
        .unwrap();
    let b = store
        .upsert_document(&fake_ident(2), Path::new("/b.pdf"), &pdf_meta("B"))
        .unwrap();
    assert!(store.pop_closed_tab().unwrap().is_none());

    store.push_closed_tab(a, None).unwrap();
    let s = state(3, ZoomMode::ActualSize);
    store.push_closed_tab(b, Some(&s)).unwrap();

    let top = store.pop_closed_tab().unwrap().unwrap();
    assert_eq!(top.document_id, b);
    assert_eq!(top.scroll, Some(s));
    let next = store.pop_closed_tab().unwrap().unwrap();
    assert_eq!(next.document_id, a);
    assert_eq!(next.scroll, None);
    assert!(store.pop_closed_tab().unwrap().is_none());

    // Bounded.
    for _ in 0..(MAX_CLOSED_TABS + 20) {
        store.push_closed_tab(a, None).unwrap();
    }
    let mut n = 0;
    while store.pop_closed_tab().unwrap().is_some() {
        n += 1;
    }
    assert_eq!(n, MAX_CLOSED_TABS);

    // Forgetting a document drops its closed tabs.
    store.push_closed_tab(b, None).unwrap();
    store.forget_document(b).unwrap();
    assert!(store.pop_closed_tab().unwrap().is_none());
}

#[test]
fn bookmarks_add_list_remove() {
    let mut store = Store::open_in_memory().unwrap();
    let a = store
        .upsert_document(&fake_ident(1), Path::new("/a.pdf"), &pdf_meta("A"))
        .unwrap();
    let loc1 = Locator::Fixed {
        page: 1,
        x: 0.0,
        y: 10.0,
    };
    let loc2 = Locator::Fixed {
        page: 7,
        x: 5.5,
        y: 0.125,
    };
    let m1 = store.add_bookmark(a, &loc1, "Intro").unwrap();
    let m2 = store.add_bookmark(a, &loc2, "Results").unwrap();

    let list = store.bookmarks(a).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(
        (list[0].id, list[0].locator, list[0].title.as_str()),
        (m1, loc1, "Intro")
    );
    assert_eq!((list[1].id, list[1].locator), (m2, loc2));

    assert!(store.remove_bookmark(m1).unwrap());
    assert!(!store.remove_bookmark(m1).unwrap());
    assert_eq!(store.bookmarks(a).unwrap().len(), 1);
}

#[test]
fn non_unicode_paths_are_an_error_not_a_panic() {
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(std::ffi::OsStr::from_bytes(b"/bad\xff.pdf"))
    };
    #[cfg(windows)]
    let path = {
        use std::os::windows::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_wide(&[0x2f, 0xd800, 0x61]))
    };
    let mut store = Store::open_in_memory().unwrap();
    let err = store
        .upsert_document(&fake_ident(1), &path, &pdf_meta("A"))
        .unwrap_err();
    assert!(matches!(err, kitab_store::Error::NonUnicodePath));
}

/// The start-up queries on a big library (PLAN.md §6.3). Prints the timing;
/// the assertion is loose so a slow CI machine does not flake.
#[test]
fn startup_queries_on_10k_documents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.db");
    let mut store = Store::open(&path).unwrap();
    let mut ids = Vec::with_capacity(10_000);
    for n in 0..10_000u64 {
        let p = PathBuf::from(format!("/library/shelf{}/book{n}.pdf", n % 50));
        let id = store
            .upsert_document(&fake_ident(n), &p, &pdf_meta(&format!("Book {n}")))
            .unwrap();
        store.mark_opened(id).unwrap();
        store
            .save_reading_state(id, &state((n % 300) as u32, ZoomMode::FitWidth))
            .unwrap();
        ids.push(id);
    }
    let s = store.begin_session().unwrap();
    let tabs: Vec<TabRecord> = ids[..20]
        .iter()
        .map(|&document_id| TabRecord {
            document_id,
            pinned: false,
            active: false,
            scroll: None,
        })
        .collect();
    store.save_session_tabs(s, &tabs).unwrap();
    drop(store);

    // Cold-ish: a fresh connection, as at start-up.
    let t0 = Instant::now();
    let store = Store::open(&path).unwrap();
    let t_open = t0.elapsed();
    let t1 = Instant::now();
    let last = store.last_session().unwrap().unwrap();
    let t_session = t1.elapsed();
    let t2 = Instant::now();
    let recent = store.recent(50).unwrap();
    let t_recent = t2.elapsed();

    // Warm: what repeated calls cost with cached statements.
    let t3 = Instant::now();
    for _ in 0..100 {
        store.last_session().unwrap();
        store.recent(50).unwrap();
    }
    let warm = t3.elapsed() / 100;

    println!(
        "10k docs: open {t_open:?}, last_session {t_session:?} (20 tabs), \
         recent(50) {t_recent:?}, warm last_session+recent(50) {warm:?}"
    );
    assert_eq!(last.tabs.len(), 20);
    assert_eq!(recent.len(), 50);
    assert_eq!(recent[0].id, ids[9_999]);
    assert!(t_session + t_recent < std::time::Duration::from_millis(50));
}

#[test]
fn marks_are_kept_listed_deleted_and_cleared() {
    use kitab_core::{Mark, MarkShape, PointPt, Rgb8};
    let mut store = Store::open_in_memory().unwrap();
    let doc = store
        .upsert_document(&fake_ident(9), Path::new("/m.pdf"), &pdf_meta("M"))
        .unwrap();
    let line = |x: f32| Mark {
        page: 1,
        shape: MarkShape::Line {
            from: PointPt { x, y: 0.0 },
            to: PointPt { x: 50.0, y: 50.0 },
            arrow: true,
        },
        color: Rgb8 {
            r: 200,
            g: 40,
            b: 40,
        },
        width: 2.0,
        opacity: 1.0,
    };
    let a = store.add_mark(doc, &line(1.0)).unwrap();
    store.add_mark(doc, &line(2.0)).unwrap();
    let mut bad = line(3.0);
    bad.width = f32::NAN;
    assert!(store.add_mark(doc, &bad).is_err());
    assert_eq!(store.marks(doc).unwrap().len(), 2);
    store.delete_mark(a).unwrap();
    let left = store.marks(doc).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].1, line(2.0));
    assert_eq!(store.clear_marks(doc).unwrap(), 1);
    assert!(store.marks(doc).unwrap().is_empty());
}

#[test]
fn clearing_history_keeps_marks_and_bookmarks() {
    use kitab_core::{Mark, MarkShape, PointPt, Rgb8};
    let mut store = Store::open_in_memory().unwrap();
    let doc = store
        .upsert_document(&fake_ident(5), Path::new("/h.pdf"), &pdf_meta("H"))
        .unwrap();
    store.mark_opened(doc).unwrap();
    store
        .save_reading_state(
            doc,
            &kitab_core::ReadingState {
                locator: kitab_core::Locator::Fixed {
                    page: 2,
                    x: 0.0,
                    y: 10.0,
                },
                zoom: kitab_core::ZoomMode::FitWidth,
                rotation: 0,
                progress: 0.5,
            },
        )
        .unwrap();
    let at = kitab_core::Locator::Fixed {
        page: 1,
        x: 0.0,
        y: 0.0,
    };
    store.add_bookmark(doc, &at, "Kept").unwrap();
    let mark = Mark {
        page: 0,
        shape: MarkShape::Note {
            at: PointPt { x: 5.0, y: 5.0 },
            text: "kept".into(),
        },
        color: Rgb8 { r: 1, g: 2, b: 3 },
        width: 1.0,
        opacity: 1.0,
    };
    store.add_mark(doc, &mark).unwrap();
    assert_eq!(store.recent(10).unwrap().len(), 1);

    store.clear_history().unwrap();
    assert!(store.recent(10).unwrap().is_empty());
    assert!(store.reading_state(doc).unwrap().is_none());
    assert_eq!(store.bookmarks(doc).unwrap().len(), 1);
    assert_eq!(store.marks(doc).unwrap().len(), 1);
}

#[test]
fn optimize_reports_sizes_and_keeps_data() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("k.db")).unwrap();
    for i in 0..200u64 {
        let p = format!("/f{i}.pdf");
        let id = store
            .upsert_document(&fake_ident(100 + i), Path::new(&p), &pdf_meta("T"))
            .unwrap();
        if i % 2 == 0 {
            store.forget_document(id).unwrap();
        }
    }
    let (before, after) = store.optimize().unwrap();
    // The planner's statistics table can take a page or two of its own.
    assert!(
        before > 0 && after > 0 && after <= before + 8192,
        "{before} -> {after}"
    );
    let (_, again) = store.optimize().unwrap();
    assert!(again <= after + 4096, "{after} -> {again}");
    assert!(
        store
            .document_by_path(Path::new("/f1.pdf"))
            .unwrap()
            .is_some()
    );
}

#[test]
fn library_search_and_reading_stats() {
    let mut store = Store::open_in_memory().unwrap();
    let id = store
        .upsert_document(
            &fake_ident(9),
            std::path::Path::new("/books/lease.pdf"),
            &DocMeta::new(Format::Pdf),
        )
        .unwrap();
    assert_eq!(store.text_indexed(id).unwrap(), None);
    store
        .index_text(
            id,
            1234,
            &[
                "Chapter one".into(),
                "".into(),
                "The rent is 25000 taka".into(),
            ],
        )
        .unwrap();
    assert_eq!(store.text_indexed(id).unwrap(), Some(1234));
    let hits = store.search_text("rent 250", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].page, 2);
    assert!(hits[0].snippet.contains("[rent]"), "{}", hits[0].snippet);
    // Indexing again replaces, never doubles.
    store
        .index_text(id, 1235, &["The rent is due".into()])
        .unwrap();
    assert_eq!(store.search_text("rent", 10).unwrap().len(), 1);
    assert!(store.search_text("\"", 10).unwrap().is_empty());

    store.add_reading(id, "2026-11-05", 120).unwrap();
    store.add_reading(id, "2026-11-06", 30).unwrap();
    store.add_reading(id, "2026-11-07", 90).unwrap();
    store.add_reading(id, "2026-11-07", 60).unwrap();
    let s = store.reading_stats("2026-11-07", 7).unwrap();
    assert_eq!(s.today_seconds, 150);
    assert_eq!(s.week_seconds, 300);
    // 11-06 had under a minute, so only today counts.
    assert_eq!((s.streak_days, s.longest_streak), (1, 1));
    assert_eq!(s.recent_days.len(), 7);
    assert_eq!(s.recent_days[6], 150);
    assert_eq!(s.top, vec![(id, 300)]);
}

#[test]
fn collections_and_shelves() {
    let mut store = Store::open_in_memory().unwrap();
    let a = store
        .upsert_document(
            &fake_ident(21),
            std::path::Path::new("/b/a.pdf"),
            &DocMeta::new(Format::Pdf),
        )
        .unwrap();
    let b = store
        .upsert_document(
            &fake_ident(22),
            std::path::Path::new("/b/b.pdf"),
            &DocMeta::new(Format::Pdf),
        )
        .unwrap();
    let work = store.create_collection("Work").unwrap();
    // Same name, any case: the same collection.
    assert_eq!(store.create_collection(" work ").unwrap(), work);
    store.set_in_collection(work, a, true).unwrap();
    store.set_in_collection(work, a, true).unwrap();
    let cs = store.collections().unwrap();
    assert_eq!((cs.len(), cs[0].count), (1, 1));
    assert_eq!(store.collections_of(a).unwrap(), [work]);
    assert_eq!(
        store
            .shelf(kitab_store::Shelf::Collection(work))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(store.shelf(kitab_store::Shelf::All).unwrap().len(), 2);
    store.set_favourite(b, true).unwrap();
    let fav = store.shelf(kitab_store::Shelf::Favourites).unwrap();
    assert_eq!(fav.iter().map(|e| e.id).collect::<Vec<_>>(), [b]);
    assert!(store.is_favourite(b).unwrap());
    store.set_finished(a, true).unwrap();
    assert_eq!(store.shelf(kitab_store::Shelf::Finished).unwrap()[0].id, a);
    store.set_in_collection(work, a, false).unwrap();
    assert!(
        store
            .shelf(kitab_store::Shelf::Collection(work))
            .unwrap()
            .is_empty()
    );
    store.rename_collection(work, "Office").unwrap();
    assert_eq!(store.collections().unwrap()[0].name, "Office");
    store.delete_collection(work).unwrap();
    assert!(store.collections().unwrap().is_empty());
    assert_eq!(
        store.shelf(kitab_store::Shelf::All).unwrap().len(),
        2,
        "documents stay"
    );

    store.watch_folder(std::path::Path::new("/books")).unwrap();
    store.watch_folder(std::path::Path::new("/books")).unwrap();
    assert_eq!(store.watched_folders().unwrap().len(), 1);
    store
        .unwatch_folder(std::path::Path::new("/books"))
        .unwrap();
    assert!(store.watched_folders().unwrap().is_empty());
}

#[test]
fn unwatching_a_folder_lets_go_of_what_only_it_brought_in() {
    let mut store = Store::open_in_memory().unwrap();
    let doc = |store: &mut Store, n: u64, path: &str| {
        store
            .upsert_document(
                &fake_ident(n),
                std::path::Path::new(path),
                &DocMeta::new(Format::Pdf),
            )
            .unwrap()
    };
    store.watch_folder(std::path::Path::new("/scans")).unwrap();
    store
        .watch_folder(std::path::Path::new("/scans/keep"))
        .unwrap();
    let found = doc(&mut store, 31, "/scans/found.pdf");
    let opened = doc(&mut store, 32, "/scans/opened.pdf");
    let fav = doc(&mut store, 33, "/scans/fav.pdf");
    let kept = doc(&mut store, 34, "/scans/in-collection.pdf");
    let nested = doc(&mut store, 35, "/scans/keep/still-watched.pdf");
    let elsewhere = doc(&mut store, 36, "/scansplus/other.pdf");
    store.mark_opened(opened).unwrap();
    store.set_favourite(fav, true).unwrap();
    let c = store.create_collection("Keep").unwrap();
    store.set_in_collection(c, kept, true).unwrap();

    let gone = store
        .unwatch_folder(std::path::Path::new("/scans"))
        .unwrap();
    assert_eq!(gone, 1, "only the never-used document under the folder");
    assert!(store.document(found).unwrap().is_none());
    for id in [opened, fav, kept, nested, elsewhere] {
        assert!(store.document(id).unwrap().is_some());
    }
    assert_eq!(
        store.watched_folders().unwrap(),
        [std::path::PathBuf::from("/scans/keep")]
    );
}
