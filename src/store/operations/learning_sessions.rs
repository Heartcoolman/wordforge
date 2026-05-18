use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::constants::MAX_CAS_RETRIES;
use crate::store::keys;
use crate::store::{Store, StoreError};

const COLS: &str = "\
    id, user_id, status, target_mastery_count, total_questions, \
    actual_mastery_count, context_shifts, created_at, updated_at, \
    summary_accuracy, summary_avg_response_time_ms, \
    summary_mastered_word_ids_json, summary_error_prone_word_ids_json, \
    summary_duration_secs, summary_hour_of_day, summary_final_difficulty, \
    correct_count, total_count";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningSession {
    pub id: String,
    pub user_id: String,
    pub status: SessionStatus,
    pub target_mastery_count: u32,
    pub total_questions: u32,
    pub actual_mastery_count: u32,
    pub context_shifts: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub summary: Option<SessionSummary>,
    #[serde(default)]
    pub correct_count: u32,
    #[serde(default)]
    pub total_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub accuracy: f64,
    pub avg_response_time_ms: i64,
    pub mastered_word_ids: Vec<String>,
    pub error_prone_word_ids: Vec<String>,
    pub duration_secs: i64,
    pub hour_of_day: u8,
    pub final_difficulty: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Completed,
    Abandoned,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Abandoned => "abandoned",
        }
    }
}

fn parse_status(s: &str) -> rusqlite::Result<SessionStatus> {
    match s {
        "active" => Ok(SessionStatus::Active),
        "completed" => Ok(SessionStatus::Completed),
        "abandoned" => Ok(SessionStatus::Abandoned),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown session status: {other}").into(),
        )),
    }
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LearningSession> {
    let status_str: String = row.get(2)?;
    let mastered_json: String = row.get(11)?;
    let error_prone_json: String = row.get(12)?;

    let summary_accuracy: Option<f64> = row.get(9)?;
    let summary = summary_accuracy.map(|accuracy| {
        let avg_response_time_ms: i64 = row.get::<_, Option<i64>>(10).ok().flatten().unwrap_or(0);
        let mastered: Vec<String> = serde_json::from_str(&mastered_json).unwrap_or_default();
        let error_prone: Vec<String> = serde_json::from_str(&error_prone_json).unwrap_or_default();
        let duration_secs: i64 = row.get::<_, Option<i64>>(13).ok().flatten().unwrap_or(0);
        let hour_of_day: u8 = row.get::<_, Option<i64>>(14).ok().flatten().unwrap_or(0) as u8;
        let final_difficulty: f64 = row.get::<_, Option<f64>>(15).ok().flatten().unwrap_or(0.0);
        SessionSummary {
            accuracy,
            avg_response_time_ms,
            mastered_word_ids: mastered,
            error_prone_word_ids: error_prone,
            duration_secs,
            hour_of_day,
            final_difficulty,
        }
    });

    Ok(LearningSession {
        id: row.get(0)?,
        user_id: row.get(1)?,
        status: parse_status(&status_str)?,
        target_mastery_count: row.get::<_, i64>(3)? as u32,
        total_questions: row.get::<_, i64>(4)? as u32,
        actual_mastery_count: row.get::<_, i64>(5)? as u32,
        context_shifts: row.get::<_, i64>(6)? as u32,
        created_at: parse_dt(row.get(7)?)?,
        updated_at: parse_dt(row.get(8)?)?,
        summary,
        correct_count: row.get::<_, i64>(16)? as u32,
        total_count: row.get::<_, i64>(17)? as u32,
    })
}

