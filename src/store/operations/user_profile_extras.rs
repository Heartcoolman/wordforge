//! v1.1-P2.5：自 `extras.rs` 拆出的用户层个性化数据访问。
//! 包含 badges / reward_preference / habit_profile / user_preferences / user_avatar。
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Badge {
    pub user_id: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub unlocked: bool,
    pub progress: f64,
    pub unlocked_at: Option<String>,
}

impl Store {
    pub fn get_badge(&self, user_id: &str, badge_id: &str) -> Result<Option<Badge>, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(badge_id)?;
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT user_id, id, name, description, unlocked, progress, unlocked_at
                 FROM badges WHERE user_id=?1 AND id=?2",
                params![user_id, badge_id],
                |r| {
                    Ok(Badge {
                        user_id: r.get(0)?,
                        id: r.get(1)?,
                        name: r.get(2)?,
                        description: r.get(3)?,
                        unlocked: r.get::<_, i64>(4)? != 0,
                        progress: r.get(5)?,
                        unlocked_at: r.get(6)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_badge(&self, badge: &Badge) -> Result<(), StoreError> {
        keys::validate_id(&badge.user_id)?;
        keys::validate_id(&badge.id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO badges (user_id, id, name, description, unlocked, progress, unlocked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(user_id, id) DO UPDATE SET name=?3, description=?4, unlocked=?5, progress=?6, unlocked_at=?7",
            params![
                &badge.user_id,
                &badge.id,
                &badge.name,
                &badge.description,
                badge.unlocked as i64,
                badge.progress,
                &badge.unlocked_at,
            ],
        )?;
        Ok(())
    }

    // -- Reward Preferences --

    pub fn get_reward_preference(
        &self,
        user_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let reward_type: Option<String> = conn
            .query_row(
                "SELECT reward_type FROM reward_preferences WHERE user_id=?1",
                params![user_id],
                |r| r.get(0),
            )
            .optional()?;
        match reward_type {
            Some(t) => Ok(Some(serde_json::json!({"reward_type": t}))),
            None => Ok(None),
        }
    }

    pub fn set_reward_preference(
        &self,
        user_id: &str,
        pref: &serde_json::Value,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let reward_type = pref
            .get("reward_type")
            .and_then(|v| v.as_str())
            .unwrap_or("standard");
        conn.execute(
            "INSERT INTO reward_preferences (user_id, reward_type) VALUES (?1, ?2)
             ON CONFLICT(user_id) DO UPDATE SET reward_type=?2",
            params![user_id, reward_type],
        )?;
        Ok(())
    }

    // -- Habit Profiles --

    pub fn get_habit_profile(
        &self,
        user_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let json: Option<String> = conn
            .query_row(
                "SELECT preferred_hours_json, median_session_length_mins, sessions_per_day,
                        temporal_hourly_stats_json, temporal_total_sessions
                 FROM habit_profiles WHERE user_id=?1",
                params![user_id],
                |r| {
                    Ok(serde_json::json!({
                        "preferred_hours": serde_json::from_str::<serde_json::Value>(&r.get::<_, String>(0)?).unwrap_or_default(),
                        "median_session_length_mins": r.get::<_, f64>(1)?,
                        "sessions_per_day": r.get::<_, f64>(2)?,
                        "temporal_hourly_stats": serde_json::from_str::<serde_json::Value>(&r.get::<_, String>(3)?).unwrap_or_default(),
                        "temporal_total_sessions": r.get::<_, i64>(4)?,
                    })
                    .to_string())
                },
            )
            .optional()?;
        match json {
            Some(s) => Ok(Some(Self::deserialize_json(&s)?)),
            None => Ok(None),
        }
    }

    pub fn set_habit_profile(
        &self,
        user_id: &str,
        profile: &serde_json::Value,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let preferred_hours = Self::serialize_json(
            &profile
                .get("preferred_hours")
                .unwrap_or(&serde_json::json!([9, 14, 20])),
        )?;
        let median = profile
            .get("median_session_length_mins")
            .and_then(|v| v.as_f64())
            .unwrap_or(15.0);
        let sessions = profile
            .get("sessions_per_day")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);
        let hourly = Self::serialize_json(
            &profile
                .get("temporal_hourly_stats")
                .unwrap_or(&serde_json::json!([])),
        )?;
        let total_sessions = profile
            .get("temporal_total_sessions")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO habit_profiles (user_id, preferred_hours_json, median_session_length_mins,
             sessions_per_day, temporal_hourly_stats_json, temporal_total_sessions)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(user_id) DO UPDATE SET
                preferred_hours_json=?2, median_session_length_mins=?3, sessions_per_day=?4,
                temporal_hourly_stats_json=?5, temporal_total_sessions=?6",
            params![
                user_id,
                preferred_hours,
                median,
                sessions,
                hourly,
                total_sessions
            ],
        )?;
        Ok(())
    }

    // -- User Preferences --

    pub fn get_user_preferences(
        &self,
        user_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        conn.query_row(
            "SELECT theme, language, notification_enabled, sound_enabled, wordbook_center_url
             FROM user_preferences WHERE user_id=?1",
            params![user_id],
            |r| {
                Ok(serde_json::json!({
                    "theme": r.get::<_, String>(0)?,
                    "language": r.get::<_, String>(1)?,
                    "notification_enabled": r.get::<_, i64>(2)? != 0,
                    "sound_enabled": r.get::<_, i64>(3)? != 0,
                    "wordbook_center_url": r.get::<_, Option<String>>(4)?,
                }))
            },
        )
        .optional()
        .map_err(StoreError::from)
    }

    pub fn set_user_preferences(
        &self,
        user_id: &str,
        prefs: &serde_json::Value,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let theme = prefs
            .get("theme")
            .and_then(|v| v.as_str())
            .unwrap_or("light");
        let language = prefs
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("en");
        let notif = prefs
            .get("notification_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true) as i64;
        let sound = prefs
            .get("sound_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true) as i64;
        let wbc_url = prefs.get("wordbook_center_url").and_then(|v| v.as_str());
        conn.execute(
            "INSERT INTO user_preferences (user_id, theme, language, notification_enabled, sound_enabled, wordbook_center_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(user_id) DO UPDATE SET theme=?2, language=?3, notification_enabled=?4, sound_enabled=?5, wordbook_center_url=?6",
            params![user_id, theme, language, notif, sound, wbc_url],
        )?;
        Ok(())
    }

    // -- User Avatars --

    pub fn set_user_avatar(
        &self,
        user_id: &str,
        avatar: &serde_json::Value,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let url = avatar
            .get("avatarUrl")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let filename = avatar
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let ext = avatar
            .get("extension")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let size = avatar
            .get("sizeBytes")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO user_avatars (user_id, avatar_url, filename, extension, size_bytes) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(user_id) DO UPDATE SET avatar_url=?2, filename=?3, extension=?4, size_bytes=?5",
            params![user_id, url, filename, ext, size],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    #[test]
    fn badge_save_and_get_roundtrip() {
        let (_t, store) = test_store();
        let badge = Badge {
            user_id: "u1".into(),
            id: "b1".into(),
            name: "starter".into(),
            description: "first badge".into(),
            unlocked: true,
            progress: 0.5,
            unlocked_at: Some("2026-05-01T00:00:00Z".into()),
        };
        store.save_badge(&badge).unwrap();
        let got = store.get_badge("u1", "b1").unwrap().unwrap();
        assert_eq!(got.name, "starter");
        assert!(got.unlocked);
        assert!((got.progress - 0.5).abs() < 1e-9);

        // upsert 覆盖
        let mut updated = badge.clone();
        updated.progress = 1.0;
        store.save_badge(&updated).unwrap();
        let got = store.get_badge("u1", "b1").unwrap().unwrap();
        assert!((got.progress - 1.0).abs() < 1e-9);
    }

    #[test]
    fn badge_get_missing_returns_none_and_validation_errors() {
        let (_t, store) = test_store();
        assert!(store.get_badge("u1", "missing").unwrap().is_none());
        assert!(matches!(
            store.get_badge("", "x").unwrap_err(),
            StoreError::Validation(_)
        ));
        let bad = Badge {
            user_id: "".into(),
            id: "x".into(),
            name: "".into(),
            description: "".into(),
            unlocked: false,
            progress: 0.0,
            unlocked_at: None,
        };
        assert!(matches!(
            store.save_badge(&bad).unwrap_err(),
            StoreError::Validation(_)
        ));
    }

    #[test]
    fn reward_preference_default_string_and_roundtrip() {
        let (_t, store) = test_store();
        assert!(store.get_reward_preference("u1").unwrap().is_none());
        // 不带 reward_type 时取默认 standard
        store.set_reward_preference("u1", &json!({})).unwrap();
        let got = store.get_reward_preference("u1").unwrap().unwrap();
        assert_eq!(got["reward_type"], json!("standard"));
        store
            .set_reward_preference("u1", &json!({"reward_type":"gold"}))
            .unwrap();
        let got = store.get_reward_preference("u1").unwrap().unwrap();
        assert_eq!(got["reward_type"], json!("gold"));
    }

    #[test]
    fn habit_profile_uses_defaults_when_fields_missing() {
        let (_t, store) = test_store();
        store.set_habit_profile("u1", &json!({})).unwrap();
        let got = store.get_habit_profile("u1").unwrap().unwrap();
        assert!((got["median_session_length_mins"].as_f64().unwrap() - 15.0).abs() < 1e-9);
        assert!((got["sessions_per_day"].as_f64().unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(got["temporal_total_sessions"], json!(0));
        assert_eq!(got["preferred_hours"], json!([9, 14, 20]));
    }

    #[test]
    fn habit_profile_persists_explicit_values() {
        let (_t, store) = test_store();
        let profile = json!({
            "preferred_hours": [8, 12, 22],
            "median_session_length_mins": 20.0,
            "sessions_per_day": 2.5,
            "temporal_hourly_stats": [{"hour":8,"count":1}],
            "temporal_total_sessions": 7
        });
        store.set_habit_profile("u1", &profile).unwrap();
        let got = store.get_habit_profile("u1").unwrap().unwrap();
        assert_eq!(got["preferred_hours"], json!([8, 12, 22]));
        assert_eq!(got["temporal_total_sessions"], json!(7));
    }

    #[test]
    fn user_preferences_default_and_explicit() {
        let (_t, store) = test_store();
        store.set_user_preferences("u1", &json!({})).unwrap();
        let got = store.get_user_preferences("u1").unwrap().unwrap();
        assert_eq!(got["theme"], json!("light"));
        assert_eq!(got["language"], json!("en"));
        assert_eq!(got["notification_enabled"], json!(true));

        store
            .set_user_preferences(
                "u1",
                &json!({
                    "theme":"dark","language":"zh","notification_enabled":false,"sound_enabled":false,
                    "wordbook_center_url":"https://example.com"
                }),
            )
            .unwrap();
        let got = store.get_user_preferences("u1").unwrap().unwrap();
        assert_eq!(got["theme"], json!("dark"));
        assert_eq!(got["notification_enabled"], json!(false));
        assert_eq!(got["wordbook_center_url"], json!("https://example.com"));
    }

    #[test]
    fn user_avatar_persists_payload() {
        let (_t, store) = test_store();
        store
            .set_user_avatar(
                "u1",
                &json!({
                    "avatarUrl":"u","filename":"a.png","extension":"png","sizeBytes":100
                }),
            )
            .unwrap();
        // 写第二次 upsert
        store
            .set_user_avatar(
                "u1",
                &json!({
                    "avatarUrl":"u2","filename":"b.png","extension":"png","sizeBytes":200
                }),
            )
            .unwrap();
        let conn = store.connection().unwrap();
        let (url, size): (String, i64) = conn
            .query_row(
                "SELECT avatar_url, size_bytes FROM user_avatars WHERE user_id='u1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(url, "u2");
        assert_eq!(size, 200);
    }
}
