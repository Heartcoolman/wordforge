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
    /// SRS 自评粒度（0=Again / 1=Hard / 2=Good / 3=Easy）；
    /// 客户端选填，落库供 AMAS half-life 模型未来分级回退使用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_rating: Option<u8>,
    /// 出题模式（word-to-meaning / meaning-to-word / audio-to-meaning / meaning-to-spelling）；
    /// 客户端选填，落库供数据分析"答题分布·题型"使用。非法值原样存，NULL 视为"未标注"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserStatsAgg {
    pub total_records: u64,
    pub correct_records: u64,
}

const RECORD_COLS: &str =
    "user_id, id, word_id, is_correct, response_time_ms, session_id, created_at, record_type, self_rating, question_mode";

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
        self_rating: row.get::<_, Option<i64>>(8)?.map(|v| v as u8),
        question_mode: row.get(9)?,
    })
}

impl Store {
    pub fn get_user_stats_agg(&self, user_id: &str) -> Result<UserStatsAgg, StoreError> {
        let conn = self.conn()?;
        let result: Option<(i64, i64)> = conn
            .query_row(
                "SELECT total_records, correct_records FROM user_stats WHERE user_id=?1",
                params![user_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match result {
            Some((total, correct)) => Ok(UserStatsAgg {
                total_records: total as u64,
                correct_records: correct as u64,
            }),
            None => Ok(UserStatsAgg::default()),
        }
    }

    /// 该用户答题过的去重词数（去掉 user_stats 增长型 word_ids_json blob 后的读时替代）。
    /// 取 **records 维度** `COUNT(DISTINCT word_id)`（与旧 word_ids 集合语义 bit 等价；不可用
    /// word_learning_states 行数——裸回放只写 records、reset 删 word_state 留 records，两者会偏）。
    /// 走 `(user_id, word_id)` 覆盖前缀索引，非热路径。
    pub fn count_distinct_words(&self, user_id: &str) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let n: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT word_id) FROM learning_records WHERE user_id=?1",
            params![user_id],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// 该用户的去重会话数（替代 session_ids_json blob）。旧逻辑仅 session_id=Some 时计入，
    /// COUNT(DISTINCT) 本就跳 NULL，显式 `IS NOT NULL` 加防御。走 `(user_id, session_id)` 索引。
    pub fn count_distinct_sessions(&self, user_id: &str) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let n: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT session_id) FROM learning_records WHERE user_id=?1 AND session_id IS NOT NULL",
            params![user_id],
            |r| r.get(0),
        )?;
        Ok(n as u64)
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
            "INSERT INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, session_id, created_at, record_type, self_rating, question_mode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                &record.user_id, &record.id, &record.word_id,
                record.is_correct as i64, record.response_time_ms,
                record.session_id.as_deref(), record.created_at.to_rfc3339(),
                record.record_type.as_str(),
                record.self_rating.map(|v| v as i64),
                record.question_mode.as_deref(),
            ],
        )?;
        Ok(())
    }

    pub fn create_record_with_updates(
        &self,
        record: &LearningRecord,
        word_state: Option<&crate::store::operations::word_states::WordLearningState>,
        learning_session: Option<&crate::store::operations::learning_sessions::LearningSession>,
        just_mastered: bool,
    ) -> Result<(), StoreError> {
        keys::validate_id(&record.id)?;
        let mut conn = self.conn()?;
        // #42：IMMEDIATE 而非默认 DEFERRED——user_stats 计数器是 SELECT total_records→Rust +1→
        // 绝对值 UPSERT 的 read-modify-write，DEFERRED 下两并发同用户事务会各自读到 N 再都写 N+1
        // （丢更新），且读后写升级易触发 SQLITE_BUSY 死锁。BEGIN IMMEDIATE 在事务起点即取写锁，
        // 让并发写串行化而非互相覆盖。
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

        // 幂等插入:仅吞主键 (user_id,id) 冲突。并发同 client_record_id 请求中,其一走"裸记录回放"
        // 抢先插入同一行时,本次全量持久化不再因主键冲突报错——否则调用方会把这当成持久化失败而
        // 误触发 AMAS 原子回滚+清幂等标记,导致本次正确应用的 AMAS 永久丢失(PR #61 审查 P1)。
        // FK/CHECK/NOT NULL 等真实约束冲突仍照常报错,交由调用方回滚。inserted==0 表示行已被并发方
        // 落库(并已计入其 user_stats),本次须跳过 user_stats 计数避免对同一事件双计;word_state/
        // session 仍无条件应用,以落本次 AMAS 派生的真实增量(裸回放方未写)。
        let inserted = tx.execute(
            "INSERT INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, session_id, created_at, record_type, self_rating, question_mode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(user_id, id) DO NOTHING",
            params![
                &record.user_id, &record.id, &record.word_id,
                record.is_correct as i64, record.response_time_ms,
                record.session_id.as_deref(), record.created_at.to_rfc3339(),
                record.record_type.as_str(),
                record.self_rating.map(|v| v as i64),
                record.question_mode.as_deref(),
            ],
        )?;

        if let Some(state) = word_state {
            let next_review = state.next_review_date.map(|d| d.to_rfc3339());
            // total_attempts/correct_streak are written as SQL-relative increments off the row's
            // OWN current value inside this same tx, not the Rust-computed absolute values the
            // caller read before this tx opened — that pre-tx read is stale under concurrent
            // submissions for the same (user_id, word_id) and silently loses increments (whichever
            // write commits last wins with its own stale count). state/mastery_level/next_review_date
            // stay absolute: they're this event's fresh AMAS-derived output, not a prior-value-
            // dependent counter, so no race there. Mirrors apply_elo_in_tx's read/write-in-one-tx
            // guarantee without needing an explicit re-read — the UPDATE itself is the read.
            tx.execute(
                "INSERT INTO word_learning_states (user_id, word_id, state, mastery_level, next_review_date, half_life, correct_streak, total_attempts, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)
                 ON CONFLICT(user_id, word_id) DO UPDATE SET
                    state=?3, mastery_level=?4, next_review_date=?5, half_life=?6,
                    correct_streak = CASE WHEN ?7 = 1 THEN word_learning_states.correct_streak + 1 ELSE 0 END,
                    total_attempts = word_learning_states.total_attempts + 1,
                    updated_at=?8",
                params![
                    &state.user_id, &state.word_id, state.state.as_str(),
                    state.mastery_level, next_review.as_deref(),
                    state.half_life, record.is_correct as i64,
                    state.updated_at.to_rfc3339(),
                ],
            )?;
        }

        if let Some(session) = learning_session {
            let summary = session.summary.as_ref();
            // summary JSON 列仅在调用方真的带 summary 时覆盖（None → COALESCE 保留行现值），
            // 不再无条件写 '[]'——迟到的 record 上报会用 tx 外读到的旧 NULL summary 覆盖
            // complete_session 刚写入的汇总（窄窗竞态）。
            let summary_mastered = summary
                .map(|s| Self::serialize_json(&s.mastered_word_ids))
                .transpose()?;
            let summary_error = summary
                .map(|s| Self::serialize_json(&s.error_prone_word_ids))
                .transpose()?;
            // total_questions/total_count/correct_count/actual_mastery_count: same read-outside-tx
            // race as word_learning_states above (caller's Rust `+= 1` reads a pre-tx snapshot).
            // Written here as SQL-relative increments off the row's own value instead — total_questions/
            // total_count always +1 per record (no bound param needed), correct_count +1 only when this
            // record was correct. actual_mastery_count 的 `just_mastered` 实为「本次答题后该词处于
            // Mastered 级」的水平判定而非进入边沿——已 Mastered 的词每答一次都会 +1（调用方按
            // mastery_level == Mastered 计算，无 prev!=Mastered 边沿检测）。命名沿用历史、口径不改，
            // 如实记录以免误读为"仅首次跨入时 +1"。
            // status：单调迁移——行现值已是 completed 时保持不变（CASE），否则才接受调用方传入值。
            // 调用方的 status 是 tx 外预读的陈旧快照，绝对回写会在窄窗内把并发 complete_session
            // 刚提交的 completed 打回 active。summary_* 同理仅在调用方带值（非 NULL 参数）时覆盖。
            // context_shifts/updated_at stay absolute: not per-record monotonic counters here.
            tx.execute(
                "UPDATE learning_sessions SET
                    status = CASE WHEN status='completed' THEN status ELSE ?1 END,
                    total_questions = total_questions + 1,
                    actual_mastery_count = actual_mastery_count + ?2,
                    context_shifts=?3,
                    updated_at=?4,
                    summary_accuracy = COALESCE(?5, summary_accuracy),
                    summary_avg_response_time_ms = COALESCE(?6, summary_avg_response_time_ms),
                    summary_mastered_word_ids_json = COALESCE(?7, summary_mastered_word_ids_json),
                    summary_error_prone_word_ids_json = COALESCE(?8, summary_error_prone_word_ids_json),
                    summary_duration_secs = COALESCE(?9, summary_duration_secs),
                    summary_hour_of_day = COALESCE(?10, summary_hour_of_day),
                    summary_final_difficulty = COALESCE(?11, summary_final_difficulty),
                    correct_count = correct_count + ?12,
                    total_count = total_count + 1
                 WHERE id=?13 AND user_id=?14",
                params![
                    session.status.as_str(), just_mastered as i64,
                    session.context_shifts as i64,
                    session.updated_at.to_rfc3339(),
                    summary.map(|s| s.accuracy),
                    summary.map(|s| s.avg_response_time_ms),
                    summary_mastered, summary_error,
                    summary.map(|s| s.duration_secs),
                    summary.map(|s| s.hour_of_day as i64),
                    summary.map(|s| s.final_difficulty),
                    record.is_correct as i64,
                    &session.id, &session.user_id,
                ],
            )?;
        }

        // Update user_stats —— 仅在记录行确实新插入时累加,避免并发裸回放与全量持久化对同一事件双计。
        // 纯 SQL 自增（不再读改写、不再维护 word_ids_json/session_ids_json 增长型集合——那两列只为
        // 读基数而存，已改由 count_distinct_words/sessions 读时 COUNT(DISTINCT) 求得）。
        // word_ids_json/session_ids_json 列保留 DEFAULT '[]' 仅停写（零迁移、可回滚）。
        if inserted > 0 {
            tx.execute(
                "INSERT INTO user_stats (user_id, total_records, correct_records)
                 VALUES (?1, 1, ?2)
                 ON CONFLICT(user_id) DO UPDATE SET
                    total_records = total_records + 1,
                    correct_records = correct_records + ?2",
                params![&record.user_id, record.is_correct as i64],
            )?;
        }

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
            self_rating: None,
            question_mode: None,
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
        assert_eq!(store.count_distinct_words("nobody").unwrap(), 0);
        assert_eq!(store.count_distinct_sessions("nobody").unwrap(), 0);
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
                role: "user".into(),
                status: "active".into(),
                last_login_at: None,
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
            .create_record_with_updates(&record, Some(&word_state), Some(&session), true)
            .unwrap();

        let stats = store.get_user_stats_agg("u1").unwrap();
        assert_eq!(stats.total_records, 1);
        assert_eq!(stats.correct_records, 1);
        assert_eq!(store.count_distinct_words("u1").unwrap(), 1);
        assert_eq!(store.count_distinct_sessions("u1").unwrap(), 1);

        // 第二条记录复用同一个 user，累加 stats
        let r2 = LearningRecord {
            is_correct: false,
            ..sample_record("r2", "u1", "w2", now + Duration::seconds(1))
        };
        store
            .create_record_with_updates(&r2, None, None, false)
            .unwrap();
        let stats2 = store.get_user_stats_agg("u1").unwrap();
        assert_eq!(stats2.total_records, 2);
        assert_eq!(stats2.correct_records, 1);
        assert_eq!(store.count_distinct_words("u1").unwrap(), 2);
    }

    /// PR #61 审查 P1 回归:并发同 client_record_id 下,"裸记录回放"先落库后,在途"全量持久化"
    /// 到达时记录行已存在——必须幂等(不报错、user_stats 不双计),且仍落 AMAS 派生的 word_state/
    /// session(否则全量方此前会因主键冲突误回滚 AMAS、永久丢失本次处理结果)。
    #[test]
    fn create_record_with_updates_idempotent_when_row_already_exists() {
        use crate::store::operations::learning_sessions::{LearningSession, SessionStatus};
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
                role: "user".into(),
                status: "active".into(),
                last_login_at: None,
            })
            .unwrap();
        let now = Utc::now();
        let session = LearningSession {
            id: "s1".into(),
            user_id: "u1".into(),
            status: SessionStatus::Active,
            target_mastery_count: 5,
            total_questions: 0,
            actual_mastery_count: 0,
            context_shifts: 0,
            created_at: now,
            updated_at: now,
            summary: None,
            correct_count: 0,
            total_count: 0,
        };
        store.create_learning_session(&session).unwrap();
        let word_state = WordLearningState {
            user_id: "u1".into(),
            word_id: "w1".into(),
            state: WordState::Reviewing,
            mastery_level: 0.7,
            next_review_date: None,
            half_life: 24.0,
            correct_streak: 2,
            total_attempts: 5,
            updated_at: now,
        };
        let record = sample_record("r1", "u1", "w1", now); // is_correct=true

        // 模拟并发:裸记录回放先落库(无 word_state/session)。
        store
            .create_record_with_updates(&record, None, None, false)
            .unwrap();
        // 随后在途全量持久化到达,记录行已存在:不得报错、不得双计 user_stats、须落 word_state。
        store
            .create_record_with_updates(&record, Some(&word_state), Some(&session), false)
            .unwrap();

        let stats = store.get_user_stats_agg("u1").unwrap();
        assert_eq!(stats.total_records, 1, "同一事件只计一次,不因二次持久化双计");
        assert_eq!(stats.correct_records, 1);
        // 记录行唯一
        assert!(store.get_user_record_by_id("u1", "r1").unwrap().is_some());
        // 全量持久化的 AMAS 派生 word_state 已落库(裸回放未写,证明二次调用确实应用了增量)
        let wls = store.get_word_learning_state("u1", "w1").unwrap().unwrap();
        assert!((wls.mastery_level - 0.7).abs() < 1e-9);
        // total_attempts 现由本函数在 tx 内按行自身当前值 SQL 相对 +1 写入(见
        // create_record_with_updates 内联注释),不再采信传入 word_state.total_attempts 的绝对值
        // (该 struct 字段是调用方 tx 外预读+Rust 累加的产物,在并发下本就可能失真——这正是本次修复
        // 要消除的漂移源)。此处 word_learning_states 行此前不存在(上一次调用 word_state=None 未
        // 写入),故这是首次真实 INSERT,相对 +1 落地为 1,而非传入 struct 里人为设置的 5。
        assert_eq!(wls.total_attempts, 1);
    }

    /// 回归:并发同 (user_id, word_id) 下,两次调用各自携带"调用方在 tx 外预读+Rust 累加"的
    /// word_state(均基于同一份陈旧 total_attempts=0 计算，都算出 total_attempts=1，模拟两个并发
    /// 请求各自读到同一快照)。若 create_record_with_updates 仍按调用方传入的绝对值写入，两次调用
    /// 后 total_attempts 会停在 1(后写者用自己算的"1"覆盖前一次的"1"，丢一次增量)；本次修复后
    /// 应为 2，因为实际写入是在 tx 内对行自身当前值做 SQL 相对 +1，与调用方传入的绝对值无关。
    #[test]
    fn create_record_with_updates_concurrent_stale_reads_still_increment_correctly() {
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
                role: "user".into(),
                status: "active".into(),
                last_login_at: None,
            })
            .unwrap();
        let now = Utc::now();
        // 两次调用共用同一个"陈旧读"结果构造出的 word_state(total_attempts/correct_streak 都算
        // 成好像是从 0 开始的第一次尝试)，模拟两个并发请求各自基于同一快照做 Rust 端 +1。
        let stale_word_state = WordLearningState {
            user_id: "u1".into(),
            word_id: "w1".into(),
            state: WordState::Learning,
            mastery_level: 0.3,
            next_review_date: None,
            half_life: 12.0,
            correct_streak: 1,
            total_attempts: 1,
            updated_at: now,
        };
        let r1 = LearningRecord {
            is_correct: true,
            ..sample_record("cr1", "u1", "w1", now)
        };
        let r2 = LearningRecord {
            is_correct: true,
            ..sample_record("cr2", "u1", "w1", now + Duration::seconds(1))
        };
        store
            .create_record_with_updates(&r1, Some(&stale_word_state), None, false)
            .unwrap();
        store
            .create_record_with_updates(&r2, Some(&stale_word_state), None, false)
            .unwrap();

        let wls = store.get_word_learning_state("u1", "w1").unwrap().unwrap();
        assert_eq!(
            wls.total_attempts, 2,
            "两次真实调用必须各计一次，不能因调用方的陈旧快照互相覆盖"
        );
        assert_eq!(
            wls.correct_streak, 2,
            "两次都正确作答，streak 应连续 +1 到 2，而非停在传入的陈旧值 1"
        );
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
        assert!(store
            .get_user_record_by_id("u1", "missing")
            .unwrap()
            .is_none());

        let by_session = store.list_records_by_session("u1", "s1").unwrap();
        assert_eq!(by_session.len(), 3);

        let page = store.get_user_records_with_offset("u1", 1, 1).unwrap();
        assert_eq!(page.len(), 1);

        let between = store
            .get_user_records_between("u1", now, now + Duration::seconds(2))
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
        assert_eq!(
            store
                .count_records_since(now - Duration::seconds(1))
                .unwrap(),
            2
        );
        assert_eq!(
            store
                .count_active_users_since(now - Duration::seconds(1))
                .unwrap(),
            1
        );

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

        let learning_total = store
            .count_user_records_stats_filtered("u1", Some(RecordType::Learning))
            .unwrap();
        assert_eq!(learning_total, (1, 1));
        let review_total = store
            .count_user_records_stats_filtered("u1", Some(RecordType::Review))
            .unwrap();
        assert_eq!(review_total, (1, 1));

        let only_review = store
            .get_user_records_filtered("u1", 10, Some(RecordType::Review))
            .unwrap();
        assert_eq!(only_review.len(), 1);
        assert_eq!(only_review[0].id, "rb");

        let unfiltered = store.get_user_records_filtered("u1", 10, None).unwrap();
        assert_eq!(unfiltered.len(), 2);

        let learning_words = store
            .distinct_word_ids_for_type("u1", RecordType::Learning)
            .unwrap();
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
        assert!(daily_rec
            .iter()
            .any(|(d, t, c)| d == &today && *t == 1 && *c == 1));
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
