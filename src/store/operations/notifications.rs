use chrono::{DateTime, Utc};
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
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

impl Store {
    pub fn batch_create_notifications(
        &self,
        entries: &[(String, String, serde_json::Value)],
    ) -> Result<(), StoreError> {
        let mut batch = sled::Batch::default();
        for (user_id, notification_id, value) in entries {
            let key = keys::notification_key(user_id, notification_id)?;
            let bytes = serde_json::to_vec(value)?;
            batch.insert(key.as_bytes(), bytes);
        }
        self.notifications.apply_batch(batch)?;
        Ok(())
    }

    pub fn list_notifications(
        &self,
        user_id: &str,
        limit: usize,
        unread_only: bool,
    ) -> Result<Vec<Notification>, StoreError> {
        let prefix = keys::notification_prefix(user_id)?;
        let mut notifications = Vec::new();

        for item in self.notifications.scan_prefix(prefix.as_bytes()) {
            let (_, raw) = match item {
                Ok(kv) => kv,
                Err(_) => continue,
            };
            if let Ok(notification) = Self::deserialize::<Notification>(&raw) {
                if unread_only && notification.read {
                    continue;
                }
                notifications.push(notification);
            }
        }

        notifications.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        notifications.truncate(limit);
        Ok(notifications)
    }

    pub fn mark_notification_read(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<Option<Notification>, StoreError> {
        let key = keys::notification_key(user_id, notification_id)?;
        let Some(raw) = self.notifications.get(key.as_bytes())? else {
            return Ok(None);
        };

        let mut notification: Notification = Self::deserialize(&raw)?;
        notification.read = true;
        self.notifications
            .insert(key.as_bytes(), Self::serialize(&notification)?)?;
        Ok(Some(notification))
    }

    pub fn mark_all_notifications_read(&self, user_id: &str) -> Result<u32, StoreError> {
        let prefix = keys::notification_prefix(user_id)?;
        let mut updates = Vec::new();
        let mut marked_read = 0u32;

        for item in self.notifications.scan_prefix(prefix.as_bytes()) {
            let (key, raw) = match item {
                Ok(kv) => kv,
                Err(_) => continue,
            };
            if let Ok(mut notification) = Self::deserialize::<Notification>(&raw) {
                if notification.read {
                    continue;
                }
                notification.read = true;
                if let Ok(bytes) = Self::serialize(&notification) {
                    updates.push((key.to_vec(), bytes));
                    marked_read += 1;
                }
            }
        }

        let mut first_error = None;
        for (key, value) in updates {
            if let Err(error) = self.notifications.insert(key, value) {
                tracing::warn!(error = %error, "Failed to mark notification as read");
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }

        if let Some(error) = first_error {
            return Err(StoreError::Sled(error));
        }

        Ok(marked_read)
    }

    pub fn delete_notification(
        &self,
        user_id: &str,
        notification_id: &str,
    ) -> Result<bool, StoreError> {
        let key = keys::notification_key(user_id, notification_id)?;
        Ok(self.notifications.remove(key.as_bytes())?.is_some())
    }

    pub fn count_unread_notifications(&self, user_id: &str) -> Result<u64, StoreError> {
        let prefix = keys::notification_prefix(user_id)?;
        let mut unread_count = 0u64;

        for item in self.notifications.scan_prefix(prefix.as_bytes()) {
            let (_, raw) = match item {
                Ok(kv) => kv,
                Err(_) => continue,
            };
            if let Ok(notification) = Self::deserialize::<Notification>(&raw) {
                if !notification.read {
                    unread_count += 1;
                }
            }
        }

        Ok(unread_count)
    }
}
