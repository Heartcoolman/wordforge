//! Aggregate-across-all-users analytics queries for the admin dashboard.
//!
//! Each public method returns a tightly-scoped row type so the route layer
//! can assemble JSON without leaking SQL plumbing. All time inputs are
//! resolved to a UTC `YYYY-MM-DD` start-of-day boundary; `learning_records`
//! and `learning_sessions` columns store ISO-8601 strings, which compare
//! lexicographically to the date prefix used in `WHERE`.

use chrono::{DateTime, Duration, Utc};
use rusqlite::params;

use crate::store::operations::records::RecordType;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Default)]
pub struct AdminStudySummaryRow {
    pub total_duration_secs: i64,
    pub session_count: i64,
    pub record_count: i64,
    pub correct_count: i64,
    pub new_words: i64,
    pub review_words: i64,
    pub mastered_words: i64,
}

#[derive(Debug, Clone, Default)]
pub struct AdminStudyDailyRow {
    pub date: String,
    pub duration_secs: i64,
    pub session_count: i64,
    pub record_count: i64,
    pub correct_count: i64,
    pub new_words: i64,
    pub review_words: i64,
    pub mastered_words: i64,
}

#[derive(Debug, Clone)]
pub struct AdminDailyRecordTypeRow {
    pub date: String,
    pub record_type: String,
    pub total: i64,
    pub correct: i64,
}

#[derive(Debug, Clone, Default)]
pub struct AdminWordStateDistributionRow {
    pub new_count: i64,
    pub learning: i64,
    pub reviewing: i64,
    pub mastered: i64,
    pub forgotten: i64,
    pub bookmarked: i64,
    pub due: i64,
    pub overdue: i64,
    pub average_mastery: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct AdminRetentionSampleRow {
    pub first_learned_at: DateTime<Utc>,
    /// `word_learning_states.half_life` is stored in **hours**.
    pub half_life_hours: Option<f64>,
    pub state_updated_at: Option<DateTime<Utc>>,
    pub mdm_last_review_at: Option<DateTime<Utc>>,
    pub total_attempts: i64,
}

#[derive(Debug, Clone)]
pub struct AdminDailyRegisteredUsersRow {
    pub date: String,
    pub registered: i64,
}

/// Returns a `YYYY-MM-DD` string for `now - (days - 1)` (inclusive day).
/// `learning_records.created_at` / `learning_sessions.created_at` /
/// `users.created_at` are ISO-8601 UTC strings (`2026-04-25T08:00:00Z`),
/// which are lexicographically `>=` `'2026-04-25'`, so this string can be
/// bound directly to range predicates and the b-tree index is used.
fn window_since_date(days: u32) -> String {
    let today = Utc::now().date_naive();
    let start = today - Duration::days(days.saturating_sub(1).max(0) as i64);
    start.format("%Y-%m-%d").to_string()
}

/// Parse `"YYYY-MM-DD HH:MM:SS"` / RFC-3339 strings; the schema uses both.
fn parse_dt(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|n| DateTime::<Utc>::from_naive_utc_and_offset(n, Utc))
        })
}

impl Store {
    pub fn admin_study_overview_summary(
        &self,
        days: u32,
        record_type: Option<RecordType>,
    ) -> Result<AdminStudySummaryRow, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        let mut out = AdminStudySummaryRow::default();

