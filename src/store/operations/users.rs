use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::constants::{LOCKOUT_DURATION_MINUTES, MAX_CAS_RETRIES, MAX_FAILED_LOGIN_ATTEMPTS};
use crate::store::keys;
use crate::store::{Store, StoreError};

const USER_COLS: &str =
    "id, email, username, password_hash, is_banned, created_at, updated_at, failed_login_count, locked_until, role, status, last_login_at, referrer_source";

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
    "feedback_items",
    "telemetry_events",
    "telemetry_summaries",
    "word_favorites",
    "word_notes",
    "wordbook_import_history",
    "wb_center_imports",
    // m025:用户自有活动日志,delete_user 时级联清理避免孤儿
    "user_activity_log",
    // GDPR Art.17:含 user_id 的剩余 PII 表(client_devices 含 last_ip/country、
    // user_elo_history 评分快照、监控事件、密码重置令牌),注销时一并清理
    "client_devices",
    "user_elo_history",
    "engine_monitoring_events",
    "password_reset_tokens",
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
    /// m022:'user' / 'staff' / 'admin'。默认 'user'(DB CHECK 限定)。
    #[serde(default = "default_role")]
    pub role: String,
    /// m022:'active' / 'inactive' / 'suspended'。与 is_banned 互不矛盾:
    /// - is_banned=1 时通常 status='suspended',但允许独立修改。
    /// - 'inactive' 表示长时间未登录(可由定时 worker 维护),不阻止登录。
    #[serde(default = "default_status")]
    pub status: String,
    /// m022:最近一次登录成功时间;NULL 表示从未登录。
    #[serde(default)]
    pub last_login_at: Option<DateTime<Utc>>,
    /// m025:注册来源(referral/techweekly 等);NULL 表示未知。
    #[serde(default)]
    pub referrer_source: Option<String>,
}

fn default_role() -> String {
    "user".to_string()
}
fn default_status() -> String {
    "active".to_string()
}

/// m024:list_users 高级过滤参数(None = 该条件不应用)。
#[derive(Debug, Default, Clone)]
pub struct UserListFilter {
    /// LIKE %needle%,小写匹配 username / email
    pub search: Option<String>,
    pub banned: Option<bool>,
    /// 'user' / 'staff' / 'admin'
    pub role: Option<String>,
    /// 'active' / 'inactive' / 'suspended'
    pub status: Option<String>,
    /// 最近 N 天未登录(含从未登录);0 / None 表示不过滤
    pub inactive_days: Option<u32>,
}

/// m024:用户答题聚合,供 list?includeStats=true 批量返回。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStats {
    pub record_count: i64,
    pub correct_count: i64,
    /// 最近 20 题 is_correct 序列(从新到旧,0/1)。不足 20 时 len < 20。
    pub last20_outcomes: Vec<i64>,
}

