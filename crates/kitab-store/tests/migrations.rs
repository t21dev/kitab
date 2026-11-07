// SPDX-License-Identifier: AGPL-3.0-or-later

//! Migration tests (AGENTS.md rule 15: forward-only, automatic, tested from
//! every released schema version).
//!
//! # How this works
//!
//! `tests/schema/vN.sql` is a frozen copy of the migration that produced
//! schema version N (`src/migrations/000N_*.sql`). [`released_migrations_are_frozen`]
//! fails if anyone edits a released migration instead of adding a new one.
//!
//! [`FIXTURES`] lists one builder per released version. A builder creates a
//! database *as that version of Kitab left it*: it applies the frozen
//! `v1.sql..=vN.sql` with plain rusqlite (never through `Store`, whose code
//! moves on), sets `user_version = N`, and inserts seed rows covering every
//! table. [`every_released_version_migrates_to_current`] opens each fixture
//! with `Store::open` and checks the seed data survives.
//!
//! # Adding schema version N+1
//!
//! 1. Add `src/migrations/000{N+1}_<name>.sql` and append it to `MIGRATIONS`
//!    in `src/migrate.rs`.
//! 2. Copy it to `tests/schema/v{N+1}.sql` and add it to [`FROZEN`].
//! 3. Add `build_v{N+1}` to [`FIXTURES`]: call `apply_frozen(conn, N+1)` and
//!    seed rows for the new tables, plus whatever the old builders seed.
//!    Keep the old builders as they are: users may still be on those versions.
//! 4. Extend `check_seed` for the new tables.

// Test helpers outside #[test] fns panic on purpose: a failed setup is a failed test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use kitab_core::{Format, Locator, ReadingState, ZoomMode};
use kitab_store::{DocumentId, Error, SCHEMA_VERSION, Store};
use rusqlite::Connection;

/// Frozen migrations, index = version - 1.
const FROZEN: &[&str] = &[
    include_str!("schema/v1.sql"),
    include_str!("schema/v2.sql"),
    include_str!("schema/v3.sql"),
    include_str!("schema/v4.sql"),
];

/// The live migrations, read straight from the source tree.
const LIVE: &[&str] = &[
    include_str!("../src/migrations/0001_initial.sql"),
    include_str!("../src/migrations/0002_marks.sql"),
    include_str!("../src/migrations/0003_search_stats.sql"),
    include_str!("../src/migrations/0004_collections.sql"),
];

/// One fixture builder per released schema version.
type Builder = fn(&Connection);
const FIXTURES: &[(u32, Builder)] = &[(1, build_v1), (2, build_v2), (3, build_v3), (4, build_v4)];

fn normalise(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn apply_frozen(conn: &Connection, version: u32) {
    for sql in &FROZEN[..version as usize] {
        conn.execute_batch(sql).unwrap();
    }
    conn.execute_batch(&format!("PRAGMA user_version = {version}"))
        .unwrap();
}

/// Kitab 0.1 (schema 1): one PDF with reading state and a bookmark, a
/// crashed session with that tab, and a closed tab.
fn build_v1(conn: &Connection) {
    apply_frozen(conn, 1);
    conn.execute_batch(
        "INSERT INTO documents (id, path, file_key, quick_hash, size_bytes, mtime_ms, format,
             title, author, pages, added_at, last_opened_at, favourite, missing)
         VALUES (7, '/books/a.pdf', 'u:1:2', -42, 60307, 1000, 'pdf',
             'A Title', 'An Author', 12, 1000, 2000, 1, 0);
         INSERT INTO reading_state (document_id, locator_kind, page, x, y, progress,
             zoom_mode, zoom, rotation, updated_at)
         VALUES (7, 'fixed', 3, 10.5, 200.25, 0.25, 'custom', 1.5, 90, 2000);
         INSERT INTO bookmarks (document_id, locator, title, created_at)
         VALUES (7, 'fixed:5:0:100', 'Chapter 2', 1500);
         INSERT INTO sessions (id, created_at, closed_cleanly) VALUES (3, 1800, 0);
         INSERT INTO session_tabs (session_id, ord, document_id, pinned, active, scroll_state)
         VALUES (3, 0, 7, 1, 1, 'v1;fixed:3:10.5:200.25;fit_width;0;0.25');
         INSERT INTO closed_tabs (document_id, closed_at, scroll_state)
         VALUES (7, 1900, NULL);",
    )
    .unwrap();
}

/// Schema 2: version 1's seed (rebuilt on the v2 schema) plus a mark.
fn build_v2(conn: &Connection) {
    build_v1(conn);
    conn.execute_batch(FROZEN[1]).unwrap();
    // The colour is data; assembled so the view-colour lint doesn't read it as UI.
    let amber = format!("{}e6b450", '#');
    conn.execute_batch(&format!(
        r#"PRAGMA user_version = 2;
         INSERT INTO marks (document_id, page, kind, data, created_at)
         VALUES (7, 2, 'rect', '{{"color":"{amber}","width":2.0,"opacity":1.0,"rect":[10,20,30,40]}}', 2100);"#
    ))
    .unwrap();
}

/// Schema 3: version 2's seed plus indexed page text and reading time.
fn build_v3(conn: &Connection) {
    build_v2(conn);
    conn.execute_batch(FROZEN[2]).unwrap();
    conn.execute_batch(
        "PRAGMA user_version = 3;
         INSERT INTO page_text (body, document_id, page) VALUES ('the lease ends in June', 7, 4);
         INSERT INTO text_index (document_id, file_mtime, pages, indexed_at) VALUES (7, 1000, 12, 2200);
         INSERT INTO reading_days (day, document_id, seconds) VALUES ('2026-11-06', 7, 600);",
    )
    .unwrap();
}

