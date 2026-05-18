use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordType {
    Learning,
    Review,
    #[default]
    All,
}

impl RecordType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Learning => "learning",
            Self::Review => "review",
            Self::All => "all",
        }
    }

    pub fn parse(s: &str) -> Result<Self, StoreError> {
        match s {
            "learning" => Ok(Self::Learning),
            "review" => Ok(Self::Review),
            "all" => Ok(Self::All),
            _ => Err(StoreError::Validation(format!("invalid record_type: {s}"))),
        }
    }
}

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
    #[serde(default)]
    pub record_type: RecordType,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserStatsAgg {
    pub total_records: u64,
    pub correct_records: u64,
    pub word_ids: HashSet<String>,
    pub session_ids: HashSet<String>,
}

const RECORD_COLS: &str =
    "user_id, id, word_id, is_correct, response_time_ms, session_id, created_at, record_type";

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LearningRecord> {
    let record_type_str: String = row.get(7)?;
    let record_type = RecordType::parse(&record_type_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(LearningRecord {
        user_id: row.get(0)?,
        id: row.get(1)?,
        word_id: row.get(2)?,
        is_correct: row.get::<_, i64>(3)? != 0,
        response_time_ms: row.get(4)?,
        session_id: row.get(5)?,
        created_at: parse_dt(row.get(6)?)?,
        record_type,
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
            "INSERT INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, session_id, created_at, record_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &record.user_id, &record.id, &record.word_id,
                record.is_correct as i64, record.response_time_ms,
                record.session_id.as_deref(), record.created_at.to_rfc3339(),
                record.record_type.as_str(),
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
            "INSERT INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, session_id, created_at, record_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &record.user_id, &record.id, &record.word_id,
                record.is_correct as i64, record.response_time_ms,
                record.session_id.as_deref(), record.created_at.to_rfc3339(),
                record.record_type.as_str(),
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
            let summary_mastered =
                Self::serialize_json(&summary.map(|s| &s.mastered_word_ids).unwrap_or(&vec![]))?;
            let summary_error =
                Self::serialize_json(&summary.map(|s| &s.error_prone_word_ids).unwrap_or(&vec![]))?;
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
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn list_records_by_session(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<LearningRecord>, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(session_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {RECORD_COLS} FROM learning_records
             WHERE user_id=?1 AND session_id=?2
             ORDER BY created_at ASC"
        ))?;
        let rows = stmt.query_map(params![user_id, session_id], record_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
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
        let rows = stmt.query_map(
            params![user_id, limit as i64, offset as i64],
            record_from_row,
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn get_user_records_between(
        &self,
        user_id: &str,
        start_at: DateTime<Utc>,
        end_before: DateTime<Utc>,
    ) -> Result<Vec<LearningRecord>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {RECORD_COLS} FROM learning_records
             WHERE user_id=?1 AND created_at >= ?2 AND created_at < ?3
             ORDER BY created_at ASC"
        ))?;
        let rows = stmt.query_map(
            params![user_id, start_at.to_rfc3339(), end_before.to_rfc3339()],
            record_from_row,
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn first_record_times_for_words(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<HashMap<String, DateTime<Utc>>, StoreError> {
        keys::validate_id(user_id)?;
        if word_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.conn()?;
        let mut out = HashMap::new();
        for chunk in word_ids.chunks(400) {
            let placeholders: Vec<&str> = chunk.iter().map(|_| "?").collect();
            let sql = format!(
                "SELECT word_id, MIN(created_at)
                 FROM learning_records
                 WHERE user_id = ? AND word_id IN ({})
                 GROUP BY word_id",
                placeholders.join(",")
            );
            let mut values: Vec<&dyn rusqlite::types::ToSql> = Vec::with_capacity(chunk.len() + 1);
            values.push(&user_id);
            for word_id in chunk {
                values.push(word_id);
            }
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(values.as_slice(), |row| {
                let word_id: String = row.get(0)?;
                let first_at = parse_dt(row.get(1)?)?;
                Ok((word_id, first_at))
            })?;
            for row in rows {
                let (word_id, first_at) = row?;
                out.insert(word_id, first_at);
            }
        }
        Ok(out)
    }

    pub fn count_user_records_stats(&self, user_id: &str) -> Result<(usize, usize), StoreError> {
        self.count_user_records_stats_filtered(user_id, None)
    }

    pub fn count_user_records_stats_filtered(
        &self,
        user_id: &str,
        record_type: Option<RecordType>,
    ) -> Result<(usize, usize), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let (total, correct): (i64, i64) = match record_type {
            None => conn.query_row(
                "SELECT COUNT(*), SUM(CASE WHEN is_correct=1 THEN 1 ELSE 0 END) FROM learning_records WHERE user_id=?1",
                params![user_id],
                |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
            )?,
            Some(rt) => conn.query_row(
                "SELECT COUNT(*), SUM(CASE WHEN is_correct=1 THEN 1 ELSE 0 END) FROM learning_records WHERE user_id=?1 AND record_type=?2",
                params![user_id, rt.as_str()],
                |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
            )?,
        };
        Ok((total as usize, correct as usize))
    }

    pub fn get_user_records_filtered(
        &self,
        user_id: &str,
        limit: usize,
        record_type: Option<RecordType>,
    ) -> Result<Vec<LearningRecord>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        match record_type {
            None => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {RECORD_COLS} FROM learning_records WHERE user_id=?1 ORDER BY created_at DESC LIMIT ?2"
                ))?;
                let rows = stmt
                    .query_map(params![user_id, limit as i64], record_from_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
            Some(rt) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {RECORD_COLS} FROM learning_records WHERE user_id=?1 AND record_type=?2 ORDER BY created_at DESC LIMIT ?3"
                ))?;
                let rows = stmt
                    .query_map(params![user_id, rt.as_str(), limit as i64], record_from_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
        }
    }

    pub fn distinct_word_ids_for_type(
        &self,
        user_id: &str,
        record_type: RecordType,
    ) -> Result<HashSet<String>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT word_id FROM learning_records WHERE user_id=?1 AND record_type=?2",
        )?;
        let rows = stmt.query_map(params![user_id, record_type.as_str()], |r| {
            r.get::<_, String>(0)
        })?;
        rows.collect::<Result<HashSet<_>, _>>()
            .map_err(StoreError::from)
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
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM learning_records", [], |r| r.get(0))?;
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
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn daily_active_users(&self, days: u32) -> Result<Vec<(String, i64)>, StoreError> {
        let conn = self.conn()?;
        let since = (chrono::Utc::now() - chrono::Duration::days(days as i64)).date_naive();
        let since_str = since.format("%Y-%m-%d").to_string();
        let mut stmt = conn.prepare(
            "SELECT DATE(created_at) as d, COUNT(DISTINCT user_id)
             FROM learning_records
             WHERE DATE(created_at) >= ?1
             GROUP BY d ORDER BY d",
        )?;
        let rows = stmt.query_map(params![&since_str], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn daily_records(&self, days: u32) -> Result<Vec<(String, i64, i64)>, StoreError> {
        let conn = self.conn()?;
        let since = (chrono::Utc::now() - chrono::Duration::days(days as i64)).date_naive();
        let since_str = since.format("%Y-%m-%d").to_string();
        let mut stmt = conn.prepare(
            "SELECT DATE(created_at) as d, COUNT(*), COALESCE(SUM(CASE WHEN is_correct=1 THEN 1 ELSE 0 END), 0)
             FROM learning_records
             WHERE DATE(created_at) >= ?1
             GROUP BY d ORDER BY d",
        )?;
        let rows = stmt.query_map(params![&since_str], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn count_records_on_date(&self, date_str: &str) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM learning_records WHERE DATE(created_at) = ?1",
            params![date_str],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn count_correct_records_on_date(&self, date_str: &str) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM learning_records WHERE DATE(created_at) = ?1 AND is_correct=1",
            params![date_str],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn count_active_users_on_date(&self, date_str: &str) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT user_id) FROM learning_records WHERE DATE(created_at) = ?1",
            params![date_str],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    fn sample_record(
        id: &str,
        user_id: &str,
        word_id: &str,
        created_at: DateTime<Utc>,
    ) -> LearningRecord {
        LearningRecord {
            id: id.into(),
            user_id: user_id.into(),
            word_id: word_id.into(),
            is_correct: true,
            response_time_ms: 1000,
            session_id: Some("s1".into()),
            created_at,
            record_type: RecordType::All,
        }
    }

    #[test]
    fn records_returned_desc_order() {
        let store = test_store();
        let now = Utc::now();
        store
            .create_record(&sample_record(
                "r1",
                "u1",
                "w1",
                now - Duration::seconds(30),
            ))
            .unwrap();
        store
            .create_record(&sample_record("r2", "u1", "w1", now))
            .unwrap();
        let list = store.get_user_records("u1", 10).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "r2");
        assert_eq!(list[1].id, "r1");
    }

    #[test]
    fn count_records_works() {
        let store = test_store();
        let now = Utc::now();
        store
            .create_record(&sample_record("r1", "u1", "w1", now))
            .unwrap();
        store
            .create_record(&sample_record("r2", "u1", "w2", now))
            .unwrap();
        assert_eq!(store.count_user_records("u1").unwrap(), 2);
        assert_eq!(store.count_all_records().unwrap(), 2);
    }

    fn tempfile_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    #[test]
    fn record_type_as_str_and_parse_roundtrip() {
        for rt in [RecordType::Learning, RecordType::Review, RecordType::All] {
            let s = rt.as_str();
            let parsed = RecordType::parse(s).unwrap();
            assert_eq!(parsed, rt);
        }
        assert!(matches!(
            RecordType::parse("bogus"),
            Err(StoreError::Validation(_))
        ));
        assert_eq!(RecordType::default(), RecordType::All);
    }

    #[test]
    fn get_user_stats_agg_returns_default_when_missing() {
        let store = test_store();
        let s = store.get_user_stats_agg("nobody").unwrap();
        assert_eq!(s.total_records, 0);
        assert_eq!(s.correct_records, 0);
        assert!(s.word_ids.is_empty());
        assert!(s.session_ids.is_empty());
    }

    #[test]
    fn count_user_records_filtered_handles_no_rows_returning_zeros() {
        let store = test_store();
        let (total, correct) = store.count_user_records_stats("u-missing").unwrap();
        assert_eq!(total, 0);
        assert_eq!(correct, 0);
        let (t2, c2) = store
            .count_user_records_stats_filtered("u-missing", Some(RecordType::Review))
            .unwrap();
        assert_eq!((t2, c2), (0, 0));
    }

    #[test]
    fn first_record_times_for_empty_list_short_circuits() {
        let store = test_store();
        let m = store.first_record_times_for_words("u1", &[]).unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn create_record_with_updates_writes_record_state_session_and_stats() {
        use crate::store::operations::learning_sessions::{
            LearningSession, SessionStatus, SessionSummary,
        };
        use crate::store::operations::word_states::{WordLearningState, WordState};
        let (_tmp, store) = tempfile_store();
        store
            .create_user(&super::super::users::User {
                id: "u1".into(),
                email: "a@b.com".into(),
                username: "a".into(),
                password_hash: "h".into(),
                is_banned: false,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                failed_login_count: 0,
                locked_until: None,
            })
            .unwrap();
        // 创建一条 active session
        let now = Utc::now();
        let session = LearningSession {
            id: "s1".into(),
            user_id: "u1".into(),
            status: SessionStatus::Completed,
            target_mastery_count: 5,
            total_questions: 3,
            actual_mastery_count: 1,
            context_shifts: 0,
            created_at: now,
            updated_at: now,
            summary: Some(SessionSummary {
                accuracy: 0.5,
                avg_response_time_ms: 1234,
                mastered_word_ids: vec!["w1".into()],
                error_prone_word_ids: vec!["w2".into()],
                duration_secs: 60,
                hour_of_day: 14,
                final_difficulty: 0.4,
            }),
            correct_count: 2,
            total_count: 3,
        };
        store.create_learning_session(&session).unwrap();
        let word_state = WordLearningState {
            user_id: "u1".into(),
            word_id: "w1".into(),
            state: WordState::Reviewing,
            mastery_level: 0.7,
            next_review_date: Some(now + Duration::hours(1)),
            half_life: 24.0,
            correct_streak: 2,
            total_attempts: 5,
            updated_at: now,
        };
        let record = sample_record("r1", "u1", "w1", now);
        store
            .create_record_with_updates(&record, Some(&word_state), Some(&session))
            .unwrap();

        let stats = store.get_user_stats_agg("u1").unwrap();
        assert_eq!(stats.total_records, 1);
        assert_eq!(stats.correct_records, 1);
        assert!(stats.word_ids.contains("w1"));
        assert!(stats.session_ids.contains("s1"));

        // 第二条记录复用同一个 user，累加 stats
        let r2 = LearningRecord {
            is_correct: false,
            ..sample_record("r2", "u1", "w2", now + Duration::seconds(1))
        };
        store.create_record_with_updates(&r2, None, None).unwrap();
        let stats2 = store.get_user_stats_agg("u1").unwrap();
        assert_eq!(stats2.total_records, 2);
        assert_eq!(stats2.correct_records, 1);
        assert!(stats2.word_ids.contains("w2"));
    }

    #[test]
    fn record_lookup_helpers_cover_session_word_offset_and_between() {
        let store = test_store();
        let now = Utc::now();
        for (i, rid) in ["r1", "r2", "r3"].iter().enumerate() {
            let rec = sample_record(rid, "u1", "w1", now + Duration::seconds(i as i64));
            store.create_record(&rec).unwrap();
        }
        let one = store.get_user_record_by_id("u1", "r2").unwrap().unwrap();
        assert_eq!(one.id, "r2");
        assert!(store.get_user_record_by_id("u1", "missing").unwrap().is_none());

        let by_session = store.list_records_by_session("u1", "s1").unwrap();
        assert_eq!(by_session.len(), 3);

        let page = store.get_user_records_with_offset("u1", 1, 1).unwrap();
        assert_eq!(page.len(), 1);

        let between = store
            .get_user_records_between(
                "u1",
                now,
                now + Duration::seconds(2),
            )
            .unwrap();
        assert_eq!(between.len(), 2);

        let by_word = store.get_user_word_records("u1", "w1", 10).unwrap();
        assert_eq!(by_word.len(), 3);
    }

    #[test]
    fn count_helpers_and_first_record_times() {
        let store = test_store();
        let now = Utc::now();
        store
            .create_record(&sample_record("r1", "u1", "w1", now))
            .unwrap();
        store
            .create_record(&LearningRecord {
                is_correct: false,
                ..sample_record("r2", "u1", "w2", now)
            })
            .unwrap();
        assert_eq!(store.count_all_correct_records().unwrap(), 1);
        assert_eq!(store.count_records_since(now - Duration::seconds(1)).unwrap(), 2);
        assert_eq!(store.count_active_users_since(now - Duration::seconds(1)).unwrap(), 1);

        let map = store
            .first_record_times_for_words("u1", &["w1".to_string(), "w-missing".to_string()])
            .unwrap();
        assert!(map.contains_key("w1"));
        assert!(!map.contains_key("w-missing"));
    }

    #[test]
    fn filtered_lookups_consider_record_type() {
        let store = test_store();
        let now = Utc::now();
        let mut a = sample_record("ra", "u1", "wa", now);
        a.record_type = RecordType::Learning;
        store.create_record(&a).unwrap();
        let mut b = sample_record("rb", "u1", "wb", now);
        b.record_type = RecordType::Review;
        store.create_record(&b).unwrap();

        let learning_total =
            store.count_user_records_stats_filtered("u1", Some(RecordType::Learning)).unwrap();
        assert_eq!(learning_total, (1, 1));
        let review_total =
            store.count_user_records_stats_filtered("u1", Some(RecordType::Review)).unwrap();
        assert_eq!(review_total, (1, 1));

        let only_review = store
            .get_user_records_filtered("u1", 10, Some(RecordType::Review))
            .unwrap();
        assert_eq!(only_review.len(), 1);
        assert_eq!(only_review[0].id, "rb");

        let unfiltered = store.get_user_records_filtered("u1", 10, None).unwrap();
        assert_eq!(unfiltered.len(), 2);

        let learning_words =
            store.distinct_word_ids_for_type("u1", RecordType::Learning).unwrap();
        assert!(learning_words.contains("wa"));
        assert!(!learning_words.contains("wb"));
    }

    #[test]
    fn date_bucket_helpers_match_today_records() {
        let store = test_store();
        let now = Utc::now();
        let today = now.date_naive().format("%Y-%m-%d").to_string();
        store
            .create_record(&sample_record("r1", "u1", "w1", now))
            .unwrap();
        assert_eq!(store.count_records_on_date(&today).unwrap(), 1);
        assert_eq!(store.count_correct_records_on_date(&today).unwrap(), 1);
        assert_eq!(store.count_active_users_on_date(&today).unwrap(), 1);

        let daily = store.daily_active_users(7).unwrap();
        assert!(daily.iter().any(|(d, c)| d == &today && *c == 1));
        let daily_rec = store.daily_records(7).unwrap();
        assert!(daily_rec.iter().any(|(d, t, c)| d == &today && *t == 1 && *c == 1));
    }

    #[test]
    fn create_record_rejects_invalid_id() {
        let store = test_store();
        let now = Utc::now();
        let mut rec = sample_record("", "u1", "w1", now);
        rec.id = "".into();
        assert!(matches!(
            store.create_record(&rec).unwrap_err(),
            StoreError::Validation(_)
        ));
    }
}
