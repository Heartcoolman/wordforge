//! 客户端埋点事件流（app-events, m073）的存取与日聚合。
//!
//! raw 表 `app_events` 摄取即单事务批量 INSERT OR IGNORE（UNIQUE(device_id, client_event_id)
//! 承担跨请求幂等），无内存计数器故无 shutdown flush 需求。日聚合三表由 app_event_rollup
//! worker 对最近 7 天逐日调 [`Store::rollup_app_events_day`] 幂等重算——重算窗口 = 摄取端
//! clientTsMs 回填钳制窗（7d），离线补传的晚到事件最终一致。
//! perf 分位数在 Rust 侧精确计算（SQLite 无 percentile 函数，日级数据量小）。

use rusqlite::params;
use std::collections::BTreeMap;

use crate::store::{Store, StoreError};

/// 摄取端校验/钳制完成后的落库行。
pub struct AppEventRow {
    pub device_id: String,
    pub user_id: String,
    pub platform: String,
    pub app_version: String,
    pub category: String,
    pub name: String,
    pub client_event_id: String,
    pub client_ts_ms: i64,
    pub event_day: String,
    pub props_json: Option<String>,
}

/// 最近邻秩法（nearest-rank）分位：q ∈ (0,1]，输入须升序非空。
pub(crate) fn percentile(sorted: &[i64], q: f64) -> i64 {
    let n = sorted.len();
    let idx = ((q * n as f64).ceil() as usize).clamp(1, n) - 1;
    sorted[idx]
}

