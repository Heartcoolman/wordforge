//! Aggregate-across-all-users analytics queries for the admin dashboard.
//!
//! Each public method returns a tightly-scoped row type so the route layer
//! can assemble JSON without leaking SQL plumbing. All time inputs are
//! resolved to a UTC `YYYY-MM-DD` start-of-day boundary; `learning_records`
//! and `learning_sessions` columns store ISO-8601 strings, which compare
//! lexicographically to the date prefix used in `WHERE`.

use chrono::{DateTime, Datelike, Duration, Utc};
use rusqlite::params;
use serde::Serialize;

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

/// m022:/analytics/hourly 单元格(原始稀疏行,前端组装 7×24 矩阵)。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminHourlyBucketRow {
    pub dow: i32,
    pub hour: i32,
    pub count: i64,
}

/// m022:/analytics/wordbook-rank 单行。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminWordbookRankRow {
    pub wordbook_id: String,
    pub name: String,
    pub learner_count: i64,
    pub record_count: i64,
    pub correct_count: i64,
    pub accuracy: Option<f64>,
}

/// m022:/analytics/retention-cohort 矩阵单元(cohort_start, days_since, retained_users)。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminRetentionCohortRow {
    pub cohort_start: String,
    pub days_since: u32,
    pub retained_users: i64,
}

#[derive(Debug, Clone)]
pub struct AdminDailyRegisteredUsersRow {
    pub date: String,
    pub registered: i64,
}

// ──────────── analytics 数据看板 6 端点的 tightly-scoped 行类型 ────────────

/// kpi-summary 单窗口原始值(同窗口取一次,prev 窗口再取一次,route 算 delta)。
#[derive(Debug, Clone, Default)]
pub struct AdminKpiWindowRow {
    pub new_registrations: i64,
    pub study_duration_secs: i64,
    pub dau_average: i64,
    /// d7 留存基数:本窗口注册且注册满 7 天的用户数。
    pub d7_eligible: i64,
    /// d7 留存命中:上述用户中第 7 天 ±1 天有答题的数量。
    pub d7_retained: i64,
}

/// funnel 单窗口各步骤命中数(注册队列内)。
#[derive(Debug, Clone, Default)]
pub struct AdminFunnelRow {
    pub register: i64,
    pub choose_wordbook: i64,
    pub first_answer: i64,
    pub first_session: i64,
    pub d1_return: i64,
    pub d7_retention: i64,
    pub d30_retention: i64,
}

/// retention-matrix 单 cohort(注册周)行;active_by_week[k] = 第 k 周活跃用户数。
#[derive(Debug, Clone)]
pub struct AdminRetentionMatrixRow {
    pub cohort_start: String,
    pub size: i64,
    pub active_by_week: Vec<i64>,
}

/// question-distribution 单窗口聚合(question_modes 稀疏,difficulty_bins 固定 5 箱)。
#[derive(Debug, Clone, Default)]
pub struct AdminQuestionDistRow {
    pub total: i64,
    /// (question_mode 原始值, count);NULL 已折叠为 "" 由 route 映射"未标注"。
    pub question_modes: Vec<(String, i64)>,
    /// 5 箱计数:≤1000 / 1000–1200 / 1200–1400 / 1400–1600 / ≥1600。
    pub difficulty_bins: [i64; 5],
}

/// word-frequency 单词行(spelling=words.text,pos=words.part_of_speech)。
#[derive(Debug, Clone)]
pub struct AdminWordFreqRow {
    pub word_id: String,
    pub spelling: String,
    pub pos: Option<String>,
    pub record_count: i64,
    pub accuracy: Option<f64>,
    pub elo: Option<f64>,
    /// 掌握度:窗口内答到该词的全体用户 `word_learning_states.mastery_level` 均值
    /// (0..1),无任何 wls 行时为 NULL。供前端"掌握度"tab 排序与展示。
    pub mastery: Option<f64>,
}

