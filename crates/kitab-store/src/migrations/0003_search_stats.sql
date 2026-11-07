-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Library search and reading stats (PLAN.md Phase 9).

-- The text of every page of the documents Kitab has opened, for searching
-- the whole library. Built in the background; rebuilt when the file changes.
CREATE VIRTUAL TABLE page_text USING fts5(
    body,
    document_id UNINDEXED,
    page UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- Which documents are in page_text, and the file they were read from.
CREATE TABLE text_index (
    document_id INTEGER PRIMARY KEY REFERENCES documents (id) ON DELETE CASCADE,
    file_mtime  INTEGER NOT NULL,
    pages       INTEGER NOT NULL,
    indexed_at  INTEGER NOT NULL
);

-- Time spent reading, per local day ('YYYY-MM-DD') and document.
CREATE TABLE reading_days (
    day         TEXT    NOT NULL,
    document_id INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    seconds     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, document_id)
) WITHOUT ROWID;
