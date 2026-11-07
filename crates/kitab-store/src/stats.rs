// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reading stats (PLAN.md Phase 9): seconds read per local day and
//! document, and what Home shows from them: today, this week, the streak.

use rusqlite::params;

use crate::documents::DocumentId;
use crate::{Result, Store};

/// A day counts towards a streak with at least this much reading.
pub const STREAK_SECONDS: i64 = 60;

/// What Home shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadingStats {
    pub today_seconds: i64,
    /// The last seven days, today included.
    pub week_seconds: i64,
    /// Days in a row with reading, ending today (or yesterday, if today
    /// hasn't started yet).
    pub streak_days: u32,
    pub longest_streak: u32,
    /// Seconds per day for the last `days` days, oldest first.
    pub recent_days: Vec<i64>,
    /// The documents read most in the last 30 days: (id, seconds).
    pub top: Vec<(DocumentId, i64)>,
}

/// Days since 1970-01-01 for a 'YYYY-MM-DD' date.
pub fn day_number(day: &str) -> Option<i64> {
    let mut it = day.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's days-from-civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// The 'YYYY-MM-DD' of a day number.
pub fn day_name(n: i64) -> String {
    let z = n + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Current and longest streaks from the days with reading (day numbers,
/// any order), as of `today`.
pub fn streaks(days: &[i64], today: i64) -> (u32, u32) {
    let mut days: Vec<i64> = days.to_vec();
    days.sort_unstable();
    days.dedup();
    let (mut longest, mut run, mut prev) = (0u32, 0u32, None);
    for &d in &days {
        run = if prev == Some(d - 1) { run + 1 } else { 1 };
        longest = longest.max(run);
        prev = Some(d);
    }
    // The current streak ends today, or yesterday when today is still empty.
    let end = if days.binary_search(&today).is_ok() {
        today
    } else {
        today - 1
    };
    let mut current = 0u32;
    let mut d = end;
    while days.binary_search(&d).is_ok() {
        current += 1;
        d -= 1;
    }
    (current, longest)
}

impl Store {
    /// Add `seconds` of reading `id` on `day` ('YYYY-MM-DD', local).
    pub fn add_reading(&mut self, id: DocumentId, day: &str, seconds: i64) -> Result<()> {
        if seconds <= 0 || day_number(day).is_none() {
            return Ok(());
        }
        self.conn
            .prepare_cached(
                "INSERT INTO reading_days (day, document_id, seconds) VALUES (?1, ?2, ?3)
                 ON CONFLICT (day, document_id) DO UPDATE SET seconds = seconds + excluded.seconds",
            )?
            .execute(params![day, id.0, seconds.min(86_400)])?;
        Ok(())
    }

    /// The stats as of `today` ('YYYY-MM-DD', local), with `days` days of
    /// history for the chart.
    pub fn reading_stats(&self, today: &str, days: usize) -> Result<ReadingStats> {
        let Some(t) = day_number(today) else {
            return Ok(ReadingStats::default());
        };
        let mut per_day: Vec<(i64, i64)> = Vec::new();
        {
            let mut stmt = self
                .conn
                .prepare_cached("SELECT day, sum(seconds) FROM reading_days GROUP BY day")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (day, secs) = row?;
                if let Some(n) = day_number(&day) {
                    per_day.push((n, secs));
                }
            }
        }
        let sum_since = |from: i64| -> i64 {
            per_day
                .iter()
                .filter(|(d, _)| *d >= from && *d <= t)
                .map(|(_, s)| s)
                .sum()
        };
        let read_days: Vec<i64> = per_day
            .iter()
            .filter(|(_, s)| *s >= STREAK_SECONDS)
            .map(|(d, _)| *d)
            .collect();
        let (streak_days, longest_streak) = streaks(&read_days, t);
        let recent_days = (0..days as i64)
            .rev()
            .map(|back| {
                per_day
                    .iter()
                    .filter(|(d, _)| *d == t - back)
                    .map(|(_, s)| s)
                    .sum()
            })
            .collect();
        let since = day_name(t - 29);
        let mut stmt = self.conn.prepare_cached(
            "SELECT document_id, sum(seconds) AS s FROM reading_days
             WHERE day >= ?1 GROUP BY document_id ORDER BY s DESC LIMIT 5",
        )?;
        let top = stmt
            .query_map([since], |r| {
                Ok((DocumentId(r.get(0)?), r.get::<_, i64>(1)?))
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(ReadingStats {
            today_seconds: sum_since(t),
            week_seconds: sum_since(t - 6),
            streak_days,
            longest_streak,
            recent_days,
            top,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip() {
        assert_eq!(day_number("1970-01-01"), Some(0));
        assert_eq!(
            day_number("2024-02-29").map(day_name).as_deref(),
            Some("2024-02-29")
        );
        assert_eq!(
            day_name(day_number("2026-11-07").unwrap_or(0)),
            "2026-11-07"
        );
        assert_eq!(day_number("2026-13-01"), None);
        assert_eq!(day_number("nonsense"), None);
    }

    #[test]
    fn streaks_count_days_in_a_row() {
        // Read 1–3 and 6–8; today is 8.
        assert_eq!(streaks(&[1, 2, 3, 6, 7, 8], 8), (3, 3));
        // Today not started yet: yesterday's run still counts.
        assert_eq!(streaks(&[6, 7, 8], 9), (3, 3));
        // A missed day ends it.
        assert_eq!(streaks(&[1, 2, 3, 4], 6), (0, 4));
        assert_eq!(streaks(&[], 5), (0, 0));
    }
}
