//! v1.1-P0.2：资源包热更存储层。
//!
//! 表关系：
//!   resource_packs (pack_id PK)
//!     ├─ resource_pack_versions (pack_id, version) — 全量版本
//!     ├─ resource_pack_active   (pack_id, channel) — 每 channel 当前激活 version
//!     └─ resource_pack_install_log              — 客户端安装/校验失败/回滚 telemetry
//!
//! 对外 DTO `ResourcePackManifest` 严格对齐
//! `docs/backend-handoff-resource-pack-v1.1.md` §2.1 字段名（camelCase）。
//!
//! 业务 channel 与 `services::updater::Channel`（二进制自更新通道）刻意分开 ——
//! 后者只有 Stable/Beta，本模块需要额外的 Internal 内测通道。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

/// 资源包发布通道。serde 序列化为小写 `"stable"` / `"beta"` / `"internal"`，
/// 与迁移 020 中 `CHECK (channel IN (...))` 一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourcePackChannel {
    Stable,
    Beta,
    Internal,
}

impl ResourcePackChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourcePackChannel::Stable => "stable",
            ResourcePackChannel::Beta => "beta",
            ResourcePackChannel::Internal => "internal",
        }
    }

    // 故意不实现 std::str::FromStr：返回 Option 而非 Result<Self, Err>，调用方
    // 用 .ok_or_else 拼自己的 AppError 更直接。
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "stable" => Some(Self::Stable),
            "beta" => Some(Self::Beta),
            "internal" => Some(Self::Internal),
            _ => None,
        }
    }
}

/// 资源包元数据。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePack {
    pub pack_id: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 单个资源包版本的完整 row。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackVersion {
    pub pack_id: String,
    pub version: String,
    pub sha256: String,
    pub signature: Option<String>,
    pub signature_alg: String,
    pub size_bytes: i64,
    pub min_app_version: Option<String>,
    pub channel: ResourcePackChannel,
    pub payload_path: String,
    pub published_at: String,
    pub deactivated_at: Option<String>,
}

/// 当前激活指针（每 pack × channel 一行）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackActive {
    pub pack_id: String,
    pub channel: ResourcePackChannel,
    pub version: String,
    pub activated_at: String,
    pub activated_by: Option<String>,
}

/// 客户端 telemetry 单条记录。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackInstallLog {
    pub id: i64,
    pub pack_id: String,
    pub version: String,
    pub client_id: Option<String>,
    pub app_version: Option<String>,
    pub installed_at: String,
    pub outcome: String,
}

/// 客户端 manifest 端点响应 DTO。字段名严格对齐对接文档 §2.1
/// （注意 `downloadURL` 是全大写 URL，serde camelCase 不会正确转换，需手动 rename）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackManifest {
    pub pack_id: String,
    pub version: String,
    #[serde(rename = "downloadURL")]
    pub download_url: String,
    pub sha256: String,
    pub size_bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,
    pub channel: ResourcePackChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_algorithm: Option<String>,
}

/// 近 7 天某维度三态 outcome 计数（installed / verify_failed / rollback）。
/// 失败 = verify_failed + rollback；下载/安装总数以 install_log 计数为真实源。
#[derive(Debug, Clone, Default)]
pub struct PackOutcomeCounts {
    pub installed: i64,
    pub verify_failed: i64,
    pub rollback: i64,
}

impl PackOutcomeCounts {
    fn add(&mut self, outcome: &str, count: i64) {
        match outcome {
            "installed" => self.installed += count,
            "verify_failed" => self.verify_failed += count,
            "rollback" => self.rollback += count,
            _ => {}
        }
    }

    /// 失败计数 = verify_failed + rollback。
    pub fn failures(&self) -> i64 {
        self.verify_failed + self.rollback
    }

    /// 三态总计。
    pub fn total(&self) -> i64 {
        self.installed + self.verify_failed + self.rollback
    }
}

