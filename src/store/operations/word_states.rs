use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordLearningState {
    pub user_id: String,
    pub word_id: String,
    pub state: WordState,
    pub mastery_level: f64,
    pub next_review_date: Option<DateTime<Utc>>,
    pub half_life: f64,
    pub correct_streak: u32,
    pub total_attempts: u32,
    pub updated_at: DateTime<Utc>,
}

/// P3#7：wire 上以 lowercase 序列化（与 SessionStatus 风格一致）。
/// 注意：DB 持久化层仍用大写字符串（schema CHECK 限定 'NEW'/'MASTERED'/…，以及一批 SQL
/// 聚合用了 state = 'MASTERED' 形式），由独立的 `as_str` / `from_str` 维持兼容。换言之：
///   wire 出/入 = lowercase（serde 控制）
///   DB  出/入 = uppercase（as_str / from_str 控制）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WordState {
    New,
    Learning,
    Reviewing,
    Mastered,
    Forgotten,
}

impl WordState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::New => "NEW",
            Self::Learning => "LEARNING",
            Self::Reviewing => "REVIEWING",
            Self::Mastered => "MASTERED",
            Self::Forgotten => "FORGOTTEN",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, StoreError> {
        match s {
            "NEW" => Ok(Self::New),
            "LEARNING" => Ok(Self::Learning),
            "REVIEWING" => Ok(Self::Reviewing),
            "MASTERED" => Ok(Self::Mastered),
            "FORGOTTEN" => Ok(Self::Forgotten),
            _ => Err(StoreError::Validation(format!("invalid word state: {s}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WordStateStats {
    pub new_count: u64,
    pub learning: u64,
    pub reviewing: u64,
    pub mastered: u64,
    pub forgotten: u64,
    /// 预计完成今日到期复习所需分钟（dueCount × avg_response_time_ms / 60000，向上取整；无样本时回退 5 秒/词）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_review_estimated_minutes: Option<u32>,
}

const WLS_COLS: &str =
    "user_id, word_id, state, mastery_level, next_review_date, half_life, correct_streak, total_attempts, updated_at";

fn wls_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WordLearningState> {
    let state_str: String = row.get(2)?;
    let state = WordState::from_str(&state_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let next_review: Option<String> = row.get(4)?;
    Ok(WordLearningState {
        user_id: row.get(0)?,
        word_id: row.get(1)?,
        state,
        mastery_level: row.get(3)?,
        next_review_date: next_review.map(parse_dt).transpose()?,
        half_life: row.get(5)?,
        correct_streak: row.get::<_, i64>(6)? as u32,
        total_attempts: row.get::<_, i64>(7)? as u32,
        updated_at: parse_dt(row.get(8)?)?,
    })
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

impl Store {
    pub fn get_word_learning_state(
        &self,
        user_id: &str,
        word_id: &str,
    ) -> Result<Option<WordLearningState>, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        Self::get_word_learning_state_conn(&conn, user_id, word_id)
    }

    pub(crate) fn get_word_learning_state_conn(
        conn: &rusqlite::Connection,
        user_id: &str,
        word_id: &str,
    ) -> Result<Option<WordLearningState>, StoreError> {
        Ok(conn
            .query_row(
                &format!(
                    "SELECT {WLS_COLS} FROM word_learning_states WHERE user_id=?1 AND word_id=?2"
                ),
                params![user_id, word_id],
                wls_from_row,
            )
            .optional()?)
    }

    pub fn set_word_learning_state(&self, wls: &WordLearningState) -> Result<(), StoreError> {
        keys::validate_id(&wls.user_id)?;
        keys::validate_id(&wls.word_id)?;
        let conn = self.conn()?;
        Self::set_word_learning_state_conn(&conn, wls)
    }

    pub(crate) fn set_word_learning_state_conn(
        conn: &rusqlite::Connection,
        wls: &WordLearningState,
    ) -> Result<(), StoreError> {
        let next_review = wls.next_review_date.map(|d| d.to_rfc3339());
        conn.execute(
            "INSERT OR REPLACE INTO word_learning_states
             (user_id, word_id, state, mastery_level, next_review_date, half_life, correct_streak, total_attempts, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &wls.user_id,
                &wls.word_id,
                wls.state.as_str(),
                wls.mastery_level,
                next_review.as_deref(),
                wls.half_life,
                wls.correct_streak as i64,
                wls.total_attempts as i64,
                wls.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_word_states_batch(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<Vec<WordLearningState>, StoreError> {
        keys::validate_id(user_id)?;
        if word_ids.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn()?;
        let placeholders: Vec<String> =
            (0..word_ids.len()).map(|i| format!("?{}", i + 2)).collect();
        let sql = format!(
            "SELECT {WLS_COLS} FROM word_learning_states WHERE user_id=?1 AND word_id IN ({})",
            placeholders.join(",")
        );
        let mut stmt = conn.prepare(&sql)?;

        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> =
            Vec::with_capacity(word_ids.len() + 1);
        param_values.push(Box::new(user_id.to_string()));
        for wid in word_ids {
            param_values.push(Box::new(wid.clone()));
        }
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let rows: std::collections::HashMap<String, WordLearningState> = stmt
            .query_map(param_refs.as_slice(), wls_from_row)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|s| (s.word_id.clone(), s))
            .collect();

        let mut result = Vec::with_capacity(word_ids.len());
        for wid in word_ids {
            if let Some(state) = rows.get(wid) {
                result.push(state.clone());
            }
        }
        Ok(result)
    }

    pub fn get_due_words(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<WordLearningState>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let now = Utc::now().to_rfc3339();
        let mut stmt = conn.prepare(&format!(
            "SELECT {WLS_COLS} FROM word_learning_states
             WHERE user_id=?1 AND next_review_date IS NOT NULL AND next_review_date <= ?2
             ORDER BY next_review_date ASC
             LIMIT ?3"
        ))?;
        let states = stmt
            .query_map(params![user_id, &now, limit as i64], wls_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(states)
    }

    pub fn get_word_state_stats(&self, user_id: &str) -> Result<WordStateStats, StoreError> {
        self.get_word_state_stats_filtered(user_id, None)
    }

    /// 计算 due 词条 ETA：dueCount × avgResponseTimeMs / 60000，向上取整；无样本时按 5s/词 估算。
    pub fn get_due_review_estimated_minutes(&self, user_id: &str) -> Result<u32, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let now = Utc::now().to_rfc3339();
        let due_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM word_learning_states
             WHERE user_id=?1 AND next_review_date IS NOT NULL AND next_review_date <= ?2",
            params![user_id, &now],
            |r| r.get(0),
        )?;
        if due_count <= 0 {
            return Ok(0);
        }
        // 用最近 200 条记录的平均响应时间作为单词耗时估算；不足则回退 5000ms/词。
        let avg_ms: Option<f64> = conn
            .query_row(
                "SELECT AVG(response_time_ms) FROM (
                    SELECT response_time_ms FROM learning_records
                    WHERE user_id=?1 ORDER BY created_at DESC LIMIT 200
                 )",
                params![user_id],
                |r| r.get::<_, Option<f64>>(0),
            )
            .optional()?
            .flatten();
        let per_word_ms = avg_ms
            .filter(|v| v.is_finite() && *v >= 500.0)
            .unwrap_or(5000.0);
        let total_ms = (due_count as f64) * per_word_ms;
        let minutes = (total_ms / 60_000.0).ceil();
        Ok(minutes.clamp(0.0, u32::MAX as f64) as u32)
    }

    pub fn get_word_state_stats_filtered(
        &self,
        user_id: &str,
        record_type: Option<crate::store::operations::records::RecordType>,
    ) -> Result<WordStateStats, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stats = WordStateStats::default();
        let rows: Vec<(String, i64)> = match record_type {
            None => {
                let mut stmt = conn.prepare(
                    "SELECT state, COUNT(*) FROM word_learning_states WHERE user_id=?1 GROUP BY state",
                )?;
                let rows = stmt
                    .query_map(params![user_id], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            }
            Some(rt) => {
                let mut stmt = conn.prepare(
                    "SELECT wls.state, COUNT(*) FROM word_learning_states wls
                     WHERE wls.user_id=?1 AND wls.word_id IN (
                        SELECT DISTINCT word_id FROM learning_records WHERE user_id=?1 AND record_type=?2
                     )
                     GROUP BY wls.state",
                )?;
                let rows = stmt
                    .query_map(params![user_id, rt.as_str()], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            }
        };
        for (state, count) in rows {
            match state.as_str() {
                "NEW" => stats.new_count = count as u64,
                "LEARNING" => stats.learning = count as u64,
                "REVIEWING" => stats.reviewing = count as u64,
                "MASTERED" => stats.mastered = count as u64,
                "FORGOTTEN" => stats.forgotten = count as u64,
                _ => {}
            }
        }
        Ok(stats)
    }

    pub fn delete_word_learning_state(
        &self,
        user_id: &str,
        word_id: &str,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM word_learning_states WHERE user_id=?1 AND word_id=?2",
            params![user_id, word_id],
        )?;
        Ok(())
    }

    pub fn list_user_word_states(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WordLearningState>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {WLS_COLS} FROM word_learning_states WHERE user_id=?1 LIMIT ?2 OFFSET ?3"
        ))?;
        let states = stmt
            .query_map(params![user_id, limit as i64, offset as i64], wls_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(states)
    }

    pub fn list_all_user_word_states(
        &self,
        user_id: &str,
    ) -> Result<Vec<WordLearningState>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {WLS_COLS} FROM word_learning_states
             WHERE user_id=?1
             ORDER BY updated_at DESC"
        ))?;
        let states = stmt
            .query_map(params![user_id], wls_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(states)
    }
}

#[cfg(test)]
mod tests {
    use super::{WordLearningState, WordState};
    use crate::store::Store;
    use chrono::{Duration, Utc};

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    fn mock_word_learning_state(
        user_id: &str,
        word_id: &str,
        total_attempts: u32,
    ) -> WordLearningState {
        WordLearningState {
            user_id: user_id.to_string(),
            word_id: word_id.to_string(),
            state: WordState::Learning,
            mastery_level: 0.42,
            next_review_date: None,
            half_life: 24.0,
            correct_streak: 1,
            total_attempts,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn set_and_get_word_state() {
        let store = test_store();
        let wls = mock_word_learning_state("u1", "w1", 3);
        store.set_word_learning_state(&wls).unwrap();
        let got = store.get_word_learning_state("u1", "w1").unwrap().unwrap();
        assert_eq!(got.word_id, "w1");
        assert_eq!(got.total_attempts, 3);
    }

    #[test]
    fn get_missing_returns_none() {
        let store = test_store();
        assert!(store
            .get_word_learning_state("u1", "missing")
            .unwrap()
            .is_none());
    }

    #[test]
    fn get_word_states_batch_preserves_order_duplicates_and_skips_missing() {
        let store = test_store();
        let w1 = mock_word_learning_state("u1", "w1", 3);
        let w3 = mock_word_learning_state("u1", "w3", 7);
        store.set_word_learning_state(&w1).unwrap();
        store.set_word_learning_state(&w3).unwrap();

        let results = store
            .get_word_states_batch(
                "u1",
                &[
                    "w3".to_string(),
                    "missing".to_string(),
                    "w1".to_string(),
                    "w3".to_string(),
                ],
            )
            .unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].word_id, "w3");
        assert_eq!(results[1].word_id, "w1");
        assert_eq!(results[2].word_id, "w3");
        assert_eq!(results[0].total_attempts, 7);
        assert_eq!(results[1].total_attempts, 3);
        assert_eq!(results[2].total_attempts, 7);
    }

    #[test]
    fn get_due_words_returns_asc_order_and_respects_limit() {
        let store = test_store();
        let now = Utc::now();
        let mut w1 = mock_word_learning_state("u1", "w1", 1);
        w1.next_review_date = Some(now - Duration::minutes(5));
        let mut w2 = mock_word_learning_state("u1", "w2", 1);
        w2.next_review_date = Some(now - Duration::minutes(1));
        let mut w3 = mock_word_learning_state("u1", "w3", 1);
        w3.next_review_date = Some(now - Duration::minutes(3));
        let mut w4 = mock_word_learning_state("u1", "w4", 1);
        w4.next_review_date = Some(now + Duration::minutes(1));

        store.set_word_learning_state(&w1).unwrap();
        store.set_word_learning_state(&w2).unwrap();
        store.set_word_learning_state(&w3).unwrap();
        store.set_word_learning_state(&w4).unwrap();

        let due = store.get_due_words("u1", 2).unwrap();
        assert_eq!(due.len(), 2);
        assert_eq!(due[0].word_id, "w1");
        assert_eq!(due[1].word_id, "w3");
    }

    #[test]
    fn get_due_words_uses_latest_review_date_after_update() {
        let store = test_store();
        let now = Utc::now();
        let mut state = mock_word_learning_state("u1", "w1", 1);
        state.next_review_date = Some(now - Duration::minutes(5));
        store.set_word_learning_state(&state).unwrap();

        state.next_review_date = Some(now - Duration::minutes(1));
        store.set_word_learning_state(&state).unwrap();

        let due = store.get_due_words("u1", 10).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].word_id, "w1");
        assert_eq!(due[0].next_review_date, state.next_review_date);
    }

    #[test]
    fn deleted_word_state_disappears_from_due_words() {
        let store = test_store();
        let now = Utc::now();
        let mut state = mock_word_learning_state("u1", "w1", 1);
        state.next_review_date = Some(now - Duration::minutes(2));
        store.set_word_learning_state(&state).unwrap();

        assert_eq!(store.get_due_words("u1", 10).unwrap().len(), 1);
        store.delete_word_learning_state("u1", "w1").unwrap();
        assert!(store.get_due_words("u1", 10).unwrap().is_empty());
    }

    #[test]
    fn word_state_stats_counts_correctly() {
        let store = test_store();
        let mut s1 = mock_word_learning_state("u1", "w1", 1);
        s1.state = WordState::New;
        let mut s2 = mock_word_learning_state("u1", "w2", 1);
        s2.state = WordState::Learning;
        let mut s3 = mock_word_learning_state("u1", "w3", 1);
        s3.state = WordState::Mastered;

        store.set_word_learning_state(&s1).unwrap();
        store.set_word_learning_state(&s2).unwrap();
        store.set_word_learning_state(&s3).unwrap();

        let stats = store.get_word_state_stats("u1").unwrap();
        assert_eq!(stats.new_count, 1);
        assert_eq!(stats.learning, 1);
        assert_eq!(stats.mastered, 1);
        assert_eq!(stats.reviewing, 0);
        assert_eq!(stats.forgotten, 0);
    }

    #[test]
    fn list_user_word_states_with_limit_offset() {
        let store = test_store();
        for i in 0..5 {
            let wls = mock_word_learning_state("u1", &format!("w{i}"), i);
            store.set_word_learning_state(&wls).unwrap();
        }
        let page = store.list_user_word_states("u1", 2, 1).unwrap();
        assert_eq!(page.len(), 2);
    }

    #[test]
    fn upsert_overwrites_existing() {
        let store = test_store();
        let mut wls = mock_word_learning_state("u1", "w1", 1);
        store.set_word_learning_state(&wls).unwrap();

        wls.total_attempts = 5;
        wls.state = WordState::Mastered;
        store.set_word_learning_state(&wls).unwrap();

        let got = store.get_word_learning_state("u1", "w1").unwrap().unwrap();
        assert_eq!(got.total_attempts, 5);
        assert_eq!(got.state, WordState::Mastered);
    }

    use crate::store::operations::records::{LearningRecord, RecordType};
    use crate::store::StoreError;

    fn tempfile_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    #[test]
    fn word_state_as_str_and_from_str_roundtrip() {
        for st in [
            WordState::New,
            WordState::Learning,
            WordState::Reviewing,
            WordState::Mastered,
            WordState::Forgotten,
        ] {
            let s = st.as_str();
            let parsed = WordState::from_str(s).unwrap();
            assert_eq!(parsed, st);
        }
        assert!(matches!(
            WordState::from_str("bogus"),
            Err(StoreError::Validation(_))
        ));
    }

    #[test]
    fn validation_rejects_empty_ids() {
        let store = test_store();
        assert!(matches!(
            store.get_word_learning_state("", "w").unwrap_err(),
            StoreError::Validation(_)
        ));
        assert!(matches!(
            store.delete_word_learning_state("u", "").unwrap_err(),
            StoreError::Validation(_)
        ));
        let mut bad = mock_word_learning_state("", "w", 1);
        bad.user_id = "".into();
        assert!(matches!(
            store.set_word_learning_state(&bad).unwrap_err(),
            StoreError::Validation(_)
        ));
    }

    #[test]
    fn get_word_states_batch_empty_input_returns_empty() {
        let store = test_store();
        let v = store.get_word_states_batch("u1", &[]).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn get_due_words_zero_limit_short_circuits() {
        let store = test_store();
        let v = store.get_due_words("u1", 0).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn list_all_user_word_states_orders_by_updated_desc() {
        let store = test_store();
        let now = Utc::now();
        let mut s1 = mock_word_learning_state("u1", "w1", 1);
        s1.updated_at = now - Duration::seconds(10);
        let mut s2 = mock_word_learning_state("u1", "w2", 1);
        s2.updated_at = now;
        store.set_word_learning_state(&s1).unwrap();
        store.set_word_learning_state(&s2).unwrap();
        let all = store.list_all_user_word_states("u1").unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].word_id, "w2");
    }

    #[test]
    fn due_review_eta_returns_zero_when_no_due() {
        let store = test_store();
        // 没有 word_learning_states
        assert_eq!(store.get_due_review_estimated_minutes("u1").unwrap(), 0);
    }

    #[test]
    fn due_review_eta_uses_avg_response_time_when_available() {
        let (_t, store) = tempfile_store();
        let now = Utc::now();
        // 一条 due 词条
        let mut state = mock_word_learning_state("u1", "w1", 1);
        state.next_review_date = Some(now - Duration::minutes(1));
        store.set_word_learning_state(&state).unwrap();
        // 一条 learning record，avg response time 600ms
        let r = LearningRecord {
            id: "r1".into(),
            user_id: "u1".into(),
            word_id: "w1".into(),
            is_correct: true,
            response_time_ms: 600,
            session_id: None,
            created_at: now,
            record_type: RecordType::All,
            self_rating: None,
        };
        store.create_record(&r).unwrap();
        let eta = store.get_due_review_estimated_minutes("u1").unwrap();
        // 1 个词 * 600 ms = 600 ms -> 0.01 min ceil -> 1
        assert_eq!(eta, 1);
    }

    #[test]
    fn due_review_eta_falls_back_to_5000ms_when_avg_too_small() {
        let (_t, store) = tempfile_store();
        let now = Utc::now();
        let mut state = mock_word_learning_state("u1", "w1", 1);
        state.next_review_date = Some(now - Duration::minutes(1));
        store.set_word_learning_state(&state).unwrap();
        // 平均 100ms，低于 500ms 阈值 → 回退 5000ms
        let r = LearningRecord {
            id: "r1".into(),
            user_id: "u1".into(),
            word_id: "w1".into(),
            is_correct: true,
            response_time_ms: 100,
            session_id: None,
            created_at: now,
            record_type: RecordType::All,
            self_rating: None,
        };
        store.create_record(&r).unwrap();
        let eta = store.get_due_review_estimated_minutes("u1").unwrap();
        // 1 个词 * 5000 ms = 5s -> ceil(5/60) = 1
        assert_eq!(eta, 1);
    }

    #[test]
    fn word_state_stats_filtered_by_record_type() {
        let (_t, store) = tempfile_store();
        // 两个 word state，仅 w1 在 learning record 中
        let mut s1 = mock_word_learning_state("u1", "w1", 1);
        s1.state = WordState::Learning;
        store.set_word_learning_state(&s1).unwrap();
        let mut s2 = mock_word_learning_state("u1", "w2", 1);
        s2.state = WordState::Mastered;
        store.set_word_learning_state(&s2).unwrap();

        let r = LearningRecord {
            id: "r1".into(),
            user_id: "u1".into(),
            word_id: "w1".into(),
            is_correct: true,
            response_time_ms: 100,
            session_id: None,
            created_at: Utc::now(),
            record_type: RecordType::Learning,
            self_rating: None,
        };
        store.create_record(&r).unwrap();
        let stats = store
            .get_word_state_stats_filtered("u1", Some(RecordType::Learning))
            .unwrap();
        assert_eq!(stats.learning, 1);
        assert_eq!(stats.mastered, 0);
    }

    #[test]
    fn write_with_next_review_date_persists_and_reads_back() {
        let store = test_store();
        let now = Utc::now();
        let mut s = mock_word_learning_state("u1", "w1", 1);
        s.next_review_date = Some(now);
        store.set_word_learning_state(&s).unwrap();
        let got = store.get_word_learning_state("u1", "w1").unwrap().unwrap();
        assert!(got.next_review_date.is_some());
    }
}
