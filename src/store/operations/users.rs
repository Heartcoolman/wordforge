use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::constants::{LOCKOUT_DURATION_MINUTES, MAX_CAS_RETRIES, MAX_FAILED_LOGIN_ATTEMPTS};
use crate::store::keys;
use crate::store::{Store, StoreError};

const USER_COLS: &str =
    "id, email, username, password_hash, is_banned, created_at, updated_at, failed_login_count, locked_until";

const USER_SCOPED_TABLES: &[&str] = &[
    "sessions",
    "learning_records",
    "word_learning_states",
    "study_configs",
    "engine_user_states",
    "reward_preferences",
    "user_avatars",
    "habit_profiles",
    "notifications",
    "badges",
    "user_preferences",
    "learning_sessions",
    "user_elo",
    "mastery_states",
    "engine_algo_states",
    "user_stats",
    "alert_dedup",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub email: String,
    pub username: String,
    pub password_hash: String,
    pub is_banned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub failed_login_count: u32,
    #[serde(default)]
    pub locked_until: Option<DateTime<Utc>>,
}

fn user_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?,
        email: row.get(1)?,
        username: row.get(2)?,
        password_hash: row.get(3)?,
        is_banned: row.get::<_, i64>(4)? != 0,
        created_at: parse_dt(row.get(5)?)?,
        updated_at: parse_dt(row.get(6)?)?,
        failed_login_count: row.get::<_, i64>(7)? as u32,
        locked_until: row.get::<_, Option<String>>(8)?.map(parse_dt).transpose()?,
    })
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn is_unique_violation(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _)
            if e.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn get_user_conn(conn: &rusqlite::Connection, user_id: &str) -> Result<Option<User>, StoreError> {
    Ok(conn
        .query_row(
            &format!("SELECT {USER_COLS} FROM users WHERE id = ?1"),
            params![user_id],
            user_from_row,
        )
        .optional()?)
}

impl Store {
    pub fn count_users(&self) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    pub fn create_user(&self, user: &User) -> Result<(), StoreError> {
        keys::validate_id(&user.id)?;
        let conn = self.conn()?;
        let locked = user.locked_until.map(|t| t.to_rfc3339());
        match conn.execute(
            "INSERT INTO users (id, email, username, password_hash, is_banned, created_at, updated_at, failed_login_count, locked_until)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &user.id, &user.email, &user.username, &user.password_hash,
                user.is_banned as i64, user.created_at.to_rfc3339(), user.updated_at.to_rfc3339(),
                user.failed_login_count as i64, locked.as_deref(),
            ],
        ) {
            Ok(_) => Ok(()),
            Err(e) if is_unique_violation(&e) => Err(StoreError::Conflict {
                entity: "user_email".into(),
                key: user.email.clone(),
            }),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        get_user_conn(&conn, user_id)
    }