        // -- record + correct + new/review words --------------------------
        // `new_words` are (user_id, word_id) pairs whose ALL-TIME first record
        // (filtered by category if requested) falls inside the window. Anything
        // older than the window in the same category is treated as `review`.
        let (records, correct, new_words, review_words): (i64, i64, i64, i64) = match record_type {
            Some(rt) => conn.query_row(
                "SELECT
                    COUNT(*),
                    COALESCE(SUM(is_correct), 0),
                    COUNT(DISTINCT CASE WHEN NOT EXISTS (
                        SELECT 1 FROM learning_records p
                        WHERE p.user_id = lr.user_id
                          AND p.word_id = lr.word_id
                          AND p.created_at < ?1
                          AND p.record_type = ?2
                    ) THEN lr.user_id || ':' || lr.word_id END),
                    COUNT(DISTINCT CASE WHEN EXISTS (
                        SELECT 1 FROM learning_records p
                        WHERE p.user_id = lr.user_id
                          AND p.word_id = lr.word_id
                          AND p.created_at < ?1
                          AND p.record_type = ?2
                    ) THEN lr.user_id || ':' || lr.word_id END)
                 FROM learning_records lr
                 WHERE lr.created_at >= ?1 AND lr.record_type = ?2",
                params![&since, rt.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?,
            None => conn.query_row(
                "SELECT
                    COUNT(*),
                    COALESCE(SUM(is_correct), 0),
                    COUNT(DISTINCT CASE WHEN NOT EXISTS (
                        SELECT 1 FROM learning_records p
                        WHERE p.user_id = lr.user_id
                          AND p.word_id = lr.word_id
                          AND p.created_at < ?1
                    ) THEN lr.user_id || ':' || lr.word_id END),
                    COUNT(DISTINCT CASE WHEN EXISTS (
                        SELECT 1 FROM learning_records p
                        WHERE p.user_id = lr.user_id
                          AND p.word_id = lr.word_id
                          AND p.created_at < ?1
                    ) THEN lr.user_id || ':' || lr.word_id END)
                 FROM learning_records lr
                 WHERE lr.created_at >= ?1",
                params![&since],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?,
        };
        out.record_count = records;
        out.correct_count = correct;
        out.new_words = new_words;
        out.review_words = review_words;

        // -- sessions + duration -----------------------------------------
        // Duration falls back through the same ladder used by the user-side
        // dashboard: `summary_duration_secs` → completed wall time → 0.
        let (sessions, duration): (i64, i64) = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE
                    WHEN summary_duration_secs IS NOT NULL THEN summary_duration_secs
                    WHEN status = 'completed'
                        THEN CAST((julianday(updated_at) - julianday(created_at)) * 86400 AS INTEGER)
                    ELSE 0
                END), 0)
             FROM learning_sessions
             WHERE created_at >= ?1",
            params![&since],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        out.session_count = sessions;
        out.total_duration_secs = duration.max(0);

        // -- mastered words (by state transition window) -----------------
        out.mastered_words = conn.query_row(
            "SELECT COUNT(*)
             FROM word_learning_states
             WHERE updated_at >= ?1 AND state = 'MASTERED'",
            params![&since],
            |r| r.get(0),
        )?;

        Ok(out)
    }

    pub fn admin_daily_study_overview(
        &self,
        days: u32,
        record_type: Option<RecordType>,
    ) -> Result<Vec<AdminStudyDailyRow>, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        let mut by_date = std::collections::BTreeMap::<String, AdminStudyDailyRow>::new();
        let row = |date: String| AdminStudyDailyRow {
            date,
            ..Default::default()
        };

        // Records per day (with new/review classification).
        let sql = match record_type {
            Some(_) => {
                "SELECT DATE(lr.created_at) AS d,
                    COUNT(*),
                    COALESCE(SUM(lr.is_correct), 0),
                    COUNT(DISTINCT CASE WHEN NOT EXISTS (
                        SELECT 1 FROM learning_records p
                        WHERE p.user_id = lr.user_id
                          AND p.word_id = lr.word_id
                          AND DATE(p.created_at) < DATE(lr.created_at)
                          AND p.record_type = ?2
                    ) THEN lr.user_id || ':' || lr.word_id END),
                    COUNT(DISTINCT CASE WHEN EXISTS (
                        SELECT 1 FROM learning_records p
                        WHERE p.user_id = lr.user_id
                          AND p.word_id = lr.word_id
                          AND DATE(p.created_at) < DATE(lr.created_at)
                          AND p.record_type = ?2
                    ) THEN lr.user_id || ':' || lr.word_id END)
                 FROM learning_records lr
                 WHERE lr.created_at >= ?1 AND lr.record_type = ?2
                 GROUP BY d ORDER BY d"
            }
            None => {
                "SELECT DATE(lr.created_at) AS d,
                    COUNT(*),
                    COALESCE(SUM(lr.is_correct), 0),
                    COUNT(DISTINCT CASE WHEN NOT EXISTS (
                        SELECT 1 FROM learning_records p
                        WHERE p.user_id = lr.user_id
                          AND p.word_id = lr.word_id
                          AND DATE(p.created_at) < DATE(lr.created_at)
                    ) THEN lr.user_id || ':' || lr.word_id END),
                    COUNT(DISTINCT CASE WHEN EXISTS (
                        SELECT 1 FROM learning_records p
                        WHERE p.user_id = lr.user_id
                          AND p.word_id = lr.word_id
                          AND DATE(p.created_at) < DATE(lr.created_at)
                    ) THEN lr.user_id || ':' || lr.word_id END)
                 FROM learning_records lr
                 WHERE lr.created_at >= ?1
                 GROUP BY d ORDER BY d"
            }
        };
        let mut stmt = conn.prepare(sql)?;
        let mapper = |r: &rusqlite::Row<'_>| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        };
        let rows = match record_type {
            Some(rt) => stmt.query_map(params![&since, rt.as_str()], mapper)?,
            None => stmt.query_map(params![&since], mapper)?,
        };
        for r in rows {
            let (date, total, correct, new_w, review_w) = r?;
            let entry = by_date.entry(date.clone()).or_insert_with(|| row(date));
            entry.record_count = total;
            entry.correct_count = correct;
            entry.new_words = new_w;
            entry.review_words = review_w;
        }

