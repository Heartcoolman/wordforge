//! S5 + v1.1-P2.10：admin 敏感操作审计表 `update_audit_log`。
//!
//! 历史背景：本表最早只为「二进制自更新」设计（apply 一次写一条，from_version →
//! to_version，outcome=in_progress → success/failed）。v1.1-P2.10 起扩展为通用 admin
//! 审计表：通过新增 `action / target_type / target_id / metadata_json` 列，覆盖
//! 资源包热更、封禁/解禁、密码重置/直设等所有 admin 高权限操作。
//!
//! 两套写入入口语义不重叠：
//!   - `insert_update_audit`：保留为自更新专用，outcome 由 `complete_update_audit` 收尾；
//!   - `insert_admin_audit`：一次写入即终态（outcome='success'），用于资源包 / 用户管理
//!     等无后续异步阶段的操作。

use rusqlite::params;
use serde::Serialize;

use crate::store::{Store, StoreError};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAuditEntry {
    pub id: String,
    pub admin_id: String,
    pub from_version: String,
    pub to_version: String,
    pub channel: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    /// `success` | `failed` | `in_progress`
    pub outcome: String,
    pub error: Option<String>,
    /// v1.1-P2.10：操作类型；老 self_update 行该列默认 'self_update'。
    pub action: String,
    /// v1.1-P2.10：操作目标类型（如 `resource_pack`、`user`）；self_update 行为 None。
    pub target_type: Option<String>,
    /// v1.1-P2.10：操作目标 ID（如 packId、userId）；self_update 行为 None。
    pub target_id: Option<String>,
    /// v1.1-P2.10：与该操作相关的额外元数据 JSON（如 version、channel、reason）。
    pub metadata_json: Option<String>,
}

/// P1：全局 admin 审计查询的动态过滤条件（全部可选；`started_at` 用 RFC3339 字符串
/// 做字典序比较，与写入口径一致）。
#[derive(Debug, Default, Clone)]
pub struct AdminAuditFilter {
    pub admin_id: Option<String>,
    pub action: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
}

impl Store {
    /// 写入一条升级记录（apply 触发时调用，outcome 初始为 `in_progress`）。
    pub fn insert_update_audit(
        &self,
        id: &str,
        admin_id: &str,
        from_version: &str,
        to_version: &str,
        channel: &str,
    ) -> Result<(), StoreError> {
        self.insert_update_audit_with_action(
            id,
            admin_id,
            from_version,
            to_version,
            channel,
            "self_update",
        )
    }

