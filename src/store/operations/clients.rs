use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

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

    pub fn get_recently_active_clients(&self, minutes: i64) -> Result<Vec<ClientDevice>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT device_id, platform, user_id, first_seen_at, last_seen_at,
                    is_banned, banned_at, banned_by, ban_reason
             FROM client_devices
             WHERE last_seen_at >= datetime('now', ?1)
             ORDER BY last_seen_at DESC",
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
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM client_devices WHERE device_id = ?1)",
                params![device_id],
                |r| r.get(0),
            )?;
        Ok(exists)
    }
}
