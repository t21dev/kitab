// SPDX-License-Identifier: AGPL-3.0-or-later

//! Bookmarks: named positions in a document.

use kitab_core::Locator;
use rusqlite::params;

use crate::codec::{decode_locator, encode_locator};
use crate::documents::DocumentId;
use crate::{MAX_META_CHARS, Result, Store};

/// A row in `bookmarks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BookmarkId(pub i64);

/// A saved position with a name.
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub id: BookmarkId,
    pub document_id: DocumentId,
    pub locator: Locator,
    pub title: String,
    /// Unix milliseconds.
    pub created_at: i64,
}

impl Store {
    /// Add a bookmark. The title is trimmed to [`crate::MAX_META_CHARS`] characters.
    pub fn add_bookmark(
        &mut self,
        document_id: DocumentId,
        locator: &Locator,
        title: &str,
    ) -> Result<BookmarkId> {
        let now = self.now_ms();
        let title: String = title.chars().take(MAX_META_CHARS).collect();
        self.conn
            .prepare_cached(
                "INSERT INTO bookmarks (document_id, locator, title, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )?
            .execute(params![document_id.0, encode_locator(locator), title, now])?;
        Ok(BookmarkId(self.conn.last_insert_rowid()))
    }

    /// A document's bookmarks, oldest first. Rows whose locator cannot be
    /// decoded are skipped rather than failing the whole list.
    pub fn bookmarks(&self, document_id: DocumentId) -> Result<Vec<Bookmark>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, locator, title, created_at FROM bookmarks
             WHERE document_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([document_id.0], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, locator, title, created_at) = row?;
            if let Ok(locator) = decode_locator(&locator) {
                out.push(Bookmark {
                    id: BookmarkId(id),
                    document_id,
                    locator,
                    title,
                    created_at,
                });
            }
        }
        Ok(out)
    }

    /// Remove a bookmark. Returns `false` if it did not exist.
    pub fn remove_bookmark(&mut self, id: BookmarkId) -> Result<bool> {
        let n = self
            .conn
            .prepare_cached("DELETE FROM bookmarks WHERE id = ?1")?
            .execute([id.0])?;
        Ok(n > 0)
    }
}
