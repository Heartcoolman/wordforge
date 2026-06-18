use crate::amas::elo::EloRating;
use crate::amas::memory::mastery::WordMasteryState;
use crate::amas::memory::mdm::MdmState;
use crate::store::keys;
use crate::store::{Store, StoreError};
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

impl Store {
    fn decode_mastery_mdm_state(value: serde_json::Value) -> Option<MdmState> {
        serde_json::from_value::<MdmState>(value.clone())
            .ok()
            .or_else(|| {
                serde_json::from_value::<WordMasteryState>(value)
                    .ok()
                    .map(|state| state.mdm)
            })
    }

    pub fn get_user_elo(&self, user_id: &str) -> Result<EloRating, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let result = conn
            .query_row(
                "SELECT rating, games FROM user_elo WHERE user_id=?1",
                params![user_id],
                |r| {
                    Ok(EloRating {
                        rating: r.get(0)?,
                        games: r.get(1)?,
                    })
                },
            )
            .optional()?;
        Ok(result.unwrap_or_default())
    }

    pub fn set_user_elo(&self, user_id: &str, elo: &EloRating) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO user_elo (user_id, rating, games) VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id) DO UPDATE SET rating=?2, games=?3",
            params![user_id, elo.rating, elo.games],
        )?;
        Ok(())
    }

    pub fn get_word_elo(&self, word_id: &str) -> Result<EloRating, StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        let result = conn
            .query_row(
                "SELECT rating, games FROM word_elo WHERE word_id=?1",
                params![word_id],
                |r| {
                    Ok(EloRating {
                        rating: r.get(0)?,
                        games: r.get(1)?,
                    })
                },
            )
            .optional()?;
        Ok(result.unwrap_or_default())
    }

    /// #14 抗投毒账本：取单用户对某词全局评分的累计净位移（m050）。无行视为 0.0。
    /// 回滚快照（W1-1）据此把 `word_elo_user_contrib` 连同 `word_elo` 一并捕获/恢复，
    /// 否则部分提交后回滚会留下账本前移、重放误钳合法位移。
    pub fn get_word_elo_user_contrib(
        &self,
        user_id: &str,
        word_id: &str,
    ) -> Result<f64, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        let v: Option<f64> = conn
            .query_row(
                "SELECT net_displacement FROM word_elo_user_contrib WHERE user_id=?1 AND word_id=?2",
                params![user_id, word_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.unwrap_or(0.0))
    }

    /// #3：批量取候选词 ELO。原实现按 word_id 逐个 `get_word_elo`（N 次 pool.get + N 次查询）；
    /// 改为单条 `WHERE word_id IN (...)`，分块 ≤900 占位符以避开 SQLite 参数上限。**表中缺失的词
    /// （未被任何人作答过的新词，常态）回退 `EloRating::default()`（rating=default_elo 1200）填入**，
    /// 与旧逐个查 `get_word_elo`（内部 `unwrap_or_default` 落 EloRating 默认 1200）同语义：调用方
    /// word_selector 需要"全员默认 1200"才能正确算 ZPD 距离；若让其 `Option::<f64>::unwrap_or_default()`
    /// 得 0.0，新词会被挤出 ZPD 高斯峰值、选词排序全错。
    pub fn get_word_elos_by_ids(
        &self,
        word_ids: &[String],
    ) -> Result<HashMap<String, EloRating>, StoreError> {
        if word_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.conn()?;
        let mut result = HashMap::with_capacity(word_ids.len());
        for chunk in word_ids.chunks(900) {
            let placeholders: Vec<&str> = chunk.iter().map(|_| "?").collect();
            let sql = format!(
                "SELECT word_id, rating, games FROM word_elo WHERE word_id IN ({})",
                placeholders.join(",")
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    EloRating {
                        rating: r.get(1)?,
                        games: r.get(2)?,
                    },
                ))
            })?;
            for row in rows {
                let (word_id, elo) = row?;
                result.entry(word_id).or_insert(elo);
            }
        }
        // 表中缺失的词回退默认 ELO（rating 1200），保持与旧逐个查 unwrap_or_default 一致的"全员默认"
        // 语义；缺了它，调用方对缺失项会落到 f64 默认 0.0，污染新词 ZPD 评分。
        for word_id in word_ids {
            result
                .entry(word_id.clone())
                .or_insert_with(EloRating::default);
        }
        Ok(result)
    }

    pub fn set_word_elo(&self, word_id: &str, elo: &EloRating) -> Result<(), StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO word_elo (word_id, rating, games) VALUES (?1, ?2, ?3)
             ON CONFLICT(word_id) DO UPDATE SET rating=?2, games=?3",
            params![word_id, elo.rating, elo.games],
        )?;
        Ok(())
    }

    /// T1.1：批量取候选词「选词链」评分(rating_select)。供 ZPD 选词在 parallel_elo_enabled 时读。
    /// 与 [`Self::get_word_elos_by_ids`] 同语义：缺失词回退默认 ELO(1200)，避免新词 ZPD 评分被 0.0 污染。
    pub fn get_word_select_ratings_by_ids(
        &self,
        word_ids: &[String],
    ) -> Result<HashMap<String, f64>, StoreError> {
        if word_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.conn()?;
        let mut result = HashMap::with_capacity(word_ids.len());
        for chunk in word_ids.chunks(900) {
            let placeholders: Vec<&str> = chunk.iter().map(|_| "?").collect();
            let sql = format!(
                "SELECT word_id, rating_select FROM word_elo WHERE word_id IN ({})",
                placeholders.join(",")
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
            })?;
            for row in rows {
                let (word_id, rating) = row?;
                result.entry(word_id).or_insert(rating);
            }
        }
        let default_elo = EloRating::default().rating;
        for word_id in word_ids {
            result.entry(word_id.clone()).or_insert(default_elo);
        }
        Ok(result)
    }

    /// #3：批量取候选词 mastery 状态。原实现按 word_id 逐个 `get_engine_algo_state`（N 次
    /// pool.get + N 次查询）；改为单条 `WHERE user_id=? AND algo_id IN ('mastery:w1',...)`，分块
    /// ≤900 占位符避开 SQLite 参数上限。返回值 key 为 word_id（去掉 `mastery:` 前缀），缺失项为
    /// `None`，与原逐个查行为一致。
    fn batch_get_mastery_values(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<HashMap<String, Option<serde_json::Value>>, StoreError> {
        keys::validate_id(user_id)?;
        let mut map: HashMap<String, Option<serde_json::Value>> =
            HashMap::with_capacity(word_ids.len());
        // 预填 None，保证每个去重后的 word_id 都有条目（缺行即 None）。
        for word_id in word_ids {
            map.entry(word_id.clone()).or_insert(None);
        }
        if word_ids.is_empty() {
            return Ok(map);
        }
        let conn = self.conn()?;
        let unique_ids: Vec<&String> = {
            let mut seen = std::collections::HashSet::new();
            word_ids.iter().filter(|id| seen.insert(*id)).collect()
        };
        for chunk in unique_ids.chunks(900) {
            let algo_ids: Vec<String> = chunk.iter().map(|w| format!("mastery:{w}")).collect();
            // 占位符 ?2..?N+1（?1 留给 user_id）。
            let placeholders: String = (0..algo_ids.len())
                .map(|i| format!("?{}", i + 2))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT algo_id, state_json FROM engine_algo_states
                 WHERE user_id=?1 AND algo_id IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            // 参数：user_id 在前，随后 N 个 algo_id。
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(algo_ids.len() + 1);
            params.push(&user_id);
            for a in &algo_ids {
                params.push(a);
            }
            let rows = stmt.query_map(params.as_slice(), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (algo_id, json) = row?;
                if let Some(word_id) = algo_id.strip_prefix("mastery:") {
                    let value: serde_json::Value = Self::deserialize_json(&json)?;
                    map.insert(word_id.to_string(), Some(value));
                }
            }
        }
        Ok(map)
    }

    #[deprecated(note = "Use batch_get_engine_mastery_mdm_states for typed mastery states")]
    pub fn batch_get_engine_mastery_states(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<Vec<(String, Option<serde_json::Value>)>, StoreError> {
        let map = self.batch_get_mastery_values(user_id, word_ids)?;
        let mut results = Vec::with_capacity(word_ids.len());
        for word_id in word_ids {
            let state = map.get(word_id).cloned().unwrap_or(None);
            results.push((word_id.clone(), state));
        }
        Ok(results)
    }

    pub fn batch_get_engine_mastery_mdm_states(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<HashMap<String, MdmState>, StoreError> {
        let map = self.batch_get_mastery_values(user_id, word_ids)?;
        let mut results = HashMap::with_capacity(map.len());
        for (word_id, state) in map {
            if let Some(mdm) = state.and_then(Self::decode_mastery_mdm_state) {
                results.insert(word_id, mdm);
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
// 测试用 `let mut cfg = X::default(); cfg.field = v` 易读，本 mod 豁免 field_reassign。
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use crate::amas::config::MemoryModelConfig;
    use crate::amas::elo::EloRating;
    use crate::amas::memory::mastery::{update_mastery_at, WordMasteryState};
    use crate::amas::memory::mdm::MdmState;
    use crate::store::Store;

    #[test]
    fn elo_default_when_missing() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        let elo = store.get_user_elo("u1").unwrap();
        assert_eq!(elo.rating, 1200.0);
        assert_eq!(elo.games, 0);
    }

    #[test]
    fn user_elo_round_trip() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        let elo = EloRating {
            rating: 1350.5,
            games: 10,
        };
        store.set_user_elo("u1", &elo).unwrap();
        let got = store.get_user_elo("u1").unwrap();
        assert_eq!(got.rating, 1350.5);
        assert_eq!(got.games, 10);
    }

    #[test]
    fn word_elo_round_trip() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        let elo = EloRating {
            rating: 1100.0,
            games: 5,
        };
        store.set_word_elo("w1", &elo).unwrap();
        let got = store.get_word_elo("w1").unwrap();
        assert_eq!(got.rating, 1100.0);
    }

    #[test]
    #[allow(deprecated)]
    fn batch_mastery_states() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        let state = serde_json::json!({"level": 0.8});
        store
            .set_engine_algo_state("u1", "mastery:w1", &state)
            .unwrap();
        let results = store
            .batch_get_engine_mastery_states("u1", &["w1".into(), "w2".into()])
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].1.is_some());
        assert!(results[1].1.is_none());
    }

    #[test]
    fn batch_get_engine_mastery_mdm_states_extracts_nested_mastery_state() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();

        let mut mastery = WordMasteryState::new("w1");
        let config = MemoryModelConfig::default();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let _ = update_mastery_at(&mut mastery, true, 0.9, 1.0, 0.9, now_ms, &config, None);

        store
            .set_engine_algo_state("u1", "mastery:w1", &serde_json::to_value(&mastery).unwrap())
            .unwrap();

        let results = store
            .batch_get_engine_mastery_mdm_states("u1", &["w1".into(), "w2".into()])
            .unwrap();

        let mdm = results.get("w1").expect("decoded mastery state");
        assert!(mdm.review_count > 0);
        assert!(results.get("w2").is_none());
    }

    #[test]
    fn batch_get_engine_mastery_mdm_states_still_supports_raw_mdm_state() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();

        let mut mdm = MdmState::default();
        mdm.review_count = 3;
        mdm.last_review_at = Some(123);

        store
            .set_engine_algo_state("u1", "mastery:w1", &serde_json::to_value(&mdm).unwrap())
            .unwrap();

        let results = store
            .batch_get_engine_mastery_mdm_states("u1", &["w1".into()])
            .unwrap();

        assert_eq!(results["w1"].review_count, 3);
        assert_eq!(results["w1"].last_review_at, Some(123));
    }
}
