//! m073：客户端埋点分析读端点（`/api/admin/app-events/*`，look「端侧埋点」页数据源）。
//!
//! 口径：历史日读 rollup（app_event_daily / app_user_daily / app_user_first_seen，
//! 01:10 worker 生成），当日补 raw 索引扫（store 层 UNION 合并）。窗口语义复用
//! analytics.rs 的 WindowQuery/resolve_window（days ∈ 7/14/30/90 或 from&to）。

use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::analytics::{delta_pct, resolve_window, WindowQuery};
use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::app_events::{percentile, AppEventsPerfDay};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/overview", get(overview))
        .route("/trend", get(trend))
        .route("/top-events", get(top_events))
        .route("/errors", get(errors))
        .route("/perf", get(perf))
        .route("/funnel", get(funnel))
        .route("/retention-matrix", get(retention_matrix))
        .route("/activity", get(activity))
}

fn today_str() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

// ---------------------------------------------------------------------------
// 1) overview
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OverviewMetric {
    value: f64,
    prev_value: f64,
    delta_pct: Option<f64>,
}

fn metric(cur: f64, prev: f64) -> OverviewMetric {
    OverviewMetric {
        value: cur,
        prev_value: prev,
        delta_pct: delta_pct(cur, prev),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OverviewResponse {
    generated_at: DateTime<Utc>,
    days: i64,
    range_start: String,
    range_end: String,
    total_events: OverviewMetric,
    dau_average: OverviewMetric,
    error_count: OverviewMetric,
    /// 1 - 窗口内出现 crash 事件的用户 / 活跃用户；无活跃用户 → null
    crash_free_pct: Option<f64>,
    /// 日 p95 按 count 加权平均（近似口径，当日不计入）
    api_rtt_p95: Option<f64>,
}

async fn overview(
    _admin: AdminAuthUser,
    Query(q): Query<WindowQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let w = resolve_window(&q)?;
    let today = today_str();
    let (cur, prev) = {
        let (s, e, ps, pe, t) = (
            w.start.clone(),
            w.end_excl.clone(),
            w.prev_start.clone(),
            w.prev_end_excl.clone(),
            today,
        );
        state
            .run_store_task("admin.app_events.overview", move |store| {
                Ok::<_, crate::store::StoreError>((
                    store.admin_app_events_overview(&s, &e, &t)?,
                    store.admin_app_events_overview(&ps, &pe, &t)?,
                ))
            })
            .await??
    };
    let dau_avg = |sum: i64| sum as f64 / w.days as f64;
    Ok(ok(OverviewResponse {
        generated_at: Utc::now(),
        days: w.days,
        range_start: w.start,
        range_end: w.end,
        total_events: metric(cur.total_events as f64, prev.total_events as f64),
        dau_average: metric(dau_avg(cur.active_days_sum), dau_avg(prev.active_days_sum)),
        error_count: metric(cur.error_count as f64, prev.error_count as f64),
        crash_free_pct: (cur.active_users > 0)
            .then(|| 1.0 - cur.crash_users as f64 / cur.active_users as f64),
        api_rtt_p95: cur.api_rtt_p95,
    }))
}

// ---------------------------------------------------------------------------
// 2) trend
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrendDay {
    day: String,
    behavior: i64,
    error: i64,
    perf: i64,
    dau: i64,
}

async fn trend(
    _admin: AdminAuthUser,
    Query(q): Query<WindowQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let w = resolve_window(&q)?;
    let today = today_str();
    let rows = {
        let (s, e) = (w.start.clone(), w.end_excl.clone());
        state
            .run_store_task("admin.app_events.trend", move |store| {
                store.admin_app_events_trend(&s, &e, &today)
            })
            .await??
    };
    let days: Vec<TrendDay> = rows
        .into_iter()
        .map(|r| TrendDay {
            day: r.day,
            behavior: r.behavior,
            error: r.error,
            perf: r.perf,
            dau: r.dau,
        })
        .collect();
    Ok(ok(serde_json::json!({
        "days": w.days,
        "rangeStart": w.start,
        "rangeEnd": w.end,
        "rows": days,
    })))
}

// ---------------------------------------------------------------------------
// 3) top-events
// ---------------------------------------------------------------------------

// 注:serde(flatten) 与 query-string 反序列化不兼容(数值字段经 flatten 缓冲变字符串),
// 故窗口参数用第二个 Query<WindowQuery> 提取器分开解析(axum 允许多个 Query)。
#[derive(Deserialize)]
struct TopQuery {
    category: Option<String>,
    limit: Option<u32>,
}

async fn top_events(
    _admin: AdminAuthUser,
    Query(wq): Query<WindowQuery>,
    Query(q): Query<TopQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let w = resolve_window(&wq)?;
    let category = match q.category.as_deref() {
        None | Some("") => String::new(),
        Some(c @ ("behavior" | "error" | "perf")) => c.to_string(),
        Some(_) => {
            return Err(AppError::bad_request(
                "APP_EVENT_INVALID_CATEGORY",
                "category 必须为 behavior|error|perf",
            ))
        }
    };
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let rows = {
        let (s, e) = (w.start.clone(), w.end_excl.clone());
        state
            .run_store_task("admin.app_events.top", move |store| {
                store.admin_app_events_top(&s, &e, &category, limit)
            })
            .await??
    };
    let rows: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "name": r.name, "category": r.category,
                "count": r.count, "users": r.users,
            })
        })
        .collect();
    Ok(ok(serde_json::json!({
        "days": w.days, "rangeStart": w.start, "rangeEnd": w.end, "rows": rows,
    })))
}