/// 三通道计数（stable / beta / internal）。
#[derive(Debug, Clone, Default)]
pub struct ChannelCounts {
    pub stable: i64,
    pub beta: i64,
    pub internal: i64,
}

impl ChannelCounts {
    fn add(&mut self, channel: &str, count: i64) {
        match channel {
            "stable" => self.stable += count,
            "beta" => self.beta += count,
            "internal" => self.internal += count,
            _ => {}
        }
    }
}

/// 跨包全局聚合结果（`resource_pack_summary` 返回）。failureRate7d 由 handler
/// 从 `outcomes_7d` 算（分母为 0 时返回 0，不在 store 侧除零）。
#[derive(Debug, Clone)]
pub struct ResourcePackSummary {
    pub total_packs: i64,
    pub new_packs_this_month: i64,
    pub total_versions: i64,
    pub versions_by_channel: ChannelCounts,
    pub installs_today: i64,
    pub installs_today_success: i64,
    pub outcomes_7d: PackOutcomeCounts,
}

const VERSION_COLS: &str = "pack_id, version, sha256, signature, signature_alg, size_bytes, \
     min_app_version, channel, payload_path, published_at, deactivated_at";

fn version_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResourcePackVersion> {
    let channel_str: String = row.get(7)?;
    let channel = ResourcePackChannel::from_str(&channel_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            format!("unknown channel: {channel_str}").into(),
        )
    })?;
    Ok(ResourcePackVersion {
        pack_id: row.get(0)?,
        version: row.get(1)?,
        sha256: row.get(2)?,
        signature: row.get(3)?,
        signature_alg: row.get(4)?,
        size_bytes: row.get(5)?,
        min_app_version: row.get(6)?,
        channel,
        payload_path: row.get(8)?,
        published_at: row.get(9)?,
        deactivated_at: row.get(10)?,
    })
}

impl Store {
    /// upsert 一条 pack 元数据。同 pack_id 重复调用只刷新 updated_at + description。
    pub fn upsert_resource_pack(
        &self,
        pack_id: &str,
        description: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO resource_packs (pack_id, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(pack_id) DO UPDATE SET
                description = excluded.description,
                updated_at  = excluded.updated_at",
            params![pack_id, description, now],
        )?;
        Ok(())
    }

