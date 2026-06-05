//! GDPR Article 20 数据导出：按表分块读取用户全量可携带数据。
//!
//! 每次调用单个 `export_*` 方法读取一张表，避免单次长事务阻塞写入。

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::store::{Store, StoreError};

/// 导出 profile（用户基础信息）块。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProfile {
    pub id: String,
    pub email: String,
    pub username: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 导出学习记录行（仅可携带字段，去掉 password_hash 等内部字段）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRecord {
    pub id: String,
    pub word_id: String,
    pub is_correct: bool,
    pub response_time_ms: i64,
    pub session_id: Option<String>,
    pub record_type: String,
    pub self_rating: Option<i64>,
    pub created_at: String,
}

/// 导出单词学习状态行。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportWordState {
    pub word_id: String,
    pub state: String,
    pub mastery_level: f64,
    pub next_review_date: Option<String>,
    pub half_life: f64,
    pub correct_streak: i64,
    pub total_attempts: i64,
    pub updated_at: String,
}

/// 导出收藏行。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFavorite {
    pub word_id: String,
    pub created_at: String,
}

/// 导出单词备注行。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportNote {
    pub id: String,
    pub word_id: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 导出学习偏好配置。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportStudyConfig {
    pub selected_wordbook_ids: Vec<String>,
    pub daily_word_count: i64,
    pub study_mode: String,
    pub daily_mastery_target: i64,
}

/// 导出学习会话行。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSession {
    pub id: String,
    pub status: String,
    pub target_mastery_count: i64,
    pub total_questions: i64,
    pub actual_mastery_count: i64,
    pub correct_count: i64,
    pub total_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl Store {
    pub fn export_profile(&self, user_id: &str) -> Result<Option<ExportProfile>, StoreError> {
        crate::store::keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT id, email, username, created_at, updated_at FROM users WHERE id=?1",
                params![user_id],
                |r| {
                    Ok(ExportProfile {
                        id: r.get(0)?,
                        email: r.get(1)?,
                        username: r.get(2)?,
                        created_at: r.get(3)?,
                        updated_at: r.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// 分页读取学习记录（每页 500 条，避免单次长事务）。
    pub fn export_records(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ExportRecord>, StoreError> {
        crate::store::keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, word_id, is_correct, response_time_ms, session_id,
                    record_type, self_rating, created_at
             FROM learning_records
             WHERE user_id=?1
             ORDER BY created_at ASC
             LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![user_id, limit as i64, offset as i64], |r| {
            Ok(ExportRecord {
                id: r.get(0)?,
                word_id: r.get(1)?,
                is_correct: r.get::<_, i64>(2)? != 0,
                response_time_ms: r.get(3)?,
                session_id: r.get(4)?,
                record_type: r.get(5)?,
                self_rating: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn export_word_states(&self, user_id: &str) -> Result<Vec<ExportWordState>, StoreError> {
        crate::store::keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT word_id, state, mastery_level, next_review_date, half_life,
                    correct_streak, total_attempts, updated_at
             FROM word_learning_states
             WHERE user_id=?1
             ORDER BY updated_at ASC",
        )?;
        let rows = stmt.query_map(params![user_id], |r| {
            Ok(ExportWordState {
                word_id: r.get(0)?,
                state: r.get(1)?,
                mastery_level: r.get(2)?,
                next_review_date: r.get(3)?,
                half_life: r.get(4)?,
                correct_streak: r.get(5)?,
                total_attempts: r.get(6)?,
                updated_at: r.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn export_favorites(&self, user_id: &str) -> Result<Vec<ExportFavorite>, StoreError> {
        crate::store::keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT word_id, created_at FROM word_favorites WHERE user_id=?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![user_id], |r| {
            Ok(ExportFavorite {
                word_id: r.get(0)?,
                created_at: r.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn export_notes(&self, user_id: &str) -> Result<Vec<ExportNote>, StoreError> {
        crate::store::keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, word_id, content, created_at, updated_at
             FROM word_notes
             WHERE user_id=?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![user_id], |r| {
            Ok(ExportNote {
                id: r.get(0)?,
                word_id: r.get(1)?,
                content: r.get(2)?,
                created_at: r.get(3)?,
                updated_at: r.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn export_study_config(
        &self,
        user_id: &str,
    ) -> Result<Option<ExportStudyConfig>, StoreError> {
        crate::store::keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT selected_wordbook_ids_json, daily_word_count, study_mode, daily_mastery_target
                 FROM study_configs WHERE user_id=?1",
                params![user_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;
        match row {
            Some((ids_json, daily, mode, target)) => {
                let selected_wordbook_ids: Vec<String> =
                    serde_json::from_str(&ids_json).unwrap_or_default();
                Ok(Some(ExportStudyConfig {
                    selected_wordbook_ids,
                    daily_word_count: daily,
                    study_mode: mode,
                    daily_mastery_target: target,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn export_sessions(&self, user_id: &str) -> Result<Vec<ExportSession>, StoreError> {
        crate::store::keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, status, target_mastery_count, total_questions,
                    actual_mastery_count, correct_count, total_count,
                    created_at, updated_at
             FROM learning_sessions
             WHERE user_id=?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![user_id], |r| {
            Ok(ExportSession {
                id: r.get(0)?,
                status: r.get(1)?,
                target_mastery_count: r.get(2)?,
                total_questions: r.get(3)?,
                actual_mastery_count: r.get(4)?,
                correct_count: r.get(5)?,
                total_count: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}
