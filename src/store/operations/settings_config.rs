//! m033:可扩展设置配置存储。settings.html 的 11 个面板(site/auth/roles/email/
//! sms/storage/db/limits/audit/backup/keys)各自按 section 持久化为 JSON;
//! 另含快照(snapshot/rollback)存全部 section 的聚合。

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::store::{Store, StoreError};

/// 单条 section 配置行。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSection {
    pub section: String,
    /// 该 section 的任意 JSON 配置体。
    pub json: serde_json::Value,
    pub updated_at: String,
}

/// 快照元信息(列表用,不含 json 体)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshotMeta {
    pub id: String,
    pub label: Option<String>,
    pub created_at: String,
}

impl Store {
    /// 读取全部 section 配置(按 section 升序)。
    pub fn list_settings_config(&self) -> Result<Vec<SettingsSection>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT section, json, updated_at FROM settings_config ORDER BY section ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let raw: String = r.get(1)?;
                Ok(SettingsSection {
                    section: r.get(0)?,
                    json: serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null),
                    updated_at: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 读取单个 section 的原始(未遮蔽)JSON 配置体。不存在返回 None。
    pub fn get_settings_config(
        &self,
        section: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT json FROM settings_config WHERE section = ?1",
                params![section],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)))
    }

    /// upsert 单个 section。json 必须为合法 JSON object/value;调用方负责验证业务字段。
    pub fn upsert_settings_config(
        &self,
        section: &str,
        json: &serde_json::Value,
    ) -> Result<SettingsSection, StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        let raw = serde_json::to_string(json)?;
        conn.execute(
            "INSERT INTO settings_config (section, json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(section) DO UPDATE SET json=?2, updated_at=?3",
            params![section, raw, now],
        )?;
        Ok(SettingsSection {
            section: section.to_string(),
            json: json.clone(),
            updated_at: now,
        })
    }

    /// 创建一个快照:把当前全部 section 聚合为 {section: json} 落库。返回快照 id。
    pub fn create_settings_snapshot(
        &self,
        label: Option<&str>,
    ) -> Result<SettingsSnapshotMeta, StoreError> {
        let sections = self.list_settings_config()?;
        let mut map = serde_json::Map::new();
        for s in sections {
            map.insert(s.section, s.json);
        }
        let raw = serde_json::to_string(&serde_json::Value::Object(map))?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO settings_snapshots (id, label, json, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, label, raw, now],
        )?;
        Ok(SettingsSnapshotMeta {
            id,
            label: label.map(str::to_string),
            created_at: now,
        })
    }

    /// 列出快照元信息(按创建时间倒序)。
    pub fn list_settings_snapshots(
        &self,
        limit: usize,
    ) -> Result<Vec<SettingsSnapshotMeta>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, label, created_at FROM settings_snapshots
             ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(SettingsSnapshotMeta {
                    id: r.get(0)?,
                    label: r.get(1)?,
                    created_at: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 回滚到指定快照:用快照的聚合 JSON 重建 settings_config(全量替换)。
    /// 快照不存在返回 `Ok(None)`(供路由层映射 404),否则返回回滚后的全部 section。
    pub fn restore_settings_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<Vec<SettingsSection>>, StoreError> {
        let conn = self.conn()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT json FROM settings_snapshots WHERE id=?1",
                params![snapshot_id],
                |r| r.get(0),
            )
            .optional()?;
        let raw = match raw {
            Some(r) => r,
            None => return Ok(None),
        };
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&raw)?;
        let now = chrono::Utc::now().to_rfc3339();
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM settings_config", [])?;
        for (section, json) in &map {
            let body = serde_json::to_string(json)?;
            tx.execute(
                "INSERT INTO settings_config (section, json, updated_at) VALUES (?1, ?2, ?3)",
                params![section, body, now],
            )?;
        }
        tx.commit()?;
        // 直接在本连接读回(避免从 size=1 连接池二次借用导致死锁)。
        let mut out: Vec<SettingsSection> = map
            .into_iter()
            .map(|(section, json)| SettingsSection {
                section,
                json,
                updated_at: now.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.section.cmp(&b.section));
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    #[test]
    fn upsert_and_list_roundtrip() {
        let store = test_store();
        store.run_migrations().unwrap();
        store
            .upsert_settings_config("site", &serde_json::json!({"title": "WordForge"}))
            .unwrap();
        store
            .upsert_settings_config("site", &serde_json::json!({"title": "WF2"}))
            .unwrap();
        let all = store.list_settings_config().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].section, "site");
        assert_eq!(all[0].json["title"], "WF2");
    }

    #[test]
    fn snapshot_and_restore() {
        let store = test_store();
        store.run_migrations().unwrap();
        store
            .upsert_settings_config("site", &serde_json::json!({"title": "A"}))
            .unwrap();
        let snap = store
            .create_settings_snapshot(Some("before-change"))
            .unwrap();
        // 改了再回滚
        store
            .upsert_settings_config("site", &serde_json::json!({"title": "B"}))
            .unwrap();
        store
            .upsert_settings_config("auth", &serde_json::json!({"mfa": true}))
            .unwrap();
        let restored = store.restore_settings_snapshot(&snap.id).unwrap().unwrap();
        // 回滚后仅剩快照时刻的 site=A
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].section, "site");
        assert_eq!(restored[0].json["title"], "A");
    }

    #[test]
    fn restore_unknown_snapshot_returns_none() {
        let store = test_store();
        store.run_migrations().unwrap();
        assert!(store.restore_settings_snapshot("nope").unwrap().is_none());
    }
}
