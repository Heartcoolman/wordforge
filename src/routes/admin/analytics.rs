use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/engagement", get(user_engagement))
        .route("/learning", get(learning_metrics))
        .route("/daily-active-users", get(daily_active_users))
        .route("/daily-records", get(daily_records))
}

fn calc_trend(today: usize, yesterday: usize) -> i64 {
    if yesterday == 0 {
        return 0;
    }
    ((today as f64 - yesterday as f64) / yesterday as f64 * 100.0).round() as i64
}

fn default_days() -> u32 {
    7
}

#[derive(Debug, Deserialize)]
struct DaysQuery {
    #[serde(default = "default_days")]
    days: u32,
}

fn fill_dates_active(
    data: &[(String, i64)],
    days: u32,
) -> Vec<serde_json::Value> {
    let mut map = std::collections::HashMap::new();
    for (date, count) in data {
        map.insert(date.as_str(), *count);
    }
    let today = chrono::Utc::now().date_naive();
    (0..days)
        .map(|i| {
            let d = today - chrono::Duration::days((days - 1 - i) as i64);
            let ds = d.format("%Y-%m-%d").to_string();
            let count = map.get(ds.as_str()).copied().unwrap_or(0);
            serde_json::json!({ "date": ds, "count": count })
        })
        .collect()
}

fn fill_dates_records(
    data: &[(String, i64, i64)],
    days: u32,
) -> Vec<serde_json::Value> {
    let mut map = std::collections::HashMap::new();
    for (date, total, correct) in data {
        map.insert(date.as_str(), (*total, *correct));
    }
    let today = chrono::Utc::now().date_naive();
    (0..days)
        .map(|i| {
            let d = today - chrono::Duration::days((days - 1 - i) as i64);
            let ds = d.format("%Y-%m-%d").to_string();
            let (total, correct) = map.get(ds.as_str()).copied().unwrap_or((0, 0));
            serde_json::json!({ "date": ds, "correct": correct, "total": total })
        })
        .collect()
}

async fn daily_active_users(
    _admin: AdminAuthUser,
    Query(q): Query<DaysQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !(1..=30).contains(&q.days) {
        return Err(AppError::bad_request(
            "INVALID_DAYS",
            "days must be between 1 and 30",
        ));
    }
    let raw = state.store().daily_active_users(q.days)?;
    let result = fill_dates_active(&raw, q.days);
    Ok(ok(result))
}

async fn daily_records(
    _admin: AdminAuthUser,
    Query(q): Query<DaysQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !(1..=30).contains(&q.days) {
        return Err(AppError::bad_request(
            "INVALID_DAYS",
            "days must be between 1 and 30",
        ));
    }
    let raw = state.store().daily_records(q.days)?;
    let result = fill_dates_records(&raw, q.days);
    Ok(ok(result))
}

async fn user_engagement(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let total_users = state.store().count_users()?;

    let day_start = chrono::Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|dt| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);
    let active_today = state.store().count_active_users_since(day_start)?;

    let today_str = chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string();
    let yesterday_str = (chrono::Utc::now().date_naive() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let active_today_count = state.store().count_active_users_on_date(&today_str)?;
    let active_yesterday = state.store().count_active_users_on_date(&yesterday_str)?;

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
    let total_words = state.store().count_words()?;
    let total_records = state.store().count_all_records()? as u64;
    let total_correct = state.store().count_all_correct_records()? as u64;

    let today_str = chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string();
    let yesterday_str = (chrono::Utc::now().date_naive() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();

    let records_today = state.store().count_records_on_date(&today_str)?;
    let records_yesterday = state.store().count_records_on_date(&yesterday_str)?;
    let correct_today = state.store().count_correct_records_on_date(&today_str)?;
    let correct_yesterday = state.store().count_correct_records_on_date(&yesterday_str)?;

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