        // Sessions + duration per day.
        let mut stmt = conn.prepare(
            "SELECT DATE(created_at) AS d,
                COUNT(*),
                COALESCE(SUM(CASE
                    WHEN summary_duration_secs IS NOT NULL THEN summary_duration_secs
                    WHEN status = 'completed'
                        THEN CAST((julianday(updated_at) - julianday(created_at)) * 86400 AS INTEGER)
                    ELSE 0
                END), 0)
             FROM learning_sessions
             WHERE created_at >= ?1
             GROUP BY d ORDER BY d",
        )?;
        let session_rows = stmt.query_map(params![&since], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for r in session_rows {
            let (date, sessions, duration) = r?;
            let entry = by_date.entry(date.clone()).or_insert_with(|| row(date));
            entry.session_count = sessions;
            entry.duration_secs = duration.max(0);
        }

        // Mastered transitions per day.
        let mut stmt = conn.prepare(
            "SELECT DATE(updated_at) AS d, COUNT(*)
             FROM word_learning_states
             WHERE updated_at >= ?1 AND state = 'MASTERED'
             GROUP BY d ORDER BY d",
        )?;
        let mastered_rows = stmt.query_map(params![&since], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for r in mastered_rows {
            let (date, mastered) = r?;
            let entry = by_date.entry(date.clone()).or_insert_with(|| row(date));
            entry.mastered_words = mastered;
        }

        Ok(by_date.into_values().collect())
    }

    pub fn admin_daily_record_type_counts(
        &self,
        days: u32,
    ) -> Result<Vec<AdminDailyRecordTypeRow>, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        let mut stmt = conn.prepare(
            "SELECT DATE(created_at), record_type, COUNT(*),
                    COALESCE(SUM(is_correct), 0)
             FROM learning_records
             WHERE created_at >= ?1
             GROUP BY DATE(created_at), record_type
             ORDER BY DATE(created_at), record_type",
        )?;
        let rows = stmt.query_map(params![&since], |r| {
            Ok(AdminDailyRecordTypeRow {
                date: r.get(0)?,
                record_type: r.get(1)?,
                total: r.get(2)?,
                correct: r.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn admin_word_state_distribution(
        &self,
        record_type: Option<RecordType>,
    ) -> Result<AdminWordStateDistributionRow, StoreError> {
        let conn = self.conn()?;
        let mut out = AdminWordStateDistributionRow::default();
        let now_iso = Utc::now().to_rfc3339();

        // -- per-state counts --------------------------------------------
        let state_sql = match record_type {
            Some(_) => {
                "SELECT wls.state, COUNT(*)
                 FROM word_learning_states wls
                 WHERE EXISTS (
                    SELECT 1 FROM learning_records lr
                    WHERE lr.user_id = wls.user_id
                      AND lr.word_id = wls.word_id
                      AND lr.record_type = ?1
                 )
                 GROUP BY wls.state"
            }
            None => "SELECT state, COUNT(*) FROM word_learning_states GROUP BY state",
        };
        let mut stmt = conn.prepare(state_sql)?;
        let mapper = |r: &rusqlite::Row<'_>| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?));
        let rows = match record_type {
            Some(rt) => stmt.query_map(params![rt.as_str()], mapper)?,
            None => stmt.query_map([], mapper)?,
        };
        for r in rows {
            let (state, count) = r?;
            match state.as_str() {
                "NEW" => out.new_count = count,
                "LEARNING" => out.learning = count,
                "REVIEWING" => out.reviewing = count,
                "MASTERED" => out.mastered = count,
                "FORGOTTEN" => out.forgotten = count,
                _ => {}
            }
        }

        // -- bookmark + due/overdue + average mastery --------------------
        let (bookmarked, due, overdue, avg) = match record_type {
            Some(rt) => conn.query_row(
                "SELECT
                    (SELECT COUNT(*) FROM word_favorites wf WHERE EXISTS (
                        SELECT 1 FROM learning_records lr
                        WHERE lr.user_id = wf.user_id
                          AND lr.word_id = wf.word_id
                          AND lr.record_type = ?1
                    )),
                    (SELECT COUNT(*) FROM word_learning_states wls
                        WHERE wls.next_review_date IS NOT NULL
                          AND wls.next_review_date <= ?2
                          AND EXISTS (
                            SELECT 1 FROM learning_records lr
                            WHERE lr.user_id = wls.user_id
                              AND lr.word_id = wls.word_id
                              AND lr.record_type = ?1
                          )),
                    (SELECT COUNT(*) FROM word_learning_states wls
                        WHERE wls.next_review_date IS NOT NULL
                          AND wls.next_review_date < ?2
                          AND EXISTS (
                            SELECT 1 FROM learning_records lr
                            WHERE lr.user_id = wls.user_id
                              AND lr.word_id = wls.word_id
                              AND lr.record_type = ?1
                          )),
                    (SELECT AVG(wls.mastery_level) FROM word_learning_states wls WHERE EXISTS (
                        SELECT 1 FROM learning_records lr
                        WHERE lr.user_id = wls.user_id
                          AND lr.word_id = wls.word_id
                          AND lr.record_type = ?1
                    ))",
                params![rt.as_str(), &now_iso],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Option<f64>>(3)?,
                    ))
                },
            )?,
            None => conn.query_row(
                "SELECT
                    (SELECT COUNT(*) FROM word_favorites),
                    (SELECT COUNT(*) FROM word_learning_states
                        WHERE next_review_date IS NOT NULL AND next_review_date <= ?1),
                    (SELECT COUNT(*) FROM word_learning_states
                        WHERE next_review_date IS NOT NULL AND next_review_date < ?1),
                    (SELECT AVG(mastery_level) FROM word_learning_states)",
                params![&now_iso],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Option<f64>>(3)?,
                    ))
                },
            )?,
        };
        out.bookmarked = bookmarked;
        out.due = due;
        out.overdue = overdue;
        out.average_mastery = avg;

