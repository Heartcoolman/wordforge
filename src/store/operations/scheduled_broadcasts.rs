//! 设备推送「投递时机调度」+「草稿存储」的 Store 操作（m042，D2）。
//!
//! - `scheduled_broadcasts`：延时/指定时间下发的广播队列。受众过滤维度（平台/版本
//!   下限/最近活跃天数/显式 user_ids）落库；定时 worker 扫到期记录复用即时 fan-out。
//! - `push_drafts`：推送编辑器「保存草稿」。仿 m036 feedback_reply_drafts，但 admin
//!   后台仅单运维面、推送 composer 只有一个，故全局单行（id 固定 `default`）upsert。

use rusqlite::{params, OptionalExtension};

use crate::store::{Store, StoreError};

/// 推送草稿的固定单行主键（全局单份）。
pub const PUSH_DRAFT_ID: &str = "default";

/// 一条待发的定时广播（受众过滤维度均已解出为强类型）。
#[derive(Debug, Clone)]
pub struct ScheduledBroadcastRow {
    pub id: String,
    pub title: String,
    pub message: String,
    pub admin_id: String,
    pub platforms: Vec<String>,
    pub version_min: Option<String>,
    pub last_active_days: Option<i64>,
    pub user_ids: Vec<String>,
    pub scheduled_at: String,
}

/// 一条待发定时广播的完整明细（admin 队列视图用，含受众回显 + 时间）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingScheduledBroadcast {
    pub id: String,
    pub title: String,
    pub message: String,
    pub platforms: Vec<String>,
    pub version_min: Option<String>,
    pub last_active_days: Option<i64>,
    pub user_ids: Vec<String>,
    pub scheduled_at: String,
    pub created_at: String,
}

/// 新建定时广播入参。
pub struct NewScheduledBroadcast<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub message: &'a str,
    pub admin_id: &'a str,
    pub platforms: &'a [String],
    pub version_min: Option<&'a str>,
    pub last_active_days: Option<i64>,
    pub user_ids: &'a [String],
    pub scheduled_at: &'a str,
    pub created_at: &'a str,
}

/// 推送草稿。受众过滤维度与 composer 表单一一对应。
#[derive(Debug, Clone)]
pub struct PushDraft {
    pub title: String,
    pub message: String,
    pub platforms: Vec<String>,
    pub version_min: Option<String>,
    pub last_active_days: Option<i64>,
    pub author_id: Option<String>,
    pub updated_at: String,
}

impl Store {
    /// 入队一条定时广播（status='pending'）。受众维度按 None=不过滤存 NULL。
    pub fn insert_scheduled_broadcast(&self, b: &NewScheduledBroadcast) -> Result<(), StoreError> {
        let platforms = encode_str_list(b.platforms)?;
        let user_ids = encode_str_list(b.user_ids)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO scheduled_broadcasts
                (id, title, message, admin_id, platforms, version_min,
                 last_active_days, user_ids, scheduled_at, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', ?10)",
            params![
                b.id,
                b.title,
                b.message,
                b.admin_id,
                platforms,
                b.version_min,
                b.last_active_days,
                user_ids,
                b.scheduled_at,
                b.created_at,
            ],
        )?;
        Ok(())
    }

