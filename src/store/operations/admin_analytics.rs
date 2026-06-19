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

// ────────────────── 监控大屏:学习类聚合返回类型 ──────────────────

/// 答题响应时延分箱 + P50/P95(分箱近似)。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminResponseTimeDistribution {
    /// 各分箱计数:0-500 / 500-1000 / 1000-2000 / >2000 ms。
    pub bin_0_500: i64,
    pub bin_500_1000: i64,
    pub bin_1000_2000: i64,
    pub bin_2000_plus: i64,
    pub total: i64,
    /// 中位数/95 分位(ms),无数据为 None。
    pub p50_ms: Option<i64>,
    pub p95_ms: Option<i64>,
}

/// 按题型分组的首答正确率行。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminFirstAttemptByModeRow {
    pub question_mode: String,
    pub first_attempts: i64,
    pub correct: i64,
    pub accuracy: Option<f64>,
}

/// 首答正确率整体 + 按题型分组。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminFirstAttemptAccuracy {
    pub total_first_attempts: i64,
    pub total_correct: i64,
    pub overall_accuracy: Option<f64>,
    pub by_mode: Vec<AdminFirstAttemptByModeRow>,
}

/// 会话状态分布行(active/completed/abandoned)。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminSessionStatusRow {
    pub status: String,
    pub session_count: i64,
    /// 占全体会话比例。
    pub share: f64,
    /// 该状态平均时长(秒),NULL 计 0。
    pub avg_duration_secs: Option<f64>,
}

/// 会话状态汇总:各状态明细 + 完成率/放弃率。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminSessionStatusStats {
    pub total_sessions: i64,
    pub completion_rate: Option<f64>,
    pub abandon_rate: Option<f64>,
    pub by_status: Vec<AdminSessionStatusRow>,
}

/// 自评(self_rating 0-3)× 是否答对 的分布行。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminSelfRatingRow {
    pub self_rating: i64,
    pub count: i64,
    pub correct: i64,
    pub accuracy: Option<f64>,
    /// 占有 self_rating 记录总数比例。
    pub share: f64,
}

/// per-word 正确率二次分箱行(<50 / 50-70 / 70-90 / >90%)。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminWordAccuracyBinRow {
    pub bin: String,
    pub word_count: i64,
    pub avg_elo: Option<f64>,
    pub user_count: i64,
}

/// 题型 × ELO 难度分箱 二维矩阵格。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminQuestionDifficultyCell {
    pub question_mode: String,
    /// ELO 分箱标签(如 "<1000")。
    pub elo_bin: String,
    pub count: i64,
    pub accuracy: Option<f64>,
}

/// 词库学习统计行(含正确率/掌握度/学习人数/新词占比)。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminWordbookLearningRow {
    pub wordbook_id: String,
    pub name: String,
    pub word_count: i64,
    /// 该词库下有学习状态的词条聚合人次。
    pub user_count: i64,
    pub accuracy: Option<f64>,
    pub mastery_avg: Option<f64>,
    /// NEW 状态占比。
    pub new_words_pct: Option<f64>,
}

/// 掌握度分布行:state × mastery_level 分箱。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminMasteryDistributionRow {
    pub state: String,
    /// mastery 分箱标签(0-25 / 25-50 / 50-75 / 75-100)。
    pub mastery_bin: String,
    pub count: i64,
}

/// 连续学习天数分布桶。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminConsecutiveStudyDays {
    /// 桶:1-7 / 8-14 / 15-30 / >30。
    pub bucket_1_7: i64,
    pub bucket_8_14: i64,
    pub bucket_15_30: i64,
    pub bucket_30_plus: i64,
    pub user_count: i64,
    pub max_streak: i64,
}

