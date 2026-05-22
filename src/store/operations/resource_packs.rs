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

/// 客户端 manifest 端点响应 DTO。字段名严格对齐对接文档 §2.1。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackManifest {
    pub pack_id: String,
    pub version: String,
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
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let s = Store::open(":memory:", 5000, 1).unwrap();
        crate::store::migrate::run(&s).unwrap();
        s
    }

    fn sample_version(pack: &str, ver: &str, ch: ResourcePackChannel) -> ResourcePackVersion {
        ResourcePackVersion {
            pack_id: pack.to_string(),
            version: ver.to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_string(),
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
        assert_eq!(ResourcePackChannel::from_str("internal").unwrap().as_str(), "internal");
        assert!(ResourcePackChannel::from_str("bogus").is_none());
    }

    #[test]
    fn upsert_and_list_packs() {
        let s = store();
        s.upsert_resource_pack("wordbook-core", Some("核心词库")).unwrap();
        s.upsert_resource_pack("homepage-banners", None).unwrap();
        let list = s.list_resource_packs().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].pack_id, "homepage-banners");
        assert_eq!(list[1].pack_id, "wordbook-core");
        assert_eq!(list[1].description.as_deref(), Some("核心词库"));

        // upsert 同 pack 刷新 description
        s.upsert_resource_pack("wordbook-core", Some("v2 描述")).unwrap();
        let list2 = s.list_resource_packs().unwrap();
        assert_eq!(list2[1].description.as_deref(), Some("v2 描述"));
    }

    #[test]
    fn insert_version_and_get_active() {
        let s = store();
        s.upsert_resource_pack("wordbook-core", None).unwrap();
        s.insert_pack_version(&sample_version("wordbook-core", "1.0.0", ResourcePackChannel::Stable))
            .unwrap();
        s.insert_pack_version(&sample_version("wordbook-core", "1.1.0-rc.1", ResourcePackChannel::Beta))
            .unwrap();

        // 未激活时 get_active 返回 None
        assert!(s
            .get_active_pack_version("wordbook-core", ResourcePackChannel::Stable)
            .unwrap()
            .is_none());

        s.set_active_pack_version("wordbook-core", ResourcePackChannel::Stable, "1.0.0", Some("admin-1"))
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
        s.insert_pack_version(&sample_version("wb", "1.0.0", ResourcePackChannel::Stable)).unwrap();
        s.insert_pack_version(&sample_version("wb", "1.1.0", ResourcePackChannel::Stable)).unwrap();
        s.set_active_pack_version("wb", ResourcePackChannel::Stable, "1.0.0", None).unwrap();
        s.set_active_pack_version("wb", ResourcePackChannel::Stable, "1.1.0", None).unwrap();
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
        s.insert_pack_version(&sample_version("wb", "1.0.0", ResourcePackChannel::Stable)).unwrap();
        s.set_active_pack_version("wb", ResourcePackChannel::Stable, "1.0.0", None).unwrap();
        s.deactivate_pack_version("wb", "1.0.0").unwrap();
        // 软删除后 get_active 应当返回 None（manifest 路由摘除）
        assert!(s
            .get_active_pack_version("wb", ResourcePackChannel::Stable)
            .unwrap()
            .is_none());
    }

    #[test]
    fn install_log_outcome_validation() {
        let s = store();
        s.upsert_resource_pack("wb", None).unwrap();
        s.record_pack_install("wb", "1.0.0", Some("client-1"), Some("1.0.0"), "installed")
            .unwrap();
        s.record_pack_install("wb", "1.0.0", None, None, "verify_failed").unwrap();
        s.record_pack_install("wb", "1.0.0", None, None, "rollback").unwrap();

        let err = s
            .record_pack_install("wb", "1.0.0", None, None, "garbage")
            .unwrap_err();
        match err {
            StoreError::Validation(msg) => assert!(msg.contains("invalid")),
            _ => panic!("expected Validation, got {err:?}"),
        }
    }
}
