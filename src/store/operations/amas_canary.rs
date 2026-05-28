//! AMAS 灰度发布(canary)配置存储。
//!
//! m022 引入:同一时刻最多一个 active 配置(由 `uq_amas_canary_active` 唯一索引保证)。
//! `set_amas_canary` 先把所有 active=1 置 0,再插入新行;`disable_active_amas_canary`
//! 把当前 active 标记为 0。`get_active_amas_canary` 读 active=1 的那一行。
//!
//! 数据由 admin 端 PUT /api/admin/amas/config/canary 写入,AMAS engine 在
//! `effective_config_for_user(user_id)` 时按 hash(user_id) % 100 < percent 决定走 canary。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

/// 一条灰度配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmasCanaryConfig {
    pub id: i64,
    pub version_hash: String,
    pub percent: u32,
    /// 强制走 canary 的 user_id 列表;与 percent 抽样取并集。
    pub force_user_ids: Vec<String>,
    pub created_at: String,
    pub created_by: String,
}

impl Store {
    /// 设置 active canary 配置:把现有 active 全部 deactivate,再插一条新 active 行。
    /// 单事务,避免短暂"无 active"或"双 active"窗口。
    pub fn set_amas_canary(
        &self,
        version_hash: &str,
        percent: u32,
        force_user_ids: &[String],
        created_by: &str,
    ) -> Result<AmasCanaryConfig, StoreError> {
        if percent > 100 {
            return Err(StoreError::Validation(format!(
                "percent must be 0..=100, got {percent}"
            )));
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE amas_canary_config SET active = 0 WHERE active = 1",
            [],
        )?;
        let force_json = serde_json::to_string(force_user_ids)
            .map_err(StoreError::Serialization)?;
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO amas_canary_config
                (version_hash, percent, force_user_ids, active, created_at, created_by)
             VALUES (?1, ?2, ?3, 1, ?4, ?5)",
            params![version_hash, percent as i64, force_json, now, created_by],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(AmasCanaryConfig {
            id,
            version_hash: version_hash.to_string(),
            percent,
            force_user_ids: force_user_ids.to_vec(),
            created_at: now,
            created_by: created_by.to_string(),
        })
    }

    /// 读当前 active canary;无配置返 None。
    pub fn get_active_amas_canary(&self) -> Result<Option<AmasCanaryConfig>, StoreError> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT id, version_hash, percent, force_user_ids, created_at, created_by
                 FROM amas_canary_config WHERE active = 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)? as u32,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, version_hash, percent, force_json, created_at, created_by)) = row else {
            return Ok(None);
        };
        let force_user_ids: Vec<String> =
            serde_json::from_str(&force_json).unwrap_or_default();
        Ok(Some(AmasCanaryConfig {
            id,
            version_hash,
            percent,
            force_user_ids,
            created_at,
            created_by,
        }))
    }

    /// 关闭 active canary;返回是否真的关掉了一行。
    pub fn disable_active_amas_canary(&self) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE amas_canary_config SET active = 0 WHERE active = 1",
            [],
        )?;
        Ok(affected > 0)
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
    fn set_then_get_returns_latest() {
        let s = store();
        // 先 seed 一个 version 行(canary FK 不会真校验,但 store route 层会)
        let cfg1 = s
            .set_amas_canary("hash-a", 25, &["user-1".into()], "admin-1")
            .unwrap();
        assert_eq!(cfg1.percent, 25);
        assert_eq!(cfg1.force_user_ids, vec!["user-1".to_string()]);

        let active = s.get_active_amas_canary().unwrap().unwrap();
        assert_eq!(active.version_hash, "hash-a");
        assert_eq!(active.percent, 25);
    }

    #[test]
    fn set_overrides_previous_active() {
        let s = store();
        s.set_amas_canary("hash-a", 10, &[], "admin-1").unwrap();
        s.set_amas_canary("hash-b", 50, &[], "admin-2").unwrap();
        let active = s.get_active_amas_canary().unwrap().unwrap();
        assert_eq!(active.version_hash, "hash-b");
        assert_eq!(active.percent, 50);
    }

    #[test]
    fn disable_clears_active() {
        let s = store();
        s.set_amas_canary("hash-a", 10, &[], "admin-1").unwrap();
        assert!(s.get_active_amas_canary().unwrap().is_some());
        let was = s.disable_active_amas_canary().unwrap();
        assert!(was);
        assert!(s.get_active_amas_canary().unwrap().is_none());
    }

    #[test]
    fn invalid_percent_rejected() {
        let s = store();
        let err = s.set_amas_canary("h", 150, &[], "admin").unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));
    }
}