// ---------------------------------------------------------------------------
// 4) errors
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ErrorsQuery {
    limit: Option<u32>,
}

async fn errors(
    _admin: AdminAuthUser,
    Query(wq): Query<WindowQuery>,
    Query(q): Query<ErrorsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let w = resolve_window(&wq)?;
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let groups = {
        let (s, e) = (w.start.clone(), w.end_excl.clone());
        state
            .run_store_task("admin.app_events.errors", move |store| {
                store.admin_app_events_errors(&s, &e, limit)
            })
            .await??
    };
    let rows: Vec<serde_json::Value> = groups
        .into_iter()
        .map(|g| {
            let samples: Vec<serde_json::Value> = g
                .samples
                .iter()
                .filter_map(|p| serde_json::from_str(p).ok())
                .collect();
            serde_json::json!({
                "name": g.name, "signature": g.signature, "count": g.count,
                "lastSeen": g.last_seen, "samples": samples,
            })
        })
        .collect();
    Ok(ok(serde_json::json!({
        "days": w.days, "rangeStart": w.start, "rangeEnd": w.end, "rows": rows,
    })))
}

// ---------------------------------------------------------------------------
// 5) perf
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PerfQuery {
    name: Option<String>,
}

async fn perf(
    _admin: AdminAuthUser,
    Query(wq): Query<WindowQuery>,
    Query(q): Query<PerfQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let w = resolve_window(&wq)?;
    let today = today_str();
    let name = q.name.unwrap_or_else(|| "api_rtt".to_string());
    let (names, mut series, today_ms) = {
        let (s, e, t, n) = (w.start.clone(), w.end_excl.clone(), today.clone(), name.clone());
        state
            .run_store_task("admin.app_events.perf", move |store| {
                store.admin_app_events_perf(&s, &e, &t, &n)
            })
            .await??
    };
    // 当日 raw 分位补点（worker 尚未 rollup 当日）。
    if !today_ms.is_empty() {
        series.push(AppEventsPerfDay {
            day: today,
            count: today_ms.len() as i64,
            p50_ms: Some(percentile(&today_ms, 0.50)),
            p95_ms: Some(percentile(&today_ms, 0.95)),
            p99_ms: Some(percentile(&today_ms, 0.99)),
        });
    }
    let rows: Vec<serde_json::Value> = series
        .into_iter()
        .map(|d| {
            serde_json::json!({
                "day": d.day, "count": d.count,
                "p50Ms": d.p50_ms, "p95Ms": d.p95_ms, "p99Ms": d.p99_ms,
            })
        })
        .collect();
    Ok(ok(serde_json::json!({
        "days": w.days, "rangeStart": w.start, "rangeEnd": w.end,
        "name": name, "names": names, "rows": rows,
    })))
}

