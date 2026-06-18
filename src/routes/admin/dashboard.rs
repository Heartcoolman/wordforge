//! 管理员监控大屏聚合接口。
//!
//! - `GET /api/admin/dashboard/overview?days=7` —— 首屏关键指标(概览/趋势/在线/平台/概要健康/序列)。
//! - `GET /api/admin/dashboard/learning?days=7` —— 学习深度指标(响应时延/首答正确率/会话状态/自评/难点词/题型难度/词库/掌握度/连续天数/分钟热力)。
//! - `GET /api/admin/dashboard/amas?days=7` —— AMAS 深度指标(实验 arm 对比/冷启动质量/ELO 趋势)。
//!
//! 全部后端短 TTL 缓存(按 endpoint+days 分桶)。详细系统资源(CPU/内存/磁盘)走 `GET /api/admin/monitoring/health`。

use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use chrono::Utc;
use serde::Deserialize;

use crate::auth::AdminAuthUser;
use crate::blocking;
use crate::response::{ok, AppError};
use crate::state::AppState;

/// 大屏聚合缓存 TTL:大屏轮询频繁,短缓存削峰;过期回落重算。
const CACHE_TTL: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct DaysQuery {
    #[serde(default = "default_days")]
    days: u32,
}

fn default_days() -> u32 {
    7
}

/// 命中未过期缓存则返回其副本。
fn cache_get(state: &AppState, key: &str) -> Option<serde_json::Value> {
    state.dashboard_cache().get(key).and_then(|e| {
        let (at, val) = e.value();
        (at.elapsed() < CACHE_TTL).then(|| val.clone())
    })
}

fn cache_put(state: &AppState, key: String, value: &serde_json::Value) {
    state
        .dashboard_cache()
        .insert(key, (Instant::now(), value.clone()));
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/overview", get(overview))
        .route("/learning", get(learning))
        .route("/amas", get(amas))
}

/// GET /api/admin/dashboard/overview
async fn overview(
    _admin: AdminAuthUser,
    Query(q): Query<DaysQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let days = q.days.clamp(1, 30);
    let key = format!("overview:{days}");
    if let Some(val) = cache_get(&state, &key) {
        return Ok(ok(val));
    }

    // 概览卡片 + 今日环比趋势(复用现成聚合函数)。
    let stats = super::admin_get_stats(&state).await?;

    // 在线连接(进程内存,实时)。
    let sse_devices = state.active_sse().len();
    let sse_conns: u64 = state
        .active_sse()
        .iter()
        .map(|e| e.value().len() as u64)
        .sum();

    // 运行时指标(不走 DB)。
    let uptime = state.uptime_secs();
    let (req_total, req_5xx) = crate::metrics_counters::snapshot();
    let error_rate = if req_total > 0 {
        req_5xx as f64 / req_total as f64
    } else {
        0.0
    };
    let (rl_user, rl_anon, rl_admin, rl_brute) = crate::metrics_counters::rate_limit_hit_snapshot();

    // DB 聚合在单个 blocking 闭包内批量取,减少线程池往返。
    let store = state.store().clone();
    let (probe_ok, outbox, platforms, dau, study) =
        blocking::run_blocking("admin.dashboard.overview", move || {
            let probe_ok = store.db_ping().is_ok();
            let outbox = store.outbox_stats()?;
            let platforms = store.aggregate_clients_by_platform()?;
            let dau = store.daily_active_users(days)?;
            let study = store.admin_daily_study_overview(days, None)?;
            Ok::<_, crate::store::StoreError>((probe_ok, outbox, platforms, dau, study))
        })
        .await??;

    let value = serde_json::json!({
        "generatedAt": Utc::now().to_rfc3339(),
        "days": days,
        "totals": {
            "users": stats.users,
            "words": stats.words,
            "records": stats.records,
        },
        "trend": {
            "usersPct": stats.trend.users.value,
            "recordsPct": stats.trend.records.value,
        },
        "online": {
            "sseConnections": sse_conns,
            "sseDevices": sse_devices,
        },
        "platforms": platforms
            .iter()
            .map(|(platform, total, active7d, mom_pct)| serde_json::json!({
                "platform": platform,
                "total": total,
                "active7d": active7d,
                "momPct": mom_pct,
            }))
            .collect::<Vec<_>>(),
        "health": {
            "storeProbeOk": probe_ok,
            "uptimeSecs": uptime,
            "errorRate": error_rate,
            "outbox": {
                "pending": outbox.pending,
                "lagSecs": outbox.lag_secs,
                "deadLetter": outbox.dead_letter,
            },
            "rateLimitHits": {
                "user": rl_user,
                "anon": rl_anon,
                "admin": rl_admin,
                "authBruteforce": rl_brute,
            },
        },
        "series": {
            "dailyActiveUsers": dau
                .iter()
                .map(|(date, count)| serde_json::json!({ "date": date, "count": count }))
                .collect::<Vec<_>>(),
            "dailyRecords": study
                .iter()
                .map(|r| serde_json::json!({
                    "date": r.date,
                    "correct": r.correct_count,
                    "total": r.record_count,
                }))
                .collect::<Vec<_>>(),
        },
    });

    cache_put(&state, key, &value);
    Ok(ok(value))
}

/// GET /api/admin/dashboard/learning —— 学习深度指标。
async fn learning(
    _admin: AdminAuthUser,
    Query(q): Query<DaysQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let days = q.days.clamp(1, 30);
    let key = format!("learning:{days}");
    if let Some(val) = cache_get(&state, &key) {
        return Ok(ok(val));
    }

    let store = state.store().clone();
    let mut value = blocking::run_blocking("admin.dashboard.learning", move || {
        Ok::<_, crate::store::StoreError>(serde_json::json!({
            "days": days,
            "responseTime": store.admin_response_time_distribution(days)?,
            "firstAttemptAccuracy": store.admin_first_attempt_accuracy(days)?,
            "sessionStatus": store.admin_session_status_stats(days)?,
            "selfRating": store.admin_self_rating_distribution(days)?,
            "wordAccuracyBins": store.admin_word_accuracy_bins()?,
            "questionDifficultyMatrix": store.admin_question_difficulty_matrix(days)?,
            "wordbookLearningStats": store.admin_wordbook_learning_stats()?,
            "masteryDistribution": store.admin_mastery_distribution()?,
            "consecutiveStudyDays": store.admin_consecutive_study_days()?,
            "peakTimeHeatmap": store.admin_peak_time_minute_heatmap(days)?,
        }))
    })
    .await??;
    value["generatedAt"] = serde_json::json!(Utc::now().to_rfc3339());

    cache_put(&state, key, &value);
    Ok(ok(value))
}

/// GET /api/admin/dashboard/amas —— AMAS 深度指标。
async fn amas(
    _admin: AdminAuthUser,
    Query(q): Query<DaysQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let days = q.days.clamp(1, 30);
    let key = format!("amas:{days}");
    if let Some(val) = cache_get(&state, &key) {
        return Ok(ok(val));
    }

    let store = state.store().clone();
    let mut value = blocking::run_blocking("admin.dashboard.amas", move || {
        Ok::<_, crate::store::StoreError>(serde_json::json!({
            "days": days,
            "experimentArmComparison": store.amas_experiment_arm_comparison(days)?,
            "coldStartQuality": store.amas_cold_start_quality(days)?,
            "eloTrends": store.amas_elo_trends()?,
        }))
    })
    .await??;
    value["generatedAt"] = serde_json::json!(Utc::now().to_rfc3339());

    cache_put(&state, key, &value);
    Ok(ok(value))
}
