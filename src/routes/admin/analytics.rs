use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::admin_analytics::{
    AdminDailyRecordTypeRow, AdminDailyRegisteredUsersRow, AdminRetentionSampleRow,
    AdminStudyDailyRow, AdminStudySummaryRow,
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
}

impl From<AdminStudySummaryRow> for StudySummary {
    fn from(r: AdminStudySummaryRow) -> Self {
        Self {
            accuracy: accuracy(r.correct_count, r.record_count),
            total_duration_secs: r.total_duration_secs,
            session_count: r.session_count,
            record_count: r.record_count,
            correct_count: r.correct_count,
            new_words: r.new_words,
            review_words: r.review_words,
            mastered_words: r.mastered_words,
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
    let (summary, daily) = state
        .run_store_task("admin.analytics.study_overview", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.admin_study_overview_summary(days, category)?,
                store.admin_daily_study_overview(days, category)?,
            ))
        })
        .await??;

    Ok(ok(StudyOverviewResponse {
        generated_at: Utc::now(),
        days: q.days,
        category: category_label(category).to_string(),
        summary: summary.into(),
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
