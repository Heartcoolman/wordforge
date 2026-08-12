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

    /// 把 UserState JSON(camelCase)投影到 engine_user_states 标量列并 upsert（全部写路径共用，
    /// 防列集漂移）。state_json 是引擎真值,但 get_data_upload_status / amas_dashboard /
    /// user-state 均值 / 认知分布 / stage 7d 留存等按标量列查询;每次落库须把列与 JSON 同步,
    /// 否则列恒为 schema DEFAULT → 计数误判 nil(m053 前科)、看板均值/分布恒常数、留存恒 0。
    /// 缺键/坏值回落列 DEFAULT,不覆盖为 0。
    fn upsert_engine_user_state_conn(
        conn: &rusqlite::Connection,
        user_id: &str,
        state: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let json = Self::serialize_json(state)?;
        // 计数为非负整数;兼容 JSON 整数与浮点表示(如 5.0),并 clamp 到 [0, i64::MAX],
        // 避免 as_i64() 对浮点/超界值返回 None 而静默退化为 0、覆盖标量列误判 nil。
        let count = |key: &str| -> i64 {
            state
                .get(key)
                .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
                .unwrap_or(0)
                .max(0)
        };
        let num = |path: &[&str], default: f64| -> f64 {
            let mut cur = state;
            for key in path {
                match cur.get(key) {
                    Some(v) => cur = v,
                    None => return default,
                }
            }
            cur.as_f64().unwrap_or(default)
        };
        let last_active_at = state.get("lastActiveAt").and_then(|v| v.as_str());
        let created_at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO engine_user_states
               (user_id, state_json, total_event_count, session_event_count, created_at,
                attention, fatigue, motivation, confidence, last_active_at,
                cognitive_memory_capacity, cognitive_processing_speed, cognitive_stability)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(user_id) DO UPDATE SET
               state_json=?2, total_event_count=?3, session_event_count=?4,
               attention=?6, fatigue=?7, motivation=?8, confidence=?9, last_active_at=?10,
               cognitive_memory_capacity=?11, cognitive_processing_speed=?12, cognitive_stability=?13",
            params![
                user_id,
                json,
                count("totalEventCount"),
                count("sessionEventCount"),
                created_at,
                num(&["attention"], 0.7),
                num(&["fatigue"], 0.0),
                num(&["motivation"], 0.0),
                num(&["confidence"], 0.1),
                last_active_at,
                num(&["cognitiveProfile", "memoryCapacity"], 0.5),
                num(&["cognitiveProfile", "processingSpeed"], 0.5),
                num(&["cognitiveProfile", "stability"], 0.5),
            ],
        )?;
        Ok(())
    }

    pub fn set_engine_user_state(
        &self,
        user_id: &str,
        state: &serde_json::Value,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        Self::upsert_engine_user_state_conn(&conn, user_id, state)
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

    /// mastery 键 → word_id：`mastery:{w}` 或 `mastery:{w}:{mode}`。ID 禁含 ':'
    /// （[`keys::validate_id`]），故前缀后首段即 word_id，无歧义；非 mastery 键返回 None。
    pub(crate) fn mastery_key_word(algo_id: &str) -> Option<&str> {
        algo_id
            .strip_prefix("mastery:")
            .and_then(|rest| rest.split(':').next())
            .filter(|w| !w.is_empty())
    }

    /// 把某词的 mastery 投影行重导出到 `mastery_states`（admin 用户档案 / look MDM 列的数据源，
    /// 此前无任何写入路径、恒 NULL）：从该词全部候选痕迹键（legacy + 已知 question mode）现存
    /// blob 中取「最近复习」的一条摊平 upsert；无任何痕迹则删投影行。写入 / 回滚 / 删除路径共用
    /// 同一重导出口径（回滚可能把状态还原到更旧值、多痕迹下单键增删不代表整词，直接投影写入值
    /// 会漂移），必须与 engine_algo_states 的变更在同一 conn/tx 内调用。
    pub(crate) fn reproject_mastery(
        conn: &rusqlite::Connection,
        user_id: &str,
        word_id: &str,
    ) -> Result<(), StoreError> {
        use crate::amas::memory::mastery::KNOWN_QUESTION_MODES;
        let mut candidate_keys = Vec::with_capacity(1 + KNOWN_QUESTION_MODES.len());
        candidate_keys.push(format!("mastery:{word_id}"));
        for mode in KNOWN_QUESTION_MODES {
            candidate_keys.push(format!("mastery:{word_id}:{mode}"));
        }
        let placeholders = (2..2 + candidate_keys.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT state_json FROM engine_algo_states
              WHERE user_id=?1 AND algo_id IN ({placeholders})"
        );
        let mut best: Option<crate::amas::memory::mdm::MdmState> = None;
        {
            let mut stmt = conn.prepare_cached(&sql)?;
            let mut sql_params: Vec<&dyn rusqlite::ToSql> = vec![&user_id];
            for key in &candidate_keys {
                sql_params.push(key);
            }
            let mut rows = stmt.query(sql_params.as_slice())?;
            while let Some(row) = rows.next()? {
                let json: String = row.get(0)?;
                let value: serde_json::Value = Self::deserialize_json(&json)?;
                if let Some(mdm) = Self::decode_mastery_mdm_state(value) {
                    #[allow(clippy::unnecessary_map_or)] // is_none_or 需 1.82，MSRV 1.77
                    let newer = best.as_ref().map_or(true, |b| {
                        (mdm.last_review_at.unwrap_or(i64::MIN), mdm.review_count)
                            > (b.last_review_at.unwrap_or(i64::MIN), b.review_count)
                    });
                    if newer {
                        best = Some(mdm);
                    }
                }
            }
        }
        match best {
            Some(m) => {
                conn.execute(
                    "INSERT INTO mastery_states
                       (user_id, word_id, mdm_stability, mdm_difficulty, mdm_memory_strength,
                        mdm_last_review_at_ms, mdm_review_count)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(user_id, word_id) DO UPDATE SET
                       mdm_stability=?3, mdm_difficulty=?4, mdm_memory_strength=?5,
                       mdm_last_review_at_ms=?6, mdm_review_count=?7",
                    params![
                        user_id,
                        word_id,
                        m.stability,
                        m.difficulty,
                        m.memory_strength,
                        m.last_review_at,
                        m.review_count as i64
                    ],
                )?;
            }
            None => {
                conn.execute(
                    "DELETE FROM mastery_states WHERE user_id=?1 AND word_id=?2",
                    params![user_id, word_id],
                )?;
            }
        }
        Ok(())
    }

    pub fn set_engine_algo_state(
        &self,
        user_id: &str,
        algo_id: &str,
        state: &serde_json::Value,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let mut conn = self.conn()?;
        let json = Self::serialize_json(state)?;
        // 投影同步须与 algo 写同笔原子（batch 回滚路径经此还原 mastery 键）。
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO engine_algo_states (user_id, algo_id, state_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id, algo_id) DO UPDATE SET state_json=?3",
            params![user_id, algo_id, json],
        )?;
        if let Some(word_id) = Self::mastery_key_word(algo_id) {
            Self::reproject_mastery(&tx, user_id, word_id)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_engine_algo_state(&self, user_id: &str, algo_id: &str) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM engine_algo_states WHERE user_id=?1 AND algo_id=?2",
            params![user_id, algo_id],
        )?;
        if let Some(word_id) = Self::mastery_key_word(algo_id) {
            Self::reproject_mastery(&tx, user_id, word_id)?;
        }
        tx.commit()?;
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
        // T1.3 A/B：实验切分维度（NULL=非实验事件）。
        let experiment_id: Option<String> = event
            .get("experimentId")
            .or_else(|| event.get("experiment_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let experiment_arm: Option<String> = event
            .get("experimentArm")
            .or_else(|| event.get("experiment_arm"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        // m066:决策↔日志关联。请求级 request_id（NULL=诊断端点/无请求上下文）。
        let request_id: Option<String> = event
            .get("requestId")
            .or_else(|| event.get("request_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let json = Self::serialize_json(event)?;
        // strategy_json 存整坨 event blob 供 get_recent_monitoring_events 向后兼容回读;
        // reward_json 独立存 event.reward 子对象(此前误与 strategy_json 共用 ?15 占位符,
        // 导致 reward_json 列恒被写成整坨 event 而非奖励信号)。
        let reward_json = event
            .get("reward")
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string());

        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO engine_monitoring_events (
                id, user_id, session_id, event_type, timestamp, latency_ms, is_anomaly,
                invariant_violations_json, user_state_attention, user_state_fatigue,
                user_state_motivation, user_state_confidence, user_state_session_event_count,
                user_state_total_event_count, strategy_json, reward_json, cold_start_phase,
                selection_constraints_met, reward_value, config_version,
                routing_algo, routing_weights_json, is_correct,
                experiment_id, experiment_arm, request_id
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26
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
                reward_json,
                cold_start_phase,
                selection_constraints_met,
                reward_value,
                config_version,
                routing_algo,
                routing_weights_json,
                is_correct,
                experiment_id,
                experiment_arm,
                request_id
            ],
        )?;
        Ok(())
    }

    /// 任务B(m065):词态迁移流水 append 一行。边沿触发（频率 < 答题率），out-of-txn、与
    /// insert_word_mastery_event 同 idempotency 门控，best-effort 落库。记录全部等级对（不止
    /// mastered/forgotten）。from/to 为 MasteryLevel 字符串（New/Learning/Reviewing/Mastered/Forgotten）。
    pub fn insert_word_state_transition(
        &self,
        user_id: &str,
        word_id: &str,
        from_state: &str,
        to_state: &str,
        created_at: &str,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO word_state_transitions (user_id, word_id, from_state, to_state, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![user_id, word_id, from_state, to_state, created_at],
        )?;
        Ok(())
    }

    /// 任务B(m065):读取词态迁移流水。可选按 user_id 过滤，created_at DESC 取 limit 行，
    /// 返回 camelCase JSON 行（id/userId/wordId/fromState/toState/createdAt）。
    pub fn list_word_state_transitions(
        &self,
        user_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        let where_sql = if let Some(uid) = user_id {
            args.push(uid.to_string().into());
            "WHERE user_id = ?1 ".to_string()
        } else {
            String::new()
        };
        args.push(limit.into());
        let limit_ph = args.len();
        let sql = format!(
            "SELECT id, user_id, word_id, from_state, to_state, created_at
             FROM word_state_transitions {where_sql}
             ORDER BY created_at DESC LIMIT ?{limit_ph}"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "userId": r.get::<_, String>(1)?,
                    "wordId": r.get::<_, String>(2)?,
                    "fromState": r.get::<_, String>(3)?,
                    "toState": r.get::<_, String>(4)?,
                    "createdAt": r.get::<_, String>(5)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
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

    /// 跨全部历史日期按 algorithm 聚合累计 (call_count, error_count, total_latency_us)。
    /// 供 Prometheus 计数器暴露:5 分钟 flush 会 reset 内存 registry,直接读 registry 会
    /// 让 `_total` 计数器每 5 分钟锯齿归零(违反计数器单调性)。以已落库的历史累计为基准,
    /// 调用方再叠加当前未 flush 的内存增量,即得单调累计值。
    pub fn cumulative_metrics_totals(
        &self,
    ) -> Result<std::collections::HashMap<String, (u64, u64, u64)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT algorithm_id, metrics_json FROM algorithm_metrics_daily",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut totals: std::collections::HashMap<String, (u64, u64, u64)> =
            std::collections::HashMap::new();
        for row in rows {
            let (algo_id, json) = row?;
            if let Ok(snap) =
                serde_json::from_str::<crate::amas::metrics::MetricsSnapshot>(&json)
            {
                let e = totals.entry(algo_id).or_insert((0, 0, 0));
                e.0 = e.0.saturating_add(snap.call_count);
                e.1 = e.1.saturating_add(snap.error_count);
                e.2 = e.2.saturating_add(snap.total_latency_us);
            }
        }
        Ok(totals)
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

    /// 原子写入 AMAS 状态（user_state + algo_states + 可选 ELO），可选附幂等标记。
    ///
    /// 返回值（仅在 `idempotency` 为 `Some` 时有意义）：`true`=标记本次新插入、AMAS 状态已提交；
    /// `false`=标记已存在（并发竞态下另一请求先行处理），**整笔 tx 回滚**、本次 AMAS 增量被丢弃。
    /// `idempotency` 为 `None` 时恒返回 `true`（无标记可竞争）。
    ///
    /// W1-1 并发收口：`INSERT OR IGNORE` 标记的 affected rows 是权威仲裁——锁外预检 + 持锁应用之间
    /// 仍可能两个同 `client_record_id` 请求各自通过预检，此处以"谁先写进标记谁生效、后者整笔回滚"
    /// 消除二次累加 ELO/mastery/trust 的窗口。
    ///
    /// #11/#13/#18/#39：`elo` 为 `Some` 时，ELO 的 read-modify-write 在**本 tx 内**完成
    /// （IMMEDIATE 写锁起点取得，详见 [`Store::with_user_tx`] 注释），与 user_state/algo_states/
    /// 幂等标记同笔原子提交。这样既消除 ELO 跨连接 RMW 的丢更新，又让 ELO 进入「标记存在 ⟺ AMAS
    /// 已应用」不变式（崩溃重试不丢/不二次累加 ELO）。
    pub fn persist_engine_state_atomic(
        &self,
        user_id: &str,
        user_state: &serde_json::Value,
        algo_states: &[(String, serde_json::Value)],
        swd_append: Option<&crate::amas::decision::swd::StrategyRewardEntry>,
        elo: Option<&EloUpdateSpec>,
        idempotency: Option<&str>,
    ) -> Result<PersistOutcome, StoreError> {
        keys::validate_id(user_id)?;
        let mut conn = self.conn()?;
        // IMMEDIATE：ELO RMW 在 BEGIN 即取写锁，杜绝读后写升级丢更新/死锁。
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        Self::upsert_engine_user_state_conn(&tx, user_id, user_state)?;
        let created_at = chrono::Utc::now().to_rfc3339();
        let mut mastery_words = std::collections::BTreeSet::new();
        for (algo_id, value) in algo_states {
            let json = Self::serialize_json(value)?;
            tx.execute(
                "INSERT INTO engine_algo_states (user_id, algo_id, state_json)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(user_id, algo_id) DO UPDATE SET state_json=?3",
                params![user_id, algo_id, json],
            )?;
            if let Some(word_id) = Self::mastery_key_word(algo_id) {
                mastery_words.insert(word_id);
            }
        }
        // mastery 投影同 tx 同步（幂等冲突回滚时随整笔撤销）。按词去重后重导出，
        // 每词一次 ≤5 行 PK 查——热路径每事件通常仅 1 个 mastery 键。
        for word_id in mastery_words {
            Self::reproject_mastery(&tx, user_id, word_id)?;
        }
        // 写放大重构：swd 滚动历史从"每事件全量重写整块 Vec blob"改为**追加一行**到 engine_swd_history
        // （热路径仅 1 INSERT、无 prune；无界增长由维护 worker 兜底）。与 user_state/algo/ELO/幂等标记
        // 同一原子 tx 提交，守 W1-1。返回 seq 供 tx2 失败时按行回滚。
        let mut swd_appended_seq = None;
        if let Some(entry) = swd_append {
            tx.execute(
                "INSERT INTO engine_swd_history
                 (user_id, snap_attention, snap_fatigue, snap_motivation, snap_total_events,
                  strat_difficulty, strat_batch_size, strat_new_ratio, strat_interval_scale,
                  strat_review_mode, reward, ts_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    user_id,
                    entry.user_state_snapshot.attention,
                    entry.user_state_snapshot.fatigue,
                    entry.user_state_snapshot.motivation,
                    entry.user_state_snapshot.total_event_count as i64,
                    entry.strategy.difficulty,
                    entry.strategy.batch_size as i64,
                    entry.strategy.new_ratio,
                    entry.strategy.interval_scale,
                    entry.strategy.review_mode as i64,
                    entry.reward,
                    entry.timestamp,
                ],
            )?;
            swd_appended_seq = Some(tx.last_insert_rowid());
        }
        // #11/#14/#39：ELO 同 tx 原子 RMW。
        if let Some(spec) = elo {
            Self::apply_elo_in_tx(&tx, user_id, spec)?;
        }
        // W1-1：幂等标记与 AMAS 状态同 tx 原子提交,保证"标记存在 ⟺ AMAS 已应用"。
        // affected rows==0 即标记已被并发请求抢先写入 → 回滚本次重复增量（含上面 append 的 swd 行）。
        if let Some(client_record_id) = idempotency {
            let inserted = tx.execute(
                "INSERT OR IGNORE INTO processed_events (user_id, client_record_id, processed_at)
                 VALUES (?1, ?2, ?3)",
                params![user_id, client_record_id, created_at],
            )?;
            if inserted == 0 {
                tx.rollback()?;
                return Ok(PersistOutcome {
                    committed: false,
                    swd_appended_seq: None,
                });
            }
        }
        tx.commit()?;
        Ok(PersistOutcome {
            committed: true,
            swd_appended_seq,
        })
    }

    /// 读取某用户最近 ≤`max` 条 swd 策略历史，重建 `Vec<StrategyRewardEntry>`。
    /// 取 `seq DESC LIMIT max` 再翻回 **`seq ASC`** —— 保持与旧内存 Vec 相同的插入顺序，使
    /// `swd::generate` 的浮点加权求和逐位相同（bit-exact）。缺行返回空 Vec（= `SwdState::default`）。
    pub fn load_swd_history(
        &self,
        user_id: &str,
        max: usize,
    ) -> Result<Vec<crate::amas::decision::swd::StrategyRewardEntry>, StoreError> {
        use crate::amas::decision::swd::{StrategyRewardEntry, UserStateSnapshot};
        use crate::amas::types::StrategyParams;
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT snap_attention, snap_fatigue, snap_motivation, snap_total_events,
                    strat_difficulty, strat_batch_size, strat_new_ratio, strat_interval_scale,
                    strat_review_mode, reward, ts_ms
             FROM (SELECT * FROM engine_swd_history WHERE user_id=?1 ORDER BY seq DESC LIMIT ?2)
             ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![user_id, max as i64], |r| {
            Ok(StrategyRewardEntry {
                user_state_snapshot: UserStateSnapshot {
                    attention: r.get(0)?,
                    fatigue: r.get(1)?,
                    motivation: r.get(2)?,
                    total_event_count: r.get::<_, i64>(3)? as u64,
                },
                strategy: StrategyParams {
                    difficulty: r.get(4)?,
                    batch_size: r.get::<_, i64>(5)? as u32,
                    new_ratio: r.get(6)?,
                    interval_scale: r.get(7)?,
                    review_mode: r.get::<_, i64>(8)? != 0,
                },
                reward: r.get(9)?,
                timestamp: r.get(10)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 某用户 swd 历史的当前最大 seq（无行返回 0）。batch 路径在批量首条前捕获，供"全批失败"
    /// 时 [`Self::delete_swd_history_after`] 把批内 append 的所有 swd 行删回。
    pub fn swd_max_seq(&self, user_id: &str) -> Result<i64, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM engine_swd_history WHERE user_id=?1",
            params![user_id],
            |r| r.get(0),
        )?;
        Ok(seq)
    }

    /// 删除某用户 `seq > after_seq` 的 swd 历史行（batch 全批回滚到批前快照）。
    pub fn delete_swd_history_after(&self, user_id: &str, after_seq: i64) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM engine_swd_history WHERE user_id=?1 AND seq>?2",
            params![user_id, after_seq],
        )?;
        Ok(())
    }

    /// #23/#24：在**单个 IMMEDIATE tx** 内完整重置某用户的引擎态——把 user_state 写回
    /// `UserState::default`、删除该用户在 `engine_algo_states` 的**全部**行（ige/swd/trust + 每词
    /// mastery:*/evm:* + 全局 iad/mtp 等，而非仅枚举三键）、清掉 `user_elo` 与
    /// `word_elo_user_contrib` 账本行。全有或全无，杜绝原四条 autocommit 的部分重置/中途崩溃残留。
    /// 调用侧须持 per-user 锁串行化于 process_event（见 [`AMASEngine::reset_user_state`]）。
    ///
    /// #14 抗投毒：清账本前必须先把该用户对各词全局 `word_elo` 的累计净位移**扣回**。否则
    /// `/api/amas/reset` 用户可达上限→reset→再推，反复刷新单词位移配额绕过钳制，而被推高的
    /// 全局词评分仍残留污染他人排序。扣减把全局词评分还原到"该用户从未触碰"的水平。
    pub fn reset_engine_state_atomic(
        &self,
        user_id: &str,
        default_user_state: &serde_json::Value,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        Self::upsert_engine_user_state_conn(&tx, user_id, default_user_state)?;
        // 删除该用户**所有** algo 状态：mastery:*/evm:*/iad/mtp/ige/swd/trust 一网打尽，
        // 否则旧词记忆/IAD/MTP 残留会与 total_event_count=0 的冷启动态自相矛盾（#24）。
        tx.execute(
            "DELETE FROM engine_algo_states WHERE user_id=?1",
            params![user_id],
        )?;
        // mastery 投影随源数据一并清空（真值 mastery:* 键已全删）。
        tx.execute(
            "DELETE FROM mastery_states WHERE user_id=?1",
            params![user_id],
        )?;
        // 写放大重构：swd 历史已迁出 engine_algo_states 到独立行表，reset 一并清空。
        tx.execute(
            "DELETE FROM engine_swd_history WHERE user_id=?1",
            params![user_id],
        )?;
        tx.execute("DELETE FROM user_elo WHERE user_id=?1", params![user_id])?;
        // #14：先把该用户对各全局词评分的累计净位移扣回，再清账本——否则 reset 等于免费
        // 归还位移配额，同设备可反复 push→reset→push 突破单词钳制（被推高的全局评分仍残留）。
        tx.execute(
            "UPDATE word_elo
                SET rating = rating - COALESCE((
                        SELECT net_displacement FROM word_elo_user_contrib c
                         WHERE c.word_id = word_elo.word_id AND c.user_id = ?1), 0.0)
              WHERE word_id IN (
                        SELECT word_id FROM word_elo_user_contrib WHERE user_id = ?1)",
            params![user_id],
        )?;
        tx.execute(
            "DELETE FROM word_elo_user_contrib WHERE user_id=?1",
            params![user_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// #11/#14：在给定 tx 内对 user_elo + word_elo 做一次 read-modify-write。
    ///
    /// - user_elo：按 `update_elo` 常规累加。
    /// - word_elo（**全局共享**，被全员选词读取）：先按 `update_elo` 计算本次位移，再用
    ///   `word_elo_user_contrib`（m050，per-(user,word) 累计净贡献）把**单个用户**对该词全局评分
    ///   的累计净位移钳在 `±max_user_word_displacement`（#14 抗投毒）：单设备反复全错/全对再不能把
    ///   某词推到 clamp 边界污染他人排序。games 计数仍照常累加（统计真实对局数）。
    fn apply_elo_in_tx(
        tx: &rusqlite::Transaction,
        user_id: &str,
        spec: &EloUpdateSpec,
    ) -> Result<(), StoreError> {
        use crate::amas::elo::{update_elo_with_trend, EloRating};

        // T1.2:连同 trend（带符号残差 EWMA）一并读出；动态 K 关闭时 trend 恒 0、不参与。
        let read_elo =
            |table: &str, key_col: &str, key: &str| -> Result<(EloRating, f64), StoreError> {
                let r = tx
                    .query_row(
                        &format!("SELECT rating, games, trend FROM {table} WHERE {key_col}=?1"),
                        params![key],
                        |r| {
                            Ok((
                                EloRating {
                                    rating: r.get(0)?,
                                    games: r.get(1)?,
                                },
                                r.get::<_, f64>(2)?,
                            ))
                        },
                    )
                    .optional()?;
                Ok(r.unwrap_or((EloRating::default(), 0.0)))
            };

        let (mut user_elo, mut user_trend) = read_elo("user_elo", "user_id", user_id)?;
        let (mut word_elo, mut word_trend) = read_elo("word_elo", "word_id", &spec.word_id)?;
        // T1.1 选词链：word_elo 独有列（user_elo 无）。缺行回退默认 ELO，与 read_elo 同语义。
        let mut word_select: f64 = tx
            .query_row(
                "SELECT rating_select FROM word_elo WHERE word_id=?1",
                params![&spec.word_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| EloRating::default().rating);
        let word_rating_before = word_elo.rating;

        update_elo_with_trend(
            &mut user_elo,
            &mut word_elo,
            &mut user_trend,
            &mut word_trend,
            spec.is_correct,
            &spec.config,
        );

        // #14 抗投毒：限制单用户对该词全局评分的累计净位移。
        let cap = spec.max_user_word_displacement;
        if cap > 0.0 {
            let prior: f64 = tx
                .query_row(
                    "SELECT net_displacement FROM word_elo_user_contrib
                     WHERE user_id=?1 AND word_id=?2",
                    params![user_id, &spec.word_id],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or(0.0);
            let raw_delta = word_elo.rating - word_rating_before;
            // 把"用户累计净位移"钳到 [-cap, cap]，本次允许的实际位移随之收窄。
            let allowed_total = (prior + raw_delta).clamp(-cap, cap);
            let allowed_delta = allowed_total - prior;
            word_elo.rating = (word_rating_before + allowed_delta)
                .clamp(spec.config.min_elo, spec.config.max_elo);
            tx.execute(
                "INSERT INTO word_elo_user_contrib (user_id, word_id, net_displacement)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(user_id, word_id) DO UPDATE SET net_displacement=?3",
                params![user_id, &spec.word_id, allowed_total],
            )?;
        }

        // T1.1 选词链维护（在抗投毒钳制后、用最终 word_elo.rating）：开启时每 refresh_games 局
        // 把选词链快照到估计链当前值（延迟解耦）；关闭时保持同步（无行为影响，便于将来开启）。
        if spec.config.parallel_elo_enabled {
            let interval = spec.config.parallel_elo_refresh_games.max(1);
            if word_elo.games % interval == 0 {
                word_select = word_elo.rating;
            }
        } else {
            word_select = word_elo.rating;
        }

        tx.execute(
            "INSERT INTO user_elo (user_id, rating, games, trend) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(user_id) DO UPDATE SET rating=?2, games=?3, trend=?4",
            params![user_id, user_elo.rating, user_elo.games, user_trend],
        )?;
        tx.execute(
            "INSERT INTO word_elo (word_id, rating, games, trend, rating_select)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(word_id) DO UPDATE SET rating=?2, games=?3, trend=?4, rating_select=?5",
            params![
                &spec.word_id,
                word_elo.rating,
                word_elo.games,
                word_trend,
                word_select
            ],
        )?;
        Ok(())
    }

    // S2 收尾（v1.3.0）：restore_engine_state_atomic（W1-1 的原子回滚 + 清标记）已随路由层
    // 手动快照回滚一并删除。tx2 失败的恢复统一走 processed_events 幂等账本短路（标记保留、
    // AMAS 不重放、重试仅补落裸记录行），不再需要回滚原语。

    // ─────────── admin 决策事件分页钻取 + ?raw 原始样本拉取 ───────────

    /// admin /decisions 分页钻取：对 engine_monitoring_events 按可选 user_id / is_anomaly /
    /// experiment_id 动态 WHERE 过滤，timestamp DESC 分页；同 WHERE 取 COUNT(*) 总数。每行返回
    /// 结构化 camelCase 列 + `raw`（strategy_json 整坨事件 blob，原样 parse 不改写）。
    pub fn list_monitoring_events(
        &self,
        user_id: Option<&str>,
        anomaly_only: bool,
        experiment_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<serde_json::Value>, i64), StoreError> {
        let conn = self.conn()?;

        // 动态 WHERE：占位符按 args 顺序编号，count 与 data 查询共用同一组过滤参数。
        let mut clauses: Vec<String> = Vec::new();
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(uid) = user_id {
            args.push(uid.to_string().into());
            clauses.push(format!("user_id = ?{}", args.len()));
        }
        if anomaly_only {
            clauses.push("is_anomaly = 1".to_string());
        }
        if let Some(eid) = experiment_id {
            args.push(eid.to_string().into());
            clauses.push(format!("experiment_id = ?{}", args.len()));
        }
        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };

        // 总数（同 WHERE，不含分页参数）。
        let count_sql = format!("SELECT COUNT(*) FROM engine_monitoring_events {where_sql}");
        let total: i64 =
            conn.query_row(&count_sql, rusqlite::params_from_iter(args.iter()), |r| {
                r.get(0)
            })?;

        // 数据页：追加 limit / offset 占位符。
        let mut data_args = args.clone();
        data_args.push(limit.into());
        let limit_ph = data_args.len();
        data_args.push(offset.into());
        let offset_ph = data_args.len();
        let data_sql = format!(
            "SELECT id, user_id, session_id, event_type, timestamp, latency_ms, is_anomaly,
                    experiment_id, experiment_arm, is_correct, reward_value, cold_start_phase,
                    config_version, routing_algo, strategy_json, request_id
             FROM engine_monitoring_events
             {where_sql}
             ORDER BY timestamp DESC
             LIMIT ?{limit_ph} OFFSET ?{offset_ph}"
        );
        let mut stmt = conn.prepare(&data_sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(data_args.iter()), |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, i64>(9)?,
                    r.get::<_, f64>(10)?,
                    r.get::<_, Option<String>>(11)?,
                    r.get::<_, String>(12)?,
                    r.get::<_, String>(13)?,
                    r.get::<_, String>(14)?,
                    r.get::<_, Option<String>>(15)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut items = Vec::with_capacity(rows.len());
        for (
            id,
            uid,
            sid,
            etype,
            ts,
            latency,
            anomaly,
            exp_id,
            exp_arm,
            correct,
            reward,
            cold,
            cfg_ver,
            algo,
            strategy_json,
            req_id,
        ) in rows
        {
            // strategy_json 存整坨事件 blob；解析失败回退 null，不阻断分页。
            let raw: serde_json::Value =
                serde_json::from_str(&strategy_json).unwrap_or(serde_json::Value::Null);
            items.push(serde_json::json!({
                "id": id,
                "userId": uid,
                "sessionId": sid,
                "eventType": etype,
                "timestamp": ts,
                "latencyMs": latency,
                "isAnomaly": anomaly != 0,
                "experimentId": exp_id,
                "experimentArm": exp_arm,
                "isCorrect": correct != 0,
                "rewardValue": reward,
                "coldStartPhase": cold,
                "configVersion": cfg_ver,
                "routingAlgo": algo,
                "requestId": req_id,
                "raw": raw,
            }));
        }
        Ok((items, total))
    }

    /// metrics_timeseries ?raw：逐事件原始点。默认分支读 algorithm_metrics_daily 日聚合（无更细
    /// 粒度），故原始样本切到 engine_monitoring_events 逐事件（窗口同为 days），不分箱不降采样。
    pub fn raw_amas_metric_event_points(
        &self,
        days: u32,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT timestamp, latency_ms, reward_value, is_correct, is_anomaly, routing_algo
             FROM engine_monitoring_events
             WHERE datetime(timestamp) >= datetime(?1)
             ORDER BY timestamp ASC",
        )?;
        let rows = stmt
            .query_map([&cutoff], |r| {
                Ok(serde_json::json!({
                    "timestamp": r.get::<_, String>(0)?,
                    "latencyMs": r.get::<_, i64>(1)?,
                    "rewardValue": r.get::<_, f64>(2)?,
                    "isCorrect": r.get::<_, i64>(3)? != 0,
                    "isAnomaly": r.get::<_, i64>(4)? != 0,
                    "routingAlgo": r.get::<_, String>(5)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// user_state_distribution ?raw：逐事件原始 UserState 标量（喂给直方图前的未分箱样本）。
    pub fn raw_amas_user_state_samples(
        &self,
        days: u32,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT user_state_attention, user_state_fatigue, user_state_motivation,
                    user_state_confidence, cold_start_phase
             FROM engine_monitoring_events
             WHERE datetime(timestamp) >= datetime(?1)",
        )?;
        let rows = stmt
            .query_map([&cutoff], |r| {
                Ok(serde_json::json!({
                    "attention": r.get::<_, f64>(0)?,
                    "fatigue": r.get::<_, f64>(1)?,
                    "motivation": r.get::<_, f64>(2)?,
                    "confidence": r.get::<_, f64>(3)?,
                    "coldStartPhase": r.get::<_, Option<String>>(4)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// cognitive_distribution ?raw：逐用户原始认知三轴值（engine_user_states 当前队列）。
    pub fn raw_amas_cognitive_samples(&self) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT user_id, cognitive_memory_capacity, cognitive_processing_speed, cognitive_stability
             FROM engine_user_states",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "userId": r.get::<_, String>(0)?,
                    "memoryCapacity": r.get::<_, f64>(1)?,
                    "processingSpeed": r.get::<_, f64>(2)?,
                    "stability": r.get::<_, f64>(3)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// metrics_mdm_heatmap ?raw：逐 (词,日) 原始遗忘样本（默认分支按 日×难度段 求均的底层行）。
    pub fn raw_amas_mdm_samples(&self, days: u32) -> Result<Vec<serde_json::Value>, StoreError> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT substr(wls.updated_at, 1, 10) AS d, wls.word_id, w.difficulty, wls.mastery_level
             FROM word_learning_states wls
             JOIN words w ON w.id = wls.word_id
             WHERE wls.updated_at >= ?1
             ORDER BY wls.updated_at ASC",
        )?;
        let rows = stmt
            .query_map([&cutoff], |r| {
                let mastery = r.get::<_, f64>(3)?;
                Ok(serde_json::json!({
                    "date": r.get::<_, String>(0)?,
                    "wordId": r.get::<_, String>(1)?,
                    "difficulty": r.get::<_, f64>(2)?,
                    "masteryLevel": mastery,
                    "forget": (1.0 - mastery).clamp(0.0, 1.0),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// metrics_fatigue_timeseries ?raw：逐事件原始疲劳点（默认分支按天 GROUP BY 的底层样本）。
    pub fn raw_amas_fatigue_samples(
        &self,
        days: u32,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT timestamp, user_id, user_state_fatigue
             FROM engine_monitoring_events
             WHERE timestamp >= ?1
             ORDER BY timestamp ASC",
        )?;
        let rows = stmt
            .query_map([&cutoff], |r| {
                Ok(serde_json::json!({
                    "timestamp": r.get::<_, String>(0)?,
                    "userId": r.get::<_, String>(1)?,
                    "fatigue": r.get::<_, f64>(2)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// metrics_decision_histogram ?raw：逐用户原始决策数（默认分支分桶 + P50/P95 前的原始值数组）。
    pub fn raw_amas_decision_counts(
        &self,
        days: u32,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT user_id, COUNT(*) c FROM learning_records
             WHERE created_at >= ?1 GROUP BY user_id ORDER BY c DESC",
        )?;
        let rows = stmt
            .query_map([&cutoff], |r| {
                Ok(serde_json::json!({
                    "userId": r.get::<_, String>(0)?,
                    "count": r.get::<_, i64>(1)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// 写放大重构：[`Store::persist_engine_state_atomic`] 的返回。
/// `committed=false` 表示幂等标记被并发请求抢先写入、整笔回滚未生效（调用方走裸记录回放）。
/// `swd_appended_seq` 为本次 append 到 `engine_swd_history` 的行 seq（committed 时 Some），供
/// tx2 失败时按行回滚删除该条 swd 历史。
#[derive(Debug, Clone)]
pub struct PersistOutcome {
    pub committed: bool,
    pub swd_appended_seq: Option<i64>,
}

/// #11/#14/#39：[`Store::persist_engine_state_atomic`] 的 ELO 原子更新入参。
/// 把"读 user_elo/word_elo → update_elo → 回写"收进引擎状态同一 tx，使 ELO 既不丢更新、
/// 又落入「标记存在 ⟺ AMAS 已应用」不变式。
pub struct EloUpdateSpec {
    pub word_id: String,
    pub is_correct: bool,
    pub config: crate::amas::config::EloConfig,
    /// 单用户对某词全局评分的累计净位移硬上限（#14 抗投毒）。`<= 0` 表示不限。
    pub max_user_word_displacement: f64,
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
        // 跑全量迁移，使测试库与生产(main.rs 启动即 run_migrations)对齐——
        // engine_monitoring_events.request_id 等列由 m066 迁移补充，不在 schema.rs 基础 DDL 中。
        store.run_migrations().unwrap();
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
            .persist_engine_state_atomic("u1", &user_state, &algo, None, None, None)
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
            .persist_engine_state_atomic("u1", &serde_json::json!({"attention":0.9}), &algo2, None, None, None)
            .unwrap();
        assert_eq!(
            store.get_engine_algo_state("u1", "a1").unwrap().unwrap()["l"],
            99
        );

        // 错误 ID
        assert!(matches!(
            store
                .persist_engine_state_atomic("", &user_state, &[], None, None, None)
                .unwrap_err(),
            crate::store::StoreError::Validation(_)
        ));
    }

    /// #24：reset_engine_state_atomic 必须清除该用户**全部** algo 状态（含每词 mastery/EVM、
    /// 全局 IAD/MTP）+ user_elo，而非仅 ige/swd/trust。
    #[test]
    fn reset_engine_state_atomic_clears_all_per_user_state() {
        let _t = tempfile::tempdir().unwrap();
        let store = Store::open(_t.path().join("t.db").to_str().unwrap(), 5000, 2).unwrap();
        store.run_migrations().unwrap();

        // 铺设各类状态。
        store
            .set_engine_user_state("u1", &serde_json::json!({"totalEventCount": 42}))
            .unwrap();
        for key in ["ige", "swd", "trust", "mastery:w1", "evm:w1", "iad", "mtp"] {
            store
                .set_engine_algo_state("u1", key, &serde_json::json!({"x": 1}))
                .unwrap();
        }
        store
            .set_user_elo("u1", &crate::amas::elo::EloRating { rating: 1500.0, games: 9 })
            .unwrap();

        let default_state = serde_json::json!({"totalEventCount": 0});
        store
            .reset_engine_state_atomic("u1", &default_state)
            .unwrap();

        // user_state 回默认。
        let us = store.get_engine_user_state("u1").unwrap().unwrap();
        assert_eq!(us["totalEventCount"], 0);
        // 全部 algo 状态删除（含 mastery/evm/iad/mtp，非仅三键）。
        for key in ["ige", "swd", "trust", "mastery:w1", "evm:w1", "iad", "mtp"] {
            assert!(
                store.get_engine_algo_state("u1", key).unwrap().is_none(),
                "reset 后 {key} 应被删除"
            );
        }
        // user_elo 回默认（删除后读取得默认值，games=0）。
        assert_eq!(store.get_user_elo("u1").unwrap().games, 0);
    }

    /// mastery_states 投影：写入/setter/reset 全路径与 engine_algo_states 真值同步
    ///（回滚路径已随 S2 收尾删除 restore_engine_state_atomic 一并移除）。
    #[test]
    fn mastery_projection_syncs_on_all_write_paths() {
        use rusqlite::{params, OptionalExtension};
        let (_t, store) = tempfile_store();
        let read_row = |w: &str| -> Option<(f64, f64, f64, Option<i64>, i64)> {
            store
                .conn()
                .unwrap()
                .query_row(
                    "SELECT mdm_stability, mdm_difficulty, mdm_memory_strength,
                            mdm_last_review_at_ms, mdm_review_count
                       FROM mastery_states WHERE user_id='u1' AND word_id=?1",
                    params![w],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()
                .unwrap()
        };
        let mdm = |s: f64, rc: u32, at: i64| {
            serde_json::json!({
                "stability": s, "difficulty": 4.0, "memory_strength": s,
                "last_review_at": at, "review_count": rc
            })
        };

        // ① 热路径 persist：legacy 键 → 投影行出现。
        let algo = vec![("mastery:w1".to_string(), mdm(2.5, 3, 1_000))];
        store
            .persist_engine_state_atomic("u1", &serde_json::json!({}), &algo, None, None, None)
            .unwrap();
        let row = read_row("w1").expect("persist 后投影行应存在");
        assert_eq!(row, (2.5, 4.0, 2.5, Some(1_000), 3));

        // ② 多痕迹：更新 per-mode 键（更近复习）→ 投影切到最新痕迹。
        let algo2 = vec![(
            "mastery:w1:word-to-meaning".to_string(),
            mdm(7.0, 4, 2_000),
        )];
        store
            .persist_engine_state_atomic("u1", &serde_json::json!({}), &algo2, None, None, None)
            .unwrap();
        assert_eq!(read_row("w1").unwrap().0, 7.0, "投影应取最近复习痕迹");

        // ③ 通用 delete 同步：删除 per-mode 键 → 投影重导出回 legacy 痕迹。
        store
            .delete_engine_algo_state("u1", "mastery:w1:word-to-meaning")
            .unwrap();
        assert_eq!(read_row("w1").unwrap().0, 2.5, "删除 per-mode 痕迹后投影应回退到 legacy 痕迹");

        // ④ 通用 setter/delete 同步；删尽全部痕迹 → 投影行删除。
        store
            .set_engine_algo_state("u1", "mastery:w2", &mdm(1.0, 1, 500))
            .unwrap();
        assert!(read_row("w2").is_some());
        store.delete_engine_algo_state("u1", "mastery:w2").unwrap();
        assert!(read_row("w2").is_none(), "痕迹删尽后投影行应删除");

        // ⑤ reset：投影随 algo 状态整体清空。
        store
            .reset_engine_state_atomic("u1", &serde_json::json!({}))
            .unwrap();
        assert!(read_row("w1").is_none(), "reset 后投影应清空");

        // ⑥ 幂等冲突整笔回滚：投影不残留。
        let algo3 = vec![("mastery:w3".to_string(), mdm(3.0, 2, 3_000))];
        store
            .persist_engine_state_atomic("u1", &serde_json::json!({}), &algo3, None, None, Some("r1"))
            .unwrap();
        let algo4 = vec![("mastery:w4".to_string(), mdm(9.0, 9, 9_000))];
        let out = store
            .persist_engine_state_atomic("u1", &serde_json::json!({}), &algo4, None, None, Some("r1"))
            .unwrap();
        assert!(!out.committed);
        assert!(read_row("w4").is_none(), "重复事件整笔回滚，投影不应残留");
        assert!(read_row("w3").is_some());
    }

    /// #11/#39：persist_engine_state_atomic 带 EloUpdateSpec 时，在同一 tx 内累加 user_elo/word_elo。
    #[test]
    fn persist_engine_state_atomic_applies_elo_in_same_tx() {
        let _t = tempfile::tempdir().unwrap();
        let store = Store::open(_t.path().join("t.db").to_str().unwrap(), 5000, 2).unwrap();
        store.run_migrations().unwrap();

        let spec = super::EloUpdateSpec {
            word_id: "w1".to_string(),
            is_correct: true,
            config: crate::amas::config::EloConfig::default(),
            max_user_word_displacement: 0.0, // 关掉抗投毒钳制,只验常规累加
        };
        store
            .persist_engine_state_atomic(
                "u1",
                &serde_json::json!({"attention": 0.5}),
                &[],
                None,
                Some(&spec),
                Some("rec-1"),
            )
            .unwrap();

        let user_elo = store.get_user_elo("u1").unwrap();
        let word_elo = store.get_word_elo("w1").unwrap();
        assert_eq!(user_elo.games, 1);
        assert_eq!(word_elo.games, 1);
        assert!(user_elo.rating > 1200.0, "答对后用户 ELO 应上升");
        assert!(store.is_event_processed("u1", "rec-1").unwrap());
    }

    /// T1.2：k_dynamic_enabled 时 trend（残差 EWMA）跨 apply_elo_in_tx 调用持久化累积。
    #[test]
    fn dynamic_k_trend_persists_across_events() {
        let _t = tempfile::tempdir().unwrap();
        let store = Store::open(_t.path().join("t.db").to_str().unwrap(), 5000, 2).unwrap();
        store.run_migrations().unwrap();

        let mut cfg = crate::amas::config::EloConfig::default();
        cfg.k_dynamic_enabled = true;
        // 同一用户对同一词连续全对：user 残差恒正、word 残差恒负 → trend 应同向累积。
        for i in 0..6 {
            let spec = super::EloUpdateSpec {
                word_id: "wt".to_string(),
                is_correct: true,
                config: cfg.clone(),
                max_user_word_displacement: 0.0,
            };
            store
                .persist_engine_state_atomic(
                    "ut",
                    &serde_json::json!({ "attention": 0.5 }),
                    &[],
                    None,
                    Some(&spec),
                    Some(&format!("rec-{i}")),
                )
                .unwrap();
        }
        let conn = store.conn().unwrap();
        let user_trend: f64 = conn
            .query_row("SELECT trend FROM user_elo WHERE user_id='ut'", [], |r| r.get(0))
            .unwrap();
        let word_trend: f64 = conn
            .query_row("SELECT trend FROM word_elo WHERE word_id='wt'", [], |r| r.get(0))
            .unwrap();
        assert!(user_trend > 0.1, "连续全对 user_trend 应正且累积，实际 {user_trend}");
        assert!(word_trend < -0.1, "连续全对 word_trend 应负且累积，实际 {word_trend}");
    }

    /// T1.1：parallel_elo_enabled 时选词链(rating_select)滞后估计链(rating)——刷新前保持快照，
    /// 与估计链解耦。
    #[test]
    fn parallel_elo_select_chain_lags_estimate() {
        let _t = tempfile::tempdir().unwrap();
        let store = Store::open(_t.path().join("t.db").to_str().unwrap(), 5000, 2).unwrap();
        store.run_migrations().unwrap();

        let mut cfg = crate::amas::config::EloConfig::default();
        cfg.parallel_elo_enabled = true;
        cfg.parallel_elo_refresh_games = 8; // 刷新间隔 8 局
                                            // 跑 5 局全对（< 8，尚未触发刷新）：估计链下行，选词链应仍为初始默认 1200。
        for i in 0..5 {
            let spec = super::EloUpdateSpec {
                word_id: "wp".to_string(),
                is_correct: true,
                config: cfg.clone(),
                max_user_word_displacement: 0.0,
            };
            store
                .persist_engine_state_atomic(
                    "up",
                    &serde_json::json!({ "attention": 0.5 }),
                    &[],
                    None,
                    Some(&spec),
                    Some(&format!("rec-{i}")),
                )
                .unwrap();
        }
        let word_elo = store.get_word_elo("wp").unwrap();
        let select = store
            .get_word_select_ratings_by_ids(&["wp".to_string()])
            .unwrap();
        let select_rating = *select.get("wp").unwrap();
        assert!(word_elo.rating < 1200.0, "答对后估计链应下行，实际 {}", word_elo.rating);
        assert!(
            (select_rating - 1200.0).abs() < 1e-9,
            "刷新(8局)前选词链应保持初始 1200，实际 {select_rating}"
        );
        assert!(select_rating > word_elo.rating, "选词链应滞后于已下行的估计链");
    }

    /// #14：单用户对某词全局评分的累计净位移被钳在硬上限内——反复全错不能把该词推到 clamp 边界。
    #[test]
    fn word_elo_user_contribution_is_bounded() {
        let _t = tempfile::tempdir().unwrap();
        let store = Store::open(_t.path().join("t.db").to_str().unwrap(), 5000, 2).unwrap();
        store.run_migrations().unwrap();

        let cap = 50.0;
        // 同一用户对同一词反复全错（word_elo 上行），50 次。
        for i in 0..50 {
            let spec = super::EloUpdateSpec {
                word_id: "wpoison".to_string(),
                is_correct: false,
                config: crate::amas::config::EloConfig::default(),
                max_user_word_displacement: cap,
            };
            store
                .persist_engine_state_atomic(
                    "attacker",
                    &serde_json::json!({}),
                    &[],
                    None,
                    Some(&spec),
                    Some(&format!("rec-{i}")),
                )
                .unwrap();
        }
        let word_elo = store.get_word_elo("wpoison").unwrap();
        // 净位移被钳在 ±cap：评分相对默认 1200 的偏移不超过 cap（含浮点余量）。
        assert!(
            (word_elo.rating - 1200.0).abs() <= cap + 1e-6,
            "单用户净位移应被钳在 ±{cap}，实测 rating={}",
            word_elo.rating
        );
    }

    /// #14（Codex P2）：reset 必须把该用户对各词全局评分的累计净位移**扣回**再清账本，
    /// 否则 push→reset→push 可反复免费归还配额绕过钳制，而被推高的全局评分仍残留污染他人。
    #[test]
    fn reset_subtracts_word_contrib_restoring_global_elo() {
        let _t = tempfile::tempdir().unwrap();
        let store = Store::open(_t.path().join("t.db").to_str().unwrap(), 5000, 2).unwrap();
        store.run_migrations().unwrap();

        // 攻击者反复全错把某词全局评分推离默认（is_correct=false → word_elo 上行）。
        let cap = 50.0;
        for i in 0..30 {
            let spec = super::EloUpdateSpec {
                word_id: "wpoison".to_string(),
                is_correct: false,
                config: crate::amas::config::EloConfig::default(),
                max_user_word_displacement: cap,
            };
            store
                .persist_engine_state_atomic(
                    "attacker",
                    &serde_json::json!({}),
                    &[],
                    None,
                    Some(&spec),
                    Some(&format!("rec-{i}")),
                )
                .unwrap();
        }
        let pushed = store.get_word_elo("wpoison").unwrap().rating;
        let contrib = store.get_word_elo_user_contrib("attacker", "wpoison").unwrap();
        assert!(contrib.abs() > 1.0, "账本应记录非零净位移");
        assert!((pushed - 1200.0).abs() > 1.0, "全局评分应已被推离默认 1200");

        store
            .reset_engine_state_atomic("attacker", &serde_json::json!({}))
            .unwrap();

        let after = store.get_word_elo("wpoison").unwrap().rating;
        assert!(
            (after - (pushed - contrib)).abs() < 1e-6,
            "reset 应把净位移 {contrib} 扣回全局评分：{pushed}→{after}"
        );
        assert!(
            (after - 1200.0).abs() < 1e-6,
            "唯一贡献者 reset 后全局评分应回到从未触碰的水平 1200，实测 {after}"
        );
        assert_eq!(
            store
                .get_word_elo_user_contrib("attacker", "wpoison")
                .unwrap(),
            0.0,
            "账本行应已清除"
        );
    }

    fn swd_entry(reward: f64, ec: u64) -> crate::amas::decision::swd::StrategyRewardEntry {
        crate::amas::decision::swd::StrategyRewardEntry {
            user_state_snapshot: crate::amas::decision::swd::UserStateSnapshot {
                attention: 0.7,
                fatigue: 0.2,
                motivation: 0.1,
                total_event_count: ec,
            },
            strategy: crate::amas::types::StrategyParams {
                difficulty: 0.4 + reward,
                batch_size: 8,
                new_ratio: 0.3,
                interval_scale: 1.2,
                review_mode: ec % 2 == 0,
            },
            reward,
            timestamp: 1000 + ec as i64,
        }
    }

    fn assert_entry_eq(
        a: &crate::amas::decision::swd::StrategyRewardEntry,
        b: &crate::amas::decision::swd::StrategyRewardEntry,
    ) {
        // 逐字段 == （非 epsilon）：bit-exact 守卫，浮点经 REAL 精确往返。
        assert_eq!(a.user_state_snapshot.attention, b.user_state_snapshot.attention);
        assert_eq!(a.user_state_snapshot.fatigue, b.user_state_snapshot.fatigue);
        assert_eq!(a.user_state_snapshot.motivation, b.user_state_snapshot.motivation);
        assert_eq!(
            a.user_state_snapshot.total_event_count,
            b.user_state_snapshot.total_event_count
        );
        assert_eq!(a.strategy.difficulty, b.strategy.difficulty);
        assert_eq!(a.strategy.batch_size, b.strategy.batch_size);
        assert_eq!(a.strategy.new_ratio, b.strategy.new_ratio);
        assert_eq!(a.strategy.interval_scale, b.strategy.interval_scale);
        assert_eq!(a.strategy.review_mode, b.strategy.review_mode);
        assert_eq!(a.reward, b.reward);
        assert_eq!(a.timestamp, b.timestamp);
    }

    /// 写放大重构：swd 历史行表 append → load 保持插入顺序（bit-exact），load max 截取最近 N 条，
    /// 批级删除按 seq 界删行。
    #[test]
    fn swd_history_append_load_order_max_and_rollback() {
        let _t = tempfile::tempdir().unwrap();
        let store = Store::open(_t.path().join("t.db").to_str().unwrap(), 5000, 2).unwrap();
        store.run_migrations().unwrap();

        let e0 = swd_entry(0.10, 0);
        let e1 = swd_entry(0.20, 1);
        let e2 = swd_entry(0.30, 2);
        let mut seqs = Vec::new();
        for e in [&e0, &e1, &e2] {
            let outcome = store
                .persist_engine_state_atomic("u1", &serde_json::json!({}), &[], Some(e), None, None)
                .unwrap();
            assert!(outcome.committed);
            seqs.push(outcome.swd_appended_seq.expect("append seq"));
        }
        // seq 单调递增（FIFO）。
        assert!(seqs[0] < seqs[1] && seqs[1] < seqs[2]);

        // 全量 load：插入序 e0,e1,e2。
        let all = store.load_swd_history("u1", 10).unwrap();
        assert_eq!(all.len(), 3);
        assert_entry_eq(&all[0], &e0);
        assert_entry_eq(&all[1], &e1);
        assert_entry_eq(&all[2], &e2);

        // max=2：取最近 2 条，仍按插入序 e1,e2。
        let recent = store.load_swd_history("u1", 2).unwrap();
        assert_eq!(recent.len(), 2);
        assert_entry_eq(&recent[0], &e1);
        assert_entry_eq(&recent[1], &e2);

        // 按 seq 界删除（learning 事件回放的用户级回滚路径）：删 seq > seqs[0] → 仅剩 e0。
        store.delete_swd_history_after("u1", seqs[0]).unwrap();
        let batch_after = store.load_swd_history("u1", 10).unwrap();
        assert_eq!(batch_after.len(), 1);
        assert_entry_eq(&batch_after[0], &e0);
        assert_eq!(store.swd_max_seq("u1").unwrap(), seqs[0]);

        // reset 清空 swd 历史。
        store
            .reset_engine_state_atomic("u1", &serde_json::json!({}))
            .unwrap();
        assert!(store.load_swd_history("u1", 10).unwrap().is_empty());
        assert_eq!(store.swd_max_seq("u1").unwrap(), 0);
    }
}
