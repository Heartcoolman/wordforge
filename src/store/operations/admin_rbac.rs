//! m034:settings.html「管理员与角色」(RBAC) + 「API 密钥」存储层。
//!
//! 复用既有 `admins` 表(m034 加 `role` 列),不动 admins.rs 的登录/锁定逻辑。
//! API 密钥:库里只存 argon2 hash + 前缀掩码,明文仅生成时返回一次。

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::store::{Store, StoreError};

/// 管理员 RBAC 视图(不含 password_hash)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminRoleView {
    pub id: String,
    pub email: String,
    /// 'super_admin' / 'admin'
    pub role: String,
    pub created_at: String,
    /// NULL 表示从未登录 / 无记录
    pub locked_until: Option<String>,
}

/// API 密钥的安全视图(掩码,绝不含明文/hash)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyView {
    pub id: String,
    pub name: String,
    /// 'read' / 'write' / 'admin'
    pub scope: String,
    /// 前缀掩码,如 "wf_live_a3f…9c2"
    pub prefix: String,
    pub created_at: String,
    pub created_by: Option<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

/// 落库用的 API 密钥行(含 hash,仅 store 内部使用)。
pub struct NewApiKey<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub scope: &'a str,
    pub prefix: &'a str,
    pub hash: &'a str,
    pub created_at: &'a str,
    pub created_by: Option<&'a str>,
    pub expires_at: Option<&'a str>,
}

/// 角色变更/删除守卫的原子结果:区分「目标不存在」「最后一个 super-admin」与成功。
/// 路由层据此映射到既有 AppError(not_found / LAST_SUPER_ADMIN 409),保持响应不变。
pub enum RoleGuardOutcome<T> {
    /// 目标管理员不存在。
    NotFound,
    /// 命中「最后一个 super-admin」守卫,操作被拒。
    LastSuperAdmin,
    /// 操作成功,携带返回值(改角色返回视图;删除返回 ())。
    Ok(T),
}

impl Store {
    // ─────────────── 管理员 RBAC ───────────────

