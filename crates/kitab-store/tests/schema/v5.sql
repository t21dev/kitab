-- SPDX-License-Identifier: AGPL-3.0-or-later
-- page_text rows get rowid = document_id * 2^20 + page, so one document's
-- text is a rowid range: replacing or dropping it touches only its rows
-- (document_id is UNINDEXED, so deleting by it scanned the whole index).
-- Rows of documents no longer in the library are dropped on the way.

CREATE TEMP TABLE page_text_old AS
    SELECT body, document_id, page FROM page_text
    WHERE document_id IN (SELECT id FROM documents) AND page >= 0 AND page < 1048576;
DELETE FROM page_text;
INSERT INTO page_text (rowid, body, document_id, page)
    SELECT document_id * 1048576 + page, body, document_id, page FROM page_text_old;
DROP TABLE page_text_old;
DELETE FROM text_index WHERE document_id NOT IN (SELECT id FROM documents);

-- A document leaving the library (forgotten, its folder unwatched) takes its
-- text with it; FTS tables can't have foreign keys.
CREATE TRIGGER page_text_document_gone AFTER DELETE ON documents BEGIN
    DELETE FROM page_text
    WHERE rowid BETWEEN old.id * 1048576 AND old.id * 1048576 + 1048575;
END;
