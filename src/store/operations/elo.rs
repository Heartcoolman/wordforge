use crate::amas::elo::EloRating;
use crate::amas::memory::mdm::MdmState;
use crate::store::keys;
use crate::store::{Store, StoreError};
use std::collections::HashMap;

impl Store {
    fn batch_get_engine_mastery_state_values_by_ids(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<HashMap<String, Option<serde_json::Value>>, StoreError> {
        let mut state_by_word_id = HashMap::with_capacity(word_ids.len());

        for word_id in word_ids {
            if state_by_word_id.contains_key(word_id) {
                continue;
            }

            let algo_id = format!("mastery:{word_id}");
            let state = self.get_engine_algo_state(user_id, &algo_id)?;
            state_by_word_id.insert(word_id.clone(), state);
        }

        Ok(state_by_word_id)
    }

    /// 获取用户 ELO 评分，不存在时返回默认值
    pub fn get_user_elo(&self, user_id: &str) -> Result<EloRating, StoreError> {
        let key = keys::user_elo_key(user_id)?;
        match self.engine_algorithm_states.get(key.as_bytes())? {
            Some(raw) => Ok(Self::deserialize(&raw)?),
            None => Ok(EloRating::default()),
        }
    }

    /// 设置用户 ELO 评分
    pub fn set_user_elo(&self, user_id: &str, elo: &EloRating) -> Result<(), StoreError> {
        let key = keys::user_elo_key(user_id)?;
        self.engine_algorithm_states
            .insert(key.as_bytes(), Self::serialize(elo)?)?;
        Ok(())
    }

    /// 获取单词 ELO 评分，不存在时返回默认值
    pub fn get_word_elo(&self, word_id: &str) -> Result<EloRating, StoreError> {
        let key = keys::word_elo_key(word_id)?;
        match self.engine_algorithm_states.get(key.as_bytes())? {
            Some(raw) => Ok(Self::deserialize(&raw)?),
            None => Ok(EloRating::default()),
        }
    }

    /// 批量获取单词 ELO（缺失时填充默认值）
    pub fn get_word_elos_by_ids(
        &self,
        word_ids: &[String],
    ) -> Result<HashMap<String, EloRating>, StoreError> {
        let mut result = HashMap::with_capacity(word_ids.len());

        for word_id in word_ids {
            if result.contains_key(word_id) {
                continue;
            }

            result.insert(word_id.clone(), self.get_word_elo(word_id)?);
        }

        Ok(result)
    }

    /// 设置单词 ELO 评分
    pub fn set_word_elo(&self, word_id: &str, elo: &EloRating) -> Result<(), StoreError> {
        let key = keys::word_elo_key(word_id)?;
        self.engine_algorithm_states
            .insert(key.as_bytes(), Self::serialize(elo)?)?;
        Ok(())
    }

    /// 批量读取 mastery 状态
    #[deprecated(note = "Use batch_get_engine_mastery_mdm_states for typed mastery states")]
    pub fn batch_get_engine_mastery_states(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<Vec<(String, Option<serde_json::Value>)>, StoreError> {
        let state_by_word_id =
            self.batch_get_engine_mastery_state_values_by_ids(user_id, word_ids)?;
        let mut results = Vec::with_capacity(word_ids.len());

        for word_id in word_ids {
            let state = state_by_word_id.get(word_id).cloned().unwrap_or(None);
            results.push((word_id.clone(), state));
        }

        Ok(results)
    }

    /// 批量读取 mastery 状态并直接转换为 MDM 状态。
    ///
    /// - 缺失状态返回 `MdmState::default()`
    /// - 非法状态（反序列化失败）回退为 `MdmState::default()`
    pub fn batch_get_engine_mastery_mdm_states(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<HashMap<String, MdmState>, StoreError> {
        let state_by_word_id =
            self.batch_get_engine_mastery_state_values_by_ids(user_id, word_ids)?;
        let mut results = HashMap::with_capacity(state_by_word_id.len());

        for (word_id, state) in state_by_word_id {
            let mdm_state = state
                .and_then(|value| serde_json::from_value::<MdmState>(value).ok())
                .unwrap_or_default();

            results.insert(word_id, mdm_state);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::amas::elo::EloRating;
    use crate::store::Store;

    #[test]
    fn elo_default_when_missing() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db").to_str().unwrap()).unwrap();

        let elo = store.get_user_elo("u1").unwrap();
        assert_eq!(elo.rating, 1200.0);
        assert_eq!(elo.games, 0);
    }

    #[test]
    fn user_elo_round_trip() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db").to_str().unwrap()).unwrap();

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
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db").to_str().unwrap()).unwrap();

        let elo = EloRating {
            rating: 1100.0,
            games: 5,
        };
        store.set_word_elo("w1", &elo).unwrap();
        let got = store.get_word_elo("w1").unwrap();
        assert_eq!(got.rating, 1100.0);
        assert_eq!(got.games, 5);
    }

    #[test]
    #[allow(deprecated)]
    fn batch_mastery_states() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db").to_str().unwrap()).unwrap();

        // 设置一个 mastery 状态
        let state = serde_json::json!({"level": 0.8});
        store
            .set_engine_algo_state("u1", "mastery:w1", &state)
            .unwrap();

        let results = store
            .batch_get_engine_mastery_states("u1", &["w1".into(), "w2".into()])
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "w1");
        assert!(results[0].1.is_some());
        assert_eq!(results[1].0, "w2");
        assert!(results[1].1.is_none());
    }

    #[test]
    #[allow(deprecated)]
    fn batch_mastery_states_preserves_order_and_duplicates() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db").to_str().unwrap()).unwrap();

        let state = serde_json::json!({"memory_strength": 0.6, "review_count": 2});
        store
            .set_engine_algo_state("u1", "mastery:w1", &state)
            .unwrap();

        let results = store
            .batch_get_engine_mastery_states(
                "u1",
                &["w1".to_string(), "w2".to_string(), "w1".to_string()],
            )
            .unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, "w1");
        assert!(results[0].1.is_some());
        assert_eq!(results[1].0, "w2");
        assert!(results[1].1.is_none());
        assert_eq!(results[2].0, "w1");
        assert!(results[2].1.is_some());
    }

    #[test]
    fn get_word_elos_by_ids_fills_default_for_missing() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db").to_str().unwrap()).unwrap();

        let elo = EloRating {
            rating: 1400.0,
            games: 9,
        };
        store.set_word_elo("w1", &elo).unwrap();

        let elos = store
            .get_word_elos_by_ids(&["w1".to_string(), "w2".to_string(), "w1".to_string()])
            .unwrap();

        assert_eq!(elos.len(), 2);
        assert_eq!(elos.get("w1").unwrap().rating, 1400.0);
        assert_eq!(elos.get("w1").unwrap().games, 9);
        assert_eq!(elos.get("w2").unwrap().rating, EloRating::default().rating);
        assert_eq!(elos.get("w2").unwrap().games, EloRating::default().games);
    }

    #[test]
    fn batch_mastery_mdm_states_returns_typed_values() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db").to_str().unwrap()).unwrap();

        let state = serde_json::json!({
            "memory_strength": 0.8,
            "review_count": 3,
            "short_term_strength": 0.2,
            "medium_term_strength": 0.3,
            "long_term_strength": 0.4,
            "consolidation": 0.5,
        });
        store
            .set_engine_algo_state("u1", "mastery:w1", &state)
            .unwrap();

        let states = store
            .batch_get_engine_mastery_mdm_states(
                "u1",
                &["w1".to_string(), "w2".to_string(), "w1".to_string()],
            )
            .unwrap();

        assert_eq!(states.len(), 2);
        assert_eq!(states.get("w1").unwrap().memory_strength, 0.8);
        assert_eq!(states.get("w1").unwrap().review_count, 3);
        assert_eq!(states.get("w2").unwrap().memory_strength, 0.0);
        assert_eq!(states.get("w2").unwrap().review_count, 0);
    }
}