/// 高峰时段热力图格:(hour, 10 分钟桶) 计数。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPeakTimeCell {
    pub hour: i64,
    /// 10 分钟桶索引 0..=5(对应 0-9/10-19/.../50-59 分钟)。
    pub minute_bucket: i64,
    pub count: i64,
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

    // ────────────────── 监控大屏:学习类聚合查询 ──────────────────

    /// 答题响应时延分箱(0-500/500-1000/1000-2000/>2000 ms)+ P50/P95。
    /// P50/P95 用 SQL 百分位:按 response_time_ms 排序后取偏移行。
    pub fn admin_response_time_distribution(
        &self,
        days: u32,
    ) -> Result<AdminResponseTimeDistribution, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        let mut stmt = conn.prepare(
            "SELECT
                SUM(CASE WHEN response_time_ms < 500 THEN 1 ELSE 0 END) AS b0,
                SUM(CASE WHEN response_time_ms >= 500 AND response_time_ms < 1000 THEN 1 ELSE 0 END) AS b1,
                SUM(CASE WHEN response_time_ms >= 1000 AND response_time_ms < 2000 THEN 1 ELSE 0 END) AS b2,
                SUM(CASE WHEN response_time_ms >= 2000 THEN 1 ELSE 0 END) AS b3,
                COUNT(*) AS total
             FROM learning_records
             WHERE created_at >= ?1",
        )?;
        let (b0, b1, b2, b3, total) = stmt.query_row(params![&since], |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                r.get::<_, i64>(4)?,
            ))
        })?;

        // P50/P95:OFFSET = floor(p * (total-1)),按时延升序取该行。
        let percentile = |conn: &rusqlite::Connection, p: f64| -> Result<Option<i64>, StoreError> {
            if total == 0 {
                return Ok(None);
            }
            let offset = ((p * (total - 1) as f64).floor()) as i64;
            let v: i64 = conn.query_row(
                "SELECT response_time_ms FROM learning_records
                 WHERE created_at >= ?1
                 ORDER BY response_time_ms ASC
                 LIMIT 1 OFFSET ?2",
                params![&since, offset],
                |r| r.get(0),
            )?;
            Ok(Some(v))
        };
        let p50_ms = percentile(&conn, 0.50)?;
        let p95_ms = percentile(&conn, 0.95)?;

        Ok(AdminResponseTimeDistribution {
            bin_0_500: b0,
            bin_500_1000: b1,
            bin_1000_2000: b2,
            bin_2000_plus: b3,
            total,
            p50_ms,
            p95_ms,
        })
    }

    /// 首答正确率:CTE 取每 (user_id, word_id) 最早 created_at 的首答记录,
    /// 算总体首答正确率 + 按 question_mode 分组正确率(NULL 题型归 'unknown')。
    pub fn admin_first_attempt_accuracy(
        &self,
        days: u32,
    ) -> Result<AdminFirstAttemptAccuracy, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        // first_attempts:每 (user_id, word_id) 取窗口内最早一条(created_at,id 决胜)。
        let cte = "WITH ranked AS (
                SELECT user_id, word_id, is_correct, question_mode,
                    ROW_NUMBER() OVER (
                        PARTITION BY user_id, word_id
                        ORDER BY created_at ASC, id ASC
                    ) AS rn
                FROM learning_records
                WHERE created_at >= ?1
            ),
            first_attempts AS (SELECT * FROM ranked WHERE rn = 1)";

        let overall_sql = format!(
            "{cte}
             SELECT COUNT(*), SUM(CASE WHEN is_correct = 1 THEN 1 ELSE 0 END)
             FROM first_attempts"
        );
        let (total_first_attempts, total_correct) =
            conn.query_row(&overall_sql, params![&since], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                ))
            })?;

        let by_mode_sql = format!(
            "{cte}
             SELECT COALESCE(question_mode, 'unknown') AS qm,
                    COUNT(*) AS cnt,
                    SUM(CASE WHEN is_correct = 1 THEN 1 ELSE 0 END) AS correct
             FROM first_attempts
             GROUP BY qm
             ORDER BY cnt DESC"
        );
        let mut stmt = conn.prepare(&by_mode_sql)?;
        let by_mode = stmt
            .query_map(params![&since], |r| {
                let cnt: i64 = r.get(1)?;
                let correct: i64 = r.get::<_, Option<i64>>(2)?.unwrap_or(0);
                Ok(AdminFirstAttemptByModeRow {
                    question_mode: r.get(0)?,
                    first_attempts: cnt,
                    correct,
                    accuracy: if cnt > 0 {
                        Some(correct as f64 / cnt as f64)
                    } else {
                        None
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(AdminFirstAttemptAccuracy {
            total_first_attempts,
            total_correct,
            overall_accuracy: if total_first_attempts > 0 {
                Some(total_correct as f64 / total_first_attempts as f64)
            } else {
                None
            },
            by_mode,
        })
    }

    /// 会话状态统计:GROUP BY learning_sessions.status,算各状态计数、
    /// 平均时长(summary_duration_secs,NULL 计 0)+ 完成率/放弃率。
    pub fn admin_session_status_stats(
        &self,
        days: u32,
    ) -> Result<AdminSessionStatusStats, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        let mut stmt = conn.prepare(
            "SELECT
                status,
                COUNT(*) AS cnt,
                AVG(COALESCE(summary_duration_secs, 0)) AS avg_dur
             FROM learning_sessions
             WHERE created_at >= ?1
             GROUP BY status",
        )?;
        let raw = stmt
            .query_map(params![&since], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<f64>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let total_sessions: i64 = raw.iter().map(|(_, c, _)| *c).sum();
        let completed: i64 = raw
            .iter()
            .filter(|(s, _, _)| s == "completed")
            .map(|(_, c, _)| *c)
            .sum();
        let abandoned: i64 = raw
            .iter()
            .filter(|(s, _, _)| s == "abandoned")
            .map(|(_, c, _)| *c)
            .sum();

        let by_status = raw
            .into_iter()
            .map(|(status, cnt, avg_dur)| AdminSessionStatusRow {
                status,
                session_count: cnt,
                share: if total_sessions > 0 {
                    cnt as f64 / total_sessions as f64
                } else {
                    0.0
                },
                avg_duration_secs: avg_dur,
            })
            .collect();

        Ok(AdminSessionStatusStats {
            total_sessions,
            completion_rate: if total_sessions > 0 {
                Some(completed as f64 / total_sessions as f64)
            } else {
                None
            },
            abandon_rate: if total_sessions > 0 {
                Some(abandoned as f64 / total_sessions as f64)
            } else {
                None
            },
            by_status,
        })
    }

    /// 自评分布:GROUP BY self_rating(忽略 NULL),交叉 is_correct,
    /// 每档 count/correct/accuracy/share。
    pub fn admin_self_rating_distribution(
        &self,
        days: u32,
    ) -> Result<Vec<AdminSelfRatingRow>, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        let mut stmt = conn.prepare(
            "SELECT
                self_rating,
                COUNT(*) AS cnt,
                SUM(CASE WHEN is_correct = 1 THEN 1 ELSE 0 END) AS correct
             FROM learning_records
             WHERE created_at >= ?1 AND self_rating IS NOT NULL
             GROUP BY self_rating
             ORDER BY self_rating ASC",
        )?;
        let raw = stmt
            .query_map(params![&since], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let total: i64 = raw.iter().map(|(_, c, _)| *c).sum();
        Ok(raw
            .into_iter()
            .map(|(self_rating, count, correct)| AdminSelfRatingRow {
                self_rating,
                count,
                correct,
                accuracy: if count > 0 {
                    Some(correct as f64 / count as f64)
                } else {
                    None
                },
                share: if total > 0 {
                    count as f64 / total as f64
                } else {
                    0.0
                },
            })
            .collect())
    }

    /// per-word 正确率二次分箱(<50/50-70/70-90/>90%):内层算每词正确率/人数/
    /// elo(word_elo.rating),外层按正确率分箱统计 wordCount/avgElo/userCount。
    pub fn admin_word_accuracy_bins(&self) -> Result<Vec<AdminWordAccuracyBinRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "WITH per_word AS (
                SELECT
                    lr.word_id,
                    AVG(lr.is_correct) AS acc,
                    COUNT(DISTINCT lr.user_id) AS users,
                    we.rating AS elo
                FROM learning_records lr
                LEFT JOIN word_elo we ON we.word_id = lr.word_id
                GROUP BY lr.word_id, we.rating
            ),
            binned AS (
                SELECT
                    CASE
                        WHEN acc < 0.5 THEN '<50%'
                        WHEN acc < 0.7 THEN '50-70%'
                        WHEN acc < 0.9 THEN '70-90%'
                        ELSE '>90%'
                    END AS bin,
                    elo,
                    users
                FROM per_word
            )
            SELECT bin,
                   COUNT(*) AS word_count,
                   AVG(elo) AS avg_elo,
                   SUM(users) AS user_count
            FROM binned
            GROUP BY bin",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AdminWordAccuracyBinRow {
                    bin: r.get(0)?,
                    word_count: r.get(1)?,
                    avg_elo: r.get(2)?,
                    user_count: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // 固定桶序输出(缺桶补 0)。
        let order = ["<50%", "50-70%", "70-90%", ">90%"];
        let mut out: Vec<AdminWordAccuracyBinRow> = Vec::with_capacity(4);
        for label in order {
            match rows.iter().find(|r| r.bin == label) {
                Some(found) => out.push(AdminWordAccuracyBinRow {
                    bin: found.bin.clone(),
                    word_count: found.word_count,
                    avg_elo: found.avg_elo,
                    user_count: found.user_count,
                }),
                None => out.push(AdminWordAccuracyBinRow {
                    bin: label.to_string(),
                    word_count: 0,
                    avg_elo: None,
                    user_count: 0,
                }),
            }
        }
        Ok(out)
    }

    /// 题型 × ELO 难度分箱 二维矩阵:question_mode(NULL→'unknown')× word_elo.rating
    /// 分箱(<1000/1000-1200/1200-1400/>1400),每格 count/accuracy。
    pub fn admin_question_difficulty_matrix(
        &self,
        days: u32,
    ) -> Result<Vec<AdminQuestionDifficultyCell>, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        let mut stmt = conn.prepare(
            "SELECT
                COALESCE(lr.question_mode, 'unknown') AS qm,
                CASE
                    WHEN we.rating IS NULL THEN 'unknown'
                    WHEN we.rating < 1000 THEN '<1000'
                    WHEN we.rating < 1200 THEN '1000-1200'
                    WHEN we.rating < 1400 THEN '1200-1400'
                    ELSE '>1400'
                END AS elo_bin,
                COUNT(*) AS cnt,
                AVG(lr.is_correct) AS accuracy
             FROM learning_records lr
             LEFT JOIN word_elo we ON we.word_id = lr.word_id
             WHERE lr.created_at >= ?1
             GROUP BY qm, elo_bin
             ORDER BY qm, elo_bin",
        )?;
        let rows = stmt
            .query_map(params![&since], |r| {
                Ok(AdminQuestionDifficultyCell {
                    question_mode: r.get(0)?,
                    elo_bin: r.get(1)?,
                    count: r.get(2)?,
                    accuracy: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 词库学习统计:JOIN wordbook_words → word_learning_states,
    /// 每词库算 accuracy(learning_records)/mastery_avg/user_count/new_words_pct。
    pub fn admin_wordbook_learning_stats(
        &self,
    ) -> Result<Vec<AdminWordbookLearningRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT
                wb.id,
                wb.name,
                COUNT(DISTINCT ww.word_id) AS word_count,
                COUNT(DISTINCT wls.user_id) AS user_count,
                AVG(wls.mastery_level) AS mastery_avg,
                AVG(CASE WHEN wls.state = 'NEW' THEN 1.0 ELSE 0.0 END) AS new_pct,
                (SELECT AVG(lr.is_correct)
                   FROM learning_records lr
                   JOIN wordbook_words ww2 ON ww2.word_id = lr.word_id
                   WHERE ww2.wordbook_id = wb.id) AS accuracy
             FROM wordbooks wb
             JOIN wordbook_words ww ON ww.wordbook_id = wb.id
             LEFT JOIN word_learning_states wls ON wls.word_id = ww.word_id
             GROUP BY wb.id, wb.name
             ORDER BY word_count DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AdminWordbookLearningRow {
                    wordbook_id: r.get(0)?,
                    name: r.get(1)?,
                    word_count: r.get(2)?,
                    user_count: r.get(3)?,
                    mastery_avg: r.get(4)?,
                    new_words_pct: r.get(5)?,
                    accuracy: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 掌握度分布:GROUP BY word_learning_states.state × mastery_level 分箱
    /// (0-25/25-50/50-75/75-100,mastery_level 范围 0..1 故 ×100)。
    pub fn admin_mastery_distribution(
        &self,
    ) -> Result<Vec<AdminMasteryDistributionRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT
                state,
                CASE
                    WHEN mastery_level * 100 < 25 THEN '0-25'
                    WHEN mastery_level * 100 < 50 THEN '25-50'
                    WHEN mastery_level * 100 < 75 THEN '50-75'
                    ELSE '75-100'
                END AS mastery_bin,
                COUNT(*) AS cnt
             FROM word_learning_states
             GROUP BY state, mastery_bin
             ORDER BY state, mastery_bin",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AdminMasteryDistributionRow {
                    state: r.get(0)?,
                    mastery_bin: r.get(1)?,
                    count: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 连续学习天数:每 user 计当前连续学习天数(以最近活跃日为锚,
    /// 向前逐日回溯无间断的活跃日数),分布桶 1-7/8-14/15-30/>30。
    /// 活跃日 = learning_records 去重 date(created_at)。
    pub fn admin_consecutive_study_days(&self) -> Result<AdminConsecutiveStudyDays, StoreError> {
        let conn = self.conn()?;
        // 取每 user 的去重活跃日(升序),内存按用户分组算最大尾部连续段。
        let mut stmt = conn.prepare(
            "SELECT user_id, date(created_at) AS d
             FROM learning_records
             GROUP BY user_id, d
             ORDER BY user_id, d ASC",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        use std::collections::HashMap;
        // user_id -> 升序活跃日列表。
        let mut by_user: HashMap<String, Vec<chrono::NaiveDate>> = HashMap::new();
        for (uid, d) in rows {
            if let Ok(nd) = chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d") {
                by_user.entry(uid).or_default().push(nd);
            }
        }

        let mut b1 = 0i64;
        let mut b2 = 0i64;
        let mut b3 = 0i64;
        let mut b4 = 0i64;
        let mut user_count = 0i64;
        let mut max_streak = 0i64;
        for (_, days_vec) in by_user {
            if days_vec.is_empty() {
                continue;
            }
            user_count += 1;
            // 最近活跃日往前数无间断段长度。
            let mut streak = 1i64;
            for i in (1..days_vec.len()).rev() {
                let prev = days_vec[i - 1];
                let cur = days_vec[i];
                if (cur - prev).num_days() == 1 {
                    streak += 1;
                } else {
                    break;
                }
            }
            if streak > max_streak {
                max_streak = streak;
            }
            match streak {
                1..=7 => b1 += 1,
                8..=14 => b2 += 1,
                15..=30 => b3 += 1,
                _ => b4 += 1,
            }
        }

        Ok(AdminConsecutiveStudyDays {
            bucket_1_7: b1,
            bucket_8_14: b2,
            bucket_15_30: b3,
            bucket_30_plus: b4,
            user_count,
            max_streak,
        })
    }

    /// 高峰时段热力图:窗口内按 (hour, 10 分钟桶) 聚合 created_at 计数。
    /// strftime('%H') 取小时,strftime('%M') 取分钟,整除 10 得 0..=5 桶。
    pub fn admin_peak_time_minute_heatmap(
        &self,
        days: u32,
    ) -> Result<Vec<AdminPeakTimeCell>, StoreError> {
        let conn = self.conn()?;
        let since = window_since_date(days);
        let mut stmt = conn.prepare(
            "SELECT
                CAST(strftime('%H', created_at) AS INTEGER) AS hour,
                CAST(strftime('%M', created_at) AS INTEGER) / 10 AS minute_bucket,
                COUNT(*) AS cnt
             FROM learning_records
             WHERE created_at >= ?1
             GROUP BY hour, minute_bucket
             ORDER BY hour, minute_bucket",
        )?;
        let rows = stmt
            .query_map(params![&since], |r| {
                Ok(AdminPeakTimeCell {
                    hour: r.get(0)?,
                    minute_bucket: r.get(1)?,
                    count: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

// ===========================================================================
// Reward-type distribution / chronotype segments (Users 看板 mock 替换)
// ===========================================================================

/// chronotype-segments 每段聚合的中间累计行（store 层 derive 后返回，handler 算均值/占比）。
#[derive(Debug, Clone, Default)]
pub struct AdminChronotypeSegmentRow {
    /// "morning" | "evening" | "neutral"
    pub seg: String,
    pub users: i64,
    /// SUM(user_stats.correct_records) / SUM(user_stats.total_records);分母 0 → None。
    pub correct_sum: i64,
    pub total_sum: i64,
    /// SUM(habit_profiles.sessions_per_day) —— handler 除以 users 得均值。
    pub sessions_per_day_sum: f64,
    /// SUM(median_session_length_mins * 60) —— handler 除以 users 得 durationSecsAvg。
    pub duration_secs_sum: f64,
}

impl Store {
    /// reward-distribution：按 reward_type 分布(LEFT JOIN users，无偏好行的活跃用户隐式计 'standard')。
    /// reward_type 为自由 TEXT 列(无 CHECK)，返回真实存在的取值；handler 计算 pct。
    /// 镜像 `admin_question_distribution` 的简单 GROUP BY + Rust pct 模式。
    pub fn admin_reward_distribution(&self) -> Result<Vec<(String, i64)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT COALESCE(rp.reward_type, 'standard') AS t, COUNT(*) AS n
             FROM users u
             LEFT JOIN reward_preferences rp ON rp.user_id = u.id
             WHERE u.is_banned = 0
             GROUP BY t
             ORDER BY n DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// chronotype-segments：habit_profiles LEFT JOIN user_stats，按 preferred_hours_json 的
    /// 均值 derive 三段(mean<11→morning，mean>=18→evening，else neutral)，每段累计计数/正确数/
    /// 总答题数/sessions_per_day/时长。chronotype 非存储列，全部在 Rust 端从 JSON 派生。
    /// habit_profiles 为空时返回三段全零行。
    pub fn admin_chronotype_segments(&self) -> Result<Vec<AdminChronotypeSegmentRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT hp.preferred_hours_json, hp.sessions_per_day, hp.median_session_length_mins,
                    COALESCE(us.correct_records, 0), COALESCE(us.total_records, 0)
             FROM habit_profiles hp
             LEFT JOIN user_stats us ON us.user_id = hp.user_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // 固定三段顺序，缺数据段保留零行。
        let mut segs: [AdminChronotypeSegmentRow; 3] = [
            AdminChronotypeSegmentRow {
                seg: "morning".to_string(),
                ..Default::default()
            },
            AdminChronotypeSegmentRow {
                seg: "evening".to_string(),
                ..Default::default()
            },
            AdminChronotypeSegmentRow {
                seg: "neutral".to_string(),
                ..Default::default()
            },
        ];

        for (hours_json, spd, median_mins, correct, total) in rows {
            let hours: Vec<f64> = serde_json::from_str(&hours_json).unwrap_or_default();
            // 空数组无法判型 → 归 neutral(索引 2)。
            let idx = if hours.is_empty() {
                2
            } else {
                let mean = hours.iter().sum::<f64>() / hours.len() as f64;
                if mean < 11.0 {
                    0
                } else if mean >= 18.0 {
                    1
                } else {
                    2
                }
            };
            let s = &mut segs[idx];
            s.users += 1;
            s.correct_sum += correct;
            s.total_sum += total;
            s.sessions_per_day_sum += spd;
            s.duration_secs_sum += median_mins * 60.0;
        }

        Ok(segs.into_iter().collect())
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