    /// 判定某 `(pack_id, version)` 是否已存在（含软删除版本）。
    /// 上传入口用此做先验去重，避免重传同版本号覆盖磁盘 payload.json 致激活版本验签失败。
    pub fn pack_version_exists(&self, pack_id: &str, version: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM resource_pack_versions WHERE pack_id = ?1 AND version = ?2 LIMIT 1",
                params![pack_id, version],
                |row| row.get(0),
            )
            .optional()?;
        Ok(exists.is_some())
    }

    /// 写入一个新版本。pack_id 必须先 upsert 过（外键约束）。
    pub fn insert_pack_version(&self, v: &ResourcePackVersion) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            &format!(
                "INSERT INTO resource_pack_versions ({VERSION_COLS}) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
            ),
            params![
                v.pack_id,
                v.version,
                v.sha256,
                v.signature,
                v.signature_alg,
                v.size_bytes,
                v.min_app_version,
                v.channel.as_str(),
                v.payload_path,
                v.published_at,
                v.deactivated_at,
            ],
        )?;
        Ok(())
    }

    /// 拿某 pack × channel 当前激活版本的完整 row。manifest 端点用。
    pub fn get_active_pack_version(
        &self,
        pack_id: &str,
        channel: ResourcePackChannel,
    ) -> Result<Option<ResourcePackVersion>, StoreError> {
        let conn = self.conn()?;
        // JOIN 后 pack_id/version/channel 在两表都存在，必须用 v. 前缀消除歧义。
        conn.query_row(
            "SELECT v.pack_id, v.version, v.sha256, v.signature, v.signature_alg, \
                    v.size_bytes, v.min_app_version, v.channel, v.payload_path, \
                    v.published_at, v.deactivated_at \
             FROM resource_pack_versions v \
             INNER JOIN resource_pack_active a \
               ON a.pack_id = v.pack_id AND a.channel = v.channel AND a.version = v.version \
             WHERE v.pack_id = ?1 AND a.channel = ?2 AND v.deactivated_at IS NULL",
            params![pack_id, channel.as_str()],
            version_from_row,
        )
        .optional()
        .map_err(StoreError::from)
    }

    /// 拿某 `(pack_id, version)` 的完整 row（不限激活/通道，含软删除）。
    /// web-app 激活前据此取 payload_path/sha256/signature，实现"先校验+解包成功、再翻转激活指针"。
    pub fn get_pack_version(
        &self,
        pack_id: &str,
        version: &str,
    ) -> Result<Option<ResourcePackVersion>, StoreError> {
        let conn = self.conn()?;
        conn.query_row(
            &format!(
                "SELECT {VERSION_COLS} FROM resource_pack_versions \
                 WHERE pack_id = ?1 AND version = ?2 LIMIT 1"
            ),
            params![pack_id, version],
            version_from_row,
        )
        .optional()
        .map_err(StoreError::from)
    }

    /// 切换某 channel 的当前激活版本。upsert 语义。
    pub fn set_active_pack_version(
        &self,
        pack_id: &str,
        channel: ResourcePackChannel,
        version: &str,
        activated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO resource_pack_active (pack_id, channel, version, activated_at, activated_by)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(pack_id, channel) DO UPDATE SET
                version = excluded.version,
                activated_at = excluded.activated_at,
                activated_by = excluded.activated_by",
            params![pack_id, channel.as_str(), version, now, activated_by],
        )?;
        Ok(())
    }

    /// 软删除某版本（manifest 路由摘除，文件保留 30 天供回滚兜底，由 GC worker 删盘）。
    pub fn deactivate_pack_version(&self, pack_id: &str, version: &str) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE resource_pack_versions
             SET deactivated_at = ?3
             WHERE pack_id = ?1 AND version = ?2",
            params![pack_id, version, now],
        )?;
        Ok(())
    }

    /// 列出所有非删除的 packs。匿名 `GET /api/resource-packs` 用。
    pub fn list_resource_packs(&self) -> Result<Vec<ResourcePack>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT pack_id, description, created_at, updated_at \
             FROM resource_packs ORDER BY pack_id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ResourcePack {
                pack_id: row.get(0)?,
                description: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// 聚合某 pack × version 的 telemetry：返回 (outcome, count) 列表。
    /// 不指定 version 时返回 pack 维度的全量聚合（version=*）。
    pub fn pack_install_stats(
        &self,
        pack_id: &str,
    ) -> Result<Vec<(String, String, i64)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT version, outcome, COUNT(*) FROM resource_pack_install_log
             WHERE pack_id = ?1
             GROUP BY version, outcome
             ORDER BY version DESC, outcome ASC",
        )?;
        let rows = stmt.query_map(params![pack_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// 拿某 pack 的所有版本（admin 列表用）。倒序按 published_at。
    pub fn list_pack_versions(
        &self,
        pack_id: &str,
    ) -> Result<Vec<ResourcePackVersion>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {VERSION_COLS} FROM resource_pack_versions \
             WHERE pack_id = ?1 ORDER BY published_at DESC"
        ))?;
        let rows = stmt.query_map(params![pack_id], version_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// 某 pack 在 install_log 的全量记录数（KPI「总下载/总安装」以 install_log 计数为真实源）。
    pub fn pack_total_installs(&self, pack_id: &str) -> Result<i64, StoreError> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM resource_pack_install_log WHERE pack_id = ?1",
            params![pack_id],
            |row| row.get(0),
        )
        .map_err(StoreError::from)
    }

    /// 某 pack 近 7 天各 outcome 计数 (installed / verify_failed / rollback)。
    /// `since` 为 RFC3339 边界（now-7d），调用方在 Rust 侧用 chrono 算出后传入。
    pub fn pack_outcomes_since(
        &self,
        pack_id: &str,
        since: &str,
    ) -> Result<PackOutcomeCounts, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT outcome, COUNT(*) FROM resource_pack_install_log
             WHERE pack_id = ?1 AND installed_at >= ?2
             GROUP BY outcome",
        )?;
        let rows = stmt.query_map(params![pack_id, since], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut out = PackOutcomeCounts::default();
        for r in rows {
            let (outcome, count) = r?;
            out.add(&outcome, count);
        }
        Ok(out)
    }

    /// 某 pack 各通道当前激活版本：返回 (channel, version) 列表（无激活则空）。
    /// JOIN `resource_pack_versions` 并过滤 `v.deactivated_at IS NULL`，与
    /// `get_active_pack_version`（manifest 端点）一致 —— 停用版本不再被报告为激活，
    /// 避免 admin 列表 active.{channel} 仍指向已被 manifest 摘除的版本。
    pub fn pack_active_versions(
        &self,
        pack_id: &str,
    ) -> Result<Vec<(ResourcePackChannel, String)>, StoreError> {
        let conn = self.conn()?;
        // JOIN 后 pack_id/channel/version 在两表都存在，必须用 a./v. 前缀消歧。
        let mut stmt = conn.prepare(
            "SELECT a.channel, a.version FROM resource_pack_active a \
             INNER JOIN resource_pack_versions v \
               ON v.pack_id = a.pack_id AND v.channel = a.channel AND v.version = a.version \
             WHERE a.pack_id = ?1 AND v.deactivated_at IS NULL",
        )?;
        let rows = stmt.query_map(params![pack_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (ch, ver) = r?;
            if let Some(channel) = ResourcePackChannel::from_str(&ch) {
                out.push((channel, ver));
            }
        }
        Ok(out)
    }

    /// 跨包全局聚合，供 `GET /api/admin/resource-packs/summary`。
    /// 时间边界 (month_start / today_start / since_7d) 均为 RFC3339 字符串，
    /// 由调用方在 Rust 侧用 chrono 算出后传入（同格式同时区下字典序==时间序）。
    pub fn resource_pack_summary(
        &self,
        month_start: &str,
        today_start: &str,
        since_7d: &str,
    ) -> Result<ResourcePackSummary, StoreError> {
        let conn = self.conn()?;

        let total_packs: i64 =
            conn.query_row("SELECT COUNT(*) FROM resource_packs", [], |r| r.get(0))?;
        let new_packs_this_month: i64 = conn.query_row(
            "SELECT COUNT(*) FROM resource_packs WHERE created_at >= ?1",
            params![month_start],
            |r| r.get(0),
        )?;
        let total_versions: i64 =
            conn.query_row("SELECT COUNT(*) FROM resource_pack_versions", [], |r| {
                r.get(0)
            })?;

        // 各通道版本数
        let mut versions_by_channel = ChannelCounts::default();
        {
            let mut stmt = conn
                .prepare("SELECT channel, COUNT(*) FROM resource_pack_versions GROUP BY channel")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for r in rows {
                let (ch, count) = r?;
                versions_by_channel.add(&ch, count);
            }
        }

        let installs_today: i64 = conn.query_row(
            "SELECT COUNT(*) FROM resource_pack_install_log WHERE installed_at >= ?1",
            params![today_start],
            |r| r.get(0),
        )?;
        let installs_today_success: i64 = conn.query_row(
            "SELECT COUNT(*) FROM resource_pack_install_log
             WHERE installed_at >= ?1 AND outcome = 'installed'",
            params![today_start],
            |r| r.get(0),
        )?;

        // 近 7 天各 outcome 计数（用于 failureRate7d + failures7dByOutcome）
        let mut outcomes_7d = PackOutcomeCounts::default();
        {
            let mut stmt = conn.prepare(
                "SELECT outcome, COUNT(*) FROM resource_pack_install_log
                 WHERE installed_at >= ?1 GROUP BY outcome",
            )?;
            let rows = stmt.query_map(params![since_7d], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for r in rows {
                let (outcome, count) = r?;
                outcomes_7d.add(&outcome, count);
            }
        }

        Ok(ResourcePackSummary {
            total_packs,
            new_packs_this_month,
            total_versions,
            versions_by_channel,
            installs_today,
            installs_today_success,
            outcomes_7d,
        })
    }

    /// 写入一条客户端 telemetry 记录。
    pub fn record_pack_install(
        &self,
        pack_id: &str,
        version: &str,
        client_id: Option<&str>,
        app_version: Option<&str>,
        outcome: &str,
    ) -> Result<(), StoreError> {
        if !matches!(outcome, "installed" | "verify_failed" | "rollback") {
            return Err(StoreError::Validation(format!(
                "invalid resource_pack install outcome: {outcome}"
            )));
        }
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO resource_pack_install_log
                (pack_id, version, client_id, app_version, installed_at, outcome)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![pack_id, version, client_id, app_version, now, outcome],
        )?;
        Ok(())
    }

    /// 资源包安装留痕表 retention 清理：删除 installed_at < cutoff 的行，返回删除行数。
    /// **注意**：installed_at 由 `record_pack_install` 写入 `to_rfc3339()`（'T' 分隔 + 时区后缀），
    /// 故 cutoff 必须同为 RFC3339，不可复用空格格式 cutoff（TEXT 字典序比较会因 'T' vs 空格失配）。
    pub fn delete_pack_installs_older_than(
        &self,
        cutoff_rfc3339: &str,
    ) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM resource_pack_install_log WHERE installed_at < ?1",
            params![cutoff_rfc3339],
        )?;
        Ok(deleted as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    fn store() -> Store {
        let s = Store::open(":memory:", 5000, 1).unwrap();
        crate::store::migrate::run(&s).unwrap();
        s
    }

    fn sample_version(pack: &str, ver: &str, ch: ResourcePackChannel) -> ResourcePackVersion {
        ResourcePackVersion {
            pack_id: pack.to_string(),
            version: ver.to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            signature: Some("base64-sig".to_string()),
            signature_alg: "ed25519".to_string(),
            size_bytes: 1234,
            min_app_version: Some("1.0.0".to_string()),
            channel: ch,
            payload_path: format!("static/packs/{pack}/{ver}/payload.json"),
            published_at: chrono::Utc::now().to_rfc3339(),
            deactivated_at: None,
        }
    }

    #[test]
    fn channel_serde_roundtrip() {
        assert_eq!(
            serde_json::to_string(&ResourcePackChannel::Stable).unwrap(),
            "\"stable\""
        );
        assert_eq!(
            serde_json::to_string(&ResourcePackChannel::Internal).unwrap(),
            "\"internal\""
        );
        let c: ResourcePackChannel = serde_json::from_str("\"beta\"").unwrap();
        assert!(matches!(c, ResourcePackChannel::Beta));
        assert_eq!(
            ResourcePackChannel::from_str("internal").unwrap().as_str(),
            "internal"
        );
        assert!(ResourcePackChannel::from_str("bogus").is_none());
    }

    #[test]
    fn upsert_and_list_packs() {
        let s = store();
        s.upsert_resource_pack("wordbook-core", Some("核心词库"))
            .unwrap();
        s.upsert_resource_pack("homepage-banners", None).unwrap();
        let list = s.list_resource_packs().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].pack_id, "homepage-banners");
        assert_eq!(list[1].pack_id, "wordbook-core");
        assert_eq!(list[1].description.as_deref(), Some("核心词库"));

        // upsert 同 pack 刷新 description
        s.upsert_resource_pack("wordbook-core", Some("v2 描述"))
            .unwrap();
        let list2 = s.list_resource_packs().unwrap();
        assert_eq!(list2[1].description.as_deref(), Some("v2 描述"));
    }

    #[test]
    fn insert_version_and_get_active() {
        let s = store();
        s.upsert_resource_pack("wordbook-core", None).unwrap();
        s.insert_pack_version(&sample_version(
            "wordbook-core",
            "1.0.0",
            ResourcePackChannel::Stable,
        ))
        .unwrap();
        s.insert_pack_version(&sample_version(
            "wordbook-core",
            "1.1.0-rc.1",
            ResourcePackChannel::Beta,
        ))
        .unwrap();

        // 未激活时 get_active 返回 None
        assert!(s
            .get_active_pack_version("wordbook-core", ResourcePackChannel::Stable)
            .unwrap()
            .is_none());

        s.set_active_pack_version(
            "wordbook-core",
            ResourcePackChannel::Stable,
            "1.0.0",
            Some("admin-1"),
        )
        .unwrap();
        let active = s
            .get_active_pack_version("wordbook-core", ResourcePackChannel::Stable)
            .unwrap()
            .expect("active stable should exist");
        assert_eq!(active.version, "1.0.0");
        assert_eq!(active.sha256.len(), 64);

        // beta channel 独立
        assert!(s
            .get_active_pack_version("wordbook-core", ResourcePackChannel::Beta)
            .unwrap()
            .is_none());
    }

    #[test]
    fn set_active_overwrites_previous() {
        let s = store();
        s.upsert_resource_pack("wb", None).unwrap();
        s.insert_pack_version(&sample_version("wb", "1.0.0", ResourcePackChannel::Stable))
            .unwrap();
        s.insert_pack_version(&sample_version("wb", "1.1.0", ResourcePackChannel::Stable))
            .unwrap();
        s.set_active_pack_version("wb", ResourcePackChannel::Stable, "1.0.0", None)
            .unwrap();
        s.set_active_pack_version("wb", ResourcePackChannel::Stable, "1.1.0", None)
            .unwrap();
        let active = s
            .get_active_pack_version("wb", ResourcePackChannel::Stable)
            .unwrap()
            .unwrap();
        assert_eq!(active.version, "1.1.0");
    }

    #[test]
    fn deactivate_excludes_from_active_query() {
        let s = store();
        s.upsert_resource_pack("wb", None).unwrap();
        s.insert_pack_version(&sample_version("wb", "1.0.0", ResourcePackChannel::Stable))
            .unwrap();
        s.set_active_pack_version("wb", ResourcePackChannel::Stable, "1.0.0", None)
            .unwrap();
        s.deactivate_pack_version("wb", "1.0.0").unwrap();
        // 软删除后 get_active 应当返回 None（manifest 路由摘除）
        assert!(s
            .get_active_pack_version("wb", ResourcePackChannel::Stable)
            .unwrap()
            .is_none());
    }

    #[test]
    fn pack_active_versions_excludes_deactivated() {
        // 与 get_active_pack_version 对齐：停用后 admin 列表也不应再把它当激活版本。
        let s = store();
        s.upsert_resource_pack("wb", None).unwrap();
        s.insert_pack_version(&sample_version("wb", "1.0.0", ResourcePackChannel::Stable))
            .unwrap();
        s.insert_pack_version(&sample_version("wb", "1.1.0-rc", ResourcePackChannel::Beta))
            .unwrap();
        s.set_active_pack_version("wb", ResourcePackChannel::Stable, "1.0.0", None)
            .unwrap();
        s.set_active_pack_version("wb", ResourcePackChannel::Beta, "1.1.0-rc", None)
            .unwrap();

        // 停用前：两通道均报告激活
        assert_eq!(s.pack_active_versions("wb").unwrap().len(), 2);

        // 停用 stable 的激活版本后：只剩 beta，且与 get_active_pack_version 一致
        s.deactivate_pack_version("wb", "1.0.0").unwrap();
        let active = s.pack_active_versions("wb").unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(
            active[0],
            (ResourcePackChannel::Beta, "1.1.0-rc".to_string())
        );
        assert!(s
            .get_active_pack_version("wb", ResourcePackChannel::Stable)
            .unwrap()
            .is_none());
    }

    /// 直接插一条 install_log 并指定 installed_at（绕过 record_pack_install 的 now）。
    fn seed_install(s: &Store, pack: &str, ver: &str, outcome: &str, installed_at: &str) {
        let conn = s.conn().unwrap();
        conn.execute(
            "INSERT INTO resource_pack_install_log
                (pack_id, version, client_id, app_version, installed_at, outcome)
             VALUES (?1, ?2, NULL, NULL, ?3, ?4)",
            params![pack, ver, installed_at, outcome],
        )
        .unwrap();
    }

    #[test]
    fn per_pack_aggregations_total_outcomes7d_and_active() {
        let s = store();
        let now = chrono::Utc::now();
        let today = now.to_rfc3339();
        let week_ago = (now - chrono::Duration::days(8)).to_rfc3339();
        let since_7d = (now - chrono::Duration::days(7)).to_rfc3339();

        s.upsert_resource_pack("wb", None).unwrap();
        s.insert_pack_version(&sample_version("wb", "1.0.0", ResourcePackChannel::Stable))
            .unwrap();
        s.insert_pack_version(&sample_version("wb", "1.1.0-rc", ResourcePackChannel::Beta))
            .unwrap();

        // 今日窗口内：2 installed + 1 verify_failed + 1 rollback
        seed_install(&s, "wb", "1.0.0", "installed", &today);
        seed_install(&s, "wb", "1.0.0", "installed", &today);
        seed_install(&s, "wb", "1.0.0", "verify_failed", &today);
        seed_install(&s, "wb", "1.0.0", "rollback", &today);
        // 8 天前（落在 7 天窗口外）：1 installed
        seed_install(&s, "wb", "1.0.0", "installed", &week_ago);

        // 全量含窗口外那条
        assert_eq!(s.pack_total_installs("wb").unwrap(), 5);

        let o7 = s.pack_outcomes_since("wb", &since_7d).unwrap();
        assert_eq!(o7.installed, 2);
        assert_eq!(o7.verify_failed, 1);
        assert_eq!(o7.rollback, 1);
        assert_eq!(o7.failures(), 2);

        // 各通道激活指针
        s.set_active_pack_version("wb", ResourcePackChannel::Stable, "1.0.0", None)
            .unwrap();
        s.set_active_pack_version("wb", ResourcePackChannel::Beta, "1.1.0-rc", None)
            .unwrap();
        let mut active = s.pack_active_versions("wb").unwrap();
        active.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        assert_eq!(active.len(), 2);
        assert_eq!(
            active[0],
            (ResourcePackChannel::Beta, "1.1.0-rc".to_string())
        );
        assert_eq!(
            active[1],
            (ResourcePackChannel::Stable, "1.0.0".to_string())
        );
    }

    #[test]
    fn global_summary_aggregation() {
        let s = store();
        let now = chrono::Utc::now();
        let today = now.to_rfc3339();
        let week_ago = (now - chrono::Duration::days(8)).to_rfc3339();
        let today_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .to_rfc3339();
        let since_7d = (now - chrono::Duration::days(7)).to_rfc3339();
        // 取一个保证早于本月所有数据的本月界（本月 1 号 00:00:00Z）
        let month_start = now
            .date_naive()
            .with_day(1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .to_rfc3339();

        // 2 个 pack：upsert_resource_pack 用 now 作 created_at，故均计入「本月新增」
        s.upsert_resource_pack("wb", None).unwrap();
        s.upsert_resource_pack("hp", None).unwrap();
        // 版本分布：wb 2 stable + 1 beta，hp 1 internal → stable=2 beta=1 internal=1，total=4
        s.insert_pack_version(&sample_version("wb", "1.0.0", ResourcePackChannel::Stable))
            .unwrap();
        s.insert_pack_version(&sample_version("wb", "1.1.0", ResourcePackChannel::Stable))
            .unwrap();
        s.insert_pack_version(&sample_version("wb", "1.2.0-rc", ResourcePackChannel::Beta))
            .unwrap();
        s.insert_pack_version(&sample_version(
            "hp",
            "0.9.0",
            ResourcePackChannel::Internal,
        ))
        .unwrap();

        // install_log：今日 3 installed + 1 verify_failed + 1 rollback；8 天前 2 installed（窗口外）
        seed_install(&s, "wb", "1.0.0", "installed", &today);
        seed_install(&s, "wb", "1.0.0", "installed", &today);
        seed_install(&s, "wb", "1.1.0", "installed", &today);
        seed_install(&s, "wb", "1.0.0", "verify_failed", &today);
        seed_install(&s, "hp", "0.9.0", "rollback", &today);
        seed_install(&s, "wb", "1.0.0", "installed", &week_ago);
        seed_install(&s, "wb", "1.0.0", "installed", &week_ago);

        let sum = s
            .resource_pack_summary(&month_start, &today_start, &since_7d)
            .unwrap();
        assert_eq!(sum.total_packs, 2);
        assert_eq!(sum.new_packs_this_month, 2);
        assert_eq!(sum.total_versions, 4);
        assert_eq!(sum.versions_by_channel.stable, 2);
        assert_eq!(sum.versions_by_channel.beta, 1);
        assert_eq!(sum.versions_by_channel.internal, 1);
        // 今日 5 条，其中 3 installed
        assert_eq!(sum.installs_today, 5);
        assert_eq!(sum.installs_today_success, 3);
        // 近 7 天（含今日，排除 8 天前）：3 installed + 1 verify_failed + 1 rollback
        assert_eq!(sum.outcomes_7d.installed, 3);
        assert_eq!(sum.outcomes_7d.verify_failed, 1);
        assert_eq!(sum.outcomes_7d.rollback, 1);
        assert_eq!(sum.outcomes_7d.failures(), 2);
        assert_eq!(sum.outcomes_7d.total(), 5);
    }

    #[test]
    fn summary_empty_db_has_no_panic_and_zero_failures() {
        let s = store();
        let now = chrono::Utc::now();
        let b = now.to_rfc3339();
        let sum = s.resource_pack_summary(&b, &b, &b).unwrap();
        assert_eq!(sum.total_packs, 0);
        assert_eq!(sum.total_versions, 0);
        assert_eq!(sum.installs_today, 0);
        // failures()/total() 在空数据下安全返回 0（handler 据此规避除零）
        assert_eq!(sum.outcomes_7d.failures(), 0);
        assert_eq!(sum.outcomes_7d.total(), 0);
    }

    #[test]
    fn install_log_outcome_validation() {
        let s = store();
        s.upsert_resource_pack("wb", None).unwrap();
        s.record_pack_install("wb", "1.0.0", Some("client-1"), Some("1.0.0"), "installed")
            .unwrap();
        s.record_pack_install("wb", "1.0.0", None, None, "verify_failed")
            .unwrap();
        s.record_pack_install("wb", "1.0.0", None, None, "rollback")
            .unwrap();

        let err = s
            .record_pack_install("wb", "1.0.0", None, None, "garbage")
            .unwrap_err();
        match err {
            StoreError::Validation(msg) => assert!(msg.contains("invalid")),
            _ => panic!("expected Validation, got {err:?}"),
        }
    }
}
