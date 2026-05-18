use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationType {
    System,
    Achievement,
    Reminder,
    Info,
    Broadcast,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    pub user_id: String,
    #[serde(rename = "type")]
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overdue_hours: Option<i64>,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

fn notification_type_from_str(s: &str) -> NotificationType {
    match s {
        "achievement" => NotificationType::Achievement,
        "reminder" => NotificationType::Reminder,
        "info" => NotificationType::Info,
        "broadcast" => NotificationType::Broadcast,
        _ => NotificationType::System,
    }
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn notification_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Notification> {
    Ok(Notification {
        user_id: row.get(0)?,
        id: row.get(1)?,
        notification_type: notification_type_from_str(&row.get::<_, String>(2)?),
        title: row.get(3)?,
        message: row.get(4)?,
        word_id: row.get(5)?,
        overdue_hours: row.get(6)?,
        read: row.get::<_, i64>(7)? != 0,
        created_at: parse_dt(row.get(8)?)?,
    })
}

const COLS: &str =
    "user_id, id, notification_type, title, message, word_id, overdue_hours, read, created_at";

impl Store {
    pub fn batch_create_notifications(
        &self,
        entries: &[(String, String, serde_json::Value)],
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "INSERT OR IGNORE INTO notifications (user_id, id, notification_type, title, message, word_id, overdue_hours, read, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for (user_id, _notification_id, value) in entries {
            let id = value["id"].as_str().unwrap_or_default();
            let ntype = value["type"].as_str().unwrap_or("system");
            let title = value["title"].as_str().unwrap_or_default();
            let message = value["message"].as_str().unwrap_or_default();
            let word_id = value["wordId"].as_str();
            let overdue_hours = value["overdueHours"].as_i64();
            let read = value["read"].as_bool().unwrap_or(false);
            let created_at = value["createdAt"].as_str().unwrap_or_default();
            stmt.execute(params![
                user_id,
                id,
                ntype,
                title,
                message,
                word_id,
                overdue_hours,
                read as i64,
                created_at,
            ])?;
        }
        Ok(())
    }

    pub fn list_notifications(
        &self,
        user_id: &str,
        limit: usize,
        unread_only: bool,
    ) -> Result<Vec<Notification>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        if unread_only {
            let mut stmt = conn.prepare(
                &format!("SELECT {COLS} FROM notifications WHERE user_id = ?1 AND read = 0 ORDER BY created_at DESC LIMIT ?2"),
            )?;
            let notifications = stmt
                .query_map(params![user_id, limit as i64], notification_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(notifications)
        } else {
            let mut stmt = conn.prepare(
                &format!("SELECT {COLS} FROM notifications WHERE user_id = ?1 ORDER BY created_at DESC LIMIT ?2"),
            )?;
            let notifications = stmt
                .query_map(params![user_id, limit as i64], notification_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(notifications)
        }
    }

    pub fn mark_notification_read(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<Option<Notification>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let updated = conn.execute(
            "UPDATE notifications SET read = 1 WHERE user_id = ?1 AND id = ?2",
            params![user_id, notification_id],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        Ok(conn
            .query_row(
                &format!("SELECT {COLS} FROM notifications WHERE user_id = ?1 AND id = ?2"),
                params![user_id, notification_id],
                notification_from_row,
            )
            .optional()?)
    }

    pub fn mark_all_notifications_read(&self, user_id: &str) -> Result<u32, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let updated = conn.execute(
            "UPDATE notifications SET read = 1 WHERE user_id = ?1 AND read = 0",
            params![user_id],
        )?;
        Ok(updated as u32)
    }

    pub fn delete_notification(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<bool, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM notifications WHERE user_id = ?1 AND id = ?2",
            params![user_id, notification_id],
        )?;
        Ok(deleted > 0)
    }

    pub fn count_unread_notifications(&self, user_id: &str) -> Result<u64, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND read = 0",
            params![user_id],
            |r| r.get(0),
        )?;
        Ok(count as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    fn insert_notification(store: &Store, user_id: &str, id: &str, title: &str, read: bool) {
        let entries = vec![(
            user_id.to_string(),
            id.to_string(),
            serde_json::json!({
                "id": id,
                "userId": user_id,
                "type": "system",
                "title": title,
                "message": "msg",
                "read": read,
                "createdAt": Utc::now().to_rfc3339(),
            }),
        )];
        store.batch_create_notifications(&entries).unwrap();
    }

    #[test]
    fn batch_create_and_list() {
        let store = test_store();
        insert_notification(&store, "u1", "n1", "Title1", false);
        insert_notification(&store, "u1", "n2", "Title2", false);
        insert_notification(&store, "u2", "n3", "Title3", false);
        let list = store.list_notifications("u1", 50, false).unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn list_unread_only() {
        let store = test_store();
        insert_notification(&store, "u1", "n1", "T1", false);
        insert_notification(&store, "u1", "n2", "T2", true);
        let unread = store.list_notifications("u1", 50, true).unwrap();
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].id, "n1");
    }

    #[test]
    fn mark_read() {
        let store = test_store();
        insert_notification(&store, "u1", "n1", "T", false);
        let notif = store.mark_notification_read("u1", "n1").unwrap().unwrap();
        assert!(notif.read);
        assert!(store
            .mark_notification_read("u1", "nope")
            .unwrap()
            .is_none());
    }

    #[test]
    fn mark_all_read() {
        let store = test_store();
        insert_notification(&store, "u1", "n1", "T1", false);
        insert_notification(&store, "u1", "n2", "T2", false);
        insert_notification(&store, "u1", "n3", "T3", true);
        let count = store.mark_all_notifications_read("u1").unwrap();
        assert_eq!(count, 2);
        assert_eq!(store.count_unread_notifications("u1").unwrap(), 0);
    }

    #[test]
    fn delete_notification() {
        let store = test_store();
        insert_notification(&store, "u1", "n1", "T", false);
        assert!(store.delete_notification("u1", "n1").unwrap());
        assert!(!store.delete_notification("u1", "n1").unwrap());
    }

    #[test]
    fn count_unread() {
        let store = test_store();
        insert_notification(&store, "u1", "n1", "T1", false);
        insert_notification(&store, "u1", "n2", "T2", true);
        insert_notification(&store, "u1", "n3", "T3", false);
        assert_eq!(store.count_unread_notifications("u1").unwrap(), 2);
    }

    #[test]
    fn batch_create_with_word_id_and_overdue() {
        let store = test_store();
        let entries = vec![(
            "u1".to_string(),
            "n1".to_string(),
            serde_json::json!({
                "id": "n1",
                "userId": "u1",
                "type": "forgetting_alert",
                "wordId": "w123",
                "overdueHours": 48,
                "read": false,
                "createdAt": Utc::now().to_rfc3339(),
            }),
        )];
        store.batch_create_notifications(&entries).unwrap();
        let list = store.list_notifications("u1", 10, false).unwrap();
        assert_eq!(list[0].word_id.as_deref(), Some("w123"));
        assert_eq!(list[0].overdue_hours, Some(48));
    }
}
