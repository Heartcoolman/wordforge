use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

type SuggestionRow = (
    i64,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<f64>,
    Option<i64>,
    Option<i64>,
    Option<f64>,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionStatus {
    Pending,
    Approved,
    Rejected,
    Superseded,
    Expired,
    AutoApplied,
}

impl SuggestionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
            Self::Expired => "expired",
            Self::AutoApplied => "auto_applied",
        }
    }

    pub fn parse(s: &str) -> Result<Self, StoreError> {
        Ok(match s {
            "pending" => Self::Pending,
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            "superseded" => Self::Superseded,
            "expired" => Self::Expired,
            "auto_applied" => Self::AutoApplied,
            other => return Err(StoreError::Validation(format!("unknown status: {other}"))),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuningSuggestionRow {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub based_on_version_hash: String,
    pub patch_json: serde_json::Value,
    pub rationale: String,
    pub evidence_json: serde_json::Value,
    pub status: SuggestionStatus,
    pub decided_by: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision_note: Option<String>,
    pub cost_usd: Option<f64>,
    pub tokens_input: Option<u64>,
    pub tokens_output: Option<u64>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct InsertSuggestion {
    pub based_on_version_hash: String,
    pub patch_json: String,
    pub rationale: String,
    pub evidence_json: String,
    pub cost_usd: Option<f64>,
    pub tokens_input: Option<u64>,
    pub tokens_output: Option<u64>,
    pub confidence: Option<f64>,
    pub initial_status: SuggestionStatus, // 通常 Pending；auto-apply 直接置 AutoApplied
    pub decided_by: Option<String>,
    pub decision_note: Option<String>,
}

fn parse_dt(s: String) -> Result<DateTime<Utc>, StoreError> {
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| StoreError::Validation(format!("bad datetime: {e}")))
}

fn row_to_suggestion(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<SuggestionRow> {
    Ok((
        row.get::<_, i64>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, String>(4)?,
        row.get::<_, String>(5)?,
        row.get::<_, String>(6)?,
        row.get::<_, Option<String>>(7)?,
        row.get::<_, Option<String>>(8)?,
        row.get::<_, Option<String>>(9)?,
        row.get::<_, Option<f64>>(10)?,
        row.get::<_, Option<i64>>(11)?,
        row.get::<_, Option<i64>>(12)?,
        row.get::<_, Option<f64>>(13)?,
    ))
}

fn build(
    (
        id,
        created_at,
        based_on,
        patch,
        rationale,
        evidence,
        status,
        decided_by,
        decided_at,
        decision_note,
        cost,
        tin,
        tout,
        conf,
    ): SuggestionRow,
) -> Result<TuningSuggestionRow, StoreError> {
    Ok(TuningSuggestionRow {
        id,
        created_at: parse_dt(created_at)?,
        based_on_version_hash: based_on,
        patch_json: serde_json::from_str(&patch).map_err(StoreError::Serialization)?,
        rationale,
        evidence_json: serde_json::from_str(&evidence).map_err(StoreError::Serialization)?,
        status: SuggestionStatus::parse(&status)?,
        decided_by,
        decided_at: decided_at.map(parse_dt).transpose()?,
        decision_note,
        cost_usd: cost,
        tokens_input: tin.map(|v| v as u64),
        tokens_output: tout.map(|v| v as u64),
        confidence: conf,
    })
}

const COLS: &str = "id, created_at, based_on_version_hash, patch_json, rationale, evidence_json, status, decided_by, decided_at, decision_note, cost_usd, tokens_input, tokens_output, confidence";

impl Store {
    pub fn insert_amas_suggestion(&self, s: &InsertSuggestion) -> Result<i64, StoreError> {
        let conn = self.conn()?;
        let now = Utc::now().to_rfc3339();
        let decided_at = if matches!(s.initial_status, SuggestionStatus::Pending) {
            None
        } else {
            Some(now.clone())
        };
        conn.execute(
            "INSERT INTO amas_tuning_suggestions
             (created_at, based_on_version_hash, patch_json, rationale, evidence_json,
              status, decided_by, decided_at, decision_note, cost_usd, tokens_input, tokens_output, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                now,
                &s.based_on_version_hash,
                &s.patch_json,
                &s.rationale,
                &s.evidence_json,
                s.initial_status.as_str(),
                s.decided_by.as_deref(),
                decided_at.as_deref(),
                s.decision_note.as_deref(),
                s.cost_usd,
                s.tokens_input.map(|v| v as i64),
                s.tokens_output.map(|v| v as i64),
                s.confidence,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_amas_suggestions(
        &self,
        status: Option<SuggestionStatus>,
        limit: usize,
    ) -> Result<Vec<TuningSuggestionRow>, StoreError> {
        let limit = limit.min(500) as i64;
        let conn = self.conn()?;
        let raw: Vec<_> = match status {
            Some(s) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COLS} FROM amas_tuning_suggestions WHERE status = ?1
                     ORDER BY created_at DESC LIMIT ?2"
                ))?;
                let rows: Result<Vec<_>, _> = stmt
                    .query_map(params![s.as_str(), limit], row_to_suggestion)?
                    .collect();
                rows?
            }
            None => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COLS} FROM amas_tuning_suggestions
                     ORDER BY created_at DESC LIMIT ?1"
                ))?;
                let rows: Result<Vec<_>, _> =
                    stmt.query_map(params![limit], row_to_suggestion)?.collect();
                rows?
            }
        };
        raw.into_iter().map(build).collect()
    }

    pub fn get_amas_suggestion(&self, id: i64) -> Result<Option<TuningSuggestionRow>, StoreError> {
        let conn = self.conn()?;
        let raw = conn
            .query_row(
                &format!("SELECT {COLS} FROM amas_tuning_suggestions WHERE id = ?1"),
                params![id],
                row_to_suggestion,
            )
            .optional()?;
        match raw {
            Some(r) => Ok(Some(build(r)?)),
            None => Ok(None),
        }
    }

    pub fn update_amas_suggestion_status(
        &self,
        id: i64,
        new_status: SuggestionStatus,
        decided_by: Option<&str>,
        decision_note: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let now = Utc::now().to_rfc3339();
        let affected = conn.execute(
            "UPDATE amas_tuning_suggestions
             SET status = ?1, decided_by = ?2, decided_at = ?3, decision_note = ?4
             WHERE id = ?5",
            params![new_status.as_str(), decided_by, now, decision_note, id],
        )?;
        if affected == 0 {
            return Err(StoreError::NotFound {
                entity: "amas_tuning_suggestions".into(),
                key: id.to_string(),
            });
        }
        Ok(())
    }

    /// 已用日成本 / 已用 token —— 用于触发日上限
    pub fn aggregate_amas_suggestion_spend_today(&self) -> Result<(f64, u64, u64), StoreError> {
        let cutoff = (Utc::now() - Duration::days(1)).to_rfc3339();
        let conn = self.conn()?;
        let (cost, tin, tout) = conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0), COALESCE(SUM(tokens_input), 0), COALESCE(SUM(tokens_output), 0)
             FROM amas_tuning_suggestions WHERE created_at >= ?1",
            params![cutoff],
            |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, i64>(2)? as u64,
                ))
            },
        )?;
        Ok((cost, tin, tout))
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

    fn ins(initial_status: SuggestionStatus) -> InsertSuggestion {
        InsertSuggestion {
            based_on_version_hash: "abc123".into(),
            patch_json: r#"{"memoryModel.baseDesiredRetention":0.85}"#.into(),
            rationale: "test".into(),
            evidence_json: "{}".into(),
            cost_usd: Some(0.01),
            tokens_input: Some(100),
            tokens_output: Some(50),
            confidence: Some(0.7),
            initial_status,
            decided_by: None,
            decision_note: None,
        }
    }

    #[test]
    fn insert_and_get_pending() {
        let store = fresh_store();
        let id = store
            .insert_amas_suggestion(&ins(SuggestionStatus::Pending))
            .unwrap();
        let row = store.get_amas_suggestion(id).unwrap().expect("must exist");
        assert_eq!(row.status, SuggestionStatus::Pending);
        assert_eq!(row.cost_usd, Some(0.01));
        assert!(row.decided_at.is_none());
    }

    #[test]
    fn list_filters_by_status() {
        let store = fresh_store();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::Pending))
            .unwrap();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::AutoApplied))
            .unwrap();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::Pending))
            .unwrap();

        let pending = store
            .list_amas_suggestions(Some(SuggestionStatus::Pending), 10)
            .unwrap();
        assert_eq!(pending.len(), 2);
        let auto = store
            .list_amas_suggestions(Some(SuggestionStatus::AutoApplied), 10)
            .unwrap();
        assert_eq!(auto.len(), 1);
        let all = store.list_amas_suggestions(None, 10).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn update_status_transitions() {
        let store = fresh_store();
        let id = store
            .insert_amas_suggestion(&ins(SuggestionStatus::Pending))
            .unwrap();
        store
            .update_amas_suggestion_status(
                id,
                SuggestionStatus::Approved,
                Some("admin"),
                Some("ok"),
            )
            .unwrap();
        let row = store.get_amas_suggestion(id).unwrap().unwrap();
        assert_eq!(row.status, SuggestionStatus::Approved);
        assert_eq!(row.decided_by.as_deref(), Some("admin"));
        assert_eq!(row.decision_note.as_deref(), Some("ok"));
        assert!(row.decided_at.is_some());
    }

    #[test]
    fn spend_today_aggregates() {
        let store = fresh_store();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::Pending))
            .unwrap();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::AutoApplied))
            .unwrap();
        let (cost, tin, tout) = store.aggregate_amas_suggestion_spend_today().unwrap();
        assert!((cost - 0.02).abs() < 1e-9);
        assert_eq!(tin, 200);
        assert_eq!(tout, 100);
    }
}
