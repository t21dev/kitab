// SPDX-License-Identifier: AGPL-3.0-or-later

//! The library: documents, recognising moved files, reading state, recents.

use std::path::{Path, PathBuf};

use kitab_core::{DocInfo, Format, Locator, ReadingState};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::codec::{
    clamp_progress, finite, format_from_str, format_to_str, zoom_from_columns, zoom_to_columns,
};
use crate::store::b;
use crate::{Error, FileIdentity, MAX_META_CHARS, MAX_RECENT, Result, Store};

/// A row in `documents`. Stable for the life of the database: never reused
/// after a delete, so annotations, collections and tags can point at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(pub i64);

/// The metadata the library keeps about a document, taken from [`DocInfo`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocMeta {
    pub format: Format,
    pub title: Option<String>,
    pub author: Option<String>,
    pub pages: Option<u32>,
}

impl From<&DocInfo> for DocMeta {
    fn from(info: &DocInfo) -> Self {
        DocMeta {
            format: info.format,
            title: info.title.clone(),
            author: info.author.clone(),
            pages: Some(info.page_count),
        }
    }
}

impl DocMeta {
    /// Metadata with just a format (before the engine has read the file).
    pub fn new(format: Format) -> Self {
        DocMeta {
            format,
            title: None,
            author: None,
            pages: None,
        }
    }
}

/// One document as the library sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentRecord {
    pub id: DocumentId,
    pub path: PathBuf,
    pub file_key: Option<String>,
    pub quick_hash: u64,
    pub size_bytes: u64,
    pub format: Format,
    pub title: Option<String>,
    pub author: Option<String>,
    pub pages: Option<u32>,
    /// Unix milliseconds.
    pub added_at: i64,
    pub last_opened_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub favourite: bool,
    pub missing: bool,
}

/// One row of Home's "Recent" list.
#[derive(Debug, Clone, PartialEq)]
pub struct RecentEntry {
    pub id: DocumentId,
    pub path: PathBuf,
    pub title: Option<String>,
    pub author: Option<String>,
    pub format: Format,
    /// 0.0..=1.0, `None` if the document has no saved reading state.
    pub progress: Option<f32>,
    /// Unix milliseconds.
    pub last_opened_at: i64,
    /// The file was not found the last time anyone looked (greyed out, "Locate…").
    pub missing: bool,
    /// Marked a favourite (a ribbon on its card).
    pub favourite: bool,
}

/// Paths are stored as text; non-Unicode paths are rejected rather than mangled.
pub(crate) fn path_str(path: &Path) -> Result<&str> {
    path.to_str().ok_or(Error::NonUnicodePath)
}

fn u64_to_db(v: u64) -> i64 {
    i64::from_le_bytes(v.to_le_bytes())
}

fn u64_from_db(v: i64) -> u64 {
    u64::from_le_bytes(v.to_le_bytes())
}

/// Truncate untrusted metadata to [`MAX_META_CHARS`] characters; empty becomes `None`.
fn clip(s: Option<&str>) -> Option<String> {
    let s = s?.trim();
    if s.is_empty() {
        return None;
    }
    Some(s.chars().take(MAX_META_CHARS).collect())
}

/// How many same-hash candidates to look at when matching a moved file.
const MATCH_CANDIDATES: i64 = 16;

