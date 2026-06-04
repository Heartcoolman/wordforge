use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackItem {
    pub id: String,
    pub user_id: String,
    pub category: Option<String>,
    pub body: String,
    pub route: Option<String>,
    pub created_at: DateTime<Utc>,
    pub priority: String,
    pub status: String,
    pub assignee_admin_id: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolution: Option<String>,
    /// m022:用户提交时附带的设备指纹快照(platform / appVersion / osName 等任意 JSON shape)。
    /// 旧客户端不传时为 NULL,admin UI 显示 'no snapshot' 占位。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_profile: Option<serde_json::Value>,
    /// m022:答题上下文快照(最近 N 步答题事件 / 当前正在答的题 ID 等)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_snapshot: Option<serde_json::Value>,
    // m030:工单化字段
    pub read_at: Option<DateTime<Utc>>,
    pub first_response_at: Option<DateTime<Utc>>,
    pub csat_score: Option<i64>,
    pub csat_comment: Option<String>,
    #[serde(default)]
    pub dedup_count: i64,
    pub github_issue_url: Option<String>,
    // m030:enrich-only 字段（LEFT JOIN users / 解析 device_profile_json，不落库）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
}

/// m030:工单回复。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackReply {
    pub id: String,
    pub feedback_id: String,
    pub author_kind: String,
    pub author_id: Option<String>,
    pub body: String,
    pub push_inapp: bool,
    pub cc_email: bool,
    pub created_at: DateTime<Utc>,
}

/// m030:工单时间线事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackEvent {
    pub id: String,
    pub feedback_id: String,
    pub kind: String,
    pub actor: Option<String>,
    pub summary: String,
    pub ref_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// m030:工单附件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackAttachment {
    pub id: String,
    pub feedback_id: String,
    pub name: String,
    pub url: String,
    pub kind: String,
    pub created_at: DateTime<Utc>,
}

/// m030:反馈中心 KPI 统计。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackStatusCounts {
    pub open: i64,
    pub in_progress: i64,
    pub resolved: i64,
    pub closed: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackStats {
    pub total: i64,
    pub unresolved: i64,
    pub by_category: BTreeMap<String, i64>,
    pub by_status: FeedbackStatusCounts,
    pub unread_count: i64,
    pub assigned_count: i64,
    pub resolved_count: i64,
    pub median_response_minutes: Option<f64>,
    pub resolve_rate7d: Option<f64>,
    pub csat_avg: Option<f64>,
    pub csat_count: i64,
    /// 近 7 日每日序列(旧→新,缺数据日为 0),驱动 KPI 卡 sparkline。
    pub response_spark: Vec<f64>,
    pub resolve_spark: Vec<f64>,
    pub csat_spark: Vec<f64>,
    /// 周环比:本周(近 7 日)相对上周(8~14 日前)的变化;数据不足时 None。
    pub response_delta: Option<f64>,
    pub resolve_delta: Option<f64>,
    pub csat_delta: Option<f64>,
}

/// m030:工单详情聚合。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackDetail {
    pub item: FeedbackItem,
    pub replies: Vec<FeedbackReply>,
    pub events: Vec<FeedbackEvent>,
    pub attachments: Vec<FeedbackAttachment>,
}

/// m036:反馈中心公告 / FAQ。kind ∈ {announcement, faq}；published 控制对外可见。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackAnnouncement {
    pub id: String,
    pub title: String,
    pub body: String,
    pub kind: String,
    pub published: bool,
    pub author_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// m036:公告 / FAQ 更新入参（PATCH）。全 Option，仅更新出现的字段。
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAnnouncementRequest {
    pub title: Option<String>,
    pub body: Option<String>,
    pub kind: Option<String>,
    pub published: Option<bool>,
}

/// m036:工单回复草稿（composer "存为草稿"）。每工单一份，upsert 覆盖。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackReplyDraft {
    pub feedback_id: String,
    pub body: String,
    pub push_inapp: bool,
    pub cc_email: bool,
    pub author_id: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// m030:create_feedback 可选附件入参。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAttachment {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub kind: Option<String>,
}

/// PATCH /api/admin/feedback/:id 请求体。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFeedbackRequest {
    pub priority: Option<String>,
    pub status: Option<String>,
    pub assignee_admin_id: Option<Option<String>>,
    pub resolution: Option<String>,
}