    /// 取到期（status='pending' 且 scheduled_at <= now）的定时广播，供 worker fan-out。
    /// `now_rfc3339` 由调用方传入（便于测试），limit 控制单批扫描量。
    pub fn list_due_scheduled_broadcasts(
        &self,
        now_rfc3339: &str,
        limit: i64,
    ) -> Result<Vec<ScheduledBroadcastRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, message, admin_id, platforms, version_min,
                    last_active_days, user_ids, scheduled_at
             FROM scheduled_broadcasts
             WHERE status = 'pending' AND scheduled_at <= ?1
             ORDER BY scheduled_at ASC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![now_rfc3339, limit], |r| {
                Ok(ScheduledBroadcastRow {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    message: r.get(2)?,
                    admin_id: r.get(3)?,
                    platforms: decode_str_list(r.get::<_, Option<String>>(4)?),
                    version_min: r.get(5)?,
                    last_active_days: r.get(6)?,
                    user_ids: decode_str_list(r.get::<_, Option<String>>(7)?),
                    scheduled_at: r.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// fan-out 前原子抢占（status: 'pending' → 'sending'）。返回 true 表示本 worker 独占了
    /// 该行可下发；false 表示该行已被取消（cancel 抢先把 pending 改 canceled）或不存在，
    /// worker 应跳过、绝不下发。与 cancel 的 `WHERE status='pending'` 互斥：单条 UPDATE 在
    /// SQLite 中原子，二者只有一个能命中 pending 行。闭合「cancel 返回成功但已群发」的 TOCTOU。
    pub fn claim_scheduled_broadcast_for_send(&self, id: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let n = conn.execute(
            "UPDATE scheduled_broadcasts SET status = 'sending'
             WHERE id = ?1 AND status = 'pending'",
            params![id],
        )?;
        Ok(n > 0)
    }

    /// 复活上次扫描崩溃/进程重启遗留的 'sending' 行（抢占后未及 mark_sent/mark_failed）。
    /// worker 扫描严格顺序且 leader 单实例，故扫描起点的 'sending' 必为陈旧，重置回 'pending'
    /// 以便重新下发（at-least-once，宁可重发不可永久搁置）。返回重置行数。
    pub fn reset_stale_sending_broadcasts(&self) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let n = conn.execute(
            "UPDATE scheduled_broadcasts SET status = 'pending' WHERE status = 'sending'",
            [],
        )?;
        Ok(n as u64)
    }

    /// 标记定时广播已发送（status='sent' + sent_count + sent_at）。仅在 'sending'（已被本
    /// worker 抢占）时生效，防止抢占失败/被取消的行被误覆盖回 sent。
    pub fn mark_scheduled_broadcast_sent(
        &self,
        id: &str,
        sent_count: i64,
        sent_at: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE scheduled_broadcasts
             SET status = 'sent', sent_count = ?2, sent_at = ?3, error = NULL
             WHERE id = ?1 AND status = 'sending'",
            params![id, sent_count, sent_at],
        )?;
        Ok(())
    }

    /// 标记定时广播失败（status='failed' + error）。受众过滤后零命中也记 failed。
    /// 同样仅在 'sending' 时生效，与抢占语义一致。
    pub fn mark_scheduled_broadcast_failed(&self, id: &str, error: &str) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE scheduled_broadcasts SET status = 'failed', error = ?2
             WHERE id = ?1 AND status = 'sending'",
            params![id, error],
        )?;
        Ok(())
    }

    /// W2-2:列出全部待发（status='pending'）定时广播，按计划时间升序。受众维度回显
    /// 沿用 decode_str_list 的 NULL=不过滤语义。复用 idx_scheduled_broadcasts_due。
    pub fn list_scheduled_broadcasts(
        &self,
        limit: i64,
    ) -> Result<Vec<PendingScheduledBroadcast>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, message, platforms, version_min, last_active_days,
                    user_ids, scheduled_at, created_at
             FROM scheduled_broadcasts
             WHERE status = 'pending'
             ORDER BY scheduled_at ASC
             LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |r| {
                Ok(PendingScheduledBroadcast {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    message: r.get(2)?,
                    platforms: decode_str_list(r.get::<_, Option<String>>(3)?),
                    version_min: r.get(4)?,
                    last_active_days: r.get(5)?,
                    user_ids: decode_str_list(r.get::<_, Option<String>>(6)?),
                    scheduled_at: r.get(7)?,
                    created_at: r.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// W2-2:取消一条待发定时广播。`UPDATE...WHERE id=? AND status='pending'` 原子抢占——
    /// 受影响行数=0 表示该条已被 60s 扫描 worker fan-out（或不存在），返回 false 让路由回 409，
    /// 避免把已发出的误标取消。
    pub fn cancel_scheduled_broadcast(&self, id: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let n = conn.execute(
            "UPDATE scheduled_broadcasts SET status = 'canceled'
             WHERE id = ?1 AND status = 'pending'",
            params![id],
        )?;
        Ok(n > 0)
    }

    /// upsert 推送草稿（全局单份，覆盖旧草稿）。
    pub fn save_push_draft(
        &self,
        title: &str,
        message: &str,
        platforms: &[String],
        version_min: Option<&str>,
        last_active_days: Option<i64>,
        author_id: Option<&str>,
    ) -> Result<PushDraft, StoreError> {
        let platforms_json = encode_str_list(platforms)?;
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO push_drafts
                (id, title, message, platforms, version_min, last_active_days, author_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                title = ?2, message = ?3, platforms = ?4, version_min = ?5,
                last_active_days = ?6, author_id = ?7, updated_at = ?8",
            params![
                PUSH_DRAFT_ID,
                title,
                message,
                platforms_json,
                version_min,
                last_active_days,
                author_id,
                &now,
            ],
        )?;
        Ok(PushDraft {
            title: title.to_string(),
            message: message.to_string(),
            platforms: platforms.to_vec(),
            version_min: version_min.map(|s| s.to_string()),
            last_active_days,
            author_id: author_id.map(|s| s.to_string()),
            updated_at: now,
        })
    }

    /// 取推送草稿（无则 None）。
    pub fn get_push_draft(&self) -> Result<Option<PushDraft>, StoreError> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT title, message, platforms, version_min, last_active_days, author_id, updated_at
                 FROM push_drafts WHERE id = ?1",
                params![PUSH_DRAFT_ID],
                |r| {
                    Ok(PushDraft {
                        title: r.get(0)?,
                        message: r.get(1)?,
                        platforms: decode_str_list(r.get::<_, Option<String>>(2)?),
                        version_min: r.get(3)?,
                        last_active_days: r.get(4)?,
                        author_id: r.get(5)?,
                        updated_at: r.get(6)?,
                    })
                },
            )
            .optional()?)
    }

    /// 删除推送草稿（不存在返回 false）。
    pub fn delete_push_draft(&self) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        Ok(conn.execute(
            "DELETE FROM push_drafts WHERE id = ?1",
            params![PUSH_DRAFT_ID],
        )? > 0)
    }
}

