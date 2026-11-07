// SPDX-License-Identifier: AGPL-3.0-or-later

//! Collections, the smart lists (favourites, in progress, finished, every
//! document) and watched folders (PLAN.md Phase 9).

use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use crate::codec::format_from_str;
use crate::documents::{DocumentId, RecentEntry};
use crate::{MAX_META_CHARS, Result, Store};

/// A row in `collections`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CollectionId(pub i64);

/// A collection and how many documents it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    pub id: CollectionId,
    pub name: String,
    pub count: u32,
}

/// Which documents a library list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    /// Every document the library knows, opened or found in a watched folder.
    All,
    Favourites,
    /// Started and not finished.
    InProgress,
    Finished,
    Collection(CollectionId),
}

/// The most documents a list returns.
pub const MAX_SHELF: usize = 2_000;

impl Store {
    pub fn collections(&self) -> Result<Vec<Collection>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT c.id, c.name, count(i.document_id)
             FROM collections c LEFT JOIN collection_items i ON i.collection_id = c.id
             GROUP BY c.id ORDER BY c.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Collection {
                id: CollectionId(r.get(0)?),
                name: r.get(1)?,
                count: r.get::<_, i64>(2)?.max(0) as u32,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// A new collection, or the one already called `name` (any case).
    pub fn create_collection(&mut self, name: &str) -> Result<CollectionId> {
        let name: String = name.trim().chars().take(MAX_META_CHARS).collect();
        if let Some(id) = self
            .conn
            .prepare_cached("SELECT id FROM collections WHERE name = ?1")?
            .query_row([&name], |r| r.get(0))
            .optional()?
        {
            return Ok(CollectionId(id));
        }
        let now = self.now_ms();
        self.conn
            .prepare_cached("INSERT INTO collections (name, created_at) VALUES (?1, ?2)")?
            .execute(params![name, now])?;
        Ok(CollectionId(self.conn.last_insert_rowid()))
    }

    pub fn rename_collection(&mut self, id: CollectionId, name: &str) -> Result<()> {
        let name: String = name.trim().chars().take(MAX_META_CHARS).collect();
        self.conn
            .prepare_cached("UPDATE collections SET name = ?1 WHERE id = ?2")?
            .execute(params![name, id.0])?;
        Ok(())
    }

    /// Delete a collection; its documents stay in the library.
    pub fn delete_collection(&mut self, id: CollectionId) -> Result<()> {
        self.conn
            .prepare_cached("DELETE FROM collections WHERE id = ?1")?
            .execute([id.0])?;
        Ok(())
    }

    /// Put a document in a collection (`true`) or take it out.
    pub fn set_in_collection(
        &mut self,
        collection: CollectionId,
        doc: DocumentId,
        inside: bool,
    ) -> Result<()> {
        if inside {
            let now = self.now_ms();
            self.conn
                .prepare_cached(
                    "INSERT OR IGNORE INTO collection_items (collection_id, document_id, added_at)
                     VALUES (?1, ?2, ?3)",
                )?
                .execute(params![collection.0, doc.0, now])?;
        } else {
            self.conn
                .prepare_cached(
                    "DELETE FROM collection_items WHERE collection_id = ?1 AND document_id = ?2",
                )?
                .execute(params![collection.0, doc.0])?;
        }
        Ok(())
    }

    /// The collections a document is in.
    pub fn collections_of(&self, doc: DocumentId) -> Result<Vec<CollectionId>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT collection_id FROM collection_items WHERE document_id = ?1")?;
        let rows = stmt.query_map([doc.0], |r| Ok(CollectionId(r.get(0)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The documents on a shelf, most recently opened first (never-opened
    /// ones after, by title).
    pub fn shelf(&self, shelf: Shelf) -> Result<Vec<RecentEntry>> {
        let (filter, arg) = match shelf {
            Shelf::All => ("1", 0),
            Shelf::Favourites => ("d.favourite = 1", 0),
            Shelf::InProgress => (
                "rs.progress > 0.0 AND rs.progress < 0.995 AND d.finished_at IS NULL",
                0,
            ),
            Shelf::Finished => ("(d.finished_at IS NOT NULL OR rs.progress >= 0.995)", 0),
            Shelf::Collection(c) => (
                "d.id IN (SELECT document_id FROM collection_items WHERE collection_id = ?2)",
                c.0,
            ),
        };
        let sql = format!(
            "SELECT d.id, d.path, d.title, d.author, d.format, rs.progress,
                    coalesce(d.last_opened_at, 0), d.missing
             FROM documents d
             LEFT JOIN reading_state rs ON rs.document_id = d.id
             WHERE {filter} AND (?2 = ?2)
             ORDER BY d.last_opened_at IS NULL, d.last_opened_at DESC,
                      coalesce(d.title, d.path) COLLATE NOCASE
             LIMIT ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![MAX_SHELF as i64, arg], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<f64>>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, bool>(7)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, path, title, author, format, progress, last, missing) = row?;
            out.push(RecentEntry {
                id: DocumentId(id),
                path: PathBuf::from(path),
                title,
                author,
                format: format_from_str(&format)?,
                progress: progress.map(|p| (p as f32).clamp(0.0, 1.0)),
                last_opened_at: last,
                missing,
            });
        }
        Ok(out)
    }

    /// Whether a document is a favourite.
    pub fn is_favourite(&self, doc: DocumentId) -> Result<bool> {
        Ok(self
            .conn
            .prepare_cached("SELECT favourite FROM documents WHERE id = ?1")?
            .query_row([doc.0], |r| r.get::<_, bool>(0))
            .optional()?
            .unwrap_or(false))
    }

    pub fn watched_folders(&self) -> Result<Vec<PathBuf>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT path FROM watched_folders ORDER BY path")?;
        let rows = stmt.query_map([], |r| Ok(PathBuf::from(r.get::<_, String>(0)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn watch_folder(&mut self, path: &Path) -> Result<()> {
        let now = self.now_ms();
        self.conn
            .prepare_cached(
                "INSERT OR IGNORE INTO watched_folders (path, added_at) VALUES (?1, ?2)",
            )?
            .execute(params![path.to_string_lossy(), now])?;
        Ok(())
    }

    /// Stop watching a folder; documents already found stay in the library.
    pub fn unwatch_folder(&mut self, path: &Path) -> Result<()> {
        self.conn
            .prepare_cached("DELETE FROM watched_folders WHERE path = ?1")?
            .execute([path.to_string_lossy()])?;
        Ok(())
    }
}
