use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::admin_analytics::{
    AdminDailyRecordTypeRow, AdminDailyRegisteredUsersRow, AdminFunnelRow, AdminHourlyBucketRow,
    AdminKpiWindowRow, AdminQuestionDistRow, AdminRetentionCohortRow, AdminRetentionMatrixRow,
    AdminRetentionSampleRow, AdminStudyDailyRow, AdminStudySummaryRow, AdminWordFreqRow,
    AdminWordbookRankRow,
};
use crate::store::operations::records::RecordType;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/engagement", get(user_engagement))
        .route("/learning", get(learning_metrics))
        .route("/daily-active-users", get(daily_active_users))
        .route("/daily-records", get(daily_records))
        .route("/study-overview", get(study_overview))
        .route("/record-types", get(record_types))
        .route("/word-states", get(word_states))
        .route("/retention-curve", get(retention_curve))
        // m022:三个新端点(时段热图 / 词库排名 / cohort 留存矩阵)
        .route("/hourly", get(hourly_buckets))
        .route("/wordbook-rank", get(wordbook_rank))
        .route("/retention-cohort", get(retention_cohort))
        // 数据看板 6 端点(KPI / 漏斗 / 留存矩阵 / 题型难度分布 / 高频词 / 洞察)
        .route("/kpi-summary", get(kpi_summary))
        .route("/funnel", get(funnel))
        .route("/retention-matrix", get(retention_matrix))
        .route("/question-distribution", get(question_distribution))
        .route("/word-frequency", get(word_frequency))
        .route("/insights", get(insights))
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn calc_trend(today: usize, yesterday: usize) -> i64 {
    if yesterday == 0 {
        return 0;
    }
    ((today as f64 - yesterday as f64) / yesterday as f64 * 100.0).round() as i64
}

fn default_days() -> u32 {
    7
}

fn ensure_days(days: u32) -> Result<(), AppError> {
    if !(1..=30).contains(&days) {
        return Err(AppError::bad_request(
            "INVALID_DAYS",
            "days must be between 1 and 30",
        ));
    }
    Ok(())
}

fn parse_category(raw: Option<&str>) -> Result<Option<RecordType>, AppError> {
    match raw.unwrap_or("all") {
        "all" => Ok(None),
        "learning" => Ok(Some(RecordType::Learning)),
        "review" => Ok(Some(RecordType::Review)),
        other => Err(AppError::bad_request(
            "INVALID_CATEGORY",
            &format!("category must be all, learning, or review; got {other}"),
        )),
    }
}

fn category_label(category: Option<RecordType>) -> &'static str {
    match category {
        Some(rt) => rt.as_str(),
        None => "all",
    }
}

fn accuracy(correct: i64, total: i64) -> Option<f64> {
    if total > 0 {
        Some(correct as f64 / total as f64)
    } else {
        None
    }
}

fn enumerate_window(days: u32) -> impl Iterator<Item = String> {
    let today = Utc::now().date_naive();
    (0..days).map(move |i| {
        let d = today - chrono::Duration::days((days - 1 - i) as i64);
        d.format("%Y-%m-%d").to_string()
    })
}

#[derive(Debug, Deserialize)]
struct DaysQuery {
    #[serde(default = "default_days")]
    days: u32,
}

#[derive(Debug, Deserialize)]
struct CategoryDaysQuery {
    #[serde(default = "default_days")]
    days: u32,
    category: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CategoryQuery {
    category: Option<String>,
}

// ---------------------------------------------------------------------------
// Existing endpoints (extended additively)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DailyActiveUsersEntry {
    date: String,
    count: i64,
    registered: i64,
}

fn fill_active_entries(
    activity: &[(String, i64)],
    registrations: &[AdminDailyRegisteredUsersRow],
    days: u32,
) -> Vec<DailyActiveUsersEntry> {
    let mut active_map = std::collections::HashMap::new();
    for (date, count) in activity {
        active_map.insert(date.as_str(), *count);
    }
    let mut reg_map = std::collections::HashMap::new();
    for row in registrations {
        reg_map.insert(row.date.as_str(), row.registered);
    }
    enumerate_window(days)
        .map(|date| DailyActiveUsersEntry {
            count: active_map.get(date.as_str()).copied().unwrap_or(0),
            registered: reg_map.get(date.as_str()).copied().unwrap_or(0),
            date,
        })
        .collect()
}

async fn daily_active_users(
    _admin: AdminAuthUser,
    Query(q): Query<DaysQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    ensure_days(q.days)?;
    let days = q.days;
    let (activity, registrations) = state
        .run_store_task("admin.analytics.daily_active_users", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.daily_active_users(days)?,
                store.admin_daily_registered_users(days)?,
            ))
        })
        .await??;
    Ok(ok(fill_active_entries(&activity, &registrations, q.days)))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DailyRecordsEntry {
    date: String,
    correct: i64,
    total: i64,
    duration_secs: i64,
    new_words: i64,
}

fn fill_daily_records_entries(rows: &[AdminStudyDailyRow], days: u32) -> Vec<DailyRecordsEntry> {
    let mut map = std::collections::HashMap::new();
    for row in rows {
        map.insert(row.date.as_str(), row);
    }
    enumerate_window(days)
        .map(|date| {
            let row = map.get(date.as_str());
            DailyRecordsEntry {
                correct: row.map(|r| r.correct_count).unwrap_or(0),
                total: row.map(|r| r.record_count).unwrap_or(0),
                duration_secs: row.map(|r| r.duration_secs).unwrap_or(0),
                new_words: row.map(|r| r.new_words).unwrap_or(0),
                date,
            }
        })
        .collect()
}

