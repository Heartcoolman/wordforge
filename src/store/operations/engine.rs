use rusqlite::{params, OptionalExtension};

use crate::store::keys;
use crate::store::{Store, StoreError};

impl Store {
    pub fn get_engine_user_state(
        &self,
        user_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let json: Option<String> = conn
            .query_row(
                "SELECT state_json FROM engine_user_states WHERE user_id=?1",
                params![user_id],
                |r| r.get(0),
            )
            .optional()?;
        match json {
            Some(s) => Ok(Some(Self::deserialize_json(&s)?)),
            None => Ok(None),
        }
    }

    pub fn set_engine_user_state(
        &self,
        user_id: &str,
        state: &serde_json::Value,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let json = Self::serialize_json(state)?;
        let created_at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO engine_user_states (user_id, state_json, created_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id) DO UPDATE SET state_json=?2",
            params![user_id, json, created_at],
        )?;
        Ok(())
    }

    pub fn delete_engine_user_state(&self, user_id: &str) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM engine_user_states WHERE user_id=?1",
            params![user_id],
        )?;
        Ok(())
    }

    pub fn get_engine_algo_state(
        &self,
        user_id: &str,
        algo_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let json: Option<String> = conn
            .query_row(
                "SELECT state_json FROM engine_algo_states WHERE user_id=?1 AND algo_id=?2",
                params![user_id, algo_id],
                |r| r.get(0),
            )
            .optional()?;
        match json {
            Some(s) => Ok(Some(Self::deserialize_json(&s)?)),
            None => Ok(None),
        }
    }

    pub fn set_engine_algo_state(
        &self,
        user_id: &str,
        algo_id: &str,
        state: &serde_json::Value,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let json = Self::serialize_json(state)?;
        conn.execute(
            "INSERT INTO engine_algo_states (user_id, algo_id, state_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id, algo_id) DO UPDATE SET state_json=?3",
            params![user_id, algo_id, json],
        )?;
        Ok(())
    }

    pub fn delete_engine_algo_state(&self, user_id: &str, algo_id: &str) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM engine_algo_states WHERE user_id=?1 AND algo_id=?2",
            params![user_id, algo_id],
        )?;
        Ok(())
    }

    /// 把 MonitoringEvent（camelCase JSON）拆进 engine_monitoring_events 的全部专用列。
    /// 历史上此处只写 6 列、14 个专用列恒为 DEFAULT，导致 version/anomaly/user-state 聚合读零；
    /// 现按列写入让 SQL GROUP BY/AVG 可用，同时保留 strategy_json/reward_json 整坨 blob 供
    /// get_recent_monitoring_events 与 admin 详情按原方式读取（向后兼容）。
    pub fn insert_monitoring_event(&self, event: &serde_json::Value) -> Result<(), StoreError> {
        let get_str = |keys: &[&str]| -> String {
            for k in keys {
                if let Some(s) = event.get(*k).and_then(|v| v.as_str()) {
                    return s.to_string();
                }
            }
            String::new()
        };
        let id = event
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let user_id = get_str(&["userId", "user_id"]);
        let session_id = get_str(&["sessionId", "session_id"]);
        let timestamp = get_str(&["timestamp"]);
        let event_type = {
            let t = get_str(&["eventType", "event_type"]);
            if t.is_empty() {
                "process_event".to_string()
            } else {
                t
            }
        };
        let latency_ms = event.get("latencyMs").and_then(|v| v.as_i64()).unwrap_or(0);
        let is_anomaly = event
            .get("isAnomaly")
            .and_then(|v| v.as_bool())
            .unwrap_or(false) as i64;
        let invariant_violations_json = event
            .get("invariantViolations")
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()))
            .unwrap_or_else(|| "[]".to_string());
        let us = event.get("userState");
        let us_f = |key: &str, dflt: f64| -> f64 {
            us.and_then(|u| u.get(key))
                .and_then(|v| v.as_f64())
                .unwrap_or(dflt)
        };
        let us_i = |key: &str| -> i64 {
            us.and_then(|u| u.get(key))
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
        };
        let attention = us_f("attention", 0.7);
        let fatigue = us_f("fatigue", 0.0);
        let motivation = us_f("motivation", 0.0);
        let confidence = us_f("confidence", 0.1);
        let session_event_count = us_i("sessionEventCount");
        let total_event_count = us_i("totalEventCount");
        let cold_start_phase: Option<String> = event
            .get("coldStartPhase")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let selection_constraints_met = event
            .get("selectionConstraintsMet")
            .and_then(|v| v.as_bool())
            .unwrap_or(false) as i64;
        let reward_value = event
            .get("rewardValue")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let config_version = get_str(&["configVersion", "config_version"]);
        let routing_algo = get_str(&["routingAlgo", "routing_algo"]);
        let routing_weights_json = event
            .get("routingWeights")
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string());
        let is_correct = event
            .get("isCorrect")
            .and_then(|v| v.as_bool())
            .unwrap_or(false) as i64;
        let json = Self::serialize_json(event)?;

        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO engine_monitoring_events (
                id, user_id, session_id, event_type, timestamp, latency_ms, is_anomaly,
                invariant_violations_json, user_state_attention, user_state_fatigue,
                user_state_motivation, user_state_confidence, user_state_session_event_count,
                user_state_total_event_count, strategy_json, reward_json, cold_start_phase,
                selection_constraints_met, reward_value, config_version,
                routing_algo, routing_weights_json, is_correct
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16,
                ?17, ?18, ?19, ?20, ?21, ?22
             )",
            params![
                id,
                user_id,
                session_id,
                event_type,
                timestamp,
                latency_ms,
                is_anomaly,
                invariant_violations_json,
                attention,
                fatigue,
                motivation,
                confidence,
                session_event_count,
                total_event_count,
                json,
                cold_start_phase,
                selection_constraints_met,
                reward_value,
                config_version,
                routing_algo,
                routing_weights_json,
                is_correct
            ],
        )?;
        Ok(())
    }

    pub fn get_recent_monitoring_events(
        &self,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT strategy_json FROM engine_monitoring_events ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| r.get::<_, String>(0))?;
        let mut events = Vec::new();
        for row in rows {
            let json = row?;
            events.push(Self::deserialize_json(&json)?);
        }
        Ok(events)
    }

    pub fn upsert_metrics_daily(
        &self,
        date: &str,
        algo_id: &str,
        metrics: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let json = Self::serialize_json(metrics)?;
        conn.execute(
            "INSERT INTO algorithm_metrics_daily (metric_date, algorithm_id, metrics_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(metric_date, algorithm_id) DO UPDATE SET metrics_json=?3",
            params![date, algo_id, json],
        )?;
        Ok(())
    }

    pub fn batch_upsert_metrics_daily(
        &self,
        entries: &[(String, serde_json::Value)],
    ) -> Result<(), StoreError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for (key, value) in entries {
            let parts: Vec<&str> = key.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue;
            }
            let json = Self::serialize_json(value)?;
            tx.execute(
                "INSERT INTO algorithm_metrics_daily (metric_date, algorithm_id, metrics_json)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(metric_date, algorithm_id) DO UPDATE SET metrics_json=?3",
                params![parts[0], parts[1], json],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_metrics_daily(
        &self,
        date: &str,
        algo_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let json: Option<String> = conn
            .query_row(
                "SELECT metrics_json FROM algorithm_metrics_daily WHERE metric_date=?1 AND algorithm_id=?2",
                params![date, algo_id],
                |r| r.get(0),
            )
            .optional()?;
        match json {
            Some(s) => Ok(Some(Self::deserialize_json(&s)?)),
            None => Ok(None),
        }
    }

    /// 原子写入 AMAS 状态（user_state + algo_states），可选附幂等标记。
    ///
    /// 返回值（仅在 `idempotency` 为 `Some` 时有意义）：`true`=标记本次新插入、AMAS 状态已提交；
    /// `false`=标记已存在（并发竞态下另一请求先行处理），**整笔 tx 回滚**、本次 AMAS 增量被丢弃。
    /// `idempotency` 为 `None` 时恒返回 `true`（无标记可竞争）。
    ///
    /// W1-1 并发收口：`INSERT OR IGNORE` 标记的 affected rows 是权威仲裁——锁外预检 + 持锁应用之间
    /// 仍可能两个同 `client_record_id` 请求各自通过预检，此处以"谁先写进标记谁生效、后者整笔回滚"
    /// 消除二次累加 ELO/mastery/trust 的窗口。
    pub fn persist_engine_state_atomic(
        &self,
        user_id: &str,
        user_state: &serde_json::Value,
        algo_states: &[(String, serde_json::Value)],
        idempotency: Option<&str>,
    ) -> Result<bool, StoreError> {
        keys::validate_id(user_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let user_json = Self::serialize_json(user_state)?;
        let created_at = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO engine_user_states (user_id, state_json, created_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id) DO UPDATE SET state_json=?2",
            params![user_id, user_json, created_at],
        )?;
        for (algo_id, value) in algo_states {
            let json = Self::serialize_json(value)?;
            tx.execute(
                "INSERT INTO engine_algo_states (user_id, algo_id, state_json)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(user_id, algo_id) DO UPDATE SET state_json=?3",
                params![user_id, algo_id, json],
            )?;
        }
        // W1-1：幂等标记与 AMAS 状态同 tx 原子提交,保证"标记存在 ⟺ AMAS 已应用"。
        // affected rows==0 即标记已被并发请求抢先写入 → 回滚本次重复增量。
        if let Some(client_record_id) = idempotency {
            let inserted = tx.execute(
                "INSERT OR IGNORE INTO processed_events (user_id, client_record_id, processed_at)
                 VALUES (?1, ?2, ?3)",
                params![user_id, client_record_id, created_at],
            )?;
            if inserted == 0 {
                tx.rollback()?;
                return Ok(false);
            }
        }
        tx.commit()?;
        Ok(true)
    }

    /// W1-1：一次性**原子**回滚引擎状态 + 清除幂等标记。
    ///
    /// 写侧 [`Self::persist_engine_state_atomic`] 把 AMAS 状态 + 标记同 tx 原子写入；回滚侧也必须
    /// 把「恢复 AMAS 状态」与「删除标记」放进同一 tx，否则进程在两者之间崩溃会留下「标记在、AMAS
    /// 已回滚」的悬置态——重放命中标记走裸记录路径跳过 AMAS，永久丢该事件 AMAS 贡献，破坏
    /// 「标记存在 ⟺ AMAS 已应用」不变式。本方法把 user_state / algo_states / ELO / 标记清除全部
    /// 收进一个 tx，崩溃要么全回滚（标记已清），要么全不动（标记在、AMAS 仍在），两种结局均守不变式。
    pub fn restore_engine_state_atomic(&self, r: &EngineStateRestore) -> Result<(), StoreError> {
        keys::validate_id(r.user_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        // user_state：Some(inner) 按 inner set/delete；外层 None 不动（batch 路径不碰 user_state）。
        if let Some(state_opt) = r.user_state {
            match state_opt {
                Some(v) => {
                    let json = Self::serialize_json(v)?;
                    let created_at = chrono::Utc::now().to_rfc3339();
                    tx.execute(
                        "INSERT INTO engine_user_states (user_id, state_json, created_at)
                         VALUES (?1, ?2, ?3)
                         ON CONFLICT(user_id) DO UPDATE SET state_json=?2",
                        params![r.user_id, json, created_at],
                    )?;
                }
                None => {
                    tx.execute(
                        "DELETE FROM engine_user_states WHERE user_id=?1",
                        params![r.user_id],
                    )?;
                }
            }
        }
        for (algo_id, val) in r.algo_states {
            match val {
                Some(v) => {
                    let json = Self::serialize_json(v)?;
                    tx.execute(
                        "INSERT INTO engine_algo_states (user_id, algo_id, state_json)
                         VALUES (?1, ?2, ?3)
                         ON CONFLICT(user_id, algo_id) DO UPDATE SET state_json=?3",
                        params![r.user_id, algo_id, json],
                    )?;
                }
                None => {
                    tx.execute(
                        "DELETE FROM engine_algo_states WHERE user_id=?1 AND algo_id=?2",
                        params![r.user_id, algo_id],
                    )?;
                }
            }
        }
        if let Some(elo) = r.user_elo {
            tx.execute(
                "INSERT INTO user_elo (user_id, rating, games) VALUES (?1, ?2, ?3)
                 ON CONFLICT(user_id) DO UPDATE SET rating=?2, games=?3",
                params![r.user_id, elo.rating, elo.games],
            )?;
        }
        if let Some((word_id, elo)) = r.word_elo {
            tx.execute(
                "INSERT INTO word_elo (word_id, rating, games) VALUES (?1, ?2, ?3)
                 ON CONFLICT(word_id) DO UPDATE SET rating=?2, games=?3",
                params![word_id, elo.rating, elo.games],
            )?;
        }
        if let Some(rec_id) = r.clear_marker_record_id {
            tx.execute(
                "DELETE FROM processed_events WHERE user_id=?1 AND client_record_id=?2",
                params![r.user_id, rec_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// W1-1：[`Store::restore_engine_state_atomic`] 的入参。各字段语义见该方法文档。
pub struct EngineStateRestore<'a> {
    pub user_id: &'a str,
    /// 外层 None=不动 user_state；Some(&Some(v))=恢复为 v；Some(&None)=删除该行。
    pub user_state: Option<&'a Option<serde_json::Value>>,
    /// 每项 (algo_id, Some=恢复 / None=删除)。
    pub algo_states: &'a [(&'a str, &'a Option<serde_json::Value>)],
    pub user_elo: Option<&'a crate::amas::elo::EloRating>,
    pub word_elo: Option<(&'a str, &'a crate::amas::elo::EloRating)>,
    /// Some(client_record_id)=同 tx 删除 processed_events 标记。
    pub clear_marker_record_id: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use crate::store::Store;

    #[test]
    fn save_and_load_engine_state() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        let state = serde_json::json!({"attention": 0.7});
        store.set_engine_user_state("u1", &state).unwrap();
        let got = store.get_engine_user_state("u1").unwrap().unwrap();
        assert_eq!(got["attention"], 0.7);
    }

    #[test]
    fn algo_state_round_trip() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        let state = serde_json::json!({"level": 0.8});
        store
            .set_engine_algo_state("u1", "mastery:w1", &state)
            .unwrap();
        let got = store
            .get_engine_algo_state("u1", "mastery:w1")
            .unwrap()
            .unwrap();
        assert_eq!(got["level"], 0.8);
    }

    fn tempfile_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    #[test]
    fn get_engine_user_state_missing_returns_none() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        assert!(store.get_engine_user_state("u1").unwrap().is_none());
        assert!(store.get_engine_algo_state("u1", "x").unwrap().is_none());
    }

    #[test]
    fn delete_user_and_algo_state_idempotent() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store
            .set_engine_user_state("u1", &serde_json::json!({}))
            .unwrap();
        store.delete_engine_user_state("u1").unwrap();
        assert!(store.get_engine_user_state("u1").unwrap().is_none());
        store.delete_engine_user_state("u1").unwrap(); // 再删不报错

        store
            .set_engine_algo_state("u1", "a", &serde_json::json!({}))
            .unwrap();
        store.delete_engine_algo_state("u1", "a").unwrap();
        assert!(store.get_engine_algo_state("u1", "a").unwrap().is_none());
        store.delete_engine_algo_state("u1", "a").unwrap();
    }

    #[test]
    fn engine_state_upsert_overwrites_existing() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store
            .set_engine_user_state("u1", &serde_json::json!({"v":1}))
            .unwrap();
        store
            .set_engine_user_state("u1", &serde_json::json!({"v":2}))
            .unwrap();
        let got = store.get_engine_user_state("u1").unwrap().unwrap();
        assert_eq!(got["v"], 2);
    }

    #[test]
    fn engine_validation_rejects_bad_id() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        assert!(matches!(
            store.get_engine_user_state("").unwrap_err(),
            crate::store::StoreError::Validation(_)
        ));
        assert!(matches!(
            store
                .set_engine_user_state("", &serde_json::json!({}))
                .unwrap_err(),
            crate::store::StoreError::Validation(_)
        ));
        assert!(matches!(
            store.delete_engine_user_state("").unwrap_err(),
            crate::store::StoreError::Validation(_)
        ));
        assert!(matches!(
            store
                .set_engine_algo_state("", "a", &serde_json::json!({}))
                .unwrap_err(),
            crate::store::StoreError::Validation(_)
        ));
        assert!(matches!(
            store.delete_engine_algo_state("", "a").unwrap_err(),
            crate::store::StoreError::Validation(_)
        ));
    }

    #[test]
    fn monitoring_event_insert_and_recent_query() {
        let (_t, store) = tempfile_store();
        let evt1 = serde_json::json!({
            "id":"e1","userId":"u1","sessionId":"s1","timestamp":"2026-05-01T12:00:00Z","strategy":{"a":1}
        });
        let evt2 = serde_json::json!({
            "user_id":"u1","session_id":"s2","timestamp":"2026-05-02T12:00:00Z","strategy":{"a":2}
        });
        store.insert_monitoring_event(&evt1).unwrap();
        store.insert_monitoring_event(&evt2).unwrap();
        let recent = store.get_recent_monitoring_events(10).unwrap();
        assert_eq!(recent.len(), 2);
        // 最新在前
        assert_eq!(
            recent[0]["timestamp"],
            serde_json::json!("2026-05-02T12:00:00Z")
        );
    }

    #[test]
    fn monitoring_event_populates_typed_columns() {
        // PR-0 地基回归：insert_monitoring_event 必须把 camelCase 事件 JSON 拆进专用列，
        // 否则 version/anomaly/user-state 聚合恒读 DEFAULT。校验 23 列占位符映射无错位。
        let (_t, store) = tempfile_store();
        let evt = serde_json::json!({
            "id": "e1", "userId": "u1", "sessionId": "s1", "eventType": "process_event",
            "timestamp": "2026-05-29T10:00:00Z", "latencyMs": 42, "isAnomaly": true,
            "invariantViolations": [{"field":"fatigue","value":1.2,"expectedRange":"[0,1]"}],
            "userState": {
                "attention": 0.55, "fatigue": 0.8, "motivation": 0.3,
                "confidence": 0.6, "sessionEventCount": 7, "totalEventCount": 321
            },
            "coldStartPhase": "Explore", "selectionConstraintsMet": true, "rewardValue": 0.77,
            "configVersion": "abc123", "routingAlgo": "ensemble",
            "routingWeights": {"ensemble": 0.6, "mdm": 0.4}, "isCorrect": true
        });
        store.insert_monitoring_event(&evt).unwrap();

        let conn = store.conn().unwrap();
        #[allow(clippy::type_complexity)]
        let (lat, fatigue, attention, anomaly, cold, scm, rv, cv, algo, rw, ic, sec, tec): (
            i64, f64, f64, i64, Option<String>, i64, f64, String, String, String, i64, i64, i64,
        ) = conn
            .query_row(
                "SELECT latency_ms, user_state_fatigue, user_state_attention, is_anomaly,
                        cold_start_phase, selection_constraints_met, reward_value, config_version,
                        routing_algo, routing_weights_json, is_correct,
                        user_state_session_event_count, user_state_total_event_count
                 FROM engine_monitoring_events WHERE id='e1'",
                [],
                |r| {
                    Ok((
                        r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?,
                        r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?,
                        r.get(12)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(lat, 42);
        assert_eq!(fatigue, 0.8);
        assert_eq!(attention, 0.55);
        assert_eq!(anomaly, 1);
        assert_eq!(cold.as_deref(), Some("Explore"));
        assert_eq!(scm, 1);
        assert!((rv - 0.77).abs() < 1e-9);
        assert_eq!(cv, "abc123");
        assert_eq!(algo, "ensemble");
        assert!(rw.contains("ensemble") && rw.contains("mdm"));
        assert_eq!(ic, 1);
        assert_eq!(sec, 7);
        assert_eq!(tec, 321);
        // 整坨 blob 仍可经 strategy_json 回读（向后兼容）
        let recent = store.get_recent_monitoring_events(1).unwrap();
        assert_eq!(recent[0]["isCorrect"], serde_json::json!(true));
    }

    #[test]
    fn metrics_daily_upsert_and_batch() {
        let (_t, store) = tempfile_store();
        store
            .upsert_metrics_daily("2026-05-01", "algo-a", &serde_json::json!({"x":1}))
            .unwrap();
        store
            .upsert_metrics_daily("2026-05-01", "algo-a", &serde_json::json!({"x":2}))
            .unwrap();
        let m = store
            .get_metrics_daily("2026-05-01", "algo-a")
            .unwrap()
            .unwrap();
        assert_eq!(m["x"], 2);

        // batch 形式
        store
            .batch_upsert_metrics_daily(&[
                ("2026-05-02:algo-b".into(), serde_json::json!({"y":3})),
                ("invalid-key-no-colon".into(), serde_json::json!({})), // 应被忽略
            ])
            .unwrap();
        let b = store
            .get_metrics_daily("2026-05-02", "algo-b")
            .unwrap()
            .unwrap();
        assert_eq!(b["y"], 3);
        assert!(store
            .get_metrics_daily("invalid-key-no-colon", "")
            .unwrap()
            .is_none());
    }

    #[test]
    fn persist_engine_state_atomic_writes_user_and_algo_states() {
        let (_t, store) = tempfile_store();
        let user_state = serde_json::json!({"attention":0.5});
        let algo = vec![
            ("a1".to_string(), serde_json::json!({"l":1})),
            ("a2".to_string(), serde_json::json!({"l":2})),
        ];
        store
            .persist_engine_state_atomic("u1", &user_state, &algo, None)
            .unwrap();
        let us = store.get_engine_user_state("u1").unwrap().unwrap();
        assert_eq!(us["attention"], 0.5);
        let a1 = store.get_engine_algo_state("u1", "a1").unwrap().unwrap();
        assert_eq!(a1["l"], 1);
        let a2 = store.get_engine_algo_state("u1", "a2").unwrap().unwrap();
        assert_eq!(a2["l"], 2);

        // 二次 atomic upsert 替换
        let algo2 = vec![("a1".into(), serde_json::json!({"l":99}))];
        store
            .persist_engine_state_atomic("u1", &serde_json::json!({"attention":0.9}), &algo2, None)
            .unwrap();
        assert_eq!(
            store.get_engine_algo_state("u1", "a1").unwrap().unwrap()["l"],
            99
        );

        // 错误 ID
        assert!(matches!(
            store
                .persist_engine_state_atomic("", &user_state, &[], None)
                .unwrap_err(),
            crate::store::StoreError::Validation(_)
        ));
    }

    /// W1-1：原子回滚 + 清标记守不变式。验证 restore_engine_state_atomic 在一个 tx 内
    /// 同时恢复 AMAS 状态（含删除）与清除 processed_events 标记。
    #[test]
    fn restore_engine_state_atomic_rolls_back_and_clears_marker() {
        use crate::store::operations::engine::EngineStateRestore;
        // 需 processed_events 表（m045），故跑全量迁移而非裸 tempfile_store。
        let _t = tempfile::tempdir().unwrap();
        let store = Store::open(_t.path().join("t.db").to_str().unwrap(), 5000, 2).unwrap();
        store.run_migrations().unwrap();

        // 写侧：AMAS 状态 + 标记原子写入（模拟 process_event_idempotent）。
        let algo = vec![("mastery:w1".to_string(), serde_json::json!({"m": 0.9}))];
        store
            .persist_engine_state_atomic(
                "u1",
                &serde_json::json!({"attention": 0.9}),
                &algo,
                Some("rec-1"),
            )
            .unwrap();
        assert!(store.is_event_processed("u1", "rec-1").unwrap());
        assert!(store.get_engine_user_state("u1").unwrap().is_some());

        // 回滚侧：user_state 删除（pre-event 为无）、mastery 删除、清标记——全在一个 tx。
        let elo = crate::amas::elo::EloRating::default();
        let none_val: Option<serde_json::Value> = None;
        store
            .restore_engine_state_atomic(&EngineStateRestore {
                user_id: "u1",
                user_state: Some(&none_val),
                algo_states: &[("mastery:w1", &none_val)],
                user_elo: Some(&elo),
                word_elo: Some(("w1", &elo)),
                clear_marker_record_id: Some("rec-1"),
            })
            .unwrap();

        // 不变式：标记已清 ⟺ AMAS 已回滚（user_state 与 mastery 均删除）。
        assert!(
            !store.is_event_processed("u1", "rec-1").unwrap(),
            "回滚后标记应被同 tx 清除"
        );
        assert!(store.get_engine_user_state("u1").unwrap().is_none());
        assert!(store
            .get_engine_algo_state("u1", "mastery:w1")
            .unwrap()
            .is_none());
    }
}
