// SPDX-License-Identifier: AGPL-3.0-or-later

//! Upkeep from Settings › Advanced: forget the reading history, and compact
//! the database.

use crate::{Result, Store};

impl Store {
    /// Forget what was read and when: the Recent list, saved reading
    /// positions and closed tabs. Marks and bookmarks are the reader's own
    /// work and stay, as do the documents they belong to and the session that
    /// reopens today's tabs.
    pub fn clear_history(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch(
            "UPDATE documents SET last_opened_at = NULL;
             DELETE FROM reading_state;
             DELETE FROM closed_tabs;",
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The database's size in bytes (pages in use times page size).
    pub fn size_bytes(&self) -> Result<u64> {
        let pages: i64 = self.conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let size: i64 = self.conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok((pages.max(0) as u64) * (size.max(0) as u64))
    }

    /// Fold the write-ahead log back in, rebuild the file without free pages
    /// and refresh the query planner's statistics. Returns the size before
    /// and after, in bytes.
    pub fn optimize(&mut self) -> Result<(u64, u64)> {
        let before = self.size_bytes()?;
        self.conn.execute_batch(
            "PRAGMA optimize;
             PRAGMA wal_checkpoint(TRUNCATE);
             VACUUM;",
        )?;
        Ok((before, self.size_bytes()?))
    }
}