    /// 列出全部管理员(按 created_at 升序)。
    pub fn list_admins(&self) -> Result<Vec<AdminRoleView>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, email, role, created_at, locked_until
             FROM admins ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AdminRoleView {
                    id: r.get(0)?,
                    email: r.get(1)?,
                    role: r.get(2)?,
                    created_at: r.get(3)?,
                    locked_until: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 邀请管理员:插入一行带 role。email 唯一冲突映射 Conflict。
    #[allow(clippy::too_many_arguments)]
    pub fn invite_admin(
        &self,
        id: &str,
        email: &str,
        password_hash: &str,
        role: &str,
        created_at: &str,
    ) -> Result<AdminRoleView, StoreError> {
        crate::store::keys::validate_id(id)?;
        let conn = self.conn()?;
        match conn.execute(
            "INSERT INTO admins (id, email, password_hash, created_at, updated_at,
                                 failed_login_count, locked_until, role)
             VALUES (?1, ?2, ?3, ?4, ?4, 0, NULL, ?5)",
            params![id, email, password_hash, created_at, role],
        ) {
            Ok(_) => Ok(AdminRoleView {
                id: id.to_string(),
                email: email.to_string(),
                role: role.to_string(),
                created_at: created_at.to_string(),
                locked_until: None,
            }),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(StoreError::Conflict {
                    entity: "admin_email".into(),
                    key: email.to_string(),
                })
            }
            Err(e) => Err(e.into()),
        }
    }

    /// 统计某角色的管理员数(用于「最后一个 super-admin」守卫)。
    pub fn count_admins_with_role(&self, role: &str) -> Result<i64, StoreError> {
        let conn = self.conn()?;
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM admins WHERE role = ?1",
            params![role],
            |r| r.get(0),
        )?)
    }

    /// 读取单个管理员当前角色;不存在返回 None。
    pub fn get_admin_role(&self, admin_id: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT role FROM admins WHERE id = ?1",
                params![admin_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?)
    }

    /// 原子变更管理员角色:在**单条连接 + BEGIN IMMEDIATE 事务**内完成
    /// 「读当前角色 → 计数 super-admin → UPDATE」,消除路由层 check-then-act 的
    /// TOCTOU 丢更新(两个并发降级各自看到 supers==2 而双双提交,清空 super-admin)。
    pub fn update_admin_role_guarded(
        &self,
        admin_id: &str,
        new_role: &str,
        updated_at: &str,
    ) -> Result<RoleGuardOutcome<AdminRoleView>, StoreError> {
        self.with_user_tx(|tx| {
            let current: Option<String> = tx
                .query_row(
                    "SELECT role FROM admins WHERE id = ?1",
                    params![admin_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?;
            let Some(current) = current else {
                return Ok(RoleGuardOutcome::NotFound);
            };
            // 守卫:降级最后一个 super-admin 会导致无人能管角色。
            if current == "super_admin" && new_role != "super_admin" {
                let supers: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM admins WHERE role = ?1",
                    params!["super_admin"],
                    |r| r.get(0),
                )?;
                if supers <= 1 {
                    return Ok(RoleGuardOutcome::LastSuperAdmin);
                }
            }
            tx.execute(
                "UPDATE admins SET role = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_role, updated_at, admin_id],
            )?;
            let view = tx.query_row(
                "SELECT id, email, role, created_at, locked_until FROM admins WHERE id = ?1",
                params![admin_id],
                |r| {
                    Ok(AdminRoleView {
                        id: r.get(0)?,
                        email: r.get(1)?,
                        role: r.get(2)?,
                        created_at: r.get(3)?,
                        locked_until: r.get(4)?,
                    })
                },
            )?;
            Ok(RoleGuardOutcome::Ok(view))
        })
    }

    /// 原子删除管理员:在**单条连接 + BEGIN IMMEDIATE 事务**内完成
    /// 「读当前角色 → 计数 super-admin → 级联删 admin_sessions + admins」,
    /// 同 [`Self::update_admin_role_guarded`] 消除「删最后一个 super-admin」的 TOCTOU。
    pub fn delete_admin_guarded(
        &self,
        admin_id: &str,
    ) -> Result<RoleGuardOutcome<()>, StoreError> {
        self.with_user_tx(|tx| {
            let current: Option<String> = tx
                .query_row(
                    "SELECT role FROM admins WHERE id = ?1",
                    params![admin_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?;
            let Some(current) = current else {
                return Ok(RoleGuardOutcome::NotFound);
            };
            // 守卫:删除最后一个 super-admin 会锁死 RBAC 管理。
            if current == "super_admin" {
                let supers: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM admins WHERE role = ?1",
                    params!["super_admin"],
                    |r| r.get(0),
                )?;
                if supers <= 1 {
                    return Ok(RoleGuardOutcome::LastSuperAdmin);
                }
            }
            tx.execute(
                "DELETE FROM admin_sessions WHERE user_id = ?1",
                params![admin_id],
            )?;
            tx.execute("DELETE FROM admins WHERE id = ?1", params![admin_id])?;
            Ok(RoleGuardOutcome::Ok(()))
        })
    }

    /// 变更管理员角色。返回更新后的视图;不存在返回 None。
    /// 守卫由路由层负责(降级最后一个 super-admin 须先在路由层拦截)。
    pub fn update_admin_role(
        &self,
        admin_id: &str,
        role: &str,
        updated_at: &str,
    ) -> Result<Option<AdminRoleView>, StoreError> {
        let conn = self.conn()?;
        let n = conn.execute(
            "UPDATE admins SET role = ?1, updated_at = ?2 WHERE id = ?3",
            params![role, updated_at, admin_id],
        )?;
        if n == 0 {
            return Ok(None);
        }
        Ok(conn
            .query_row(
                "SELECT id, email, role, created_at, locked_until FROM admins WHERE id = ?1",
                params![admin_id],
                |r| {
                    Ok(AdminRoleView {
                        id: r.get(0)?,
                        email: r.get(1)?,
                        role: r.get(2)?,
                        created_at: r.get(3)?,
                        locked_until: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// 删除管理员(级联清其 admin_session)。返回是否删除了行。
    pub fn delete_admin(&self, admin_id: &str) -> Result<bool, StoreError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM admin_sessions WHERE user_id = ?1",
            params![admin_id],
        )?;
        let n = tx.execute("DELETE FROM admins WHERE id = ?1", params![admin_id])?;
        tx.commit()?;
        Ok(n > 0)
    }

    // ─────────────── API 密钥 ───────────────

    /// 列出全部 API 密钥(掩码视图,按 created_at 倒序)。
    pub fn list_api_keys(&self) -> Result<Vec<ApiKeyView>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, scope, prefix, created_at, created_by,
                    expires_at, last_used_at, revoked_at
             FROM api_keys ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map([], api_key_view_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 创建 API 密钥。返回掩码视图(明文由路由层单独返回一次)。
    pub fn create_api_key(&self, key: &NewApiKey<'_>) -> Result<ApiKeyView, StoreError> {
        crate::store::keys::validate_id(key.id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO api_keys (id, name, scope, prefix, hash, created_at,
                                   created_by, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                key.id,
                key.name,
                key.scope,
                key.prefix,
                key.hash,
                key.created_at,
                key.created_by,
                key.expires_at,
            ],
        )?;
        Ok(ApiKeyView {
            id: key.id.to_string(),
            name: key.name.to_string(),
            scope: key.scope.to_string(),
            prefix: key.prefix.to_string(),
            created_at: key.created_at.to_string(),
            created_by: key.created_by.map(str::to_string),
            expires_at: key.expires_at.map(str::to_string),
            last_used_at: None,
            revoked_at: None,
        })
    }

    /// 轮换:用新 hash/prefix 覆盖既有密钥(同 id 同 name/scope,过期时间重置为 new_expires)。
    /// 不存在返回 None。
    pub fn rotate_api_key(
        &self,
        id: &str,
        prefix: &str,
        hash: &str,
        rotated_at: &str,
        new_expires_at: Option<&str>,
    ) -> Result<Option<ApiKeyView>, StoreError> {
        let conn = self.conn()?;
        let n = conn.execute(
            "UPDATE api_keys
             SET prefix = ?1, hash = ?2, created_at = ?3, expires_at = ?4,
                 revoked_at = NULL, last_used_at = NULL
             WHERE id = ?5",
            params![prefix, hash, rotated_at, new_expires_at, id],
        )?;
        if n == 0 {
            return Ok(None);
        }
        // 同连接读回(避免 size=1 连接池二次借用导致死锁)。
        Ok(conn
            .query_row(
                "SELECT id, name, scope, prefix, created_at, created_by,
                        expires_at, last_used_at, revoked_at
                 FROM api_keys WHERE id = ?1",
                params![id],
                api_key_view_from_row,
            )
            .optional()?)
    }

    /// 吊销(物理删除)API 密钥。返回是否删除了行。
    pub fn delete_api_key(&self, id: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let n = conn.execute("DELETE FROM api_keys WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }
}

fn api_key_view_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ApiKeyView> {
    Ok(ApiKeyView {
        id: r.get(0)?,
        name: r.get(1)?,
        scope: r.get(2)?,
        prefix: r.get(3)?,
        created_at: r.get(4)?,
        created_by: r.get(5)?,
        expires_at: r.get(6)?,
        last_used_at: r.get(7)?,
        revoked_at: r.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let s = Store::open(":memory:", 5000, 1).unwrap();
        s.run_migrations().unwrap();
        s
    }

    #[test]
    fn invite_list_update_delete_admin() {
        let s = store();
        s.invite_admin("a1", "a1@x.com", "h", "super_admin", "2026-01-01T00:00:00Z")
            .unwrap();
        s.invite_admin("a2", "a2@x.com", "h", "admin", "2026-01-02T00:00:00Z")
            .unwrap();
        assert_eq!(s.list_admins().unwrap().len(), 2);
        assert_eq!(s.count_admins_with_role("super_admin").unwrap(), 1);

        let v = s
            .update_admin_role("a2", "super_admin", "2026-01-03T00:00:00Z")
            .unwrap()
            .unwrap();
        assert_eq!(v.role, "super_admin");
        assert_eq!(s.count_admins_with_role("super_admin").unwrap(), 2);

        assert!(s.delete_admin("a2").unwrap());
        assert_eq!(s.list_admins().unwrap().len(), 1);
        assert!(!s.delete_admin("nope").unwrap());
    }

    #[test]
    fn guarded_role_change_blocks_last_super_admin() {
        let s = store();
        s.invite_admin("a1", "a1@x.com", "h", "super_admin", "2026-01-01T00:00:00Z")
            .unwrap();
        // 仅一个 super-admin:降级被守卫拦截。
        assert!(matches!(
            s.update_admin_role_guarded("a1", "admin", "2026-01-02T00:00:00Z")
                .unwrap(),
            RoleGuardOutcome::LastSuperAdmin
        ));
        // 目标不存在:NotFound。
        assert!(matches!(
            s.update_admin_role_guarded("nope", "admin", "2026-01-02T00:00:00Z")
                .unwrap(),
            RoleGuardOutcome::NotFound
        ));
        // 有两个 super-admin 时降级成功。
        s.invite_admin("a2", "a2@x.com", "h", "super_admin", "2026-01-03T00:00:00Z")
            .unwrap();
        assert!(matches!(
            s.update_admin_role_guarded("a1", "admin", "2026-01-04T00:00:00Z")
                .unwrap(),
            RoleGuardOutcome::Ok(v) if v.role == "admin"
        ));
        assert_eq!(s.count_admins_with_role("super_admin").unwrap(), 1);
    }

    #[test]
    fn guarded_delete_blocks_last_super_admin() {
        let s = store();
        s.invite_admin("a1", "a1@x.com", "h", "super_admin", "2026-01-01T00:00:00Z")
            .unwrap();
        assert!(matches!(
            s.delete_admin_guarded("a1").unwrap(),
            RoleGuardOutcome::LastSuperAdmin
        ));
        assert!(matches!(
            s.delete_admin_guarded("nope").unwrap(),
            RoleGuardOutcome::NotFound
        ));
        s.invite_admin("a2", "a2@x.com", "h", "super_admin", "2026-01-02T00:00:00Z")
            .unwrap();
        assert!(matches!(
            s.delete_admin_guarded("a1").unwrap(),
            RoleGuardOutcome::Ok(())
        ));
        assert_eq!(s.count_admins_with_role("super_admin").unwrap(), 1);
    }

    #[test]
    fn invite_duplicate_email_conflicts() {
        let s = store();
        s.invite_admin("a1", "dup@x.com", "h", "admin", "2026-01-01T00:00:00Z")
            .unwrap();
        let err = s
            .invite_admin("a2", "dup@x.com", "h", "admin", "2026-01-02T00:00:00Z")
            .unwrap_err();
        assert!(matches!(err, StoreError::Conflict { .. }));
    }

    #[test]
    fn api_key_lifecycle() {
        let s = store();
        s.create_api_key(&NewApiKey {
            id: "k1",
            name: "Prod",
            scope: "write",
            prefix: "wf_live_abc…xyz",
            hash: "argon2hash",
            created_at: "2026-01-01T00:00:00Z",
            created_by: Some("a1"),
            expires_at: Some("2027-01-01T00:00:00Z"),
        })
        .unwrap();
        let list = s.list_api_keys().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].prefix, "wf_live_abc…xyz");

        let rotated = s
            .rotate_api_key("k1", "wf_live_new…end", "h2", "2026-02-01T00:00:00Z", None)
            .unwrap()
            .unwrap();
        assert_eq!(rotated.prefix, "wf_live_new…end");

        assert!(s.delete_api_key("k1").unwrap());
        assert!(s.list_api_keys().unwrap().is_empty());
        assert!(s
            .rotate_api_key("k1", "p", "h", "t", None)
            .unwrap()
            .is_none());
    }
}
