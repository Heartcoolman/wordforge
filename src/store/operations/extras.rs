use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

// -- Badges (Routes layer) --

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

    // -- Reward Preferences (Routes layer) --

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

    // -- Habit Profiles (Routes layer) --

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
            &profile.get("preferred_hours").unwrap_or(&serde_json::json!([9, 14, 20])),
        )?;
        let median = profile.get("median_session_length_mins").and_then(|v| v.as_f64()).unwrap_or(15.0);
        let sessions = profile.get("sessions_per_day").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let hourly = Self::serialize_json(
            &profile.get("temporal_hourly_stats").unwrap_or(&serde_json::json!([])),
        )?;
        let total_sessions = profile.get("temporal_total_sessions").and_then(|v| v.as_i64()).unwrap_or(0);
        conn.execute(
            "INSERT INTO habit_profiles (user_id, preferred_hours_json, median_session_length_mins,
             sessions_per_day, temporal_hourly_stats_json, temporal_total_sessions)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(user_id) DO UPDATE SET
                preferred_hours_json=?2, median_session_length_mins=?3, sessions_per_day=?4,
                temporal_hourly_stats_json=?5, temporal_total_sessions=?6",
            params![user_id, preferred_hours, median, sessions, hourly, total_sessions],
        )?;
        Ok(())
    }

    // -- User Preferences (Routes layer) --

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
        let theme = prefs.get("theme").and_then(|v| v.as_str()).unwrap_or("light");
        let language = prefs.get("language").and_then(|v| v.as_str()).unwrap_or("en");
        let notif = prefs.get("notification_enabled").and_then(|v| v.as_bool()).unwrap_or(true) as i64;
        let sound = prefs.get("sound_enabled").and_then(|v| v.as_bool()).unwrap_or(true) as i64;
        let wbc_url = prefs.get("wordbook_center_url").and_then(|v| v.as_str());
        conn.execute(
            "INSERT INTO user_preferences (user_id, theme, language, notification_enabled, sound_enabled, wordbook_center_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(user_id) DO UPDATE SET theme=?2, language=?3, notification_enabled=?4, sound_enabled=?5, wordbook_center_url=?6",
            params![user_id, theme, language, notif, sound, wbc_url],
        )?;
        Ok(())
    }

    // -- User Avatars (Routes layer) --

    pub fn set_user_avatar(&self, user_id: &str, avatar: &serde_json::Value) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let url = avatar.get("avatarUrl").and_then(|v| v.as_str()).unwrap_or("");
        let filename = avatar.get("filename").and_then(|v| v.as_str()).unwrap_or("");
        let ext = avatar.get("extension").and_then(|v| v.as_str()).unwrap_or("");
        let size = avatar.get("sizeBytes").and_then(|v| v.as_i64()).unwrap_or(0);
        conn.execute(
            "INSERT INTO user_avatars (user_id, avatar_url, filename, extension, size_bytes) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(user_id) DO UPDATE SET avatar_url=?2, filename=?3, extension=?4, size_bytes=?5",
            params![user_id, url, filename, ext, size],
        )?;
        Ok(())
    }

    // -- Etymologies (Routes layer + Workers layer) --

    pub fn get_etymology(&self, word_id: &str) -> Result<Option<serde_json::Value>, StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        conn.query_row(
            "SELECT word_id, word, etymology, roots_json, generated, source, generated_at
             FROM etymologies WHERE word_id=?1",
            params![word_id],
            |r| {
                Ok(serde_json::json!({
                    "word_id": r.get::<_, String>(0)?,
                    "word": r.get::<_, String>(1)?,
                    "etymology": r.get::<_, String>(2)?,
                    "roots": serde_json::from_str::<serde_json::Value>(&r.get::<_, String>(3)?).unwrap_or_default(),
                    "generated": r.get::<_, i64>(4)? != 0,
                    "source": r.get::<_, Option<String>>(5)?,
                    "generated_at": r.get::<_, Option<String>>(6)?,
                }))
            },
        )
        .optional()
        .map_err(StoreError::from)
    }

    pub fn set_etymology(&self, word_id: &str, data: &serde_json::Value) -> Result<(), StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        let word = data.get("word").and_then(|v| v.as_str()).unwrap_or("");
        let etymology = data.get("etymology").and_then(|v| v.as_str()).unwrap_or("");
        let roots = Self::serialize_json(&data.get("roots").unwrap_or(&serde_json::json!([])))?;
        let generated = data.get("generated").and_then(|v| v.as_bool()).unwrap_or(false) as i64;
        let source = data.get("source").and_then(|v| v.as_str());
        let generated_at = data.get("generated_at").and_then(|v| v.as_str());
        conn.execute(
            "INSERT INTO etymologies (word_id, word, etymology, roots_json, generated, source, generated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(word_id) DO UPDATE SET word=?2, etymology=?3, roots_json=?4, generated=?5, source=?6, generated_at=?7",
            params![word_id, word, etymology, roots, generated, source, generated_at],
        )?;
        Ok(())
    }

    pub fn delete_etymology(&self, word_id: &str) -> Result<(), StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        conn.execute("DELETE FROM etymologies WHERE word_id=?1", params![word_id])?;
        Ok(())
    }

    // -- Workers layer methods --

    pub fn insert_monitoring_timeseries(
        &self,
        period_id: &str,
        data: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let json = Self::serialize_json(data)?;
        conn.execute(
            "INSERT INTO monitoring_timeseries (period_id, data_json) VALUES (?1, ?2)
             ON CONFLICT(period_id) DO UPDATE SET data_json=?2",
            params![period_id, json],
        )?;
        Ok(())
    }

    pub fn cleanup_old_monitoring_events(&self, before_timestamp: &str) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM engine_monitoring_events WHERE timestamp < ?1",
            params![before_timestamp],
        )?;
        Ok(deleted)
    }

    pub fn cleanup_expired_reset_tokens(&self) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        let deleted = conn.execute(
            "DELETE FROM password_reset_tokens WHERE expires_at < ?1",
            params![now],
        )?;
        Ok(deleted)
    }

    pub fn list_words_without_etymology(&self, limit: usize) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT w.id, w.text FROM words w
             LEFT JOIN etymologies e ON w.id = e.word_id
             WHERE e.word_id IS NULL LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "text": r.get::<_, String>(1)?,
            }))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    pub fn list_all_words(&self) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT id, text, difficulty FROM words")?;
        let rows = stmt.query_map([], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "text": r.get::<_, String>(1)?,
                "difficulty": r.get::<_, f64>(2)?,
            }))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    pub fn get_alert_dedup(&self, user_id: &str, word_id: &str) -> Result<Option<i64>, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        conn.query_row(
            "SELECT last_alerted_at_ms FROM alert_dedup WHERE user_id=?1 AND word_id=?2",
            params![user_id, word_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(StoreError::from)
    }

    pub fn set_alert_dedup(&self, user_id: &str, word_id: &str, ts_ms: i64) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO alert_dedup (user_id, word_id, last_alerted_at_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id, word_id) DO UPDATE SET last_alerted_at_ms=?3",
            params![user_id, word_id, ts_ms],
        )?;
        Ok(())
    }

    pub fn db_size_bytes(&self) -> Result<i64, StoreError> {
        let conn = self.conn()?;
        let page_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok(page_count * page_size)
    }

    pub fn db_table_list(&self) -> Result<Vec<String>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    pub fn get_confusion_pairs_for_word(
        &self,
        word_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, f64)>, StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT word_id_a, word_id_b, score FROM confusion_pairs
             WHERE word_id_a = ?1 OR word_id_b = ?1
             ORDER BY score DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![word_id, limit as i64], |r| {
            let a: String = r.get(0)?;
            let b: String = r.get(1)?;
            let score: f64 = r.get(2)?;
            let other = if a == word_id { b } else { a };
            Ok((other, score))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    pub fn set_confusion_pair(
        &self,
        word_id_a: &str,
        word_id_b: &str,
        score: f64,
    ) -> Result<(), StoreError> {
        keys::validate_id(word_id_a)?;
        keys::validate_id(word_id_b)?;
        let (a, b) = keys::canonical_pair(word_id_a, word_id_b);
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO confusion_pairs (word_id_a, word_id_b, score, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(word_id_a, word_id_b) DO UPDATE SET score=?3, updated_at=?4",
            params![a, b, score, now],
        )?;
        Ok(())
    }

    // -- Password Reset Tokens --

    pub fn create_password_reset_token(
        &self,
        token_hash: &str,
        user_id: &str,
        expires_at: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO password_reset_tokens (token_hash, user_id, expires_at) VALUES (?1, ?2, ?3)",
            params![token_hash, user_id, expires_at],
        )?;
        Ok(())
    }

    pub fn get_password_reset_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT token_hash, user_id, expires_at FROM password_reset_tokens WHERE token_hash=?1",
            params![token_hash],
            |r| {
                Ok(serde_json::json!({
                    "token_hash": r.get::<_, String>(0)?,
                    "user_id": r.get::<_, String>(1)?,
                    "expires_at": r.get::<_, String>(2)?,
                }))
            },
        )
        .optional()
        .map_err(StoreError::from)
    }

    pub fn take_password_reset_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let entry = conn
            .query_row(
                "DELETE FROM password_reset_tokens WHERE token_hash=?1
                 RETURNING token_hash, user_id, expires_at",
                params![token_hash],
                |r| {
                    Ok(serde_json::json!({
                        "token_hash": r.get::<_, String>(0)?,
                        "user_id": r.get::<_, String>(1)?,
                        "expires_at": r.get::<_, String>(2)?,
                    }))
                },
            )
            .optional()?;
        Ok(entry)
    }

    // -- Word Morphemes Write --

    pub fn set_word_morphemes(
        &self,
        word_id: &str,
        morphemes: &[serde_json::Value],
    ) -> Result<(), StoreError> {
        keys::validate_id(word_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM word_morphemes WHERE word_id=?1", params![word_id])?;
        for (i, m) in morphemes.iter().enumerate() {
            let text = m.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let mtype = m.get("type").or(m.get("morpheme_type")).and_then(|v| v.as_str()).unwrap_or("");
            let meaning = m.get("meaning").and_then(|v| v.as_str()).unwrap_or("");
            tx.execute(
                "INSERT INTO word_morphemes (word_id, position, text, morpheme_type, meaning)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![word_id, i as i64, text, mtype, meaning],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Aggregate (total_records, total_correct) since a given timestamp.
    pub fn aggregate_records_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<(u64, u64), StoreError> {
        let conn = self.conn()?;
        let (total, correct): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN is_correct=1 THEN 1 ELSE 0 END), 0)
             FROM learning_records WHERE created_at >= ?1",
            params![since.to_rfc3339()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((total as u64, correct as u64))
    }

    /// Aggregate daily stats: (total, correct, unique_users, unique_words) for records on or after `since`.
    pub fn daily_aggregation_stats(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<(u64, u64, u64, u64), StoreError> {
        let conn = self.conn()?;
        let since_str = since.to_rfc3339();
        let (total, correct): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN is_correct=1 THEN 1 ELSE 0 END), 0)
             FROM learning_records WHERE created_at >= ?1",
            params![&since_str],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let unique_users: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT user_id) FROM learning_records WHERE created_at >= ?1",
            params![&since_str],
            |r| r.get(0),
        )?;
        let unique_words: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT word_id) FROM learning_records WHERE created_at >= ?1",
            params![&since_str],
            |r| r.get(0),
        )?;
        Ok((total as u64, correct as u64, unique_users as u64, unique_words as u64))
    }

    /// Create a single notification.
    pub fn create_notification(&self, notification: &serde_json::Value) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let user_id = notification["userId"].as_str().unwrap_or_default();
        let id = notification["id"].as_str().unwrap_or_default();
        let ntype = notification["type"].as_str().unwrap_or("system");
        let title = notification["title"].as_str().unwrap_or_default();
        let message = notification["message"].as_str().unwrap_or_default();
        let word_id = notification["wordId"].as_str();
        let overdue_hours = notification["overdueHours"].as_i64();
        let read = notification["read"].as_bool().unwrap_or(false);
        let created_at = notification["createdAt"].as_str().unwrap_or_default();
        conn.execute(
            "INSERT OR IGNORE INTO notifications (user_id, id, notification_type, title, message, word_id, overdue_hours, read, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![user_id, id, ntype, title, message, word_id, overdue_hours, read as i64, created_at],
        )?;
        Ok(())
    }

    /// List all words with tags (for clustering).
    pub fn list_all_words_with_tags(&self) -> Result<Vec<(String, f64, Vec<String>)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT id, difficulty, tags_json FROM words")?;
        let rows = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            let difficulty: f64 = r.get(1)?;
            let tags_json: String = r.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            Ok((id, difficulty, tags))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    /// Get user records with minimal fields for confusion analysis.
    pub fn get_user_records_minimal(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, bool)>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT word_id, is_correct FROM learning_records
             WHERE user_id=?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![user_id, limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    // -- AMAS layer --

    pub fn get_word_morphemes(&self, word_id: &str) -> Result<Option<serde_json::Value>, StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT text, morpheme_type, meaning FROM word_morphemes
             WHERE word_id=?1 ORDER BY position",
        )?;
        let rows: Vec<serde_json::Value> = stmt
            .query_map(params![word_id], |r| {
                Ok(serde_json::json!({
                    "text": r.get::<_, String>(0)?,
                    "type": r.get::<_, String>(1)?,
                    "meaning": r.get::<_, String>(2)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.is_empty() {
            Ok(None)
        } else {
            Ok(Some(serde_json::Value::Array(rows)))
        }
    }
}
