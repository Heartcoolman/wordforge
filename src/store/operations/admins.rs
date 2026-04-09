use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::constants::{LOCKOUT_DURATION_MINUTES, MAX_CAS_RETRIES, MAX_FAILED_LOGIN_ATTEMPTS};
use crate::store::keys;
use crate::store::{Store, StoreError};

const ADMIN_COLS: &str =
    "id, email, password_hash, created_at, updated_at, failed_login_count, locked_until";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Admin {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub failed_login_count: u32,
    #[serde(default)]
    pub locked_until: Option<DateTime<Utc>>,
}

fn admin_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Admin> {
    Ok(Admin {
        id: row.get(0)?,
        email: row.get(1)?,
        password_hash: row.get(2)?,
        created_at: parse_dt(row.get(3)?)?,
        updated_at: parse_dt(row.get(4)?)?,
        failed_login_count: row.get::<_, i64>(5)? as u32,
        locked_until: row.get::<_, Option<String>>(6)?.map(parse_dt).transpose()?,
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

fn is_unique_violation(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _)
            if e.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn get_admin_conn(conn: &rusqlite::Connection, admin_id: &str) -> Result<Option<Admin>, StoreError> {
    Ok(conn
        .query_row(
            &format!("SELECT {ADMIN_COLS} FROM admins WHERE id = ?1"),
            params![admin_id],
            admin_from_row,
        )
        .optional()?)
}

impl Store {
    pub fn create_admin(&self, admin: &Admin) -> Result<(), StoreError> {
        keys::validate_id(&admin.id)?;
        let conn = self.conn()?;
        let locked = admin.locked_until.map(|t| t.to_rfc3339());
        match conn.execute(
            "INSERT INTO admins (id, email, password_hash, created_at, updated_at, failed_login_count, locked_until)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                &admin.id, &admin.email, &admin.password_hash,
                admin.created_at.to_rfc3339(), admin.updated_at.to_rfc3339(),
                admin.failed_login_count as i64, locked.as_deref(),
            ],
        ) {
            Ok(_) => Ok(()),
            Err(e) if is_unique_violation(&e) => Err(StoreError::Conflict {
                entity: "admin_email".into(),
                key: admin.email.clone(),
            }),
            Err(e) => Err(e.into()),
        }
    }

    pub fn create_first_admin(&self, admin: &Admin) -> Result<(), StoreError> {
        keys::validate_id(&admin.id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM admins", [], |r| r.get(0))?;
        if count > 0 {
            return Err(StoreError::Conflict {
                entity: "admin".into(),
                key: "already_exists".into(),
            });
        }
        let locked = admin.locked_until.map(|t| t.to_rfc3339());
        match tx.execute(
            "INSERT INTO admins (id, email, password_hash, created_at, updated_at, failed_login_count, locked_until)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                &admin.id, &admin.email, &admin.password_hash,
                admin.created_at.to_rfc3339(), admin.updated_at.to_rfc3339(),
                admin.failed_login_count as i64, locked.as_deref(),
            ],
        ) {
            Ok(_) => {}
            Err(e) if is_unique_violation(&e) => {
                return Err(StoreError::Conflict {
                    entity: "admin_email".into(),
                    key: admin.email.clone(),
                });
            }
            Err(e) => return Err(e.into()),
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_admin_by_id(&self, admin_id: &str) -> Result<Option<Admin>, StoreError> {
        keys::validate_id(admin_id)?;
        let conn = self.conn()?;
        get_admin_conn(&conn, admin_id)
    }

    pub fn get_admin_by_email(&self, email: &str) -> Result<Option<Admin>, StoreError> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {ADMIN_COLS} FROM admins WHERE email = ?1 COLLATE NOCASE"),
                params![email],
                admin_from_row,
            )
            .optional()?)
    }

    pub fn any_admin_exists(&self) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM admins", [], |r| r.get(0))?;
        Ok(count > 0)
    }

    pub fn record_admin_failed_login(&self, admin_id: &str) -> Result<bool, StoreError> {
        keys::validate_id(admin_id)?;
        let conn = self.conn()?;
        for _ in 0..MAX_CAS_RETRIES {
            let admin = get_admin_conn(&conn, admin_id)?.ok_or_else(|| StoreError::NotFound {
                entity: "admin".into(),
                key: admin_id.into(),
            })?;
            let new_count = admin.failed_login_count + 1;
            let locked = new_count >= MAX_FAILED_LOGIN_ATTEMPTS;
            let locked_until = if locked {
                Some((Utc::now() + Duration::minutes(LOCKOUT_DURATION_MINUTES)).to_rfc3339())
            } else {
                admin.locked_until.map(|t| t.to_rfc3339())
            };
            match conn.execute(
                "UPDATE admins SET failed_login_count=?1, locked_until=?2, updated_at=?3
                 WHERE id=?4 AND updated_at=?5",
                params![
                    new_count as i64, locked_until.as_deref(), Utc::now().to_rfc3339(),
                    admin_id, admin.updated_at.to_rfc3339(),
                ],
            ) {
                Ok(1) => return Ok(locked),
                Ok(0) => continue,
                Err(e) => return Err(e.into()),
                _ => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "admin".into(),
            key: admin_id.into(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    pub fn reset_admin_login_attempts(&self, admin_id: &str) -> Result<(), StoreError> {
        keys::validate_id(admin_id)?;
        let conn = self.conn()?;
        for _ in 0..MAX_CAS_RETRIES {
            let admin = get_admin_conn(&conn, admin_id)?.ok_or_else(|| StoreError::NotFound {
                entity: "admin".into(),
                key: admin_id.into(),
            })?;
            if admin.failed_login_count == 0 && admin.locked_until.is_none() {
                return Ok(());
            }
            match conn.execute(
                "UPDATE admins SET failed_login_count=0, locked_until=NULL, updated_at=?1
                 WHERE id=?2 AND updated_at=?3",
                params![Utc::now().to_rfc3339(), admin_id, admin.updated_at.to_rfc3339()],
            ) {
                Ok(1) => return Ok(()),
                Ok(0) => continue,
                Err(e) => return Err(e.into()),
                _ => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "admin".into(),
            key: admin_id.into(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    pub fn is_admin_account_locked(&self, admin_id: &str) -> Result<bool, StoreError> {
        keys::validate_id(admin_id)?;
        let conn = self.conn()?;
        let locked_until: Option<Option<String>> = conn
            .query_row(
                "SELECT locked_until FROM admins WHERE id=?1",
                params![admin_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(locked_until) = locked_until else {
            return Err(StoreError::NotFound {
                entity: "admin".into(),
                key: admin_id.into(),
            });
        };
        match locked_until {
            Some(s) => Ok(parse_dt(s)? > Utc::now()),
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    fn sample_admin(id: &str, email: &str) -> Admin {
        Admin {
            id: id.into(),
            email: email.into(),
            password_hash: "hash".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            failed_login_count: 0,
            locked_until: None,
        }
    }

    #[test]
    fn create_and_get_admin() {
        let store = test_store();
        let admin = sample_admin("a1", "a1@test.com");
        store.create_admin(&admin).unwrap();
        let got = store.get_admin_by_id("a1").unwrap().unwrap();
        assert_eq!(got.email, "a1@test.com");
    }

    #[test]
    fn duplicate_email_conflicts() {
        let store = test_store();
        store.create_admin(&sample_admin("a1", "dup@test.com")).unwrap();
        let err = store.create_admin(&sample_admin("a2", "dup@test.com")).unwrap_err();
        assert!(matches!(err, StoreError::Conflict { .. }));
    }

    #[test]
    fn get_admin_by_email_case_insensitive() {
        let store = test_store();
        store.create_admin(&sample_admin("a1", "Admin@Test.COM")).unwrap();
        let admin = store.get_admin_by_email("admin@test.com").unwrap();
        assert!(admin.is_some());
    }

    #[test]
    fn create_first_admin_blocks_second() {
        let store = test_store();
        store.create_first_admin(&sample_admin("a1", "first@test.com")).unwrap();
        let err = store.create_first_admin(&sample_admin("a2", "second@test.com")).unwrap_err();
        assert!(matches!(err, StoreError::Conflict { .. }));
    }

    #[test]
    fn any_admin_exists_empty() {
        let store = test_store();
        assert!(!store.any_admin_exists().unwrap());
    }

    #[test]
    fn any_admin_exists_after_create() {
        let store = test_store();
        store.create_admin(&sample_admin("a1", "a1@test.com")).unwrap();
        assert!(store.any_admin_exists().unwrap());
    }

    #[test]
    fn record_failed_login_locks_account() {
        let store = test_store();
        store.create_admin(&sample_admin("a1", "a1@test.com")).unwrap();
        for i in 0..MAX_FAILED_LOGIN_ATTEMPTS {
            let locked = store.record_admin_failed_login("a1").unwrap();
            assert_eq!(locked, i + 1 >= MAX_FAILED_LOGIN_ATTEMPTS);
        }
        assert!(store.is_admin_account_locked("a1").unwrap());
    }

    #[test]
    fn reset_login_attempts_unlocks() {
        let store = test_store();
        store.create_admin(&sample_admin("a1", "a1@test.com")).unwrap();
        for _ in 0..MAX_FAILED_LOGIN_ATTEMPTS {
            store.record_admin_failed_login("a1").unwrap();
        }
        store.reset_admin_login_attempts("a1").unwrap();
        assert!(!store.is_admin_account_locked("a1").unwrap());
        let admin = store.get_admin_by_id("a1").unwrap().unwrap();
        assert_eq!(admin.failed_login_count, 0);
        assert!(admin.locked_until.is_none());
    }

    #[test]
    fn is_admin_account_locked_not_found() {
        let store = test_store();
        let err = store.is_admin_account_locked("nonexistent").unwrap_err();
        assert!(matches!(err, StoreError::NotFound { .. }));
    }
}
