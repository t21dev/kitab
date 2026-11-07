-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Schema 2: markup a reader adds (PLAN.md Phase 6). Kept for every format, so
-- nothing drawn is lost before a PDF is saved, and books, comics and XPS (which
-- Kitab doesn't write into) keep their marks here. Saving a PDF writes its
-- marks into the file and deletes these rows.

CREATE TABLE marks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    -- 0-based page.
    page        INTEGER NOT NULL,
    -- 'ink', 'line', 'arrow', 'rect', 'ellipse', 'highlight', 'underline',
    -- 'strike', 'text', 'note' (Mark::kind), for lists without decoding.
    kind        TEXT    NOT NULL,
    -- The mark as JSON (codec.rs), validated when read back.
    data        TEXT    NOT NULL,
    created_at  INTEGER NOT NULL
) STRICT;

CREATE INDEX marks_by_document ON marks (document_id, page);
