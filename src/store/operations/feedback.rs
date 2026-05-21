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
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            0, rusqlite::types::Type::Text, Box::new(e),
        ))
}

fn feedback_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedbackItem> {
    Ok(FeedbackItem {
        id: row.get(0)?,
        user_id: row.get(1)?,
        category: row.get(2)?,
        body: row.get(3)?,
        route: row.get(4)?,
        created_at: parse_dt(row.get(5)?)?,
        priority: row.get::<_, String>(6).unwrap_or_else(|_| "normal".to_string()),
        status: row.get::<_, String>(7).unwrap_or_else(|_| "open".to_string()),
        assignee_admin_id: row.get(8)?,
        resolved_at: parse_dt_opt(row.get(9)?)?,
        resolution: row.get(10)?,
    })
}

const VALID_PRIORITIES: &[&str] = &["low", "normal", "high", "urgent"];
const VALID_STATUSES: &[&str] = &["open", "in_progress", "resolved", "closed"];

impl Store {
    pub fn create_feedback(&self, item: &FeedbackItem) -> Result<(), StoreError> {
        keys::validate_id(&item.id)?;
        keys::validate_id(&item.user_id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO feedback_items
                (id, user_id, category, body, route, created_at, priority, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &item.id,
                &item.user_id,
                item.category.as_deref(),
                &item.body,
                item.route.as_deref(),
                item.created_at.to_rfc3339(),
                &item.priority,
                &item.status,
            ],
        )?;
        Ok(())
    }

    pub fn list_feedback(
        &self,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<FeedbackItem>, u64), StoreError> {
        self.list_feedback_filtered(page, per_page, None, None)
    }

    pub fn list_feedback_filtered(
        &self,
        page: u64,
        per_page: u64,
        category: Option<&str>,
        status: Option<&str>,
    ) -> Result<(Vec<FeedbackItem>, u64), StoreError> {
        let conn = self.conn()?;
        let offset = page.saturating_sub(1).saturating_mul(per_page);

        // 过滤参数占位从 ?1 起，LIMIT/OFFSET 跟在末尾
        let mut filter_params: Vec<String> = Vec::new();
        let mut where_parts: Vec<String> = Vec::new();
        if let Some(c) = category {
            filter_params.push(c.to_string());
            where_parts.push(format!("category = ?{}", filter_params.len()));
        }
        if let Some(s) = status {
            filter_params.push(s.to_string());
            where_parts.push(format!("status = ?{}", filter_params.len()));
        }
        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };
        let limit_idx = filter_params.len() + 1;
        let offset_idx = filter_params.len() + 2;

        let count_sql = format!("SELECT COUNT(*) FROM feedback_items {}", where_clause);
        let total: i64 = conn.query_row(
            &count_sql,
            rusqlite::params_from_iter(filter_params.iter()),
            |row| row.get(0),
        )?;

        let list_sql = format!(
            "SELECT id, user_id, category, body, route, created_at,
                    priority, status, assignee_admin_id, resolved_at, resolution
             FROM feedback_items
             {}
             ORDER BY created_at DESC, id DESC
             LIMIT ?{limit_idx} OFFSET ?{offset_idx}",
            where_clause,
        );
        let mut stmt = conn.prepare(&list_sql)?;
        let items: Vec<FeedbackItem> = stmt
            .query_map(
                rusqlite::params_from_iter(
                    filter_params.iter().map(|s| s.as_str())
                        .chain([per_page as i64, offset as i64]
                            .iter().map(|n| n.to_string()).collect::<Vec<_>>().iter().map(|s| s.as_str()))
                ),
                feedback_from_row,
            )?
            .collect::<Result<_, _>>()?;
        Ok((items, total as u64))
    }

    pub fn get_feedback(&self, id: &str) -> Result<Option<FeedbackItem>, StoreError> {
        keys::validate_id(id)?;
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT id, user_id, category, body, route, created_at,
                        priority, status, assignee_admin_id, resolved_at, resolution
                 FROM feedback_items
                 WHERE id = ?1",
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
                    "SELECT id, user_id, category, body, route, created_at,
                            priority, status, assignee_admin_id, resolved_at, resolution
                     FROM feedback_items WHERE id = ?1",
                    params![id],
                    feedback_from_row,
                )
                .optional()?);
        }

        let set_clause = format!(
            "{} {}",
            resolved_at_sql,
            set_parts.join(", ")
        )
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
                "SELECT id, user_id, category, body, route, created_at,
                        priority, status, assignee_admin_id, resolved_at, resolution
                 FROM feedback_items WHERE id = ?1",
                params![id],
                feedback_from_row,
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
        }
    }

    #[test]
    fn create_and_get() {
        let s = store();
        let item = make_item("fb-001");
        s.create_feedback(&item).unwrap();
        let got = s.get_feedback("fb-001").unwrap().unwrap();
        assert_eq!(got.priority, "normal");
        assert_eq!(got.status, "open");
        assert!(got.assignee_admin_id.is_none());
    }

    #[test]
    fn update_status_sets_resolved_at() {
        let s = store();
        s.create_feedback(&make_item("fb-002")).unwrap();
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
        s.create_feedback(&make_item("fb-003")).unwrap();
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
        s.create_feedback(&make_item("fb-004")).unwrap();
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
        s.create_feedback(&make_item("fb-005")).unwrap();
        let mut item2 = make_item("fb-006");
        item2.status = "resolved".to_string();
        s.create_feedback(&item2).unwrap();

        let (open_items, open_total) = s.list_feedback_filtered(1, 10, None, Some("open")).unwrap();
        assert_eq!(open_total, 1);
        assert_eq!(open_items[0].id, "fb-005");

        let (all_items, all_total) = s.list_feedback(1, 10).unwrap();
        assert_eq!(all_total, 2);
        let _ = all_items;
    }
}