        Ok(out)
    }

    /// Returns one row per `(user_id, word_id)` whose first-ever (or
    /// first-in-category) learning record falls in the last `window_days`.
    /// Bucket assignment & retention math live in the route layer.
    pub fn admin_retention_curve_samples(
        &self,
        record_type: Option<RecordType>,
        window_days: u32,
    ) -> Result<Vec<AdminRetentionSampleRow>, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(window_days);
        let sql = match record_type {
            Some(_) => {
                "WITH firsts AS (
                    SELECT lr.user_id, lr.word_id, MIN(lr.created_at) AS first_at
                    FROM learning_records lr
                    WHERE lr.created_at >= ?1
                      AND lr.record_type = ?2
                      AND NOT EXISTS (
                        SELECT 1 FROM learning_records p
                        WHERE p.user_id = lr.user_id
                          AND p.word_id = lr.word_id
                          AND p.created_at < ?1
                          AND p.record_type = ?2
                      )
                    GROUP BY lr.user_id, lr.word_id
                )
                SELECT f.first_at, wls.half_life, wls.updated_at, wls.total_attempts,
                       ms.mdm_last_review_at_ms
                FROM firsts f
                LEFT JOIN word_learning_states wls
                    ON wls.user_id = f.user_id AND wls.word_id = f.word_id
                LEFT JOIN mastery_states ms
                    ON ms.user_id = f.user_id AND ms.word_id = f.word_id"
            }
            None => {
                "WITH firsts AS (
                    SELECT lr.user_id, lr.word_id, MIN(lr.created_at) AS first_at
                    FROM learning_records lr
                    WHERE lr.created_at >= ?1
                      AND NOT EXISTS (
                        SELECT 1 FROM learning_records p
                        WHERE p.user_id = lr.user_id
                          AND p.word_id = lr.word_id
                          AND p.created_at < ?1
                      )
                    GROUP BY lr.user_id, lr.word_id
                )
                SELECT f.first_at, wls.half_life, wls.updated_at, wls.total_attempts,
                       ms.mdm_last_review_at_ms
                FROM firsts f
                LEFT JOIN word_learning_states wls
                    ON wls.user_id = f.user_id AND wls.word_id = f.word_id
                LEFT JOIN mastery_states ms
                    ON ms.user_id = f.user_id AND ms.word_id = f.word_id"
            }
        };
        let mut stmt = conn.prepare(sql)?;
        let mapper = |r: &rusqlite::Row<'_>| -> rusqlite::Result<Option<AdminRetentionSampleRow>> {
            let first_raw: String = r.get(0)?;
            let Some(first_learned_at) = parse_dt(&first_raw) else {
                return Ok(None);
            };
            let half_life_hours: Option<f64> = r.get(1)?;
            let state_updated_raw: Option<String> = r.get(2)?;
            let total_attempts: Option<i64> = r.get(3)?;
            let mdm_ms: Option<i64> = r.get(4)?;
            Ok(Some(AdminRetentionSampleRow {
                first_learned_at,
                half_life_hours,
                state_updated_at: state_updated_raw.as_deref().and_then(parse_dt),
                mdm_last_review_at: mdm_ms.and_then(DateTime::<Utc>::from_timestamp_millis),
                total_attempts: total_attempts.unwrap_or(0),
            }))
        };
        let rows = match record_type {
            Some(rt) => stmt.query_map(params![&since, rt.as_str()], mapper)?,
            None => stmt.query_map(params![&since], mapper)?,
        };
        let mut out = Vec::new();
        for r in rows {
            if let Some(sample) = r? {
                out.push(sample);
            }
        }
        Ok(out)
    }

    pub fn admin_daily_registered_users(
        &self,
        days: u32,
    ) -> Result<Vec<AdminDailyRegisteredUsersRow>, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        let mut stmt = conn.prepare(
            "SELECT DATE(created_at), COUNT(*)
             FROM users
             WHERE created_at >= ?1
             GROUP BY DATE(created_at)
             ORDER BY DATE(created_at)",
        )?;
        let rows = stmt.query_map(params![&since], |r| {
            Ok(AdminDailyRegisteredUsersRow {
                date: r.get(0)?,
                registered: r.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::operations::learning_sessions::{
        LearningSession, SessionStatus, SessionSummary,
    };
    use crate::store::operations::records::LearningRecord;
    use crate::store::operations::users::User;
    use crate::store::operations::word_states::{WordLearningState, WordState};
    use chrono::Utc;

    fn test_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    fn user(store: &Store, id: &str) {
        let u = User {
            id: id.into(),
            email: format!("{id}@e.com"),
            username: id.into(),
            password_hash: "h".into(),
            is_banned: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            failed_login_count: 0,
            locked_until: None,
        };
        store.create_user(&u).unwrap();
    }

    fn seed_record(
        store: &Store,
        id: &str,
        user_id: &str,
        word_id: &str,
        rt: RecordType,
        correct: bool,
        at: DateTime<Utc>,
    ) {
        let r = LearningRecord {
            id: id.into(),
            user_id: user_id.into(),
            word_id: word_id.into(),
            is_correct: correct,
            response_time_ms: 500,
            session_id: Some("s1".into()),
            created_at: at,
            record_type: rt,
            self_rating: None,
        };
        store.create_record(&r).unwrap();
    }

    fn seed_word_state(
        store: &Store,
        user_id: &str,
        word_id: &str,
        state: WordState,
        mastery: f64,
        next_review: Option<DateTime<Utc>>,
    ) {
        let s = WordLearningState {
            user_id: user_id.into(),
            word_id: word_id.into(),
            state,
            mastery_level: mastery,
            next_review_date: next_review,
            half_life: 24.0,
            correct_streak: 0,
            total_attempts: 1,
            updated_at: Utc::now(),
        };
        store.set_word_learning_state(&s).unwrap();
    }

    #[test]
    fn window_since_date_uses_inclusive_offset() {
        let s1 = window_since_date(1);
        let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();
        assert_eq!(s1, today);
        // days=0 saturates to 0 days back -> today
        assert_eq!(window_since_date(0), today);
        // days=2 -> yesterday
        let yest = (Utc::now().date_naive() - Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        assert_eq!(window_since_date(2), yest);
    }

    #[test]
    fn parse_dt_handles_rfc3339_and_sqlite_datetime() {
        assert!(parse_dt("2026-04-25T08:00:00Z").is_some());
        assert!(parse_dt("2026-04-25 08:00:00").is_some());
        assert!(parse_dt("not-a-date").is_none());
    }

    #[test]
    fn summary_empty_db_returns_zeros() {
        let (_t, store) = test_store();
        let s = store.admin_study_overview_summary(7, None).unwrap();
        assert_eq!(s.record_count, 0);
        assert_eq!(s.correct_count, 0);
        assert_eq!(s.session_count, 0);
        assert_eq!(s.total_duration_secs, 0);
        assert_eq!(s.mastered_words, 0);
    }

    #[test]
    fn summary_with_records_aggregates_correctly() {
        let (_t, store) = test_store();
        user(&store, "u1");
        let now = Utc::now();
        seed_record(&store, "r1", "u1", "w1", RecordType::Learning, true, now);
        seed_record(&store, "r2", "u1", "w2", RecordType::Learning, false, now);
        // 老的 review on same w1 在 window 之外
        seed_record(
            &store,
            "r0",
            "u1",
            "w1",
            RecordType::Learning,
            true,
            now - Duration::days(40),
        );

        let summary = store.admin_study_overview_summary(7, None).unwrap();
        // window 内 r1 + r2 + r0 都 >= since-date 因为 since 是 7 天前的午夜，r0 在 40 天前 → 应在窗口外
        assert_eq!(summary.record_count, 2);
        assert_eq!(summary.correct_count, 1);
        // new vs review classification: w2 是新（无更早记录），w1 因 r0 存在被算作 review
        assert_eq!(summary.new_words, 1);
        assert_eq!(summary.review_words, 1);

        // filtered by record_type
        let filtered = store
            .admin_study_overview_summary(7, Some(RecordType::Learning))
            .unwrap();
        assert_eq!(filtered.record_count, 2);
    }

    #[test]
    fn daily_overview_groups_by_date() {
        let (_t, store) = test_store();
        user(&store, "u1");
        let now = Utc::now();
        seed_record(&store, "r1", "u1", "w1", RecordType::All, true, now);
        seed_record(
            &store,
            "r2",
            "u1",
            "w2",
            RecordType::All,
            true,
            now - Duration::days(1),
        );

        let daily = store.admin_daily_study_overview(7, None).unwrap();
        assert!(daily.len() >= 2);
        let total: i64 = daily.iter().map(|r| r.record_count).sum();
        assert_eq!(total, 2);

        let daily_l = store
            .admin_daily_study_overview(7, Some(RecordType::All))
            .unwrap();
        let total2: i64 = daily_l.iter().map(|r| r.record_count).sum();
        assert_eq!(total2, 2);
    }

    #[test]
    fn daily_overview_includes_sessions_and_mastered() {
        let (_t, store) = test_store();
        user(&store, "u1");
        let now = Utc::now();
        let session = LearningSession {
            id: "s1".into(),
            user_id: "u1".into(),
            status: SessionStatus::Completed,
            target_mastery_count: 1,
            total_questions: 1,
            actual_mastery_count: 1,
            context_shifts: 0,
            created_at: now - Duration::minutes(5),
            updated_at: now,
            summary: Some(SessionSummary {
                accuracy: 1.0,
                avg_response_time_ms: 100,
                mastered_word_ids: vec!["w1".into()],
                error_prone_word_ids: vec![],
                duration_secs: 60,
                hour_of_day: 9,
                final_difficulty: 0.5,
            }),
            correct_count: 1,
            total_count: 1,
        };
        store.create_learning_session(&session).unwrap();
        store.update_learning_session(&session).unwrap();
        seed_word_state(&store, "u1", "w1", WordState::Mastered, 0.9, None);

        let daily = store.admin_daily_study_overview(7, None).unwrap();
        assert!(daily.iter().any(|r| r.session_count >= 1));
        assert!(daily.iter().any(|r| r.mastered_words >= 1));
    }

    #[test]
    fn daily_record_type_counts() {
        let (_t, store) = test_store();
        user(&store, "u1");
        let now = Utc::now();
        seed_record(&store, "r1", "u1", "w1", RecordType::Learning, true, now);
        seed_record(&store, "r2", "u1", "w2", RecordType::Review, false, now);
        let rows = store.admin_daily_record_type_counts(7).unwrap();
        let total: i64 = rows.iter().map(|r| r.total).sum();
        assert_eq!(total, 2);
        assert!(rows
            .iter()
            .any(|r| r.record_type == "learning" && r.correct == 1));
        assert!(rows
            .iter()
            .any(|r| r.record_type == "review" && r.correct == 0));
    }

    #[test]
    fn word_state_distribution_counts_states_and_due() {
        let (_t, store) = test_store();
        user(&store, "u1");
        let now = Utc::now();
        seed_word_state(&store, "u1", "w-new", WordState::New, 0.0, None);
        seed_word_state(&store, "u1", "w-learn", WordState::Learning, 0.3, None);
        seed_word_state(
            &store,
            "u1",
            "w-rev",
            WordState::Reviewing,
            0.5,
            Some(now - Duration::hours(1)),
        );
        seed_word_state(&store, "u1", "w-mast", WordState::Mastered, 0.95, None);
        seed_word_state(&store, "u1", "w-forg", WordState::Forgotten, 0.1, None);
        store.upsert_word_favorite("u1", "w-new").unwrap();

        let d = store.admin_word_state_distribution(None).unwrap();
        assert_eq!(d.new_count, 1);
        assert_eq!(d.learning, 1);
        assert_eq!(d.reviewing, 1);
        assert_eq!(d.mastered, 1);
        assert_eq!(d.forgotten, 1);
        assert_eq!(d.bookmarked, 1);
        assert_eq!(d.due, 1);
        assert_eq!(d.overdue, 1);
        assert!(d.average_mastery.is_some());

        // 过滤模式：no learning_records → 全 0
        let filtered = store
            .admin_word_state_distribution(Some(RecordType::Learning))
            .unwrap();
        assert_eq!(
            filtered.new_count
                + filtered.learning
                + filtered.reviewing
                + filtered.mastered
                + filtered.forgotten,
            0
        );

        // 加 record 后部分恢复
        seed_record(&store, "r1", "u1", "w-new", RecordType::Learning, true, now);
        let filtered2 = store
            .admin_word_state_distribution(Some(RecordType::Learning))
            .unwrap();
        assert_eq!(filtered2.new_count, 1);
        assert_eq!(filtered2.bookmarked, 1);
    }

    #[test]
    fn retention_samples_filter_by_window_and_record_type() {
        let (_t, store) = test_store();
        user(&store, "u1");
        let now = Utc::now();
        seed_record(
            &store,
            "r1",
            "u1",
            "w1",
            RecordType::Learning,
            true,
            now - Duration::days(2),
        );
        seed_word_state(&store, "u1", "w1", WordState::Learning, 0.5, None);

        let samples = store.admin_retention_curve_samples(None, 7).unwrap();
        assert_eq!(samples.len(), 1);
        let s = &samples[0];
        assert!(s.half_life_hours.is_some());
        assert_eq!(s.total_attempts, 1);

        let filtered = store
            .admin_retention_curve_samples(Some(RecordType::Learning), 7)
            .unwrap();
        assert_eq!(filtered.len(), 1);
        let none_match = store
            .admin_retention_curve_samples(Some(RecordType::Review), 7)
            .unwrap();
        assert!(none_match.is_empty());
    }

    #[test]
    fn daily_registered_users_counts_today() {
        let (_t, store) = test_store();
        user(&store, "u1");
        let rows = store.admin_daily_registered_users(7).unwrap();
        let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();
        assert!(rows.iter().any(|r| r.date == today && r.registered == 1));
    }
}