    pub fn get_user_by_email(&self, email: &str) -> Result<Option<User>, StoreError> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {USER_COLS} FROM users WHERE email = ?1 COLLATE NOCASE"),
                params![email],
                user_from_row,
            )
            .optional()?)
    }

    pub fn update_user(&self, user: &User) -> Result<(), StoreError> {
        keys::validate_id(&user.id)?;
        let conn = self.conn()?;
        let locked = user.locked_until.map(|t| t.to_rfc3339());

        for _ in 0..MAX_CAS_RETRIES {
            let existing = get_user_conn(&conn, &user.id)?.ok_or_else(|| StoreError::NotFound {
                entity: "user".into(),
                key: user.id.clone(),
            })?;
            match conn.execute(
                "UPDATE users SET email=?1, username=?2, password_hash=?3, is_banned=?4,
                 created_at=?5, updated_at=?6, failed_login_count=?7, locked_until=?8
                 WHERE id=?9 AND updated_at=?10",
                params![
                    &user.email,
                    &user.username,
                    &user.password_hash,
                    user.is_banned as i64,
                    user.created_at.to_rfc3339(),
                    user.updated_at.to_rfc3339(),
                    user.failed_login_count as i64,
                    locked.as_deref(),
                    &user.id,
                    existing.updated_at.to_rfc3339(),
                ],
            ) {
                Ok(1) => return Ok(()),
                Ok(0) => continue,
                Err(e) if is_unique_violation(&e) => {
                    return Err(StoreError::Conflict {
                        entity: "user_email".into(),
                        key: user.email.clone(),
                    });
                }
                Err(e) => return Err(e.into()),
                _ => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "user".into(),
            key: user.id.clone(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    fn set_banned(&self, user_id: &str, banned: bool) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        for _ in 0..MAX_CAS_RETRIES {
            let user = get_user_conn(&conn, user_id)?.ok_or_else(|| StoreError::NotFound {
                entity: "user".into(),
                key: user_id.into(),
            })?;
            if user.is_banned == banned {
                return Ok(());
            }
            match conn.execute(
                "UPDATE users SET is_banned=?1, updated_at=?2 WHERE id=?3 AND updated_at=?4",
                params![
                    banned as i64,
                    Utc::now().to_rfc3339(),
                    user_id,
                    user.updated_at.to_rfc3339()
                ],
            ) {
                Ok(1) => return Ok(()),
                Ok(0) => continue,
                Err(e) => return Err(e.into()),
                _ => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "user".into(),
            key: user_id.into(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    pub fn ban_user(&self, user_id: &str) -> Result<(), StoreError> {
        self.set_banned(user_id, true)
    }

    pub fn unban_user(&self, user_id: &str) -> Result<(), StoreError> {
        self.set_banned(user_id, false)
    }

    pub fn list_user_ids(&self) -> Result<Vec<String>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT id FROM users")?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(ids)
    }

    pub fn list_users(&self, limit: usize, offset: usize) -> Result<Vec<User>, StoreError> {
        let conn = self.conn()?;
        let sql =
            format!("SELECT {USER_COLS} FROM users ORDER BY created_at DESC LIMIT ?1 OFFSET ?2");
        let mut stmt = conn.prepare(&sql)?;
        let users = stmt
            .query_map(params![limit as i64, offset as i64], user_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(users)
    }

    pub fn record_failed_login(&self, user_id: &str) -> Result<bool, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        for _ in 0..MAX_CAS_RETRIES {
            let user = get_user_conn(&conn, user_id)?.ok_or_else(|| StoreError::NotFound {
                entity: "user".into(),
                key: user_id.into(),
            })?;
            let new_count = user.failed_login_count + 1;
            let locked = new_count >= MAX_FAILED_LOGIN_ATTEMPTS;
            let locked_until = if locked {
                Some((Utc::now() + Duration::minutes(LOCKOUT_DURATION_MINUTES)).to_rfc3339())
            } else {
                user.locked_until.map(|t| t.to_rfc3339())
            };
            match conn.execute(
                "UPDATE users SET failed_login_count=?1, locked_until=?2, updated_at=?3
                 WHERE id=?4 AND updated_at=?5",
                params![
                    new_count as i64,
                    locked_until.as_deref(),
                    Utc::now().to_rfc3339(),
                    user_id,
                    user.updated_at.to_rfc3339(),
                ],
            ) {
                Ok(1) => return Ok(locked),
                Ok(0) => continue,
                Err(e) => return Err(e.into()),
                _ => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "user".into(),
            key: user_id.into(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    pub fn reset_login_attempts(&self, user_id: &str) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        for _ in 0..MAX_CAS_RETRIES {
            let user = get_user_conn(&conn, user_id)?.ok_or_else(|| StoreError::NotFound {
                entity: "user".into(),
                key: user_id.into(),
            })?;
            if user.failed_login_count == 0 && user.locked_until.is_none() {
                return Ok(());
            }
            match conn.execute(
                "UPDATE users SET failed_login_count=0, locked_until=NULL, updated_at=?1
                 WHERE id=?2 AND updated_at=?3",
                params![
                    Utc::now().to_rfc3339(),
                    user_id,
                    user.updated_at.to_rfc3339()
                ],
            ) {
                Ok(1) => return Ok(()),
                Ok(0) => continue,
                Err(e) => return Err(e.into()),
                _ => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "user".into(),
            key: user_id.into(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    pub fn is_account_locked(&self, user_id: &str) -> Result<bool, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let locked_until: Option<Option<String>> = conn
            .query_row(
                "SELECT locked_until FROM users WHERE id=?1",
                params![user_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(locked_until) = locked_until else {
            return Err(StoreError::NotFound {
                entity: "user".into(),
                key: user_id.into(),
            });
        };
        match locked_until {
            Some(s) => Ok(parse_dt(s)? > Utc::now()),
            None => Ok(false),
        }
    }

    pub fn delete_user(&self, user_id: &str) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for table in USER_SCOPED_TABLES {
            tx.execute(
                &format!("DELETE FROM {table} WHERE user_id=?1"),
                params![user_id],
            )?;
        }
        let deleted = tx.execute("DELETE FROM users WHERE id=?1", params![user_id])?;
        if deleted == 0 {
            return Err(StoreError::NotFound {
                entity: "user".into(),
                key: user_id.into(),
            });
        }
        tx.commit()?;
        Ok(())
    }

    pub fn count_users_registered_on_date(&self, date_str: &str) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM users WHERE DATE(created_at) = ?1",
            params![date_str],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    fn sample_user(id: &str, email: &str) -> User {
        User {
            id: id.into(),
            email: email.into(),
            username: "demo".into(),
            password_hash: "hash".into(),
            is_banned: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            failed_login_count: 0,
            locked_until: None,
        }
    }

    #[test]
    fn create_and_get_user() {
        let store = test_store();
        let user = sample_user("u1", "u1@test.com");
        store.create_user(&user).unwrap();
        let got = store.get_user_by_id("u1").unwrap().unwrap();
        assert_eq!(got.email, "u1@test.com");
    }

    #[test]
    fn duplicate_email_conflicts() {
        let store = test_store();
        store
            .create_user(&sample_user("u1", "dup@test.com"))
            .unwrap();
        let err = store
            .create_user(&sample_user("u2", "dup@test.com"))
            .unwrap_err();
        assert!(matches!(err, StoreError::Conflict { .. }));
    }

    #[test]
    fn list_user_ids_works() {
        let store = test_store();
        store
            .create_user(&sample_user("u1", "u1@test.com"))
            .unwrap();
        store
            .create_user(&sample_user("u2", "u2@test.com"))
            .unwrap();
        let mut ids = store.list_user_ids().unwrap();
        ids.sort();
        assert_eq!(ids, vec!["u1", "u2"]);
    }

    #[test]
    fn get_user_by_email_case_insensitive() {
        let store = test_store();
        store
            .create_user(&sample_user("u1", "Test@Example.COM"))
            .unwrap();
        let user = store.get_user_by_email("test@example.com").unwrap();
        assert!(user.is_some());
    }

    #[test]
    fn delete_user_removes_all() {
        let store = test_store();
        store
            .create_user(&sample_user("u1", "u1@test.com"))
            .unwrap();
        store.delete_user("u1").unwrap();
        assert!(store.get_user_by_id("u1").unwrap().is_none());
    }

    fn tempfile_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    #[test]
    fn count_users_and_registered_on_date() {
        let store = test_store();
        assert_eq!(store.count_users().unwrap(), 0);
        store.create_user(&sample_user("u1", "a@b.com")).unwrap();
        store.create_user(&sample_user("u2", "c@d.com")).unwrap();
        assert_eq!(store.count_users().unwrap(), 2);
        let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();
        assert_eq!(store.count_users_registered_on_date(&today).unwrap(), 2);
        assert_eq!(store.count_users_registered_on_date("2000-01-01").unwrap(), 0);
    }

    #[test]
    fn list_users_pagination_orders_by_created_desc() {
        let store = test_store();
        let now = Utc::now();
        for i in 0..3 {
            let mut u = sample_user(&format!("u{i}"), &format!("u{i}@e.com"));
            u.created_at = now - Duration::seconds(i as i64);
            u.updated_at = u.created_at;
            store.create_user(&u).unwrap();
        }
        let page = store.list_users(2, 0).unwrap();
        assert_eq!(page.len(), 2);
        // 最新（u0）在前
        assert_eq!(page[0].id, "u0");
        let next = store.list_users(2, 2).unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].id, "u2");
    }

    #[test]
    fn update_user_persists_changes() {
        let store = test_store();
        let original = sample_user("u1", "a@b.com");
        store.create_user(&original).unwrap();
        let mut updated = original.clone();
        updated.username = "new-name".into();
        updated.is_banned = true;
        store.update_user(&updated).unwrap();
        let got = store.get_user_by_id("u1").unwrap().unwrap();
        assert_eq!(got.username, "new-name");
        assert!(got.is_banned);
    }

    #[test]
    fn update_user_with_email_conflict_returns_conflict() {
        let store = test_store();
        store.create_user(&sample_user("u1", "a@b.com")).unwrap();
        store.create_user(&sample_user("u2", "c@d.com")).unwrap();
        let mut u2 = store.get_user_by_id("u2").unwrap().unwrap();
        u2.email = "a@b.com".into();
        assert!(matches!(
            store.update_user(&u2).unwrap_err(),
            StoreError::Conflict { .. }
        ));
    }

    #[test]
    fn update_user_missing_returns_not_found() {
        let store = test_store();
        let ghost = sample_user("ghost", "ghost@e.com");
        assert!(matches!(
            store.update_user(&ghost).unwrap_err(),
            StoreError::NotFound { .. }
        ));
    }

    #[test]
    fn ban_unban_toggles_state_and_idempotent() {
        let store = test_store();
        store.create_user(&sample_user("u1", "a@b.com")).unwrap();
        assert!(!store.get_user_by_id("u1").unwrap().unwrap().is_banned);
        store.ban_user("u1").unwrap();
        assert!(store.get_user_by_id("u1").unwrap().unwrap().is_banned);
        store.ban_user("u1").unwrap(); // 已禁仍 ok
        store.unban_user("u1").unwrap();
        assert!(!store.get_user_by_id("u1").unwrap().unwrap().is_banned);
        store.unban_user("u1").unwrap();
        // 不存在用户
        assert!(matches!(
            store.ban_user("ghost").unwrap_err(),
            StoreError::NotFound { .. }
        ));
    }

    #[test]
    fn record_failed_login_locks_after_threshold() {
        let store = test_store();
        store.create_user(&sample_user("u1", "a@b.com")).unwrap();
        let max = crate::constants::MAX_FAILED_LOGIN_ATTEMPTS;
        let mut last_locked = false;
        for i in 1..=max {
            last_locked = store.record_failed_login("u1").unwrap();
            if i < max {
                assert!(!last_locked, "should not lock before threshold at attempt {i}");
            }
        }
        assert!(last_locked, "should lock at threshold");
        assert!(store.is_account_locked("u1").unwrap());

        // 不存在用户
        assert!(matches!(
            store.record_failed_login("ghost").unwrap_err(),
            StoreError::NotFound { .. }
        ));
    }

    #[test]
    fn reset_login_attempts_clears_count_and_unlock_status() {
        let store = test_store();
        store.create_user(&sample_user("u1", "a@b.com")).unwrap();
        let max = crate::constants::MAX_FAILED_LOGIN_ATTEMPTS;
        for _ in 0..max {
            let _ = store.record_failed_login("u1").unwrap();
        }
        assert!(store.is_account_locked("u1").unwrap());
        store.reset_login_attempts("u1").unwrap();
        assert!(!store.is_account_locked("u1").unwrap());
        // 已清空再次调用幂等
        store.reset_login_attempts("u1").unwrap();
    }

    #[test]
    fn is_account_locked_returns_not_found_for_missing_user() {
        let store = test_store();
        assert!(matches!(
            store.is_account_locked("ghost").unwrap_err(),
            StoreError::NotFound { .. }
        ));
    }

    #[test]
    fn delete_user_returns_not_found_when_missing() {
        let store = test_store();
        assert!(matches!(
            store.delete_user("ghost").unwrap_err(),
            StoreError::NotFound { .. }
        ));
    }

    #[test]
    fn delete_user_cleans_user_scoped_tables() {
        let (_t, store) = tempfile_store();
        store.create_user(&sample_user("u1", "a@b.com")).unwrap();
        let now = Utc::now().to_rfc3339();
        let conn = store.connection().unwrap();
        conn.execute(
            "INSERT INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, created_at)
             VALUES ('u1','r1','w1',1,100,?1)",
            params![now],
        ).unwrap();
        conn.execute(
            "INSERT INTO notifications (user_id, id, notification_type, title, message, created_at)
             VALUES ('u1','n1','review','t','m',?1)",
            params![now],
        ).unwrap();
        drop(conn);
        store.delete_user("u1").unwrap();
        let conn = store.connection().unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM learning_records WHERE user_id='u1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        let m: i64 = conn
            .query_row("SELECT COUNT(*) FROM notifications WHERE user_id='u1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(m, 0);
    }

    #[test]
    fn validation_rejects_empty_ids() {
        let store = test_store();
        let mut bad = sample_user("", "x@e.com");
        bad.id = "".into();
        assert!(matches!(
            store.create_user(&bad).unwrap_err(),
            StoreError::Validation(_)
        ));
        assert!(matches!(
            store.get_user_by_id("").unwrap_err(),
            StoreError::Validation(_)
        ));
    }

    #[test]
    fn record_failed_login_keeps_locked_until_when_below_threshold() {
        let store = test_store();
        let mut u = sample_user("u1", "a@b.com");
        u.locked_until = Some(Utc::now() + Duration::minutes(5));
        store.create_user(&u).unwrap();
        // 第一次 failed login 不会重新计算 lock，但保持已有 locked_until
        let _ = store.record_failed_login("u1").unwrap();
        let got = store.get_user_by_id("u1").unwrap().unwrap();
        assert!(got.locked_until.is_some());
    }
}