    /// m022:同 `insert_update_audit`,但允许指定 `action`(默认 `self_update`)。
    /// rollback 操作通过这个传入 `"rollback"`,前端 history 列表按 action 区分图标/颜色。
    pub fn insert_update_audit_with_action(
        &self,
        id: &str,
        admin_id: &str,
        from_version: &str,
        to_version: &str,
        channel: &str,
        action: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO update_audit_log
                (id, admin_id, from_version, to_version, channel, started_at, outcome, action)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'in_progress', ?7)",
            params![id, admin_id, from_version, to_version, channel, now, action],
        )?;
        Ok(())
    }

    /// 更新升级记录的最终状态（后台任务完成/失败时调用）。
    pub fn complete_update_audit(
        &self,
        id: &str,
        outcome: &str,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE update_audit_log
             SET outcome = ?2, completed_at = ?3, error = ?4
             WHERE id = ?1",
            params![id, outcome, now, error],
        )?;
        Ok(())
    }

    /// 启动期对账：自更新 / 回滚成功后进程通过 `std::process::exit(0)` 重启，终态由
    /// watcher 子进程异步写入。两个非终态会被遗留：
    ///   - `in_progress`：apply 在换二进制前就 exit（极少）。
    ///   - `applied_pending_watcher`：apply 已换好二进制、待 watcher 健康确认即写的中间态。
    ///     而 watcher 是 parent 的 detached 子进程，systemd 默认 `KillMode=control-group`，
    ///     parent exit(0) 触发 restart 时整个 cgroup 被清，watcher 往往在写 success/rolled_back
    ///     前即被杀 → 审计永久停在该中间态（升级历史显示「进行中」）。
    /// 失败的升级因 `apply` 返回 Err、进程不退出，已被标 `failed`，不在此列。故启动时
    /// 残留的这两类自更新 / 回滚审计实为「已成功但未记录」，批量收尾为 success。返回收尾行数。
    pub fn reconcile_stale_update_audits(&self) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        let n = conn.execute(
            "UPDATE update_audit_log
             SET outcome = 'success', completed_at = COALESCE(completed_at, ?1)
             WHERE outcome IN ('in_progress', 'applied_pending_watcher')
               AND action IN ('self_update', 'rollback')",
            params![now],
        )?;
        Ok(n)
    }

    /// v1.1-P2.10：写入一条 admin 敏感操作审计记录，一次写入即终态。
    ///
    /// 失败时只 log 警告、不抛错（调用方都用 `let _ = ...` 包），避免审计写入失败
    /// 影响真实业务流程。`metadata` 应为 JSON 兼容的 `serde_json::Value`，None 时
    /// 写空字串占位以保持列对齐。
    pub fn insert_admin_audit(
        &self,
        admin_id: &str,
        action: &str,
        target_type: Option<&str>,
        target_id: Option<&str>,
        metadata: Option<&serde_json::Value>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let metadata_str = metadata.map(|m| m.to_string());
        conn.execute(
            "INSERT INTO update_audit_log
                (id, admin_id, from_version, to_version, channel,
                 started_at, completed_at, outcome,
                 action, target_type, target_id, metadata_json)
             VALUES (?1, ?2, '', '', '', ?3, ?3, 'success',
                     ?4, ?5, ?6, ?7)",
            params![
                id,
                admin_id,
                now,
                action,
                target_type,
                target_id,
                metadata_str
            ],
        )?;
        Ok(())
    }

    /// m024:按 (target_type, target_id) 取该目标最近 N 条审计。供 admin
    /// 用户管理 Drawer 的"操作日志" tab(target_type='user', target_id=user.id)。
    pub fn list_admin_audit_for_target(
        &self,
        target_type: &str,
        target_id: &str,
        limit: usize,
    ) -> Result<Vec<UpdateAuditEntry>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, admin_id, from_version, to_version, channel,
                    started_at, completed_at, outcome, error,
                    action, target_type, target_id, metadata_json
             FROM update_audit_log
             WHERE target_type = ?1 AND target_id = ?2
             ORDER BY started_at DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![target_type, target_id, limit as i64], |row| {
            Ok(UpdateAuditEntry {
                id: row.get(0)?,
                admin_id: row.get(1)?,
                from_version: row.get(2)?,
                to_version: row.get(3)?,
                channel: row.get(4)?,
                started_at: row.get(5)?,
                completed_at: row.get(6)?,
                outcome: row.get(7)?,
                error: row.get(8)?,
                action: row.get(9)?,
                target_type: row.get(10)?,
                target_id: row.get(11)?,
                metadata_json: row.get(12)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// P1：全局 admin 审计查询（`GET /api/admin/audit-log`）。
    /// 区别于 `list_update_audit`（仅 self_update/rollback）与 `list_admin_audit_for_target`
    /// （按单一目标）：返回**全表**，按动态 WHERE 过滤 + 倒序分页，并附总数。
    pub fn list_admin_audit_global(
        &self,
        filter: &AdminAuditFilter,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<UpdateAuditEntry>, u64), StoreError> {
        let conn = self.conn()?;
        let mut where_parts: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        // 动态条件：每命中一个过滤项就追加 "col 比较 ?N"（N=已有参数数+1）+ 对应参数。
        let conditions: [(&str, &Option<String>); 6] = [
            ("admin_id =", &filter.admin_id),
            ("action =", &filter.action),
            ("target_type =", &filter.target_type),
            ("target_id =", &filter.target_id),
            ("started_at >=", &filter.since),
            ("started_at <=", &filter.until),
        ];
        for (expr, val) in conditions {
            if let Some(v) = val.clone() {
                let n = params.len() + 1;
                where_parts.push(format!("{expr} ?{n}"));
                params.push(Box::new(v));
            }
        }
        let where_sql = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };

        let count_sql = format!("SELECT COUNT(*) FROM update_audit_log {where_sql}");
        let total: i64 = conn.query_row(
            &count_sql,
            rusqlite::params_from_iter(params.iter().map(|b| b.as_ref())),
            |r| r.get(0),
        )?;

        let limit_pos = params.len() + 1;
        let offset_pos = params.len() + 2;
        let list_sql = format!(
            "SELECT id, admin_id, from_version, to_version, channel,
                    started_at, completed_at, outcome, error,
                    action, target_type, target_id, metadata_json
             FROM update_audit_log {where_sql}
             ORDER BY started_at DESC
             LIMIT ?{limit_pos} OFFSET ?{offset_pos}"
        );
        params.push(Box::new(limit));
        params.push(Box::new(offset));

        let mut stmt = conn.prepare(&list_sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params.iter().map(|b| b.as_ref())),
            |row| {
                Ok(UpdateAuditEntry {
                    id: row.get(0)?,
                    admin_id: row.get(1)?,
                    from_version: row.get(2)?,
                    to_version: row.get(3)?,
                    channel: row.get(4)?,
                    started_at: row.get(5)?,
                    completed_at: row.get(6)?,
                    outcome: row.get(7)?,
                    error: row.get(8)?,
                    action: row.get(9)?,
                    target_type: row.get(10)?,
                    target_id: row.get(11)?,
                    metadata_json: row.get(12)?,
                })
            },
        )?;
        let items = rows.collect::<Result<Vec<_>, _>>()?;
        Ok((items, total as u64))
    }

    /// 返回最近 N 条升级记录（按 started_at 倒序）。
    /// 列出「版本历史」：仅自更新 / 回滚（action IN self_update/rollback）。
    /// 通用 admin 审计行（资源包热更 / 封禁 / 改密等，from/to 版本为空）不在此返回——
    /// 它们经 `insert_admin_audit` 写入同表，但应走 `list_admin_audit_for_target` 按目标查看，
    /// 不混入升级历史（否则前端版本列会渲染成空白「→」）。
    pub fn list_update_audit(&self, limit: usize) -> Result<Vec<UpdateAuditEntry>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, admin_id, from_version, to_version, channel,
                    started_at, completed_at, outcome, error,
                    action, target_type, target_id, metadata_json
             FROM update_audit_log
             WHERE action IN ('self_update', 'rollback')
             ORDER BY started_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(UpdateAuditEntry {
                id: row.get(0)?,
                admin_id: row.get(1)?,
                from_version: row.get(2)?,
                to_version: row.get(3)?,
                channel: row.get(4)?,
                started_at: row.get(5)?,
                completed_at: row.get(6)?,
                outcome: row.get(7)?,
                error: row.get(8)?,
                action: row.get(9)?,
                target_type: row.get(10)?,
                target_id: row.get(11)?,
                metadata_json: row.get(12)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let s = Store::open(":memory:", 5000, 1).unwrap();
        crate::store::migrate::run(&s).unwrap();
        s
    }

    #[test]
    fn insert_and_list() {
        let s = store();
        s.insert_update_audit("id-1", "admin-1", "v1.0.0", "v1.1.0", "stable")
            .unwrap();
        let list = s.list_update_audit(10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].outcome, "in_progress");
        assert_eq!(list[0].from_version, "v1.0.0");
        // P2.10：老行 action 默认 self_update
        assert_eq!(list[0].action, "self_update");
        assert!(list[0].target_type.is_none());
    }

    #[test]
    fn complete_updates_outcome() {
        let s = store();
        s.insert_update_audit("id-2", "admin-1", "v1.0.0", "v1.1.0", "stable")
            .unwrap();
        s.complete_update_audit("id-2", "success", None).unwrap();
        let list = s.list_update_audit(10).unwrap();
        assert_eq!(list[0].outcome, "success");
        assert!(list[0].completed_at.is_some());
        assert!(list[0].error.is_none());
    }

    #[test]
    fn complete_with_error() {
        let s = store();
        s.insert_update_audit("id-3", "admin-1", "v1.0.0", "v1.1.0", "beta")
            .unwrap();
        s.complete_update_audit("id-3", "failed", Some("sha256 mismatch"))
            .unwrap();
        let list = s.list_update_audit(10).unwrap();
        assert_eq!(list[0].outcome, "failed");
        assert_eq!(list[0].error.as_deref(), Some("sha256 mismatch"));
    }

    #[test]
    fn list_returns_most_recent_first() {
        let s = store();
        s.insert_update_audit("id-a", "admin-1", "v1.0.0", "v1.1.0", "stable")
            .unwrap();
        // 插入第二条时 started_at 可能相同秒，ORDER BY started_at DESC 结果稳定
        s.insert_update_audit("id-b", "admin-1", "v1.1.0", "v1.2.0", "stable")
            .unwrap();
        let list = s.list_update_audit(1).unwrap();
        assert_eq!(list.len(), 1); // limit=1 只返回最新一条
    }

    /// P2.10：admin 通用审计写入 — 资源包上传 / 切激活 / 用户禁封 / 密码重置等。
    #[test]
    fn insert_admin_audit_writes_full_columns() {
        let s = store();
        let meta = serde_json::json!({"version": "1.2.3", "channel": "stable"});
        s.insert_admin_audit(
            "admin-7",
            "resource_pack.upload",
            Some("resource_pack"),
            Some("wordbook-core"),
            Some(&meta),
        )
        .unwrap();
        // admin 审计行不进升级历史，按目标读回校验列
        let list = s
            .list_admin_audit_for_target("resource_pack", "wordbook-core", 10)
            .unwrap();
        assert_eq!(list.len(), 1);
        let row = &list[0];
        assert_eq!(row.admin_id, "admin-7");
        assert_eq!(row.action, "resource_pack.upload");
        assert_eq!(row.target_type.as_deref(), Some("resource_pack"));
        assert_eq!(row.target_id.as_deref(), Some("wordbook-core"));
        // 终态：outcome=success、completed_at == started_at（同一时刻）
        assert_eq!(row.outcome, "success");
        assert!(row.completed_at.is_some());
        // metadata_json 应能反序列化回原结构
        let parsed: serde_json::Value =
            serde_json::from_str(row.metadata_json.as_deref().unwrap()).unwrap();
        assert_eq!(parsed["version"], "1.2.3");
        assert_eq!(parsed["channel"], "stable");
    }

    /// P2.10：metadata 为 None 时 metadata_json 列写 NULL（不报错）。
    #[test]
    fn insert_admin_audit_allows_null_metadata() {
        let s = store();
        s.insert_admin_audit("admin-8", "user.unban", Some("user"), Some("uid-1"), None)
            .unwrap();
        let list = s
            .list_admin_audit_for_target("user", "uid-1", 10)
            .unwrap();
        assert!(list[0].metadata_json.is_none());
        assert_eq!(list[0].action, "user.unban");
    }

    /// 回归：通用 admin 审计行（无版本）不应混入升级历史 `list_update_audit`，
    /// 否则前端「版本历史」会出现空白「→」行。
    #[test]
    fn list_update_audit_excludes_admin_audit() {
        let s = store();
        s.insert_update_audit("up-1", "admin-1", "v1.0.0", "v1.1.0", "stable")
            .unwrap();
        s.insert_admin_audit(
            "admin-9",
            "resource_pack.upload",
            Some("resource_pack"),
            Some("pk"),
            None,
        )
        .unwrap();
        let list = s.list_update_audit(10).unwrap();
        assert_eq!(list.len(), 1, "admin 审计不应出现在升级历史");
        assert_eq!(list[0].action, "self_update");
    }

    /// 启动对账：残留 in_progress / applied_pending_watcher 的自更新/回滚收尾为 success；
    /// 已终态 / 非升级行不动。
    #[test]
    fn reconcile_marks_stale_in_progress_success() {
        let s = store();
        s.insert_update_audit("up-1", "a", "v1.0.0", "v1.1.0", "stable")
            .unwrap(); // in_progress
        s.insert_update_audit_with_action("rb-1", "a", "v1.1.0", "v1.0.0", "stable", "rollback")
            .unwrap(); // in_progress
        // watcher 被 cgroup 杀掉、停在中间态的真实卡死场景
        s.insert_update_audit("up-pw", "a", "v1.1.0", "v1.2.0", "stable")
            .unwrap();
        s.complete_update_audit("up-pw", "applied_pending_watcher", None)
            .unwrap();
        // 已终态(failed)不应被改回
        s.insert_update_audit("up-2", "a", "v1.1.0", "v1.2.0", "stable")
            .unwrap();
        s.complete_update_audit("up-2", "failed", Some("x")).unwrap();

        let n = s.reconcile_stale_update_audits().unwrap();
        assert_eq!(n, 3, "in_progress×2 + applied_pending_watcher×1 被收尾");

        let list = s.list_update_audit(10).unwrap();
        let by = |id: &str| list.iter().find(|e| e.id == id).unwrap().outcome.clone();
        assert_eq!(by("up-1"), "success");
        assert_eq!(by("rb-1"), "success");
        assert_eq!(by("up-pw"), "success", "applied_pending_watcher 应被收尾");
        assert_eq!(by("up-2"), "failed", "已 failed 的不应被改");
        assert!(list.iter().find(|e| e.id == "up-1").unwrap().completed_at.is_some());
    }
}