impl Store {
    /// 单事务批量幂等插入；返回 (inserted, duplicates)。
    /// duplicates = UNIQUE(device_id, client_event_id) 冲突被 IGNORE 的行数。
    pub fn insert_app_events_batch(
        &self,
        rows: &[AppEventRow],
    ) -> Result<(usize, usize), StoreError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let mut inserted = 0usize;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO app_events
                 (device_id, user_id, platform, app_version, category, name,
                  client_event_id, client_ts_ms, event_day, props_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for r in rows {
                inserted += stmt.execute(params![
                    r.device_id,
                    r.user_id,
                    r.platform,
                    r.app_version,
                    r.category,
                    r.name,
                    r.client_event_id,
                    r.client_ts_ms,
                    r.event_day,
                    r.props_json.as_deref(),
                ])?;
            }
        }
        tx.commit()?;
        Ok((inserted, rows.len() - inserted))
    }

    /// 幂等重算指定 day（'YYYY-MM-DD'）的三张聚合表（单事务）：
    /// app_event_daily 先 DELETE 再重算（perf 行补 p50/p95/p99）；app_user_daily 同；
    /// app_user_first_seen 用 first_day 取小 upsert，重算顺序无关。
    pub fn rollup_app_events_day(&self, day: &str) -> Result<(), StoreError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;

        tx.execute("DELETE FROM app_event_daily WHERE day = ?1", params![day])?;
        tx.execute(
            "INSERT INTO app_event_daily (day, platform, category, name, count, users)
             SELECT event_day, platform, category, name, COUNT(*), COUNT(DISTINCT user_id)
               FROM app_events WHERE event_day = ?1
              GROUP BY platform, category, name",
            params![day],
        )?;

        // perf 分位数：props_json->'$.ms' 升序取值，Rust 侧最近邻秩计算后回写。
        let perf_groups: BTreeMap<(String, String), Vec<i64>> = {
            let mut stmt = tx.prepare(
                "SELECT platform, name, CAST(json_extract(props_json,'$.ms') AS INTEGER)
                   FROM app_events
                  WHERE event_day = ?1 AND category = 'perf'
                    AND json_extract(props_json,'$.ms') IS NOT NULL
                  ORDER BY platform, name, 3",
            )?;
            let mapped = stmt.query_map(params![day], |r| {
                Ok((
                    (r.get::<_, String>(0)?, r.get::<_, String>(1)?),
                    r.get::<_, i64>(2)?,
                ))
            })?;
            let mut groups: BTreeMap<(String, String), Vec<i64>> = BTreeMap::new();
            for row in mapped {
                let (key, ms) = row?;
                groups.entry(key).or_default().push(ms);
            }
            groups
        };
        for ((platform, name), values) in &perf_groups {
            tx.execute(
                "UPDATE app_event_daily SET p50_ms = ?1, p95_ms = ?2, p99_ms = ?3
                  WHERE day = ?4 AND platform = ?5 AND category = 'perf' AND name = ?6",
                params![
                    percentile(values, 0.50),
                    percentile(values, 0.95),
                    percentile(values, 0.99),
                    day,
                    platform,
                    name
                ],
            )?;
        }

        tx.execute("DELETE FROM app_user_daily WHERE day = ?1", params![day])?;
        // 跨平台用户按当日事件数最多的平台归位（PK 一用户一行）。
        tx.execute(
            "INSERT INTO app_user_daily (day, user_id, platform, events)
             SELECT event_day, user_id,
                    (SELECT e2.platform FROM app_events e2
                      WHERE e2.event_day = e.event_day AND e2.user_id = e.user_id
                      GROUP BY e2.platform ORDER BY COUNT(*) DESC, e2.platform LIMIT 1),
                    COUNT(*)
               FROM app_events e WHERE event_day = ?1
              GROUP BY user_id",
            params![day],
        )?;

        tx.execute(
            "INSERT INTO app_user_first_seen (user_id, first_day, platform)
             SELECT user_id, ?1,
                    (SELECT e2.platform FROM app_events e2
                      WHERE e2.event_day = e.event_day AND e2.user_id = e.user_id
                      GROUP BY e2.platform ORDER BY COUNT(*) DESC, e2.platform LIMIT 1)
               FROM app_events e WHERE event_day = ?1
              GROUP BY user_id
             ON CONFLICT(user_id) DO UPDATE SET
                first_day = excluded.first_day, platform = excluded.platform
              WHERE excluded.first_day < first_day",
            params![day],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// raw retention 清扫。cutoff 必须与入库侧 `datetime('now')` 同为空格格式
    /// （`YYYY-MM-DD HH:MM:SS`），理由同 telemetry_cleanup::retention_cutoff_str。
    pub fn delete_app_events_older_than(&self, cutoff: &str) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let n = conn.execute(
            "DELETE FROM app_events WHERE server_ts < ?1",
            params![cutoff],
        )?;
        Ok(n as u64)
    }

    /// app_user_daily 长保留清扫（cutoff_day 为 'YYYY-MM-DD'）。
    pub fn delete_app_user_daily_older_than(&self, cutoff_day: &str) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let n = conn.execute(
            "DELETE FROM app_user_daily WHERE day < ?1",
            params![cutoff_day],
        )?;
        Ok(n as u64)
    }

    // ------------------------------------------------------------------
    // admin 读端点数据源（/api/admin/app-events/*）。
    // 口径：历史日读 rollup 三表，当日（today）补 raw 索引扫，UNION 合并——
    // rollup 由 01:10 worker 生成，当日数据只存在于 raw。
    // ------------------------------------------------------------------

    pub fn admin_app_events_overview(
        &self,
        start: &str,
        end_excl: &str,
        today: &str,
    ) -> Result<AppEventsOverviewRow, StoreError> {
        let conn = self.conn()?;
        let (total_events, error_count): (i64, i64) = conn.query_row(
            "WITH daily AS (
                SELECT category, count FROM app_event_daily
                 WHERE day >= ?1 AND day < ?2 AND day < ?3
                UNION ALL
                SELECT category, COUNT(*) FROM app_events
                 WHERE event_day = ?3 AND event_day >= ?1 AND event_day < ?2
                 GROUP BY category
             )
             SELECT COALESCE(SUM(count), 0),
                    COALESCE(SUM(CASE WHEN category = 'error' THEN count ELSE 0 END), 0)
               FROM daily",
            params![start, end_excl, today],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let (active_days_sum, active_users): (i64, i64) = conn.query_row(
            "WITH udays AS (
                SELECT day, user_id FROM app_user_daily
                 WHERE day >= ?1 AND day < ?2 AND day < ?3
                UNION ALL
                SELECT DISTINCT event_day, user_id FROM app_events
                 WHERE event_day = ?3 AND event_day >= ?1 AND event_day < ?2
             )
             SELECT COUNT(*), COUNT(DISTINCT user_id) FROM udays",
            params![start, end_excl, today],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let crash_users: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT user_id) FROM app_events
              WHERE event_day >= ?1 AND event_day < ?2 AND name = 'crash'",
            params![start, end_excl],
            |r| r.get(0),
        )?;
        // api_rtt P95：日 p95 按 count 加权平均（近似口径，非跨窗真 P95；当日无 rollup 不计入）。
        let api_rtt_p95: Option<f64> = conn.query_row(
            "SELECT SUM(p95_ms * count) * 1.0 / SUM(count) FROM app_event_daily
              WHERE day >= ?1 AND day < ?2 AND day < ?3
                AND name = 'api_rtt' AND p95_ms IS NOT NULL",
            params![start, end_excl, today],
            |r| r.get(0),
        )?;
        Ok(AppEventsOverviewRow {
            total_events,
            error_count,
            active_days_sum,
            active_users,
            crash_users,
            api_rtt_p95,
        })
    }

    pub fn admin_app_events_trend(
        &self,
        start: &str,
        end_excl: &str,
        today: &str,
    ) -> Result<Vec<AppEventsTrendRow>, StoreError> {
        let conn = self.conn()?;
        let mut rows: std::collections::BTreeMap<String, AppEventsTrendRow> = {
            let mut stmt = conn.prepare(
                "WITH daily AS (
                    SELECT day, category, count FROM app_event_daily
                     WHERE day >= ?1 AND day < ?2 AND day < ?3
                    UNION ALL
                    SELECT event_day, category, COUNT(*) FROM app_events
                     WHERE event_day = ?3 AND event_day >= ?1 AND event_day < ?2
                     GROUP BY category
                 )
                 SELECT day,
                        SUM(CASE WHEN category = 'behavior' THEN count ELSE 0 END),
                        SUM(CASE WHEN category = 'error' THEN count ELSE 0 END),
                        SUM(CASE WHEN category = 'perf' THEN count ELSE 0 END)
                   FROM daily GROUP BY day ORDER BY day",
            )?;
            let mapped = stmt.query_map(params![start, end_excl, today], |r| {
                Ok(AppEventsTrendRow {
                    day: r.get(0)?,
                    behavior: r.get(1)?,
                    error: r.get(2)?,
                    perf: r.get(3)?,
                    dau: 0,
                })
            })?;
            let mut map = std::collections::BTreeMap::new();
            for row in mapped {
                let row = row?;
                map.insert(row.day.clone(), row);
            }
            map
        };
        let mut stmt = conn.prepare(
            "WITH udays AS (
                SELECT day, user_id FROM app_user_daily
                 WHERE day >= ?1 AND day < ?2 AND day < ?3
                UNION ALL
                SELECT DISTINCT event_day, user_id FROM app_events
                 WHERE event_day = ?3 AND event_day >= ?1 AND event_day < ?2
             )
             SELECT day, COUNT(*) FROM udays GROUP BY day",
        )?;
        let dau_rows = stmt.query_map(params![start, end_excl, today], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in dau_rows {
            let (day, dau) = row?;
            rows.entry(day.clone())
                .or_insert_with(|| AppEventsTrendRow {
                    day,
                    behavior: 0,
                    error: 0,
                    perf: 0,
                    dau: 0,
                })
                .dau = dau;
        }
        Ok(rows.into_values().collect())
    }

    /// top-events 全走 raw（窗口 ≤90d = raw retention，跨日 distinct users 需要 user 级行）。
    pub fn admin_app_events_top(
        &self,
        start: &str,
        end_excl: &str,
        category: &str,
        limit: u32,
    ) -> Result<Vec<AppEventsTopRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT name, category, COUNT(*), COUNT(DISTINCT user_id)
               FROM app_events
              WHERE event_day >= ?1 AND event_day < ?2 AND (?3 = '' OR category = ?3)
              GROUP BY name, category
              ORDER BY COUNT(*) DESC, name LIMIT ?4",
        )?;
        let mapped = stmt.query_map(params![start, end_excl, category, limit], |r| {
            Ok(AppEventsTopRow {
                name: r.get(0)?,
                category: r.get(1)?,
                count: r.get(2)?,
                users: r.get(3)?,
            })
        })?;
        mapped.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 错误分组（name + props.signature），每组带最近 ≤3 条样本 props。全走 raw。
    pub fn admin_app_events_errors(
        &self,
        start: &str,
        end_excl: &str,
        limit: u32,
    ) -> Result<Vec<AppEventsErrorGroup>, StoreError> {
        let conn = self.conn()?;
        let mut groups: Vec<AppEventsErrorGroup> = {
            let mut stmt = conn.prepare(
                "SELECT name, COALESCE(json_extract(props_json, '$.signature'), ''),
                        COUNT(*), MAX(server_ts)
                   FROM app_events
                  WHERE event_day >= ?1 AND event_day < ?2 AND category = 'error'
                  GROUP BY 1, 2 ORDER BY 3 DESC LIMIT ?3",
            )?;
            let mapped = stmt.query_map(params![start, end_excl, limit], |r| {
                Ok(AppEventsErrorGroup {
                    name: r.get(0)?,
                    signature: r.get(1)?,
                    count: r.get(2)?,
                    last_seen: r.get(3)?,
                    samples: Vec::new(),
                })
            })?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        let mut stmt = conn.prepare(
            "SELECT name, sig, props_json FROM (
                SELECT name, COALESCE(json_extract(props_json, '$.signature'), '') AS sig,
                       props_json,
                       ROW_NUMBER() OVER (
                           PARTITION BY name, COALESCE(json_extract(props_json, '$.signature'), '')
                           ORDER BY id DESC) AS rn
                  FROM app_events
                 WHERE event_day >= ?1 AND event_day < ?2 AND category = 'error'
             ) WHERE rn <= 3",
        )?;
        let samples = stmt.query_map(params![start, end_excl], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        for row in samples {
            let (name, sig, props) = row?;
            if let Some(g) = groups
                .iter_mut()
                .find(|g| g.name == name && g.signature == sig)
            {
                if let Some(p) = props {
                    g.samples.push(p);
                }
            }
        }
        Ok(groups)
    }

    /// perf 逐日分位：历史日读 rollup，当日从 raw 取 ms 升序值（分位由调用方计算）。
    /// 返回 (窗口内 perf 事件名列表, 历史日序列, 当日 ms 值)。
    pub fn admin_app_events_perf(
        &self,
        start: &str,
        end_excl: &str,
        today: &str,
        name: &str,
    ) -> Result<(Vec<String>, Vec<AppEventsPerfDay>, Vec<i64>), StoreError> {
        let conn = self.conn()?;
        let names: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT name FROM (
                    SELECT name FROM app_event_daily
                     WHERE day >= ?1 AND day < ?2 AND category = 'perf'
                    UNION ALL
                    SELECT name FROM app_events
                     WHERE event_day = ?3 AND event_day >= ?1 AND event_day < ?2
                       AND category = 'perf'
                 ) ORDER BY name",
            )?;
            let mapped = stmt.query_map(params![start, end_excl, today], |r| r.get(0))?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        let series: Vec<AppEventsPerfDay> = {
            let mut stmt = conn.prepare(
                "SELECT day, count, p50_ms, p95_ms, p99_ms FROM app_event_daily
                  WHERE day >= ?1 AND day < ?2 AND day < ?3
                    AND category = 'perf' AND name = ?4
                  ORDER BY day",
            )?;
            let mapped = stmt.query_map(params![start, end_excl, today, name], |r| {
                Ok(AppEventsPerfDay {
                    day: r.get(0)?,
                    count: r.get(1)?,
                    p50_ms: r.get(2)?,
                    p95_ms: r.get(3)?,
                    p99_ms: r.get(4)?,
                })
            })?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        let today_ms: Vec<i64> = {
            let mut stmt = conn.prepare(
                "SELECT CAST(json_extract(props_json, '$.ms') AS INTEGER)
                   FROM app_events
                  WHERE event_day = ?1 AND event_day >= ?2 AND event_day < ?3
                    AND category = 'perf' AND name = ?4
                    AND json_extract(props_json, '$.ms') IS NOT NULL
                  ORDER BY 1",
            )?;
            let mapped = stmt.query_map(params![today, start, end_excl, name], |r| r.get(0))?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        Ok((names, series, today_ms))
    }

    /// 使用漏斗：窗口内 distinct user 达成各步的人数（步定义见 routes/admin/app_events.rs
    /// APP_FUNNEL_DEFS）。全走 raw。
    pub fn admin_app_events_funnel(
        &self,
        start: &str,
        end_excl: &str,
    ) -> Result<[i64; 5], StoreError> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT
                COUNT(DISTINCT CASE WHEN name = 'session_start' THEN user_id END),
                COUNT(DISTINCT CASE WHEN name = 'screen_view'
                    AND json_extract(props_json, '$.screen') = 'study' THEN user_id END),
                COUNT(DISTINCT CASE WHEN name = 'study_start' THEN user_id END),
                COUNT(DISTINCT CASE WHEN name = 'study_complete' THEN user_id END),
                COUNT(DISTINCT CASE WHEN name = 'word_lookup' THEN user_id END)
               FROM app_events
              WHERE event_day >= ?1 AND event_day < ?2",
            params![start, end_excl],
            |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?]),
        )
        .map_err(Into::into)
    }

    /// 留存 cohort 明细：first_seen（限定最早 cohort 起点）× 活跃日 join，
    /// 周分桶与矩阵计算在调用方（数据量 = 活跃行级，小）。
    pub fn admin_app_events_retention_pairs(
        &self,
        min_first_day: &str,
    ) -> Result<Vec<AppRetentionPair>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT f.user_id, f.first_day, d.day
               FROM app_user_first_seen f
               LEFT JOIN app_user_daily d ON d.user_id = f.user_id AND d.day >= f.first_day
              WHERE f.first_day >= ?1",
        )?;
        let mapped = stmt.query_map(params![min_first_day], |r| {
            Ok(AppRetentionPair {
                user_id: r.get(0)?,
                first_day: r.get(1)?,
                active_day: r.get(2)?,
            })
        })?;
        mapped.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 活跃明细 (day, user_id, platform)：窗口向前多取 6 天供 WAU 滚动计算；
    /// 当日补 raw（platform 取当日事件数最多者）。
    pub fn admin_app_events_activity_pairs(
        &self,
        ext_start: &str,
        end_excl: &str,
        today: &str,
    ) -> Result<Vec<(String, String, String)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT day, user_id, platform FROM app_user_daily
              WHERE day >= ?1 AND day < ?2 AND day < ?3
             UNION ALL
             SELECT event_day, user_id,
                    (SELECT e2.platform FROM app_events e2
                      WHERE e2.event_day = e.event_day AND e2.user_id = e.user_id
                      GROUP BY e2.platform ORDER BY COUNT(*) DESC, e2.platform LIMIT 1)
               FROM app_events e
              WHERE event_day = ?3 AND event_day >= ?1 AND event_day < ?2
              GROUP BY user_id",
        )?;
        let mapped = stmt.query_map(params![ext_start, end_excl, today], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        mapped.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

pub struct AppEventsOverviewRow {
    pub total_events: i64,
    pub error_count: i64,
    /// Σ 日 DAU（除以窗口天数 = 平均 DAU）
    pub active_days_sum: i64,
    pub active_users: i64,
    pub crash_users: i64,
    pub api_rtt_p95: Option<f64>,
}

pub struct AppEventsTrendRow {
    pub day: String,
    pub behavior: i64,
    pub error: i64,
    pub perf: i64,
    pub dau: i64,
}

pub struct AppEventsTopRow {
    pub name: String,
    pub category: String,
    pub count: i64,
    pub users: i64,
}

pub struct AppEventsErrorGroup {
    pub name: String,
    pub signature: String,
    pub count: i64,
    pub last_seen: String,
    pub samples: Vec<String>,
}

pub struct AppEventsPerfDay {
    pub day: String,
    pub count: i64,
    pub p50_ms: Option<i64>,
    pub p95_ms: Option<i64>,
    pub p99_ms: Option<i64>,
}

pub struct AppRetentionPair {
    pub user_id: String,
    pub first_day: String,
    pub active_day: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let s = Store::open(":memory:", 5000, 1).unwrap();
        s.run_migrations().unwrap();
        s
    }

    fn row(
        device: &str,
        user: &str,
        platform: &str,
        category: &str,
        name: &str,
        ceid: &str,
        day: &str,
        props: Option<&str>,
    ) -> AppEventRow {
        AppEventRow {
            device_id: device.into(),
            user_id: user.into(),
            platform: platform.into(),
            app_version: "1.0.0".into(),
            category: category.into(),
            name: name.into(),
            client_event_id: ceid.into(),
            client_ts_ms: 1_700_000_000_000,
            event_day: day.into(),
            props_json: props.map(str::to_string),
        }
    }

    #[test]
    fn batch_insert_dedups_on_device_and_event_id() {
        let s = store();
        let rows = vec![
            row("d1", "u1", "web", "behavior", "screen_view", "e1", "2026-08-10", None),
            row("d1", "u1", "web", "behavior", "screen_view", "e2", "2026-08-10", None),
        ];
        assert_eq!(s.insert_app_events_batch(&rows).unwrap(), (2, 0));
        // 同 (device_id, client_event_id) 重放 → duplicates；异 device 同 event id → 插入
        let replay = vec![
            row("d1", "u1", "web", "behavior", "screen_view", "e1", "2026-08-10", None),
            row("d2", "u2", "ios", "behavior", "screen_view", "e1", "2026-08-10", None),
        ];
        assert_eq!(s.insert_app_events_batch(&replay).unwrap(), (1, 1));
    }

    #[test]
    fn rollup_recomputes_idempotently_with_perf_percentiles() {
        let s = store();
        let mut rows = vec![
            row("d1", "u1", "web", "behavior", "screen_view", "b1", "2026-08-10", None),
            row("d1", "u1", "web", "behavior", "screen_view", "b2", "2026-08-10", None),
            row("d2", "u2", "ios", "behavior", "screen_view", "b3", "2026-08-10", None),
        ];
        for (i, ms) in [100i64, 200, 300, 400, 1000].iter().enumerate() {
            rows.push(row(
                "d1",
                "u1",
                "web",
                "perf",
                "api_rtt",
                &format!("p{i}"),
                "2026-08-10",
                Some(&format!("{{\"ms\":{ms}}}")),
            ));
        }
        s.insert_app_events_batch(&rows).unwrap();
        s.rollup_app_events_day("2026-08-10").unwrap();
        // 幂等：重跑不翻倍
        s.rollup_app_events_day("2026-08-10").unwrap();

        let conn = s.conn().unwrap();
        let (count, users): (i64, i64) = conn
            .query_row(
                "SELECT count, users FROM app_event_daily
                  WHERE day='2026-08-10' AND platform='web' AND name='screen_view'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((count, users), (2, 1));
        let (p50, p95, p99): (i64, i64, i64) = conn
            .query_row(
                "SELECT p50_ms, p95_ms, p99_ms FROM app_event_daily
                  WHERE day='2026-08-10' AND platform='web' AND name='api_rtt'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        // 最近邻秩:n=5 → p50=第3个(300), p95/p99=第5个(1000)
        assert_eq!((p50, p95, p99), (300, 1000, 1000));
        let dau: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM app_user_daily WHERE day='2026-08-10'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dau, 2);
    }

    #[test]
    fn first_seen_keeps_earliest_day_regardless_of_rollup_order() {
        let s = store();
        s.insert_app_events_batch(&[
            row("d1", "u1", "web", "behavior", "session_start", "a", "2026-08-09", None),
            row("d1", "u1", "web", "behavior", "session_start", "b", "2026-08-10", None),
        ])
        .unwrap();
        // 逆序重算：后处理更早的 day，first_day 仍应取小
        s.rollup_app_events_day("2026-08-10").unwrap();
        s.rollup_app_events_day("2026-08-09").unwrap();
        let first: String = s
            .conn()
            .unwrap()
            .query_row(
                "SELECT first_day FROM app_user_first_seen WHERE user_id='u1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(first, "2026-08-09");
    }

    #[test]
    fn retention_deletes_by_space_format_cutoff_and_day() {
        let s = store();
        s.insert_app_events_batch(&[row(
            "d1",
            "u1",
            "web",
            "behavior",
            "screen_view",
            "e1",
            "2026-01-01",
            None,
        )])
        .unwrap();
        // server_ts 为 datetime('now')，远未来 cutoff 全删；空格格式（无 'T'）
        assert_eq!(s.delete_app_events_older_than("2099-01-01 00:00:00").unwrap(), 1);
        s.conn()
            .unwrap()
            .execute(
                "INSERT INTO app_user_daily (day, user_id, platform, events)
                 VALUES ('2025-01-01','u1','web',3), ('2026-08-10','u1','web',2)",
                [],
            )
            .unwrap();
        assert_eq!(s.delete_app_user_daily_older_than("2026-01-01").unwrap(), 1);
    }
}