impl Store {
    /// Add a document to the library, or update it if it is already known.
    /// Returns its id. Does not count as opening it: call
    /// [`Store::mark_opened`] for that.
    ///
    /// A known document is found by, in order:
    /// 1. its path;
    /// 2. its `file_key`, if the quick hash also matches or the old path is
    ///    gone (a rename, a move within a volume, or a move plus an edit);
    /// 3. its quick hash and size, if the old path is gone (a move across
    ///    volumes, or a copy back from a backup).
    ///
    /// In cases 2 and 3 the stored path is updated, so reading state,
    /// bookmarks and tabs follow the file. A copy whose original still exists
    /// becomes a new document. The row is marked not missing. Title, author
    /// and page count are only overwritten by values that are present.
    ///
    /// Does file-system checks (`Path::exists`) on candidates: io thread only.
    pub fn upsert_document(
        &mut self,
        identity: &FileIdentity,
        path: &Path,
        meta: &DocMeta,
    ) -> Result<DocumentId> {
        let path_s = path_str(path)?;
        let now = self.now_ms();
        let tx = self.conn.transaction()?;
        let found = match find_by_path(&tx, path_s)? {
            Some(id) => Some(id),
            None => match find_by_file_key(&tx, identity)? {
                Some(id) => Some(id),
                None => find_by_hash(&tx, identity)?,
            },
        };
        let title = clip(meta.title.as_deref());
        let author = clip(meta.author.as_deref());
        let pages = meta.pages.map(i64::from);
        let id = match found {
            Some(id) => {
                tx.prepare_cached(
                    "UPDATE documents SET path = ?1, file_key = ?2, quick_hash = ?3,
                         size_bytes = ?4, mtime_ms = ?5, format = ?6,
                         title = COALESCE(?7, title), author = COALESCE(?8, author),
                         pages = COALESCE(?9, pages), missing = 0
                     WHERE id = ?10",
                )?
                .execute(params![
                    path_s,
                    identity.file_key,
                    u64_to_db(identity.quick_hash),
                    u64_to_db(identity.size),
                    identity.mtime_ms,
                    format_to_str(meta.format),
                    title,
                    author,
                    pages,
                    id.0
                ])?;
                id
            }
            None => {
                tx.prepare_cached(
                    "INSERT INTO documents (path, file_key, quick_hash, size_bytes, mtime_ms,
                         format, title, author, pages, added_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )?
                .execute(params![
                    path_s,
                    identity.file_key,
                    u64_to_db(identity.quick_hash),
                    u64_to_db(identity.size),
                    identity.mtime_ms,
                    format_to_str(meta.format),
                    title,
                    author,
                    pages,
                    now
                ])?;
                DocumentId(tx.last_insert_rowid())
            }
        };
        tx.commit()?;
        Ok(id)
    }

    /// Record that the document was opened now (moves it to the top of Recent).
    pub fn mark_opened(&mut self, id: DocumentId) -> Result<()> {
        let now = self.now_ms();
        self.conn
            .prepare_cached("UPDATE documents SET last_opened_at = ?1 WHERE id = ?2")?
            .execute(params![now, id.0])?;
        Ok(())
    }

    /// Flag the document's file as missing (greyed out with "Locate…"). It
    /// keeps its reading state; [`Store::upsert_document`] clears the flag.
    pub fn mark_missing(&mut self, id: DocumentId) -> Result<()> {
        self.conn
            .prepare_cached("UPDATE documents SET missing = 1 WHERE id = ?1")?
            .execute(params![id.0])?;
        Ok(())
    }

    /// Check whether the document's file still exists at its stored path and
    /// update the missing flag to match. Returns `true` if it exists.
    /// File-system access: io thread only, and not at start-up.
    pub fn refresh_missing(&mut self, id: DocumentId) -> Result<bool> {
        let Some(doc) = self.document(id)? else {
            return Ok(false);
        };
        let exists = doc.path.is_file();
        self.conn
            .prepare_cached("UPDATE documents SET missing = ?1 WHERE id = ?2")?
            .execute(params![b(!exists), id.0])?;
        Ok(exists)
    }

    /// Mark the document finished (or not).
    pub fn set_finished(&mut self, id: DocumentId, finished: bool) -> Result<()> {
        let at = finished.then(|| self.now_ms());
        self.conn
            .prepare_cached("UPDATE documents SET finished_at = ?1 WHERE id = ?2")?
            .execute(params![at, id.0])?;
        Ok(())
    }

    /// Star or unstar the document.
    pub fn set_favourite(&mut self, id: DocumentId, favourite: bool) -> Result<()> {
        self.conn
            .prepare_cached("UPDATE documents SET favourite = ?1 WHERE id = ?2")?
            .execute(params![b(favourite), id.0])?;
        Ok(())
    }

    /// Remove the document from the library, with its reading state,
    /// bookmarks, closed-tab entries and session tabs. The file is not touched.
    pub fn forget_document(&mut self, id: DocumentId) -> Result<()> {
        self.conn
            .prepare_cached("DELETE FROM documents WHERE id = ?1")?
            .execute(params![id.0])?;
        Ok(())
    }

