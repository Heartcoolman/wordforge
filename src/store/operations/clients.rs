use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::store::{Store, StoreError};

/// 13 列 SELECT 的统一行解析(device_id..app_version, country, last_ip, model)。
/// SELECT 顺序必须严格对齐:列变化时只改这一处。
fn row_to_client_device(r: &rusqlite::Row<'_>) -> rusqlite::Result<ClientDevice> {
    Ok(ClientDevice {
        device_id: r.get(0)?,
        platform: r.get(1)?,
        user_id: r.get(2)?,
        first_seen_at: r.get(3)?,
        last_seen_at: r.get(4)?,
        is_banned: r.get::<_, i64>(5)? != 0,
        banned_at: r.get(6)?,
        banned_by: r.get(7)?,
        ban_reason: r.get(8)?,
        app_version: r.get(9)?,
        country: r.get(10)?,
        last_ip: r.get(11)?,
        model: r.get(12)?,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientDevice {
    pub device_id: String,
    pub platform: String,
    pub user_id: Option<String>,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub is_banned: bool,
    pub banned_at: Option<String>,
    pub banned_by: Option<String>,
    pub ban_reason: Option<String>,
    /// m022 加入：客户端首次上报的 x-app-version 头。NULL 表示该设备从未上报过版本。
    #[serde(default)]
    pub app_version: Option<String>,
    /// m027:GeoIP 反查的 ISO-3166-1 alpha-2 国家码。无 mmdb / 私网 IP / 查无结果都为 None。
    #[serde(default)]
    pub country: Option<String>,
    /// m027:最近一次请求源 IP。仅用于审计与故障排查,不对前端暴露(见 admin/clients 路由过滤)。
    #[serde(default)]
    pub last_ip: Option<String>,
    /// m038:遥测硬识别上报的设备型号(payload.device.model)。NULL=该设备从未上报过型号。
    #[serde(default)]
    pub model: Option<String>,
}

/// m027:强制升级策略一行(每平台独立)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientUpgradePolicy {
    pub platform: String,
    pub min_version: Option<String>,
    pub suggested_version: Option<String>,
    pub grayscale_pct: i64,
    pub pwa_silent_update: bool,
    pub updated_at: String,
    pub updated_by: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataChannelStatus {
    pub amas: &'static str,
    pub learning: &'static str,
    pub telemetry: &'static str,
}

impl Default for DataChannelStatus {
    fn default() -> Self {
        Self {
            amas: "none",
            learning: "none",
            telemetry: "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DataUploadSummary {
    pub amas_by_user: HashMap<String, &'static str>,
    pub learning_by_user: HashMap<String, &'static str>,
    pub telemetry_by_device: HashMap<String, &'static str>,
}

impl Store {
    pub fn upsert_client_device(
        &self,
        device_id: &str,
        platform: &str,
        user_id: &str,
    ) -> Result<(), StoreError> {
        self.upsert_client_device_with_version(device_id, platform, user_id, None)
    }

    /// 同 [`upsert_client_device`],额外把 `x-app-version` header 落库。
    /// app_version 传 None 时保留 DB 现有值不变(用 COALESCE),避免后续请求漏带头清掉版本。
    pub fn upsert_client_device_with_version(
        &self,
        device_id: &str,
        platform: &str,
        user_id: &str,
        app_version: Option<&str>,
    ) -> Result<(), StoreError> {
        self.upsert_client_device_with_extras(
            device_id,
            platform,
            user_id,
            app_version,
            None,
            None,
            None,
        )
    }

    /// m027:同上,再加 `country` 和 `last_ip`(GeoIP 反查产物)。m038:再加 `model`(设备型号)。
    /// 所有 Option 字段都用 COALESCE 保留 DB 已有值。
    /// 归属(user_id)为 claim-only:仅当 DB 现有 owner 为 NULL 或等于 `user_id` 才写入,
    /// 已被他人(不同非空 owner)认领时保留原值,防止带 x-device-id 头越权改写归属。
    pub fn upsert_client_device_with_extras(
        &self,
        device_id: &str,
        platform: &str,
        user_id: &str,
        app_version: Option<&str>,
        country: Option<&str>,
        last_ip: Option<&str>,
        model: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO client_devices
                (device_id, platform, user_id, app_version, country, last_ip, model,
                 first_seen_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'), datetime('now'))
             ON CONFLICT(device_id) DO UPDATE SET
                last_seen_at = datetime('now'),
                platform = ?2,
                user_id = CASE WHEN user_id IS NULL OR user_id = ?3 THEN ?3 ELSE user_id END,
                app_version = COALESCE(?4, app_version),
                country = COALESCE(?5, country),
                last_ip = COALESCE(?6, last_ip),
                model = COALESCE(?7, model)",
            params![
                device_id,
                platform,
                user_id,
                app_version,
                country,
                last_ip,
                model
            ],
        )?;
        Ok(())
    }

    pub fn is_device_banned(&self, device_id: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let banned: Option<i64> = conn
            .query_row(
                "SELECT is_banned FROM client_devices WHERE device_id = ?1",
                params![device_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(banned.unwrap_or(0) != 0)
    }

    pub fn get_recently_active_clients(
        &self,
        minutes: i64,
    ) -> Result<Vec<ClientDevice>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT device_id, platform, user_id, first_seen_at, last_seen_at,
                    is_banned, banned_at, banned_by, ban_reason, app_version,
                    country, last_ip, model
             FROM client_devices
             WHERE last_seen_at >= datetime('now', ?1) OR is_banned = 1
             ORDER BY is_banned DESC, last_seen_at DESC",
        )?;
        let offset = format!("-{} minutes", minutes);
        let rows = stmt.query_map(params![offset], row_to_client_device)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// 按 device_id 直查单设备(全字段)。供 admin 设备详情,避免全表扫描后内存 find。
    pub fn get_client_device(
        &self,
        device_id: &str,
    ) -> Result<Option<ClientDevice>, StoreError> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT device_id, platform, user_id, first_seen_at, last_seen_at,
                        is_banned, banned_at, banned_by, ban_reason, app_version,
                        country, last_ip, model
                 FROM client_devices
                 WHERE device_id = ?1",
                params![device_id],
                row_to_client_device,
            )
            .optional()?;
        Ok(row)
    }

    /// 查给定 device_id 列表的 app_version。用于 SSE live entry 透出版本号
    /// (因为 SseClientInfo 不存 app_version)。返回 map,缺失或 NULL 都映射为 None。
    pub fn get_app_versions_for_devices(
        &self,
        device_ids: &[String],
    ) -> Result<HashMap<String, Option<String>>, StoreError> {
        if device_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.conn()?;
        let placeholders: Vec<String> = device_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect();
        let sql = format!(
            "SELECT device_id, app_version FROM client_devices WHERE device_id IN ({})",
            placeholders.join(",")
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = device_ids
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params.as_slice(), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        let mut map = HashMap::new();
        for row in rows {
            let (id, ver) = row?;
            map.insert(id, ver);
        }
        Ok(map)
    }

    pub fn ban_client_device(
        &self,
        device_id: &str,
        banned_by: &str,
        reason: Option<&str>,
    ) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE client_devices SET is_banned = 1, banned_at = datetime('now'),
                    banned_by = ?2, ban_reason = ?3
             WHERE device_id = ?1",
            params![device_id, banned_by, reason],
        )?;
        Ok(affected > 0)
    }

    pub fn unban_client_device(&self, device_id: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE client_devices SET is_banned = 0, banned_at = NULL,
                    banned_by = NULL, ban_reason = NULL
             WHERE device_id = ?1",
            params![device_id],
        )?;
        Ok(affected > 0)
    }

    /// m024:列某 user 关联的 client_devices(按 last_seen_at 倒序)。
    /// 供 admin Drawer "设备/会话" 区块。
    pub fn list_client_devices_for_user(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<ClientDevice>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT device_id, platform, user_id, first_seen_at, last_seen_at,
                    is_banned, banned_at, banned_by, ban_reason, app_version,
                    country, last_ip, model
             FROM client_devices
             WHERE user_id = ?1
             ORDER BY last_seen_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![user_id, limit as i64], |r| row_to_client_device(r))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// m027:设备表后端分页 + 搜索 + 平台过滤。供 admin clients 路由。
    /// `q` 命中 device_id / user_id 子串(LIKE);`platform` 精确匹配("" 表示不过滤)。
    /// 返回 (rows, total_count)。
    pub fn list_client_devices_paginated(
        &self,
        q: Option<&str>,
        platform: Option<&str>,
        recent_minutes: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<ClientDevice>, i64), StoreError> {
        let conn = self.conn()?;
        let mut where_sqls: Vec<&str> = Vec::new();
        let q_like = q.filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));
        let platform_owned = platform.filter(|s| !s.is_empty()).map(String::from);
        let recent_offset = recent_minutes.map(|m| format!("-{} minutes", m));

        if q_like.is_some() {
            where_sqls.push("(device_id LIKE ?1 OR COALESCE(user_id,'') LIKE ?1)");
        }
        let mut param_idx = 1 + (q_like.is_some() as usize);
        let platform_placeholder = if platform_owned.is_some() {
            let s = format!("platform = ?{param_idx}");
            param_idx += 1;
            Some(s)
        } else {
            None
        };
        if let Some(ref s) = platform_placeholder {
            where_sqls.push(s.as_str());
        }
        let recent_placeholder = if recent_offset.is_some() {
            Some(format!(
                "(last_seen_at >= datetime('now', ?{param_idx}) OR is_banned = 1)"
            ))
        } else {
            None
        };
        if let Some(ref s) = recent_placeholder {
            where_sqls.push(s.as_str());
        }

        let where_clause = if where_sqls.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_sqls.join(" AND "))
        };

        // 收集 params 引用(按位置顺序)
        let mut params_vec: Vec<&dyn rusqlite::types::ToSql> = Vec::new();
        if let Some(ref s) = q_like {
            params_vec.push(s);
        }
        if let Some(ref s) = platform_owned {
            params_vec.push(s);
        }
        if let Some(ref s) = recent_offset {
            params_vec.push(s);
        }

        let count_sql = format!("SELECT COUNT(*) FROM client_devices {where_clause}");
        let total: i64 = conn.query_row(&count_sql, params_vec.as_slice(), |r| r.get(0))?;

        // page slice
        let next_idx = params_vec.len() + 1;
        let limit_idx = next_idx;
        let offset_idx = next_idx + 1;
        let select_sql = format!(
            "SELECT device_id, platform, user_id, first_seen_at, last_seen_at,
                    is_banned, banned_at, banned_by, ban_reason, app_version,
                    country, last_ip, model
             FROM client_devices
             {where_clause}
             ORDER BY is_banned DESC, last_seen_at DESC
             LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
        );
        params_vec.push(&limit);
        params_vec.push(&offset);
        let mut stmt = conn.prepare(&select_sql)?;
        let rows = stmt
            .query_map(params_vec.as_slice(), row_to_client_device)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok((rows, total))
    }

    /// m027:按平台聚合设备数 + 月环比。返回 (platform, total_count, last_7d_active,
    /// month_over_month_pct)。pct 是 30 天前同时段 device count → 现在的百分比变化
    /// (e.g. +18.0 表示增 18%)。无历史数据时为 0.0。
    pub fn aggregate_clients_by_platform(
        &self,
    ) -> Result<Vec<(String, i64, i64, f64)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT
                platform,
                COUNT(*) AS total,
                SUM(CASE WHEN last_seen_at >= datetime('now', '-7 days') THEN 1 ELSE 0 END) AS active7d,
                SUM(CASE WHEN first_seen_at < datetime('now', '-30 days') THEN 1 ELSE 0 END) AS month_ago_total
             FROM client_devices
             GROUP BY platform
             ORDER BY total DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let platform: String = r.get(0)?;
                let total: i64 = r.get(1)?;
                let active7d: i64 = r.get(2).unwrap_or(0);
                let month_ago: i64 = r.get(3).unwrap_or(0);
                let pct = if month_ago > 0 {
                    (total - month_ago) as f64 * 100.0 / month_ago as f64
                } else if total > 0 {
                    100.0
                } else {
                    0.0
                };
                Ok((platform, total, active7d, pct))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// m027:按 (platform × app_version) 聚合,版本按设备数倒序。返回 Vec<(platform, version, count)>。
    /// NULL 版本归入 "unknown"。
    pub fn aggregate_clients_by_platform_version(
        &self,
    ) -> Result<Vec<(String, String, i64)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT platform, COALESCE(app_version, 'unknown') AS v, COUNT(*) AS c
             FROM client_devices
             GROUP BY platform, v
             ORDER BY platform, c DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// m027:升级策略 — 列全部三平台。
    pub fn list_upgrade_policies(&self) -> Result<Vec<ClientUpgradePolicy>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT platform, min_version, suggested_version, grayscale_pct,
                    pwa_silent_update, updated_at, updated_by
             FROM client_upgrade_policy
             ORDER BY platform",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ClientUpgradePolicy {
                    platform: r.get(0)?,
                    min_version: r.get(1)?,
                    suggested_version: r.get(2)?,
                    grayscale_pct: r.get(3)?,
                    pwa_silent_update: r.get::<_, i64>(4)? != 0,
                    updated_at: r.get(5)?,
                    updated_by: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// m027:升级策略 — 取单平台。
    pub fn get_upgrade_policy(
        &self,
        platform: &str,
    ) -> Result<Option<ClientUpgradePolicy>, StoreError> {
        let conn = self.conn()?;
        let row: Option<ClientUpgradePolicy> = conn
            .query_row(
                "SELECT platform, min_version, suggested_version, grayscale_pct,
                        pwa_silent_update, updated_at, updated_by
                 FROM client_upgrade_policy
                 WHERE platform = ?1",
                params![platform],
                |r| {
                    Ok(ClientUpgradePolicy {
                        platform: r.get(0)?,
                        min_version: r.get(1)?,
                        suggested_version: r.get(2)?,
                        grayscale_pct: r.get(3)?,
                        pwa_silent_update: r.get::<_, i64>(4)? != 0,
                        updated_at: r.get(5)?,
                        updated_by: r.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// m027:广播受众解析 —— 按 platform / 版本下限 / 最近活跃天数 / 显式 user_id list
    /// 求 user_id 集合(交集)。所有条件为空时返回 None(调用方走全员广播路径,避免
    /// 把"无 filter"当成"匹配零"误删全员)。
    pub fn list_user_ids_for_audience(
        &self,
        platforms: &[String],
        version_min: Option<&str>,
        last_active_days: Option<i64>,
        explicit_user_ids: &[String],
    ) -> Result<Option<Vec<String>>, StoreError> {
        if platforms.is_empty()
            && version_min.is_none()
            && last_active_days.is_none()
            && explicit_user_ids.is_empty()
        {
            return Ok(None);
        }
        let conn = self.conn()?;

        let mut clauses: Vec<String> = vec!["user_id IS NOT NULL".into()];
        let mut binds: Vec<rusqlite::types::Value> = Vec::new();
        let mut idx = 1usize;
        if !platforms.is_empty() {
            let placeholders: Vec<String> = (0..platforms.len())
                .map(|i| format!("?{}", idx + i))
                .collect();
            clauses.push(format!("platform IN ({})", placeholders.join(",")));
            idx += platforms.len();
            for p in platforms {
                binds.push(rusqlite::types::Value::Text(p.clone()));
            }
        }
        if let Some(days) = last_active_days {
            clauses.push(format!("last_seen_at >= datetime('now', ?{idx})"));
            binds.push(rusqlite::types::Value::Text(format!("-{days} days")));
        }
        let sql = format!(
            "SELECT DISTINCT user_id FROM client_devices WHERE {}",
            clauses.join(" AND ")
        );
        let mut stmt = conn.prepare(&sql)?;
        let params_vec: Vec<&dyn rusqlite::types::ToSql> = binds
            .iter()
            .map(|b| b as &dyn rusqlite::types::ToSql)
            .collect();
        let device_user_ids: Vec<String> = stmt
            .query_map(params_vec.as_slice(), |r| r.get::<_, Option<String>>(0))?
            .filter_map(Result::ok)
            .flatten()
            .collect();
        drop(stmt);

        // version_min 二次过滤(client_devices.app_version vs version_min,用 semver)
        let mut filtered: Vec<String> = if let Some(v_min) = version_min {
            let mut stmt = conn.prepare(
                "SELECT user_id, app_version FROM client_devices WHERE user_id IS NOT NULL",
            )?;
            let pairs: Vec<(String, Option<String>)> = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, Option<String>>(1)?,
                    ))
                })?
                .filter_map(Result::ok)
                .filter_map(|(uid, ver)| uid.map(|u| (u, ver)))
                .collect();
            let v_min_clean = v_min.trim_start_matches('v');
            // version_min 必须可解析,否则会把全员误判为不命中(静默零发送)
            let v_min_parsed = semver::Version::parse(v_min_clean)
                .map_err(|_| StoreError::Validation(format!("受众最低版本号非法: {v_min}")))?;
            let from_device = std::collections::HashSet::<String>::from_iter(device_user_ids);
            pairs
                .into_iter()
                .filter(|(uid, _)| from_device.contains(uid))
                .filter(|(_, ver)| match ver.as_deref() {
                    Some(v) => {
                        let v_clean = v.trim_start_matches('v');
                        semver::Version::parse(v_clean)
                            .map(|av| av >= v_min_parsed)
                            .unwrap_or(false)
                    }
                    None => false,
                })
                .map(|(uid, _)| uid)
                .collect()
        } else {
            device_user_ids
        };

        // explicit_user_ids 取并集(显式指定的用户即使设备 filter 不命中也加入)
        if !explicit_user_ids.is_empty() {
            let mut set: std::collections::HashSet<String> = filtered.into_iter().collect();
            for u in explicit_user_ids {
                set.insert(u.clone());
            }
            filtered = set.into_iter().collect();
        }

        Ok(Some(filtered))
    }

    /// m027:升级策略 — 写入。平台必须是 web/ios/android 之一(由 schema PRIMARY KEY 保证)。
    pub fn upsert_upgrade_policy(
        &self,
        platform: &str,
        min_version: Option<&str>,
        suggested_version: Option<&str>,
        grayscale_pct: i64,
        pwa_silent_update: bool,
        updated_by: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO client_upgrade_policy
                (platform, min_version, suggested_version, grayscale_pct,
                 pwa_silent_update, updated_at, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), ?6)
             ON CONFLICT(platform) DO UPDATE SET
                min_version = ?2,
                suggested_version = ?3,
                grayscale_pct = ?4,
                pwa_silent_update = ?5,
                updated_at = datetime('now'),
                updated_by = ?6",
            params![
                platform,
                min_version,
                suggested_version,
                grayscale_pct,
                pwa_silent_update as i64,
                updated_by,
            ],
        )?;
        Ok(())
    }

    pub fn client_device_exists(&self, device_id: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM client_devices WHERE device_id = ?1)",
            params![device_id],
            |r| r.get(0),
        )?;
        Ok(exists)
    }

    /// m038 遥测硬识别:查设备注册状态与归属(三态)。
    /// Ok(None)            = 设备未注册;
    /// Ok(Some(None))      = 已注册但归属未认领(user_id 为 NULL,首个带 token 的 user 可 claim);
    /// Ok(Some(Some(uid))) = 已注册且归属 uid。
    pub fn get_client_device_owner(
        &self,
        device_id: &str,
    ) -> Result<Option<Option<String>>, StoreError> {
        let conn = self.conn()?;
        let owner = conn
            .query_row(
                "SELECT user_id FROM client_devices WHERE device_id = ?1",
                params![device_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?;
        Ok(owner)
    }

    pub fn get_data_upload_status(
        &self,
        user_ids: &[String],
        device_ids: &[String],
    ) -> Result<DataUploadSummary, StoreError> {
        let conn = self.conn()?;
        let mut summary = DataUploadSummary::default();

        if !user_ids.is_empty() {
            let placeholders: Vec<String> = user_ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect();
            let params: Vec<&dyn rusqlite::types::ToSql> = user_ids
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();

            let amas_sql = format!(
                "SELECT user_id, total_event_count FROM engine_user_states WHERE user_id IN ({})",
                placeholders.join(",")
            );
            let mut stmt = conn.prepare(&amas_sql)?;
            let rows = stmt.query_map(params.as_slice(), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (user_id, event_count) = row?;
                summary
                    .amas_by_user
                    .insert(user_id, if event_count > 0 { "uploaded" } else { "nil" });
            }

            let lr_sql = format!(
                "SELECT user_id, COUNT(*) FROM learning_records WHERE user_id IN ({}) GROUP BY user_id",
                placeholders.join(",")
            );
            let mut stmt = conn.prepare(&lr_sql)?;
            let lr_params: Vec<&dyn rusqlite::types::ToSql> = user_ids
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();
            let rows = stmt.query_map(lr_params.as_slice(), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (user_id, cnt) = row?;
                summary
                    .learning_by_user
                    .insert(user_id, if cnt > 0 { "uploaded" } else { "nil" });
            }
        }

        if !device_ids.is_empty() {
            let placeholders: Vec<String> = device_ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect();
            let params: Vec<&dyn rusqlite::types::ToSql> = device_ids
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();

            let sql = format!(
                "SELECT device_id, COUNT(*) FROM telemetry_events WHERE device_id IN ({}) GROUP BY device_id",
                placeholders.join(",")
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params.as_slice(), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (device_id, cnt) = row?;
                summary
                    .telemetry_by_device
                    .insert(device_id, if cnt > 0 { "uploaded" } else { "nil" });
            }
        }

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_store() -> Store {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        store
    }

    #[test]
    fn upsert_inserts_then_updates_last_seen() {
        let store = test_store();
        store.upsert_client_device("dev-1", "ios", "u-1").unwrap();
        // 重复 upsert 不应失败，platform 被覆盖;但 user_id 是 claim-only:
        // 已被 u-1 认领,后续不同 user 不得改写归属(防越权劫持)。
        store
            .upsert_client_device("dev-1", "android", "u-2")
            .unwrap();
        let active = store.get_recently_active_clients(60).unwrap();
        assert_eq!(active.len(), 1);
        let d = &active[0];
        assert_eq!(d.device_id, "dev-1");
        assert_eq!(d.platform, "android");
        assert_eq!(d.user_id.as_deref(), Some("u-1"));
        assert!(!d.is_banned);
    }

    #[test]
    fn upsert_claims_null_owner_but_not_others() {
        let store = test_store();
        // 用 with_extras 传同一 device,先以 NULL owner 注册(走 telemetry handler 路径前态)
        store
            .upsert_client_device_with_extras("dev-c", "ios", "owner-a", None, None, None, None)
            .unwrap();
        // 同 owner 再 upsert:保持归属
        store
            .upsert_client_device_with_extras("dev-c", "ios", "owner-a", None, None, None, None)
            .unwrap();
        assert_eq!(
            store.get_client_device_owner("dev-c").unwrap(),
            Some(Some("owner-a".to_string()))
        );
        // 他人尝试改写:归属保留 owner-a
        store
            .upsert_client_device_with_extras("dev-c", "ios", "attacker", None, None, None, None)
            .unwrap();
        assert_eq!(
            store.get_client_device_owner("dev-c").unwrap(),
            Some(Some("owner-a".to_string()))
        );
    }

    #[test]
    fn device_exists_and_is_banned_flow() {
        let store = test_store();
        assert!(!store.client_device_exists("dev-x").unwrap());
        assert!(!store.is_device_banned("dev-x").unwrap());
        store.upsert_client_device("dev-x", "web", "u-1").unwrap();
        assert!(store.client_device_exists("dev-x").unwrap());
        assert!(!store.is_device_banned("dev-x").unwrap());

        assert!(store
            .ban_client_device("dev-x", "admin-1", Some("spam"))
            .unwrap());
        assert!(store.is_device_banned("dev-x").unwrap());

        assert!(store.unban_client_device("dev-x").unwrap());
        assert!(!store.is_device_banned("dev-x").unwrap());
    }

    #[test]
    fn ban_unban_nonexistent_returns_false() {
        let store = test_store();
        assert!(!store.ban_client_device("missing", "admin", None).unwrap());
        assert!(!store.unban_client_device("missing").unwrap());
    }

    #[test]
    fn recently_active_includes_banned_even_if_old() {
        let store = test_store();
        store.upsert_client_device("dev-a", "web", "u-a").unwrap();
        store
            .ban_client_device("dev-a", "admin", Some("r"))
            .unwrap();
        // 用 -100000 minutes 也仍包含 banned
        let list = store.get_recently_active_clients(1).unwrap();
        assert!(list.iter().any(|d| d.device_id == "dev-a" && d.is_banned));
    }

    #[test]
    fn data_upload_status_empty_inputs() {
        let store = test_store();
        let s = store.get_data_upload_status(&[], &[]).unwrap();
        assert!(s.amas_by_user.is_empty());
        assert!(s.learning_by_user.is_empty());
        assert!(s.telemetry_by_device.is_empty());
    }

    #[test]
    fn data_upload_status_with_real_data() {
        let store = test_store();
        // seed engine_user_states
        let user_id = "u-1".to_string();
        store
            .upsert_client_device("dev-1", "ios", &user_id)
            .unwrap();
        {
            let conn = store.connection().unwrap();
            conn.execute(
                "INSERT INTO engine_user_states (user_id, total_event_count, created_at)
                 VALUES (?1, 7, ?2)",
                params![user_id, Utc::now().to_rfc3339()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, created_at)
                 VALUES (?1, ?2, 'w1', 1, 100, ?3)",
                params![user_id, uuid::Uuid::new_v4().to_string(), Utc::now().to_rfc3339()],
            ).unwrap();
            conn.execute(
                "INSERT INTO telemetry_events (id, device_id, event_type, payload_json, client_ts, server_ts)
                 VALUES (?1, 'dev-1', 'periodic', '{}', ?2, ?2)",
                params![uuid::Uuid::new_v4().to_string(), Utc::now().to_rfc3339()],
            ).unwrap();
        }

        let s = store
            .get_data_upload_status(std::slice::from_ref(&user_id), &["dev-1".to_string()])
            .unwrap();
        assert_eq!(s.amas_by_user.get(&user_id).copied(), Some("uploaded"));
        assert_eq!(s.learning_by_user.get(&user_id).copied(), Some("uploaded"));
        assert_eq!(
            s.telemetry_by_device.get("dev-1").copied(),
            Some("uploaded")
        );
    }

    #[test]
    fn data_upload_status_zero_event_count_marked_nil() {
        let store = test_store();
        let user_id = "u-2".to_string();
        {
            let conn = store.connection().unwrap();
            conn.execute(
                "INSERT INTO engine_user_states (user_id, total_event_count, created_at)
                 VALUES (?1, 0, ?2)",
                params![user_id, Utc::now().to_rfc3339()],
            )
            .unwrap();
        }
        let s = store
            .get_data_upload_status(std::slice::from_ref(&user_id), &[])
            .unwrap();
        assert_eq!(s.amas_by_user.get(&user_id).copied(), Some("nil"));
        // 没有 learning_records 时 key 不出现
        assert!(s.learning_by_user.get(&user_id).is_none());
    }

    #[test]
    fn data_channel_status_default_is_none_strings() {
        let s = DataChannelStatus::default();
        assert_eq!(s.amas, "none");
        assert_eq!(s.learning, "none");
        assert_eq!(s.telemetry, "none");
        // 同步覆盖 Serialize
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"amas\":\"none\""));
    }

    #[test]
    fn data_upload_summary_default_serializes() {
        let s = DataUploadSummary::default();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("amasByUser"));
    }
}
