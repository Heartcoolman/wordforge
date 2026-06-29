use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::store::{Store, StoreError};

/// 17 列 SELECT 的统一行解析(device_id..model, risk_flag, risk_reason,
/// risk_flagged_at, risk_related_device)。SELECT 顺序必须严格对齐:列变化时只改这一处。
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
        risk_flag: r.get::<_, i64>(13)? != 0,
        risk_reason: r.get(14)?,
        risk_flagged_at: r.get(15)?,
        risk_related_device: r.get(16)?,
    })
}

/// row_to_client_device 依赖的 17 列 SELECT 列表(顺序与解析严格对齐)。各查询点统一引用,
/// 避免多处手抄列名漂移。
const CLIENT_DEVICE_COLS: &str = "device_id, platform, user_id, first_seen_at, last_seen_at,
        is_banned, banned_at, banned_by, ban_reason, app_version,
        country, last_ip, model, risk_flag, risk_reason, risk_flagged_at, risk_related_device";

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
    /// m054:关联风控标记。某设备被封时,共享出口 IP / 同账号的其它设备自动置 true 供 admin 复核。
    #[serde(default)]
    pub risk_flag: bool,
    /// m054:风控标记原因(人类可读,含触发源设备与命中信号)。
    #[serde(default)]
    pub risk_reason: Option<String>,
    /// m054:打标时间(ISO 8601)。
    #[serde(default)]
    pub risk_flagged_at: Option<String>,
    /// m054:触发本次标记的源被封设备 device_id。
    #[serde(default)]
    pub risk_related_device: Option<String>,
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
        let sql = format!(
            "SELECT {CLIENT_DEVICE_COLS}
             FROM client_devices
             WHERE last_seen_at >= datetime('now', ?1) OR is_banned = 1
             ORDER BY is_banned DESC, last_seen_at DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
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
        let sql =
            format!("SELECT {CLIENT_DEVICE_COLS} FROM client_devices WHERE device_id = ?1");
        let row = conn
            .query_row(&sql, params![device_id], row_to_client_device)
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
        Self::ban_device_on_conn(&conn, device_id, banned_by, reason)
    }

    /// 在给定连接(可为事务)上执行封禁 UPDATE。供单连接事务复用,使"封禁 + 关联打标"原子化。
    fn ban_device_on_conn(
        conn: &rusqlite::Connection,
        device_id: &str,
        banned_by: &str,
        reason: Option<&str>,
    ) -> Result<bool, StoreError> {
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
        Self::unban_device_on_conn(&conn, device_id)
    }

    /// 在给定连接(可为事务)上执行解封 UPDATE。供单连接事务复用。
    fn unban_device_on_conn(
        conn: &rusqlite::Connection,
        device_id: &str,
    ) -> Result<bool, StoreError> {
        let affected = conn.execute(
            "UPDATE client_devices SET is_banned = 0, banned_at = NULL,
                    banned_by = NULL, ban_reason = NULL
             WHERE device_id = ?1",
            params![device_id],
        )?;
        Ok(affected > 0)
    }

    /// 原子封禁:在单个 `BEGIN IMMEDIATE` 事务内执行"封禁 UPDATE + m054 关联打标",
    /// 二者全成功或全回滚。修复此前两步各取独立连接、autocommit 互不在一事务,导致
    /// 关联打标失败时封禁已落库却返回 500 的状态不一致。返回 (是否实际封禁, 被关联打标的设备列表)。
    pub fn ban_device_with_flagging(
        &self,
        device_id: &str,
        banned_by: &str,
        reason: Option<&str>,
    ) -> Result<(bool, Vec<String>), StoreError> {
        self.with_transaction(|conn| {
            let banned = Self::ban_device_on_conn(conn, device_id, banned_by, reason)?;
            let flagged = Self::flag_related_on_conn(conn, device_id)?;
            Ok((banned, flagged))
        })
    }

    /// 原子解封:在单个事务内执行"解封 UPDATE + 重算由该设备触发的关联标记"。
    /// 重算(而非盲清)修复"多个被封设备共同牵连同一设备时,解封其一会错清/漏清"的缺陷:
    /// 仅当被牵连设备不再与任何**其它仍被封**设备共享信号时才清标,否则改指向仍有效的源。
    /// 返回 (是否实际解封, 被清除标记的设备数)。
    pub fn unban_device_with_flag_recompute(
        &self,
        device_id: &str,
    ) -> Result<(bool, usize), StoreError> {
        self.with_transaction(|conn| {
            let unbanned = Self::unban_device_on_conn(conn, device_id)?;
            let cleared = Self::recompute_flags_for_source(conn, device_id)?;
            Ok((unbanned, cleared))
        })
    }

    /// m054(B 层封禁绕过缓解):某设备被封后,自动给"共享出口 IP / 同账号"的其它设备
    /// 打关联风控标记(risk_flag=1)供 admin 复核。**仅标记不硬封**,避免 CGNAT / 共享
    /// 网络 / 公司出口等场景误伤无辜设备。
    ///
    /// 命中信号(任一即标记):last_ip 与被封设备相同(非空)、或 user_id 相同(非空)。
    /// 排除被封设备自身与已封设备;已标记设备刷新原因/时间/触发源。返回被标记的 device_id
    /// 列表。被封设备 IP 与账号都为空时直接返回空(无可关联信号)。
    pub fn flag_related_devices(
        &self,
        banned_device_id: &str,
    ) -> Result<Vec<String>, StoreError> {
        let conn = self.conn()?;
        Self::flag_related_on_conn(&conn, banned_device_id)
    }

    /// 在给定连接(可为事务)上执行关联打标。供 [`ban_device_with_flagging`] 在单事务内复用,
    /// 使 SELECT 候选 + 逐条 UPDATE 整批落在一次写锁内(消除部分提交 + 降低 SQLITE_BUSY)。
    fn flag_related_on_conn(
        conn: &rusqlite::Connection,
        banned_device_id: &str,
    ) -> Result<Vec<String>, StoreError> {
        let signals: Option<(Option<String>, Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT last_ip, user_id, fp_coarse FROM client_devices WHERE device_id = ?1",
                params![banned_device_id],
                |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let (ip, uid, coarse) = match signals {
            Some(v) => v,
            None => return Ok(Vec::new()),
        };
        // 空串等同无信号:防御匿名/历史脏数据把所有 "" 设备误聚成一簇。
        let ip = ip.filter(|s| !s.is_empty());
        let uid = uid.filter(|s| !s.is_empty());
        let coarse = coarse.filter(|s| !s.is_empty());
        if ip.is_none() && uid.is_none() && coarse.is_none() {
            return Ok(Vec::new());
        }

        // NULL 绑定时 `= ?` 求值为 NULL(false),空信号维度自然跳过。
        let mut stmt = conn.prepare(
            "SELECT device_id,
                    (last_ip IS NOT NULL AND last_ip = ?2) AS ip_match,
                    (user_id IS NOT NULL AND user_id = ?3) AS user_match,
                    (fp_coarse IS NOT NULL AND fp_coarse = ?4) AS fp_match
             FROM client_devices
             WHERE device_id != ?1
               AND is_banned = 0
               AND ((last_ip IS NOT NULL AND last_ip = ?2)
                 OR (user_id IS NOT NULL AND user_id = ?3)
                 OR (fp_coarse IS NOT NULL AND fp_coarse = ?4))",
        )?;
        let candidates: Vec<(String, bool, bool, bool)> = stmt
            .query_map(params![banned_device_id, ip, uid, coarse], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)? != 0,
                    r.get::<_, i64>(2)? != 0,
                    r.get::<_, i64>(3)? != 0,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut flagged = Vec::with_capacity(candidates.len());
        for (device_id, ip_match, user_match, fp_match) in candidates {
            let mut parts: Vec<&str> = Vec::new();
            if ip_match {
                parts.push("共享出口 IP");
            }
            if user_match {
                parts.push("同一账号");
            }
            if fp_match {
                parts.push("相同设备指纹(模糊)");
            }
            if parts.is_empty() {
                continue; // WHERE 已过滤,理论不可达
            }
            let reason = format!("关联自被封设备 {banned_device_id}:{}", parts.join("、"));
            conn.execute(
                "UPDATE client_devices
                    SET risk_flag = 1, risk_reason = ?2,
                        risk_flagged_at = datetime('now'), risk_related_device = ?3
                  WHERE device_id = ?1",
                params![device_id, reason, banned_device_id],
            )?;
            flagged.push(device_id);
        }
        Ok(flagged)
    }

    /// m054:清除单设备的关联风控标记(admin 复核判定误报)。
    pub fn clear_device_risk_flag(&self, device_id: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE client_devices
                SET risk_flag = 0, risk_reason = NULL,
                    risk_flagged_at = NULL, risk_related_device = NULL
              WHERE device_id = ?1 AND risk_flag = 1",
            params![device_id],
        )?;
        Ok(affected > 0)
    }

    /// m054:解封某设备时顺带清除由它触发的全部关联标记,避免误封纠正后留悬挂标记。
    /// 返回被清除的设备数。
    pub fn clear_risk_flags_related_to(
        &self,
        source_device_id: &str,
    ) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE client_devices
                SET risk_flag = 0, risk_reason = NULL,
                    risk_flagged_at = NULL, risk_related_device = NULL
              WHERE risk_related_device = ?1 AND risk_flag = 1",
            params![source_device_id],
        )?;
        Ok(affected)
    }

    /// m054 修复:解封某源设备后,重算"由它触发的关联标记"应保留还是清除。
    /// 对每个仍指向该源的被标记设备 X:若 X 仍与**任一其它仍被封**设备共享信号(IP/账号/模糊指纹),
    /// 则改指向那个仍有效的源(刷新原因/时间),保持标记;否则清除标记。返回被清除的设备数。
    /// 调用前须已将 source 自身置为 is_banned=0(故下方 `is_banned=1` 过滤天然排除 source)。
    fn recompute_flags_for_source(
        conn: &rusqlite::Connection,
        source_device_id: &str,
    ) -> Result<usize, StoreError> {
        let mut stmt = conn.prepare(
            "SELECT device_id, last_ip, user_id, fp_coarse
             FROM client_devices
             WHERE risk_related_device = ?1 AND risk_flag = 1",
        )?;
        let affected: Vec<(String, Option<String>, Option<String>, Option<String>)> = stmt
            .query_map(params![source_device_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut cleared = 0usize;
        for (x_id, ip, uid, coarse) in affected {
            let ip = ip.filter(|s| !s.is_empty());
            let uid = uid.filter(|s| !s.is_empty());
            let coarse = coarse.filter(|s| !s.is_empty());
            // 找另一台仍被封、且与 X 共享任一信号的设备(排除 X 自身;source 已 is_banned=0 自然排除)。
            let other: Option<(String, bool, bool, bool)> = conn
                .query_row(
                    "SELECT device_id,
                            (last_ip IS NOT NULL AND last_ip = ?2) AS ip_match,
                            (user_id IS NOT NULL AND user_id = ?3) AS user_match,
                            (fp_coarse IS NOT NULL AND fp_coarse = ?4) AS fp_match
                     FROM client_devices
                     WHERE is_banned = 1 AND device_id != ?1
                       AND ((last_ip IS NOT NULL AND last_ip = ?2)
                         OR (user_id IS NOT NULL AND user_id = ?3)
                         OR (fp_coarse IS NOT NULL AND fp_coarse = ?4))
                     LIMIT 1",
                    params![x_id, ip, uid, coarse],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)? != 0,
                            r.get::<_, i64>(2)? != 0,
                            r.get::<_, i64>(3)? != 0,
                        ))
                    },
                )
                .optional()?;
            match other {
                Some((new_src, ip_match, user_match, fp_match)) => {
                    let mut parts: Vec<&str> = Vec::new();
                    if ip_match {
                        parts.push("共享出口 IP");
                    }
                    if user_match {
                        parts.push("同一账号");
                    }
                    if fp_match {
                        parts.push("相同设备指纹(模糊)");
                    }
                    let reason = format!("关联自被封设备 {new_src}:{}", parts.join("、"));
                    conn.execute(
                        "UPDATE client_devices
                            SET risk_reason = ?2, risk_flagged_at = datetime('now'),
                                risk_related_device = ?3
                          WHERE device_id = ?1",
                        params![x_id, reason, new_src],
                    )?;
                }
                None => {
                    conn.execute(
                        "UPDATE client_devices
                            SET risk_flag = 0, risk_reason = NULL,
                                risk_flagged_at = NULL, risk_related_device = NULL
                          WHERE device_id = ?1",
                        params![x_id],
                    )?;
                    cleared += 1;
                }
            }
        }
        Ok(cleared)
    }

    /// m027 修复:取某平台全部设备的 (device_id, app_version),单次查询一致快照。
    /// 替代 broadcast_upgrade 的 OFFSET 分页循环——后者按易变的 last_seen_at 排序,
    /// 扫描期间设备活跃刷新会使 OFFSET 窗口错位,导致漏推/重复推。低频 admin 操作,全量可接受。
    pub fn list_device_versions_for_platform(
        &self,
        platform: &str,
    ) -> Result<Vec<(String, Option<String>)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT device_id, app_version FROM client_devices WHERE platform = ?1",
        )?;
        let rows = stmt
            .query_map(params![platform], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// m054:列当前被标记关联风险的设备(按打标时间倒序),供 admin 复核面板。
    pub fn list_flagged_devices(&self, limit: i64) -> Result<Vec<ClientDevice>, StoreError> {
        let conn = self.conn()?;
        let sql = format!(
            "SELECT {CLIENT_DEVICE_COLS}
             FROM client_devices
             WHERE risk_flag = 1
             ORDER BY risk_flagged_at DESC
             LIMIT ?1"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit], row_to_client_device)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// m055:列共享同一设备指纹的设备簇(同硬件多账号 / 封禁绕过排查)。
    /// `kind` ∈ {"coarse","strong"} 选 fp_coarse|fp_strong 列,非法值退化为 coarse。
    /// 按簇内设备数倒序,仅返回 count >= `min_count` 的簇。返回 Vec<(指纹, 设备数, deviceIds)>。
    /// 索引 idx_client_devices_fp_coarse_all / _fp_strong_all 已覆盖(m063)。
    pub fn list_fingerprint_collisions(
        &self,
        kind: &str,
        min_count: i64,
        limit: i64,
    ) -> Result<Vec<(String, i64, Vec<String>)>, StoreError> {
        // 列名不能用占位符绑定,只能字符串拼接;故走白名单映射防注入。
        let col = match kind {
            "strong" => "fp_strong",
            _ => "fp_coarse",
        };
        let conn = self.conn()?;
        // 空串等同无信号:与本文件 flag_related_on_conn 一致排除,防历史脏数据把所有
        // '' 设备误聚成一簇(否则直接架空本端点排查多账号/绕过的目的)。
        let sql = format!(
            "SELECT {col}, COUNT(*) AS c, GROUP_CONCAT(device_id) AS ids
             FROM client_devices
             WHERE {col} IS NOT NULL AND {col} != ''
             GROUP BY {col}
             HAVING c >= ?1
             ORDER BY c DESC
             LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![min_count, limit], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let result = rows
            .into_iter()
            .map(|(fp, count, ids)| {
                // GROUP_CONCAT 默认按 ',' 拼接;空段(理论不可达)过滤掉。
                let device_ids: Vec<String> = ids
                    .unwrap_or_default()
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
                (fp, count, device_ids)
            })
            .collect();
        Ok(result)
    }

    /// m055:落库设备浏览器指纹(随请求头来,与 last_ip 同性质)。COALESCE 保留已有值,
    /// 漏带头不清空。设备行须已存在(中间件先 upsert),不存在则 0 行无副作用。
    pub fn update_device_fingerprint(
        &self,
        device_id: &str,
        fp_strong: Option<&str>,
        fp_coarse: Option<&str>,
    ) -> Result<(), StoreError> {
        if fp_strong.is_none() && fp_coarse.is_none() {
            return Ok(());
        }
        let conn = self.conn()?;
        conn.execute(
            "UPDATE client_devices
                SET fp_strong = COALESCE(?2, fp_strong),
                    fp_coarse = COALESCE(?3, fp_coarse)
              WHERE device_id = ?1",
            params![device_id, fp_strong, fp_coarse],
        )?;
        Ok(())
    }

    /// m055:请求路径"是否被封"判定——设备 id 被封 **或** 强指纹命中任一被封设备。
    /// 后者使清缓存/隐私模式/换标签得到的新 device_id 仍被同一台机器的封禁覆盖。
    /// fp_strong 为 None 时退化为纯 device_id 判定。
    pub fn is_client_banned(
        &self,
        device_id: &str,
        fp_strong: Option<&str>,
    ) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let banned: bool = conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM client_devices
                WHERE is_banned = 1
                  AND (device_id = ?1
                    OR (?2 IS NOT NULL AND fp_strong IS NOT NULL AND fp_strong = ?2))
            )",
            params![device_id, fp_strong],
            |r| r.get(0),
        )?;
        Ok(banned)
    }

    /// m055:若本设备的模糊指纹命中某被封设备(同硬件、换浏览器/会话),给本设备打 risk_flag
    /// 供 admin 复核(不硬封,coarse 低熵会撞机型)。返回命中的被封设备 id(无命中=None)。
    /// 本设备已封或已标记则跳过。
    pub fn flag_device_if_coarse_banned(
        &self,
        device_id: &str,
        fp_coarse: &str,
    ) -> Result<Option<String>, StoreError> {
        if fp_coarse.is_empty() {
            return Ok(None);
        }
        let conn = self.conn()?;
        let related: Option<String> = conn
            .query_row(
                "SELECT device_id FROM client_devices
                  WHERE is_banned = 1 AND fp_coarse = ?1 AND device_id != ?2
                  LIMIT 1",
                params![fp_coarse, device_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(related_id) = related else {
            return Ok(None);
        };
        let reason = format!("关联自被封设备 {related_id}:相同设备指纹(模糊)");
        conn.execute(
            "UPDATE client_devices
                SET risk_flag = 1, risk_reason = ?2,
                    risk_flagged_at = datetime('now'), risk_related_device = ?3
              WHERE device_id = ?1 AND is_banned = 0 AND risk_flag = 0",
            params![device_id, reason, related_id],
        )?;
        Ok(Some(related_id))
    }

    /// m024:列某 user 关联的 client_devices(按 last_seen_at 倒序)。
    /// 供 admin Drawer "设备/会话" 区块。
    pub fn list_client_devices_for_user(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<ClientDevice>, StoreError> {
        let conn = self.conn()?;
        let sql = format!(
            "SELECT {CLIENT_DEVICE_COLS}
             FROM client_devices
             WHERE user_id = ?1
             ORDER BY last_seen_at DESC
             LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql)?;
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
            "SELECT {CLIENT_DEVICE_COLS}
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

    /// 设备归属两态计数(Telemetry 看板 ownership)：client_devices 按 user_id 是否为 NULL 分
    /// claimed/unclaimed。返回 (claimed, unclaimed)。mismatch/not_registered 是逐请求摄取结果
    /// (异主行相对请求者、或根本无行)，无法从表派生 → handler 置 null。
    pub fn admin_device_ownership_counts(&self) -> Result<(i64, i64), StoreError> {
        let conn = self.conn()?;
        let row = conn.query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN user_id IS NOT NULL THEN 1 ELSE 0 END), 0) AS claimed,
                COALESCE(SUM(CASE WHEN user_id IS NULL THEN 1 ELSE 0 END), 0) AS unclaimed
             FROM client_devices",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        Ok(row)
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
    fn fingerprint_collisions_groups_shared_fp() {
        let store = test_store();
        store.upsert_client_device("d1", "web", "u1").unwrap();
        store.upsert_client_device("d2", "web", "u2").unwrap();
        store.upsert_client_device("d3", "web", "u3").unwrap();
        // 两台空串 coarse 不得聚成假簇(空串=无信号)。
        store.upsert_client_device("d4", "web", "u4").unwrap();
        store.upsert_client_device("d5", "web", "u5").unwrap();
        // d1/d2 共享 coarse "cA";d3 独占 "cB"。strong 各不相同。
        store
            .update_device_fingerprint("d1", Some("s1"), Some("cA"))
            .unwrap();
        store
            .update_device_fingerprint("d2", Some("s2"), Some("cA"))
            .unwrap();
        store
            .update_device_fingerprint("d3", Some("s3"), Some("cB"))
            .unwrap();
        store
            .update_device_fingerprint("d4", Some("s4"), Some(""))
            .unwrap();
        store
            .update_device_fingerprint("d5", Some("s5"), Some(""))
            .unwrap();

        // coarse:仅 cA 满足 count>=2;cB(1 台)被 HAVING 滤掉。
        let coarse = store.list_fingerprint_collisions("coarse", 2, 100).unwrap();
        assert_eq!(coarse.len(), 1);
        let (fp, count, ids) = &coarse[0];
        assert_eq!(fp, "cA");
        assert_eq!(*count, 2);
        let mut ids = ids.clone();
        ids.sort();
        assert_eq!(ids, vec!["d1".to_string(), "d2".to_string()]);

        // 非法 kind 退化为 coarse(同结果)。
        let bogus = store.list_fingerprint_collisions("bogus", 2, 100).unwrap();
        assert_eq!(bogus.len(), 1);
        assert_eq!(bogus[0].0, "cA");

        // strong 各不相同 → 无碰撞。
        let strong = store.list_fingerprint_collisions("strong", 2, 100).unwrap();
        assert!(strong.is_empty());
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
    fn data_upload_status_reflects_persisted_engine_state() {
        // 回归:引擎写路径须把 state_json 里的 totalEventCount 投影到标量列。
        // 经正常写路径落库(不手动塞列),数据状态面板 AMAS 通道应判 uploaded;
        // 修复前写路径只写 state_json、列恒为 0,此处会退回 nil。
        let store = test_store();
        let user_id = "u-3".to_string();
        store
            .set_engine_user_state(
                &user_id,
                &serde_json::json!({ "totalEventCount": 5, "sessionEventCount": 2 }),
            )
            .unwrap();
        let s = store
            .get_data_upload_status(std::slice::from_ref(&user_id), &[])
            .unwrap();
        assert_eq!(s.amas_by_user.get(&user_id).copied(), Some("uploaded"));
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

    #[test]
    fn flag_related_devices_marks_shared_ip_and_user() {
        let store = test_store();
        // 被封设备:IP=1.2.3.4, user=u-1
        store
            .upsert_client_device_with_extras(
                "dev-bad", "web", "u-1", None, None, Some("1.2.3.4"), None,
            )
            .unwrap();
        // 共享 IP(不同账号)
        store
            .upsert_client_device_with_extras(
                "dev-ip", "web", "u-2", None, None, Some("1.2.3.4"), None,
            )
            .unwrap();
        // 同账号(不同 IP)
        store
            .upsert_client_device_with_extras(
                "dev-user", "web", "u-1", None, None, Some("9.9.9.9"), None,
            )
            .unwrap();
        // 无关设备
        store
            .upsert_client_device_with_extras(
                "dev-clean", "web", "u-3", None, None, Some("5.5.5.5"), None,
            )
            .unwrap();

        store
            .ban_client_device("dev-bad", "admin", Some("spam"))
            .unwrap();
        let mut flagged = store.flag_related_devices("dev-bad").unwrap();
        flagged.sort();
        assert_eq!(flagged, vec!["dev-ip".to_string(), "dev-user".to_string()]);

        let listed = store.list_flagged_devices(10).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|d| d.risk_flag));
        assert!(listed
            .iter()
            .all(|d| d.risk_related_device.as_deref() == Some("dev-bad")));

        let clean = store.get_client_device("dev-clean").unwrap().unwrap();
        assert!(!clean.risk_flag);

        // 解封触发源 → 关联标记清除
        store.unban_client_device("dev-bad").unwrap();
        let cleared = store.clear_risk_flags_related_to("dev-bad").unwrap();
        assert_eq!(cleared, 2);
        assert!(store.list_flagged_devices(10).unwrap().is_empty());
    }

    #[test]
    fn ban_device_with_flagging_is_atomic_and_returns_flagged() {
        let store = test_store();
        store
            .upsert_client_device_with_extras("dev-bad", "web", "u-1", None, None, Some("1.2.3.4"), None)
            .unwrap();
        store
            .upsert_client_device_with_extras("dev-rel", "web", "u-2", None, None, Some("1.2.3.4"), None)
            .unwrap();
        let (banned, flagged) = store
            .ban_device_with_flagging("dev-bad", "admin", Some("spam"))
            .unwrap();
        assert!(banned);
        assert_eq!(flagged, vec!["dev-rel".to_string()]);
        assert!(store.is_device_banned("dev-bad").unwrap());
        let rel = store.get_client_device("dev-rel").unwrap().unwrap();
        assert!(rel.risk_flag);
        assert_eq!(rel.risk_related_device.as_deref(), Some("dev-bad"));
    }

    #[test]
    fn unban_recompute_keeps_flag_when_other_banned_source_remains() {
        // X 同时被 A、B 牵连,但 A、B 彼此无共享信号(A 与 X 共享 IP;B 与 X 共享账号)。
        // 封 A→X 标记(源 A);封 B→X 改指向 B。解封 B(recompute)→X 仍保持标记(A 仍被封,
        // 重指向 A);再解封 A→X 清除。锁定多源牵连下解封不错清/不漏清。
        let store = test_store();
        // A: IP=1.1.1.1, user=uA
        store
            .upsert_client_device_with_extras("dev-a", "web", "uA", None, None, Some("1.1.1.1"), None)
            .unwrap();
        // B: IP=2.2.2.2, user=uB(与 A 既不同 IP 也不同账号)
        store
            .upsert_client_device_with_extras("dev-b", "web", "uB", None, None, Some("2.2.2.2"), None)
            .unwrap();
        // X: IP=1.1.1.1(同 A), user=uB(同 B)
        store
            .upsert_client_device_with_extras("dev-x", "web", "uB", None, None, Some("1.1.1.1"), None)
            .unwrap();
        store.ban_device_with_flagging("dev-a", "admin", None).unwrap();
        assert_eq!(
            store.get_client_device("dev-x").unwrap().unwrap().risk_related_device.as_deref(),
            Some("dev-a")
        );
        store.ban_device_with_flagging("dev-b", "admin", None).unwrap();
        assert_eq!(
            store.get_client_device("dev-x").unwrap().unwrap().risk_related_device.as_deref(),
            Some("dev-b")
        );

        // 解封 B:X 不应被清(A 仍被封)→重指向 A,标记保留。
        let (_, cleared_b) = store.unban_device_with_flag_recompute("dev-b").unwrap();
        assert_eq!(cleared_b, 0);
        let x = store.get_client_device("dev-x").unwrap().unwrap();
        assert!(x.risk_flag);
        assert_eq!(x.risk_related_device.as_deref(), Some("dev-a"));

        // 解封 A:再无被封关联源 → X 清除。
        let (_, cleared_a) = store.unban_device_with_flag_recompute("dev-a").unwrap();
        assert_eq!(cleared_a, 1);
        assert!(!store.get_client_device("dev-x").unwrap().unwrap().risk_flag);
    }

    #[test]
    fn flag_related_skips_already_banned_and_self() {
        let store = test_store();
        store
            .upsert_client_device_with_extras(
                "dev-a", "web", "u-1", None, None, Some("1.1.1.1"), None,
            )
            .unwrap();
        store
            .upsert_client_device_with_extras(
                "dev-b", "web", "u-1", None, None, Some("1.1.1.1"), None,
            )
            .unwrap();
        store.ban_client_device("dev-b", "admin", None).unwrap(); // 已封,应被排除
        store.ban_client_device("dev-a", "admin", None).unwrap();
        // dev-b 已封被排除;dev-a 自身被排除 → 空
        assert!(store.flag_related_devices("dev-a").unwrap().is_empty());
    }

    #[test]
    fn flag_related_noop_when_no_signals() {
        let store = test_store();
        // last_ip 与 user_id 都为空 → 无可关联信号
        store
            .upsert_client_device_with_extras("dev-x", "web", "", None, None, None, None)
            .unwrap();
        store
            .upsert_client_device_with_extras("dev-y", "web", "", None, None, None, None)
            .unwrap();
        store.ban_client_device("dev-x", "admin", None).unwrap();
        assert!(store.flag_related_devices("dev-x").unwrap().is_empty());

        // 误报清除
        store
            .upsert_client_device_with_extras(
                "dev-z", "web", "u-9", None, None, Some("2.2.2.2"), None,
            )
            .unwrap();
        store
            .upsert_client_device_with_extras(
                "dev-z2", "web", "u-9", None, None, Some("2.2.2.2"), None,
            )
            .unwrap();
        store.ban_client_device("dev-z", "admin", None).unwrap();
        assert_eq!(store.flag_related_devices("dev-z").unwrap().len(), 1);
        assert!(store.clear_device_risk_flag("dev-z2").unwrap());
        assert!(store.list_flagged_devices(10).unwrap().is_empty());
    }

    #[test]
    fn fingerprint_strong_ban_matches_new_device_id() {
        let store = test_store();
        // 老设备:强指纹 FP_A,被封
        store
            .upsert_client_device_with_extras("dev-old", "web", "u-1", None, None, None, None)
            .unwrap();
        store
            .update_device_fingerprint("dev-old", Some("FP_A"), Some("CO_A"))
            .unwrap();
        store
            .ban_client_device("dev-old", "admin", Some("spam"))
            .unwrap();

        // 新 device_id(清缓存/隐私模式),但强指纹相同 → 视为被封
        assert!(store.is_client_banned("dev-new", Some("FP_A")).unwrap());
        // 强指纹不同 → 未封
        assert!(!store.is_client_banned("dev-new", Some("FP_B")).unwrap());
        // 不带指纹 → 退化为纯 device_id 判定(新 id 未封)
        assert!(!store.is_client_banned("dev-new", None).unwrap());
        // device_id 自身被封仍命中
        assert!(store.is_client_banned("dev-old", None).unwrap());
    }

    #[test]
    fn coarse_fingerprint_flags_not_bans() {
        let store = test_store();
        store
            .upsert_client_device_with_extras("dev-old", "web", "u-1", None, None, None, None)
            .unwrap();
        store
            .update_device_fingerprint("dev-old", Some("FP_A"), Some("CO_A"))
            .unwrap();
        store.ban_client_device("dev-old", "admin", None).unwrap();

        // 新设备:换浏览器→强指纹不同(FP_B),但同硬件→模糊指纹相同(CO_A)
        store
            .upsert_client_device_with_extras("dev-new", "web", "u-2", None, None, None, None)
            .unwrap();
        store
            .update_device_fingerprint("dev-new", Some("FP_B"), Some("CO_A"))
            .unwrap();

        // 强指纹不同 → 不硬封
        assert!(!store.is_client_banned("dev-new", Some("FP_B")).unwrap());
        // 模糊指纹命中 → 打标(不硬封)
        let related = store.flag_device_if_coarse_banned("dev-new", "CO_A").unwrap();
        assert_eq!(related.as_deref(), Some("dev-old"));
        let flagged = store.list_flagged_devices(10).unwrap();
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].device_id, "dev-new");
        assert_eq!(flagged[0].risk_related_device.as_deref(), Some("dev-old"));
        // 不匹配的 coarse → 无操作
        assert!(store
            .flag_device_if_coarse_banned("dev-new", "CO_OTHER")
            .unwrap()
            .is_none());
    }

    #[test]
    fn flag_related_devices_includes_coarse_fingerprint() {
        let store = test_store();
        store
            .upsert_client_device_with_extras(
                "dev-bad", "web", "u-1", None, None, Some("1.2.3.4"), None,
            )
            .unwrap();
        store
            .update_device_fingerprint("dev-bad", Some("S1"), Some("COARSE"))
            .unwrap();
        // 不同 IP、不同账号,但同硬件模糊指纹
        store
            .upsert_client_device_with_extras(
                "dev-fp", "web", "u-9", None, None, Some("8.8.8.8"), None,
            )
            .unwrap();
        store
            .update_device_fingerprint("dev-fp", Some("S2"), Some("COARSE"))
            .unwrap();
        store.ban_client_device("dev-bad", "admin", None).unwrap();
        let flagged = store.flag_related_devices("dev-bad").unwrap();
        assert_eq!(flagged, vec!["dev-fp".to_string()]);
        let d = store.get_client_device("dev-fp").unwrap().unwrap();
        assert!(d.risk_reason.unwrap().contains("指纹"));
    }
}