async fn daily_records(
    _admin: AdminAuthUser,
    Query(q): Query<DaysQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    ensure_days(q.days)?;
    let days = q.days;
    let rows = state
        .run_store_task("admin.analytics.daily_records", move |store| {
            store.admin_daily_study_overview(days, None)
        })
        .await??;
    Ok(ok(fill_daily_records_entries(&rows, q.days)))
}

async fn user_engagement(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let day_start = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
        .unwrap_or_else(Utc::now);
    let today_str = Utc::now().date_naive().format("%Y-%m-%d").to_string();
    let yesterday_str = (Utc::now().date_naive() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let (total_users, active_today, active_today_count, active_yesterday) = state
        .run_store_task("admin.analytics.user_engagement", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.count_users()?,
                store.count_active_users_since(day_start)?,
                store.count_active_users_on_date(&today_str)?,
                store.count_active_users_on_date(&yesterday_str)?,
            ))
        })
        .await??;

    Ok(ok(serde_json::json!({
        "totalUsers": total_users,
        "activeToday": active_today,
        "retentionRate": if total_users > 0 { active_today as f64 / total_users as f64 } else { 0.0 },
        "trend": {
            "activeToday": { "value": calc_trend(active_today_count, active_yesterday), "label": "较昨日" }
        }
    })))
}

async fn learning_metrics(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let today_str = Utc::now().date_naive().format("%Y-%m-%d").to_string();
    let yesterday_str = (Utc::now().date_naive() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let (
        total_words,
        total_records,
        total_correct,
        records_today,
        records_yesterday,
        correct_today,
        correct_yesterday,
    ) = state
        .run_store_task("admin.analytics.learning_metrics", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.count_words()?,
                store.count_all_records()? as u64,
                store.count_all_correct_records()? as u64,
                store.count_records_on_date(&today_str)?,
                store.count_records_on_date(&yesterday_str)?,
                store.count_correct_records_on_date(&today_str)?,
                store.count_correct_records_on_date(&yesterday_str)?,
            ))
        })
        .await??;

    let acc_today = if records_today > 0 {
        (correct_today as f64 / records_today as f64 * 100.0).round() as usize
    } else {
        0
    };
    let acc_yesterday = if records_yesterday > 0 {
        (correct_yesterday as f64 / records_yesterday as f64 * 100.0).round() as usize
    } else {
        0
    };

    Ok(ok(serde_json::json!({
        "totalWords": total_words,
        "totalRecords": total_records,
        "totalCorrect": total_correct,
        "overallAccuracy": if total_records > 0 { total_correct as f64 / total_records as f64 } else { 0.0 },
        "trend": {
            "totalRecords": { "value": calc_trend(records_today, records_yesterday), "label": "较昨日" },
            "overallAccuracy": { "value": calc_trend(acc_today, acc_yesterday), "label": "较昨日" }
        }
    })))
}

// ---------------------------------------------------------------------------
// New endpoint: study-overview
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudySummary {
    total_duration_secs: i64,
    session_count: i64,
    record_count: i64,
    correct_count: i64,
    accuracy: Option<f64>,
    new_words: i64,
    review_words: i64,
    mastered_words: i64,
    /// 上一等长窗口的答题数(环比基数)。
    prev_record_count: i64,
    /// 上一等长窗口的正确率(0..1),无答题为 null。
    prev_accuracy: Option<f64>,
    /// 答题数环比变化率((cur-prev)/prev),prev=0 时 null。
    record_count_delta_pct: Option<f64>,
    /// 正确率环比变化(百分点,cur-prev),任一窗口无答题为 null。
    accuracy_delta_pt: Option<f64>,
}

