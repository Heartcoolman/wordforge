//! AMAS LLM 调参白名单的 Store 层(C4)。
//!
//! `amas_tuning_whitelist` 表替代 const `TIER_A_WHITELIST`:启动时若空则 seed 自 const,
//! 之后 admin 可经 /advisor/whitelist 增删。validate_patch / llm_advisor build_system_prompt
//! 改为从本表读(const 仅作 seed 源 + fallback)。

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::amas::tuning_whitelist::TIER_A_WHITELIST;
use crate::store::{Store, StoreError};

/// 一条白名单条目。camelCase 序列化:path / minSafe / maxSafe。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhitelistRow {
    pub path: String,
    pub min_safe: f64,
    pub max_safe: f64,
}

impl Store {
    /// 列出全部白名单条目,按 path 升序。
    pub fn list_tuning_whitelist(&self) -> Result<Vec<WhitelistRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT path, min_safe, max_safe FROM amas_tuning_whitelist ORDER BY path ASC",
        )?;
        let rows: Result<Vec<_>, _> = stmt
            .query_map([], |r| {
                Ok(WhitelistRow {
                    path: r.get::<_, String>(0)?,
                    min_safe: r.get::<_, f64>(1)?,
                    max_safe: r.get::<_, f64>(2)?,
                })
            })?
            .collect();
        Ok(rows?)
    }

    /// 新增/覆盖一条白名单条目(upsert by path)。min_safe < max_safe,否则 Validation。
    pub fn insert_tuning_whitelist(
        &self,
        path: &str,
        min_safe: f64,
        max_safe: f64,
        created_by: &str,
    ) -> Result<WhitelistRow, StoreError> {
        if min_safe >= max_safe {
            return Err(StoreError::Validation(format!(
                "min_safe ({min_safe}) must be < max_safe ({max_safe})"
            )));
        }
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO amas_tuning_whitelist (path, min_safe, max_safe, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET min_safe=?2, max_safe=?3",
            params![path, min_safe, max_safe, now, created_by],
        )?;
        Ok(WhitelistRow {
            path: path.to_string(),
            min_safe,
            max_safe,
        })
    }

    /// 删除一条白名单条目;返回是否真的删掉一行。
    pub fn delete_tuning_whitelist(&self, path: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "DELETE FROM amas_tuning_whitelist WHERE path = ?1",
            params![path],
        )?;
        Ok(affected > 0)
    }

    /// 若表为空,用 TIER_A_WHITELIST seed(created_by='system')。返回 seed 进的条数(已有则 0)。
    pub fn seed_tuning_whitelist_if_empty(&self) -> Result<usize, StoreError> {
        let mut conn = self.conn()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM amas_tuning_whitelist", [], |r| {
            r.get(0)
        })?;
        if count > 0 {
            return Ok(0);
        }
        let now = chrono::Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        for e in TIER_A_WHITELIST {
            tx.execute(
                "INSERT OR IGNORE INTO amas_tuning_whitelist
                    (path, min_safe, max_safe, created_at, created_by)
                 VALUES (?1, ?2, ?3, ?4, 'system')",
                params![e.path, e.min_safe, e.max_safe, now],
            )?;
        }
        tx.commit()?;
        Ok(TIER_A_WHITELIST.len())
    }
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
    fn empty_then_seed_loads_fifteen() {
        let s = store();
        assert!(s.list_tuning_whitelist().unwrap().is_empty());
        let n = s.seed_tuning_whitelist_if_empty().unwrap();
        assert_eq!(n, 15);
        let rows = s.list_tuning_whitelist().unwrap();
        assert_eq!(rows.len(), 15);
        // seed 内容对齐 const(任取一条核对)
        let ret = rows
            .iter()
            .find(|r| r.path == "memoryModel.baseDesiredRetention")
            .expect("must contain baseDesiredRetention");
        assert!((ret.min_safe - 0.75).abs() < 1e-9);
        assert!((ret.max_safe - 0.95).abs() < 1e-9);
    }

    #[test]
    fn seed_is_idempotent() {
        let s = store();
        assert_eq!(s.seed_tuning_whitelist_if_empty().unwrap(), 15);
        // 二次 seed 不重复插入
        assert_eq!(s.seed_tuning_whitelist_if_empty().unwrap(), 0);
        assert_eq!(s.list_tuning_whitelist().unwrap().len(), 15);
    }

    #[test]
    fn insert_then_list_includes_new_path() {
        let s = store();
        let row = s
            .insert_tuning_whitelist("memoryModel.w[5]", 0.1, 2.0, "admin-1")
            .unwrap();
        assert_eq!(row.path, "memoryModel.w[5]");
        let list = s.list_tuning_whitelist().unwrap();
        assert!(list.iter().any(|r| r.path == "memoryModel.w[5]"));
    }

    #[test]
    fn insert_upserts_existing_path() {
        let s = store();
        s.insert_tuning_whitelist("memoryModel.w[5]", 0.1, 2.0, "admin-1")
            .unwrap();
        s.insert_tuning_whitelist("memoryModel.w[5]", 0.2, 3.0, "admin-2")
            .unwrap();
        let list = s.list_tuning_whitelist().unwrap();
        let hit = list.iter().find(|r| r.path == "memoryModel.w[5]").unwrap();
        assert!((hit.min_safe - 0.2).abs() < 1e-9);
        assert!((hit.max_safe - 3.0).abs() < 1e-9);
        // 仍只有一行
        assert_eq!(
            list.iter().filter(|r| r.path == "memoryModel.w[5]").count(),
            1
        );
    }

    #[test]
    fn insert_rejects_inverted_range() {
        let s = store();
        let err = s
            .insert_tuning_whitelist("memoryModel.w[5]", 2.0, 1.0, "admin")
            .unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));
    }

    #[test]
    fn delete_returns_true_then_false() {
        let s = store();
        s.insert_tuning_whitelist("memoryModel.w[5]", 0.1, 2.0, "admin")
            .unwrap();
        assert!(s.delete_tuning_whitelist("memoryModel.w[5]").unwrap());
        assert!(!s.delete_tuning_whitelist("memoryModel.w[5]").unwrap());
    }

    #[test]
    fn whitelist_row_serializes_camel_case() {
        let row = WhitelistRow {
            path: "memoryModel.w[0]".into(),
            min_safe: 0.05,
            max_safe: 3.0,
        };
        let v = serde_json::to_value(&row).unwrap();
        assert!(v.get("minSafe").is_some());
        assert!(v.get("maxSafe").is_some());
        assert!(v.get("path").is_some());
    }
}
