// SPDX-License-Identifier: AGPL-3.0-or-later

//! Sessions (open tabs, crash detection) and the closed-tab stack.

use std::path::PathBuf;

use kitab_core::{Format, ReadingState};
use rusqlite::{OptionalExtension, params};

use crate::codec::{decode_state, encode_state, format_from_str};
use crate::documents::DocumentId;
use crate::store::b;
use crate::{MAX_CLOSED_TABS, MAX_SESSIONS, Result, Store};

/// One run of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(pub i64);

/// An entry on the closed-tab stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClosedTabId(pub i64);

/// One open tab, as saved in a session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabRecord {
    pub document_id: DocumentId,
    pub pinned: bool,
    /// The focused tab. The caller keeps at most one per session.
    pub active: bool,
    /// The tab's own position, if it differs from the document's reading state
    /// (two tabs on one document). `None` means "use the reading state".
    pub scroll: Option<ReadingState>,
}

/// A saved tab plus what the tab strip needs to draw it without opening the
/// file (PLAN.md §6.3: inactive tabs are metadata until focused).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionTab {
    pub tab: TabRecord,
    pub path: PathBuf,
    pub title: Option<String>,
    pub format: Format,
    pub missing: bool,
}

/// The newest session, for "Restore previous session".
#[derive(Debug, Clone, PartialEq)]
pub struct LastSession {
    pub id: SessionId,
    /// `false` if the app crashed or was killed: offer to restore.
    pub closed_cleanly: bool,
    /// Tabs in strip order.
    pub tabs: Vec<SessionTab>,
}

/// A popped closed tab, ready to reopen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClosedTab {
    pub id: ClosedTabId,
    pub document_id: DocumentId,
    /// Unix milliseconds.
    pub closed_at: i64,
    pub scroll: Option<ReadingState>,
}

/// A damaged snapshot is dropped, not fatal: the tab falls back to the
/// document's reading state.
fn decode_scroll(s: Option<String>) -> Option<ReadingState> {
    s.and_then(|s| decode_state(&s).ok())
}

impl Store {
    /// The newest session and its tabs, in one indexed query. Call it at
    /// start-up *before* [`Store::begin_session`], which starts a newer one.
    /// Tabs whose document was forgotten are gone (foreign-key cascade).
    pub fn last_session(&self) -> Result<Option<LastSession>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT s.id, s.closed_cleanly, t.document_id, t.pinned, t.active, t.scroll_state,
                    d.path, d.title, d.format, d.missing
             FROM (SELECT id, closed_cleanly FROM sessions ORDER BY id DESC LIMIT 1) s
             LEFT JOIN session_tabs t ON t.session_id = s.id
             LEFT JOIN documents d ON d.id = t.document_id
             ORDER BY t.ord",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, bool>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<bool>>(3)?,
                r.get::<_, Option<bool>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<bool>>(9)?,
            ))
        })?;
        let mut session: Option<LastSession> = None;
        for row in rows {
            let (sid, clean, doc, pinned, active, scroll, path, title, format, missing) = row?;
            let s = session.get_or_insert_with(|| LastSession {
                id: SessionId(sid),
                closed_cleanly: clean,
                tabs: Vec::new(),
            });
            // A LEFT JOIN row with no tab means the session has no tabs.
            let (Some(doc), Some(path), Some(format)) = (doc, path, format) else {
                continue;
            };
            s.tabs.push(SessionTab {
                tab: TabRecord {
                    document_id: DocumentId(doc),
                    pinned: pinned.unwrap_or(false),
                    active: active.unwrap_or(false),
                    scroll: decode_scroll(scroll),
                },
                path: PathBuf::from(path),
                title,
                format: format_from_str(&format)?,
                missing: missing.unwrap_or(false),
            });
        }
        Ok(session)
    }

    /// Start a new session (not closed cleanly until
    /// [`Store::end_session_cleanly`]). Keeps the newest [`crate::MAX_SESSIONS`].
    pub fn begin_session(&mut self) -> Result<SessionId> {
        let now = self.now_ms();
        let tx = self.conn.transaction()?;
        tx.prepare_cached("INSERT INTO sessions (created_at) VALUES (?1)")?
            .execute([now])?;
        let id = SessionId(tx.last_insert_rowid());
        tx.prepare_cached(
            "DELETE FROM sessions WHERE id <= (
                 SELECT id FROM sessions ORDER BY id DESC LIMIT 1 OFFSET ?1)",
        )?
        .execute([i64::try_from(MAX_SESSIONS).unwrap_or(i64::MAX)])?;
        tx.commit()?;
        Ok(id)
    }

    /// Replace the session's tabs with `tabs`, in strip order, atomically.
    /// Called on tab changes (debounced) and at quit.
    pub fn save_session_tabs(&mut self, session: SessionId, tabs: &[TabRecord]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.prepare_cached("DELETE FROM session_tabs WHERE session_id = ?1")?
            .execute([session.0])?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT INTO session_tabs (session_id, ord, document_id, pinned, active, scroll_state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (ord, tab) in tabs.iter().enumerate() {
                insert.execute(params![
                    session.0,
                    i64::try_from(ord).unwrap_or(i64::MAX),
                    tab.document_id.0,
                    b(tab.pinned),
                    b(tab.active),
                    tab.scroll.as_ref().map(encode_state),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Mark the session as closed cleanly (normal quit). A session left
    /// without this was a crash.
    pub fn end_session_cleanly(&mut self, session: SessionId) -> Result<()> {
        self.conn
            .prepare_cached("UPDATE sessions SET closed_cleanly = 1 WHERE id = ?1")?
            .execute([session.0])?;
        Ok(())
    }

    /// Push a closed tab (Ctrl+W) for Ctrl+Shift+T. Keeps the newest
    /// [`crate::MAX_CLOSED_TABS`].
    pub fn push_closed_tab(
        &mut self,
        document_id: DocumentId,
        scroll: Option<&ReadingState>,
    ) -> Result<ClosedTabId> {
        let now = self.now_ms();
        let tx = self.conn.transaction()?;
        tx.prepare_cached(
            "INSERT INTO closed_tabs (document_id, closed_at, scroll_state) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![document_id.0, now, scroll.map(encode_state)])?;
        let id = ClosedTabId(tx.last_insert_rowid());
        tx.prepare_cached(
            "DELETE FROM closed_tabs WHERE id <= (
                 SELECT id FROM closed_tabs ORDER BY id DESC LIMIT 1 OFFSET ?1)",
        )?
        .execute([i64::try_from(MAX_CLOSED_TABS).unwrap_or(i64::MAX)])?;
        tx.commit()?;
        Ok(id)
    }

    /// Pop the most recently closed tab, if any (Ctrl+Shift+T).
    pub fn pop_closed_tab(&mut self) -> Result<Option<ClosedTab>> {
        let tx = self.conn.transaction()?;
        let row = tx
            .prepare_cached(
                "SELECT id, document_id, closed_at, scroll_state FROM closed_tabs
                 ORDER BY id DESC LIMIT 1",
            )?
            .query_row([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .optional()?;
        let Some((id, doc, closed_at, scroll)) = row else {
            return Ok(None);
        };
        tx.prepare_cached("DELETE FROM closed_tabs WHERE id = ?1")?
            .execute([id])?;
        tx.commit()?;
        Ok(Some(ClosedTab {
            id: ClosedTabId(id),
            document_id: DocumentId(doc),
            closed_at,
            scroll: decode_scroll(scroll),
        }))
    }
}