// ---------------------------------------------------------------------------
// 6) funnel（镜像 analytics::FunnelResponse 形状）
// ---------------------------------------------------------------------------

/// 应用使用漏斗步骤（固定枚举事件名，与三端词表对齐）。
const APP_FUNNEL_DEFS: [(&str, &str, &str, &str); 5] = [
    ("session_start", "打开应用", "会话开始", "good"),
    ("study_screen", "进入学习页", "screen_view(study)", "good"),
    ("study_start", "开始练习", "发起学习会话", "normal"),
    ("study_complete", "完成练习", "学习会话完成", "normal"),
    ("word_lookup", "查词", "主动查词行为", "warn"),
];

#[derive(Serialize, Clone)]
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
            .run_store_task("admin.app_events.funnel", move |store| {
                Ok::<_, crate::store::StoreError>((
                    store.admin_app_events_funnel(&s, &e)?,
                    store.admin_app_events_funnel(&ps, &pe)?,
                ))
            })
            .await??
    };
    let base = cur[0] as f64;
    let prev_base = prev[0] as f64;
    let steps: Vec<FunnelStep> = APP_FUNNEL_DEFS
        .iter()
        .enumerate()
        .map(|(i, (key, label, sublabel, tone))| {
            let pct = if base > 0.0 { cur[i] as f64 / base } else { 0.0 };
            let prev_pct = (prev_base > 0.0).then(|| prev[i] as f64 / prev_base);
            FunnelStep {
                key: key.to_string(),
                label: label.to_string(),
                sublabel: sublabel.to_string(),
                count: cur[i],
                pct,
                delta_pt: prev_pct.map(|p| pct - p),
                tone: tone.to_string(),
            }
        })
        .collect();
    let mut biggest = (String::new(), String::new(), 0.0_f64);
    for pair in steps.windows(2) {
        let drop = pair[0].pct - pair[1].pct;
        if drop > biggest.2 {
            biggest = (pair[0].label.clone(), pair[1].label.clone(), drop);
        }
    }
    Ok(ok(serde_json::json!({
        "generatedAt": Utc::now(),
        "days": w.days, "rangeStart": w.start, "rangeEnd": w.end,
        "steps": steps,
        "biggestDropFrom": biggest.0, "biggestDropTo": biggest.1, "biggestDropPt": biggest.2,
    })))
}

