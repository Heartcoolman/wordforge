use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub token_hash: String,
    pub user_id: String,
    pub token_type: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
}

fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        token_hash: row.get(0)?,
        user_id: row.get(1)?,
        token_type: row.get(2)?,
        created_at: parse_dt(row.get(3)?)?,
        expires_at: parse_dt(row.get(4)?)?,
        revoked: row.get::<_, i64>(5)? != 0,
    })
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })
}

const COLS: &str = "token_hash, user_id, token_type, created_at, expires_at, revoked";

const INSERT_SQL: &str =
    "INSERT INTO {T} (token_hash, user_id, token_type, created_at, expires_at, revoked) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

fn insert_session(
    conn: &rusqlite::Connection,
    table: &str,
    s: &Session,
) -> Result<(), StoreError> {
    conn.execute(
        &INSERT_SQL.replace("{T}", table),
        params![
            &s.token_hash,
            &s.user_id,
            &s.token_type,
            s.created_at.to_rfc3339(),
            s.expires_at.to_rfc3339(),
            s.revoked as i64,
        ],
    )?;
    Ok(())
}

fn get_active_session(
    conn: &rusqlite::Connection,
    table: &str,
    token_hash: &str,
) -> Result<Option<Session>, StoreError> {
    let sql = format!(
        "SELECT {COLS} FROM {table} WHERE token_hash = ?1 AND revoked = 0 AND expires_at > ?2"
    );
    Ok(conn
        .query_row(&sql, params![token_hash, Utc::now().to_rfc3339()], session_from_row)
        .optional()?)
}

fn delete_session_row(
    conn: &rusqlite::Connection,
    table: &str,
    token_hash: &str,
) -> Result<(), StoreError> {
    conn.execute(
        &format!("DELETE FROM {table} WHERE token_hash = ?1"),
        params![token_hash],
    )?;
    Ok(())
}

impl Store {
    pub fn create_session(&self, session: &Session) -> Result<(), StoreError> {
        let conn = self.conn()?;
        insert_session(&conn, "sessions", session)
    }

    pub fn get_session(&self, token_hash: &str) -> Result<Option<Session>, StoreError> {
        let conn = self.conn()?;
        get_active_session(&conn, "sessions", token_hash)
    }

    pub fn delete_session(&self, token_hash: &str) -> Result<(), StoreError> {
        let conn = self.conn()?;
        delete_session_row(&conn, "sessions", token_hash)
    }

    pub fn delete_session_if_exists(&self, token_hash: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM sessions WHERE token_hash = ?1",
            params![token_hash],
        )?;
        Ok(deleted > 0)
    }

    pub fn delete_user_sessions(&self, user_id: &str) -> Result<u32, StoreError> {
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM sessions WHERE user_id = ?1",
            params![user_id],
        )?;
        Ok(deleted as u32)
    }

    pub fn count_user_sessions(&self, user_id: &str) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE user_id = ?1",
            params![user_id],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn cleanup_oldest_user_sessions(
        &self,
        user_id: &str,
        max_sessions: usize,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM sessions WHERE token_hash IN (
                SELECT token_hash FROM sessions
                WHERE user_id = ?1
                ORDER BY created_at DESC
                LIMIT -1 OFFSET ?2
            )",
            params![user_id, max_sessions as i64],
        )?;
        Ok(())
    }

    pub fn cleanup_expired_sessions(&self) -> Result<u32, StoreError> {
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM sessions WHERE token_hash IN (
                SELECT token_hash FROM sessions
                WHERE expires_at <= ?1 OR revoked = 1
                LIMIT 1000
            )",
            params![Utc::now().to_rfc3339()],
        )?;
        Ok(deleted as u32)
    }

    pub fn create_admin_session(&self, session: &Session) -> Result<(), StoreError> {
        let conn = self.conn()?;
        insert_session(&conn, "admin_sessions", session)
    }

    pub fn get_admin_session(&self, token_hash: &str) -> Result<Option<Session>, StoreError> {
        let conn = self.conn()?;
        get_active_session(&conn, "admin_sessions", token_hash)
    }

    pub fn delete_admin_session(&self, token_hash: &str) -> Result<(), StoreError> {
        let conn = self.conn()?;
        delete_session_row(&conn, "admin_sessions", token_hash)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    fn sample_session(token_hash: &str, user_id: &str, expires_in_hours: i64) -> Session {
        Session {
            token_hash: token_hash.to_string(),
            user_id: user_id.to_string(),
            token_type: "user".to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(expires_in_hours),
            revoked: false,
        }
    }

    #[test]
    fn create_and_get_session() {
        let store = test_store();
        let session = sample_session("h1", "u1", 1);
        store.create_session(&session).unwrap();

        let got = store.get_session("h1").unwrap().unwrap();
        assert_eq!(got.user_id, "u1");
    }

    #[test]
    fn expired_session_not_returned() {
        let store = test_store();
        store
            .create_session(&sample_session("h_expired", "u1", -1))
            .unwrap();
        assert!(store.get_session("h_expired").unwrap().is_none());
    }

    #[test]
    fn revoked_session_not_returned() {
        let store = test_store();
        let mut s = sample_session("h_revoked", "u1", 1);
        s.revoked = true;
        store.create_session(&s).unwrap();
        assert!(store.get_session("h_revoked").unwrap().is_none());
    }

    #[test]
    fn delete_session_if_exists_returns_correct_flag() {
        let store = test_store();
        store
            .create_session(&sample_session("h1", "u1", 1))
            .unwrap();
        assert!(store.delete_session_if_exists("h1").unwrap());
        assert!(!store.delete_session_if_exists("h1").unwrap());
    }

    #[test]
    fn delete_user_sessions_removes_all() {
        let store = test_store();
        store
            .create_session(&sample_session("h1", "u1", 1))
            .unwrap();
        store
            .create_session(&sample_session("h2", "u1", 1))
            .unwrap();
        store
            .create_session(&sample_session("h3", "u2", 1))
            .unwrap();
        assert_eq!(store.delete_user_sessions("u1").unwrap(), 2);
        assert_eq!(store.count_user_sessions("u1").unwrap(), 0);
        assert_eq!(store.count_user_sessions("u2").unwrap(), 1);
    }

    #[test]
    fn cleanup_oldest_keeps_newest() {
        let store = test_store();
        for i in 0..5 {
            let mut s = sample_session(&format!("h{i}"), "u1", 1);
            s.created_at = Utc::now() + Duration::seconds(i);
            store.create_session(&s).unwrap();
        }
        store.cleanup_oldest_user_sessions("u1", 2).unwrap();
        assert_eq!(store.count_user_sessions("u1").unwrap(), 2);
    }

    #[test]
    fn cleanup_expired() {
        let store = test_store();
        store
            .create_session(&sample_session("h_expired", "u1", -1))
            .unwrap();
        store
            .create_session(&sample_session("h_alive", "u1", 1))
            .unwrap();

        let cleaned = store.cleanup_expired_sessions().unwrap();
        assert_eq!(cleaned, 1);
        assert_eq!(store.count_user_sessions("u1").unwrap(), 1);
    }

    #[test]
    fn admin_session_crud() {
        let store = test_store();
        let session = sample_session("ah1", "admin1", 1);
        store.create_admin_session(&session).unwrap();

        let got = store.get_admin_session("ah1").unwrap().unwrap();
        assert_eq!(got.user_id, "admin1");

        store.delete_admin_session("ah1").unwrap();
        assert!(store.get_admin_session("ah1").unwrap().is_none());
    }
}
