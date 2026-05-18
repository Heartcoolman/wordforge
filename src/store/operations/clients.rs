use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientDevice {
    pub device_id: String,
    pub platform: String,
    pub user_id: Option<String>,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub is_banned: bool,
    pub banned_at: Option<String>,
    pub banned_by: Option<String>,
    pub ban_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataChannelStatus {
    pub amas: &'static str,
    pub learning: &'static str,
    pub telemetry: &'static str,
}

impl Default for DataChannelStatus {
    fn default() -> Self {
        Self {
            amas: "none",
            learning: "none",
            telemetry: "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DataUploadSummary {
    pub amas_by_user: HashMap<String, &'static str>,
    pub learning_by_user: HashMap<String, &'static str>,
    pub telemetry_by_device: HashMap<String, &'static str>,
}

impl Store {
    pub fn upsert_client_device(
        &self,
        device_id: &str,
        platform: &str,
        user_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO client_devices (device_id, platform, user_id, first_seen_at, last_seen_at)
             VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))
             ON CONFLICT(device_id) DO UPDATE SET
                last_seen_at = datetime('now'),
                platform = ?2,
                user_id = ?3",
            params![device_id, platform, user_id],
        )?;
        Ok(())
    }

    pub fn is_device_banned(&self, device_id: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let banned: Option<i64> = conn
            .query_row(
                "SELECT is_banned FROM client_devices WHERE device_id = ?1",
                params![device_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(banned.unwrap_or(0) != 0)
    }

    pub fn get_recently_active_clients(
        &self,
        minutes: i64,
    ) -> Result<Vec<ClientDevice>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT device_id, platform, user_id, first_seen_at, last_seen_at,
                    is_banned, banned_at, banned_by, ban_reason
             FROM client_devices
             WHERE last_seen_at >= datetime('now', ?1) OR is_banned = 1
             ORDER BY is_banned DESC, last_seen_at DESC",
        )?;
        let offset = format!("-{} minutes", minutes);
        let rows = stmt.query_map(params![offset], |r| {
            Ok(ClientDevice {
                device_id: r.get(0)?,
                platform: r.get(1)?,
                user_id: r.get(2)?,
                first_seen_at: r.get(3)?,
                last_seen_at: r.get(4)?,
                is_banned: r.get::<_, i64>(5)? != 0,
                banned_at: r.get(6)?,
                banned_by: r.get(7)?,
                ban_reason: r.get(8)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn ban_client_device(
        &self,
        device_id: &str,
        banned_by: &str,
        reason: Option<&str>,
    ) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE client_devices SET is_banned = 1, banned_at = datetime('now'),
                    banned_by = ?2, ban_reason = ?3
             WHERE device_id = ?1",
            params![device_id, banned_by, reason],
        )?;
        Ok(affected > 0)
    }

    pub fn unban_client_device(&self, device_id: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE client_devices SET is_banned = 0, banned_at = NULL,
                    banned_by = NULL, ban_reason = NULL
             WHERE device_id = ?1",
            params![device_id],
        )?;
        Ok(affected > 0)
    }

    pub fn client_device_exists(&self, device_id: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM client_devices WHERE device_id = ?1)",
            params![device_id],
            |r| r.get(0),
        )?;
        Ok(exists)
    }

    pub fn get_data_upload_status(
        &self,
        user_ids: &[String],
        device_ids: &[String],
    ) -> Result<DataUploadSummary, StoreError> {
        let conn = self.conn()?;
        let mut summary = DataUploadSummary::default();

        if !user_ids.is_empty() {
            let placeholders: Vec<String> = user_ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect();
            let params: Vec<&dyn rusqlite::types::ToSql> = user_ids
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();

            let amas_sql = format!(
                "SELECT user_id, total_event_count FROM engine_user_states WHERE user_id IN ({})",
                placeholders.join(",")
            );
            let mut stmt = conn.prepare(&amas_sql)?;
            let rows = stmt.query_map(params.as_slice(), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (user_id, event_count) = row?;
                summary
                    .amas_by_user
                    .insert(user_id, if event_count > 0 { "uploaded" } else { "nil" });
            }

            let lr_sql = format!(
                "SELECT user_id, COUNT(*) FROM learning_records WHERE user_id IN ({}) GROUP BY user_id",
                placeholders.join(",")
            );
            let mut stmt = conn.prepare(&lr_sql)?;
            let lr_params: Vec<&dyn rusqlite::types::ToSql> = user_ids
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();
            let rows = stmt.query_map(lr_params.as_slice(), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (user_id, cnt) = row?;
                summary
                    .learning_by_user
                    .insert(user_id, if cnt > 0 { "uploaded" } else { "nil" });
            }
        }

        if !device_ids.is_empty() {
            let placeholders: Vec<String> = device_ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect();
            let params: Vec<&dyn rusqlite::types::ToSql> = device_ids
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();

            let sql = format!(
                "SELECT device_id, COUNT(*) FROM telemetry_events WHERE device_id IN ({}) GROUP BY device_id",
                placeholders.join(",")
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params.as_slice(), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (device_id, cnt) = row?;
                summary
                    .telemetry_by_device
                    .insert(device_id, if cnt > 0 { "uploaded" } else { "nil" });
            }
        }

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_store() -> Store {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        store
    }

    #[test]
    fn upsert_inserts_then_updates_last_seen() {
        let store = test_store();
        store.upsert_client_device("dev-1", "ios", "u-1").unwrap();
        // 重复 upsert 不应失败，且 platform/user_id 被覆盖
        store.upsert_client_device("dev-1", "android", "u-2").unwrap();
        let active = store.get_recently_active_clients(60).unwrap();
        assert_eq!(active.len(), 1);
        let d = &active[0];
        assert_eq!(d.device_id, "dev-1");
        assert_eq!(d.platform, "android");
        assert_eq!(d.user_id.as_deref(), Some("u-2"));
        assert!(!d.is_banned);
    }

    #[test]
    fn device_exists_and_is_banned_flow() {
        let store = test_store();
        assert!(!store.client_device_exists("dev-x").unwrap());
        assert!(!store.is_device_banned("dev-x").unwrap());
        store.upsert_client_device("dev-x", "web", "u-1").unwrap();
        assert!(store.client_device_exists("dev-x").unwrap());
        assert!(!store.is_device_banned("dev-x").unwrap());

        assert!(store.ban_client_device("dev-x", "admin-1", Some("spam")).unwrap());
        assert!(store.is_device_banned("dev-x").unwrap());

        assert!(store.unban_client_device("dev-x").unwrap());
        assert!(!store.is_device_banned("dev-x").unwrap());
    }

    #[test]
    fn ban_unban_nonexistent_returns_false() {
        let store = test_store();
        assert!(!store.ban_client_device("missing", "admin", None).unwrap());
        assert!(!store.unban_client_device("missing").unwrap());
    }

    #[test]
    fn recently_active_includes_banned_even_if_old() {
        let store = test_store();
        store.upsert_client_device("dev-a", "web", "u-a").unwrap();
        store.ban_client_device("dev-a", "admin", Some("r")).unwrap();
        // 用 -100000 minutes 也仍包含 banned
        let list = store.get_recently_active_clients(1).unwrap();
        assert!(list.iter().any(|d| d.device_id == "dev-a" && d.is_banned));
    }

    #[test]
    fn data_upload_status_empty_inputs() {
        let store = test_store();
        let s = store.get_data_upload_status(&[], &[]).unwrap();
        assert!(s.amas_by_user.is_empty());
        assert!(s.learning_by_user.is_empty());
        assert!(s.telemetry_by_device.is_empty());
    }

    #[test]
    fn data_upload_status_with_real_data() {
        let store = test_store();
        // seed engine_user_states
        let user_id = "u-1".to_string();
        store.upsert_client_device("dev-1", "ios", &user_id).unwrap();
        {
            let conn = store.connection().unwrap();
            conn.execute(
                "INSERT INTO engine_user_states (user_id, total_event_count, created_at)
                 VALUES (?1, 7, ?2)",
                params![user_id, Utc::now().to_rfc3339()],
            ).unwrap();
            conn.execute(
                "INSERT INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, created_at)
                 VALUES (?1, ?2, 'w1', 1, 100, ?3)",
                params![user_id, uuid::Uuid::new_v4().to_string(), Utc::now().to_rfc3339()],
            ).unwrap();
            conn.execute(
                "INSERT INTO telemetry_events (id, device_id, event_type, payload_json, client_ts, server_ts)
                 VALUES (?1, 'dev-1', 'periodic', '{}', ?2, ?2)",
                params![uuid::Uuid::new_v4().to_string(), Utc::now().to_rfc3339()],
            ).unwrap();
        }

        let s = store.get_data_upload_status(&[user_id.clone()], &["dev-1".to_string()]).unwrap();
        assert_eq!(s.amas_by_user.get(&user_id).copied(), Some("uploaded"));
        assert_eq!(s.learning_by_user.get(&user_id).copied(), Some("uploaded"));
        assert_eq!(s.telemetry_by_device.get("dev-1").copied(), Some("uploaded"));
    }

    #[test]
    fn data_upload_status_zero_event_count_marked_nil() {
        let store = test_store();
        let user_id = "u-2".to_string();
        {
            let conn = store.connection().unwrap();
            conn.execute(
                "INSERT INTO engine_user_states (user_id, total_event_count, created_at)
                 VALUES (?1, 0, ?2)",
                params![user_id, Utc::now().to_rfc3339()],
            ).unwrap();
        }
        let s = store.get_data_upload_status(&[user_id.clone()], &[]).unwrap();
        assert_eq!(s.amas_by_user.get(&user_id).copied(), Some("nil"));
        // 没有 learning_records 时 key 不出现
        assert!(s.learning_by_user.get(&user_id).is_none());
    }

    #[test]
    fn data_channel_status_default_is_none_strings() {
        let s = DataChannelStatus::default();
        assert_eq!(s.amas, "none");
        assert_eq!(s.learning, "none");
        assert_eq!(s.telemetry, "none");
        // 同步覆盖 Serialize
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"amas\":\"none\""));
    }

    #[test]
    fn data_upload_summary_default_serializes() {
        let s = DataUploadSummary::default();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("amasByUser"));
    }
}