/// Returns a `YYYY-MM-DD` string for `now - (days - 1)` (inclusive day).
/// `learning_records.created_at` / `learning_sessions.created_at` /
/// `users.created_at` are ISO-8601 UTC strings (`2026-04-25T08:00:00Z`),
/// which are lexicographically `>=` `'2026-04-25'`, so this string can be
/// bound directly to range predicates and the b-tree index is used.
fn window_since_date(days: u32) -> String {
    let today = Utc::now().date_naive();
    let start = today - Duration::days(days.saturating_sub(1) as i64);
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

    /// 上一等长窗口的 `(record_count, correct_count)`,供 study-overview 卡片算
    /// 答题数 / 正确率的环比。窗口 = `[today-(2*days-1), today-days)`(半开,紧邻
    /// 当前窗口往前推一个等长周期),与 `admin_study_overview_summary` 的当前窗口
    /// `[today-(days-1), today]` 不重叠。
    pub fn admin_study_summary_prev_window(
        &self,
        days: u32,
        record_type: Option<RecordType>,
    ) -> Result<(i64, i64), StoreError> {
        let conn = self.conn()?;
        let days = days.max(1);
        let today = Utc::now().date_naive();
        let prev_start = (today - Duration::days((2 * days - 1) as i64))
            .format("%Y-%m-%d")
            .to_string();
        // end_excl = 当前窗口 start = today-(days-1)
        let prev_end_excl = (today - Duration::days((days - 1) as i64))
            .format("%Y-%m-%d")
            .to_string();
        let (records, correct): (i64, i64) = match record_type {
            Some(rt) => conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(is_correct), 0)
                 FROM learning_records
                 WHERE created_at >= ?1 AND created_at < ?2 AND record_type = ?3",
                params![&prev_start, &prev_end_excl, rt.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?,
            None => conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(is_correct), 0)
                 FROM learning_records
                 WHERE created_at >= ?1 AND created_at < ?2",
                params![&prev_start, &prev_end_excl],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?,
        };
        Ok((records, correct))
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

    // ────────────────── m022:三个新 analytics 端点 ──────────────────

    /// 按 (day_of_week, hour_of_day) 聚合最近 N 天的答题计数。
    /// 返回原始稀疏行,前端组装 7×24 矩阵。
    /// strftime('%w') 返回 0-6(0=Sunday),strftime('%H') 返回 00-23。
    pub fn admin_hourly_buckets(&self, days: u32) -> Result<Vec<AdminHourlyBucketRow>, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        let mut stmt = conn.prepare(
            "SELECT
                CAST(strftime('%w', created_at) AS INTEGER) AS dow,
                CAST(strftime('%H', created_at) AS INTEGER) AS hour,
                COUNT(*) AS cnt
             FROM learning_records
             WHERE created_at >= ?1
             GROUP BY dow, hour
             ORDER BY dow, hour",
        )?;
        let rows = stmt.query_map(params![&since], |r| {
            Ok(AdminHourlyBucketRow {
                dow: r.get(0)?,
                hour: r.get(1)?,
                count: r.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// 词库使用排名:JOIN learning_records → wordbook_words → wordbooks,
    /// 按 record_count DESC 排,返回 top N。
    pub fn admin_wordbook_rank(
        &self,
        days: u32,
        limit: u32,
    ) -> Result<Vec<AdminWordbookRankRow>, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        let mut stmt = conn.prepare(
            "SELECT
                wb.id AS wordbook_id,
                wb.name AS wordbook_name,
                COUNT(DISTINCT lr.user_id) AS learner_count,
                COUNT(*) AS record_count,
                SUM(CASE WHEN lr.is_correct = 1 THEN 1 ELSE 0 END) AS correct_count
             FROM learning_records lr
             JOIN wordbook_words ww ON ww.word_id = lr.word_id
             JOIN wordbooks wb ON wb.id = ww.wordbook_id
             WHERE lr.created_at >= ?1
             GROUP BY wb.id, wb.name
             ORDER BY record_count DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![&since, limit as i64], |r| {
            let correct: i64 = r.get(4)?;
            let total: i64 = r.get(3)?;
            Ok(AdminWordbookRankRow {
                wordbook_id: r.get(0)?,
                name: r.get(1)?,
                learner_count: r.get(2)?,
                record_count: total,
                correct_count: correct,
                accuracy: if total > 0 {
                    Some(correct as f64 / total as f64)
                } else {
                    None
                },
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// cohort × daysSinceLearn 留存矩阵。cohort 维度:按 user 首次答题日期的
    /// "weekly bucket"(date(first_record, 'weekday 0'))分组,然后对每 bucket
    /// 内的用户算 1/3/7/14/30 天后是否仍在活跃。
    ///
    /// SQL 设计:
    ///   1. 子查询 1:每用户首次活跃日 first_day。
    ///   2. 子查询 2:每用户活跃日(去重 date)。
    ///   3. JOIN 后按 cohort_start(week of first_day)、days_since(active_day - first_day)聚合。
    pub fn admin_retention_cohort(
        &self,
        cohort_unit: &str,
        max_days: u32,
    ) -> Result<Vec<AdminRetentionCohortRow>, StoreError> {
        // 只支持 weekly / daily 两种 cohort 粒度
        let cohort_expr = match cohort_unit {
            "daily" => "DATE(first_day)",
            // 默认 weekly(date('YYYY-MM-DD', 'weekday 0') 拿到该日所在周的周日)
            _ => "DATE(first_day, 'weekday 0', '-6 days')",
        };
        let since = window_since_date(max_days * 2); // 留出 2 倍窗口供 cohort 形成
        let conn = self.conn()?;
        let sql = format!(
            "WITH user_first AS (
                SELECT user_id, MIN(DATE(created_at)) AS first_day
                FROM learning_records
                WHERE created_at >= ?1
                GROUP BY user_id
            ),
            user_active AS (
                SELECT DISTINCT user_id, DATE(created_at) AS active_day
                FROM learning_records
                WHERE created_at >= ?1
            )
            SELECT
                {cohort_expr} AS cohort_start,
                (julianday(active_day) - julianday(first_day)) AS days_since,
                COUNT(DISTINCT user_active.user_id) AS retained_users
            FROM user_active
            JOIN user_first USING (user_id)
            WHERE days_since BETWEEN 0 AND ?2
            GROUP BY cohort_start, days_since
            ORDER BY cohort_start, days_since"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![&since, max_days as i64], |r| {
            Ok(AdminRetentionCohortRow {
                cohort_start: r.get(0)?,
                days_since: r.get::<_, f64>(1)? as u32,
                retained_users: r.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
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

    // ──────────────────── analytics 数据看板 6 端点聚合层 ────────────────────
    //
    // 窗口语义:所有窗口型方法接受 `start`(含,`YYYY-MM-DD`)与 `end_excl`(不含,
    // = rangeEnd 次日 `YYYY-MM-DD`),谓词统一 `created_at >= start AND created_at <
    // end_excl`。这样既能用 `days`(由 route 推出 [today-(days-1), today]),也能用
    // 自定义 `from/to`(闭区间 → end_excl=to+1d),且全部走 created_at 字符串前缀
    // 比较命中 b-tree 索引。"上一等长窗口"由 route 把 start/end_excl 整体左移窗口
    // 长度后再次调用同方法实现,聚合层无需感知。

    /// kpi-summary 单窗口原始指标。dauAverage 在 route 层按"活跃天数 distinct 用户
    /// 数 / 窗口天数"取均值四舍五入;d7 留存这里只产出 cohort 基数与第 7 天回访数,
    /// 比例在 route 算。
    pub fn admin_kpi_window(
        &self,
        start: &str,
        end_excl: &str,
        window_days: i64,
    ) -> Result<AdminKpiWindowRow, StoreError> {
        let conn = self.conn()?;
        let mut out = AdminKpiWindowRow::default();

        // 新注册数。
        out.new_registrations = conn.query_row(
            "SELECT COUNT(*) FROM users WHERE created_at >= ?1 AND created_at < ?2",
            params![start, end_excl],
            |r| r.get(0),
        )?;

        // 学习时长合计(summary_duration_secs 求和,NULL 计 0)。
        out.study_duration_secs = conn.query_row(
            "SELECT COALESCE(SUM(summary_duration_secs), 0)
             FROM learning_sessions
             WHERE created_at >= ?1 AND created_at < ?2",
            params![start, end_excl],
            |r| r.get(0),
        )?;

        // 日活均值:窗口内每个有答题的"日历日"取 distinct 用户数,求和后除以窗口
        // 天数(含无活跃日,故除数固定为 window_days)。
        let active_user_days: i64 = conn.query_row(
            "SELECT COALESCE(SUM(c), 0) FROM (
                SELECT COUNT(DISTINCT user_id) AS c
                FROM learning_records
                WHERE created_at >= ?1 AND created_at < ?2
                GROUP BY DATE(created_at)
             )",
            params![start, end_excl],
            |r| r.get(0),
        )?;
        out.dau_average = if window_days > 0 {
            (active_user_days as f64 / window_days as f64).round() as i64
        } else {
            0
        };

        // d7 留存基数:本窗口注册、且注册满 7 天的用户;命中:注册后第 7 天 ±1 天
        // (即 [reg+6d, reg+8d) 半开)内有答题。比例在 route 层算 retained/eligible。
        // eligible = 注册日 <= now-7d 的本窗口注册用户。
        let now_date = Utc::now().date_naive().format("%Y-%m-%d").to_string();
        let (d7_eligible, d7_retained): (i64, i64) = conn.query_row(
            "WITH cohort AS (
                SELECT id AS user_id, DATE(created_at) AS reg_day
                FROM users
                WHERE created_at >= ?1 AND created_at < ?2
                  AND DATE(created_at, '+7 days') <= ?3
             )
             SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN EXISTS (
                    SELECT 1 FROM learning_records lr
                    WHERE lr.user_id = c.user_id
                      AND DATE(lr.created_at) >= DATE(c.reg_day, '+6 days')
                      AND DATE(lr.created_at) <  DATE(c.reg_day, '+8 days')
                ) THEN 1 ELSE 0 END), 0)
             FROM cohort c",
            params![start, end_excl, &now_date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        out.d7_eligible = d7_eligible;
        out.d7_retained = d7_retained;

        Ok(out)
    }

    /// funnel:统计"在 [start, end_excl) 注册"的用户队列各步骤命中数。
    /// 返回固定 7 元组(register..d30),pct/deltaPt/tone 由 route 组装。
    ///
    /// 精确口径(注册队列内):
    /// - register:队列基数(注册用户数)。
    /// - choose_wordbook:study_configs.selected_wordbook_ids_json 解析后非空数组
    ///   (用 json_array_length > 0;无 study_configs 行视为未选)。
    /// - first_answer:存在 ≥1 条 learning_records。
    /// - first_session:存在 ≥1 个 已完成(status='completed') learning_sessions 且
    ///   该会话答题数 ≥20(total_count/total_questions 取较大值近似"展示AMAS节奏")。
    /// - d1_return:注册满 1 天(reg+1d <= today)且 [reg+1d, reg+2d) 有答题(注册次日
    ///   有有效答题,近似"注册后24h内回访")。
    /// - d7_retention:注册满 7 天且 [reg+6d, reg+8d) 有答题(第7天±1天)。
    /// - d30_retention:注册满 30 天且 [reg+29d, reg+31d) 有答题(第30天±1天)。
    pub fn admin_funnel_window(
        &self,
        start: &str,
        end_excl: &str,
    ) -> Result<AdminFunnelRow, StoreError> {
        let conn = self.conn()?;
        let now_date = Utc::now().date_naive().format("%Y-%m-%d").to_string();
        let row = conn.query_row(
            "WITH cohort AS (
                SELECT id AS user_id, DATE(created_at) AS reg_day
                FROM users
                WHERE created_at >= ?1 AND created_at < ?2
             )
             SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN (
                    SELECT json_array_length(sc.selected_wordbook_ids_json)
                    FROM study_configs sc WHERE sc.user_id = c.user_id
                ) > 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN EXISTS (
                    SELECT 1 FROM learning_records lr WHERE lr.user_id = c.user_id
                ) THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN EXISTS (
                    SELECT 1 FROM learning_sessions ls
                    WHERE ls.user_id = c.user_id
                      AND ls.status = 'completed'
                      AND MAX(ls.total_count, ls.total_questions) >= 20
                ) THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN DATE(c.reg_day, '+1 days') <= ?3 AND EXISTS (
                    SELECT 1 FROM learning_records lr WHERE lr.user_id = c.user_id
                      AND DATE(lr.created_at) >= DATE(c.reg_day, '+1 days')
                      AND DATE(lr.created_at) <  DATE(c.reg_day, '+2 days')
                ) THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN DATE(c.reg_day, '+7 days') <= ?3 AND EXISTS (
                    SELECT 1 FROM learning_records lr WHERE lr.user_id = c.user_id
                      AND DATE(lr.created_at) >= DATE(c.reg_day, '+6 days')
                      AND DATE(lr.created_at) <  DATE(c.reg_day, '+8 days')
                ) THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN DATE(c.reg_day, '+30 days') <= ?3 AND EXISTS (
                    SELECT 1 FROM learning_records lr WHERE lr.user_id = c.user_id
                      AND DATE(lr.created_at) >= DATE(c.reg_day, '+29 days')
                      AND DATE(lr.created_at) <  DATE(c.reg_day, '+31 days')
                ) THEN 1 ELSE 0 END), 0)
             FROM cohort c",
            params![start, end_excl, &now_date],
            |r| {
                Ok(AdminFunnelRow {
                    register: r.get(0)?,
                    choose_wordbook: r.get(1)?,
                    first_answer: r.get(2)?,
                    first_session: r.get(3)?,
                    d1_return: r.get(4)?,
                    d7_retention: r.get(5)?,
                    d30_retention: r.get(6)?,
                })
            },
        )?;
        Ok(row)
    }

    /// retention-matrix:按"注册周(周一为周起点)"分组的 cohort 留存。返回最近
    /// `weeks` 个 cohort(升序);每 cohort 给出 size 与第 0..weeks-1 周仍活跃用户数。
    /// "第 k 周活跃" = [cohortStart+7k, cohortStart+7(k+1)) 内有答题。cells[0] 含
    /// 注册周本身,故恒等于 size(比例 1.0,route 层填)。未过完的周由 route 置 null。
    pub fn admin_retention_matrix(
        &self,
        weeks: u32,
    ) -> Result<Vec<AdminRetentionMatrixRow>, StoreError> {
        let conn = self.conn()?;
        let weeks = weeks.max(1);
        // 取最近 weeks 个注册周:从 (今天所在周一) 往前推 weeks-1 周。
        let today = Utc::now().date_naive();
        let dow_mon0 = today.weekday().num_days_from_monday() as i64; // 周一=0
        let this_monday = today - Duration::days(dow_mon0);
        let earliest_monday = this_monday - Duration::weeks((weeks - 1) as i64);
        let earliest_str = earliest_monday.format("%Y-%m-%d").to_string();

        // 每个 cohort 周一 → size(本周注册用户数)。SQLite weekday 0=周日,
        // DATE(d,'weekday 0','-6 days') 拿到该日所在周的周一。
        let mut stmt = conn.prepare(
            "SELECT DATE(created_at, 'weekday 0', '-6 days') AS wk, COUNT(*) AS n
             FROM users
             WHERE DATE(created_at) >= ?1
             GROUP BY wk
             ORDER BY wk",
        )?;
        let size_rows = stmt.query_map(params![&earliest_str], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut cohorts: Vec<AdminRetentionMatrixRow> = Vec::new();
        for r in size_rows {
            let (wk, n) = r?;
            cohorts.push(AdminRetentionMatrixRow {
                cohort_start: wk,
                size: n,
                active_by_week: vec![0; weeks as usize],
            });
        }

        // 整张矩阵一次算出:join users×learning_records,按 cohort 周一与
        // week_offset=(答题日−周一)/7 分组 COUNT(DISTINCT user)。口径与原逐格
        // 查询一致——周窗口 [周一+7k, 周一+7(k+1)),负偏移(注册前答题)被
        // week_offset>=0 排除,>=weeks 的越界周被裁掉。julianday 差取整后整除 7
        // 等价于原 DATE 半开区间的归桶(同一自然日落同一周)。
        let mut cell_stmt = conn.prepare(
            "SELECT cohort_monday, week_offset, COUNT(DISTINCT user_id)
             FROM (
                 SELECT
                     DATE(u.created_at, 'weekday 0', '-6 days') AS cohort_monday,
                     u.id AS user_id,
                     CAST((julianday(DATE(lr.created_at))
                           - julianday(DATE(u.created_at, 'weekday 0', '-6 days'))) / 7
                          AS INTEGER) AS week_offset
                 FROM users u
                 JOIN learning_records lr ON lr.user_id = u.id
                 WHERE DATE(u.created_at) >= ?1
             )
             WHERE week_offset >= 0 AND week_offset < ?2
             GROUP BY cohort_monday, week_offset",
        )?;
        let cells = cell_stmt.query_map(params![&earliest_str, weeks as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        // cohort_start → 行索引,把每格装配回原结构。
        let mut idx: std::collections::HashMap<String, usize> =
            std::collections::HashMap::with_capacity(cohorts.len());
        for (i, c) in cohorts.iter().enumerate() {
            idx.insert(c.cohort_start.clone(), i);
        }
        for cell in cells {
            let (monday, week_offset, n) = cell?;
            if let Some(&i) = idx.get(&monday) {
                let k = week_offset as usize;
                if k < weeks as usize {
                    cohorts[i].active_by_week[k] = n;
                }
            }
        }
        Ok(cohorts)
    }

    /// question-distribution:questionTypes 按 learning_records.question_mode 分组
    /// (运行时列;NULL/未知归"未标注");difficultyBins 按 word_elo.rating 分箱
    /// (LEFT JOIN,无评分 COALESCE 1200)。窗口同 kpi 半开区间。
    pub fn admin_question_distribution(
        &self,
        start: &str,
        end_excl: &str,
    ) -> Result<AdminQuestionDistRow, StoreError> {
        let conn = self.conn()?;
        let mut out = AdminQuestionDistRow::default();

        out.total = conn.query_row(
            "SELECT COUNT(*) FROM learning_records
             WHERE created_at >= ?1 AND created_at < ?2",
            params![start, end_excl],
            |r| r.get(0),
        )?;

        // questionTypes:question_mode 列由 records agent 本次新增,运行时存在即可。
        let mut stmt = conn.prepare(
            "SELECT question_mode, COUNT(*)
             FROM learning_records
             WHERE created_at >= ?1 AND created_at < ?2
             GROUP BY question_mode",
        )?;
        let rows = stmt.query_map(params![start, end_excl], |r| {
            Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
        })?;
        for r in rows {
            let (mode, count) = r?;
            out.question_modes.push((mode.unwrap_or_default(), count));
        }

        // difficultyBins:LEFT JOIN word_elo,COALESCE(rating, 1200) 分 5 箱。
        let mut bin_stmt = conn.prepare(
            "SELECT
                SUM(CASE WHEN rt <= 1000 THEN 1 ELSE 0 END),
                SUM(CASE WHEN rt > 1000 AND rt <= 1200 THEN 1 ELSE 0 END),
                SUM(CASE WHEN rt > 1200 AND rt <= 1400 THEN 1 ELSE 0 END),
                SUM(CASE WHEN rt > 1400 AND rt <  1600 THEN 1 ELSE 0 END),
                SUM(CASE WHEN rt >= 1600 THEN 1 ELSE 0 END)
             FROM (
                SELECT COALESCE(we.rating, 1200.0) AS rt
                FROM learning_records lr
                LEFT JOIN word_elo we ON we.word_id = lr.word_id
                WHERE lr.created_at >= ?1 AND lr.created_at < ?2
             )",
        )?;
        out.difficulty_bins = bin_stmt.query_row(params![start, end_excl], |r| {
            Ok([
                r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                r.get::<_, Option<i64>>(4)?.unwrap_or(0),
            ])
        })?;

        Ok(out)
    }

    /// word-frequency:窗口内按词聚合 recordCount / accuracy / elo,JOIN words 取
    /// 拼写(words.text)与词性(words.part_of_speech),LEFT JOIN word_elo 取 rating。
    /// `sort` ∈ count|accuracy|elo;limit 已由 route 夹到 1..=100。
    pub fn admin_word_frequency(
        &self,
        start: &str,
        end_excl: &str,
        sort: &str,
        limit: u32,
    ) -> Result<Vec<AdminWordFreqRow>, StoreError> {
        let conn = self.conn()?;
        let order_by = match sort {
            // 前端"错误率"tab:正确率升序 = 错误率最高(掌握最差)的词排最前
            "accuracy" => "accuracy ASC",
            "elo" => "elo DESC",
            // "掌握度"tab:掌握度升序(掌握最差的词排最前,与错误率同向定位薄弱词)。
            // NULL(无 wls)沉底,避免无数据词占据榜首。
            "mastery" => "mastery IS NULL, mastery ASC",
            _ => "record_count DESC",
        };
        // mastery:相关子查询,对窗口内答到该词的用户取 wls.mastery_level 均值。
        // 与主聚合的 (user_id, word_id) 口径一致(同窗口、同词)。
        let sql = format!(
            "SELECT
                lr.word_id,
                w.text,
                w.part_of_speech,
                COUNT(*) AS record_count,
                AVG(lr.is_correct) AS accuracy,
                we.rating AS elo,
                (SELECT AVG(wls.mastery_level)
                   FROM word_learning_states wls
                   WHERE wls.word_id = lr.word_id
                     AND wls.user_id IN (
                       SELECT DISTINCT lr2.user_id FROM learning_records lr2
                       WHERE lr2.word_id = lr.word_id
                         AND lr2.created_at >= ?1 AND lr2.created_at < ?2
                     )) AS mastery
             FROM learning_records lr
             JOIN words w ON w.id = lr.word_id
             LEFT JOIN word_elo we ON we.word_id = lr.word_id
             WHERE lr.created_at >= ?1 AND lr.created_at < ?2
             GROUP BY lr.word_id, w.text, w.part_of_speech, we.rating
             ORDER BY {order_by}
             LIMIT ?3"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![start, end_excl, limit as i64], |r| {
            Ok(AdminWordFreqRow {
                word_id: r.get(0)?,
                spelling: r.get(1)?,
                pos: r.get(2)?,
                record_count: r.get(3)?,
                accuracy: r.get(4)?,
                elo: r.get(5)?,
                mastery: r.get(6)?,
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
            role: "user".to_string(),
            status: "active".to_string(),
            last_login_at: None,
            referrer_source: None,
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
            question_mode: None,
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

    // 覆盖"今天"的 7 天半开窗口边界 [start, end_excl)。
    fn week_window() -> (String, String) {
        let today = Utc::now().date_naive();
        let start = (today - Duration::days(6)).format("%Y-%m-%d").to_string();
        let end_excl = (today + Duration::days(1)).format("%Y-%m-%d").to_string();
        (start, end_excl)
    }

    #[test]
    fn kpi_window_counts_window_registrations() {
        let (_t, store) = test_store();
        user(&store, "u1");
        user(&store, "u2");
        let (start, end_excl) = week_window();
        let row = store.admin_kpi_window(&start, &end_excl, 7).unwrap();
        assert_eq!(row.new_registrations, 2);
        // 刚注册的用户尚不满 7 天 → 不进入 d7 基数(避免把未成熟队列计入留存)
        assert_eq!(row.d7_eligible, 0);
        assert_eq!(row.d7_retained, 0);
    }

    #[test]
    fn funnel_counts_register_and_first_answer_only() {
        let (_t, store) = test_store();
        user(&store, "u1");
        user(&store, "u2");
        user(&store, "u3");
        // 仅 u1 有答题 → first_answer=1;无 study_configs/会话 → choose/session=0
        seed_record(&store, "r1", "u1", "w1", RecordType::All, true, Utc::now());
        let (start, end_excl) = week_window();
        let f = store.admin_funnel_window(&start, &end_excl).unwrap();
        assert_eq!(f.register, 3);
        assert_eq!(f.first_answer, 1);
        assert_eq!(f.choose_wordbook, 0);
        assert_eq!(f.first_session, 0);
    }

    #[test]
    fn retention_matrix_reports_size_and_week0_active() {
        let (_t, store) = test_store();
        user(&store, "u1");
        user(&store, "u2");
        // u1 本周有答题 → 第 0 周活跃=1;两人同周注册 → 单一 cohort,size=2
        seed_record(&store, "r1", "u1", "w1", RecordType::All, true, Utc::now());
        let rows = store.admin_retention_matrix(7).unwrap();
        let cohort = rows.last().expect("至少一个 cohort");
        assert_eq!(cohort.size, 2);
        assert_eq!(cohort.active_by_week.len(), 7);
        assert_eq!(cohort.active_by_week[0], 1);
    }

    #[test]
    fn question_distribution_groups_modes_and_folds_null() {
        let (_t, store) = test_store();
        user(&store, "u1");
        let now = Utc::now();
        let mk = |id: &str, mode: Option<&str>| LearningRecord {
            id: id.into(),
            user_id: "u1".into(),
            word_id: "w1".into(),
            is_correct: true,
            response_time_ms: 500,
            session_id: None,
            created_at: now,
            record_type: RecordType::All,
            self_rating: None,
            question_mode: mode.map(|s| s.to_string()),
        };
        store
            .create_record(&mk("a", Some("word-to-meaning")))
            .unwrap();
        store
            .create_record(&mk("b", Some("word-to-meaning")))
            .unwrap();
        store.create_record(&mk("c", None)).unwrap();
        let (start, end_excl) = week_window();
        let d = store
            .admin_question_distribution(&start, &end_excl)
            .unwrap();
        assert_eq!(d.total, 3);
        let wtm = d
            .question_modes
            .iter()
            .find(|(k, _)| k == "word-to-meaning")
            .map(|(_, c)| *c);
        assert_eq!(wtm, Some(2));
        // NULL question_mode 折叠为 "" 计 1
        let null_c = d
            .question_modes
            .iter()
            .find(|(k, _)| k.is_empty())
            .map(|(_, c)| *c);
        assert_eq!(null_c, Some(1));
        // 词无 word_elo → COALESCE 1200 入某一箱;3 条记录全部落箱
        assert_eq!(d.difficulty_bins.iter().sum::<i64>(), 3);
    }
}
