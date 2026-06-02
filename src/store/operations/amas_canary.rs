//! AMAS 灰度发布(canary)配置存储。
//!
//! m022 引入:同一时刻最多一个 active 配置(由 `uq_amas_canary_active` 唯一索引保证)。
//! `set_amas_canary` 先把所有 active=1 置 0,再插入新行;`disable_active_amas_canary`
//! 把当前 active 标记为 0。`get_active_amas_canary` 读 active=1 的那一行。
//!
//! 数据由 admin 端 PUT /api/admin/amas/config/canary 写入,AMAS engine 在
//! `effective_config_for_user(user_id)` 时按 hash(user_id) % 100 < percent 决定走 canary,
//! 并由 `user_matches_crowd_filters` 叠加 m035 人群过滤(平台/账龄/活跃)。

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
    /// m035:灰度发布人群过滤(crowd-filter)原始 JSON;None=无过滤(全量灰度桶)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crowd_filters: Option<serde_json::Value>,
}

/// m035:灰度人群过滤后的可触达设备估算(真实 client_devices 聚合,非伪造)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanaryAudience {
    /// 有 user_id 的设备总数(分母)。
    pub total_users: i64,
    /// 满足全部过滤条件的设备数(分子,过滤后基数)。
    pub eligible_users: i64,
}

impl Store {
    /// 设置 active canary 配置:把现有 active 全部 deactivate,再插一条新 active 行。
    /// 单事务,避免短暂"无 active"或"双 active"窗口。
    /// `crowd_filters` 为发布策略人群过滤 JSON(None=不限)。
    pub fn set_amas_canary(
        &self,
        version_hash: &str,
        percent: u32,
        force_user_ids: &[String],
        crowd_filters: Option<&serde_json::Value>,
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
        let force_json =
            serde_json::to_string(force_user_ids).map_err(StoreError::Serialization)?;
        let filters_json = crowd_filters.map(|v| v.to_string());
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO amas_canary_config
                (version_hash, percent, force_user_ids, active, created_at, created_by, crowd_filters)
             VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6)",
            params![version_hash, percent as i64, force_json, now, created_by, filters_json],
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
            crowd_filters: crowd_filters.cloned(),
        })
    }

    /// 读当前 active canary;无配置返 None。
    pub fn get_active_amas_canary(&self) -> Result<Option<AmasCanaryConfig>, StoreError> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT id, version_hash, percent, force_user_ids, created_at, created_by, crowd_filters
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
                        r.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, version_hash, percent, force_json, created_at, created_by, filters_raw)) =
            row
        else {
            return Ok(None);
        };
        let force_user_ids: Vec<String> = serde_json::from_str(&force_json).unwrap_or_default();
        let crowd_filters = filters_raw
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        Ok(Some(AmasCanaryConfig {
            id,
            version_hash,
            percent,
            force_user_ids,
            created_at,
            created_by,
            crowd_filters,
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

    /// m035:按人群过滤条件统计 client_devices,估算灰度可触达基数(真实聚合)。
    /// - min_account_age_days:仅纳入 first_seen_at 早于 N 天前的设备(过滤新注册)。
    /// - prefer_active:仅纳入 last_seen_at 近 14 天的设备。
    /// - web_only:仅纳入 platform='web' 的设备。
    pub fn estimate_canary_audience(
        &self,
        min_account_age_days: Option<u32>,
        prefer_active: bool,
        web_only: bool,
    ) -> Result<CanaryAudience, StoreError> {
        let conn = self.conn()?;
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM client_devices WHERE user_id IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        let mut clauses: Vec<String> = vec!["user_id IS NOT NULL".into()];
        if let Some(days) = min_account_age_days {
            // first_seen_at 是注册近似;早于 N 天前 = 账号年龄 ≥ N 天。
            clauses.push(format!("first_seen_at <= datetime('now', '-{days} days')"));
        }
        if prefer_active {
            clauses.push("last_seen_at >= datetime('now', '-14 days')".into());
        }
        if web_only {
            clauses.push("LOWER(platform) = 'web'".into());
        }
        let sql = format!(
            "SELECT COUNT(*) FROM client_devices WHERE {}",
            clauses.join(" AND ")
        );
        let eligible: i64 = conn.query_row(&sql, [], |r| r.get(0))?;
        Ok(CanaryAudience {
            total_users: total,
            eligible_users: eligible,
        })
    }

    /// m035 接线:判定单个 user_id 是否落入 canary crowd_filters 人群。
    /// 过滤维度与 estimate_canary_audience 同源(first_seen_at 账龄 / last_seen_at 活跃 /
    /// platform=web)。该用户名下任一设备满足全部条件即算命中(EXISTS 语义)。
    /// `filters` 为持久化的原始 JSON;None/无人群字段 → true(不限)。autoScale24h 为运维
    /// 标记非人群条件,不参与判定。查询出错由调用方决定回退策略。
    pub fn user_matches_crowd_filters(
        &self,
        user_id: &str,
        filters: &serde_json::Value,
    ) -> Result<bool, StoreError> {
        let min_age = filters.get("minAccountAgeDays").and_then(|v| v.as_u64());
        let prefer_active = filters
            .get("preferActive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let web_only = filters
            .get("webOnly")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if min_age.is_none() && !prefer_active && !web_only {
            return Ok(true);
        }
        let mut clauses: Vec<String> = vec!["user_id = ?1".into()];
        if let Some(days) = min_age {
            clauses.push(format!("first_seen_at <= datetime('now', '-{days} days')"));
        }
        if prefer_active {
            clauses.push("last_seen_at >= datetime('now', '-14 days')".into());
        }
        if web_only {
            clauses.push("LOWER(platform) = 'web'".into());
        }
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM client_devices WHERE {})",
            clauses.join(" AND ")
        );
        let conn = self.conn()?;
        let hit: i64 = conn.query_row(&sql, params![user_id], |r| r.get(0))?;
        Ok(hit != 0)
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
            .set_amas_canary("hash-a", 25, &["user-1".into()], None, "admin-1")
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
        s.set_amas_canary("hash-a", 10, &[], None, "admin-1")
            .unwrap();
        s.set_amas_canary("hash-b", 50, &[], None, "admin-2")
            .unwrap();
        let active = s.get_active_amas_canary().unwrap().unwrap();
        assert_eq!(active.version_hash, "hash-b");
        assert_eq!(active.percent, 50);
    }

    #[test]
    fn disable_clears_active() {
        let s = store();
        s.set_amas_canary("hash-a", 10, &[], None, "admin-1")
            .unwrap();
        assert!(s.get_active_amas_canary().unwrap().is_some());
        let was = s.disable_active_amas_canary().unwrap();
        assert!(was);
        assert!(s.get_active_amas_canary().unwrap().is_none());
    }

    #[test]
    fn invalid_percent_rejected() {
        let s = store();
        let err = s.set_amas_canary("h", 150, &[], None, "admin").unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));
    }
}
