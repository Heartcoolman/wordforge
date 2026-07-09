//! admin 词库中心的跨用户聚合查询与审计读写。
//!
//! 统计口径均为"admin 全局视角"(跨所有用户聚合);时间窗口(如 now-7d)由 route
//! 层算好 `DateTime<Utc>` cutoff 传入。每个方法返回 tightly-scoped 行类型,route
//! 层负责组装 JSON。

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::Serialize;

use crate::store::keys;
use crate::store::operations::wordbooks::Wordbook;
use crate::store::operations::words::Word;
use crate::store::{Store, StoreError};

/// 列表行:词库摘要 + 跨用户统计。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordbookSummaryRow {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub book_type: String,
    pub user_id: Option<String>,
    pub owner_email: Option<String>,
    pub word_count: i64,
    pub active_users: i64,
    pub tags: Vec<String>,
    pub created_at: String,
}

/// 列表附带的跨类型计数。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordbookCounts {
    pub all: i64,
    pub system: i64,
    pub user: i64,
    pub total_entries: i64,
}

/// 单词库统计卡。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordbookStats {
    pub wordbook_id: String,
    pub total_words: i64,
    pub active_users: i64,
    pub avg_mastery: f64,
    pub weekly_answers: i64,
}

/// 词条行(不含 embedding)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordEntryRow {
    pub id: String,
    pub text: String,
    pub pronunciation: Option<String>,
    pub part_of_speech: Option<String>,
    pub meaning: String,
    pub examples: Vec<String>,
    pub appear_count: i64,
    pub accuracy: Option<f64>,
}

/// 热力图单元。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapCell {
    pub word_id: String,
    pub text: String,
    pub count: i64,
}

/// 用户掌握度分桶。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionBucket {
    pub label: String,
    pub min: f64,
    pub max: f64,
    pub user_count: i64,
}

/// 审计日志行。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordbookAuditEntry {
    pub id: String,
    pub wordbook_id: String,
    pub action: String,
    pub detail: String,
    pub admin_id: Option<String>,
    pub created_at: String,
}

/// 列表查询过滤参数。
#[derive(Debug, Clone)]
pub struct WordbookListFilter {
    /// "all" | "system" | "user"
    pub type_filter: String,
    pub search: Option<String>,
    /// "newest" | "hottest" | 默认(system 在前再 createdAt desc)
    pub sort: String,
    pub limit: i64,
    pub offset: i64,
}

/// 词条查询过滤参数。
#[derive(Debug, Clone)]
pub struct WordEntryFilter {
    pub search: Option<String>,
    pub pos: Option<String>,
    /// "frequency" | "alpha" | "difficulty"
    pub sort: String,
    pub limit: i64,
    pub offset: i64,
}