impl StudySummary {
    /// 由当前窗口 summary + 上一窗口 `(record_count, correct_count)` 组装,算环比。
    fn from_rows(r: AdminStudySummaryRow, prev: (i64, i64)) -> Self {
        let (prev_records, prev_correct) = prev;
        let cur_acc = accuracy(r.correct_count, r.record_count);
        let prev_acc = accuracy(prev_correct, prev_records);
        Self {
            accuracy: cur_acc,
            total_duration_secs: r.total_duration_secs,
            session_count: r.session_count,
            record_count: r.record_count,
            correct_count: r.correct_count,
            new_words: r.new_words,
            review_words: r.review_words,
            mastered_words: r.mastered_words,
            prev_record_count: prev_records,
            prev_accuracy: prev_acc,
            record_count_delta_pct: if prev_records > 0 {
                Some((r.record_count - prev_records) as f64 / prev_records as f64)
            } else {
                None
            },
            accuracy_delta_pt: match (cur_acc, prev_acc) {
                (Some(c), Some(p)) => Some(c - p),
                _ => None,
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudyDaily {
    date: String,
    duration_secs: i64,
    session_count: i64,
    record_count: i64,
    correct_count: i64,
    accuracy: Option<f64>,
    new_words: i64,
    review_words: i64,
    mastered_words: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudyOverviewResponse {
    generated_at: DateTime<Utc>,
    days: u32,
    category: String,
    summary: StudySummary,
    daily: Vec<StudyDaily>,
}

fn fill_study_daily(rows: &[AdminStudyDailyRow], days: u32) -> Vec<StudyDaily> {
    let mut map = std::collections::HashMap::new();
    for row in rows {
        map.insert(row.date.as_str(), row);
    }
    enumerate_window(days)
        .map(|date| {
            let row = map.get(date.as_str());
            let total = row.map(|r| r.record_count).unwrap_or(0);
            let correct = row.map(|r| r.correct_count).unwrap_or(0);
            StudyDaily {
                duration_secs: row.map(|r| r.duration_secs).unwrap_or(0),
                session_count: row.map(|r| r.session_count).unwrap_or(0),
                record_count: total,
                correct_count: correct,
                accuracy: accuracy(correct, total),
                new_words: row.map(|r| r.new_words).unwrap_or(0),
                review_words: row.map(|r| r.review_words).unwrap_or(0),
                mastered_words: row.map(|r| r.mastered_words).unwrap_or(0),
                date,
            }
        })
        .collect()
}

async fn study_overview(
    _admin: AdminAuthUser,
    Query(q): Query<CategoryDaysQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    ensure_days(q.days)?;
    let category = parse_category(q.category.as_deref())?;
    let days = q.days;
    let (summary, prev, daily) = state
        .run_store_task("admin.analytics.study_overview", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.admin_study_overview_summary(days, category)?,
                store.admin_study_summary_prev_window(days, category)?,
                store.admin_daily_study_overview(days, category)?,
            ))
        })
        .await??;

    Ok(ok(StudyOverviewResponse {
        generated_at: Utc::now(),
        days: q.days,
        category: category_label(category).to_string(),
        summary: StudySummary::from_rows(summary, prev),
        daily: fill_study_daily(&daily, q.days),
    }))
}

// ---------------------------------------------------------------------------
// New endpoint: record-types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct RecordTypeDaily {
    date: String,
    learning: i64,
    review: i64,
    all: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordTypeTotal {
    record_type: String,
    total: i64,
    correct: i64,
    accuracy: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordTypesResponse {
    generated_at: DateTime<Utc>,
    days: u32,
    totals: Vec<RecordTypeTotal>,
    daily: Vec<RecordTypeDaily>,
}

fn fill_record_type_daily(rows: &[AdminDailyRecordTypeRow], days: u32) -> Vec<RecordTypeDaily> {
    let mut map = std::collections::HashMap::<String, RecordTypeDaily>::new();
    for row in rows {
        let entry = map
            .entry(row.date.clone())
            .or_insert_with(|| RecordTypeDaily {
                date: row.date.clone(),
                ..Default::default()
            });
        match row.record_type.as_str() {
            "learning" => entry.learning = row.total,
            "review" => entry.review = row.total,
            "all" => entry.all = row.total,
            _ => {}
        }
    }
    enumerate_window(days)
        .map(|date| {
            map.remove(&date).unwrap_or(RecordTypeDaily {
                date,
                ..Default::default()
            })
        })
        .collect()
}

fn aggregate_record_type_totals(rows: &[AdminDailyRecordTypeRow]) -> Vec<RecordTypeTotal> {
    let mut totals: std::collections::HashMap<&str, (i64, i64)> =
        [("learning", (0, 0)), ("review", (0, 0)), ("all", (0, 0))]
            .into_iter()
            .collect();
    for row in rows {
        if let Some(slot) = totals.get_mut(row.record_type.as_str()) {
            slot.0 += row.total;
            slot.1 += row.correct;
        }
    }
    ["learning", "review", "all"]
        .into_iter()
        .map(|key| {
            let (total, correct) = totals.get(key).copied().unwrap_or((0, 0));
            RecordTypeTotal {
                record_type: key.to_string(),
                total,
                correct,
                accuracy: accuracy(correct, total),
            }
        })
        .collect()
}

async fn record_types(
    _admin: AdminAuthUser,
    Query(q): Query<DaysQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    ensure_days(q.days)?;
    let days = q.days;
    let rows = state
        .run_store_task("admin.analytics.record_types", move |store| {
            store.admin_daily_record_type_counts(days)
        })
        .await??;
    Ok(ok(RecordTypesResponse {
        generated_at: Utc::now(),
        days: q.days,
        totals: aggregate_record_type_totals(&rows),
        daily: fill_record_type_daily(&rows, q.days),
    }))
}

// ---------------------------------------------------------------------------
// New endpoint: word-states
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordStateCounts {
    new_count: i64,
    learning: i64,
    reviewing: i64,
    mastered: i64,
    forgotten: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordStateTotals {
    tracked_words: i64,
    bookmarked_words: i64,
    due_review_words: i64,
    overdue_review_words: i64,
    average_mastery_level: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordStatesResponse {
    generated_at: DateTime<Utc>,
    category: String,
    states: WordStateCounts,
    totals: WordStateTotals,
}

async fn word_states(
    _admin: AdminAuthUser,
    Query(q): Query<CategoryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let category = parse_category(q.category.as_deref())?;
    let dist = state
        .run_store_task("admin.analytics.word_states", move |store| {
            store.admin_word_state_distribution(category)
        })
        .await??;

    let tracked = dist.new_count + dist.learning + dist.reviewing + dist.mastered + dist.forgotten;

    Ok(ok(WordStatesResponse {
        generated_at: Utc::now(),
        category: category_label(category).to_string(),
        states: WordStateCounts {
            new_count: dist.new_count,
            learning: dist.learning,
            reviewing: dist.reviewing,
            mastered: dist.mastered,
            forgotten: dist.forgotten,
        },
        totals: WordStateTotals {
            tracked_words: tracked,
            bookmarked_words: dist.bookmarked,
            due_review_words: dist.due,
            overdue_review_words: dist.overdue,
            average_mastery_level: dist.average_mastery,
        },
    }))
}

// ---------------------------------------------------------------------------
// New endpoint: retention-curve
// ---------------------------------------------------------------------------

const RETENTION_BUCKETS: &[u32] = &[1, 2, 4, 7, 15, 30];
/// 31-day window covers all bucket centres (max 30) plus a 1-day buffer.
const RETENTION_WINDOW_DAYS: u32 = 31;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RetentionPoint {
    days_since_learn: u32,
    retention: Option<f64>,
    sample_size: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RetentionCurveResponse {
    generated_at: DateTime<Utc>,
    category: String,
    points: Vec<RetentionPoint>,
    average_retention: Option<f64>,
}

fn nearest_bucket(days_since_learn: f64) -> Option<u32> {
    if days_since_learn < 0.5 {
        return None;
    }
    RETENTION_BUCKETS.iter().copied().min_by(|a, b| {
        (days_since_learn - *a as f64)
            .abs()
            .partial_cmp(&(days_since_learn - *b as f64).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// Same exponential half-life model as `src/routes/analytics.rs::estimated_retention`.
/// Pure aggregate over `word_learning_states.half_life` (hours); MDM `last_review_at_ms`
/// is preferred over `wls.updated_at` when available, matching the user-side endpoint.
fn estimate_retention(row: &AdminRetentionSampleRow, now: DateTime<Utc>) -> f64 {
    if row.total_attempts == 0 {
        return 1.0;
    }
    let last_review_at = row
        .mdm_last_review_at
        .or(row.state_updated_at)
        .unwrap_or(row.first_learned_at);
    let elapsed_hours = (now - last_review_at).num_seconds().max(0) as f64 / 3600.0;
    let half_life = row.half_life_hours.unwrap_or(24.0).max(0.01);
    2f64.powf(-elapsed_hours / half_life).clamp(0.0, 1.0)
}

async fn retention_curve(
    _admin: AdminAuthUser,
    Query(q): Query<CategoryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let category = parse_category(q.category.as_deref())?;
    let samples = state
        .run_store_task("admin.analytics.retention_curve", move |store| {
            store.admin_retention_curve_samples(category, RETENTION_WINDOW_DAYS)
        })
        .await??;

    let now = Utc::now();
    let mut buckets: std::collections::HashMap<u32, (f64, i64)> = std::collections::HashMap::new();
    let mut total_sum = 0.0;
    let mut total_count = 0i64;

    for sample in samples {
        let days_since_learn =
            (now - sample.first_learned_at).num_seconds().max(0) as f64 / 86_400.0;
        let Some(bucket) = nearest_bucket(days_since_learn) else {
            continue;
        };
        let retention = estimate_retention(&sample, now);
        let entry = buckets.entry(bucket).or_insert((0.0, 0));
        entry.0 += retention;
        entry.1 += 1;
        total_sum += retention;
        total_count += 1;
    }

    let points = RETENTION_BUCKETS
        .iter()
        .map(|&bucket| {
            let (sum, count) = buckets.get(&bucket).copied().unwrap_or((0.0, 0));
            RetentionPoint {
                days_since_learn: bucket,
                retention: if count > 0 {
                    Some(sum / count as f64)
                } else {
                    None
                },
                sample_size: count,
            }
        })
        .collect();

    Ok(ok(RetentionCurveResponse {
        generated_at: Utc::now(),
        category: category_label(category).to_string(),
        points,
        average_retention: if total_count > 0 {
            Some(total_sum / total_count as f64)
        } else {
            None
        },
    }))
}

// ---------------------------------------------------------------------------
// m022:三个新端点
// ---------------------------------------------------------------------------

/// `?days=` 拓宽到 1..=60(老接口最大 30 天,时段热图希望更长窗口看周环比)。
fn ensure_days_extended(days: u32) -> Result<(), AppError> {
    if !(1..=60).contains(&days) {
        return Err(AppError::bad_request(
            "INVALID_DAYS",
            "days must be between 1 and 60",
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HourlyResponse {
    generated_at: DateTime<Utc>,
    days: u32,
    /// 7×24 矩阵(行 dow=Sun..Sat,列 hour 0..23),前端直接渲染热图。
    /// 稀疏行在此被组装为稠密矩阵,缺失格填 0。
    matrix: Vec<Vec<i64>>,
    /// 全窗口总答题数,前端可显示在卡片副文。
    total: i64,
}

async fn hourly_buckets(
    _admin: AdminAuthUser,
    Query(q): Query<DaysQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    ensure_days_extended(q.days)?;
    let days = q.days;
    let rows: Vec<AdminHourlyBucketRow> = state
        .run_store_task("admin.analytics.hourly", move |store| {
            store.admin_hourly_buckets(days)
        })
        .await??;
    let mut matrix = vec![vec![0i64; 24]; 7];
    let mut total = 0i64;
    for row in rows {
        if (0..7).contains(&row.dow) && (0..24).contains(&row.hour) {
            matrix[row.dow as usize][row.hour as usize] = row.count;
            total += row.count;
        }
    }
    Ok(ok(HourlyResponse {
        generated_at: Utc::now(),
        days: q.days,
        matrix,
        total,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WordbookRankQuery {
    #[serde(default = "default_days_30")]
    days: u32,
    #[serde(default = "default_limit")]
    limit: u32,
}

fn default_days_30() -> u32 {
    30
}
fn default_limit() -> u32 {
    10
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordbookRankResponse {
    generated_at: DateTime<Utc>,
    days: u32,
    limit: u32,
    rows: Vec<AdminWordbookRankRow>,
}

async fn wordbook_rank(
    _admin: AdminAuthUser,
    Query(q): Query<WordbookRankQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    ensure_days_extended(q.days)?;
    let limit = q.limit.clamp(1, 100);
    let days = q.days;
    let rows: Vec<AdminWordbookRankRow> = state
        .run_store_task("admin.analytics.wordbook_rank", move |store| {
            store.admin_wordbook_rank(days, limit)
        })
        .await??;
    Ok(ok(WordbookRankResponse {
        generated_at: Utc::now(),
        days: q.days,
        limit,
        rows,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetentionCohortQuery {
    /// "weekly"(默认)或 "daily"。daily 在用户量大时矩阵巨大,前端可降为按行 slicing。
    #[serde(default)]
    cohort: Option<String>,
    #[serde(default = "default_cohort_max_days")]
    max_days: u32,
}

fn default_cohort_max_days() -> u32 {
    30
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RetentionCohortResponse {
    generated_at: DateTime<Utc>,
    cohort_unit: String,
    max_days: u32,
    /// 稀疏行:每条 (cohort_start, days_since, retained_users)。
    /// 前端按 cohort_start 分组组装二维矩阵。
    rows: Vec<AdminRetentionCohortRow>,
}

async fn retention_cohort(
    _admin: AdminAuthUser,
    Query(q): Query<RetentionCohortQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !(1..=90).contains(&q.max_days) {
        return Err(AppError::bad_request(
            "INVALID_MAX_DAYS",
            "max_days must be between 1 and 90",
        ));
    }
    let cohort_unit = q.cohort.as_deref().unwrap_or("weekly").to_string();
    if !matches!(cohort_unit.as_str(), "weekly" | "daily") {
        return Err(AppError::bad_request(
            "INVALID_COHORT_UNIT",
            "cohort must be 'weekly' or 'daily'",
        ));
    }
    let max_days = q.max_days;
    let cohort_for_store = cohort_unit.clone();
    let rows: Vec<AdminRetentionCohortRow> = state
        .run_store_task("admin.analytics.retention_cohort", move |store| {
            store.admin_retention_cohort(&cohort_for_store, max_days)
        })
        .await??;
    Ok(ok(RetentionCohortResponse {
        generated_at: Utc::now(),
        cohort_unit,
        max_days: q.max_days,
        rows,
    }))
}

// ===========================================================================
// 数据看板 6 端点
// ===========================================================================

/// 解析出的窗口:`[start, end]` 闭区间(`YYYY-MM-DD`),`end_excl` = end 次日(供
/// `created_at < end_excl` 半开比较),`days` = 窗口天数;另带"上一等长窗口"
/// `[prev_start, prev_end_excl)`(紧邻在 start 之前、等长往前推)。
struct Window {
    start: String,
    end: String,
    end_excl: String,
    days: i64,
    prev_start: String,
    prev_end_excl: String,
}

#[derive(Debug, Deserialize)]
struct WindowQuery {
    #[serde(default = "default_days")]
    days: u32,
    from: Option<String>,
    to: Option<String>,
}

const ALLOWED_WINDOW_DAYS: [u32; 4] = [7, 14, 30, 90];

fn parse_ymd(s: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| AppError::bad_request("INVALID_DATE", "from/to must be YYYY-MM-DD"))
}

/// from&to 都给时优先(闭区间);否则用 days(默认 7,仅允许 7/14/30/90),
/// 窗口 = [today-(days-1), today]。
fn resolve_window(q: &WindowQuery) -> Result<Window, AppError> {
    let (start_d, end_d) = match (q.from.as_deref(), q.to.as_deref()) {
        (Some(f), Some(t)) => {
            let (f, t) = (parse_ymd(f)?, parse_ymd(t)?);
            if f > t {
                return Err(AppError::bad_request("INVALID_RANGE", "from must be <= to"));
            }
            (f, t)
        }
        _ => {
            if !ALLOWED_WINDOW_DAYS.contains(&q.days) {
                return Err(AppError::bad_request(
                    "INVALID_DAYS",
                    "days must be one of 7, 14, 30, 90",
                ));
            }
            let today = Utc::now().date_naive();
            (today - Duration::days((q.days - 1) as i64), today)
        }
    };
    let days = (end_d - start_d).num_days() + 1;
    let prev_end_excl = start_d;
    let prev_start = start_d - Duration::days(days);
    Ok(Window {
        start: start_d.format("%Y-%m-%d").to_string(),
        end: end_d.format("%Y-%m-%d").to_string(),
        end_excl: (end_d + Duration::days(1)).format("%Y-%m-%d").to_string(),
        days,
        prev_start: prev_start.format("%Y-%m-%d").to_string(),
        prev_end_excl: prev_end_excl.format("%Y-%m-%d").to_string(),
    })
}

fn delta_pct(cur: f64, prev: f64) -> Option<f64> {
    if prev == 0.0 {
        None
    } else {
        Some((cur - prev) / prev)
    }
}

// ---------------------------------------------------------------------------
// 1) kpi-summary
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KpiMetricPct {
    value: f64,
    prev_value: f64,
    delta_pct: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KpiMetricPt {
    value: f64,
    prev_value: f64,
    delta_pt: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KpiSummaryResponse {
    generated_at: DateTime<Utc>,
    days: i64,
    range_start: String,
    range_end: String,
    new_registrations: KpiMetricPct,
    dau_average: KpiMetricPct,
    d7_retention: KpiMetricPt,
    study_duration_secs: KpiMetricPct,
}

fn d7_ratio(row: &AdminKpiWindowRow) -> f64 {
    if row.d7_eligible > 0 {
        row.d7_retained as f64 / row.d7_eligible as f64
    } else {
        0.0
    }
}

async fn kpi_summary(
    _admin: AdminAuthUser,
    Query(q): Query<WindowQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let w = resolve_window(&q)?;
    let (cur, prev) = {
        let (s, e, pd, ps, pe) = (
            w.start.clone(),
            w.end_excl.clone(),
            w.days,
            w.prev_start.clone(),
            w.prev_end_excl.clone(),
        );
        state
            .run_store_task("admin.analytics.kpi_summary", move |store| {
                Ok::<_, crate::store::StoreError>((
                    store.admin_kpi_window(&s, &e, pd)?,
                    store.admin_kpi_window(&ps, &pe, pd)?,
                ))
            })
            .await??
    };

    Ok(ok(KpiSummaryResponse {
        generated_at: Utc::now(),
        days: w.days,
        range_start: w.start,
        range_end: w.end,
        new_registrations: KpiMetricPct {
            value: cur.new_registrations as f64,
            prev_value: prev.new_registrations as f64,
            delta_pct: delta_pct(cur.new_registrations as f64, prev.new_registrations as f64),
        },
        dau_average: KpiMetricPct {
            value: cur.dau_average as f64,
            prev_value: prev.dau_average as f64,
            delta_pct: delta_pct(cur.dau_average as f64, prev.dau_average as f64),
        },
        d7_retention: {
            let (c, p) = (d7_ratio(&cur), d7_ratio(&prev));
            KpiMetricPt {
                value: c,
                prev_value: p,
                // 上一窗口无 d7-eligible 用户 → 无可比基准,置 null
                // (与其它 pct 指标零分母语义一致;前端按 null 渲染占位符)
                delta_pt: if prev.d7_eligible > 0 { Some(c - p) } else { None },
            }
        },
        study_duration_secs: KpiMetricPct {
            value: cur.study_duration_secs as f64,
            prev_value: prev.study_duration_secs as f64,
            delta_pct: delta_pct(
                cur.study_duration_secs as f64,
                prev.study_duration_secs as f64,
            ),
        },
    }))
}

// ---------------------------------------------------------------------------
// 2) funnel
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FunnelStep {
    key: String,
    label: String,
    sublabel: String,
    count: i64,
    pct: f64,
    delta_pt: Option<f64>,
    tone: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FunnelResponse {
    generated_at: DateTime<Utc>,
    days: i64,
    range_start: String,
    range_end: String,
    steps: Vec<FunnelStep>,
    biggest_drop_from: String,
    biggest_drop_to: String,
    biggest_drop_pt: f64,
}

/// 步骤静态定义:(key, label, sublabel, tone),与契约逐字对齐。
const FUNNEL_DEFS: [(&str, &str, &str, &str); 7] = [
    ("register", "注册", "邮箱验证完成", "good"),
    ("choose_wordbook", "选择词库", "至少选一个词库", "good"),
    ("first_answer", "首次答题", "完成第一道题", "good"),
    (
        "first_session",
        "完成首个会话(≥20题)",
        "展示AMAS节奏价值",
        "normal",
    ),
    ("d1_return", "d1 回访", "注册次日有有效答题", "normal"),
    ("d7_retention", "d7 留存", "注册第7天有有效答题", "normal"),
    ("d30_retention", "d30 留存", "稳态期", "warn"),
];

fn funnel_counts(row: &AdminFunnelRow) -> [i64; 7] {
    [
        row.register,
        row.choose_wordbook,
        row.first_answer,
        row.first_session,
        row.d1_return,
        row.d7_retention,
        row.d30_retention,
    ]
}

fn build_funnel_steps(cur: &AdminFunnelRow, prev: &AdminFunnelRow) -> Vec<FunnelStep> {
    let cur_c = funnel_counts(cur);
    let prev_c = funnel_counts(prev);
    let base = cur_c[0] as f64;
    let prev_base = prev_c[0] as f64;
    FUNNEL_DEFS
        .iter()
        .enumerate()
        .map(|(i, (key, label, sublabel, tone))| {
            let pct = if base > 0.0 { cur_c[i] as f64 / base } else { 0.0 };
            let prev_pct = if prev_base > 0.0 {
                Some(prev_c[i] as f64 / prev_base)
            } else {
                None
            };
            FunnelStep {
                key: key.to_string(),
                label: label.to_string(),
                sublabel: sublabel.to_string(),
                count: cur_c[i],
                pct,
                delta_pt: prev_pct.map(|p| pct - p),
                tone: tone.to_string(),
            }
        })
        .collect()
}

async fn funnel(
    _admin: AdminAuthUser,
    Query(q): Query<WindowQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let w = resolve_window(&q)?;
    let (cur, prev) = {
        let (s, e, ps, pe) = (
            w.start.clone(),
            w.end_excl.clone(),
            w.prev_start.clone(),
            w.prev_end_excl.clone(),
        );
        state
            .run_store_task("admin.analytics.funnel", move |store| {
                Ok::<_, crate::store::StoreError>((
                    store.admin_funnel_window(&s, &e)?,
                    store.admin_funnel_window(&ps, &pe)?,
                ))
            })
            .await??
    };

    let steps = build_funnel_steps(&cur, &prev);
    let (from, to, drop) = biggest_drop(&steps);
    Ok(ok(FunnelResponse {
        generated_at: Utc::now(),
        days: w.days,
        range_start: w.start,
        range_end: w.end,
        steps,
        biggest_drop_from: from,
        biggest_drop_to: to,
        biggest_drop_pt: drop,
    }))
}

/// 相邻步骤 pct 降幅最大者;返回 (from_label, to_label, drop_pt)。
fn biggest_drop(steps: &[FunnelStep]) -> (String, String, f64) {
    let mut best = (String::new(), String::new(), 0.0_f64);
    for pair in steps.windows(2) {
        let drop = pair[0].pct - pair[1].pct;
        if drop > best.2 {
            best = (pair[0].label.clone(), pair[1].label.clone(), drop);
        }
    }
    best
}

// ---------------------------------------------------------------------------
// 3) retention-matrix
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RetentionMatrixQuery {
    #[serde(default = "default_weeks")]
    weeks: u32,
}

fn default_weeks() -> u32 {
    7
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MatrixCohort {
    cohort_start: String,
    size: i64,
    cells: Vec<Option<f64>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RetentionMatrixResponse {
    generated_at: DateTime<Utc>,
    weeks: u32,
    cohorts: Vec<MatrixCohort>,
}

async fn retention_matrix(
    _admin: AdminAuthUser,
    Query(q): Query<RetentionMatrixQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let weeks = q.weeks.clamp(1, 26);
    let rows: Vec<AdminRetentionMatrixRow> = state
        .run_store_task("admin.analytics.retention_matrix", move |store| {
            store.admin_retention_matrix(weeks)
        })
        .await??;

    let now = Utc::now().date_naive();
    let cohorts = rows
        .into_iter()
        .map(|r| {
            let cohort_d = NaiveDate::parse_from_str(&r.cohort_start, "%Y-%m-%d").ok();
            let cells = (0..weeks as usize)
                .map(|k| {
                    // cells[0] 恒 1.0(注册周本身即基准)。
                    if k == 0 {
                        return Some(1.0);
                    }
                    // 第 k 周窗口 [cohortStart+7k, cohortStart+7(k+1));若窗口尾未过完
                    // (> now)则该 cell = null。
                    let week_end = cohort_d.map(|d| d + Duration::days(7 * (k as i64 + 1)));
                    match week_end {
                        Some(end) if end > now => None,
                        _ => {
                            if r.size > 0 {
                                Some(r.active_by_week[k] as f64 / r.size as f64)
                            } else {
                                Some(0.0)
                            }
                        }
                    }
                })
                .collect();
            MatrixCohort {
                cohort_start: r.cohort_start,
                size: r.size,
                cells,
            }
        })
        .collect();

    Ok(ok(RetentionMatrixResponse {
        generated_at: Utc::now(),
        weeks,
        cohorts,
    }))
}

// ---------------------------------------------------------------------------
// 4) question-distribution
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestionTypeEntry {
    key: String,
    label: String,
    count: i64,
    pct: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DifficultyBinEntry {
    label: String,
    min: Option<f64>,
    max: Option<f64>,
    count: i64,
    pct: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestionDistResponse {
    generated_at: DateTime<Utc>,
    days: i64,
    total_records: i64,
    question_types: Vec<QuestionTypeEntry>,
    difficulty_bins: Vec<DifficultyBinEntry>,
}

/// question_mode 原始值 → (key, label)。NULL/空/未知归 "未标注"。
fn question_type_label(raw: &str) -> (&'static str, &'static str) {
    match raw {
        "word-to-meaning" => ("word-to-meaning", "单词 → 释义"),
        "meaning-to-word" => ("meaning-to-word", "释义 → 单词"),
        "audio-to-meaning" => ("audio-to-meaning", "听音 → 释义"),
        "meaning-to-spelling" => ("meaning-to-spelling", "拼写"),
        _ => ("unknown", "未标注"),
    }
}

async fn question_distribution(
    _admin: AdminAuthUser,
    Query(q): Query<WindowQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let w = resolve_window(&q)?;
    let dist: AdminQuestionDistRow = {
        let (s, e) = (w.start.clone(), w.end_excl.clone());
        state
            .run_store_task("admin.analytics.question_distribution", move |store| {
                store.admin_question_distribution(&s, &e)
            })
            .await??
    };

    let total = dist.total;
    let pct = |c: i64| if total > 0 { c as f64 / total as f64 } else { 0.0 };

    // 合并同 label(unknown 折叠"未标注")并按 count 降序。
    let mut by_key: std::collections::BTreeMap<&str, (&str, i64)> = std::collections::BTreeMap::new();
    for (raw, count) in &dist.question_modes {
        let (key, label) = question_type_label(raw);
        let e = by_key.entry(key).or_insert((label, 0));
        e.1 += count;
    }
    let mut question_types: Vec<QuestionTypeEntry> = by_key
        .into_iter()
        .map(|(key, (label, count))| QuestionTypeEntry {
            key: key.to_string(),
            label: label.to_string(),
            count,
            pct: pct(count),
        })
        .collect();
    question_types.sort_by(|a, b| b.count.cmp(&a.count));

    let bin_meta: [(&str, Option<f64>, Option<f64>); 5] = [
        ("≤1000", None, Some(1000.0)),
        ("1000–1200", Some(1000.0), Some(1200.0)),
        ("1200–1400", Some(1200.0), Some(1400.0)),
        ("1400–1600", Some(1400.0), Some(1600.0)),
        ("≥1600", Some(1600.0), None),
    ];
    let difficulty_bins = bin_meta
        .iter()
        .enumerate()
        .map(|(i, (label, min, max))| DifficultyBinEntry {
            label: label.to_string(),
            min: *min,
            max: *max,
            count: dist.difficulty_bins[i],
            pct: pct(dist.difficulty_bins[i]),
        })
        .collect();

    Ok(ok(QuestionDistResponse {
        generated_at: Utc::now(),
        days: w.days,
        total_records: total,
        question_types,
        difficulty_bins,
    }))
}

// ---------------------------------------------------------------------------
// 5) word-frequency
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct WordFreqQuery {
    #[serde(default = "default_days")]
    days: u32,
    from: Option<String>,
    to: Option<String>,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default = "default_sort")]
    sort: String,
}

fn default_sort() -> String {
    "count".to_string()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordFreqRowOut {
    rank: i64,
    word_id: String,
    spelling: String,
    pos: Option<String>,
    record_count: i64,
    accuracy: Option<f64>,
    elo: Option<f64>,
    /// 掌握度(0..1),窗口内答该词的用户 mastery_level 均值;无数据为 null。
    mastery: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordFreqResponse {
    generated_at: DateTime<Utc>,
    days: i64,
    limit: u32,
    sort: String,
    rows: Vec<WordFreqRowOut>,
}

async fn word_frequency(
    _admin: AdminAuthUser,
    Query(q): Query<WordFreqQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let w = resolve_window(&WindowQuery {
        days: q.days,
        from: q.from.clone(),
        to: q.to.clone(),
    })?;
    let sort = match q.sort.as_str() {
        "count" | "accuracy" | "elo" | "mastery" => q.sort.clone(),
        _ => {
            return Err(AppError::bad_request(
                "INVALID_SORT",
                "sort must be count, accuracy, elo, or mastery",
            ))
        }
    };
    let limit = q.limit.clamp(1, 100);
    let rows: Vec<AdminWordFreqRow> = {
        let (s, e, srt) = (w.start.clone(), w.end_excl.clone(), sort.clone());
        state
            .run_store_task("admin.analytics.word_frequency", move |store| {
                store.admin_word_frequency(&s, &e, &srt, limit)
            })
            .await??
    };

    let rows = rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| WordFreqRowOut {
            rank: i as i64 + 1,
            word_id: r.word_id,
            spelling: r.spelling,
            pos: r.pos,
            record_count: r.record_count,
            accuracy: r.accuracy,
            elo: r.elo,
            mastery: r.mastery,
        })
        .collect();

    Ok(ok(WordFreqResponse {
        generated_at: Utc::now(),
        days: w.days,
        limit,
        sort,
        rows,
    }))
}

// ---------------------------------------------------------------------------
// 6) insights
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InsightItem {
    tone: String,
    title: String,
    body: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InsightsResponse {
    generated_at: DateTime<Utc>,
    days: i64,
    items: Vec<InsightItem>,
}

const WEEKDAY_CN: [&str; 7] = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];

async fn insights(
    _admin: AdminAuthUser,
    Query(q): Query<WindowQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let w = resolve_window(&q)?;
    let days_u32 = w.days.clamp(1, 60) as u32;

    let (funnel_cur, funnel_prev, matrix, freq, hourly) = {
        let (s, e, ps, pe) = (
            w.start.clone(),
            w.end_excl.clone(),
            w.prev_start.clone(),
            w.prev_end_excl.clone(),
        );
        state
            .run_store_task("admin.analytics.insights", move |store| {
                Ok::<_, crate::store::StoreError>((
                    store.admin_funnel_window(&s, &e)?,
                    store.admin_funnel_window(&ps, &pe)?,
                    store.admin_retention_matrix(8)?,
                    store.admin_word_frequency(&s, &e, "count", 50)?,
                    store.admin_hourly_buckets(days_u32)?,
                ))
            })
            .await??
    };

    let mut items: Vec<InsightItem> = Vec::new();

    // (1) accent:漏斗最大流失点。
    let steps = build_funnel_steps(&funnel_cur, &funnel_prev);
    let (from, to, drop) = biggest_drop(&steps);
    if drop > 0.0 && !from.is_empty() {
        items.push(InsightItem {
            tone: "accent".into(),
            title: "漏斗最大流失点".into(),
            body: format!(
                "{from} → {to} 流失 {:.0}%,是当前转化的最大瓶颈。",
                drop * 100.0
            ),
        });
    }

    // (2) success/warning:d7 留存近两周趋势(取最近两个"成熟"cohort 的第 1 周留存)。
    //     成熟 = cells[1] 非 null。比较较新 vs 较旧。
    let mature: Vec<(&str, f64)> = matrix
        .iter()
        .filter(|c| c.size > 0)
        .filter_map(|c| {
            // 第 1 周窗口需已过完;用 size>0 且至少 14 天前的 cohort 近似(此处直接
            // 用 active_by_week[1]/size,route 不重复 null 判定,取存在的最近两个)。
            let cohort_d = NaiveDate::parse_from_str(&c.cohort_start, "%Y-%m-%d").ok()?;
            let week1_end = cohort_d + Duration::days(14);
            if week1_end <= Utc::now().date_naive() && c.active_by_week.len() > 1 {
                Some((
                    c.cohort_start.as_str(),
                    c.active_by_week[1] as f64 / c.size as f64,
                ))
            } else {
                None
            }
        })
        .collect();
    if mature.len() >= 2 {
        let older = mature[mature.len() - 2].1;
        let newer = mature[mature.len() - 1].1;
        let diff = newer - older;
        let (tone, word) = if diff >= 0.0 {
            ("success", "回升")
        } else {
            ("warning", "下滑")
        };
        items.push(InsightItem {
            tone: tone.into(),
            title: "d7 留存趋势".into(),
            body: format!(
                "最近两个成熟队列的第 1 周留存从 {:.0}% {word}到 {:.0}%({:+.0} 个百分点)。",
                older * 100.0,
                newer * 100.0,
                diff * 100.0
            ),
        });
    }

    // (3) warning:高频低正确率词(recordCount 高且 accuracy < 0.5),列 1-2 个。
    let mut low_acc: Vec<&AdminWordFreqRow> = freq
        .iter()
        .filter(|r| r.record_count >= 5 && r.accuracy.map(|a| a < 0.5).unwrap_or(false))
        .collect();
    low_acc.sort_by(|a, b| b.record_count.cmp(&a.record_count));
    if !low_acc.is_empty() {
        let desc = low_acc
            .iter()
            .take(2)
            .map(|r| {
                format!(
                    "{}(正确率 {:.0}%、{} 次)",
                    r.spelling,
                    r.accuracy.unwrap_or(0.0) * 100.0,
                    r.record_count
                )
            })
            .collect::<Vec<_>>()
            .join("、");
        items.push(InsightItem {
            tone: "warning".into(),
            title: "高频易错词".into(),
            body: format!("{desc} 高频出现但正确率偏低,建议检查题面或加强讲解。"),
        });
    }

    // (4) info:时段峰值(7×24 矩阵最大单元)。dow:0=周日。
    let mut peak: Option<(usize, usize, i64)> = None;
    for row in &hourly {
        if (0..7).contains(&row.dow) && (0..24).contains(&row.hour) {
            let cur = (row.dow as usize, row.hour as usize, row.count);
            if peak.map(|p| cur.2 > p.2).unwrap_or(true) {
                peak = Some(cur);
            }
        }
    }
    if let Some((dow, hour, count)) = peak {
        if count > 0 {
            items.push(InsightItem {
                tone: "info".into(),
                title: "活跃时段峰值".into(),
                body: format!(
                    "{} {hour:02}:00 时段最活跃,共 {count} 次答题,可作为推送/活动的黄金窗口。",
                    WEEKDAY_CN[dow]
                ),
            });
        }
    }

    Ok(ok(InsightsResponse {
        generated_at: Utc::now(),
        days: w.days,
        items,
    }))
}
