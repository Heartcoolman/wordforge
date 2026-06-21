use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryRecord {
    pub id: String,
    pub device_id: String,
    pub user_id: Option<String>,
    pub event_type: String,
    pub triggered_by_request_id: Option<String>,
    pub payload: serde_json::Value,
    pub client_ts: String,
    pub server_ts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProfile {
    pub cpu_cores: Option<i64>,
    pub memory_gb: Option<f64>,
    pub screen_width: Option<i64>,
    pub screen_height: Option<i64>,
    pub pixel_ratio: Option<f64>,
    pub os_name: Option<String>,
    pub browser_name: Option<String>,
    pub browser_version: Option<String>,
    pub timezone: Option<String>,
    pub language: Option<String>,
    pub touch_support: Option<bool>,
    pub online_status: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub session_duration_secs: i64,
    pub actions_per_min: f64,
    pub error_count: i64,
    pub avg_response_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BehaviorSummary {
    pub current_route: Option<String>,
    pub click_count: Option<i64>,
    pub click_targets: Option<serde_json::Value>,
    pub scroll_depth_pct: Option<f64>,
    pub visibility_changes: Option<i64>,
    pub route_changes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySummary {
    pub id: String,
    pub device_id: String,
    pub user_id: Option<String>,
    pub event_type: String,
    pub server_ts: String,
    pub device_profile: DeviceProfile,
    pub session_stats: SessionStats,
    pub behavior_summary: BehaviorSummary,
    pub feature_usage: serde_json::Value,
}

/// 单个 event_type 的全量聚合(设备管理"遥测记录"面板分类 chip + 每类聚合行)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEventTypeStat {
    pub event_type: String,
    pub count: i64,
    pub avg_duration_secs: f64,
    pub total_errors: i64,
    pub avg_actions_per_min: f64,
    pub avg_response_ms: f64,
}

/// 名称→次数的聚合项(功能使用 / 访问页面 / 点击热点排行)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameCount {
    pub name: String,
    pub count: i64,
}

/// 设备遥测分类总览:全量计数 + 时间范围 + 按 event_type 分组聚合 + 恒定设备画像
/// + "这台设备做了什么"操作概览(功能/页面/点击/累计,全量聚合)。
/// 计数走全量(不受分页 limit 影响),保证面板概览与分类 chip 准确反映全部记录。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryDeviceSummary {
    pub total: i64,
    pub first_ts: Option<String>,
    pub last_ts: Option<String>,
    pub by_event_type: Vec<TelemetryEventTypeStat>,
    /// 最近一条记录的设备画像(同设备恒定,面板顶部只显示一次)。
    pub device_profile: Option<DeviceProfile>,
    /// featureUsage 全量累加排行(用了哪些功能)。
    pub feature_usage: Vec<NameCount>,
    /// currentRoute 分组(去过哪些页面)。
    pub routes: Vec<NameCount>,
    /// clickTargets 按 label 聚合(点了什么的热点)。
    pub click_targets: Vec<NameCount>,
    pub total_clicks: i64,
    pub total_errors: i64,
    pub total_duration_secs: i64,
    pub session_count: i64,
}

/// HashMap 聚合 → 按次数降序(同次数按名升序稳定)取 top N。
fn top_name_counts(map: std::collections::HashMap<String, i64>, limit: usize) -> Vec<NameCount> {
    let mut v: Vec<NameCount> = map
        .into_iter()
        .map(|(name, count)| NameCount { name, count })
        .collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    v.truncate(limit);
    v
}

pub struct TelemetrySummaryInput {
    pub cpu_cores: Option<i64>,
    pub memory_gb: Option<f64>,
    pub screen_width: Option<i64>,
    pub screen_height: Option<i64>,
    pub pixel_ratio: Option<f64>,
    pub os_name: Option<String>,
    pub browser_name: Option<String>,
    pub browser_version: Option<String>,
    pub timezone: Option<String>,
    pub language: Option<String>,
    pub touch_support: Option<bool>,
    pub online_status: Option<bool>,
    pub session_duration_secs: i64,
    pub actions_per_min: f64,
    pub error_count: i64,
    pub avg_response_time_ms: f64,
    pub current_route: Option<String>,
    pub click_count: Option<i64>,
    pub click_targets_json: Option<String>,
    pub scroll_depth_pct: Option<f64>,
    pub visibility_changes: Option<i64>,
    pub route_changes: Option<i64>,
    pub feature_usage_json: String,
}

