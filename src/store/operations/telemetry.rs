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
            .get_telemetry_summaries_by_device("dev2", 10, 0)
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
            .get_telemetry_summaries_by_device("nope", 10, 0)
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
            .get_telemetry_summaries_by_device("dev3", 10, 0)
            .unwrap();
        let s = &sums[0];
        assert!(s.behavior_summary.click_targets.is_none());
        assert!(s.feature_usage.is_object());
        assert!(s.feature_usage.as_object().unwrap().is_empty());
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
