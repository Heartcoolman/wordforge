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
    match s {
        None => Ok(None),
        Some(v) => parse_dt(v).map(Some),
    }
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

        // 动态 WHERE 子句
        let mut where_parts: Vec<&str> = Vec::new();
        if category.is_some() {
            where_parts.push("category = ?3");
        }
        if status.is_some() {
            where_parts.push("status = ?4");
        }
        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };

        let count_sql = format!("SELECT COUNT(*) FROM feedback_items {}", where_clause);
        let total: i64 = conn.query_row(
            &count_sql,
            rusqlite::params_from_iter(
                [category, status].iter().filter_map(|v| *v),
            ),
            |row| row.get(0),
        )?;

        let list_sql = format!(
            "SELECT id, user_id, category, body, route, created_at,
                    priority, status, assignee_admin_id, resolved_at, resolution
             FROM feedback_items
             {}
             ORDER BY created_at DESC, id DESC
             LIMIT ?1 OFFSET ?2",
            where_clause
        );
        let mut stmt = conn.prepare(&list_sql)?;
        // bind per_page, offset first, then optional filter params
        let items: Vec<FeedbackItem> = match (category, status) {
            (None, None) => stmt
                .query_map(params![per_page as i64, offset as i64], feedback_from_row)?
                .collect::<Result<_, _>>()?,
            (Some(c), None) => stmt
                .query_map(params![per_page as i64, offset as i64, c], feedback_from_row)?
                .collect::<Result<_, _>>()?,
            (None, Some(s)) => stmt
                .query_map(params![per_page as i64, offset as i64, s], feedback_from_row)?
                .collect::<Result<_, _>>()?,
            (Some(c), Some(s)) => stmt
                .query_map(params![per_page as i64, offset as i64, c, s], feedback_from_row)?
                .collect::<Result<_, _>>()?,
        };
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
                return Err(StoreError::Validation {
                    field: "priority".to_string(),
                    message: format!("invalid priority: {}", p),
                });
            }
        }
        if let Some(ref s) = req.status {
            if !VALID_STATUSES.contains(&s.as_str()) {
                return Err(StoreError::Validation {
                    field: "status".to_string(),
                    message: format!("invalid status: {}", s),
                });
            }
        }

        // 仅在 status 切换为 resolved 时写 resolved_at
        let resolved_at_update = match req.status.as_deref() {
            Some("resolved") => "resolved_at = datetime('now'),",
            Some(s) if s != "resolved" => "resolved_at = NULL,",
            _ => "",
        };

        let mut sets = Vec::new();
        if req.priority.is_some() { sets.push("priority = ?2"); }
        if req.status.is_some() { sets.push("status = ?3"); }
        if req.assignee_admin_id.is_some() { sets.push("assignee_admin_id = ?4"); }
        if req.resolution.is_some() { sets.push("resolution = ?5"); }

        if sets.is_empty() && resolved_at_update.is_empty() {
            // 无字段更新，直接返回当前记录
            return self.get_feedback(id);
        }

        let set_clause = format!(
            "{} {}",
            resolved_at_update,
            sets.join(", ")
        ).trim().to_string();

        let sql = format!(
            "UPDATE feedback_items SET {} WHERE id = ?1",
            set_clause
        );
        conn.execute(
            &sql,
            rusqlite::params_from_iter(vec![
                Some(id.to_string()),
                req.priority.clone(),
                req.status.clone(),
                req.assignee_admin_id.clone().flatten(),
                req.resolution.clone(),
            ].into_iter().filter_map(|v| v)),
        )?;
        self.get_feedback(id)
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