/// 中位数(就地排序);空集合返回 None。
fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = v.len() / 2;
    Some(if v.len() % 2 == 0 {
        (v[mid - 1] + v[mid]) / 2.0
    } else {
        v[mid]
    })
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn parse_dt_opt(s: Option<String>) -> rusqlite::Result<Option<DateTime<Utc>>> {
    let Some(v) = s else { return Ok(None) };
    // 优先尝试 RFC3339（写入路径），回退到 SQLite datetime() 格式 "YYYY-MM-DD HH:MM:SS"
    if let Ok(dt) = DateTime::parse_from_rfc3339(&v) {
        return Ok(Some(dt.with_timezone(&Utc)));
    }
    use chrono::NaiveDateTime;
    NaiveDateTime::parse_from_str(&v, "%Y-%m-%d %H:%M:%S")
        .map(|ndt| Some(ndt.and_utc()))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

/// feedback_items 基础列（不含 enrich JOIN 列）；feedback_from_row 按此顺序读 0..=18。
const FEEDBACK_COLS: &str = "id, user_id, category, body, route, created_at,
    priority, status, assignee_admin_id, resolved_at, resolution,
    device_profile_json, answer_snapshot_json,
    read_at, first_response_at, csat_score, csat_comment, dedup_count, github_issue_url";

/// 从 device_profile JSON 取字符串字段（兼容 camelCase / snake_case key）。
fn json_str(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(k).and_then(|x| x.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn feedback_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedbackItem> {
    let device_profile: Option<serde_json::Value> = row
        .get::<_, Option<String>>(11)?
        .and_then(|s| serde_json::from_str(&s).ok());
    let (platform, app_version) = match device_profile.as_ref() {
        Some(v) => (
            json_str(v, &["platform"]),
            json_str(v, &["appVersion", "app_version"]),
        ),
        None => (None, None),
    };
    Ok(FeedbackItem {
        id: row.get(0)?,
        user_id: row.get(1)?,
        category: row.get(2)?,
        body: row.get(3)?,
        route: row.get(4)?,
        created_at: parse_dt(row.get(5)?)?,
        priority: row
            .get::<_, String>(6)
            .unwrap_or_else(|_| "normal".to_string()),
        status: row
            .get::<_, String>(7)
            .unwrap_or_else(|_| "open".to_string()),
        assignee_admin_id: row.get(8)?,
        resolved_at: parse_dt_opt(row.get(9)?)?,
        resolution: row.get(10)?,
        device_profile,
        answer_snapshot: row
            .get::<_, Option<String>>(12)?
            .and_then(|s| serde_json::from_str(&s).ok()),
        read_at: parse_dt_opt(row.get(13)?)?,
        first_response_at: parse_dt_opt(row.get(14)?)?,
        csat_score: row.get(15)?,
        csat_comment: row.get(16)?,
        dedup_count: row.get::<_, i64>(17).unwrap_or(0),
        github_issue_url: row.get(18)?,
        platform,
        app_version,
        user_name: None,
    })
}

fn reply_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedbackReply> {
    Ok(FeedbackReply {
        id: row.get(0)?,
        feedback_id: row.get(1)?,
        author_kind: row.get(2)?,
        author_id: row.get(3)?,
        body: row.get(4)?,
        push_inapp: row.get::<_, i64>(5)? != 0,
        cc_email: row.get::<_, i64>(6)? != 0,
        created_at: parse_dt(row.get(7)?)?,
    })
}

fn event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedbackEvent> {
    Ok(FeedbackEvent {
        id: row.get(0)?,
        feedback_id: row.get(1)?,
        kind: row.get(2)?,
        actor: row.get(3)?,
        summary: row.get::<_, String>(4).unwrap_or_default(),
        ref_id: row.get(5)?,
        created_at: parse_dt(row.get(6)?)?,
    })
}

fn attachment_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedbackAttachment> {
    Ok(FeedbackAttachment {
        id: row.get(0)?,
        feedback_id: row.get(1)?,
        name: row.get(2)?,
        url: row.get(3)?,
        kind: row.get(4)?,
        created_at: parse_dt(row.get(5)?)?,
    })
}

fn announcement_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedbackAnnouncement> {
    Ok(FeedbackAnnouncement {
        id: row.get(0)?,
        title: row.get(1)?,
        body: row.get(2)?,
        kind: row.get(3)?,
        published: row.get::<_, i64>(4)? != 0,
        author_id: row.get(5)?,
        created_at: parse_dt(row.get(6)?)?,
        updated_at: parse_dt(row.get(7)?)?,
    })
}

const ANNOUNCEMENT_COLS: &str =
    "id, title, body, kind, published, author_id, created_at, updated_at";

const VALID_PRIORITIES: &[&str] = &["low", "normal", "high", "urgent"];
const VALID_STATUSES: &[&str] = &["open", "in_progress", "resolved", "closed"];
const VALID_ANNOUNCEMENT_KINDS: &[&str] = &["announcement", "faq"];

impl Store {
    pub fn create_feedback(
        &self,
        item: &FeedbackItem,
        attachments: &[NewAttachment],
    ) -> Result<(), StoreError> {
        keys::validate_id(&item.id)?;
        keys::validate_id(&item.user_id)?;
        let mut conn = self.conn()?;
        let device_profile_json = item.device_profile.as_ref().map(|v| v.to_string());
        let answer_snapshot_json = item.answer_snapshot.as_ref().map(|v| v.to_string());
        let created_at = item.created_at.to_rfc3339();
        // item + 附件 + 'submitted' 事件包进单事务，避免中途出错留半成品。
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO feedback_items
                (id, user_id, category, body, route, created_at, priority, status,
                 device_profile_json, answer_snapshot_json, csat_score, csat_comment)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                &item.id,
                &item.user_id,
                item.category.as_deref(),
                &item.body,
                item.route.as_deref(),
                created_at,
                &item.priority,
                &item.status,
                device_profile_json.as_deref(),
                answer_snapshot_json.as_deref(),
                item.csat_score,
                item.csat_comment.as_deref(),
            ],
        )?;
        for att in attachments {
            tx.execute(
                "INSERT INTO feedback_attachments (id, feedback_id, name, url, kind, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    &item.id,
                    &att.name,
                    &att.url,
                    att.kind.as_deref().unwrap_or("image"),
                    &created_at,
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO feedback_events (id, feedback_id, kind, actor, summary, ref_id, created_at)
             VALUES (?1, ?2, 'submitted', ?3, '用户提交反馈', NULL, ?4)",
            params![
                uuid::Uuid::new_v4().to_string(),
                &item.id,
                &item.user_id,
                &created_at,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_feedback(
        &self,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<FeedbackItem>, u64), StoreError> {
        self.list_feedback_filtered(page, per_page, None, None, false, false)
    }

    /// m030:列表查询。新增 unread(read_at IS NULL) / assigned(assignee_admin_id IS NOT NULL)
    /// 过滤维度，并 LEFT JOIN users 补 user_name。
    pub fn list_feedback_filtered(
        &self,
        page: u64,
        per_page: u64,
        category: Option<&str>,
        status: Option<&str>,
        unread: bool,
        assigned: bool,
    ) -> Result<(Vec<FeedbackItem>, u64), StoreError> {
        let conn = self.conn()?;
        let offset = page.saturating_sub(1).saturating_mul(per_page);

        // 过滤参数占位从 ?1 起，LIMIT/OFFSET 跟在末尾
        let mut filter_params: Vec<String> = Vec::new();
        let mut where_parts: Vec<String> = Vec::new();
        if let Some(c) = category {
            filter_params.push(c.to_string());
            where_parts.push(format!("f.category = ?{}", filter_params.len()));
        }
        if let Some(s) = status {
            filter_params.push(s.to_string());
            where_parts.push(format!("f.status = ?{}", filter_params.len()));
        }
        if unread {
            where_parts.push("f.read_at IS NULL".to_string());
        }
        if assigned {
            where_parts.push("f.assignee_admin_id IS NOT NULL".to_string());
        }
        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };
        let limit_idx = filter_params.len() + 1;
        let offset_idx = filter_params.len() + 2;

        let count_sql = format!("SELECT COUNT(*) FROM feedback_items f {}", where_clause);
        let total: i64 = conn.query_row(
            &count_sql,
            rusqlite::params_from_iter(filter_params.iter()),
            |row| row.get(0),
        )?;

        // f.* 列在 0..=18（与 FEEDBACK_COLS 同序），u.username 落 19 列供 enrich。
        let final_sql = format!(
            "SELECT f.id, f.user_id, f.category, f.body, f.route, f.created_at,
                    f.priority, f.status, f.assignee_admin_id, f.resolved_at, f.resolution,
                    f.device_profile_json, f.answer_snapshot_json,
                    f.read_at, f.first_response_at, f.csat_score, f.csat_comment,
                    f.dedup_count, f.github_issue_url, u.username
             FROM feedback_items f
             LEFT JOIN users u ON u.id = f.user_id
             {where_clause}
             ORDER BY f.created_at DESC, f.id DESC
             LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
        );
        let mut stmt = conn.prepare(&final_sql)?;
        let items: Vec<FeedbackItem> = stmt
            .query_map(
                rusqlite::params_from_iter(
                    filter_params.iter().map(|s| s.as_str()).chain(
                        [per_page as i64, offset as i64]
                            .iter()
                            .map(|n| n.to_string())
                            .collect::<Vec<_>>()
                            .iter()
                            .map(|s| s.as_str()),
                    ),
                ),
                |row| {
                    let mut item = feedback_from_row(row)?;
                    item.user_name = row.get::<_, Option<String>>(19)?;
                    Ok(item)
                },
            )?
            .collect::<Result<_, _>>()?;
        Ok((items, total as u64))
    }

    pub fn get_feedback(&self, id: &str) -> Result<Option<FeedbackItem>, StoreError> {
        keys::validate_id(id)?;
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {FEEDBACK_COLS} FROM feedback_items WHERE id = ?1"),
                params![id],
                feedback_from_row,
            )
            .optional()?)
    }

    /// 更新 feedback 状态/优先级/负责人/处理结果（PATCH /api/admin/feedback/:id）。
    pub fn update_feedback(
        &self,
        id: &str,
        req: &UpdateFeedbackRequest,
    ) -> Result<Option<FeedbackItem>, StoreError> {
        keys::validate_id(id)?;
        let conn = self.conn()?;

        if let Some(ref p) = req.priority {
            if !VALID_PRIORITIES.contains(&p.as_str()) {
                return Err(StoreError::Validation(format!("invalid priority: {}", p)));
            }
        }
        if let Some(ref s) = req.status {
            if !VALID_STATUSES.contains(&s.as_str()) {
                return Err(StoreError::Validation(format!("invalid status: {}", s)));
            }
        }

        // 仅在 status 切换为 resolved 时更新 resolved_at
        let resolved_at_sql = match req.status.as_deref() {
            Some("resolved") => " resolved_at = datetime('now'),",
            Some(_) => " resolved_at = NULL,",
            None => "",
        };

        // 构造参数列表：先收集实际要 SET 的值，?1 固定为 id
        let mut set_parts: Vec<String> = Vec::new();
        let mut bind_values: Vec<Option<String>> = Vec::new();
        let mut idx = 2usize;

        if let Some(ref p) = req.priority {
            set_parts.push(format!("priority = ?{idx}"));
            bind_values.push(Some(p.clone()));
            idx += 1;
        }
        if let Some(ref s) = req.status {
            set_parts.push(format!("status = ?{idx}"));
            bind_values.push(Some(s.clone()));
            idx += 1;
        }
        if let Some(ref a) = req.assignee_admin_id {
            set_parts.push(format!("assignee_admin_id = ?{idx}"));
            bind_values.push(a.clone());
            idx += 1;
        }
        if let Some(ref r) = req.resolution {
            set_parts.push(format!("resolution = ?{idx}"));
            bind_values.push(Some(r.clone()));
        }

        if set_parts.is_empty() && resolved_at_sql.is_empty() {
            return Ok(conn
                .query_row(
                    &format!("SELECT {FEEDBACK_COLS} FROM feedback_items WHERE id = ?1"),
                    params![id],
                    feedback_from_row,
                )
                .optional()?);
        }

        let set_clause = format!("{} {}", resolved_at_sql, set_parts.join(", "))
            .trim()
            .trim_end_matches(',')
            .to_string();

        let sql = format!("UPDATE feedback_items SET {} WHERE id = ?1", set_clause);
        let mut stmt = conn.prepare(&sql)?;
        stmt.execute(rusqlite::params_from_iter(
            std::iter::once(Some(id.to_string())).chain(bind_values),
        ))?;
        // 在同一连接内读回更新后的记录，避免 pool_size=1 时二次 conn() 死锁
        Ok(conn
            .query_row(
                &format!("SELECT {FEEDBACK_COLS} FROM feedback_items WHERE id = ?1"),
                params![id],
                feedback_from_row,
            )
            .optional()?)
    }

    // ========================================================================
    // m030:工单化 store 方法
    // ========================================================================

    /// KPI 统计：总数 / 未解决 / 分类构成 / 各状态计数 / 未读 / 已分派 / 已解决 /
    /// 中位首响时长（分钟）/ 近 7 日解决率 / CSAT 均值与样本数。
    pub fn feedback_stats(&self) -> Result<FeedbackStats, StoreError> {
        let conn = self.conn()?;
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM feedback_items", [], |r| r.get(0))?;
        let unresolved: i64 = conn.query_row(
            "SELECT COUNT(*) FROM feedback_items WHERE status IN ('open','in_progress')",
            [],
            |r| r.get(0),
        )?;
        let unread_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM feedback_items WHERE read_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        let assigned_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM feedback_items WHERE assignee_admin_id IS NOT NULL",
            [],
            |r| r.get(0),
        )?;

        // 分类构成
        let mut by_category: BTreeMap<String, i64> = BTreeMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT COALESCE(category, 'unknown') AS c, COUNT(*) FROM feedback_items GROUP BY c",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (c, n) = row?;
                by_category.insert(c, n);
            }
        }

        // 各状态计数
        let mut by_status = FeedbackStatusCounts {
            open: 0,
            in_progress: 0,
            resolved: 0,
            closed: 0,
        };
        {
            let mut stmt =
                conn.prepare("SELECT status, COUNT(*) FROM feedback_items GROUP BY status")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (s, n) = row?;
                match s.as_str() {
                    "open" => by_status.open = n,
                    "in_progress" => by_status.in_progress = n,
                    "resolved" => by_status.resolved = n,
                    "closed" => by_status.closed = n,
                    _ => {}
                }
            }
        }
        let resolved_count = by_status.resolved;

        // 中位首响时长（分钟）：first_response_at - created_at，仅有首响的工单参与。
        let mut deltas: Vec<f64> = {
            let mut stmt = conn.prepare(
                "SELECT created_at, first_response_at FROM feedback_items
                 WHERE first_response_at IS NOT NULL",
            )?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut v = Vec::new();
            for row in rows {
                let (c, f) = row?;
                if let (Ok(Some(c)), Ok(Some(f))) = (parse_dt_opt(Some(c)), parse_dt_opt(Some(f))) {
                    let mins = (f - c).num_seconds() as f64 / 60.0;
                    if mins >= 0.0 {
                        v.push(mins);
                    }
                }
            }
            v
        };
        let median_response_minutes = median(&mut deltas);

        // 近 7 日解决率：近 7 日创建的工单中，已 resolved/closed 占比。
        let created_7d: i64 = conn.query_row(
            "SELECT COUNT(*) FROM feedback_items WHERE datetime(created_at) >= datetime('now','-7 days')",
            [],
            |r| r.get(0),
        )?;
        let resolve_rate7d = if created_7d > 0 {
            let resolved_7d: i64 = conn.query_row(
                "SELECT COUNT(*) FROM feedback_items
                 WHERE datetime(created_at) >= datetime('now','-7 days')
                   AND status IN ('resolved','closed')",
                [],
                |r| r.get(0),
            )?;
            Some(resolved_7d as f64 / created_7d as f64)
        } else {
            None
        };

        // CSAT
        let csat_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM feedback_items WHERE csat_score IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        let csat_avg: Option<f64> = if csat_count > 0 {
            conn.query_row(
                "SELECT AVG(csat_score) FROM feedback_items WHERE csat_score IS NOT NULL",
                [],
                |r| r.get(0),
            )?
        } else {
            None
        };

        // 近 7 日每日序列(sparkline)+ 周环比(本周 vs 上周)。单次拉全表按天分桶。
        let (response_spark, resolve_spark, csat_spark, response_delta, resolve_delta, csat_delta) = {
            // 7 个当日桶:首响时长 / 创建数 / 已解决数 / csat。索引 0=6 天前,6=今天。
            let mut resp_day: Vec<Vec<f64>> = vec![Vec::new(); 7];
            let mut created_day = [0i64; 7];
            let mut resolved_day = [0i64; 7];
            let mut csat_day: Vec<Vec<f64>> = vec![Vec::new(); 7];
            // 周聚合:本周(0..=6 天)与上周(7..=13 天)。
            let (mut this_resp, mut prev_resp) = (Vec::new(), Vec::new());
            let (mut this_created, mut this_resolved) = (0i64, 0i64);
            let (mut prev_created, mut prev_resolved) = (0i64, 0i64);
            let (mut this_csat, mut prev_csat): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());

            let today = Utc::now().date_naive();
            let mut stmt = conn.prepare(
                "SELECT created_at, first_response_at, status, csat_score FROM feedback_items",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<f64>>(3)?,
                ))
            })?;
            for row in rows {
                let (c_raw, fr_raw, status, csat) = row?;
                let Ok(Some(created)) = parse_dt_opt(Some(c_raw)) else {
                    continue;
                };
                let offset = (today - created.date_naive()).num_days();
                if offset < 0 {
                    continue;
                }
                let resp_mins = match parse_dt_opt(fr_raw).ok().flatten() {
                    Some(fr) => {
                        let m = (fr - created).num_seconds() as f64 / 60.0;
                        if m >= 0.0 {
                            Some(m)
                        } else {
                            None
                        }
                    }
                    None => None,
                };
                let is_resolved = matches!(status.as_str(), "resolved" | "closed");
                if offset <= 6 {
                    let idx = (6 - offset) as usize;
                    created_day[idx] += 1;
                    if is_resolved {
                        resolved_day[idx] += 1;
                    }
                    if let Some(m) = resp_mins {
                        resp_day[idx].push(m);
                    }
                    if let Some(s) = csat {
                        csat_day[idx].push(s);
                    }
                    this_created += 1;
                    if is_resolved {
                        this_resolved += 1;
                    }
                    if let Some(m) = resp_mins {
                        this_resp.push(m);
                    }
                    if let Some(s) = csat {
                        this_csat.push(s);
                    }
                } else if offset <= 13 {
                    prev_created += 1;
                    if is_resolved {
                        prev_resolved += 1;
                    }
                    if let Some(m) = resp_mins {
                        prev_resp.push(m);
                    }
                    if let Some(s) = csat {
                        prev_csat.push(s);
                    }
                }
            }

            let avg = |v: &[f64]| -> Option<f64> {
                if v.is_empty() {
                    None
                } else {
                    Some(v.iter().sum::<f64>() / v.len() as f64)
                }
            };
            let resp_spark: Vec<f64> = resp_day
                .iter_mut()
                .map(|d| median(d).unwrap_or(0.0))
                .collect();
            let resolve_spark: Vec<f64> = (0..7)
                .map(|i| {
                    if created_day[i] > 0 {
                        resolved_day[i] as f64 / created_day[i] as f64
                    } else {
                        0.0
                    }
                })
                .collect();
            let csat_spark: Vec<f64> = csat_day.iter().map(|d| avg(d).unwrap_or(0.0)).collect();

            let resp_delta = match (
                median(&mut this_resp.clone()),
                median(&mut prev_resp.clone()),
            ) {
                (Some(t), Some(p)) => Some(t - p),
                _ => None,
            };
            let resolve_delta = if this_created > 0 && prev_created > 0 {
                Some(
                    this_resolved as f64 / this_created as f64
                        - prev_resolved as f64 / prev_created as f64,
                )
            } else {
                None
            };
            let csat_delta = match (avg(&this_csat), avg(&prev_csat)) {
                (Some(t), Some(p)) => Some(t - p),
                _ => None,
            };
            (
                resp_spark,
                resolve_spark,
                csat_spark,
                resp_delta,
                resolve_delta,
                csat_delta,
            )
        };

        Ok(FeedbackStats {
            total,
            unresolved,
            by_category,
            by_status,
            unread_count,
            assigned_count,
            resolved_count,
            median_response_minutes,
            resolve_rate7d,
            csat_avg,
            csat_count,
            response_spark,
            resolve_spark,
            csat_spark,
            response_delta,
            resolve_delta,
            csat_delta,
        })
    }

    /// 工单详情：item（含 enrich user_name/platform/app_version）+ 回复 + 时间线 + 附件。
    pub fn get_feedback_detail(&self, id: &str) -> Result<Option<FeedbackDetail>, StoreError> {
        keys::validate_id(id)?;
        let conn = self.conn()?;
        let item: Option<FeedbackItem> = conn
            .query_row(
                "SELECT f.id, f.user_id, f.category, f.body, f.route, f.created_at,
                        f.priority, f.status, f.assignee_admin_id, f.resolved_at, f.resolution,
                        f.device_profile_json, f.answer_snapshot_json,
                        f.read_at, f.first_response_at, f.csat_score, f.csat_comment,
                        f.dedup_count, f.github_issue_url, u.username
                 FROM feedback_items f
                 LEFT JOIN users u ON u.id = f.user_id
                 WHERE f.id = ?1",
                params![id],
                |row| {
                    let mut item = feedback_from_row(row)?;
                    item.user_name = row.get::<_, Option<String>>(19)?;
                    Ok(item)
                },
            )
            .optional()?;
        let Some(item) = item else {
            return Ok(None);
        };

        let replies: Vec<FeedbackReply> = {
            let mut stmt = conn.prepare(
                "SELECT id, feedback_id, author_kind, author_id, body, push_inapp, cc_email, created_at
                 FROM feedback_replies WHERE feedback_id = ?1 ORDER BY created_at ASC",
            )?;
            let rows = stmt
                .query_map(params![id], reply_from_row)?
                .collect::<Result<_, _>>()?;
            rows
        };
        let events: Vec<FeedbackEvent> = {
            let mut stmt = conn.prepare(
                "SELECT id, feedback_id, kind, actor, summary, ref_id, created_at
                 FROM feedback_events WHERE feedback_id = ?1 ORDER BY created_at ASC",
            )?;
            let rows = stmt
                .query_map(params![id], event_from_row)?
                .collect::<Result<_, _>>()?;
            rows
        };
        let attachments: Vec<FeedbackAttachment> = {
            let mut stmt = conn.prepare(
                "SELECT id, feedback_id, name, url, kind, created_at
                 FROM feedback_attachments WHERE feedback_id = ?1 ORDER BY created_at ASC",
            )?;
            let rows = stmt
                .query_map(params![id], attachment_from_row)?
                .collect::<Result<_, _>>()?;
            rows
        };

        Ok(Some(FeedbackDetail {
            item,
            replies,
            events,
            attachments,
        }))
    }

    /// 新增一条 admin 回复：插 reply + 'reply' 事件；first_response_at 为空则置 now。
    /// 返回新建的回复。工单不存在返回 None。
    pub fn add_reply(
        &self,
        id: &str,
        body: &str,
        push_inapp: bool,
        cc_email: bool,
        author_id: Option<&str>,
    ) -> Result<Option<FeedbackReply>, StoreError> {
        keys::validate_id(id)?;
        let mut conn = self.conn()?;
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM feedback_items WHERE id = ?1",
                params![id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Ok(None);
        }
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let reply_id = uuid::Uuid::new_v4().to_string();
        // reply + 'reply' 事件 + 首响时间戳包进单事务，避免中途出错留半成品。
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO feedback_replies
                (id, feedback_id, author_kind, author_id, body, push_inapp, cc_email, created_at)
             VALUES (?1, ?2, 'admin', ?3, ?4, ?5, ?6, ?7)",
            params![
                &reply_id,
                id,
                author_id,
                body,
                push_inapp as i64,
                cc_email as i64,
                &now_str,
            ],
        )?;
        tx.execute(
            "INSERT INTO feedback_events (id, feedback_id, kind, actor, summary, ref_id, created_at)
             VALUES (?1, ?2, 'reply', ?3, ?4, ?5, ?6)",
            params![
                uuid::Uuid::new_v4().to_string(),
                id,
                author_id,
                body,
                &reply_id,
                &now_str,
            ],
        )?;
        tx.execute(
            "UPDATE feedback_items SET first_response_at = ?2
             WHERE id = ?1 AND first_response_at IS NULL",
            params![id, &now_str],
        )?;
        tx.commit()?;
        Ok(Some(FeedbackReply {
            id: reply_id,
            feedback_id: id.to_string(),
            author_kind: "admin".to_string(),
            author_id: author_id.map(|s| s.to_string()),
            body: body.to_string(),
            push_inapp,
            cc_email,
            created_at: now,
        }))
    }

    /// 分派工单：update assignee + 'assigned' 事件。返回更新后的 item。
    pub fn assign_feedback(
        &self,
        id: &str,
        assignee: Option<&str>,
    ) -> Result<Option<FeedbackItem>, StoreError> {
        keys::validate_id(id)?;
        let conn = self.conn()?;
        let updated = conn.execute(
            "UPDATE feedback_items SET assignee_admin_id = ?2 WHERE id = ?1",
            params![id, assignee],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        let summary = match assignee {
            Some(a) => format!("分派给 {a}"),
            None => "取消分派".to_string(),
        };
        conn.execute(
            "INSERT INTO feedback_events (id, feedback_id, kind, actor, summary, ref_id, created_at)
             VALUES (?1, ?2, 'assigned', ?3, ?4, NULL, ?5)",
            params![
                uuid::Uuid::new_v4().to_string(),
                id,
                assignee,
                summary,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(conn
            .query_row(
                &format!("SELECT {FEEDBACK_COLS} FROM feedback_items WHERE id = ?1"),
                params![id],
                feedback_from_row,
            )
            .optional()?)
    }

    /// 合并重复工单：源 id 标 closed、目标 dedup_count+1，双方各写 'merged' 事件。
    /// 任一不存在返回 false。
    pub fn merge_feedback(&self, id: &str, target_id: &str) -> Result<bool, StoreError> {
        keys::validate_id(id)?;
        keys::validate_id(target_id)?;
        if id == target_id {
            return Err(StoreError::Validation(
                "cannot merge a feedback into itself".into(),
            ));
        }
        let mut conn = self.conn()?;
        let now = Utc::now().to_rfc3339();
        // 先做存在性校验，避免任一不存在时仍对目标 dedup_count 误加。
        let src_exists: bool = conn
            .query_row(
                "SELECT 1 FROM feedback_items WHERE id = ?1",
                params![id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        let tgt_exists: bool = conn
            .query_row(
                "SELECT 1 FROM feedback_items WHERE id = ?1",
                params![target_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !src_exists || !tgt_exists {
            return Ok(false);
        }
        // 4 条变更包进单事务，避免中途出错留半成品。
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE feedback_items SET status = 'closed' WHERE id = ?1",
            params![id],
        )?;
        tx.execute(
            "UPDATE feedback_items SET dedup_count = dedup_count + 1 WHERE id = ?1",
            params![target_id],
        )?;
        tx.execute(
            "INSERT INTO feedback_events (id, feedback_id, kind, actor, summary, ref_id, created_at)
             VALUES (?1, ?2, 'merged', NULL, ?3, ?4, ?5)",
            params![
                uuid::Uuid::new_v4().to_string(),
                id,
                format!("合并到 {target_id}"),
                target_id,
                &now,
            ],
        )?;
        tx.execute(
            "INSERT INTO feedback_events (id, feedback_id, kind, actor, summary, ref_id, created_at)
             VALUES (?1, ?2, 'merged', NULL, ?3, ?4, ?5)",
            params![
                uuid::Uuid::new_v4().to_string(),
                target_id,
                format!("合入重复工单 {id}"),
                id,
                &now,
            ],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// 记录关联的 GitHub Issue：写 github_issue_url + 'github' 事件。
    pub fn set_github_issue(&self, id: &str, url: &str) -> Result<(), StoreError> {
        keys::validate_id(id)?;
        let conn = self.conn()?;
        conn.execute(
            "UPDATE feedback_items SET github_issue_url = ?2 WHERE id = ?1",
            params![id, url],
        )?;
        conn.execute(
            "INSERT INTO feedback_events (id, feedback_id, kind, actor, summary, ref_id, created_at)
             VALUES (?1, ?2, 'github', NULL, ?3, ?4, ?5)",
            params![
                uuid::Uuid::new_v4().to_string(),
                id,
                "创建 GitHub Issue",
                url,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// 全部标记已读：read_at = now WHERE read_at IS NULL。返回受影响行数。
    pub fn mark_all_feedback_read(&self) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let updated = conn.execute(
            "UPDATE feedback_items SET read_at = ?1 WHERE read_at IS NULL",
            params![Utc::now().to_rfc3339()],
        )?;
        Ok(updated as u64)
    }

    /// 单条标记已读（仅首次置 read_at，已读则幂等不覆盖）。
    pub fn set_feedback_read(&self, id: &str) -> Result<(), StoreError> {
        keys::validate_id(id)?;
        let conn = self.conn()?;
        conn.execute(
            "UPDATE feedback_items SET read_at = ?2 WHERE id = ?1 AND read_at IS NULL",
            params![id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    // ========================================================================
    // m036:公告 / FAQ + 回复草稿
    // ========================================================================

    /// 新建公告 / FAQ。kind 缺省 'announcement'；published 缺省 false（存为草稿）。
    pub fn create_announcement(
        &self,
        title: &str,
        body: &str,
        kind: &str,
        published: bool,
        author_id: Option<&str>,
    ) -> Result<FeedbackAnnouncement, StoreError> {
        if !VALID_ANNOUNCEMENT_KINDS.contains(&kind) {
            return Err(StoreError::Validation(format!("invalid kind: {kind}")));
        }
        let conn = self.conn()?;
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO feedback_announcements
                (id, title, body, kind, published, author_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                &id,
                title,
                body,
                kind,
                published as i64,
                author_id,
                &now_str
            ],
        )?;
        Ok(FeedbackAnnouncement {
            id,
            title: title.to_string(),
            body: body.to_string(),
            kind: kind.to_string(),
            published,
            author_id: author_id.map(|s| s.to_string()),
            created_at: now,
            updated_at: now,
        })
    }

    /// 列出公告 / FAQ（按 created_at DESC）。kind=None 返回全部；published=Some(b) 时过滤。
    pub fn list_announcements(
        &self,
        kind: Option<&str>,
        published: Option<bool>,
    ) -> Result<Vec<FeedbackAnnouncement>, StoreError> {
        let conn = self.conn()?;
        let mut where_parts: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();
        if let Some(k) = kind {
            binds.push(k.to_string());
            where_parts.push(format!("kind = ?{}", binds.len()));
        }
        if let Some(p) = published {
            binds.push((p as i64).to_string());
            where_parts.push(format!("published = ?{}", binds.len()));
        }
        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };
        let sql = format!(
            "SELECT {ANNOUNCEMENT_COLS} FROM feedback_announcements {where_clause}
             ORDER BY created_at DESC, id DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(binds.iter()),
                announcement_from_row,
            )?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn get_announcement(&self, id: &str) -> Result<Option<FeedbackAnnouncement>, StoreError> {
        keys::validate_id(id)?;
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {ANNOUNCEMENT_COLS} FROM feedback_announcements WHERE id = ?1"),
                params![id],
                announcement_from_row,
            )
            .optional()?)
    }

    /// 部分更新公告 / FAQ；任一字段出现即更新，updated_at 始终刷新。不存在返回 None。
    pub fn update_announcement(
        &self,
        id: &str,
        req: &UpdateAnnouncementRequest,
    ) -> Result<Option<FeedbackAnnouncement>, StoreError> {
        keys::validate_id(id)?;
        if let Some(ref k) = req.kind {
            if !VALID_ANNOUNCEMENT_KINDS.contains(&k.as_str()) {
                return Err(StoreError::Validation(format!("invalid kind: {k}")));
            }
        }
        let conn = self.conn()?;
        let mut set_parts: Vec<String> = vec!["updated_at = ?2".to_string()];
        let mut binds: Vec<Option<String>> = Vec::new();
        let mut idx = 3usize;
        if let Some(ref t) = req.title {
            set_parts.push(format!("title = ?{idx}"));
            binds.push(Some(t.clone()));
            idx += 1;
        }
        if let Some(ref b) = req.body {
            set_parts.push(format!("body = ?{idx}"));
            binds.push(Some(b.clone()));
            idx += 1;
        }
        if let Some(ref k) = req.kind {
            set_parts.push(format!("kind = ?{idx}"));
            binds.push(Some(k.clone()));
            idx += 1;
        }
        if let Some(p) = req.published {
            set_parts.push(format!("published = ?{idx}"));
            binds.push(Some((p as i64).to_string()));
        }
        let sql = format!(
            "UPDATE feedback_announcements SET {} WHERE id = ?1",
            set_parts.join(", ")
        );
        let mut stmt = conn.prepare(&sql)?;
        let head = vec![Some(id.to_string()), Some(Utc::now().to_rfc3339())];
        stmt.execute(rusqlite::params_from_iter(head.into_iter().chain(binds)))?;
        Ok(conn
            .query_row(
                &format!("SELECT {ANNOUNCEMENT_COLS} FROM feedback_announcements WHERE id = ?1"),
                params![id],
                announcement_from_row,
            )
            .optional()?)
    }

    /// 删除公告 / FAQ。返回是否删除（false=不存在）。
    pub fn delete_announcement(&self, id: &str) -> Result<bool, StoreError> {
        keys::validate_id(id)?;
        let conn = self.conn()?;
        Ok(conn.execute(
            "DELETE FROM feedback_announcements WHERE id = ?1",
            params![id],
        )? > 0)
    }

    /// upsert 工单回复草稿（每工单一份，覆盖旧草稿）。工单不存在返回 None。
    pub fn save_reply_draft(
        &self,
        feedback_id: &str,
        body: &str,
        push_inapp: bool,
        cc_email: bool,
        author_id: Option<&str>,
    ) -> Result<Option<FeedbackReplyDraft>, StoreError> {
        keys::validate_id(feedback_id)?;
        let conn = self.conn()?;
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM feedback_items WHERE id = ?1",
                params![feedback_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Ok(None);
        }
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        conn.execute(
            "INSERT INTO feedback_reply_drafts
                (feedback_id, body, push_inapp, cc_email, author_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(feedback_id) DO UPDATE SET
                body = ?2, push_inapp = ?3, cc_email = ?4, author_id = ?5, updated_at = ?6",
            params![
                feedback_id,
                body,
                push_inapp as i64,
                cc_email as i64,
                author_id,
                &now_str
            ],
        )?;
        Ok(Some(FeedbackReplyDraft {
            feedback_id: feedback_id.to_string(),
            body: body.to_string(),
            push_inapp,
            cc_email,
            author_id: author_id.map(|s| s.to_string()),
            updated_at: now,
        }))
    }

    /// 取某工单的回复草稿（无草稿返回 None）。
    pub fn get_reply_draft(
        &self,
        feedback_id: &str,
    ) -> Result<Option<FeedbackReplyDraft>, StoreError> {
        keys::validate_id(feedback_id)?;
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT feedback_id, body, push_inapp, cc_email, author_id, updated_at
                 FROM feedback_reply_drafts WHERE feedback_id = ?1",
                params![feedback_id],
                |row| {
                    Ok(FeedbackReplyDraft {
                        feedback_id: row.get(0)?,
                        body: row.get(1)?,
                        push_inapp: row.get::<_, i64>(2)? != 0,
                        cc_email: row.get::<_, i64>(3)? != 0,
                        author_id: row.get(4)?,
                        updated_at: parse_dt(row.get(5)?)?,
                    })
                },
            )
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn store() -> Store {
        let s = Store::open(":memory:", 5000, 1).unwrap();
        crate::store::migrate::run(&s).unwrap();
        s
    }

    fn make_item(id: &str) -> FeedbackItem {
        FeedbackItem {
            id: id.to_string(),
            user_id: "user-1".to_string(),
            category: Some("bug".to_string()),
            body: "Something broke".to_string(),
            route: Some("/home".to_string()),
            created_at: Utc::now(),
            priority: "normal".to_string(),
            status: "open".to_string(),
            assignee_admin_id: None,
            resolved_at: None,
            resolution: None,
            device_profile: None,
            answer_snapshot: None,
            read_at: None,
            first_response_at: None,
            csat_score: None,
            csat_comment: None,
            dedup_count: 0,
            github_issue_url: None,
            user_name: None,
            platform: None,
            app_version: None,
        }
    }

    #[test]
    fn create_and_get() {
        let s = store();
        let item = make_item("fb-001");
        s.create_feedback(&item, &[]).unwrap();
        let got = s.get_feedback("fb-001").unwrap().unwrap();
        assert_eq!(got.priority, "normal");
        assert_eq!(got.status, "open");
        assert!(got.assignee_admin_id.is_none());
    }

    #[test]
    fn update_status_sets_resolved_at() {
        let s = store();
        s.create_feedback(&make_item("fb-002"), &[]).unwrap();
        let req = UpdateFeedbackRequest {
            priority: None,
            status: Some("resolved".to_string()),
            assignee_admin_id: None,
            resolution: Some("Fixed in v1.2".to_string()),
        };
        let updated = s.update_feedback("fb-002", &req).unwrap().unwrap();
        assert_eq!(updated.status, "resolved");
        assert!(updated.resolved_at.is_some());
        assert_eq!(updated.resolution.as_deref(), Some("Fixed in v1.2"));
    }

    #[test]
    fn update_priority_and_assignee() {
        let s = store();
        s.create_feedback(&make_item("fb-003"), &[]).unwrap();
        let req = UpdateFeedbackRequest {
            priority: Some("high".to_string()),
            status: None,
            assignee_admin_id: Some(Some("admin-42".to_string())),
            resolution: None,
        };
        let updated = s.update_feedback("fb-003", &req).unwrap().unwrap();
        assert_eq!(updated.priority, "high");
        assert_eq!(updated.assignee_admin_id.as_deref(), Some("admin-42"));
    }

    #[test]
    fn invalid_priority_rejected() {
        let s = store();
        s.create_feedback(&make_item("fb-004"), &[]).unwrap();
        let req = UpdateFeedbackRequest {
            priority: Some("critical".to_string()),
            status: None,
            assignee_admin_id: None,
            resolution: None,
        };
        assert!(s.update_feedback("fb-004", &req).is_err());
    }

    #[test]
    fn list_filtered_by_status() {
        let s = store();
        s.create_feedback(&make_item("fb-005"), &[]).unwrap();
        let mut item2 = make_item("fb-006");
        item2.status = "resolved".to_string();
        s.create_feedback(&item2, &[]).unwrap();

        let (open_items, open_total) = s
            .list_feedback_filtered(1, 10, None, Some("open"), false, false)
            .unwrap();
        assert_eq!(open_total, 1);
        assert_eq!(open_items[0].id, "fb-005");

        let (all_items, all_total) = s.list_feedback(1, 10).unwrap();
        assert_eq!(all_total, 2);
        let _ = all_items;
    }

    #[test]
    fn list_filtered_unread_and_assigned() {
        let s = store();
        // fb-a：默认未读、未分派
        s.create_feedback(&make_item("fb-a"), &[]).unwrap();
        // fb-b：已读 + 已分派
        s.create_feedback(&make_item("fb-b"), &[]).unwrap();
        s.set_feedback_read("fb-b").unwrap();
        s.assign_feedback("fb-b", Some("admin-1")).unwrap();

        // unread 过滤只剩 fb-a
        let (unread, unread_total) = s
            .list_feedback_filtered(1, 10, None, None, true, false)
            .unwrap();
        assert_eq!(unread_total, 1);
        assert_eq!(unread[0].id, "fb-a");

        // assigned 过滤只剩 fb-b
        let (assigned, assigned_total) = s
            .list_feedback_filtered(1, 10, None, None, false, true)
            .unwrap();
        assert_eq!(assigned_total, 1);
        assert_eq!(assigned[0].id, "fb-b");
    }

    #[test]
    fn create_with_attachments_and_csat() {
        let s = store();
        let mut item = make_item("fb-att");
        item.csat_score = Some(4);
        item.csat_comment = Some("还行".to_string());
        let atts = vec![
            NewAttachment {
                name: "screenshot.png".to_string(),
                url: "https://cdn/x.png".to_string(),
                kind: None,
            },
            NewAttachment {
                name: "log.txt".to_string(),
                url: "https://cdn/y.txt".to_string(),
                kind: Some("file".to_string()),
            },
        ];
        s.create_feedback(&item, &atts).unwrap();

        let got = s.get_feedback("fb-att").unwrap().unwrap();
        assert_eq!(got.csat_score, Some(4));
        assert_eq!(got.csat_comment.as_deref(), Some("还行"));

        let detail = s.get_feedback_detail("fb-att").unwrap().unwrap();
        assert_eq!(detail.attachments.len(), 2);
        // kind 缺省落 'image'，显式 'file' 保留
        let by_name: BTreeMap<_, _> = detail
            .attachments
            .iter()
            .map(|a| (a.name.clone(), a.kind.clone()))
            .collect();
        assert_eq!(
            by_name.get("screenshot.png").map(String::as_str),
            Some("image")
        );
        assert_eq!(by_name.get("log.txt").map(String::as_str), Some("file"));
        // create 会写一条 'submitted' 事件
        assert!(detail.events.iter().any(|e| e.kind == "submitted"));
    }

    #[test]
    fn detail_missing_returns_none() {
        let s = store();
        assert!(s.get_feedback_detail("no-such-id").unwrap().is_none());
    }

    #[test]
    fn add_reply_sets_first_response_and_writes_event() {
        let s = store();
        s.create_feedback(&make_item("fb-r"), &[]).unwrap();

        // 工单不存在 → None
        assert!(s
            .add_reply("missing", "hi", false, false, Some("admin-1"))
            .unwrap()
            .is_none());

        let reply = s
            .add_reply("fb-r", "  已收到，正在排查  ", true, false, Some("admin-1"))
            .unwrap()
            .unwrap();
        assert_eq!(reply.author_kind, "admin");
        assert_eq!(reply.author_id.as_deref(), Some("admin-1"));
        assert!(reply.push_inapp);
        assert!(!reply.cc_email);

        let detail = s.get_feedback_detail("fb-r").unwrap().unwrap();
        assert_eq!(detail.replies.len(), 1);
        // first_response_at 应被首条回复置上
        assert!(detail.item.first_response_at.is_some());
        let first_resp = detail.item.first_response_at;
        // reply 事件入时间线
        assert!(detail.events.iter().any(|e| e.kind == "reply"));

        // 二次回复不应覆盖 first_response_at
        s.add_reply("fb-r", "second", false, false, Some("admin-1"))
            .unwrap()
            .unwrap();
        let detail2 = s.get_feedback_detail("fb-r").unwrap().unwrap();
        assert_eq!(detail2.replies.len(), 2);
        assert_eq!(detail2.item.first_response_at, first_resp);
    }

    #[test]
    fn assign_and_unassign_writes_event() {
        let s = store();
        s.create_feedback(&make_item("fb-as"), &[]).unwrap();

        // 不存在 → None
        assert!(s
            .assign_feedback("missing", Some("admin-1"))
            .unwrap()
            .is_none());

        let assigned = s
            .assign_feedback("fb-as", Some("admin-7"))
            .unwrap()
            .unwrap();
        assert_eq!(assigned.assignee_admin_id.as_deref(), Some("admin-7"));

        // 取消分派
        let cleared = s.assign_feedback("fb-as", None).unwrap().unwrap();
        assert!(cleared.assignee_admin_id.is_none());

        let detail = s.get_feedback_detail("fb-as").unwrap().unwrap();
        let assign_events = detail
            .events
            .iter()
            .filter(|e| e.kind == "assigned")
            .count();
        assert_eq!(assign_events, 2);
    }

    #[test]
    fn merge_closes_source_and_bumps_target() {
        let s = store();
        s.create_feedback(&make_item("fb-src"), &[]).unwrap();
        s.create_feedback(&make_item("fb-tgt"), &[]).unwrap();

        // 目标不存在 → false
        assert!(!s.merge_feedback("fb-src", "missing").unwrap());
        // 源不存在 → false
        assert!(!s.merge_feedback("missing", "fb-tgt").unwrap());

        assert!(s.merge_feedback("fb-src", "fb-tgt").unwrap());

        let src = s.get_feedback("fb-src").unwrap().unwrap();
        assert_eq!(src.status, "closed");
        let tgt = s.get_feedback("fb-tgt").unwrap().unwrap();
        assert_eq!(tgt.dedup_count, 1);

        // 双方各写一条 merged 事件
        let src_detail = s.get_feedback_detail("fb-src").unwrap().unwrap();
        assert!(src_detail.events.iter().any(|e| e.kind == "merged"));
        let tgt_detail = s.get_feedback_detail("fb-tgt").unwrap().unwrap();
        assert!(tgt_detail.events.iter().any(|e| e.kind == "merged"));
    }

    #[test]
    fn set_github_issue_persists_url_and_event() {
        let s = store();
        s.create_feedback(&make_item("fb-gh"), &[]).unwrap();
        s.set_github_issue("fb-gh", "https://github.com/o/r/issues/1")
            .unwrap();
        let got = s.get_feedback("fb-gh").unwrap().unwrap();
        assert_eq!(
            got.github_issue_url.as_deref(),
            Some("https://github.com/o/r/issues/1")
        );
        let detail = s.get_feedback_detail("fb-gh").unwrap().unwrap();
        assert!(detail.events.iter().any(|e| e.kind == "github"));
    }

    #[test]
    fn mark_read_single_idempotent_and_mark_all() {
        let s = store();
        s.create_feedback(&make_item("fb-u1"), &[]).unwrap();
        s.create_feedback(&make_item("fb-u2"), &[]).unwrap();

        // 单条标已读
        s.set_feedback_read("fb-u1").unwrap();
        let got = s.get_feedback("fb-u1").unwrap().unwrap();
        let first_read = got.read_at;
        assert!(first_read.is_some());
        // 幂等：再次调用不覆盖
        s.set_feedback_read("fb-u1").unwrap();
        assert_eq!(
            s.get_feedback("fb-u1").unwrap().unwrap().read_at,
            first_read
        );

        // mark-all-read 只影响剩余未读的 fb-u2
        let updated = s.mark_all_feedback_read().unwrap();
        assert_eq!(updated, 1);
        assert!(s.get_feedback("fb-u2").unwrap().unwrap().read_at.is_some());
        // 再次 mark-all 已无未读
        assert_eq!(s.mark_all_feedback_read().unwrap(), 0);
    }

    #[test]
    fn feedback_stats_aggregates() {
        let s = store();
        // 2 open(bug) + 1 in_progress(bug) + 1 resolved(perf)
        s.create_feedback(&make_item("fb-s1"), &[]).unwrap();
        s.create_feedback(&make_item("fb-s2"), &[]).unwrap();
        let mut ip = make_item("fb-s3");
        ip.status = "in_progress".to_string();
        s.create_feedback(&ip, &[]).unwrap();
        let mut done = make_item("fb-s4");
        done.category = Some("perf".to_string());
        done.status = "resolved".to_string();
        done.csat_score = Some(5);
        s.create_feedback(&done, &[]).unwrap();
        let mut done2 = make_item("fb-s5");
        done2.status = "resolved".to_string();
        done2.csat_score = Some(3);
        s.create_feedback(&done2, &[]).unwrap();

        // 一条已读、一条已分派，验证未读/已分派计数
        s.set_feedback_read("fb-s1").unwrap();
        s.assign_feedback("fb-s2", Some("admin-1")).unwrap();
        // 首响 → 进入 median 计算
        s.add_reply("fb-s1", "ack", false, false, Some("admin-1"))
            .unwrap();

        let stats = s.feedback_stats().unwrap();
        assert_eq!(stats.total, 5);
        // open + in_progress
        assert_eq!(stats.unresolved, 3);
        assert_eq!(stats.by_status.open, 2);
        assert_eq!(stats.by_status.in_progress, 1);
        assert_eq!(stats.by_status.resolved, 2);
        assert_eq!(stats.resolved_count, 2);
        // 5 条中 4 条默认 category=bug(s1/s2/s3/s5)、1 条 perf(s4)
        assert_eq!(stats.by_category.get("bug"), Some(&4));
        assert_eq!(stats.by_category.get("perf"), Some(&1));
        // 5 条中 1 条已读 → 4 未读
        assert_eq!(stats.unread_count, 4);
        assert_eq!(stats.assigned_count, 1);
        // 2 条 CSAT，均值 (5+3)/2 = 4.0
        assert_eq!(stats.csat_count, 2);
        assert_eq!(stats.csat_avg, Some(4.0));
        // 有一条首响 → median 非空且 >= 0
        assert!(stats
            .median_response_minutes
            .map(|m| m >= 0.0)
            .unwrap_or(false));
        // 近 7 日刚创建 5 条、其中 2 条 resolved → 解决率 0.4
        assert_eq!(stats.resolve_rate7d, Some(0.4));
    }
}