/// 用户管理页顶部筛选 chip 的各类计数(对齐设计图:全部 / 活跃 / 7 天未登录 /
/// 禁用 / 管理员)。各计数相互独立(非互斥),前端 chip 点击切换对应过滤参数。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserFacets {
    /// 全部用户。
    pub total: i64,
    /// status='active'(活跃)。
    pub active: i64,
    /// 7 天未登录:last_login_at IS NULL 或 < now-7d(与 list inactive_days=7 同口径)。
    pub inactive7d: i64,
    /// is_banned=1(禁用)。
    pub banned: i64,
    /// role='admin'(管理员)。
    pub admins: i64,
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
        role: row
            .get::<_, Option<String>>(9)?
            .unwrap_or_else(default_role),
        status: row
            .get::<_, Option<String>>(10)?
            .unwrap_or_else(default_status),
        last_login_at: row
            .get::<_, Option<String>>(11)?
            .map(parse_dt)
            .transpose()?,
        referrer_source: row.get::<_, Option<String>>(12)?,
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
        // m024+m025:写 role/status/referrer_source(last_login_at 创建时为 NULL)
        match conn.execute(
            "INSERT INTO users (id, email, username, password_hash, is_banned, \
             created_at, updated_at, failed_login_count, locked_until, role, status, referrer_source) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                &user.id, &user.email, &user.username, &user.password_hash,
                user.is_banned as i64, user.created_at.to_rfc3339(), user.updated_at.to_rfc3339(),
                user.failed_login_count as i64, locked.as_deref(),
                &user.role, &user.status, user.referrer_source.as_deref(),
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

    /// m024:CAS 更新 user.role。CHECK 约束在 DB 层兜底非法值。幂等。
    pub fn update_user_role(&self, user_id: &str, new_role: &str) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        for _ in 0..MAX_CAS_RETRIES {
            let user = get_user_conn(&conn, user_id)?.ok_or_else(|| StoreError::NotFound {
                entity: "user".into(),
                key: user_id.into(),
            })?;
            if user.role == new_role {
                return Ok(());
            }
            match conn.execute(
                "UPDATE users SET role=?1, updated_at=?2 WHERE id=?3 AND updated_at=?4",
                params![
                    new_role,
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

    /// 批量封禁:单事务内对存在的用户批量改 is_banned、撤销其 session,并为每个
    /// 被操作用户写一条 admin 审计。返回实际存在(受影响)的 user_id,供 handler 计数。
    pub fn batch_ban_users(
        &self,
        user_ids: &[String],
        admin_id: &str,
        action: &str,
        reason: Option<&str>,
    ) -> Result<Vec<String>, StoreError> {
        self.batch_set_banned(user_ids, true, admin_id, action, reason)
    }

    /// 批量解封:语义同 batch_ban_users,但不撤销 session(参照原 unban 口径)。
    pub fn batch_unban_users(
        &self,
        user_ids: &[String],
        admin_id: &str,
        action: &str,
        reason: Option<&str>,
    ) -> Result<Vec<String>, StoreError> {
        self.batch_set_banned(user_ids, false, admin_id, action, reason)
    }

    fn batch_set_banned(
        &self,
        user_ids: &[String],
        banned: bool,
        admin_id: &str,
        action: &str,
        reason: Option<&str>,
    ) -> Result<Vec<String>, StoreError> {
        for id in user_ids {
            keys::validate_id(id)?;
        }
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;

        // IN(...) 占位符按 id 数量动态构建
        let placeholders = vec!["?"; user_ids.len()].join(",");
        let bind: Vec<&dyn rusqlite::ToSql> =
            user_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        // 先取出确实存在的 user_id(只对存在的写状态/session/审计)
        let existing: Vec<String> = {
            let mut stmt = tx.prepare(&format!(
                "SELECT id FROM users WHERE id IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(bind.as_slice(), |r| r.get::<_, String>(0))?;
            rows.collect::<Result<_, _>>()?
        };
        if existing.is_empty() {
            tx.commit()?;
            return Ok(existing);
        }

        let exist_ph = vec!["?"; existing.len()].join(",");
        let exist_bind: Vec<&dyn rusqlite::ToSql> =
            existing.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let now = Utc::now().to_rfc3339();

        let mut update_params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(existing.len() + 2);
        update_params.push(&banned);
        update_params.push(&now);
        update_params.extend_from_slice(exist_bind.as_slice());
        tx.execute(
            &format!("UPDATE users SET is_banned=?1, updated_at=?2 WHERE id IN ({exist_ph})"),
            update_params.as_slice(),
        )?;

        if banned {
            tx.execute(
                &format!("DELETE FROM sessions WHERE user_id IN ({exist_ph})"),
                exist_bind.as_slice(),
            )?;
        }

        // 每个被操作用户写一条审计(同事务内循环 INSERT,合并到一次连接)
        let metadata = reason.map(|r| serde_json::json!({ "reason": r }).to_string());
        for uid in &existing {
            tx.execute(
                "INSERT INTO update_audit_log
                    (id, admin_id, from_version, to_version, channel,
                     started_at, completed_at, outcome,
                     action, target_type, target_id, metadata_json)
                 VALUES (?1, ?2, '', '', '', ?3, ?3, 'success',
                         ?4, 'user', ?5, ?6)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    admin_id,
                    now,
                    action,
                    uid,
                    metadata
                ],
            )?;
        }

        tx.commit()?;
        Ok(existing)
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

    /// m024:扩展版 list_users —— 支持 search/banned/role/status/inactive_days 过滤,
    /// 返回 (页内 users, total)。WHERE 子句按需拼装,所有动态值用 ? 占位绑定。
    pub fn list_users_filtered(
        &self,
        filter: &UserListFilter,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<User>, u64), StoreError> {
        use rusqlite::types::Value;
        let conn = self.conn()?;

        let mut where_parts: Vec<&'static str> = Vec::new();
        let mut binds: Vec<Value> = Vec::new();

        if let Some(needle) = filter.search.as_deref() {
            // LIKE escape:% / _ 字符显式转义,statement 用 ESCAPE '\\'
            let pat = format!(
                "%{}%",
                needle
                    .to_lowercase()
                    .replace('\\', r"\\")
                    .replace('%', r"\%")
                    .replace('_', r"\_")
            );
            where_parts
                .push("(LOWER(username) LIKE ? ESCAPE '\\' OR LOWER(email) LIKE ? ESCAPE '\\')");
            binds.push(Value::Text(pat.clone()));
            binds.push(Value::Text(pat));
        }
        if let Some(b) = filter.banned {
            where_parts.push("is_banned = ?");
            binds.push(Value::Integer(b as i64));
        }
        if let Some(r) = filter.role.as_deref() {
            where_parts.push("role = ?");
            binds.push(Value::Text(r.to_string()));
        }
        if let Some(s) = filter.status.as_deref() {
            where_parts.push("status = ?");
            binds.push(Value::Text(s.to_string()));
        }
        if let Some(d) = filter.inactive_days.filter(|d| *d > 0) {
            // 含从未登录(NULL)
            where_parts
                .push("(last_login_at IS NULL OR datetime(last_login_at) < datetime('now', ?))");
            binds.push(Value::Text(format!("-{d} days")));
        }
        let where_sql = if where_parts.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_parts.join(" AND "))
        };

        // total
        let count_sql = format!("SELECT COUNT(*) FROM users{where_sql}");
        let total: i64 =
            conn.query_row(&count_sql, rusqlite::params_from_iter(binds.iter()), |r| {
                r.get(0)
            })?;

        // page
        let mut list_binds = binds.clone();
        list_binds.push(Value::Integer(limit as i64));
        list_binds.push(Value::Integer(offset as i64));
        let list_sql = format!(
            "SELECT {USER_COLS} FROM users{where_sql} ORDER BY created_at DESC LIMIT ? OFFSET ?"
        );
        let mut stmt = conn.prepare(&list_sql)?;
        let users = stmt
            .query_map(rusqlite::params_from_iter(list_binds.iter()), user_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((users, total as u64))
    }

    /// m024:batch 拉一组 user 的答题聚合(record_count / correct_count /
    /// 最近 20 题 outcome)。空入参直接返回空 map。
    /// 走 `idx_learning_records_user_time` 索引,3 段查询合一连接。
    pub fn list_user_stats(
        &self,
        user_ids: &[String],
    ) -> Result<std::collections::HashMap<String, UserStats>, StoreError> {
        use std::collections::HashMap;
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.conn()?;
        let placeholders = vec!["?"; user_ids.len()].join(",");

        // 初始化 map 保证未答题用户也返回 zero stats
        let mut map: HashMap<String, UserStats> = user_ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    UserStats {
                        record_count: 0,
                        correct_count: 0,
                        last20_outcomes: Vec::new(),
                    },
                )
            })
            .collect();

        // 1) record_count + correct_count
        let agg_sql = format!(
            "SELECT user_id, COUNT(*) AS n, \
             COALESCE(SUM(CASE WHEN is_correct=1 THEN 1 ELSE 0 END), 0) AS c \
             FROM learning_records WHERE user_id IN ({placeholders}) GROUP BY user_id"
        );
        let mut stmt = conn.prepare(&agg_sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(user_ids.iter()), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (uid, n, c) = row?;
            if let Some(e) = map.get_mut(&uid) {
                e.record_count = n;
                e.correct_count = c;
            }
        }
        drop(stmt);

        // 2) 最近 20 题 outcome(window function,SQLite 3.25+;rusqlite ≥3.36 默认支持)
        let win_sql = format!(
            "SELECT user_id, is_correct FROM ( \
                 SELECT user_id, is_correct, \
                        ROW_NUMBER() OVER (PARTITION BY user_id ORDER BY created_at DESC, id DESC) AS rn \
                 FROM learning_records WHERE user_id IN ({placeholders}) \
             ) WHERE rn <= 20 ORDER BY user_id, rn"
        );
        let mut stmt = conn.prepare(&win_sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(user_ids.iter()), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (uid, oc) = row?;
            if let Some(e) = map.get_mut(&uid) {
                e.last20_outcomes.push(oc);
            }
        }

        Ok(map)
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
        // 用户自建词书：先收集 id，删 wordbook_words，再删 wordbooks（主键非 user_id）
        let user_wordbook_ids: Vec<String> = {
            let mut stmt =
                tx.prepare("SELECT id FROM wordbooks WHERE book_type='user' AND user_id=?1")?;
            let rows = stmt.query_map(params![user_id], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<_, _>>()?
        };
        for wb_id in &user_wordbook_ids {
            tx.execute(
                "DELETE FROM wordbook_words WHERE wordbook_id=?1",
                params![wb_id],
            )?;
        }
        tx.execute(
            "DELETE FROM wordbooks WHERE book_type='user' AND user_id=?1",
            params![user_id],
        )?;
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

    /// 一次连接内算齐用户管理页 5 个 chip 计数。inactive7d 与 list_users_filtered
    /// 的 `inactive_days=7` 谓词逐字一致(含从未登录),保证 chip 数与点击后列表条数对齐。
    pub fn admin_user_facets(&self) -> Result<UserFacets, StoreError> {
        let conn = self.conn()?;
        let row = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN last_login_at IS NULL
                    OR datetime(last_login_at) < datetime('now', '-7 days')
                    THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_banned = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN role = 'admin' THEN 1 ELSE 0 END), 0)
             FROM users",
            [],
            |r| {
                Ok(UserFacets {
                    total: r.get(0)?,
                    active: r.get(1)?,
                    inactive7d: r.get(2)?,
                    banned: r.get(3)?,
                    admins: r.get(4)?,
                })
            },
        )?;
        Ok(row)
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

    /// 检查用户是否在 24h 内已导出过数据；返回上次导出时间（如有）。
    pub fn get_gdpr_export_last_at(
        &self,
        user_id: &str,
    ) -> Result<Option<DateTime<Utc>>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let row: Option<String> = conn
            .query_row(
                "SELECT exported_at FROM gdpr_export_log WHERE user_id=?1",
                params![user_id],
                |r| r.get(0),
            )
            .optional()?;
        match row {
            Some(s) => Ok(Some(s.parse::<DateTime<Utc>>().map_err(|e| {
                StoreError::Validation(format!("gdpr_export_log invalid date: {e}"))
            })?)),
            None => Ok(None),
        }
    }

    /// 记录本次导出时间（upsert）。
    pub fn upsert_gdpr_export_log(&self, user_id: &str) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO gdpr_export_log (user_id, exported_at) VALUES (?1, ?2)
             ON CONFLICT(user_id) DO UPDATE SET exported_at=?2",
            params![user_id, now],
        )?;
        Ok(())
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
            role: "user".to_string(),
            status: "active".to_string(),
            last_login_at: None,
            referrer_source: None,
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
        assert_eq!(
            store.count_users_registered_on_date("2000-01-01").unwrap(),
            0
        );
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
                assert!(
                    !last_locked,
                    "should not lock before threshold at attempt {i}"
                );
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
        )
        .unwrap();
        drop(conn);
        store.delete_user("u1").unwrap();
        let conn = store.connection().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM learning_records WHERE user_id='u1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        let m: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM notifications WHERE user_id='u1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(m, 0);
    }

    #[test]
    fn delete_user_cleans_favorites_notes_history_and_user_wordbooks() {
        let (_t, store) = tempfile_store();
        store.create_user(&sample_user("u1", "a@b.com")).unwrap();
        let now = Utc::now().to_rfc3339();
        let conn = store.connection().unwrap();
        // word_favorites
        conn.execute(
            "INSERT INTO word_favorites (user_id, word_id, created_at) VALUES ('u1','w1',?1)",
            params![now],
        )
        .unwrap();
        // word_notes
        conn.execute(
            "INSERT INTO word_notes (user_id, id, word_id, content, created_at, updated_at)
             VALUES ('u1','note1','w1','memo',?1,?1)",
            params![now],
        )
        .unwrap();
        // wordbook_import_history
        conn.execute(
            "INSERT INTO wordbook_import_history (id, user_id, source_type, status, created_at)
             VALUES ('h1','u1','json','success',?1)",
            params![now],
        )
        .unwrap();
        // 用户自建词书 + 关联词条
        conn.execute(
            "INSERT INTO wordbooks (id, name, description, book_type, user_id, word_count, created_at)
             VALUES ('wb_u1','my','d','user','u1',1,?1)",
            params![now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO wordbook_words (wordbook_id, word_id, added_at) VALUES ('wb_u1','w1',?1)",
            params![now],
        )
        .unwrap();
        // 系统词书及其条目不应被删
        conn.execute(
            "INSERT INTO wordbooks (id, name, description, book_type, user_id, word_count, created_at)
             VALUES ('wb_sys','sys','d','system',NULL,1,?1)",
            params![now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO wordbook_words (wordbook_id, word_id, added_at) VALUES ('wb_sys','w1',?1)",
            params![now],
        )
        .unwrap();
        drop(conn);

        store.delete_user("u1").unwrap();

        let conn = store.connection().unwrap();
        let cnt = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(
            cnt("SELECT COUNT(*) FROM word_favorites WHERE user_id='u1'"),
            0
        );
        assert_eq!(cnt("SELECT COUNT(*) FROM word_notes WHERE user_id='u1'"), 0);
        assert_eq!(
            cnt("SELECT COUNT(*) FROM wordbook_import_history WHERE user_id='u1'"),
            0
        );
        assert_eq!(
            cnt("SELECT COUNT(*) FROM wordbooks WHERE book_type='user' AND user_id='u1'"),
            0
        );
        assert_eq!(
            cnt("SELECT COUNT(*) FROM wordbook_words WHERE wordbook_id='wb_u1'"),
            0
        );
        // 系统词书保留
        assert_eq!(cnt("SELECT COUNT(*) FROM wordbooks WHERE id='wb_sys'"), 1);
        assert_eq!(
            cnt("SELECT COUNT(*) FROM wordbook_words WHERE wordbook_id='wb_sys'"),
            1
        );
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
