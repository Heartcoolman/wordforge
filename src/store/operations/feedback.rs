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
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn feedback_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedbackItem> {
    Ok(FeedbackItem {
        id: row.get(0)?,
        user_id: row.get(1)?,
        category: row.get(2)?,
        body: row.get(3)?,
        route: row.get(4)?,
        created_at: parse_dt(row.get(5)?)?,
    })
}

impl Store {
    pub fn create_feedback(&self, item: &FeedbackItem) -> Result<(), StoreError> {
        keys::validate_id(&item.id)?;
        keys::validate_id(&item.user_id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO feedback_items (id, user_id, category, body, route, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &item.id,
                &item.user_id,
                item.category.as_deref(),
                &item.body,
                item.route.as_deref(),
                item.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_feedback(
        &self,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<FeedbackItem>, u64), StoreError> {
        let conn = self.conn()?;
        let total: i64 =
            conn.query_row("SELECT COUNT(*) FROM feedback_items", [], |row| row.get(0))?;
        let offset = page.saturating_sub(1).saturating_mul(per_page);
        let mut stmt = conn.prepare(
            "SELECT id, user_id, category, body, route, created_at
             FROM feedback_items
             ORDER BY created_at DESC, id DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let items = stmt
            .query_map(params![per_page as i64, offset as i64], feedback_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((items, total as u64))
    }

    pub fn get_feedback(&self, id: &str) -> Result<Option<FeedbackItem>, StoreError> {
        keys::validate_id(id)?;
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT id, user_id, category, body, route, created_at
                 FROM feedback_items
                 WHERE id = ?1",
                params![id],
                feedback_from_row,
            )
            .optional()?)
    }
}
