use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

// ─────────── C1: 算法指标时间序列 ───────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlgoTimeseriesPoint {
    pub date: String, // YYYY-MM-DD
    pub algorithm: String,
    pub call_count: u64,
    pub avg_latency_us: f64,
    pub error_count: u64,
}

impl Store {
    /// 从 algorithm_metrics_daily 拉过去 N 天的每日每算法聚合。
    /// metrics_json 形如 {"callCount": u64, "totalLatencyUs": u64, "errorCount": u64}（MetricsSnapshot camelCase 序列化）
    pub fn list_amas_metrics_timeseries(
        &self,
        days: u32,
    ) -> Result<Vec<AlgoTimeseriesPoint>, StoreError> {
        let cutoff = (Utc::now() - Duration::days(days as i64))
            .format("%Y-%m-%d")
            .to_string();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT metric_date, algorithm_id, metrics_json
             FROM algorithm_metrics_daily
             WHERE metric_date >= ?1
             ORDER BY metric_date ASC, algorithm_id ASC",
        )?;
        let rows = stmt
            .query_map([cutoff], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut out = Vec::with_capacity(rows.len());
        for (date, algo, json) in rows {
            let parsed: serde_json::Value =
                serde_json::from_str(&json).map_err(StoreError::Serialization)?;
            let calls = parsed
                .get("callCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let total_us = parsed
                .get("totalLatencyUs")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let errors = parsed
                .get("errorCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let avg = if calls > 0 {
                total_us as f64 / calls as f64
            } else {
                0.0
            };
            out.push(AlgoTimeseriesPoint {
                date,
                algorithm: algo,
                call_count: calls,
                avg_latency_us: avg,
                error_count: errors,
            });
        }
        Ok(out)
    }
}

// ─────────── C3: Anomaly / Invariant Violation 聚合 ───────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnomalyOverview {
    pub total_events: u64,
    pub anomaly_count: u64,
    pub violation_count: u64,
    pub cold_start_explore: u64,
    pub cold_start_exploit: u64,
    pub by_day: Vec<DailyAnomaly>,
    pub top_violation_fields: Vec<ViolationFieldStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyAnomaly {
    pub date: String,
    pub total: u64,
    pub anomalies: u64,
    pub violations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViolationFieldStat {
    pub field: String,
    pub count: u64,
}

impl Store {
    pub fn aggregate_amas_anomalies(&self, days: u32) -> Result<AnomalyOverview, StoreError> {
        let cutoff = (Utc::now() - Duration::days(days as i64)).to_rfc3339();
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT timestamp, is_anomaly, invariant_violations_json, cold_start_phase
             FROM engine_monitoring_events
             WHERE datetime(timestamp) >= datetime(?1)",
        )?;
        let rows = stmt
            .query_map([&cutoff], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? != 0,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut by_day_map: std::collections::BTreeMap<String, (u64, u64, u64)> =
            Default::default();
        let mut field_counts: std::collections::HashMap<String, u64> = Default::default();
        let mut total = 0u64;
        let mut anom = 0u64;
        let mut viol = 0u64;
        let mut cs_explore = 0u64;
        let mut cs_exploit = 0u64;

        for (ts, is_anom, viol_json, cold) in rows {
            total += 1;
            let day = ts.get(0..10).unwrap_or("?").to_string();
            let entry = by_day_map.entry(day).or_insert((0, 0, 0));
            entry.0 += 1;
            if is_anom {
                anom += 1;
                entry.1 += 1;
            }

            // invariant_violations_json: 形如 [{"field":"fatigue","value":1.2,...}] 或 []
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&viol_json) {
                if !arr.is_empty() {
                    viol += arr.len() as u64;
                    entry.2 += arr.len() as u64;
                    for item in &arr {
                        if let Some(f) = item.get("field").and_then(|v| v.as_str()) {
                            *field_counts.entry(f.to_string()).or_insert(0) += 1;
                        }
                    }
                }
            }

            if let Some(cs) = cold.as_deref() {
                if cs.eq_ignore_ascii_case("explore") {
                    cs_explore += 1;
                } else if cs.eq_ignore_ascii_case("exploit") {
                    cs_exploit += 1;
                }
            }
        }

        let mut by_day: Vec<DailyAnomaly> = by_day_map
            .into_iter()
            .map(|(date, (t, a, v))| DailyAnomaly {
                date,
                total: t,
                anomalies: a,
                violations: v,
            })
            .collect();
        by_day.sort_by(|a, b| a.date.cmp(&b.date));

        let mut top: Vec<ViolationFieldStat> = field_counts
            .into_iter()
            .map(|(field, count)| ViolationFieldStat { field, count })
            .collect();
        top.sort_by(|a, b| b.count.cmp(&a.count));
        top.truncate(10);

        Ok(AnomalyOverview {
            total_events: total,
            anomaly_count: anom,
            violation_count: viol,
            cold_start_explore: cs_explore,
            cold_start_exploit: cs_exploit,
            by_day,
            top_violation_fields: top,
        })
    }
}

// ─────────── C2: 按 config_version 聚合对比 ───────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionMetricsSlice {
    pub version_hash: String,
    pub event_count: u64,
    pub anomaly_count: u64,
    pub anomaly_rate: f64,
    pub mean_latency_ms: f64,
    pub mean_reward: f64,
    pub mean_attention: f64,
    pub mean_fatigue: f64,
    pub mean_motivation: f64,
    pub mean_confidence: f64,
    pub first_event_at: Option<DateTime<Utc>>,
    pub last_event_at: Option<DateTime<Utc>>,
}

