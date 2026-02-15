use sled::Transactional;

use crate::store::keys;
use crate::store::{Store, StoreError};

impl Store {
    pub fn get_engine_user_state(
        &self,
        user_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let key = keys::engine_user_state_key(user_id)?;
        match self.engine_user_states.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn set_engine_user_state(
        &self,
        user_id: &str,
        state: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let key = keys::engine_user_state_key(user_id)?;
        self.engine_user_states
            .insert(key.as_bytes(), Self::serialize(state)?)?;
        Ok(())
    }

    pub fn delete_engine_user_state(&self, user_id: &str) -> Result<(), StoreError> {
        let key = keys::engine_user_state_key(user_id)?;
        self.engine_user_states.remove(key.as_bytes())?;
        Ok(())
    }

    pub fn get_engine_algo_state(
        &self,
        user_id: &str,
        algo_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let key = keys::engine_algo_state_key(user_id, algo_id)?;
        match self.engine_algorithm_states.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn set_engine_algo_state(
        &self,
        user_id: &str,
        algo_id: &str,
        state: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let key = keys::engine_algo_state_key(user_id, algo_id)?;
        self.engine_algorithm_states
            .insert(key.as_bytes(), Self::serialize(state)?)?;
        Ok(())
    }

    pub fn delete_engine_algo_state(&self, user_id: &str, algo_id: &str) -> Result<(), StoreError> {
        let key = keys::engine_algo_state_key(user_id, algo_id)?;
        self.engine_algorithm_states.remove(key.as_bytes())?;
        Ok(())
    }

    pub fn insert_monitoring_event(&self, event: &serde_json::Value) -> Result<(), StoreError> {
        let id = match event.get("id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => uuid::Uuid::new_v4().to_string(),
        };

        let ts = event
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_millis())
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        let key = keys::monitoring_event_key(ts, &id)?;
        self.engine_monitoring_events
            .insert(key.as_bytes(), Self::serialize(event)?)?;
        Ok(())
    }

    pub fn get_recent_monitoring_events(
        &self,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let mut events = Vec::new();
        for item in self.engine_monitoring_events.iter() {
            let (_, raw) = item?;
            events.push(Self::deserialize(&raw)?);
            if events.len() >= limit {
                break;
            }
        }
        Ok(events)
    }

    pub fn upsert_metrics_daily(
        &self,
        date: &str,
        algo_id: &str,
        metrics: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let key = keys::metrics_daily_key(date, algo_id)?;
        self.algorithm_metrics_daily
            .insert(key.as_bytes(), Self::serialize(metrics)?)?;
        Ok(())
    }

    pub fn batch_upsert_metrics_daily(
        &self,
        entries: &[(String, serde_json::Value)],
    ) -> Result<(), StoreError> {
        let mut batch = sled::Batch::default();
        for (key, value) in entries {
            batch.insert(key.as_bytes(), Self::serialize(value)?);
        }
        self.algorithm_metrics_daily.apply_batch(batch)?;
        Ok(())
    }

    pub fn get_metrics_daily(
        &self,
        date: &str,
        algo_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let key = keys::metrics_daily_key(date, algo_id)?;
        match self.algorithm_metrics_daily.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn persist_engine_state_atomic(
        &self,
        user_id: &str,
        user_state: &serde_json::Value,
        algo_states: &[(String, serde_json::Value)],
    ) -> Result<(), StoreError> {
        let user_key = keys::engine_user_state_key(user_id)?;
        let user_bytes = Self::serialize(user_state)?;

        let algo_entries: Vec<(String, Vec<u8>)> = algo_states
            .iter()
            .map(|(algo_id, value)| {
                let key = keys::engine_algo_state_key(user_id, algo_id)?;
                let bytes = Self::serialize(value)?;
                Ok((key, bytes))
            })
            .collect::<Result<Vec<_>, StoreError>>()?;

        (&self.engine_user_states, &self.engine_algorithm_states)
            .transaction(|(tx_user, tx_algo)| {
                tx_user.insert(user_key.as_bytes(), user_bytes.as_slice())?;
                for (key, bytes) in &algo_entries {
                    tx_algo.insert(key.as_bytes(), bytes.as_slice())?;
                }
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError<()>| match e {
                sled::transaction::TransactionError::Abort(()) => {
                    StoreError::Sled(sled::Error::Unsupported("transaction aborted".into()))
                }
                sled::transaction::TransactionError::Storage(se) => StoreError::Sled(se),
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::store::Store;

    #[test]
    fn save_and_load_engine_state() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("engine-db");
        let store = Store::open(db_path.to_str().unwrap()).unwrap();

        let state = serde_json::json!({"attention": 0.7});
        store.set_engine_user_state("u1", &state).unwrap();
        let got = store.get_engine_user_state("u1").unwrap().unwrap();
        assert_eq!(got["attention"], 0.7);
    }
}