    /// The document with this id, if it exists.
    /// The file moved and the user showed Kitab where: the document (its
    /// reading place, bookmarks, marks, collections) now lives at `path`.
    /// False when another document already has that path.
    pub fn relocate_document(&mut self, id: DocumentId, path: &Path) -> Result<bool> {
        let path_s = path_str(path)?;
        let tx = self.conn.transaction()?;
        if find_by_path(&tx, path_s)?.is_some_and(|other| other != id) {
            return Ok(false);
        }
        tx.execute(
            "UPDATE documents SET path = ?1, missing = 0 WHERE id = ?2",
            params![path_s, id.0],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn document(&self, id: DocumentId) -> Result<Option<DocumentRecord>> {
        self.document_where("id = ?1", rusqlite::types::Value::Integer(id.0))
    }

    /// The document stored at exactly this path, if any.
    pub fn document_by_path(&self, path: &Path) -> Result<Option<DocumentRecord>> {
        let p = path_str(path)?.to_owned();
        self.document_where("path = ?1", rusqlite::types::Value::Text(p))
    }

    fn document_where(
        &self,
        cond: &'static str,
        arg: rusqlite::types::Value,
    ) -> Result<Option<DocumentRecord>> {
        type Row = (
            i64,
            String,
            Option<String>,
            i64,
            i64,
            String,
            Option<String>,
            Option<String>,
            Option<i64>,
            i64,
            Option<i64>,
            Option<i64>,
            bool,
            bool,
        );
        let sql = format!(
            "SELECT id, path, file_key, quick_hash, size_bytes, format, title, author, pages,
                    added_at, last_opened_at, finished_at, favourite, missing
             FROM documents WHERE {cond}"
        );
        let row: Option<Row> = self
            .conn
            .prepare_cached(&sql)?
            .query_row([arg], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                    r.get(12)?,
                    r.get(13)?,
                ))
            })
            .optional()?;
        let Some(r) = row else { return Ok(None) };
        Ok(Some(DocumentRecord {
            id: DocumentId(r.0),
            path: PathBuf::from(r.1),
            file_key: r.2,
            quick_hash: u64_from_db(r.3),
            size_bytes: u64_from_db(r.4),
            format: format_from_str(&r.5)?,
            title: r.6,
            author: r.7,
            pages: r.8.and_then(|p| u32::try_from(p).ok()),
            added_at: r.9,
            last_opened_at: r.10,
            finished_at: r.11,
            favourite: r.12,
            missing: r.13,
        }))
    }

    /// The saved reading state of a document, if any.
    pub fn reading_state(&self, id: DocumentId) -> Result<Option<ReadingState>> {
        type Row = (String, i64, f64, f64, f64, String, Option<f64>, i64);
        let row: Option<Row> = self
            .conn
            .prepare_cached(
                "SELECT locator_kind, page, x, y, progress, zoom_mode, zoom, rotation
                 FROM reading_state WHERE document_id = ?1",
            )?
            .query_row([id.0], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })
            .optional()?;
        let Some((kind, page, x, y, progress, zoom_mode, zoom, rotation)) = row else {
            return Ok(None);
        };
        let locator = match kind.as_str() {
            "fixed" => Locator::Fixed {
                page: u32::try_from(page).map_err(|_| Error::Corrupt("page"))?,
                x: finite(x as f32, 0.0),
                y: finite(y as f32, 0.0),
            },
            _ => return Err(Error::Corrupt("locator_kind")),
        };
        Ok(Some(ReadingState {
            locator,
            zoom: zoom_from_columns(&zoom_mode, zoom)?,
            rotation: u16::try_from(rotation).map_err(|_| Error::Corrupt("rotation"))?,
            progress: clamp_progress(progress as f32),
        }))
    }

    /// Save a document's reading state (one indexed upsert). The caller
    /// debounces: at most one write every 2 s, plus on tab switch and quit
    /// (PLAN.md §3.6). Non-finite numbers are replaced (SQLite turns NaN
    /// into NULL) and progress is clamped to 0..=1.
    pub fn save_reading_state(&mut self, id: DocumentId, state: &ReadingState) -> Result<()> {
        let now = self.now_ms();
        let Locator::Fixed { page, x, y } = state.locator;
        let (zoom_mode, zoom) = zoom_to_columns(state.zoom);
        self.conn
            .prepare_cached(
                "INSERT INTO reading_state (document_id, locator_kind, page, x, y, progress,
                     zoom_mode, zoom, rotation, updated_at)
                 VALUES (?1, 'fixed', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT (document_id) DO UPDATE SET
                     locator_kind = excluded.locator_kind, page = excluded.page,
                     x = excluded.x, y = excluded.y, progress = excluded.progress,
                     zoom_mode = excluded.zoom_mode, zoom = excluded.zoom,
                     rotation = excluded.rotation, updated_at = excluded.updated_at",
            )?
            .execute(params![
                id.0,
                i64::from(page),
                f64::from(finite(x, 0.0)),
                f64::from(finite(y, 0.0)),
                f64::from(clamp_progress(state.progress)),
                zoom_mode,
                zoom,
                i64::from(state.rotation),
                now
            ])?;
        Ok(())
    }

    /// The most recently opened documents, newest first, at most `limit`
    /// (capped at [`crate::MAX_RECENT`]). One query on the `documents_recent`
    /// index. Missing files are included, flagged.
    pub fn recent(&self, limit: usize) -> Result<Vec<RecentEntry>> {
        let limit = i64::try_from(limit.min(MAX_RECENT)).unwrap_or(0);
        let mut stmt = self.conn.prepare_cached(
            "SELECT d.id, d.path, d.title, d.author, d.format, rs.progress,
                    d.last_opened_at, d.missing, d.favourite
             FROM documents d INDEXED BY documents_recent
             LEFT JOIN reading_state rs ON rs.document_id = d.id
             WHERE d.last_opened_at IS NOT NULL
             ORDER BY d.last_opened_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<f64>>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, bool>(7)?,
                r.get::<_, bool>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, path, title, author, format, progress, last_opened_at, missing, favourite) =
                row?;
            out.push(RecentEntry {
                id: DocumentId(id),
                path: PathBuf::from(path),
                title,
                author,
                format: format_from_str(&format)?,
                progress: progress.map(|p| clamp_progress(p as f32)),
                last_opened_at,
                missing,
                favourite,
            });
        }
        Ok(out)
    }
}