/// Schema 4: version 3's seed plus a collection holding the document and a
/// watched folder.
fn build_v4(conn: &Connection) {
    build_v3(conn);
    conn.execute_batch(FROZEN[3]).unwrap();
    conn.execute_batch(
        "PRAGMA user_version = 4;
         INSERT INTO collections (id, name, created_at) VALUES (2, 'Leases', 2300);
         INSERT INTO collection_items (collection_id, document_id, added_at) VALUES (2, 7, 2300);
         INSERT INTO watched_folders (path, added_at) VALUES ('/books', 2300);",
    )
    .unwrap();
}

/// What every fixture's seed data must look like after migrating.
fn check_seed(store: &mut Store) {
    let id = DocumentId(7);
    let doc = store.document(id).unwrap().expect("seed document survives");
    assert_eq!(doc.path, Path::new("/books/a.pdf"));
    assert_eq!(doc.format, Format::Pdf);
    assert_eq!(doc.title.as_deref(), Some("A Title"));
    assert_eq!(doc.quick_hash, (-42i64) as u64);
    assert!(doc.favourite);

    assert_eq!(
        store.reading_state(id).unwrap(),
        Some(ReadingState {
            locator: Locator::Fixed {
                page: 3,
                x: 10.5,
                y: 200.25
            },
            zoom: ZoomMode::Custom(1.5),
            rotation: 90,
            progress: 0.25,
        })
    );

    let marks = store.bookmarks(id).unwrap();
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].title, "Chapter 2");

    let session = store.last_session().unwrap().expect("seed session");
    assert!(!session.closed_cleanly);
    assert_eq!(session.tabs.len(), 1);
    assert_eq!(session.tabs[0].tab.document_id, id);
    assert_eq!(
        session.tabs[0].tab.scroll.map(|s| s.zoom),
        Some(ZoomMode::FitWidth)
    );

    let closed = store.pop_closed_tab().unwrap().expect("seed closed tab");
    assert_eq!(closed.document_id, id);

    let recent = store.recent(10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].progress, Some(0.25));

    // Schema 2: marks. A database from version 1 has none; version 2's seed has one.
    let marks = store.marks(id).unwrap();
    assert!(marks.len() <= 1);
    if let Some((_, m)) = marks.first() {
        assert_eq!(m.page, 2);
        assert_eq!(m.kind(), "rect");
    }

    // Schema 3: page text and reading time, when the version had them.
    if store.text_indexed(id).unwrap().is_some() {
        let hits = store.search_text("lease", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].page, 4);
        let stats = store.reading_stats("2026-11-07", 7).unwrap();
        assert_eq!(stats.streak_days, 1);
        assert_eq!(stats.week_seconds, 600);
    }

    // Schema 4: collections and watched folders, when the version had them.
    let collections = store.collections().unwrap();
    if let Some(c) = collections.first() {
        assert_eq!((c.name.as_str(), c.count), ("Leases", 1));
        let shelf = store.shelf(kitab_store::Shelf::Collection(c.id)).unwrap();
        assert_eq!(shelf[0].id, id);
        assert_eq!(store.watched_folders().unwrap(), [Path::new("/books")]);
    }
}

#[test]
fn released_migrations_are_frozen() {
    assert_eq!(
        LIVE.len(),
        SCHEMA_VERSION as usize,
        "list every migration in LIVE"
    );
    assert_eq!(FROZEN.len(), LIVE.len(), "freeze every migration in FROZEN");
    for (i, (frozen, live)) in FROZEN.iter().zip(LIVE).enumerate() {
        assert_eq!(
            normalise(frozen),
            normalise(live),
            "migration {} was edited after release; add a new migration instead",
            i + 1
        );
    }
    assert_eq!(
        FIXTURES.len(),
        SCHEMA_VERSION as usize,
        "one fixture per version"
    );
}

#[test]
fn empty_database_migrates_to_current() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("kitab.db");
    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    drop(store);

    // WAL is persistent, and reopening is a no-op migration.
    let conn = Connection::open(&path).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    drop(conn);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    assert!(store.recent(10).unwrap().is_empty());
    assert!(store.last_session().unwrap().is_none());
}

#[test]
fn every_released_version_migrates_to_current() {
    for (version, build) in FIXTURES {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("v{version}.db"));
        {
            let conn = Connection::open(&path).unwrap();
            build(&conn);
        }
        let mut store = Store::open(&path).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            SCHEMA_VERSION,
            "from v{version}"
        );
        check_seed(&mut store);
    }
}

#[test]
fn newer_schema_is_refused_and_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("future.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE future (x INTEGER); PRAGMA user_version = {};",
            SCHEMA_VERSION + 1
        ))
        .unwrap();
    }
    match Store::open(&path) {
        Err(Error::SchemaTooNew { found, supported }) => {
            assert_eq!(found, SCHEMA_VERSION + 1);
            assert_eq!(supported, SCHEMA_VERSION);
        }
        other => panic!("expected SchemaTooNew, got {other:?}"),
    }
    let conn = Connection::open(&path).unwrap();
    let v: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, SCHEMA_VERSION + 1);
    let docs: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'documents'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(docs, 0, "no tables were created in a newer database");
}

#[test]
fn not_a_database_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("junk.db");
    std::fs::write(&path, vec![0x5au8; 8192]).unwrap();
    assert!(matches!(Store::open(&path), Err(Error::Sqlite(_))));
}
