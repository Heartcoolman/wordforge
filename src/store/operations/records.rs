use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningRecord {
    pub id: String,
    pub user_id: String,
    pub word_id: String,
    pub is_correct: bool,
    pub response_time_ms: i64,
    pub session_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserStatsAgg {
    pub total_records: u64,
    pub correct_records: u64,
    pub word_ids: HashSet<String>,
    pub session_ids: HashSet<String>,
}

const RECORD_COLS: &str = "user_id, id, word_id, is_correct, response_time_ms, session_id, created_at";

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

fn record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LearningRecord> {
    Ok(LearningRecord {
        user_id: row.get(0)?,
        id: row.get(1)?,
        word_id: row.get(2)?,
        is_correct: row.get::<_, i64>(3)? != 0,
        response_time_ms: row.get(4)?,
        session_id: row.get(5)?,
        created_at: parse_dt(row.get(6)?)?,
    })
}

impl Store {
    pub fn get_user_stats_agg(&self, user_id: &str) -> Result<UserStatsAgg, StoreError> {
        let conn = self.conn()?;
        let result: Option<(i64, i64, String, String)> = conn
            .query_row(
                "SELECT total_records, correct_records, word_ids_json, session_ids_json FROM user_stats WHERE user_id=?1",
                params![user_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        match result {
            Some((total, correct, words_json, sessions_json)) => Ok(UserStatsAgg {
                total_records: total as u64,
                correct_records: correct as u64,
                word_ids: Self::deserialize_json(&words_json)?,
                session_ids: Self::deserialize_json(&sessions_json)?,
            }),
            None => Ok(UserStatsAgg::default()),
        }
    }

    fn set_user_stats_agg(&self, user_id: &str, stats: &UserStatsAgg) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO user_stats (user_id, total_records, correct_records, word_ids_json, session_ids_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(user_id) DO UPDATE SET
                total_records=?2, correct_records=?3, word_ids_json=?4, session_ids_json=?5",
            params![
                user_id, stats.total_records as i64, stats.correct_records as i64,
                Self::serialize_json(&stats.word_ids)?, Self::serialize_json(&stats.session_ids)?,
            ],
        )?;
        Ok(())
    }

    pub fn count_active_users_since(&self, since: DateTime<Utc>) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT user_id) FROM learning_records WHERE created_at >= ?1",
            params![since.to_rfc3339()],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn count_records_since(&self, since: DateTime<Utc>) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM learning_records WHERE created_at >= ?1",
            params![since.to_rfc3339()],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn create_record(&self, record: &LearningRecord) -> Result<(), StoreError> {
        keys::validate_id(&record.id)?;
        keys::validate_id(&record.user_id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, session_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                &record.user_id, &record.id, &record.word_id,
                record.is_correct as i64, record.response_time_ms,
                record.session_id.as_deref(), record.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn create_record_with_updates(
        &self,
        record: &LearningRecord,
        word_state: Option<&crate::store::operations::word_states::WordLearningState>,
        learning_session: Option<&crate::store::operations::learning_sessions::LearningSession>,
    ) -> Result<(), StoreError> {
        keys::validate_id(&record.id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, session_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                &record.user_id, &record.id, &record.word_id,
                record.is_correct as i64, record.response_time_ms,
                record.session_id.as_deref(), record.created_at.to_rfc3339(),
            ],
        )?;

        if let Some(state) = word_state {
            let next_review = state.next_review_date.map(|d| d.to_rfc3339());
            tx.execute(
                "INSERT INTO word_learning_states (user_id, word_id, state, mastery_level, next_review_date, half_life, correct_streak, total_attempts, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(user_id, word_id) DO UPDATE SET
                    state=?3, mastery_level=?4, next_review_date=?5, half_life=?6, correct_streak=?7, total_attempts=?8, updated_at=?9",
                params![
                    &state.user_id, &state.word_id, state.state.as_str(),
                    state.mastery_level, next_review.as_deref(),
                    state.half_life, state.correct_streak as i64,
                    state.total_attempts as i64, state.updated_at.to_rfc3339(),
                ],
            )?;
        }

        if let Some(session) = learning_session {
            let summary = session.summary.as_ref();
            let summary_mastered = Self::serialize_json(
                &summary.map(|s| &s.mastered_word_ids).unwrap_or(&vec![]),
            )?;
            let summary_error = Self::serialize_json(
                &summary.map(|s| &s.error_prone_word_ids).unwrap_or(&vec![]),
            )?;
            tx.execute(
                "UPDATE learning_sessions SET
                    status=?1, total_questions=?2, actual_mastery_count=?3, context_shifts=?4,
                    updated_at=?5, summary_accuracy=?6, summary_avg_response_time_ms=?7,
                    summary_mastered_word_ids_json=?8, summary_error_prone_word_ids_json=?9,
                    summary_duration_secs=?10, summary_hour_of_day=?11, summary_final_difficulty=?12,
                    correct_count=?13, total_count=?14
                 WHERE id=?15",
                params![
                    session.status.as_str(), session.total_questions as i64,
                    session.actual_mastery_count as i64, session.context_shifts as i64,
                    session.updated_at.to_rfc3339(),
                    summary.map(|s| s.accuracy),
                    summary.map(|s| s.avg_response_time_ms),
                    summary_mastered, summary_error,
                    summary.map(|s| s.duration_secs),
                    summary.map(|s| s.hour_of_day as i64),
                    summary.map(|s| s.final_difficulty),
                    session.correct_count as i64, session.total_count as i64,
                    &session.id,
                ],
            )?;
        }

        // Update user_stats
        let mut stats = {
            let result: Option<(i64, i64, String, String)> = tx
                .query_row(
                    "SELECT total_records, correct_records, word_ids_json, session_ids_json FROM user_stats WHERE user_id=?1",
                    params![&record.user_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?;
            match result {
                Some((total, correct, words_json, sessions_json)) => UserStatsAgg {
                    total_records: total as u64,
                    correct_records: correct as u64,
                    word_ids: serde_json::from_str(&words_json).unwrap_or_default(),
                    session_ids: serde_json::from_str(&sessions_json).unwrap_or_default(),
                },
                None => UserStatsAgg::default(),
            }
        };
        stats.total_records += 1;
        if record.is_correct {
            stats.correct_records += 1;
        }
        stats.word_ids.insert(record.word_id.clone());
        if let Some(ref sid) = record.session_id {
            stats.session_ids.insert(sid.clone());
        }
        tx.execute(
            "INSERT INTO user_stats (user_id, total_records, correct_records, word_ids_json, session_ids_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(user_id) DO UPDATE SET
                total_records=?2, correct_records=?3, word_ids_json=?4, session_ids_json=?5",
            params![
                &record.user_id, stats.total_records as i64, stats.correct_records as i64,
                serde_json::to_string(&stats.word_ids).unwrap_or_default(),
                serde_json::to_string(&stats.session_ids).unwrap_or_default(),
            ],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn get_user_record_by_id(
        &self,
        user_id: &str,
        record_id: &str,
    ) -> Result<Option<LearningRecord>, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(record_id)?;
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {RECORD_COLS} FROM learning_records WHERE user_id=?1 AND id=?2"),
                params![user_id, record_id],
                record_from_row,
            )
            .optional()?)
    }

    pub fn get_user_records(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<LearningRecord>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {RECORD_COLS} FROM learning_records WHERE user_id=?1 ORDER BY created_at DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![user_id, limit as i64], record_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    pub fn get_user_records_with_offset(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<LearningRecord>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {RECORD_COLS} FROM learning_records WHERE user_id=?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = stmt.query_map(params![user_id, limit as i64, offset as i64], record_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    pub fn count_user_records_stats(&self, user_id: &str) -> Result<(usize, usize), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let (total, correct): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), SUM(CASE WHEN is_correct=1 THEN 1 ELSE 0 END) FROM learning_records WHERE user_id=?1",
            params![user_id],
            |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
        )?;
        Ok((total as usize, correct as usize))
    }

    pub fn count_user_records(&self, user_id: &str) -> Result<usize, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM learning_records WHERE user_id=?1",
            params![user_id],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn count_all_records(&self) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM learning_records", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    pub fn count_all_correct_records(&self) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM learning_records WHERE is_correct=1",
            [],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn get_user_word_records(
        &self,
        user_id: &str,
        word_id: &str,
        limit: usize,
    ) -> Result<Vec<LearningRecord>, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {RECORD_COLS} FROM learning_records WHERE user_id=?1 AND word_id=?2 ORDER BY created_at DESC LIMIT ?3"
        ))?;
        let rows = stmt.query_map(params![user_id, word_id, limit as i64], record_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    fn sample_record(id: &str, user_id: &str, word_id: &str, created_at: DateTime<Utc>) -> LearningRecord {
        LearningRecord {
            id: id.into(),
            user_id: user_id.into(),
            word_id: word_id.into(),
            is_correct: true,
            response_time_ms: 1000,
            session_id: Some("s1".into()),
            created_at,
        }
    }

    #[test]
    fn records_returned_desc_order() {
        let store = test_store();
        let now = Utc::now();
        store.create_record(&sample_record("r1", "u1", "w1", now - Duration::seconds(30))).unwrap();
        store.create_record(&sample_record("r2", "u1", "w1", now)).unwrap();
        let list = store.get_user_records("u1", 10).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "r2");
        assert_eq!(list[1].id, "r1");
    }

    #[test]
    fn count_records_works() {
        let store = test_store();
        let now = Utc::now();
        store.create_record(&sample_record("r1", "u1", "w1", now)).unwrap();
        store.create_record(&sample_record("r2", "u1", "w2", now)).unwrap();
        assert_eq!(store.count_user_records("u1").unwrap(), 2);
        assert_eq!(store.count_all_records().unwrap(), 2);
    }
}
