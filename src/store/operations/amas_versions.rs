use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

/// AMAS 配置写入来源：人工 / LLM 建议 / LLM 自动应用
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigVersionSource {
    Manual,
    LlmSuggested,
    LlmAuto,
}

impl ConfigVersionSource {
    fn as_str(&self) -> &'static str {
        match self {
            ConfigVersionSource::Manual => "manual",
            ConfigVersionSource::LlmSuggested => "llm_suggested",
            ConfigVersionSource::LlmAuto => "llm_auto",
        }
    }

    fn from_str(s: &str) -> Result<Self, StoreError> {
        match s {
            "manual" => Ok(Self::Manual),
            "llm_suggested" => Ok(Self::LlmSuggested),
            "llm_auto" => Ok(Self::LlmAuto),
            other => Err(StoreError::Validation(format!(
                "unknown amas_config_versions.source value: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmasConfigVersionRow {
    pub id: i64,
    pub version_hash: String,
    pub author_admin_id: String,
    pub source: ConfigVersionSource,
    pub note: Option<String>,
    pub parent_version_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmasConfigVersionDetail {
    pub id: i64,
    pub version_hash: String,
    pub snapshot_json: serde_json::Value,
    pub author_admin_id: String,
    pub source: ConfigVersionSource,
    pub note: Option<String>,
    pub parent_version_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 计算 AMAS 配置的稳定哈希。
/// 与 `amas::monitoring::compute_config_hash` 保持算法一致（DefaultHasher → 16 hex），
/// 这样 `engine_monitoring_events.config_version` 与 `amas_config_versions.version_hash`
/// 可互相 JOIN。
pub fn compute_config_hash(snapshot_json: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    snapshot_json.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

impl Store {
    /// 写入一条配置版本。若 `version_hash` 已存在则返回现有 id（幂等）。
    pub fn insert_amas_config_version(
        &self,
        snapshot_json: &str,
        author_admin_id: &str,
        source: ConfigVersionSource,
        note: Option<&str>,
        parent_version_hash: Option<&str>,
    ) -> Result<(i64, String), StoreError> {
        let hash = compute_config_hash(snapshot_json);
        let conn = self.conn()?;

        // 幂等：若 hash 已存在则直接返回
        if let Some(id) = conn
            .query_row(
                "SELECT id FROM amas_config_versions WHERE version_hash = ?1",
                params![&hash],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        {
            return Ok((id, hash));
        }

        let now = Utc::now();
        conn.execute(
            "INSERT INTO amas_config_versions
             (version_hash, snapshot_json, author_admin_id, source, note, parent_version_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                &hash,
                snapshot_json,
                author_admin_id,
                source.as_str(),
                note,
                parent_version_hash,
                now.to_rfc3339(),
            ],
        )?;
        let id = conn.last_insert_rowid();
        Ok((id, hash))
    }

    /// 列出配置版本（按 created_at 倒序）。
    pub fn list_amas_config_versions(
        &self,
        limit: usize,
    ) -> Result<Vec<AmasConfigVersionRow>, StoreError> {
        let limit = limit.min(500) as i64;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, version_hash, author_admin_id, source, note, parent_version_hash, created_at
             FROM amas_config_versions ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit], |row| {
                Ok(AmasConfigVersionRow {
                    id: row.get(0)?,
                    version_hash: row.get(1)?,
                    author_admin_id: row.get(2)?,
                    source: ConfigVersionSource::from_str(&row.get::<_, String>(3)?)
                        .unwrap_or(ConfigVersionSource::Manual),
                    note: row.get(4)?,
                    parent_version_hash: row.get(5)?,
                    created_at: parse_dt(row.get::<_, String>(6)?)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 取单个版本完整内容（含 snapshot_json）。
    pub fn get_amas_config_version(
        &self,
        version_hash: &str,
    ) -> Result<Option<AmasConfigVersionDetail>, StoreError> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT id, version_hash, snapshot_json, author_admin_id, source, note, parent_version_hash, created_at
                 FROM amas_config_versions WHERE version_hash = ?1",
                params![version_hash],
                |row| {
                    let snapshot_str: String = row.get(2)?;
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        snapshot_str,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, version_hash, snapshot_str, author, source, note, parent, created)) = row
        else {
            return Ok(None);
        };
        let snapshot_json: serde_json::Value =
            serde_json::from_str(&snapshot_str).map_err(StoreError::Serialization)?;
        Ok(Some(AmasConfigVersionDetail {
            id,
            version_hash,
            snapshot_json,
            author_admin_id: author,
            source: ConfigVersionSource::from_str(&source).unwrap_or(ConfigVersionSource::Manual),
            note,
            parent_version_hash: parent,
            created_at: DateTime::parse_from_rfc3339(&created)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| StoreError::Validation(format!("bad created_at: {e}")))?,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_store() -> Store {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        store
    }

    #[test]
    fn insert_and_list_versions() {
        let store = fresh_store();
        let (id1, h1) = store
            .insert_amas_config_version(
                r#"{"a":1}"#,
                "admin-1",
                ConfigVersionSource::Manual,
                Some("first"),
                None,
            )
            .unwrap();
        let (id2, h2) = store
            .insert_amas_config_version(
                r#"{"a":2}"#,
                "admin-1",
                ConfigVersionSource::Manual,
                None,
                Some(&h1),
            )
            .unwrap();
        assert_ne!(h1, h2);
        assert!(id2 > id1);

        let list = store.list_amas_config_versions(10).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].version_hash, h2);
        assert_eq!(list[1].version_hash, h1);
        assert_eq!(list[1].note.as_deref(), Some("first"));
        assert_eq!(list[0].parent_version_hash.as_deref(), Some(h1.as_str()));
    }

    #[test]
    fn insert_is_idempotent_on_same_hash() {
        let store = fresh_store();
        let (id1, h1) = store
            .insert_amas_config_version(r#"{"x":1}"#, "a", ConfigVersionSource::Manual, None, None)
            .unwrap();
        let (id2, h2) = store
            .insert_amas_config_version(r#"{"x":1}"#, "a", ConfigVersionSource::Manual, None, None)
            .unwrap();
        assert_eq!(id1, id2);
        assert_eq!(h1, h2);
        assert_eq!(store.list_amas_config_versions(10).unwrap().len(), 1);
    }

    #[test]
    fn get_returns_full_detail() {
        let store = fresh_store();
        let (_, h) = store
            .insert_amas_config_version(
                r#"{"k":42}"#,
                "a",
                ConfigVersionSource::LlmAuto,
                Some("auto"),
                None,
            )
            .unwrap();
        let detail = store
            .get_amas_config_version(&h)
            .unwrap()
            .expect("must exist");
        assert_eq!(detail.snapshot_json["k"], 42);
        assert_eq!(detail.source, ConfigVersionSource::LlmAuto);
        assert_eq!(detail.note.as_deref(), Some("auto"));
        assert!(store
            .get_amas_config_version("nonexistent")
            .unwrap()
            .is_none());
    }
}