impl Store {
    pub fn create_learning_session(&self, session: &LearningSession) -> Result<(), StoreError> {
        keys::validate_id(&session.id)?;
        keys::validate_id(&session.user_id)?;
        let conn = self.conn()?;

        let mastered_json = match &session.summary {
            Some(s) => Self::serialize_json(&s.mastered_word_ids)?,
            None => "[]".to_string(),
        };
        let error_prone_json = match &session.summary {
            Some(s) => Self::serialize_json(&s.error_prone_word_ids)?,
            None => "[]".to_string(),
        };

        conn.execute(
            &format!(
                "INSERT INTO learning_sessions ({COLS})
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)"
            ),
            params![
                &session.id,
                &session.user_id,
                session.status.as_str(),
                session.target_mastery_count as i64,
                session.total_questions as i64,
                session.actual_mastery_count as i64,
                session.context_shifts as i64,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
                session.summary.as_ref().map(|s| s.accuracy),
                session.summary.as_ref().map(|s| s.avg_response_time_ms),
                mastered_json,
                error_prone_json,
                session.summary.as_ref().map(|s| s.duration_secs),
                session.summary.as_ref().map(|s| s.hour_of_day as i64),
                session.summary.as_ref().map(|s| s.final_difficulty),
                session.correct_count as i64,
                session.total_count as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get_learning_session(
        &self,
        session_id: &str,
    ) -> Result<Option<LearningSession>, StoreError> {
        keys::validate_id(session_id)?;
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {COLS} FROM learning_sessions WHERE id = ?1"),
                params![session_id],
                session_from_row,
            )
            .optional()?)
    }