fn find_by_path(tx: &Transaction<'_>, path: &str) -> Result<Option<DocumentId>> {
    Ok(tx
        .prepare_cached("SELECT id FROM documents WHERE path = ?1")?
        .query_row([path], |r| r.get(0))
        .optional()?
        .map(DocumentId))
}

fn find_by_file_key(tx: &Transaction<'_>, ident: &FileIdentity) -> Result<Option<DocumentId>> {
    let Some(key) = &ident.file_key else {
        return Ok(None);
    };
    let mut stmt = tx.prepare_cached(
        "SELECT id, path, quick_hash, size_bytes FROM documents
         WHERE file_key = ?1 ORDER BY id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![key, MATCH_CANDIDATES], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;
    let mut gone = None;
    for row in rows {
        let (id, old_path, hash, size) = row?;
        if u64_from_db(hash) == ident.quick_hash && u64_from_db(size) == ident.size {
            return Ok(Some(DocumentId(id)));
        }
        // File ids can be reused once a file is deleted; without a hash match
        // only accept the row if its file really has moved away.
        if gone.is_none() && !Path::new(&old_path).exists() {
            gone = Some(DocumentId(id));
        }
    }
    Ok(gone)
}

fn find_by_hash(tx: &Transaction<'_>, ident: &FileIdentity) -> Result<Option<DocumentId>> {
    let mut stmt = tx.prepare_cached(
        "SELECT id, path FROM documents
         WHERE quick_hash = ?1 AND size_bytes = ?2 ORDER BY id DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        params![
            u64_to_db(ident.quick_hash),
            u64_to_db(ident.size),
            MATCH_CANDIDATES
        ],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
    )?;
    for row in rows {
        let (id, old_path) = row?;
        // A copy whose original is still there is a different document.
        if !Path::new(&old_path).exists() {
            return Ok(Some(DocumentId(id)));
        }
    }
    Ok(None)
}