impl Store {
    #[allow(clippy::too_many_arguments)]
    pub fn insert_telemetry(
        &self,
        id: &str,
        device_id: &str,
        user_id: &str,
        event_type: &str,
        triggered_by_request_id: Option<&str>,
        payload_json: &str,
        client_ts: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO telemetry_events (id, device_id, user_id, event_type, triggered_by_request_id, payload_json, client_ts, server_ts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))",
            params![id, device_id, user_id, event_type, triggered_by_request_id, payload_json, client_ts],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_telemetry_and_summary(
        &self,
        id: &str,
        device_id: &str,
        user_id: &str,
        event_type: &str,
        triggered_by_request_id: Option<&str>,
        payload_json: &str,
        client_ts: &str,
        summary: &TelemetrySummaryInput,
    ) -> Result<(), StoreError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO telemetry_events (id, device_id, user_id, event_type, triggered_by_request_id, payload_json, client_ts, server_ts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))",
            params![id, device_id, user_id, event_type, triggered_by_request_id, payload_json, client_ts],
        )?;
        tx.execute(
            "INSERT INTO telemetry_summaries (
                id, device_id, user_id, event_type, server_ts,
                cpu_cores, memory_gb, screen_width, screen_height, pixel_ratio,
                os_name, browser_name, browser_version, timezone, language,
                touch_support, online_status,
                session_duration_secs, actions_per_min, error_count, avg_response_time_ms,
                current_route, click_count, click_targets_json, scroll_depth_pct,
                visibility_changes, route_changes, feature_usage_json
             ) VALUES (
                ?1, ?2, ?3, ?4, datetime('now'),
                ?5, ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13, ?14,
                ?15, ?16,
                ?17, ?18, ?19, ?20,
                ?21, ?22, ?23, ?24,
                ?25, ?26, ?27
             )",
            params![
                id,
                device_id,
                user_id,
                event_type,
                summary.cpu_cores,
                summary.memory_gb,
                summary.screen_width,
                summary.screen_height,
                summary.pixel_ratio,
                summary.os_name.as_deref(),
                summary.browser_name.as_deref(),
                summary.browser_version.as_deref(),
                summary.timezone.as_deref(),
                summary.language.as_deref(),
                summary.touch_support.map(|b| b as i64),
                summary.online_status.map(|b| b as i64),
                summary.session_duration_secs,
                summary.actions_per_min,
                summary.error_count,
                summary.avg_response_time_ms,
                summary.current_route.as_deref(),
                summary.click_count,
                summary.click_targets_json.as_deref(),
                summary.scroll_depth_pct,
                summary.visibility_changes,
                summary.route_changes,
                summary.feature_usage_json,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// `event_type` 传空串 = 不过滤(全部);非空则只取该类型。
    pub fn get_telemetry_summaries_by_device(
        &self,
        device_id: &str,
        event_type: &str,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<TelemetrySummary>, u64), StoreError> {
        let conn = self.conn()?;

        let total: u64 = conn.query_row(
            "SELECT COUNT(*) FROM telemetry_summaries
             WHERE device_id = ?1 AND (?2 = '' OR event_type = ?2)",
            params![device_id, event_type],
            |r| r.get::<_, i64>(0).map(|v| v as u64),
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, device_id, user_id, event_type, server_ts,
                    cpu_cores, memory_gb, screen_width, screen_height, pixel_ratio,
                    os_name, browser_name, browser_version, timezone, language,
                    touch_support, online_status,
                    session_duration_secs, actions_per_min, error_count, avg_response_time_ms,
                    current_route, click_count, click_targets_json, scroll_depth_pct,
                    visibility_changes, route_changes, feature_usage_json
             FROM telemetry_summaries
             WHERE device_id = ?1 AND (?2 = '' OR event_type = ?2)
             ORDER BY server_ts DESC
             LIMIT ?3 OFFSET ?4",
        )?;

        let rows = stmt.query_map(params![device_id, event_type, limit, offset], |r| {
            let click_targets: Option<serde_json::Value> = r
                .get::<_, Option<String>>(23)?
                .and_then(|s| serde_json::from_str(&s).ok());
            let feature_usage: serde_json::Value = r
                .get::<_, String>(27)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Object(Default::default()));

            Ok(TelemetrySummary {
                id: r.get(0)?,
                device_id: r.get(1)?,
                user_id: r.get(2)?,
                event_type: r.get(3)?,
                server_ts: r.get(4)?,
                device_profile: DeviceProfile {
                    cpu_cores: r.get(5)?,
                    memory_gb: r.get(6)?,
                    screen_width: r.get(7)?,
                    screen_height: r.get(8)?,
                    pixel_ratio: r.get(9)?,
                    os_name: r.get(10)?,
                    browser_name: r.get(11)?,
                    browser_version: r.get(12)?,
                    timezone: r.get(13)?,
                    language: r.get(14)?,
                    touch_support: r.get::<_, Option<i64>>(15)?.map(|v| v != 0),
                    online_status: r.get::<_, Option<i64>>(16)?.map(|v| v != 0),
                },
                session_stats: SessionStats {
                    session_duration_secs: r.get(17)?,
                    actions_per_min: r.get(18)?,
                    error_count: r.get(19)?,
                    avg_response_time_ms: r.get(20)?,
                },
                behavior_summary: BehaviorSummary {
                    current_route: r.get(21)?,
                    click_count: r.get(22)?,
                    click_targets,
                    scroll_depth_pct: r.get(24)?,
                    visibility_changes: r.get(25)?,
                    route_changes: r.get(26)?,
                },
                feature_usage,
            })
        })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok((records, total))
    }

    /// 设备遥测分类总览:全量按 event_type 分组聚合 + 时间范围 + 最近一条设备画像。
    pub fn get_telemetry_device_summary(
        &self,
        device_id: &str,
    ) -> Result<TelemetryDeviceSummary, StoreError> {
        let conn = self.conn()?;

        let (total, first_ts, last_ts): (i64, Option<String>, Option<String>) = conn.query_row(
            "SELECT COUNT(*), MIN(server_ts), MAX(server_ts)
             FROM telemetry_summaries WHERE device_id = ?1",
            params![device_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;

        // 饱和聚合兜底:SUM(CAST(x AS REAL)) 求和不抛溢出,再钳到 i64 区间(历史脏行也不再 500)。
        let saturate_i64 = |v: f64| -> i64 {
            if !v.is_finite() {
                0
            } else if v >= i64::MAX as f64 {
                i64::MAX
            } else if v <= i64::MIN as f64 {
                i64::MIN
            } else {
                v as i64
            }
        };

        let mut stmt = conn.prepare(
            "SELECT event_type, COUNT(*),
                    AVG(session_duration_secs), SUM(CAST(error_count AS REAL)),
                    AVG(actions_per_min), AVG(avg_response_time_ms)
             FROM telemetry_summaries
             WHERE device_id = ?1
             GROUP BY event_type
             ORDER BY COUNT(*) DESC, event_type ASC",
        )?;
        let by_event_type = stmt
            .query_map(params![device_id], |r| {
                Ok(TelemetryEventTypeStat {
                    event_type: r.get(0)?,
                    count: r.get(1)?,
                    avg_duration_secs: r.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                    total_errors: saturate_i64(r.get::<_, Option<f64>>(3)?.unwrap_or(0.0)),
                    avg_actions_per_min: r.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
                    avg_response_ms: r.get::<_, Option<f64>>(5)?.unwrap_or(0.0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // 最近一条记录的设备画像(同设备恒定,面板顶部只显示一次)。
        let device_profile = conn
            .query_row(
                "SELECT cpu_cores, memory_gb, screen_width, screen_height, pixel_ratio,
                        os_name, browser_name, browser_version, timezone, language,
                        touch_support, online_status
                 FROM telemetry_summaries WHERE device_id = ?1
                 ORDER BY server_ts DESC LIMIT 1",
                params![device_id],
                |r| {
                    Ok(DeviceProfile {
                        cpu_cores: r.get(0)?,
                        memory_gb: r.get(1)?,
                        screen_width: r.get(2)?,
                        screen_height: r.get(3)?,
                        pixel_ratio: r.get(4)?,
                        os_name: r.get(5)?,
                        browser_name: r.get(6)?,
                        browser_version: r.get(7)?,
                        timezone: r.get(8)?,
                        language: r.get(9)?,
                        touch_support: r.get::<_, Option<i64>>(10)?.map(|v| v != 0),
                        online_status: r.get::<_, Option<i64>>(11)?.map(|v| v != 0),
                    })
                },
            )
            .ok();

        // 累计标量 + 会话数(增量口径:click/error/duration 按心跳累加;会话=session_start 条数)。
        // P1 纵深防御:SUM(整数列) 在脏数据(超大/负值)下会触发 i64 溢出致整端点永久 500。
        // 设备级累计:SUM(CAST(... AS REAL)) 读为 f64(IEEE-754 不抛溢出),再饱和钳到 i64(复用上方 saturate_i64)。
        let (sum_clicks, sum_errors, sum_duration, session_count): (f64, f64, f64, i64) = conn
            .query_row(
                "SELECT COALESCE(SUM(CAST(click_count AS REAL)),0.0),
                        COALESCE(SUM(CAST(error_count AS REAL)),0.0),
                        COALESCE(SUM(CAST(session_duration_secs AS REAL)),0.0),
                        COALESCE(SUM(CASE WHEN event_type='session_start' THEN 1 ELSE 0 END),0)
                 FROM telemetry_summaries WHERE device_id = ?1",
                params![device_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        let total_clicks = saturate_i64(sum_clicks);
        let total_errors = saturate_i64(sum_errors);
        let total_duration_secs = saturate_i64(sum_duration);

        // 访问页面:current_route 分组计数(去过哪些页面)。
        let mut route_stmt = conn.prepare(
            "SELECT current_route, COUNT(*) FROM telemetry_summaries
             WHERE device_id = ?1 AND current_route IS NOT NULL AND current_route <> ''
             GROUP BY current_route ORDER BY COUNT(*) DESC, current_route ASC LIMIT 12",
        )?;
        let routes = route_stmt
            .query_map(params![device_id], |r| {
                Ok(NameCount { name: r.get(0)?, count: r.get(1)? })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // featureUsage(map)+ clickTargets(array)需在 Rust 侧逐行 JSON 解析合并。
        // P2 全表扫描兜底:单设备无 LIMIT 逐行解析,极端膨胀(异常设备刷量)会拖垮端点。
        // 加 ORDER BY server_ts DESC LIMIT K 上限,只聚合最近 K 行——featureUsage/clickTargets
        // 是「近期用了什么/点了什么」概览,最近样本足够代表,无需扫全表。
        // K=5000:覆盖正常设备全部历史(远超数月心跳),又对脏设备封顶。
        const AGG_ROW_CAP: usize = 5000;
        let mut agg_stmt = conn.prepare(
            "SELECT feature_usage_json, click_targets_json
             FROM telemetry_summaries WHERE device_id = ?1
             ORDER BY server_ts DESC LIMIT ?2",
        )?;
        let mut feature_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let mut click_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let agg_rows = agg_stmt.query_map(params![device_id, AGG_ROW_CAP as i64], |r| {
            Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        for row in agg_rows {
            let (fu, ct) = row?;
            if let Some(s) = fu {
                if let Ok(serde_json::Value::Object(map)) =
                    serde_json::from_str::<serde_json::Value>(&s)
                {
                    for (k, v) in map {
                        // P2 饱和累加:跳过非法/负值,saturating_add 防溢出回绕。
                        match v.as_i64() {
                            Some(n) if n >= 0 => {
                                let e = feature_map.entry(k).or_insert(0);
                                *e = e.saturating_add(n);
                            }
                            _ => {}
                        }
                    }
                }
            }
            if let Some(s) = ct {
                if let Ok(serde_json::Value::Array(arr)) =
                    serde_json::from_str::<serde_json::Value>(&s)
                {
                    for item in arr {
                        let label = item
                            .get("label")
                            .and_then(|v| v.as_str())
                            .or_else(|| item.get("tag").and_then(|v| v.as_str()))
                            .unwrap_or("")
                            .trim();
                        if !label.is_empty() {
                            let e = click_map.entry(label.to_string()).or_insert(0);
                            *e = e.saturating_add(1);
                        }
                    }
                }
            }
        }

        Ok(TelemetryDeviceSummary {
            total,
            first_ts,
            last_ts,
            by_event_type,
            device_profile,
            feature_usage: top_name_counts(feature_map, 12),
            routes,
            click_targets: top_name_counts(click_map, 8),
            total_clicks,
            total_errors,
            total_duration_secs,
            session_count,
        })
    }

    pub fn get_telemetry_by_device(
        &self,
        device_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<TelemetryRecord>, u64), StoreError> {
        let conn = self.conn()?;

        let total: u64 = conn.query_row(
            "SELECT COUNT(*) FROM telemetry_events WHERE device_id = ?1",
            params![device_id],
            |r| r.get::<_, i64>(0).map(|v| v as u64),
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, device_id, user_id, event_type, triggered_by_request_id,
                    payload_json, client_ts, server_ts
             FROM telemetry_events
             WHERE device_id = ?1
             ORDER BY server_ts DESC
             LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![device_id, limit, offset], |r| {
            let payload_str: String = r.get(5)?;
            let payload = serde_json::from_str(&payload_str)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            Ok(TelemetryRecord {
                id: r.get(0)?,
                device_id: r.get(1)?,
                user_id: r.get(2)?,
                event_type: r.get(3)?,
                triggered_by_request_id: r.get(4)?,
                payload,
                client_ts: r.get(6)?,
                server_ts: r.get(7)?,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok((records, total))
    }

    /// retention 清理:删除 `server_ts < cutoff` 的 telemetry_events + telemetry_summaries
    /// 两表行,返回两表总删除行数(retention-worker 按 telemetry_retention_days 调用)。
    ///
    /// 两表在单个 `BEGIN IMMEDIATE` 事务内删除,保证「事件 ⟺ 汇总」要么全删要么全留,
    /// 不出现只清一表的撕裂状态。`server_ts` 由入库侧统一写 `datetime('now')`(`YYYY-MM-DD HH:MM:SS`
    /// 空格格式),**`cutoff` 必须同格式**(见 telemetry_cleanup::retention_cutoff_str);只有两侧
    /// 同格式时 TEXT 字典序才与时间序一致,`<` 比较才安全(RFC3339 的 'T'(0x54) 会错位于空格(0x20))。
    ///
    /// 返回类型与 `probe.rs::delete_probe_older_than` 对齐(`Result<u64, StoreError>`,
    /// 返回 u64);契约文字写 usize,此处遵循本仓既有 Store 删除函数口径用 u64。
    pub fn delete_telemetry_older_than(&self, cutoff: &str) -> Result<u64, StoreError> {
        self.with_user_tx(|tx| {
            let events = tx.execute(
                "DELETE FROM telemetry_events WHERE server_ts < ?1",
                params![cutoff],
            )?;
            let summaries = tx.execute(
                "DELETE FROM telemetry_summaries WHERE server_ts < ?1",
                params![cutoff],
            )?;
            Ok((events + summaries) as u64)
        })
    }

    /// m061：摄取拒绝留痕表 retention 清理,删除 server_ts < cutoff 的行,返回删除行数。
    /// **注意**:本表 server_ts 由 `insert_ingest_rejection` 写入 `to_rfc3339()`（'T' 分隔 + 时区后缀,
    /// 与 `aggregate_ingest_rejections` 的 cutoff 口径一致),与 telemetry_events 的 `datetime('now')`
    /// 空格格式不同。故 cutoff 必须同为 RFC3339,严禁复用 `delete_telemetry_older_than` 的空格 cutoff,
    /// 否则 TEXT 字典序比较因 'T'(0x54) vs 空格(0x20) 失配。
    pub fn delete_ingest_rejections_older_than(
        &self,
        cutoff_rfc3339: &str,
    ) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM telemetry_ingest_rejections WHERE server_ts < ?1",
            params![cutoff_rfc3339],
        )?;
        Ok(deleted as u64)
    }

    /// m061：摄取拒绝留痕（fire-and-forget，由摄取热路径调用）。server_ts 用当前 UTC RFC3339。
    pub fn insert_ingest_rejection(
        &self,
        code: &str,
        device_id: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO telemetry_ingest_rejections (code, device_id, user_id, server_ts)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                code,
                device_id,
                user_id,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    /// m061：近 N 天按拒绝码聚合计数，降序。handler 计算 pct。
    pub fn aggregate_ingest_rejections(
        &self,
        days: u32,
    ) -> Result<Vec<(String, i64)>, StoreError> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT code, COUNT(*) AS n
             FROM telemetry_ingest_rejections
             WHERE server_ts >= ?1
             GROUP BY code
             ORDER BY n DESC",
        )?;
        let rows = stmt
            .query_map(params![cutoff], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    fn full_summary_input() -> TelemetrySummaryInput {
        TelemetrySummaryInput {
            cpu_cores: Some(8),
            memory_gb: Some(16.0),
            screen_width: Some(1920),
            screen_height: Some(1080),
            pixel_ratio: Some(2.0),
            os_name: Some("macOS".into()),
            browser_name: Some("Chrome".into()),
            browser_version: Some("130".into()),
            timezone: Some("Asia/Shanghai".into()),
            language: Some("zh-CN".into()),
            touch_support: Some(false),
            online_status: Some(true),
            session_duration_secs: 60,
            actions_per_min: 12.5,
            error_count: 1,
            avg_response_time_ms: 250.0,
            current_route: Some("/home".into()),
            click_count: Some(8),
            click_targets_json: Some(r#"["btn-a","btn-b"]"#.into()),
            scroll_depth_pct: Some(0.75),
            visibility_changes: Some(3),
            route_changes: Some(5),
            feature_usage_json: r#"{"feat-a":2}"#.into(),
        }
    }

    #[test]
    fn device_summary_aggregates_operations() {
        let (_t, store) = test_store();
        let mut s1 = full_summary_input();
        s1.current_route = Some("/learning".into());
        s1.feature_usage_json = r#"{"search":3,"edit":1}"#.into();
        s1.click_targets_json = Some(r#"[{"label":"开始","tag":"button"}]"#.into());
        s1.click_count = Some(5);
        s1.error_count = 0;
        s1.session_duration_secs = 60;
        store
            .insert_telemetry_and_summary("a1", "devX", "u", "periodic", None, "{}", "2026-05-01T12:00:00Z", &s1)
            .unwrap();

        let mut s2 = full_summary_input();
        s2.current_route = Some("/learning".into());
        s2.feature_usage_json = r#"{"search":2}"#.into();
        s2.click_targets_json = Some(r#"[{"label":"开始"},{"label":"设置"}]"#.into());
        s2.click_count = Some(4);
        s2.error_count = 2;
        s2.session_duration_secs = 60;
        store
            .insert_telemetry_and_summary("a2", "devX", "u", "periodic", None, "{}", "2026-05-01T12:01:00Z", &s2)
            .unwrap();

        let mut s3 = full_summary_input();
        s3.current_route = Some("/review".into());
        s3.feature_usage_json = r#"{"review":1}"#.into();
        s3.click_targets_json = Some("[]".into());
        s3.click_count = Some(0);
        s3.error_count = 0;
        s3.session_duration_secs = 30;
        store
            .insert_telemetry_and_summary("a3", "devX", "u", "session_start", None, "{}", "2026-05-01T12:02:00Z", &s3)
            .unwrap();

        let sum = store.get_telemetry_device_summary("devX").unwrap();
        assert_eq!(sum.total, 3);
        assert_eq!(sum.total_clicks, 9);
        assert_eq!(sum.total_errors, 2);
        assert_eq!(sum.total_duration_secs, 150);
        assert_eq!(sum.session_count, 1);
        // byEventType:periodic(2) 在前,session_start(1)
        assert_eq!(sum.by_event_type[0].event_type, "periodic");
        assert_eq!(sum.by_event_type[0].count, 2);
        // featureUsage 累加:search=3+2=5 居首
        assert_eq!(sum.feature_usage[0].name, "search");
        assert_eq!(sum.feature_usage[0].count, 5);
        // routes:/learning(2) 在前
        assert_eq!(sum.routes[0].name, "/learning");
        assert_eq!(sum.routes[0].count, 2);
        // clickTargets:开始 出现 2 次居首(对象 label 聚合)
        assert_eq!(sum.click_targets[0].name, "开始");
        assert_eq!(sum.click_targets[0].count, 2);
        assert!(sum.device_profile.is_some());
    }

    #[test]
    fn device_summary_empty_device_is_all_zero() {
        let (_t, store) = test_store();
        let sum = store.get_telemetry_device_summary("ghost").unwrap();
        assert_eq!(sum.total, 0);
        assert_eq!(sum.total_clicks, 0);
        assert_eq!(sum.session_count, 0);
        assert!(sum.first_ts.is_none());
        assert!(sum.by_event_type.is_empty());
        assert!(sum.feature_usage.is_empty());
        assert!(sum.device_profile.is_none());
    }

    #[test]
    fn insert_telemetry_creates_event_row() {
        let (_t, store) = test_store();
        store
            .insert_telemetry(
                "id1",
                "dev",
                "user",
                "periodic",
                Some("req-1"),
                "{\"k\":1}",
                "2026-05-01T12:00:00Z",
            )
            .unwrap();
        let (rows, total) = store.get_telemetry_by_device("dev", 10, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.id, "id1");
        assert_eq!(r.event_type, "periodic");
        assert_eq!(r.triggered_by_request_id.as_deref(), Some("req-1"));
        assert_eq!(r.payload["k"], serde_json::json!(1));
    }

    #[test]
    fn insert_with_summary_persists_both_tables() {
        let (_t, store) = test_store();
        let summary = full_summary_input();
        store
            .insert_telemetry_and_summary(
                "id2",
                "dev2",
                "u",
                "session_start",
                None,
                "{}",
                "2026-05-01T12:00:00Z",
                &summary,
            )
            .unwrap();
        let (evt, total_evt) = store.get_telemetry_by_device("dev2", 10, 0).unwrap();
        assert_eq!(total_evt, 1);
        assert_eq!(evt[0].triggered_by_request_id, None);

        let (sums, total) = store
            .get_telemetry_summaries_by_device("dev2", "", 10, 0)
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(sums.len(), 1);
        let s = &sums[0];
        assert_eq!(s.id, "id2");
        assert_eq!(s.device_profile.cpu_cores, Some(8));
        assert_eq!(s.device_profile.touch_support, Some(false));
        assert_eq!(s.device_profile.online_status, Some(true));
        assert_eq!(s.session_stats.session_duration_secs, 60);
        assert_eq!(s.behavior_summary.current_route.as_deref(), Some("/home"));
        let targets = s.behavior_summary.click_targets.as_ref().unwrap();
        assert_eq!(targets[0], serde_json::json!("btn-a"));
        assert_eq!(s.feature_usage["feat-a"], serde_json::json!(2));
    }

    #[test]
    fn pagination_respects_limit_and_offset() {
        let (_t, store) = test_store();
        for i in 0..3 {
            store
                .insert_telemetry(
                    &format!("e{i}"),
                    "dev",
                    "u",
                    "periodic",
                    None,
                    "{}",
                    &format!("2026-05-01T12:00:0{i}Z"),
                )
                .unwrap();
        }
        let (page, total) = store.get_telemetry_by_device("dev", 2, 0).unwrap();
        assert_eq!(total, 3);
        assert_eq!(page.len(), 2);
        let (page2, _) = store.get_telemetry_by_device("dev", 2, 2).unwrap();
        assert_eq!(page2.len(), 1);
    }

    #[test]
    fn empty_device_returns_empty_zero() {
        let (_t, store) = test_store();
        let (rows, total) = store.get_telemetry_by_device("nope", 10, 0).unwrap();
        assert!(rows.is_empty());
        assert_eq!(total, 0);
        let (sums, total_s) = store
            .get_telemetry_summaries_by_device("nope", "", 10, 0)
            .unwrap();
        assert!(sums.is_empty());
        assert_eq!(total_s, 0);
    }

    #[test]
    fn summary_row_with_null_click_targets_and_corrupt_feature_usage_recovers_gracefully() {
        let (_t, store) = test_store();
        let mut summary = full_summary_input();
        summary.click_targets_json = None;
        summary.feature_usage_json = "not-json".into();
        store
            .insert_telemetry_and_summary(
                "id3",
                "dev3",
                "u",
                "periodic",
                None,
                "{}",
                "2026-05-01T12:00:00Z",
                &summary,
            )
            .unwrap();
        let (sums, _) = store
            .get_telemetry_summaries_by_device("dev3", "", 10, 0)
            .unwrap();
        let s = &sums[0];
        assert!(s.behavior_summary.click_targets.is_none());
        assert!(s.feature_usage.is_object());
        assert!(s.feature_usage.as_object().unwrap().is_empty());
    }

    #[test]
    fn delete_telemetry_older_than_purges_both_tables_by_server_ts() {
        let (_t, store) = test_store();
        // 直接以显式 server_ts 写入(绕过 insert 的 datetime('now')),覆盖两表。
        {
            let conn = store.conn().unwrap();
            // 旧行:events + summaries 各一条 server_ts=2026-01-01。
            conn.execute(
                "INSERT INTO telemetry_events (id, device_id, user_id, event_type, payload_json, client_ts, server_ts)
                 VALUES ('e-old','d','u','periodic','{}','2026-01-01T00:00:00Z','2026-01-01 00:00:00')",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO telemetry_summaries (id, device_id, user_id, event_type, server_ts, session_duration_secs, actions_per_min, error_count, avg_response_time_ms, feature_usage_json)
                 VALUES ('s-old','d','u','periodic','2026-01-01 00:00:00',0,0,0,0,'{}')",
                [],
            ).unwrap();
            // 新行:events + summaries 各一条 server_ts=2026-05-19。
            conn.execute(
                "INSERT INTO telemetry_events (id, device_id, user_id, event_type, payload_json, client_ts, server_ts)
                 VALUES ('e-new','d','u','periodic','{}','2026-05-19T00:00:00Z','2026-05-19 00:00:00')",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO telemetry_summaries (id, device_id, user_id, event_type, server_ts, session_duration_secs, actions_per_min, error_count, avg_response_time_ms, feature_usage_json)
                 VALUES ('s-new','d','u','periodic','2026-05-19 00:00:00',0,0,0,0,'{}')",
                [],
            ).unwrap();
        }

        // cutoff 在两批之间(与生产 datetime('now') 同空格格式):应删旧行两条,保留新行两条。
        let deleted = store
            .delete_telemetry_older_than("2026-03-01 00:00:00")
            .unwrap();
        assert_eq!(deleted, 2);

        let (evt, total_evt) = store.get_telemetry_by_device("d", 10, 0).unwrap();
        assert_eq!(total_evt, 1);
        assert_eq!(evt[0].id, "e-new");
        let (_sums, total_sum) = store
            .get_telemetry_summaries_by_device("d", "", 10, 0)
            .unwrap();
        assert_eq!(total_sum, 1);
    }

    #[test]
    fn device_summary_survives_overflow_dirty_rows() {
        let (_t, store) = test_store();
        // 写入两条:一条正常,一条超大 click_count/error_count(脏数据,旧 SUM 会 i64 溢出 500)。
        let mut ok = full_summary_input();
        ok.click_count = Some(5);
        ok.error_count = 1;
        store
            .insert_telemetry_and_summary("ok", "devO", "u", "periodic", None, "{}", "2026-05-01T12:00:00Z", &ok)
            .unwrap();
        let mut dirty = full_summary_input();
        dirty.click_count = Some(i64::MAX);
        dirty.error_count = i64::MAX;
        store
            .insert_telemetry_and_summary("dirty", "devO", "u", "periodic", None, "{}", "2026-05-01T12:01:00Z", &dirty)
            .unwrap();

        // 不再 500:返回 Ok,累计标量饱和到 i64::MAX 而非 panic/溢出。
        let sum = store.get_telemetry_device_summary("devO").unwrap();
        assert_eq!(sum.total, 2);
        assert_eq!(sum.total_clicks, i64::MAX);
        assert_eq!(sum.total_errors, i64::MAX);
    }

    #[test]
    fn record_payload_corrupt_falls_back_to_empty_object() {
        let (_t, store) = test_store();
        store
            .insert_telemetry(
                "id4",
                "dev4",
                "u",
                "periodic",
                None,
                "not-json",
                "2026-05-01T12:00:00Z",
            )
            .unwrap();
        let (rows, _) = store.get_telemetry_by_device("dev4", 10, 0).unwrap();
        assert!(rows[0].payload.is_object());
        assert!(rows[0].payload.as_object().unwrap().is_empty());
    }
}
