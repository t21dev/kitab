-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Schema version 1 (PLAN.md §3.6). Released migrations are frozen: never edit
-- this file once it has shipped; add 0002_*.sql instead (tests/migrations.rs).
--
-- Conventions
-- * Times are Unix milliseconds (INTEGER).
-- * Booleans are INTEGER 0/1 with a CHECK.
-- * Ids that other tables (annotations, collections, tags, FTS, sync) will
--   point at use AUTOINCREMENT so a deleted id is never handed out again.
-- * STRICT tables: SQLite enforces the declared column types.

CREATE TABLE documents (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    path           TEXT    NOT NULL UNIQUE,
    -- 'w:<volume>:<file id>' on Windows, 'u:<dev>:<inode>' on Unix; NULL if unknown.
    file_key       TEXT,
    -- XXH3-64 of size + first 64 KiB + last 64 KiB, stored as a signed bit pattern.
    quick_hash     INTEGER NOT NULL,
    size_bytes     INTEGER NOT NULL,
    mtime_ms       INTEGER,
    format         TEXT    NOT NULL,
    title          TEXT,
    author         TEXT,
    language       TEXT,
    pages          INTEGER,
    cover_ref      TEXT,
    added_at       INTEGER NOT NULL,
    last_opened_at INTEGER,
    finished_at    INTEGER,
    favourite      INTEGER NOT NULL DEFAULT 0 CHECK (favourite IN (0, 1)),
    missing        INTEGER NOT NULL DEFAULT 0 CHECK (missing IN (0, 1))
) STRICT;

CREATE INDEX documents_file_key ON documents (file_key) WHERE file_key IS NOT NULL;
CREATE INDEX documents_quick_hash ON documents (quick_hash, size_bytes);
-- Home's "Recent" list and the start-up path read this index only.
CREATE INDEX documents_recent ON documents (last_opened_at DESC) WHERE last_opened_at IS NOT NULL;

CREATE TABLE reading_state (
    document_id  INTEGER PRIMARY KEY REFERENCES documents (id) ON DELETE CASCADE,
    -- 'fixed' (page + point). Reflow and Markdown locators add their own kinds and columns.
    locator_kind TEXT    NOT NULL,
    page         INTEGER NOT NULL,
    x            REAL    NOT NULL,
    y            REAL    NOT NULL,
    progress     REAL    NOT NULL CHECK (progress >= 0.0 AND progress <= 1.0),
    -- 'fit_page' | 'fit_width' | 'actual' | 'custom' (zoom holds the factor).
    zoom_mode    TEXT    NOT NULL,
    zoom         REAL,
    rotation     INTEGER NOT NULL,
    view_mode    TEXT,
    updated_at   INTEGER NOT NULL
) STRICT;

CREATE TABLE sessions (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at     INTEGER NOT NULL,
    closed_cleanly INTEGER NOT NULL DEFAULT 0 CHECK (closed_cleanly IN (0, 1))
) STRICT;

CREATE TABLE session_tabs (
    session_id   INTEGER NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    ord          INTEGER NOT NULL,
    document_id  INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    pinned       INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    active       INTEGER NOT NULL DEFAULT 0 CHECK (active IN (0, 1)),
    -- Encoded ReadingState snapshot for this tab (see codec.rs); NULL = use reading_state.
    scroll_state TEXT,
    PRIMARY KEY (session_id, ord)
) STRICT, WITHOUT ROWID;

CREATE INDEX session_tabs_document ON session_tabs (document_id);

CREATE TABLE closed_tabs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id  INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    closed_at    INTEGER NOT NULL,
    scroll_state TEXT
) STRICT;

CREATE INDEX closed_tabs_document ON closed_tabs (document_id);

CREATE TABLE bookmarks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    -- Encoded Locator (see codec.rs), e.g. 'fixed:12:0:340.5'.
    locator     TEXT    NOT NULL,
    title       TEXT    NOT NULL,
    created_at  INTEGER NOT NULL
) STRICT;

CREATE INDEX bookmarks_document ON bookmarks (document_id, created_at);