impl Store {
    /// 聚合某个 config_version 在 monitoring_events 中的统计切片。
    pub fn aggregate_amas_version_slice(
        &self,
        version_hash: &str,
    ) -> Result<VersionMetricsSlice, StoreError> {
        let conn = self.conn()?;
        let row = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(is_anomaly), 0),
                COALESCE(AVG(latency_ms), 0.0),
                COALESCE(AVG(reward_value), 0.0),
                COALESCE(AVG(user_state_attention), 0.0),
                COALESCE(AVG(user_state_fatigue), 0.0),
                COALESCE(AVG(user_state_motivation), 0.0),
                COALESCE(AVG(user_state_confidence), 0.0),
                MIN(timestamp),
                MAX(timestamp)
             FROM engine_monitoring_events
             WHERE config_version = ?1",
            [version_hash],
            |row| {
                Ok((
                    row.get::<_, i64>(0)? as u64,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, f64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, f64>(6)?,
                    row.get::<_, f64>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            },
        )?;

        let first = row
            .8
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc));
        let last = row
            .9
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc));

        let event_count = row.0;
        let anomaly_count = row.1;
        let anomaly_rate = if event_count > 0 {
            anomaly_count as f64 / event_count as f64
        } else {
            0.0
        };

        Ok(VersionMetricsSlice {
            version_hash: version_hash.to_string(),
            event_count,
            anomaly_count,
            anomaly_rate,
            mean_latency_ms: row.2,
            mean_reward: row.3,
            mean_attention: row.4,
            mean_fatigue: row.5,
            mean_motivation: row.6,
            mean_confidence: row.7,
            first_event_at: first,
            last_event_at: last,
        })
    }
}

// ─────────── C4: User state 分布直方图 ───────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStateHistogram {
    pub field: String,
    pub min: f64,
    pub max: f64,
    pub bins: Vec<u64>,
    pub mean: f64,
    pub median: f64,
    pub sample_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStateDistribution {
    pub attention: UserStateHistogram,
    pub fatigue: UserStateHistogram,
    pub motivation: UserStateHistogram,
    pub confidence: UserStateHistogram,
    pub cold_start_explore: u64,
    pub cold_start_exploit: u64,
}