    pub fn update_learning_session(&self, session: &LearningSession) -> Result<(), StoreError> {
        keys::validate_id(&session.id)?;
        let conn = self.conn()?;

        let mastered_json = match &session.summary {
            Some(s) => Self::serialize_json(&s.mastered_word_ids)?,
            None => "[]".to_string(),
        };
        let error_prone_json = match &session.summary {
            Some(s) => Self::serialize_json(&s.error_prone_word_ids)?,
            None => "[]".to_string(),
        };

        for _ in 0..MAX_CAS_RETRIES {
            let existing = conn
                .query_row(
                    "SELECT updated_at FROM learning_sessions WHERE id=?1",
                    params![&session.id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| StoreError::NotFound {
                    entity: "learning_session".into(),
                    key: session.id.clone(),
                })?;

            match conn.execute(
                "UPDATE learning_sessions SET \
                    user_id=?1, status=?2, target_mastery_count=?3, total_questions=?4, \
                    actual_mastery_count=?5, context_shifts=?6, created_at=?7, updated_at=?8, \
                    summary_accuracy=?9, summary_avg_response_time_ms=?10, \
                    summary_mastered_word_ids_json=?11, summary_error_prone_word_ids_json=?12, \
                    summary_duration_secs=?13, summary_hour_of_day=?14, summary_final_difficulty=?15, \
                    correct_count=?16, total_count=?17 \
                 WHERE id=?18 AND updated_at=?19",
                params![
                    &session.user_id,
                    session.status.as_str(),
                    session.target_mastery_count as i64,
                    session.total_questions as i64,
                    session.actual_mastery_count as i64,
                    session.context_shifts as i64,
                    session.created_at.to_rfc3339(),
                    session.updated_at.to_rfc3339(),
                    session.summary.as_ref().map(|s| s.accuracy),
                    session.summary.as_ref().map(|s| s.avg_response_time_ms),
                    mastered_json,
                    error_prone_json,
                    session.summary.as_ref().map(|s| s.duration_secs),
                    session.summary.as_ref().map(|s| s.hour_of_day as i64),
                    session.summary.as_ref().map(|s| s.final_difficulty),
                    session.correct_count as i64,
                    session.total_count as i64,
                    &session.id,
                    existing,
                ],
            ) {
                Ok(1) => return Ok(()),
                Ok(0) => continue,
                Err(e) => return Err(e.into()),
                _ => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "learning_session".into(),
            key: session.id.clone(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    pub fn get_active_sessions_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<LearningSession>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLS} FROM learning_sessions \
             WHERE user_id = ?1 AND status = 'active' \
             ORDER BY updated_at DESC"
        ))?;
        let sessions = stmt
            .query_map(params![user_id], session_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sessions)
    }

    pub fn get_session_shown_word_ids(
        &self,
        session_id: &str,
    ) -> Result<Vec<String>, StoreError> {
        keys::validate_id(session_id)?;
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare("SELECT word_id FROM session_shown_words WHERE session_id = ?1")?;
        let rows = stmt
            .query_map(params![session_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_session_shown_word_ids_for_batch(
        &self,
        session_id: &str,
        batch_index: u32,
    ) -> Result<Vec<String>, StoreError> {
        keys::validate_id(session_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT word_id FROM session_shown_words \
             WHERE session_id = ?1 AND batch_index = ?2 \
             ORDER BY shown_at ASC, word_id ASC",
        )?;
        let rows = stmt
            .query_map(params![session_id, batch_index as i64], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 原子地认领展示批次。
    ///
    /// - `requested_batch_index = Some(idx)`：幂等重放路径。命中已持久化批次直接返回；
    ///   否则在同一 `BEGIN IMMEDIATE` 事务内插入并返回。
    /// - `requested_batch_index = None`：legacy 客户端，服务端分配下一个未占用的 batch_index，
    ///   绕过重放语义，避免 batch 0 被无限复读。
    ///
    /// `INSERT OR IGNORE` 容忍跨批次 `(session_id, word_id)` 冲突（并发不同 batch 选到相同词），
    /// 然后按 rowid 重读，返回插入顺序（AMAS 教学序保留）。
    pub fn claim_session_batch(
        &self,
        session_id: &str,
        requested_batch_index: Option<u32>,
        candidate_ids: &[String],
    ) -> Result<(u32, Vec<String>), StoreError> {
        keys::validate_id(session_id)?;
        for word_id in candidate_ids {
            keys::validate_id(word_id)?;
        }

        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let batch_index: u32 = match requested_batch_index {
            Some(idx) => {
                let existing: Vec<String> = {
                    let mut stmt = tx.prepare(
                        "SELECT word_id FROM session_shown_words \
                         WHERE session_id = ?1 AND batch_index = ?2 \
                         ORDER BY rowid ASC",
                    )?;
                    let rows = stmt
                        .query_map(params![session_id, idx as i64], |row| {
                            row.get::<_, String>(0)
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    rows
                };
                if !existing.is_empty() {
                    return Ok((idx, existing));
                }
                idx
            }
            None => {
                let next: i64 = tx.query_row(
                    "SELECT COALESCE(MAX(batch_index) + 1, 0) FROM session_shown_words \
                     WHERE session_id = ?1",
                    params![session_id],
                    |r| r.get(0),
                )?;
                next as u32
            }
        };

        if candidate_ids.is_empty() {
            return Ok((batch_index, Vec::new()));
        }

        let shown_at = Utc::now().timestamp();
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO session_shown_words \
                 (session_id, word_id, shown_at, batch_index) \
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for word_id in candidate_ids {
                stmt.execute(params![session_id, word_id, shown_at, batch_index as i64])?;
            }
        }

        // 重读：按 rowid 反映插入顺序，确保返回值与 AMAS 选择顺序一致。
        // 跨批 PK 冲突被 INSERT OR IGNORE 吞掉的词在此次返回里会被剔除。
        let final_ids: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT word_id FROM session_shown_words \
                 WHERE session_id = ?1 AND batch_index = ?2 \
                 ORDER BY rowid ASC",
            )?;
            let rows = stmt
                .query_map(params![session_id, batch_index as i64], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        tx.commit()?;
        Ok((batch_index, final_ids))
    }

    /// 按 `shown_at` 时间窗清理 session_shown_words；不依赖 learning_sessions.status，
    /// 因为系统中没有自动 abandon 长时间无心跳 active session 的链路。
    /// 即使会话仍 active，超过 retention 的展示历史也属于陈旧数据可丢弃。
    pub fn prune_session_shown_words_older_than(
        &self,
        older_than_secs: i64,
    ) -> Result<u32, StoreError> {
        let conn = self.conn()?;
        let cutoff = Utc::now().timestamp() - older_than_secs;
        let removed = conn.execute(
            "DELETE FROM session_shown_words WHERE shown_at < ?1",
            params![cutoff],
        )?;
        Ok(removed as u32)
    }

    #[cfg(test)]
    fn count_session_shown_words(&self, session_id: &str) -> rusqlite::Result<i64> {
        let conn = self.conn().expect("conn");
        conn.query_row(
            "SELECT COUNT(*) FROM session_shown_words WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
    }

    pub fn close_active_sessions_for_user(&self, user_id: &str) -> Result<u32, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let now = Utc::now().to_rfc3339();
        let count = conn.execute(
            "UPDATE learning_sessions SET status = 'abandoned', updated_at = ?1 \
             WHERE user_id = ?2 AND status = 'active'",
            params![now, user_id],
        )?;
        Ok(count as u32)
    }

    pub fn get_recent_completed_sessions(
        &self,
        user_id: &str,
        max_age_secs: i64,
    ) -> Result<Vec<LearningSession>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let cutoff = (Utc::now() - chrono::Duration::seconds(max_age_secs)).to_rfc3339();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLS} FROM learning_sessions \
             WHERE user_id = ?1 AND status = 'completed' AND updated_at >= ?2 \
             ORDER BY updated_at DESC"
        ))?;
        let sessions = stmt
            .query_map(params![user_id, cutoff], session_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sessions)
    }

    pub fn list_learning_sessions_for_user(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
        start_at: Option<DateTime<Utc>>,
        end_before: Option<DateTime<Utc>>,
    ) -> Result<Vec<LearningSession>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut sql = format!("SELECT {COLS} FROM learning_sessions WHERE user_id = ?1");
        let mut params_boxed: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(user_id.to_string())];
        if let Some(start_at) = start_at {
            params_boxed.push(Box::new(start_at.to_rfc3339()));
            sql.push_str(&format!(" AND created_at >= ?{}", params_boxed.len()));
        }
        if let Some(end_before) = end_before {
            params_boxed.push(Box::new(end_before.to_rfc3339()));
            sql.push_str(&format!(" AND created_at < ?{}", params_boxed.len()));
        }
        params_boxed.push(Box::new(limit as i64));
        let limit_idx = params_boxed.len();
        params_boxed.push(Box::new(offset as i64));
        let offset_idx = params_boxed.len();
        sql.push_str(&format!(
            " ORDER BY created_at DESC LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
        ));
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_boxed.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let sessions = stmt
            .query_map(param_refs.as_slice(), session_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sessions)
    }

    pub fn count_learning_sessions_for_user(
        &self,
        user_id: &str,
        start_at: Option<DateTime<Utc>>,
        end_before: Option<DateTime<Utc>>,
    ) -> Result<u64, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut sql = "SELECT COUNT(*) FROM learning_sessions WHERE user_id = ?1".to_string();
        let mut params_boxed: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(user_id.to_string())];
        if let Some(start_at) = start_at {
            params_boxed.push(Box::new(start_at.to_rfc3339()));
            sql.push_str(&format!(" AND created_at >= ?{}", params_boxed.len()));
        }
        if let Some(end_before) = end_before {
            params_boxed.push(Box::new(end_before.to_rfc3339()));
            sql.push_str(&format!(" AND created_at < ?{}", params_boxed.len()));
        }
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_boxed.iter().map(|p| p.as_ref()).collect();
        let count: i64 = conn.query_row(&sql, param_refs.as_slice(), |r| r.get(0))?;
        Ok(count as u64)
    }

    pub fn total_session_duration_secs(&self, user_id: &str) -> Result<u64, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let total: Option<i64> = conn.query_row(
            "SELECT COALESCE(SUM(
                CASE
                    WHEN summary_duration_secs IS NOT NULL THEN summary_duration_secs
                    WHEN status = 'completed' THEN
                        CAST((julianday(updated_at) - julianday(created_at)) * 86400 AS INTEGER)
                    ELSE 0
                END
             ), 0) FROM learning_sessions WHERE user_id = ?1",
            params![user_id],
            |r| r.get(0),
        )?;
        Ok(total.unwrap_or(0).max(0) as u64)
    }

    pub fn daily_session_durations(
        &self,
        user_id: &str,
    ) -> Result<std::collections::HashMap<String, u64>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT DATE(created_at) AS d, COALESCE(SUM(
                CASE
                    WHEN summary_duration_secs IS NOT NULL THEN summary_duration_secs
                    WHEN status = 'completed' THEN
                        CAST((julianday(updated_at) - julianday(created_at)) * 86400 AS INTEGER)
                    ELSE 0
                END
             ), 0) FROM learning_sessions WHERE user_id = ?1 GROUP BY d",
        )?;
        let rows = stmt.query_map(params![user_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (day, secs) = row?;
            out.insert(day, secs.max(0) as u64);
        }
        Ok(out)
    }

    pub fn list_learning_sessions_between(
        &self,
        user_id: &str,
        start_at: DateTime<Utc>,
        end_before: DateTime<Utc>,
    ) -> Result<Vec<LearningSession>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLS} FROM learning_sessions
             WHERE user_id = ?1 AND created_at >= ?2 AND created_at < ?3
             ORDER BY created_at ASC"
        ))?;
        let sessions = stmt
            .query_map(
                params![user_id, start_at.to_rfc3339(), end_before.to_rfc3339()],
                session_from_row,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    fn sample_session(id: &str, user_id: &str) -> LearningSession {
        let now = Utc::now();
        LearningSession {
            id: id.into(),
            user_id: user_id.into(),
            status: SessionStatus::Active,
            target_mastery_count: 10,
            total_questions: 0,
            actual_mastery_count: 0,
            context_shifts: 0,
            created_at: now,
            updated_at: now,
            summary: None,
            correct_count: 0,
            total_count: 0,
        }
    }

    #[test]
    fn create_and_get() {
        let store = test_store();
        let s = sample_session("s1", "u1");
        store.create_learning_session(&s).unwrap();
        let got = store.get_learning_session("s1").unwrap().unwrap();
        assert_eq!(got.user_id, "u1");
        assert_eq!(got.status, SessionStatus::Active);
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let store = test_store();
        assert!(store.get_learning_session("nope").unwrap().is_none());
    }

    #[test]
    fn update_with_summary() {
        let store = test_store();
        let mut s = sample_session("s1", "u1");
        store.create_learning_session(&s).unwrap();

        s.status = SessionStatus::Completed;
        s.updated_at = Utc::now();
        s.summary = Some(SessionSummary {
            accuracy: 0.85,
            avg_response_time_ms: 1200,
            mastered_word_ids: vec!["w1".into(), "w2".into()],
            error_prone_word_ids: vec!["w3".into()],
            duration_secs: 600,
            hour_of_day: 14,
            final_difficulty: 0.6,
        });
        store.update_learning_session(&s).unwrap();

        let got = store.get_learning_session("s1").unwrap().unwrap();
        assert_eq!(got.status, SessionStatus::Completed);
        let summary = got.summary.unwrap();
        assert!((summary.accuracy - 0.85).abs() < f64::EPSILON);
        assert_eq!(summary.mastered_word_ids, vec!["w1", "w2"]);
    }

    #[test]
    fn active_sessions_for_user() {
        let store = test_store();
        store
            .create_learning_session(&sample_session("s1", "u1"))
            .unwrap();
        store
            .create_learning_session(&sample_session("s2", "u1"))
            .unwrap();
        store
            .create_learning_session(&sample_session("s3", "u2"))
            .unwrap();

        let active = store.get_active_sessions_for_user("u1").unwrap();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn claim_session_batch_is_idempotent_under_explicit_replay() {
        let store = test_store();
        store
            .create_learning_session(&sample_session("s1", "u1"))
            .unwrap();

        let (idx0, first) = store
            .claim_session_batch("s1", Some(0), &["w1".into(), "w2".into(), "w3".into()])
            .unwrap();
        assert_eq!(idx0, 0);
        assert_eq!(first, vec!["w1", "w2", "w3"]);

        let (idx0_replay, replay) = store
            .claim_session_batch("s1", Some(0), &["w7".into(), "w8".into()])
            .unwrap();
        assert_eq!(idx0_replay, 0);
        assert_eq!(replay, vec!["w1", "w2", "w3"]);
        assert_eq!(store.count_session_shown_words("s1").unwrap(), 3);
    }

    #[test]
    fn claim_session_batch_legacy_allocates_fresh_index() {
        let store = test_store();
        store
            .create_learning_session(&sample_session("s1", "u1"))
            .unwrap();

        let (idx_a, a) = store
            .claim_session_batch("s1", None, &["w1".into(), "w2".into()])
            .unwrap();
        let (idx_b, b) = store
            .claim_session_batch("s1", None, &["w3".into(), "w4".into()])
            .unwrap();
        assert_eq!(idx_a, 0);
        assert_eq!(idx_b, 1);
        assert_eq!(a, vec!["w1", "w2"]);
        assert_eq!(b, vec!["w3", "w4"]);
        assert_eq!(store.count_session_shown_words("s1").unwrap(), 4);
    }

    #[test]
    fn claim_session_batch_drops_cross_batch_collisions() {
        let store = test_store();
        store
            .create_learning_session(&sample_session("s1", "u1"))
            .unwrap();

        store
            .claim_session_batch("s1", Some(0), &["w1".into(), "w2".into()])
            .unwrap();
        // Batch 1 selects an overlapping word; INSERT OR IGNORE drops it.
        let (_, b1) = store
            .claim_session_batch("s1", Some(1), &["w2".into(), "w3".into()])
            .unwrap();
        assert_eq!(b1, vec!["w3"]);
        assert_eq!(store.count_session_shown_words("s1").unwrap(), 3);
    }

    #[test]
    fn claim_session_batch_preserves_insertion_order_on_replay() {
        let store = test_store();
        store
            .create_learning_session(&sample_session("s1", "u1"))
            .unwrap();

        // Out-of-alphabetical order; replay must match the original sequence.
        let (_, original) = store
            .claim_session_batch(
                "s1",
                Some(0),
                &["zeta".into(), "alpha".into(), "mu".into()],
            )
            .unwrap();
        assert_eq!(original, vec!["zeta", "alpha", "mu"]);

        let (_, replay) = store
            .claim_session_batch("s1", Some(0), &["different".into()])
            .unwrap();
        assert_eq!(replay, vec!["zeta", "alpha", "mu"]);
    }

    #[test]
    fn prune_session_shown_words_older_than_drops_aged_rows() {
        let store = test_store();
        store
            .create_learning_session(&sample_session("s1", "u1"))
            .unwrap();
        store
            .claim_session_batch("s1", Some(0), &["w1".into(), "w2".into()])
            .unwrap();

        // Backdate rows beyond the retention window.
        {
            let conn = store.conn().unwrap();
            conn.execute(
                "UPDATE session_shown_words SET shown_at = ?1 WHERE session_id = ?2",
                params![Utc::now().timestamp() - 8 * 24 * 60 * 60, "s1"],
            )
            .unwrap();
        }

        let removed = store
            .prune_session_shown_words_older_than(7 * 24 * 60 * 60)
            .unwrap();
        assert_eq!(removed, 2);
        assert_eq!(store.count_session_shown_words("s1").unwrap(), 0);
    }

    #[test]
    fn close_active_sessions() {
        let store = test_store();
        store
            .create_learning_session(&sample_session("s1", "u1"))
            .unwrap();
        store
            .create_learning_session(&sample_session("s2", "u1"))
            .unwrap();

        let closed = store.close_active_sessions_for_user("u1").unwrap();
        assert_eq!(closed, 2);

        let active = store.get_active_sessions_for_user("u1").unwrap();
        assert!(active.is_empty());
    }

    #[test]
    fn recent_completed_sessions() {
        let store = test_store();
        let mut s = sample_session("s1", "u1");
        store.create_learning_session(&s).unwrap();

        s.status = SessionStatus::Completed;
        s.updated_at = Utc::now();
        store.update_learning_session(&s).unwrap();

        let recent = store.get_recent_completed_sessions("u1", 3600).unwrap();
        assert_eq!(recent.len(), 1);

        let old = store.get_recent_completed_sessions("u1", 0).unwrap();
        assert!(old.is_empty());
    }

    #[test]
    fn status_as_str_roundtrip() {
        assert_eq!(SessionStatus::Active.as_str(), "active");
        assert_eq!(SessionStatus::Completed.as_str(), "completed");
        assert_eq!(SessionStatus::Abandoned.as_str(), "abandoned");
    }

    fn tempfile_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    #[test]
    fn update_learning_session_missing_returns_not_found() {
        let store = test_store();
        let s = sample_session("ghost", "u1");
        assert!(matches!(
            store.update_learning_session(&s).unwrap_err(),
            StoreError::NotFound { .. }
        ));
    }

    #[test]
    fn parse_status_rejects_unknown_value() {
        // 内部 helper：覆盖错误分支
        assert!(super::parse_status("active").is_ok());
        assert!(super::parse_status("bogus").is_err());
    }

    #[test]
    fn get_session_shown_word_ids_returns_all_and_per_batch() {
        let store = test_store();
        store
            .create_learning_session(&sample_session("s1", "u1"))
            .unwrap();
        store
            .claim_session_batch("s1", Some(0), &["a".into(), "b".into()])
            .unwrap();
        store
            .claim_session_batch("s1", Some(1), &["c".into()])
            .unwrap();
        let all = store.get_session_shown_word_ids("s1").unwrap();
        assert_eq!(all.len(), 3);
        let batch1 = store.get_session_shown_word_ids_for_batch("s1", 1).unwrap();
        assert_eq!(batch1, vec!["c"]);
    }

    #[test]
    fn claim_session_batch_empty_candidates_returns_assigned_index() {
        let store = test_store();
        store.create_learning_session(&sample_session("s1", "u1")).unwrap();
        let (idx, ids) = store.claim_session_batch("s1", None, &[]).unwrap();
        assert_eq!(idx, 0);
        assert!(ids.is_empty());
    }

    #[test]
    fn list_count_learning_sessions_pagination_and_time_window() {
        let (_t, store) = tempfile_store();
        let base = Utc::now();
        for i in 0..4 {
            let mut s = sample_session(&format!("s{i}"), "u1");
            s.created_at = base - chrono::Duration::days(i as i64);
            s.updated_at = s.created_at;
            store.create_learning_session(&s).unwrap();
        }
        let total = store.count_learning_sessions_for_user("u1", None, None).unwrap();
        assert_eq!(total, 4);
        let page = store
            .list_learning_sessions_for_user("u1", 2, 0, None, None)
            .unwrap();
        assert_eq!(page.len(), 2);
        // 最新（s0）在前
        assert_eq!(page[0].id, "s0");

        let window_start = base - chrono::Duration::days(2);
        let cnt = store
            .count_learning_sessions_for_user("u1", Some(window_start), None)
            .unwrap();
        assert_eq!(cnt, 3); // s0, s1, s2

        let bounded = store
            .list_learning_sessions_for_user(
                "u1",
                10,
                0,
                Some(window_start),
                Some(base + chrono::Duration::seconds(1)),
            )
            .unwrap();
        assert_eq!(bounded.len(), 3);

        let between = store
            .list_learning_sessions_between(
                "u1",
                base - chrono::Duration::days(1),
                base + chrono::Duration::seconds(1),
            )
            .unwrap();
        // s0, s1 (asc)
        assert_eq!(between.len(), 2);
        assert_eq!(between[0].id, "s1");
        assert_eq!(between[1].id, "s0");
    }

    #[test]
    fn total_session_duration_and_daily_aggregate() {
        let (_t, store) = tempfile_store();
        let now = Utc::now();
        let mut s1 = sample_session("s1", "u1");
        s1.status = SessionStatus::Completed;
        s1.created_at = now;
        s1.updated_at = now + chrono::Duration::seconds(120);
        s1.summary = Some(SessionSummary {
            accuracy: 1.0,
            avg_response_time_ms: 100,
            mastered_word_ids: vec![],
            error_prone_word_ids: vec![],
            duration_secs: 60,
            hour_of_day: 0,
            final_difficulty: 0.5,
        });
        store.create_learning_session(&s1).unwrap();
        // 第二条 completed，无 summary，按 julianday 计算 ≈120s
        let mut s2 = sample_session("s2", "u1");
        s2.status = SessionStatus::Completed;
        s2.created_at = now;
        s2.updated_at = now + chrono::Duration::seconds(120);
        s2.summary = None;
        store.create_learning_session(&s2).unwrap();

        let total = store.total_session_duration_secs("u1").unwrap();
        assert!(total >= 60); // 至少包含 s1 的 60s
        let daily = store.daily_session_durations("u1").unwrap();
        assert!(daily.values().sum::<u64>() >= 60);
    }
}
