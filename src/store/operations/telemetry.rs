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
            let payload = serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Object(Default::default()));
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
