// SPDX-License-Identifier: AGPL-3.0-or-later

//! Library search (PLAN.md Phase 9): page text in an FTS5 index, searched
//! with SQLite's own ranking and snippets.

use rusqlite::params;

use crate::documents::DocumentId;
use crate::{Result, Store};

/// One page that matches, best first.
#[derive(Debug, Clone, PartialEq)]
pub struct TextHit {
    pub document_id: DocumentId,
    /// 0-based.
    pub page: u32,
    /// A few words around the match, the matched words between `[` and `]`.
    pub snippet: String,
}

/// A search box's text as an FTS5 query: each word a prefix match, all of
/// them required, quotes and operators taken literally. Words split on
/// spaces and ASCII punctuation only, so combining marks (Bangla, Arabic)
/// stay with their letters.
pub fn fts_query(text: &str) -> Option<String> {
    let words: Vec<String> = text
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|w| !w.is_empty())
        .take(12)
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// Pages per document the index can hold: a page's row is
/// `document_id * PAGE_SPAN + page`, so a document is one rowid range.
const PAGE_SPAN: i64 = 1 << 20;

impl Store {
    /// The file time the document's text was read at, if it's indexed.
    pub fn text_indexed(&self, id: DocumentId) -> Result<Option<i64>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT file_mtime FROM text_index WHERE document_id = ?1")?;
        let mut rows = stmt.query([id.0])?;
        Ok(match rows.next()? {
            Some(r) => Some(r.get(0)?),
            None => None,
        })
    }

    /// Replace a document's text: `pages` in order, from a file of `mtime`.
    pub fn index_text(&mut self, id: DocumentId, mtime: i64, pages: &[String]) -> Result<()> {
        let now = self.now_ms();
        let tx = self.conn.transaction()?;
        let first = id.0.saturating_mul(PAGE_SPAN);
        tx.execute(
            "DELETE FROM page_text WHERE rowid BETWEEN ?1 AND ?2",
            [first, first.saturating_add(PAGE_SPAN - 1)],
        )?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT INTO page_text (rowid, body, document_id, page) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (i, body) in pages.iter().enumerate().take(PAGE_SPAN as usize) {
                if !body.trim().is_empty() {
                    let page = i as i64;
                    insert.execute(params![first + page, body, id.0, page])?;
                }
            }
        }
        tx.execute(
            "INSERT INTO text_index (document_id, file_mtime, pages, indexed_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (document_id) DO UPDATE SET
                file_mtime = excluded.file_mtime,
                pages = excluded.pages,
                indexed_at = excluded.indexed_at",
            params![id.0, mtime, pages.len() as i64, now],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Pages matching `text` across the library, best first, at most `limit`.
    pub fn search_text(&self, text: &str, limit: usize) -> Result<Vec<TextHit>> {
        let Some(query) = fts_query(text) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare_cached(
            "SELECT document_id, page, snippet(page_text, 0, '[', ']', '…', 12)
             FROM page_text WHERE page_text MATCH ?1
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![query, limit as i64], |r| {
            Ok(TextHit {
                document_id: DocumentId(r.get(0)?),
                page: r.get::<_, i64>(1)?.max(0) as u32,
                snippet: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// How many documents are indexed.
    pub fn indexed_count(&self) -> Result<u64> {
        let n: i64 = self
            .conn
            .query_row("SELECT count(*) FROM text_index", [], |r| r.get(0))?;
        Ok(n.max(0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_are_words_not_syntax() {
        assert_eq!(
            fts_query("rent  25000"),
            Some("\"rent\"* \"25000\"*".into())
        );
        assert_eq!(
            fts_query("\"x\" OR y-z"),
            Some("\"x\"* \"OR\"* \"y\"* \"z\"*".into())
        );
        assert_eq!(fts_query("  ...  "), None);
        assert_eq!(fts_query("সময় হলো"), Some("\"সময়\"* \"হলো\"*".into()));
    }
}
