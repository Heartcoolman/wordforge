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

    pub fn get_telemetry_summaries_by_device(
        &self,
        device_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<TelemetrySummary>, u64), StoreError> {
        let conn = self.conn()?;

        let total: u64 = conn.query_row(
            "SELECT COUNT(*) FROM telemetry_summaries WHERE device_id = ?1",
            params![device_id],
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
             WHERE device_id = ?1
             ORDER BY server_ts DESC
             LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt.query_map(params![device_id, limit, offset], |r| {
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
}
