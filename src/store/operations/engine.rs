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
        conn.execute("DELETE FROM engine_user_states WHERE user_id=?1", params![user_id])?;
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

    pub fn insert_monitoring_event(&self, event: &serde_json::Value) -> Result<(), StoreError> {
        let id = event
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let user_id = event.get("userId").or(event.get("user_id")).and_then(|v| v.as_str()).unwrap_or("");
        let session_id = event.get("sessionId").or(event.get("session_id")).and_then(|v| v.as_str()).unwrap_or("");
        let timestamp = event.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        let json = Self::serialize_json(event)?;

        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO engine_monitoring_events (id, user_id, session_id, timestamp, strategy_json, reward_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, user_id, session_id, timestamp, json],
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

    pub fn persist_engine_state_atomic(
        &self,
        user_id: &str,
        user_state: &serde_json::Value,
        algo_states: &[(String, serde_json::Value)],
    ) -> Result<(), StoreError> {
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
        tx.commit()?;
        Ok(())
    }
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
        store.set_engine_algo_state("u1", "mastery:w1", &state).unwrap();
        let got = store.get_engine_algo_state("u1", "mastery:w1").unwrap().unwrap();
        assert_eq!(got["level"], 0.8);
    }
}