impl Store {
    pub fn aggregate_amas_user_state_distribution(
        &self,
        days: u32,
        bins: u32,
    ) -> Result<UserStateDistribution, StoreError> {
        let bins = bins.clamp(4, 100) as usize;
        let cutoff = (Utc::now() - Duration::days(days as i64)).to_rfc3339();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT user_state_attention, user_state_fatigue, user_state_motivation, user_state_confidence, cold_start_phase
             FROM engine_monitoring_events
             WHERE datetime(timestamp) >= datetime(?1)",
        )?;
        let rows = stmt
            .query_map([&cutoff], |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut attn = Vec::with_capacity(rows.len());
        let mut fat = Vec::with_capacity(rows.len());
        let mut mot = Vec::with_capacity(rows.len());
        let mut conf = Vec::with_capacity(rows.len());
        let mut cs_explore = 0u64;
        let mut cs_exploit = 0u64;
        for (a, f, m, c, cs) in rows {
            attn.push(a);
            fat.push(f);
            mot.push(m);
            conf.push(c);
            if let Some(s) = cs.as_deref() {
                if s.eq_ignore_ascii_case("explore") {
                    cs_explore += 1;
                } else if s.eq_ignore_ascii_case("exploit") {
                    cs_exploit += 1;
                }
            }
        }

        Ok(UserStateDistribution {
            attention: build_histogram("attention", &attn, bins, 0.0, 1.0),
            fatigue: build_histogram("fatigue", &fat, bins, 0.0, 1.0),
            motivation: build_histogram("motivation", &mot, bins, -1.0, 1.0),
            confidence: build_histogram("confidence", &conf, bins, 0.0, 1.0),
            cold_start_explore: cs_explore,
            cold_start_exploit: cs_exploit,
        })
    }
}

