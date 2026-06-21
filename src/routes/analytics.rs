use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::amas::config::MemoryModelConfig;
use crate::amas::memory::mdm;
use crate::auth::AuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::learning_sessions::{LearningSession, SessionStatus};
use crate::store::operations::records::LearningRecord;
use crate::store::operations::word_states::WordLearningState;

const DEFAULT_TIMEZONE: &str = "Asia/Shanghai";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dashboard", get(dashboard))
        .route("/forgetting-risk", get(forgetting_risk))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalyticsQuery {
    range: Option<String>,
    timezone: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnalyticsRange {
    Day,
    Week,
    Month,
}

impl AnalyticsRange {
    fn parse(raw: Option<&str>, default: Self) -> Result<Self, AppError> {
        match raw.unwrap_or(match default {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }) {
            "day" => Ok(Self::Day),
            "week" => Ok(Self::Week),
            "month" => Ok(Self::Month),
            _ => Err(AppError::bad_request(
                "ANALYTICS_INVALID_RANGE",
                "range 必须是 day、week 或 month",
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }
}

#[derive(Debug, Clone)]
struct LocalDateRange {
    range_type: AnalyticsRange,
    start_date: NaiveDate,
    end_date: NaiveDate,
    start_utc: DateTime<Utc>,
    end_exclusive_utc: DateTime<Utc>,
    timezone: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RangeResponse {
    #[serde(rename = "type")]
    range_type: String,
    start_date: String,
    end_date: String,
    timezone: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardResponse {
    range: RangeResponse,
    summary: DashboardSummary,
    comparison: DashboardComparison,
    daily: Vec<DashboardDaily>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct DashboardSummary {
    study_duration_secs: u64,
    session_count: u64,
    record_count: u64,
    correct_count: u64,
    accuracy: Option<f64>,
    new_words: u64,
    review_words: u64,
    mastered_words: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardComparison {
    previous_study_duration_secs: Option<u64>,
    previous_accuracy: Option<f64>,
    previous_new_words: Option<u64>,
    previous_review_words: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardDaily {
    date: String,
    study_duration_secs: u64,
    session_count: u64,
    record_count: u64,
    correct_count: u64,
    accuracy: Option<f64>,
    new_words: u64,
    review_words: u64,
    mastered_words: u64,
}

#[derive(Default)]
struct DashboardBucket {
    study_duration_secs: u64,
    session_count: u64,
    record_count: u64,
    correct_count: u64,
    new_words: HashSet<String>,
    review_words: HashSet<String>,
    mastered_words: HashSet<String>,
}

impl DashboardBucket {
    fn accuracy(&self) -> Option<f64> {
        if self.record_count > 0 {
            Some(self.correct_count as f64 / self.record_count as f64)
        } else {
            None
        }
    }
}

struct DashboardAggregate {
    summary: DashboardBucket,
    daily: BTreeMap<NaiveDate, DashboardBucket>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ForgettingRiskResponse {
    range: RangeResponse,
    summary: ForgettingRiskSummary,
    comparison: ForgettingRiskComparison,
    retention_curve: Vec<RetentionCurvePoint>,
    risk_words: Vec<RiskWord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ForgettingRiskSummary {
    high_risk_words: u64,
    medium_risk_words: u64,
    average_retention: Option<f64>,
    due_review_words: u64,
    overdue_review_words: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ForgettingRiskComparison {
    previous_high_risk_words: Option<u64>,
    previous_medium_risk_words: Option<u64>,
    previous_average_retention: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RetentionCurvePoint {
    bucket: String,
    retention: Option<f64>,
    word_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RiskWord {
    word_id: String,
    risk_level: String,
    retention: f64,
    next_review_date: Option<DateTime<Utc>>,
    overdue_hours: u64,
}

#[derive(Default)]
struct RetentionBucket {
    retention_sum: f64,
    word_count: u64,
}

impl RetentionBucket {
    fn push(&mut self, retention: f64) {
        self.retention_sum += retention;
        self.word_count += 1;
    }

    fn point(&self, bucket: &str) -> RetentionCurvePoint {
        RetentionCurvePoint {
            bucket: bucket.to_string(),
            retention: if self.word_count > 0 {
                Some(self.retention_sum / self.word_count as f64)
            } else {
                None
            },
            word_count: self.word_count,
        }
    }
}

fn parse_timezone(raw: Option<&str>) -> Result<(Tz, String), AppError> {
    let name = raw.unwrap_or(DEFAULT_TIMEZONE);
    let tz: Tz = name.parse().map_err(|_| {
        AppError::bad_request(
            "ANALYTICS_INVALID_TIMEZONE",
            "timezone 必须是有效的 IANA 时区",
        )
    })?;
    Ok((tz, name.to_string()))
}

fn local_midnight_to_utc(tz: Tz, date: NaiveDate) -> Result<DateTime<Utc>, AppError> {
    let naive = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| AppError::internal("invalid local date"))?;
    let local = match tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => dt,
        LocalResult::Ambiguous(first, _) => first,
        LocalResult::None => {
            let mut found = None;
            for hour in 1..=6 {
                if let Some(next) = date.and_hms_opt(hour, 0, 0) {
                    if let LocalResult::Single(dt) = tz.from_local_datetime(&next) {
                        found = Some(dt);
                        break;
                    }
                }
            }
            found.ok_or_else(|| AppError::internal("failed to resolve local midnight"))?
        }
    };
    Ok(local.with_timezone(&Utc))
}

fn build_range(
    range_type: AnalyticsRange,
    tz: Tz,
    timezone: String,
    today: NaiveDate,
) -> Result<LocalDateRange, AppError> {
    let start_date = match range_type {
        AnalyticsRange::Day => today,
        AnalyticsRange::Week => {
            today - Duration::days(today.weekday().num_days_from_monday() as i64)
        }
        AnalyticsRange::Month => today
            .with_day(1)
            .ok_or_else(|| AppError::internal("invalid month start"))?,
    };
    let end_date = match range_type {
        AnalyticsRange::Day => start_date,
        AnalyticsRange::Week => start_date + Duration::days(6),
        AnalyticsRange::Month => {
            let (next_year, next_month) = if start_date.month() == 12 {
                (start_date.year() + 1, 1)
            } else {
                (start_date.year(), start_date.month() + 1)
            };
            NaiveDate::from_ymd_opt(next_year, next_month, 1)
                .ok_or_else(|| AppError::internal("invalid next month"))?
                - Duration::days(1)
        }
    };
    let start_utc = local_midnight_to_utc(tz, start_date)?;
    let end_exclusive_utc = local_midnight_to_utc(tz, end_date + Duration::days(1))?;
    Ok(LocalDateRange {
        range_type,
        start_date,
        end_date,
        start_utc,
        end_exclusive_utc,
        timezone,
    })
}

fn previous_range(range: &LocalDateRange, tz: Tz) -> Result<LocalDateRange, AppError> {
    let (start_date, end_date) = match range.range_type {
        AnalyticsRange::Month => {
            let prev_end = range.start_date - Duration::days(1);
            let prev_start = NaiveDate::from_ymd_opt(prev_end.year(), prev_end.month(), 1)
                .ok_or_else(|| AppError::internal("invalid previous month start"))?;
            (prev_start, prev_end)
        }
        AnalyticsRange::Day | AnalyticsRange::Week => {
            let days = (range.end_date - range.start_date).num_days() + 1;
            (
                range.start_date - Duration::days(days),
                range.start_date - Duration::days(1),
            )
        }
    };
    Ok(LocalDateRange {
        range_type: range.range_type,
        start_date,
        end_date,
        start_utc: local_midnight_to_utc(tz, start_date)?,
        end_exclusive_utc: local_midnight_to_utc(tz, end_date + Duration::days(1))?,
        timezone: range.timezone.clone(),
    })
}

fn range_response(range: &LocalDateRange) -> RangeResponse {
    RangeResponse {
        range_type: range.range_type.as_str().to_string(),
        start_date: range.start_date.format("%Y-%m-%d").to_string(),
        end_date: range.end_date.format("%Y-%m-%d").to_string(),
        timezone: range.timezone.clone(),
    }
}

fn session_duration_secs(session: &LearningSession) -> u64 {
    if let Some(summary) = &session.summary {
        return summary.duration_secs.max(0) as u64;
    }
    if session.status == SessionStatus::Completed {
        return (session.updated_at - session.created_at)
            .num_seconds()
            .max(0) as u64;
    }
    0
}

fn compute_dashboard_aggregate(
    records: Vec<LearningRecord>,
    sessions: Vec<LearningSession>,
    first_times: HashMap<String, DateTime<Utc>>,
    range: &LocalDateRange,
    tz: Tz,
) -> DashboardAggregate {
    let mut daily = BTreeMap::new();
    let mut cursor = range.start_date;
    while cursor <= range.end_date {
        daily.insert(cursor, DashboardBucket::default());
        cursor += Duration::days(1);
    }

    let mut summary = DashboardBucket::default();
    for record in records {
        let date = record.created_at.with_timezone(&tz).date_naive();
        if let Some(day) = daily.get_mut(&date) {
            day.record_count += 1;
            summary.record_count += 1;
            if record.is_correct {
                day.correct_count += 1;
                summary.correct_count += 1;
            }

            let first_at = first_times
                .get(&record.word_id)
                .copied()
                .unwrap_or(record.created_at);
            let first_date = first_at.with_timezone(&tz).date_naive();
            if first_date == date {
                day.new_words.insert(record.word_id.clone());
            } else {
                day.review_words.insert(record.word_id.clone());
            }
            if first_at >= range.start_utc && first_at < range.end_exclusive_utc {
                summary.new_words.insert(record.word_id);
            } else {
                summary.review_words.insert(record.word_id);
            }
        }
    }

    for session in sessions {
        let date = session.created_at.with_timezone(&tz).date_naive();
        // 仅当 session 的本地日期落在 [start_date, end_date] 桶内才计入 summary，与 daily 分桶口径
        // 保持一致（records 已采用同样的 guard）。否则在 DST 零点跳变等边界 case 下，session 会被计入
        // summary 却无对应 daily 桶，造成 sum(daily) != summary 的内部不一致。
        if let Some(day) = daily.get_mut(&date) {
            day.session_count += 1;
            day.study_duration_secs += session_duration_secs(&session);
            summary.session_count += 1;
            summary.study_duration_secs += session_duration_secs(&session);
            if let Some(summary_data) = &session.summary {
                for word_id in &summary_data.mastered_word_ids {
                    day.mastered_words.insert(word_id.clone());
                    summary.mastered_words.insert(word_id.clone());
                }
            }
        }
    }

    DashboardAggregate { summary, daily }
}

fn bucket_to_summary(bucket: &DashboardBucket) -> DashboardSummary {
    DashboardSummary {
        study_duration_secs: bucket.study_duration_secs,
        session_count: bucket.session_count,
        record_count: bucket.record_count,
        correct_count: bucket.correct_count,
        accuracy: bucket.accuracy(),
        new_words: bucket.new_words.len() as u64,
        review_words: bucket.review_words.len() as u64,
        mastered_words: bucket.mastered_words.len() as u64,
    }
}

fn aggregate_daily(aggregate: DashboardAggregate) -> Vec<DashboardDaily> {
    aggregate
        .daily
        .into_iter()
        .map(|(date, bucket)| DashboardDaily {
            date: date.format("%Y-%m-%d").to_string(),
            study_duration_secs: bucket.study_duration_secs,
            session_count: bucket.session_count,
            record_count: bucket.record_count,
            correct_count: bucket.correct_count,
            accuracy: bucket.accuracy(),
            new_words: bucket.new_words.len() as u64,
            review_words: bucket.review_words.len() as u64,
            mastered_words: bucket.mastered_words.len() as u64,
        })
        .collect()
}

async fn dashboard(
    auth: AuthUser,
    Query(q): Query<AnalyticsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let range_type = AnalyticsRange::parse(q.range.as_deref(), AnalyticsRange::Week)?;
    let (tz, timezone) = parse_timezone(q.timezone.as_deref())?;
    let today = Utc::now().with_timezone(&tz).date_naive();
    let range = build_range(range_type, tz, timezone, today)?;
    let prev_range = previous_range(&range, tz)?;
    let user_id = auth.user_id.clone();
    let range_for_task = range.clone();
    let prev_range_for_task = prev_range.clone();

    let (current, previous) = state
        .run_store_task("analytics.dashboard", move |store| -> Result<_, AppError> {
            let current_records = store.get_user_records_between(
                &user_id,
                range_for_task.start_utc,
                range_for_task.end_exclusive_utc,
            )?;
            let previous_records = store.get_user_records_between(
                &user_id,
                prev_range_for_task.start_utc,
                prev_range_for_task.end_exclusive_utc,
            )?;
            let current_sessions = store.list_learning_sessions_between(
                &user_id,
                range_for_task.start_utc,
                range_for_task.end_exclusive_utc,
            )?;
            let previous_sessions = store.list_learning_sessions_between(
                &user_id,
                prev_range_for_task.start_utc,
                prev_range_for_task.end_exclusive_utc,
            )?;

            let current_word_ids: Vec<String> = current_records
                .iter()
                .map(|record| record.word_id.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            let previous_word_ids: Vec<String> = previous_records
                .iter()
                .map(|record| record.word_id.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            let current_first_times =
                store.first_record_times_for_words(&user_id, &current_word_ids)?;
            let previous_first_times =
                store.first_record_times_for_words(&user_id, &previous_word_ids)?;

            let current = compute_dashboard_aggregate(
                current_records,
                current_sessions,
                current_first_times,
                &range_for_task,
                tz,
            );
            let previous = compute_dashboard_aggregate(
                previous_records,
                previous_sessions,
                previous_first_times,
                &prev_range_for_task,
                tz,
            );
            Ok((current, previous))
        })
        .await??;

    let response = DashboardResponse {
        range: range_response(&range),
        summary: bucket_to_summary(&current.summary),
        comparison: DashboardComparison {
            previous_study_duration_secs: Some(previous.summary.study_duration_secs),
            previous_accuracy: previous.summary.accuracy(),
            previous_new_words: Some(previous.summary.new_words.len() as u64),
            previous_review_words: Some(previous.summary.review_words.len() as u64),
        },
        daily: aggregate_daily(current),
    };
    Ok(ok(response))
}

pub(crate) fn estimated_retention(
    state: &WordLearningState,
    mdm_state: Option<&mdm::MdmState>,
    now: DateTime<Utc>,
    memory_config: &MemoryModelConfig,
) -> f64 {
    if let Some(mdm_state) = mdm_state {
        return mdm::recall_probability(mdm_state, now.timestamp_millis(), memory_config);
    }
    if state.total_attempts == 0 {
        return 1.0;
    }
    let elapsed_hours = (now - state.updated_at).num_seconds().max(0) as f64 / 3600.0;
    let half_life = state.half_life.max(0.01);
    2f64.powf(-elapsed_hours / half_life).clamp(0.0, 1.0)
}

fn last_review_at(
    state: &WordLearningState,
    mdm_state: Option<&mdm::MdmState>,
) -> Option<DateTime<Utc>> {
    mdm_state
        .and_then(|mdm_state| mdm_state.last_review_at)
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .or({
            if state.total_attempts > 0 {
                Some(state.updated_at)
            } else {
                None
            }
        })
}

fn retention_bucket_name(
    last_review_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> &'static str {
    let Some(last_review_at) = last_review_at else {
        return "new";
    };
    let elapsed_days = (now - last_review_at).num_seconds().max(0) as f64 / 86_400.0;
    if elapsed_days <= 1.0 {
        "1d"
    } else if elapsed_days <= 3.0 {
        "3d"
    } else if elapsed_days <= 7.0 {
        "7d"
    } else if elapsed_days <= 14.0 {
        "14d"
    } else {
        "30d"
    }
}

async fn forgetting_risk(
    auth: AuthUser,
    Query(q): Query<AnalyticsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let range_type = match q.range.as_deref().unwrap_or("week") {
        "week" => AnalyticsRange::Week,
        "month" => AnalyticsRange::Month,
        _ => {
            return Err(AppError::bad_request(
                "ANALYTICS_INVALID_RANGE",
                "range 必须是 week 或 month",
            ))
        }
    };
    let (tz, timezone) = parse_timezone(q.timezone.as_deref())?;
    let today = Utc::now().with_timezone(&tz).date_naive();
    let range = build_range(range_type, tz, timezone, today)?;
    let memory_config = state.amas().get_config().memory_model;
    let user_id = auth.user_id.clone();
    let now = Utc::now();

    let response = state
        .run_store_task(
            "analytics.forgetting_risk",
            move |store| -> Result<_, AppError> {
                let states = store.list_all_user_word_states(&user_id)?;
                let word_ids: Vec<String> = states.iter().map(|s| s.word_id.clone()).collect();
                let mdm_states = store.batch_get_engine_mastery_mdm_states(&user_id, &word_ids)?;

                let mut high_risk_words = 0u64;
                let mut medium_risk_words = 0u64;
                let mut due_review_words = 0u64;
                let mut overdue_review_words = 0u64;
                let mut retention_sum = 0.0;
                let mut retention_count = 0u64;
                let mut curve: HashMap<&'static str, RetentionBucket> = HashMap::new();
                for bucket in ["new", "1d", "3d", "7d", "14d", "30d"] {
                    curve.insert(bucket, RetentionBucket::default());
                }
                let mut risk_words = Vec::new();

                for state in states {
                    let mdm_state = mdm_states.get(&state.word_id);
                    let retention = estimated_retention(&state, mdm_state, now, &memory_config);
                    if state.total_attempts > 0 || mdm_state.is_some() {
                        retention_sum += retention;
                        retention_count += 1;
                    }
                    let bucket_name = retention_bucket_name(last_review_at(&state, mdm_state), now);
                    if let Some(bucket) = curve.get_mut(bucket_name) {
                        bucket.push(retention);
                    }

                    let overdue_hours = state
                        .next_review_date
                        .map(|next| (now - next).num_hours().max(0) as u64)
                        .unwrap_or(0);
                    let due_now = state
                        .next_review_date
                        .map(|next| next <= now)
                        .unwrap_or(false);
                    if due_now {
                        due_review_words += 1;
                    }
                    if overdue_hours > 0 {
                        overdue_review_words += 1;
                    }

                    let due_this_range = state
                        .next_review_date
                        .map(|next| next <= range.end_exclusive_utc)
                        .unwrap_or(false);
                    let risk_level = if overdue_hours > 0 || retention < 0.4 {
                        high_risk_words += 1;
                        Some("high")
                    } else if retention < 0.7 || due_this_range {
                        medium_risk_words += 1;
                        Some("medium")
                    } else {
                        None
                    };

                    if let Some(level) = risk_level {
                        risk_words.push(RiskWord {
                            word_id: state.word_id,
                            risk_level: level.to_string(),
                            retention,
                            next_review_date: state.next_review_date,
                            overdue_hours,
                        });
                    }
                }

                risk_words.sort_by(|a, b| {
                    a.retention
                        .partial_cmp(&b.retention)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| b.overdue_hours.cmp(&a.overdue_hours))
                });
                // 仅返回风险最高的前 N 条，避免重度用户产生数 MB 级超大响应；summary 仍含全量计数。
                risk_words.truncate(200);

                let retention_curve = ["new", "1d", "3d", "7d", "14d", "30d"]
                    .iter()
                    .map(|bucket| {
                        curve
                            .get(bucket)
                            .map(|entry| entry.point(bucket))
                            .unwrap_or(RetentionBucket::default().point(bucket))
                    })
                    .collect::<Vec<_>>();

                Ok(ForgettingRiskResponse {
                    range: range_response(&range),
                    summary: ForgettingRiskSummary {
                        high_risk_words,
                        medium_risk_words,
                        average_retention: if retention_count > 0 {
                            Some(retention_sum / retention_count as f64)
                        } else {
                            None
                        },
                        due_review_words,
                        overdue_review_words,
                    },
                    comparison: ForgettingRiskComparison {
                        previous_high_risk_words: None,
                        previous_medium_risk_words: None,
                        previous_average_retention: None,
                    },
                    retention_curve,
                    risk_words,
                })
            },
        )
        .await??;

    Ok(ok(response))
}
