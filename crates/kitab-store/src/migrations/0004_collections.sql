-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Collections and watched folders (PLAN.md Phase 9).

-- A collection is a named set of documents; a document can be in several,
-- so collections work as tags too.
CREATE TABLE collections (
    id         INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    created_at INTEGER NOT NULL
);

CREATE TABLE collection_items (
    collection_id INTEGER NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    document_id   INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    added_at      INTEGER NOT NULL,
    PRIMARY KEY (collection_id, document_id)
) WITHOUT ROWID;

CREATE INDEX collection_items_document ON collection_items (document_id);

-- Folders whose documents join the library without being opened.
CREATE TABLE watched_folders (
    path     TEXT    PRIMARY KEY,
    added_at INTEGER NOT NULL
);