fn esc_like(s: &str) -> String {
    // SQLite LIKE 默认大小写不敏感(仅 ASCII)。转义 % _ \ 防注入通配符。
    let escaped = s
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

impl Store {
    /// 跨用户列表 + counts。counts 始终按全量(忽略 type/search)统计 all/system/user/
    /// totalEntries;items 受 type/search/sort/分页约束。
    pub fn list_all_wordbooks(
        &self,
        filter: &WordbookListFilter,
    ) -> Result<(Vec<WordbookSummaryRow>, i64, WordbookCounts), StoreError> {
        let conn = self.conn()?;

        // counts:全量(不受 type/search 影响)
        let mut counts = WordbookCounts::default();
        let (all, sys, usr, total_entries): (i64, i64, i64, i64) = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN book_type = 'system' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN book_type = 'user' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(word_count), 0)
             FROM wordbooks",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        counts.all = all;
        counts.system = sys;
        counts.user = usr;
        counts.total_entries = total_entries;

        // WHERE 拼装(type + search)
        let mut where_clauses: Vec<String> = Vec::new();
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        match filter.type_filter.as_str() {
            "system" => where_clauses.push("wb.book_type = 'system'".into()),
            "user" => where_clauses.push("wb.book_type = 'user'".into()),
            _ => {}
        }
        if let Some(q) = filter.search.as_deref().filter(|s| !s.is_empty()) {
            where_clauses
                .push("(wb.name LIKE ? ESCAPE '\\' OR wb.description LIKE ? ESCAPE '\\')".into());
            let pat = esc_like(q);
            binds.push(Box::new(pat.clone()));
            binds.push(Box::new(pat));
        }
        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        // 总数(受 filter 约束)
        let count_sql = format!("SELECT COUNT(*) FROM wordbooks wb {where_sql}");
        let total: i64 = {
            let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
            conn.query_row(&count_sql, bind_refs.as_slice(), |r| r.get(0))?
        };

        // 排序
        let order_sql = match filter.sort.as_str() {
            "hottest" => "ORDER BY active_users DESC, wb.created_at DESC",
            "newest" => "ORDER BY wb.created_at DESC",
            // 默认:system 在前,再按 createdAt desc
            _ => "ORDER BY CASE WHEN wb.book_type = 'system' THEN 0 ELSE 1 END, wb.created_at DESC",
        };

        // active_users = 该书词上有 word_learning_states(total_attempts>0) 的 distinct user;
        // ownerEmail 仅 user 书 join users。
        let list_sql = format!(
            "SELECT
                wb.id, wb.name, wb.description, wb.book_type, wb.user_id,
                u.email AS owner_email,
                wb.word_count, wb.created_at,
                (SELECT COUNT(DISTINCT wls.user_id)
                 FROM wordbook_words ww
                 JOIN word_learning_states wls ON wls.word_id = ww.word_id
                 WHERE ww.wordbook_id = wb.id AND wls.total_attempts > 0
                ) AS active_users
             FROM wordbooks wb
             LEFT JOIN users u ON wb.book_type = 'user' AND u.id = wb.user_id
             {where_sql}
             {order_sql}
             LIMIT ? OFFSET ?"
        );
        binds.push(Box::new(filter.limit));
        binds.push(Box::new(filter.offset));
        let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&list_sql)?;
        let rows = stmt.query_map(bind_refs.as_slice(), |r| {
            Ok(WordbookSummaryRow {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                book_type: r.get(3)?,
                user_id: r.get(4)?,
                owner_email: r.get(5)?,
                word_count: r.get(6)?,
                created_at: r.get(7)?,
                active_users: r.get(8)?,
                tags: Vec::new(),
            })
        })?;
        let mut items: Vec<WordbookSummaryRow> = rows.collect::<Result<Vec<_>, _>>()?;

        // tags 逐书查 wordbook_local_tags(列表通常 <= 几十条)
        for it in items.iter_mut() {
            let mut tag_stmt = conn.prepare(
                "SELECT tag FROM wordbook_local_tags WHERE wordbook_id = ?1 ORDER BY tag ASC",
            )?;
            it.tags = tag_stmt
                .query_map(params![it.id], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
        }

        Ok((items, total, counts))
    }

    /// 单词库统计卡。avgMastery 跨所有用户;weeklyAnswers 由 route 传 cutoff。
    pub fn get_wordbook_admin_stats(
        &self,
        wordbook_id: &str,
        weekly_cutoff: DateTime<Utc>,
    ) -> Result<WordbookStats, StoreError> {
        keys::validate_id(wordbook_id)?;
        let conn = self.conn()?;

        let total_words: i64 = conn.query_row(
            "SELECT COUNT(*) FROM wordbook_words WHERE wordbook_id = ?1",
            params![wordbook_id],
            |r| r.get(0),
        )?;

        let active_users: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT wls.user_id)
             FROM wordbook_words ww
             JOIN word_learning_states wls ON wls.word_id = ww.word_id
             WHERE ww.wordbook_id = ?1 AND wls.total_attempts > 0",
            params![wordbook_id],
            |r| r.get(0),
        )?;

        let avg_mastery: Option<f64> = conn.query_row(
            "SELECT AVG(wls.mastery_level)
             FROM wordbook_words ww
             JOIN word_learning_states wls ON wls.word_id = ww.word_id
             WHERE ww.wordbook_id = ?1",
            params![wordbook_id],
            |r| r.get(0),
        )?;

        let cutoff = weekly_cutoff.to_rfc3339();
        let weekly_answers: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM learning_records lr
             WHERE lr.created_at >= ?2
               AND lr.word_id IN (SELECT word_id FROM wordbook_words WHERE wordbook_id = ?1)",
            params![wordbook_id, cutoff],
            |r| r.get(0),
        )?;

        Ok(WordbookStats {
            wordbook_id: wordbook_id.to_string(),
            total_words,
            active_users,
            avg_mastery: avg_mastery.unwrap_or(0.0),
            weekly_answers,
        })
    }

    /// 分页列出词条 + 跨用户 appearCount / accuracy。
    pub fn list_wordbook_word_entries(
        &self,
        wordbook_id: &str,
        filter: &WordEntryFilter,
    ) -> Result<(Vec<WordEntryRow>, i64), StoreError> {
        keys::validate_id(wordbook_id)?;
        let conn = self.conn()?;

        let mut where_clauses: Vec<String> = vec!["ww.wordbook_id = ?".into()];
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(wordbook_id.to_string())];
        if let Some(q) = filter.search.as_deref().filter(|s| !s.is_empty()) {
            where_clauses
                .push("(w.text LIKE ? ESCAPE '\\' OR w.meaning LIKE ? ESCAPE '\\')".into());
            let pat = esc_like(q);
            binds.push(Box::new(pat.clone()));
            binds.push(Box::new(pat));
        }
        if let Some(pos) = filter.pos.as_deref().filter(|s| !s.is_empty()) {
            where_clauses.push("w.part_of_speech = ?".into());
            binds.push(Box::new(pos.to_string()));
        }
        let where_sql = format!("WHERE {}", where_clauses.join(" AND "));

        let count_sql = format!(
            "SELECT COUNT(*) FROM wordbook_words ww JOIN words w ON w.id = ww.word_id {where_sql}"
        );
        let total: i64 = {
            let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
            conn.query_row(&count_sql, bind_refs.as_slice(), |r| r.get(0))?
        };

        let order_sql = match filter.sort.as_str() {
            "alpha" => "ORDER BY w.text ASC",
            "difficulty" => "ORDER BY w.difficulty DESC",
            // 默认 frequency
            _ => "ORDER BY appear_count DESC",
        };

        let list_sql = format!(
            "SELECT
                w.id, w.text, w.pronunciation, w.part_of_speech, w.meaning, w.examples_json,
                COALESCE(s.appear_count, 0) AS appear_count,
                s.accuracy AS accuracy
             FROM wordbook_words ww
             JOIN words w ON w.id = ww.word_id
             LEFT JOIN (
                SELECT word_id, COUNT(*) AS appear_count, AVG(is_correct) AS accuracy
                FROM learning_records GROUP BY word_id
             ) s ON s.word_id = w.id
             {where_sql}
             {order_sql}
             LIMIT ? OFFSET ?"
        );
        binds.push(Box::new(filter.limit));
        binds.push(Box::new(filter.offset));
        let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&list_sql)?;
        let rows = stmt.query_map(bind_refs.as_slice(), |r| {
            let examples_json: String = r.get(5)?;
            Ok(WordEntryRow {
                id: r.get(0)?,
                text: r.get(1)?,
                pronunciation: r.get(2)?,
                part_of_speech: r.get(3)?,
                meaning: r.get(4)?,
                examples: serde_json::from_str(&examples_json).unwrap_or_default(),
                appear_count: r.get(6)?,
                accuracy: r.get(7)?,
            })
        })?;
        let items = rows.collect::<Result<Vec<_>, _>>()?;
        Ok((items, total))
    }

    /// 热力图:本书词跨用户 appearCount 排序取前 limit。
    pub fn wordbook_heatmap(
        &self,
        wordbook_id: &str,
        limit: i64,
    ) -> Result<(i64, Vec<HeatmapCell>), StoreError> {
        keys::validate_id(wordbook_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT w.id, w.text, COALESCE(s.cnt, 0) AS count
             FROM wordbook_words ww
             JOIN words w ON w.id = ww.word_id
             LEFT JOIN (
                SELECT word_id, COUNT(*) AS cnt FROM learning_records GROUP BY word_id
             ) s ON s.word_id = w.id
             WHERE ww.wordbook_id = ?1
             ORDER BY count DESC
             LIMIT ?2",
        )?;
        let cells: Vec<HeatmapCell> = stmt
            .query_map(params![wordbook_id, limit], |r| {
                Ok(HeatmapCell {
                    word_id: r.get(0)?,
                    text: r.get(1)?,
                    count: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let max_count = cells.iter().map(|c| c.count).max().unwrap_or(0);
        Ok((max_count, cells))
    }

    /// 用户掌握度分桶:每个在本书词上有 wls(total_attempts>0) 的用户,取其在本书词上的
    /// AVG(mastery_level),Rust 端分 5 桶。
    pub fn wordbook_user_distribution(
        &self,
        wordbook_id: &str,
    ) -> Result<(i64, Vec<DistributionBucket>), StoreError> {
        keys::validate_id(wordbook_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT wls.user_id, AVG(wls.mastery_level) AS avg_m
             FROM wordbook_words ww
             JOIN word_learning_states wls ON wls.word_id = ww.word_id
             WHERE ww.wordbook_id = ?1 AND wls.total_attempts > 0
             GROUP BY wls.user_id",
        )?;
        let avgs: Vec<f64> = stmt
            .query_map(params![wordbook_id], |r| r.get::<_, f64>(1))?
            .collect::<Result<Vec<_>, _>>()?;

        let total_users = avgs.len() as i64;
        // 桶定义:[0,0.2) [0.2,0.4) [0.4,0.6) [0.6,0.8) [0.8,1.0]
        let edges: [(f64, f64); 5] = [(0.0, 0.2), (0.2, 0.4), (0.4, 0.6), (0.6, 0.8), (0.8, 1.0)];
        let labels = ["0-20%", "20-40%", "40-60%", "60-80%", "80-100%"];
        let mut counts = [0i64; 5];
        for m in &avgs {
            let idx = if *m >= 0.8 {
                4
            } else {
                ((m / 0.2).floor() as usize).min(4)
            };
            counts[idx] += 1;
        }
        let buckets = (0..5)
            .map(|i| DistributionBucket {
                label: labels[i].to_string(),
                min: edges[i].0,
                max: edges[i].1,
                user_count: counts[i],
            })
            .collect();
        Ok((total_users, buckets))
    }

    /// 插入一条词库审计(best-effort 由 route 调用)。
    pub fn insert_wordbook_audit(
        &self,
        wordbook_id: &str,
        action: &str,
        detail: &str,
        admin_id: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO wordbook_audit_log (id, wordbook_id, action, detail, admin_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                uuid::Uuid::new_v4().to_string(),
                wordbook_id,
                action,
                detail,
                admin_id,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// 分页列出某词库审计(created_at desc)。
    pub fn list_wordbook_audit(
        &self,
        wordbook_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<WordbookAuditEntry>, i64), StoreError> {
        keys::validate_id(wordbook_id)?;
        let conn = self.conn()?;
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM wordbook_audit_log WHERE wordbook_id = ?1",
            params![wordbook_id],
            |r| r.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT id, wordbook_id, action, detail, admin_id, created_at
             FROM wordbook_audit_log
             WHERE wordbook_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2 OFFSET ?3",
        )?;
        let entries: Vec<WordbookAuditEntry> = stmt
            .query_map(params![wordbook_id, limit, offset], |r| {
                Ok(WordbookAuditEntry {
                    id: r.get(0)?,
                    wordbook_id: r.get(1)?,
                    action: r.get(2)?,
                    detail: r.get(3)?,
                    admin_id: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((entries, total))
    }

    /// 事务删词库:wordbook + wordbook_words + wordbook_local_tags。返回是否存在。
    pub fn delete_wordbook_cascade(&self, wordbook_id: &str) -> Result<bool, StoreError> {
        keys::validate_id(wordbook_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let existed: bool = tx.query_row(
            "SELECT COUNT(*) FROM wordbooks WHERE id = ?1",
            params![wordbook_id],
            |r| r.get::<_, i64>(0),
        )? > 0;
        if existed {
            tx.execute(
                "DELETE FROM wordbook_words WHERE wordbook_id = ?1",
                params![wordbook_id],
            )?;
            tx.execute(
                "DELETE FROM wordbook_local_tags WHERE wordbook_id = ?1",
                params![wordbook_id],
            )?;
            tx.execute("DELETE FROM wordbooks WHERE id = ?1", params![wordbook_id])?;
        }
        tx.commit()?;
        Ok(existed)
    }

    /// 导出:Wordbook + 全部 Word(上限由 route 传 limit)。
    pub fn export_wordbook(
        &self,
        wordbook_id: &str,
        limit: i64,
    ) -> Result<Option<(Wordbook, Vec<Word>)>, StoreError> {
        keys::validate_id(wordbook_id)?;
        let Some(wb) = self.get_wordbook(wordbook_id)? else {
            return Ok(None);
        };
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT w.id, w.text, w.meaning, w.pronunciation, w.part_of_speech, w.difficulty,
                    w.examples_json, w.tags_json, w.created_at
             FROM wordbook_words ww
             JOIN words w ON w.id = ww.word_id
             WHERE ww.wordbook_id = ?1
             ORDER BY ww.added_at ASC
             LIMIT ?2",
        )?;
        let words: Vec<Word> = stmt
            .query_map(params![wordbook_id, limit], |r| {
                let examples_json: String = r.get(6)?;
                let tags_json: String = r.get(7)?;
                let created_at_str: String = r.get(8)?;
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            8,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Word {
                    id: r.get(0)?,
                    text: r.get(1)?,
                    meaning: r.get(2)?,
                    pronunciation: r.get(3)?,
                    part_of_speech: r.get(4)?,
                    difficulty: r.get(5)?,
                    examples: serde_json::from_str(&examples_json).unwrap_or_default(),
                    tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                    created_at,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some((wb, words)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::operations::records::{LearningRecord, RecordType};
    use crate::store::operations::word_states::{WordLearningState, WordState};
    use crate::store::operations::wordbooks::WordbookType;

    fn test_store() -> Store {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        store
    }

    fn mk_wordbook(store: &Store, id: &str, name: &str, t: WordbookType, user_id: Option<&str>) {
        store
            .upsert_wordbook(&Wordbook {
                id: id.into(),
                name: name.into(),
                description: String::new(),
                book_type: t,
                user_id: user_id.map(Into::into),
                word_count: 0,
                created_at: Utc::now(),
            })
            .unwrap();
    }

    fn mk_word(store: &Store, id: &str, text: &str, meaning: &str, pos: Option<&str>, diff: f64) {
        store
            .upsert_word(&Word {
                id: id.into(),
                text: text.into(),
                meaning: meaning.into(),
                pronunciation: None,
                part_of_speech: pos.map(Into::into),
                difficulty: diff,
                examples: vec!["ex".into()],
                tags: vec![],
                created_at: Utc::now(),
            })
            .unwrap();
    }

    fn mk_record(store: &Store, id: &str, user: &str, word: &str, correct: bool) {
        store
            .create_record(&LearningRecord {
                id: id.into(),
                user_id: user.into(),
                word_id: word.into(),
                is_correct: correct,
                response_time_ms: 100,
                session_id: None,
                created_at: Utc::now(),
                record_type: RecordType::All,
                self_rating: None,
                question_mode: None,
            })
            .unwrap();
    }

    fn mk_state(store: &Store, user: &str, word: &str, mastery: f64, attempts: u32) {
        store
            .set_word_learning_state(&WordLearningState {
                user_id: user.into(),
                word_id: word.into(),
                state: WordState::Learning,
                mastery_level: mastery,
                next_review_date: None,
                half_life: 24.0,
                correct_streak: 0,
                total_attempts: attempts,
                updated_at: Utc::now(),
            })
            .unwrap();
    }

    fn default_list_filter() -> WordbookListFilter {
        WordbookListFilter {
            type_filter: "all".into(),
            search: None,
            sort: "default".into(),
            limit: 50,
            offset: 0,
        }
    }

    #[test]
    fn list_counts_and_active_users() {
        let store = test_store();
        mk_wordbook(&store, "sys1", "System Book", WordbookType::System, None);
        mk_wordbook(&store, "usr1", "User Book", WordbookType::User, Some("u1"));
        mk_word(&store, "w1", "apple", "苹果", Some("noun"), 0.3);
        mk_word(&store, "w2", "banana", "香蕉", Some("noun"), 0.7);
        store.add_word_to_wordbook("sys1", "w1").unwrap();
        store.add_word_to_wordbook("sys1", "w2").unwrap();
        store.add_word_to_wordbook("usr1", "w1").unwrap();
        // 两个用户在 w1 上有 attempts>0
        mk_state(&store, "ua", "w1", 0.5, 3);
        mk_state(&store, "ub", "w1", 0.9, 2);
        // attempts=0 不算 active
        mk_state(&store, "uc", "w2", 0.1, 0);

        let (items, total, counts) = store.list_all_wordbooks(&default_list_filter()).unwrap();
        assert_eq!(total, 2);
        assert_eq!(counts.all, 2);
        assert_eq!(counts.system, 1);
        assert_eq!(counts.user, 1);
        assert_eq!(counts.total_entries, 3); // sys1=2 + usr1=1

        // 默认排序:system 在前
        assert_eq!(items[0].id, "sys1");
        let sys = &items[0];
        assert_eq!(sys.active_users, 2);
        assert_eq!(sys.word_count, 2);
    }

    #[test]
    fn list_type_filter_and_search() {
        let store = test_store();
        mk_wordbook(&store, "sys1", "GRE Core", WordbookType::System, None);
        mk_wordbook(&store, "usr1", "My Words", WordbookType::User, Some("u1"));

        let mut f = default_list_filter();
        f.type_filter = "system".into();
        let (items, total, _) = store.list_all_wordbooks(&f).unwrap();
        assert_eq!(total, 1);
        assert_eq!(items[0].id, "sys1");

        let mut f2 = default_list_filter();
        f2.search = Some("gre".into()); // 大小写不敏感
        let (items2, total2, _) = store.list_all_wordbooks(&f2).unwrap();
        assert_eq!(total2, 1);
        assert_eq!(items2[0].id, "sys1");
    }

    #[test]
    fn stats_avg_mastery_and_weekly() {
        let store = test_store();
        mk_wordbook(&store, "sys1", "B", WordbookType::System, None);
        mk_word(&store, "w1", "a", "释义", None, 0.5);
        mk_word(&store, "w2", "b", "释义", None, 0.5);
        store.add_word_to_wordbook("sys1", "w1").unwrap();
        store.add_word_to_wordbook("sys1", "w2").unwrap();
        mk_state(&store, "u1", "w1", 0.4, 2);
        mk_state(&store, "u1", "w2", 0.8, 2);
        mk_record(&store, "r1", "u1", "w1", true);
        mk_record(&store, "r2", "u1", "w2", false);

        let cutoff = Utc::now() - chrono::Duration::days(7);
        let stats = store.get_wordbook_admin_stats("sys1", cutoff).unwrap();
        assert_eq!(stats.total_words, 2);
        assert_eq!(stats.active_users, 1);
        assert!((stats.avg_mastery - 0.6).abs() < 1e-9);
        assert_eq!(stats.weekly_answers, 2);
    }

    #[test]
    fn word_entries_sort_and_search() {
        let store = test_store();
        mk_wordbook(&store, "sys1", "B", WordbookType::System, None);
        mk_word(&store, "w1", "alpha", "第一", Some("noun"), 0.2);
        mk_word(&store, "w2", "beta", "第二", Some("verb"), 0.9);
        store.add_word_to_wordbook("sys1", "w1").unwrap();
        store.add_word_to_wordbook("sys1", "w2").unwrap();
        // w2 出现 3 次(2对1错),w1 出现 1 次
        mk_record(&store, "r1", "u1", "w2", true);
        mk_record(&store, "r2", "u1", "w2", true);
        mk_record(&store, "r3", "u1", "w2", false);
        mk_record(&store, "r4", "u1", "w1", true);

        let freq = WordEntryFilter {
            search: None,
            pos: None,
            sort: "frequency".into(),
            limit: 50,
            offset: 0,
        };
        let (rows, total) = store.list_wordbook_word_entries("sys1", &freq).unwrap();
        assert_eq!(total, 2);
        assert_eq!(rows[0].id, "w2"); // 高频在前
        assert_eq!(rows[0].appear_count, 3);
        assert!((rows[0].accuracy.unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(rows[1].appear_count, 1);

        // alpha 排序
        let mut alpha = freq.clone();
        alpha.sort = "alpha".into();
        let (rows_a, _) = store.list_wordbook_word_entries("sys1", &alpha).unwrap();
        assert_eq!(rows_a[0].id, "w1");

        // pos 过滤
        let mut posf = freq.clone();
        posf.pos = Some("verb".into());
        let (rows_p, total_p) = store.list_wordbook_word_entries("sys1", &posf).unwrap();
        assert_eq!(total_p, 1);
        assert_eq!(rows_p[0].id, "w2");

        // search 命中 meaning
        let mut sf = freq.clone();
        sf.search = Some("第一".into());
        let (rows_s, total_s) = store.list_wordbook_word_entries("sys1", &sf).unwrap();
        assert_eq!(total_s, 1);
        assert_eq!(rows_s[0].id, "w1");
    }

    #[test]
    fn heatmap_and_distribution() {
        let store = test_store();
        mk_wordbook(&store, "sys1", "B", WordbookType::System, None);
        mk_word(&store, "w1", "a", "释义", None, 0.5);
        mk_word(&store, "w2", "b", "释义", None, 0.5);
        store.add_word_to_wordbook("sys1", "w1").unwrap();
        store.add_word_to_wordbook("sys1", "w2").unwrap();
        mk_record(&store, "r1", "u1", "w1", true);
        mk_record(&store, "r2", "u1", "w1", true);
        mk_record(&store, "r3", "u1", "w2", true);
        // 分桶:u1 avg=0.1 → 桶0;u2 avg=0.85 → 桶4
        mk_state(&store, "u1", "w1", 0.1, 2);
        mk_state(&store, "u2", "w2", 0.85, 2);

        let (max_count, cells) = store.wordbook_heatmap("sys1", 600).unwrap();
        assert_eq!(max_count, 2);
        assert_eq!(cells[0].word_id, "w1");
        assert_eq!(cells[0].count, 2);

        let (total_users, buckets) = store.wordbook_user_distribution("sys1").unwrap();
        assert_eq!(total_users, 2);
        assert_eq!(buckets[0].user_count, 1); // 0-20%
        assert_eq!(buckets[4].user_count, 1); // 80-100%
    }

    #[test]
    fn audit_insert_list_and_delete_cascade() {
        let store = test_store();
        mk_wordbook(&store, "sys1", "B", WordbookType::System, None);
        mk_word(&store, "w1", "a", "释义", None, 0.5);
        store.add_word_to_wordbook("sys1", "w1").unwrap();
        store
            .add_wordbook_local_tags("sys1", &["tag1".into()], Some("admin"))
            .unwrap();

        store
            .insert_wordbook_audit("sys1", "create", "B", Some("admin"))
            .unwrap();
        store
            .insert_wordbook_audit("sys1", "add_word", "w1", Some("admin"))
            .unwrap();
        let (entries, total) = store.list_wordbook_audit("sys1", 50, 0).unwrap();
        assert_eq!(total, 2);
        // created_at desc:最近的 add_word 在前(同毫秒时不保证,但 action 都在集合里)
        assert!(entries.iter().any(|e| e.action == "create"));
        assert!(entries.iter().any(|e| e.action == "add_word"));

        // export
        let exported = store.export_wordbook("sys1", 50000).unwrap().unwrap();
        assert_eq!(exported.0.id, "sys1");
        assert_eq!(exported.1.len(), 1);
        assert_eq!(exported.1[0].id, "w1");

        // cascade delete
        assert!(store.delete_wordbook_cascade("sys1").unwrap());
        assert!(store.get_wordbook("sys1").unwrap().is_none());
        assert!(store.list_wordbook_local_tags("sys1").unwrap().is_empty());
        assert_eq!(store.count_wordbook_words("sys1").unwrap(), 0);
        // 不存在返回 false
        assert!(!store.delete_wordbook_cascade("sys1").unwrap());
    }
}
