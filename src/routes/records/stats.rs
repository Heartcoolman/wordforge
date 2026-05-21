use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::records::RecordType;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/statistics", get(get_statistics))
        .route("/statistics/enhanced", get(get_enhanced_statistics))
}

// B32: Statistics
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordStatistics {
    total: usize,
    correct: usize,
    accuracy: f64,
    total_duration_secs: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatsCategoryQuery {
    category: Option<String>,
}

fn parse_category(raw: Option<&str>) -> Result<Option<RecordType>, AppError> {
    match raw.unwrap_or("all") {
        "all" => Ok(None),
        "learning" => Ok(Some(RecordType::Learning)),
        "review" => Ok(Some(RecordType::Review)),
        other => Err(AppError::bad_request(
            "INVALID_CATEGORY",
            &format!("category 必须是 all、learning 或 review，收到 {other}"),
        )),
    }
}

async fn get_statistics(
    auth: AuthUser,
    Query(q): Query<StatsCategoryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let category = parse_category(q.category.as_deref())?;
    let (total, correct, total_duration_secs) = state
        .run_store_task("records.statistics", move |store| -> Result<_, AppError> {
            let (total, correct) =
                store.count_user_records_stats_filtered(&auth.user_id, category)?;
            let total_duration_secs = store.total_session_duration_secs(&auth.user_id)?;
            Ok((total, correct, total_duration_secs))
        })
        .await??;
    let accuracy = if total > 0 {
        correct as f64 / total as f64
    } else {
        0.0
    };

    Ok(ok(RecordStatistics {
        total,
        correct,
        accuracy,
        total_duration_secs,
    }))
}

async fn get_enhanced_statistics(
    auth: AuthUser,
    Query(q): Query<StatsCategoryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let category = parse_category(q.category.as_deref())?;
    // 限制单次查询量，后续应改为增量聚合以支持更大数据量
    let max_stats_records = state.config().limits.max_stats_records;
    let user_id = auth.user_id.clone();
    let (records, first_times, daily_durations, total_duration_secs) = state
        .run_store_task(
            "records.enhanced_statistics",
            move |store| -> Result<_, AppError> {
                let records =
                    store.get_user_records_filtered(&user_id, max_stats_records, category)?;
                let word_ids: Vec<String> = records
                    .iter()
                    .map(|r| r.word_id.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                let first_times = store.first_record_times_for_words(&user_id, &word_ids)?;
                // 时长走无 cap 的 DB 聚合，避免被 max_stats_records 截断
                let daily_durations = store.daily_session_durations(&user_id)?;
                let total_duration_secs = store.total_session_duration_secs(&user_id)?;
                Ok((records, first_times, daily_durations, total_duration_secs))
            },
        )
        .await??;

    let total = records.len();
    let correct = records.iter().filter(|r| r.is_correct).count();
    let accuracy = if total > 0 {
        correct as f64 / total as f64
    } else {
        0.0
    };

    #[derive(Default)]
    struct DayBucket {
        total: usize,
        correct: usize,
        new_words: std::collections::HashSet<String>,
        duration_secs: u64,
    }

    // daily 仅基于"有记录"的日期，session-only 日期不进 daily 也不计入 streak
    let mut by_day: std::collections::BTreeMap<String, DayBucket> =
        std::collections::BTreeMap::new();
    for r in &records {
        let day = r.created_at.format("%Y-%m-%d").to_string();
        let entry = by_day.entry(day.clone()).or_default();
        entry.total += 1;
        if r.is_correct {
            entry.correct += 1;
        }
        let first_at = first_times.get(&r.word_id).copied().unwrap_or(r.created_at);
        if first_at.format("%Y-%m-%d").to_string() == day {
            entry.new_words.insert(r.word_id.clone());
        }
    }
    // 把当日 session 时长贴回有记录的日期
    for (day, bucket) in by_day.iter_mut() {
        if let Some(secs) = daily_durations.get(day) {
            bucket.duration_secs = *secs;
        }
    }

    let daily: Vec<serde_json::Value> = by_day
        .iter()
        .map(|(day, b)| {
            serde_json::json!({
                "date": day,
                "total": b.total,
                "correct": b.correct,
                "accuracy": if b.total > 0 { b.correct as f64 / b.total as f64 } else { 0.0 },
                "newWords": b.new_words.len() as u64,
                "durationSecs": b.duration_secs,
            })
        })
        .collect();

    // 仅基于 record 日期计算 streak，避免 session-only 日期虚增
    let dates: std::collections::BTreeSet<chrono::NaiveDate> = by_day
        .keys()
        .filter_map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .collect();
    let streak = crate::routes::users::compute_streak_from_dates(&dates);

    Ok(ok(serde_json::json!({
        "total": total,
        "correct": correct,
        "accuracy": accuracy,
        "streak": streak,
        "totalDurationSecs": total_duration_secs,
        "daily": daily,
    })))
}
