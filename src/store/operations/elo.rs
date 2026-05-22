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

    fn batch_get_mastery_values(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<HashMap<String, Option<serde_json::Value>>, StoreError> {
        let mut map = HashMap::with_capacity(word_ids.len());
        for word_id in word_ids {
            if map.contains_key(word_id) {
                continue;
            }
            let state = self.get_engine_algo_state(user_id, &format!("mastery:{word_id}"))?;
            map.insert(word_id.clone(), state);
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