// ---------------------------------------------------------------------------
// 7) retention-matrix（镜像 analytics::RetentionMatrixResponse 形状）
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RetentionQuery {
    weeks: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MatrixCohort {
    cohort_start: String,
    size: i64,
    cells: Vec<Option<f64>>,
}

/// ISO 周一对齐的周起点。
fn week_start(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

async fn retention_matrix(
    _admin: AdminAuthUser,
    Query(q): Query<RetentionQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let weeks = q.weeks.unwrap_or(7).clamp(1, 26);
    let now = Utc::now().date_naive();
    let this_week = week_start(now);
    let min_cohort = this_week - Duration::days(7 * (weeks as i64 - 1));
    let pairs = {
        let min = min_cohort.format("%Y-%m-%d").to_string();
        state
            .run_store_task("admin.app_events.retention", move |store| {
                store.admin_app_events_retention_pairs(&min)
            })
            .await??
    };

    // (cohort_start → (size 用户集, 每周活跃用户集))；活跃当日缺 rollup 的当天不计（周粒度可忽略）。
    use std::collections::{BTreeMap, HashSet};
    let mut cohort_users: BTreeMap<NaiveDate, HashSet<&str>> = BTreeMap::new();
    let mut cohort_active: BTreeMap<(NaiveDate, usize), HashSet<&str>> = BTreeMap::new();
    for p in &pairs {
        let Ok(first) = NaiveDate::parse_from_str(&p.first_day, "%Y-%m-%d") else {
            continue;
        };
        let cohort = week_start(first);
        cohort_users.entry(cohort).or_default().insert(&p.user_id);
        if let Some(day) = p
            .active_day
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        {
            let k = ((day - cohort).num_days() / 7) as usize;
            if k < weeks as usize {
                cohort_active
                    .entry((cohort, k))
                    .or_default()
                    .insert(&p.user_id);
            }
        }
    }

    let cohorts: Vec<MatrixCohort> = cohort_users
        .iter()
        .map(|(cohort, users)| {
            let size = users.len() as i64;
            let cells = (0..weeks as usize)
                .map(|k| {
                    if k == 0 {
                        return Some(1.0);
                    }
                    let cell_end = *cohort + Duration::days(7 * (k as i64 + 1));
                    if cell_end > now {
                        return None; // 未过完的周
                    }
                    if size == 0 {
                        return Some(0.0);
                    }
                    let active = cohort_active
                        .get(&(*cohort, k))
                        .map(|s| s.len())
                        .unwrap_or(0);
                    Some(active as f64 / size as f64)
                })
                .collect();
            MatrixCohort {
                cohort_start: cohort.format("%Y-%m-%d").to_string(),
                size,
                cells,
            }
        })
        .collect();

    Ok(ok(serde_json::json!({
        "generatedAt": Utc::now(),
        "weeks": weeks,
        "cohorts": cohorts,
    })))
}

// ---------------------------------------------------------------------------
// 8) activity
// ---------------------------------------------------------------------------

async fn activity(
    _admin: AdminAuthUser,
    Query(q): Query<WindowQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let w = resolve_window(&q)?;
    let today = today_str();
    // WAU 滚动窗需要窗口前 6 天的活跃行。
    let ext_start = (NaiveDate::parse_from_str(&w.start, "%Y-%m-%d")
        .map_err(|_| AppError::internal("window start parse"))?
        - Duration::days(6))
    .format("%Y-%m-%d")
    .to_string();
    let pairs = {
        let (e, t) = (w.end_excl.clone(), today);
        state
            .run_store_task("admin.app_events.activity", move |store| {
                store.admin_app_events_activity_pairs(&ext_start, &e, &t)
            })
            .await??
    };

    use std::collections::{BTreeMap, BTreeSet, HashSet};
    // day → 用户集；day → platform → 计数
    let mut day_users: BTreeMap<&str, HashSet<&str>> = BTreeMap::new();
    let mut day_platform: BTreeMap<&str, BTreeMap<&str, i64>> = BTreeMap::new();
    for (day, user, platform) in &pairs {
        day_users.entry(day).or_default().insert(user);
        *day_platform
            .entry(day)
            .or_default()
            .entry(platform)
            .or_default() += 1;
    }
    let in_window: BTreeSet<&str> = day_users
        .keys()
        .copied()
        .filter(|d| *d >= w.start.as_str())
        .collect();
    let rows: Vec<serde_json::Value> = in_window
        .iter()
        .map(|day| {
            let dau = day_users.get(day).map(|s| s.len()).unwrap_or(0);
            let day_d = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok();
            let wau = day_d
                .map(|d| {
                    let from = (d - Duration::days(6)).format("%Y-%m-%d").to_string();
                    let mut set: HashSet<&str> = HashSet::new();
                    for (dd, users) in day_users.range(from.as_str()..=*day) {
                        let _ = dd;
                        set.extend(users.iter());
                    }
                    set.len()
                })
                .unwrap_or(dau);
            let platforms: BTreeMap<&str, i64> = day_platform
                .get(day)
                .map(|m| m.iter().map(|(k, v)| (*k, *v)).collect())
                .unwrap_or_default();
            serde_json::json!({
                "day": day, "dau": dau, "wau": wau, "byPlatform": platforms,
            })
        })
        .collect();

    Ok(ok(serde_json::json!({
        "days": w.days, "rangeStart": w.start, "rangeEnd": w.end, "rows": rows,
    })))
}