/// 字符串数组 → JSON 文本；空数组存 NULL（语义=该维度不过滤）。
fn encode_str_list(list: &[String]) -> Result<Option<String>, StoreError> {
    if list.is_empty() {
        Ok(None)
    } else {
        Ok(Some(serde_json::to_string(list)?))
    }
}

/// JSON 文本 → 字符串数组；NULL / 解析失败回落空数组。
fn decode_str_list(json: Option<String>) -> Vec<String> {
    json.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn store() -> Store {
        let s = Store::open(":memory:", 5000, 1).unwrap();
        s.run_migrations().unwrap();
        s
    }

    #[test]
    fn insert_and_list_due_respects_status_and_time() {
        let store = store();
        let now = Utc::now();
        let past = (now - chrono::Duration::hours(1)).to_rfc3339();
        let future = (now + chrono::Duration::hours(1)).to_rfc3339();
        let created = now.to_rfc3339();

        store
            .insert_scheduled_broadcast(&NewScheduledBroadcast {
                id: "due-1",
                title: "T1",
                message: "M1",
                admin_id: "adm",
                platforms: &["ios".into()],
                version_min: Some("1.2.3"),
                last_active_days: Some(7),
                user_ids: &[],
                scheduled_at: &past,
                created_at: &created,
            })
            .unwrap();
        store
            .insert_scheduled_broadcast(&NewScheduledBroadcast {
                id: "future-1",
                title: "T2",
                message: "M2",
                admin_id: "adm",
                platforms: &[],
                version_min: None,
                last_active_days: None,
                user_ids: &[],
                scheduled_at: &future,
                created_at: &created,
            })
            .unwrap();

        let due = store
            .list_due_scheduled_broadcasts(&now.to_rfc3339(), 100)
            .unwrap();
        assert_eq!(due.len(), 1, "只有过期的 pending 应到期");
        let r = &due[0];
        assert_eq!(r.id, "due-1");
        assert_eq!(r.platforms, vec!["ios".to_string()]);
        assert_eq!(r.version_min.as_deref(), Some("1.2.3"));
        assert_eq!(r.last_active_days, Some(7));
        assert!(r.user_ids.is_empty());
    }

    #[test]
    fn mark_sent_removes_from_due() {
        let store = store();
        let now = Utc::now();
        let past = (now - chrono::Duration::minutes(5)).to_rfc3339();
        store
            .insert_scheduled_broadcast(&NewScheduledBroadcast {
                id: "x",
                title: "T",
                message: "M",
                admin_id: "adm",
                platforms: &[],
                version_min: None,
                last_active_days: None,
                user_ids: &[],
                scheduled_at: &past,
                created_at: &now.to_rfc3339(),
            })
            .unwrap();
        // 须先抢占（pending→sending）mark_sent 才生效。
        assert!(store.claim_scheduled_broadcast_for_send("x").unwrap());
        store
            .mark_scheduled_broadcast_sent("x", 42, &now.to_rfc3339())
            .unwrap();
        let due = store
            .list_due_scheduled_broadcasts(&now.to_rfc3339(), 100)
            .unwrap();
        assert!(due.is_empty(), "sent 后不应再到期");
    }

    #[test]
    fn mark_failed_removes_from_due() {
        let store = store();
        let now = Utc::now();
        let past = (now - chrono::Duration::minutes(5)).to_rfc3339();
        store
            .insert_scheduled_broadcast(&NewScheduledBroadcast {
                id: "y",
                title: "T",
                message: "M",
                admin_id: "adm",
                platforms: &[],
                version_min: None,
                last_active_days: None,
                user_ids: &[],
                scheduled_at: &past,
                created_at: &now.to_rfc3339(),
            })
            .unwrap();
        // 须先抢占（pending→sending）mark_failed 才生效。
        assert!(store.claim_scheduled_broadcast_for_send("y").unwrap());
        store
            .mark_scheduled_broadcast_failed("y", "EMPTY_AUDIENCE")
            .unwrap();
        let due = store
            .list_due_scheduled_broadcasts(&now.to_rfc3339(), 100)
            .unwrap();
        assert!(due.is_empty(), "failed 后不应再到期");
    }

    #[test]
    fn list_pending_and_cancel_atomic_preemption() {
        let store = store();
        let now = Utc::now();
        let future = (now + chrono::Duration::hours(2)).to_rfc3339();
        let created = now.to_rfc3339();
        for (id, sa) in [("p1", &future), ("p2", &future)] {
            store
                .insert_scheduled_broadcast(&NewScheduledBroadcast {
                    id,
                    title: "T",
                    message: "M",
                    admin_id: "adm",
                    platforms: &["ios".into()],
                    version_min: Some("1.0.0"),
                    last_active_days: None,
                    user_ids: &[],
                    scheduled_at: sa,
                    created_at: &created,
                })
                .unwrap();
        }
        // 列出两条 pending，受众回显正确。
        let pending = store.list_scheduled_broadcasts(100).unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].platforms, vec!["ios".to_string()]);
        assert_eq!(pending[0].version_min.as_deref(), Some("1.0.0"));

        // 取消 p1：成功；列表只剩 p2。
        assert!(store.cancel_scheduled_broadcast("p1").unwrap());
        let after = store.list_scheduled_broadcasts(100).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, "p2");
        // 取消已 canceled 的 p1 → false（非 pending，原子抢占失败）。
        assert!(!store.cancel_scheduled_broadcast("p1").unwrap());

        // 抢占 p2（worker fan-out 前 pending→sending）后，cancel 命中 0 行返回 false：
        // 闭合「cancel 返回成功但已群发」的 TOCTOU。
        assert!(store.claim_scheduled_broadcast_for_send("p2").unwrap());
        assert!(!store.cancel_scheduled_broadcast("p2").unwrap());
        // 已抢占的行不再在 pending 列表。
        assert!(store.list_scheduled_broadcasts(100).unwrap().is_empty());
        // mark_sent 在 'sending' 上生效。
        store.mark_scheduled_broadcast_sent("p2", 3, &created).unwrap();

        // 反向：先 cancel 抢先，worker 抢占必失败 → 不下发。
        store
            .insert_scheduled_broadcast(&NewScheduledBroadcast {
                id: "p3",
                title: "T",
                message: "M",
                admin_id: "adm",
                platforms: &[],
                version_min: None,
                last_active_days: None,
                user_ids: &[],
                scheduled_at: &future,
                created_at: &created,
            })
            .unwrap();
        assert!(store.cancel_scheduled_broadcast("p3").unwrap());
        assert!(!store.claim_scheduled_broadcast_for_send("p3").unwrap());
    }

    #[test]
    fn push_draft_upsert_get_delete() {
        let store = store();
        assert!(store.get_push_draft().unwrap().is_none());

        let saved = store
            .save_push_draft(
                "标题",
                "正文",
                &["web".into(), "ios".into()],
                Some("v1.0.0"),
                Some(30),
                Some("adm-1"),
            )
            .unwrap();
        assert_eq!(saved.title, "标题");

        let got = store.get_push_draft().unwrap().unwrap();
        assert_eq!(got.message, "正文");
        assert_eq!(got.platforms, vec!["web".to_string(), "ios".to_string()]);
        assert_eq!(got.version_min.as_deref(), Some("v1.0.0"));
        assert_eq!(got.last_active_days, Some(30));

        // 覆盖语义:再存一次只剩一份
        store
            .save_push_draft("新标题", "新正文", &[], None, None, Some("adm-1"))
            .unwrap();
        let got2 = store.get_push_draft().unwrap().unwrap();
        assert_eq!(got2.title, "新标题");
        assert!(got2.platforms.is_empty());
        assert!(got2.version_min.is_none());

        assert!(store.delete_push_draft().unwrap());
        assert!(store.get_push_draft().unwrap().is_none());
        assert!(!store.delete_push_draft().unwrap(), "重复删返回 false");
    }
}