fn build_histogram(
    field: &str,
    values: &[f64],
    bins: usize,
    fixed_min: f64,
    fixed_max: f64,
) -> UserStateHistogram {
    let n = values.len() as u64;
    if values.is_empty() {
        return UserStateHistogram {
            field: field.to_string(),
            min: fixed_min,
            max: fixed_max,
            bins: vec![0; bins],
            mean: 0.0,
            median: 0.0,
            sample_size: 0,
        };
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let median = sorted[sorted.len() / 2];

    let span = (fixed_max - fixed_min).max(f64::EPSILON);
    let mut counts = vec![0u64; bins];
    for &v in values {
        let clamped = v.clamp(fixed_min, fixed_max);
        let idx = (((clamped - fixed_min) / span) * bins as f64).floor() as usize;
        let idx = idx.min(bins - 1);
        counts[idx] += 1;
    }
    UserStateHistogram {
        field: field.to_string(),
        min: fixed_min,
        max: fixed_max,
        bins: counts,
        mean,
        median,
        sample_size: n,
    }
}

// ─────────── PR-1: metrics tab KPI 与算法决策分布 ───────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AmasMetricsKpi {
    pub decision_total: u64,
    pub hit_count: u64,
    pub hit_rate: f64,
    pub avg_latency_ms: f64,
    pub fatigue_trigger_count: u64,
    pub fatigue_trigger_rate: f64,
    pub fatigue_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlgorithmShare {
    pub algorithm: String,
    pub count: u64,
    pub pct: f64,
}

impl Store {
    /// metrics tab 顶部 4 个 KPI：N 天决策总数 / 命中率(is_correct) / 平均决策耗时(latency_ms) /
    /// 疲劳触发率(user_state_fatigue >= 阈值)。阈值来自 AMASConfig.constraints.high_fatigue_threshold，
    /// 由 handler 读出后传入（store 层无配置访问）。
    pub fn aggregate_amas_metrics_kpi(
        &self,
        days: u32,
        fatigue_threshold: f64,
    ) -> Result<AmasMetricsKpi, StoreError> {
        let cutoff = (Utc::now() - Duration::days(days as i64)).to_rfc3339();
        let conn = self.conn()?;
        let row = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(is_correct), 0),
                COALESCE(AVG(latency_ms), 0.0),
                COALESCE(SUM(CASE WHEN user_state_fatigue >= ?2 THEN 1 ELSE 0 END), 0)
             FROM engine_monitoring_events
             WHERE datetime(timestamp) >= datetime(?1)",
            rusqlite::params![cutoff, fatigue_threshold],
            |row| {
                Ok((
                    row.get::<_, i64>(0)? as u64,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, f64>(2)?,
                    row.get::<_, i64>(3)? as u64,
                ))
            },
        )?;
        let decision_total = row.0;
        let rate = |num: u64| {
            if decision_total > 0 {
                num as f64 / decision_total as f64
            } else {
                0.0
            }
        };
        Ok(AmasMetricsKpi {
            decision_total,
            hit_count: row.1,
            hit_rate: rate(row.1),
            avg_latency_ms: row.2,
            fatigue_trigger_count: row.3,
            fatigue_trigger_rate: rate(row.3),
            fatigue_threshold,
        })
    }

    /// metrics tab 8 算法决策分布甜甜圈：按 routing_algo 分组统计 N 天决策占比。
    /// 排除空 routing_algo（PR-0 前的历史行或未记录路由的行），降序返回。
    pub fn aggregate_amas_algorithm_distribution(
        &self,
        days: u32,
    ) -> Result<Vec<AlgorithmShare>, StoreError> {
        let cutoff = (Utc::now() - Duration::days(days as i64)).to_rfc3339();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT routing_algo, COUNT(*)
             FROM engine_monitoring_events
             WHERE datetime(timestamp) >= datetime(?1) AND routing_algo != ''
             GROUP BY routing_algo
             ORDER BY COUNT(*) DESC",
        )?;
        let rows = stmt
            .query_map([&cutoff], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let total: u64 = rows.iter().map(|(_, c)| *c).sum();
        Ok(rows
            .into_iter()
            .map(|(algorithm, count)| AlgorithmShare {
                algorithm,
                count,
                pct: if total > 0 {
                    count as f64 / total as f64
                } else {
                    0.0
                },
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn fresh_store() -> Store {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        store
    }

    // 测试 helper：8 个参数对应表中 8 个差异维度，再拆 struct 反而加测试样板代码。
    #[allow(clippy::too_many_arguments)]
    fn insert_event(
        store: &Store,
        id: &str,
        ts: DateTime<Utc>,
        is_anom: bool,
        violations: &str,
        cold_start: Option<&str>,
        attention: f64,
        fatigue: f64,
    ) {
        let conn = store.conn().unwrap();
        conn.execute(
            "INSERT INTO engine_monitoring_events
             (id, user_id, session_id, event_type, timestamp, latency_ms, is_anomaly, invariant_violations_json,
              user_state_attention, user_state_fatigue, user_state_motivation, user_state_confidence,
              user_state_session_event_count, user_state_total_event_count, strategy_json, reward_json,
              cold_start_phase, selection_constraints_met, reward_value, config_version)
             VALUES (?1, 'u1', 's1', 'process_event', ?2, 5, ?3, ?4,
                     ?5, ?6, 0.0, 0.5,
                     1, 10, '{}', '{}', ?7, 1, 0.5, 'abc')",
            params![id, ts.to_rfc3339(), if is_anom { 1 } else { 0 }, violations, attention, fatigue, cold_start],
        ).unwrap();
        drop(conn);
    }

    #[test]
    fn anomaly_overview_aggregates_correctly() {
        let store = fresh_store();
        let now = Utc::now();
        insert_event(
            &store,
            "e1",
            now,
            true,
            r#"[{"field":"fatigue"}]"#,
            Some("explore"),
            0.8,
            0.2,
        );
        insert_event(&store, "e2", now, false, "[]", Some("exploit"), 0.7, 0.3);
        insert_event(
            &store,
            "e3",
            now,
            true,
            r#"[{"field":"fatigue"},{"field":"motivation"}]"#,
            None,
            0.6,
            0.4,
        );

        let overview = store.aggregate_amas_anomalies(7).unwrap();
        assert_eq!(overview.total_events, 3);
        assert_eq!(overview.anomaly_count, 2);
        assert_eq!(overview.violation_count, 3);
        assert_eq!(overview.cold_start_explore, 1);
        assert_eq!(overview.cold_start_exploit, 1);
        let fatigue_stat = overview
            .top_violation_fields
            .iter()
            .find(|s| s.field == "fatigue")
            .unwrap();
        assert_eq!(fatigue_stat.count, 2);
    }

    #[test]
    fn user_state_distribution_bins_correctly() {
        let store = fresh_store();
        let now = Utc::now();
        insert_event(&store, "u1", now, false, "[]", None, 0.1, 0.9);
        insert_event(&store, "u2", now, false, "[]", None, 0.5, 0.5);
        insert_event(&store, "u3", now, false, "[]", None, 0.9, 0.1);

        let dist = store.aggregate_amas_user_state_distribution(1, 10).unwrap();
        assert_eq!(dist.attention.sample_size, 3);
        // 0.1 → bin 1; 0.5 → bin 5; 0.9 → bin 9（floor((0.9-0.0)/1.0*10) = 9）
        assert_eq!(dist.attention.bins[1], 1);
        assert_eq!(dist.attention.bins[5], 1);
        assert_eq!(dist.attention.bins[9], 1);
        assert!((dist.attention.mean - 0.5).abs() < 1e-9);
    }

    #[test]
    fn metrics_timeseries_returns_recent_rows() {
        let store = fresh_store();
        let today = Utc::now().format("%Y-%m-%d").to_string();
        {
            let conn = store.conn().unwrap();
            conn.execute(
                "INSERT INTO algorithm_metrics_daily (metric_date, algorithm_id, metrics_json)
                 VALUES (?1, 'Heuristic', '{\"callCount\":100,\"totalLatencyUs\":200000,\"errorCount\":2}')",
                params![today],
            ).unwrap();
        }
        let series = store.list_amas_metrics_timeseries(7).unwrap();
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].algorithm, "Heuristic");
        assert_eq!(series[0].call_count, 100);
        assert!((series[0].avg_latency_us - 2000.0).abs() < 1e-6);
        assert_eq!(series[0].error_count, 2);
    }

    #[test]
    fn metrics_kpi_aggregates_hit_and_fatigue() {
        let store = fresh_store();
        let now = Utc::now().to_rfc3339();
        {
            let conn = store.conn().unwrap();
            let rows = [
                ("k1", 1, 0.2, 4),
                ("k2", 0, 0.7, 6),
                ("k3", 1, 0.9, 8),
                ("k4", 0, 0.1, 2),
            ];
            for (id, correct, fatigue, lat) in rows {
                conn.execute(
                    "INSERT INTO engine_monitoring_events
                     (id,user_id,session_id,timestamp,latency_ms,user_state_fatigue,is_correct,routing_algo)
                     VALUES (?1,'u1','s1',?2,?3,?4,?5,'ensemble')",
                    params![id, now, lat, fatigue, correct],
                )
                .unwrap();
            }
        }
        let kpi = store.aggregate_amas_metrics_kpi(7, 0.6).unwrap();
        assert_eq!(kpi.decision_total, 4);
        assert_eq!(kpi.hit_count, 2);
        assert!((kpi.hit_rate - 0.5).abs() < 1e-9);
        assert!((kpi.avg_latency_ms - 5.0).abs() < 1e-9); // (4+6+8+2)/4
        assert_eq!(kpi.fatigue_trigger_count, 2); // 0.7, 0.9 >= 0.6
        assert!((kpi.fatigue_trigger_rate - 0.5).abs() < 1e-9);
        assert!((kpi.fatigue_threshold - 0.6).abs() < 1e-9);
    }

    #[test]
    fn algorithm_distribution_groups_and_excludes_empty() {
        let store = fresh_store();
        let now = Utc::now().to_rfc3339();
        {
            let conn = store.conn().unwrap();
            let rows = [
                ("a1", "ensemble"),
                ("a2", "ensemble"),
                ("a3", "ensemble"),
                ("a4", "mdm"),
                ("a5", ""),
                ("a6", ""),
            ];
            for (id, algo) in rows {
                conn.execute(
                    "INSERT INTO engine_monitoring_events
                     (id,user_id,session_id,timestamp,routing_algo)
                     VALUES (?1,'u1','s1',?2,?3)",
                    params![id, now, algo],
                )
                .unwrap();
            }
        }
        let dist = store.aggregate_amas_algorithm_distribution(7).unwrap();
        assert_eq!(dist.len(), 2); // 空 routing_algo 被排除
        assert_eq!(dist[0].algorithm, "ensemble");
        assert_eq!(dist[0].count, 3);
        assert!((dist[0].pct - 0.75).abs() < 1e-9); // 3/(3+1)
        assert_eq!(dist[1].algorithm, "mdm");
        assert_eq!(dist[1].count, 1);
    }
}
