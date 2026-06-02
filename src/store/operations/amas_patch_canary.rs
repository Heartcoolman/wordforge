//! AMAS per-patch 真灰度(canary)的 Store 层(C6)。
//!
//! 区别于 amas_canary_config(单 active 配置版本灰度):本表支持多条 active patch 并行灰度,
//! 每条占据 cohort 区间 [cohort_lo, cohort_hi) ⊂ 0..100,active 行之间互不重叠(落库前校验)。
//! engine.effective_config_for_user 遍历 active 行,按 hash(user_id)%100 命中其一。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

/// 一条 patch canary。camelCase 序列化。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchCanary {
    pub id: i64,
    pub suggestion_id: i64,
    pub version_hash: String,
    pub percent: u32,
    pub cohort_lo: u32,
    pub cohort_hi: u32,
    pub status: String,
    pub baseline_metrics_json: String,
    pub started_at: String,
    pub updated_at: String,
}

const COLS: &str = "id, suggestion_id, version_hash, percent, cohort_lo, cohort_hi, status, baseline_metrics_json, started_at, updated_at";

fn row_to_canary(r: &rusqlite::Row<'_>) -> rusqlite::Result<PatchCanary> {
    Ok(PatchCanary {
        id: r.get::<_, i64>(0)?,
        suggestion_id: r.get::<_, i64>(1)?,
        version_hash: r.get::<_, String>(2)?,
        percent: r.get::<_, i64>(3)? as u32,
        cohort_lo: r.get::<_, i64>(4)? as u32,
        cohort_hi: r.get::<_, i64>(5)? as u32,
        status: r.get::<_, String>(6)?,
        baseline_metrics_json: r.get::<_, String>(7)?,
        started_at: r.get::<_, String>(8)?,
        updated_at: r.get::<_, String>(9)?,
    })
}

/// [lo, hi) 与 [other_lo, other_hi) 是否相交(半开区间)。
fn overlaps(lo: u32, hi: u32, other_lo: u32, other_hi: u32) -> bool {
    lo < other_hi && other_lo < hi
}

impl Store {
    /// 列出 canary;status=None 返回全部(按 started_at 倒序),Some 按状态过滤。
    pub fn list_patch_canaries(
        &self,
        status: Option<&str>,
    ) -> Result<Vec<PatchCanary>, StoreError> {
        let conn = self.conn()?;
        // 单条 SQL + 可空过滤:?1 为 NULL 时返回全部,否则按 status 过滤(避免 match 分支
        // 内 stmt 借用跨越块尾的生命周期问题)。
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLS} FROM amas_patch_canary
             WHERE (?1 IS NULL OR status = ?1)
             ORDER BY started_at DESC"
        ))?;
        let rows: Result<Vec<PatchCanary>, rusqlite::Error> =
            stmt.query_map(params![status], row_to_canary)?.collect();
        Ok(rows?)
    }

    /// 当前所有 active canary(供 engine 路由 + monitor worker 用)。
    pub fn get_active_patch_canaries(&self) -> Result<Vec<PatchCanary>, StoreError> {
        self.list_patch_canaries(Some("active"))
    }

    /// 新建一条 active patch canary。cohort 区间需 ⊂ 0..100 且与现存 active 行不重叠,否则 Validation。
    pub fn insert_patch_canary(
        &self,
        suggestion_id: i64,
        version_hash: &str,
        percent: u32,
        cohort_lo: u32,
        cohort_hi: u32,
        baseline_metrics_json: &str,
    ) -> Result<i64, StoreError> {
        self.validate_cohort(cohort_lo, cohort_hi, None)?;
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO amas_patch_canary
                (suggestion_id, version_hash, percent, cohort_lo, cohort_hi, status,
                 baseline_metrics_json, started_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?7, ?7)",
            params![
                suggestion_id,
                version_hash,
                percent as i64,
                cohort_lo as i64,
                cohort_hi as i64,
                baseline_metrics_json,
                now,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 扩量:更新 percent + cohort 区间;校验不与其它 active 行重叠(排除自身)。
    pub fn update_patch_canary_scale(
        &self,
        id: i64,
        percent: u32,
        cohort_lo: u32,
        cohort_hi: u32,
    ) -> Result<(), StoreError> {
        self.validate_cohort(cohort_lo, cohort_hi, Some(id))?;
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        let affected = conn.execute(
            "UPDATE amas_patch_canary
             SET percent = ?1, cohort_lo = ?2, cohort_hi = ?3, updated_at = ?4
             WHERE id = ?5",
            params![percent as i64, cohort_lo as i64, cohort_hi as i64, now, id],
        )?;
        if affected == 0 {
            return Err(StoreError::NotFound {
                entity: "amas_patch_canary".into(),
                key: id.to_string(),
            });
        }
        Ok(())
    }

    /// 置状态(active/effective/rolled_back)。
    pub fn set_patch_canary_status(&self, id: i64, status: &str) -> Result<(), StoreError> {
        if !matches!(status, "active" | "effective" | "rolled_back") {
            return Err(StoreError::Validation(format!(
                "invalid canary status: {status}"
            )));
        }
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        let affected = conn.execute(
            "UPDATE amas_patch_canary SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, now, id],
        )?;
        if affected == 0 {
            return Err(StoreError::NotFound {
                entity: "amas_patch_canary".into(),
                key: id.to_string(),
            });
        }
        Ok(())
    }

    /// cohort 校验:[lo,hi) ⊂ 0..100 且 lo<hi,且不与现存 active 行(可排除 exclude_id)重叠。
    fn validate_cohort(&self, lo: u32, hi: u32, exclude_id: Option<i64>) -> Result<(), StoreError> {
        if lo >= hi || hi > 100 {
            return Err(StoreError::Validation(format!(
                "cohort range invalid: [{lo}, {hi}) must satisfy 0<=lo<hi<=100"
            )));
        }
        for c in self.get_active_patch_canaries()? {
            if Some(c.id) == exclude_id {
                continue;
            }
            if overlaps(lo, hi, c.cohort_lo, c.cohort_hi) {
                return Err(StoreError::Validation(format!(
                    "cohort [{lo}, {hi}) overlaps active canary #{} [{}, {})",
                    c.id, c.cohort_lo, c.cohort_hi
                )));
            }
        }
        Ok(())
    }

    /// 取单条 canary;不存在返 None。
    pub fn get_patch_canary(&self, id: i64) -> Result<Option<PatchCanary>, StoreError> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {COLS} FROM amas_patch_canary WHERE id = ?1"),
                params![id],
                row_to_canary,
            )
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let s = Store::open(":memory:", 5000, 1).unwrap();
        s.run_migrations().unwrap();
        s
    }

    #[test]
    fn insert_then_get_roundtrip() {
        let s = store();
        let id = s
            .insert_patch_canary(7, "hash-a", 20, 0, 20, r#"{"reward":0.5}"#)
            .unwrap();
        let c = s.get_patch_canary(id).unwrap().unwrap();
        assert_eq!(c.suggestion_id, 7);
        assert_eq!(c.version_hash, "hash-a");
        assert_eq!(c.percent, 20);
        assert_eq!((c.cohort_lo, c.cohort_hi), (0, 20));
        assert_eq!(c.status, "active");
    }

    #[test]
    fn active_list_only_active() {
        let s = store();
        let a = s.insert_patch_canary(1, "h1", 20, 0, 20, "{}").unwrap();
        let _b = s.insert_patch_canary(2, "h2", 20, 20, 40, "{}").unwrap();
        s.set_patch_canary_status(a, "rolled_back").unwrap();
        let active = s.get_active_patch_canaries().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].version_hash, "h2");
    }

    #[test]
    fn overlapping_cohort_rejected() {
        let s = store();
        s.insert_patch_canary(1, "h1", 20, 0, 20, "{}").unwrap();
        let err = s
            .insert_patch_canary(2, "h2", 20, 10, 30, "{}")
            .unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));
    }

    #[test]
    fn adjacent_cohort_allowed() {
        let s = store();
        s.insert_patch_canary(1, "h1", 20, 0, 20, "{}").unwrap();
        // [20,40) 与 [0,20) 半开相邻不重叠
        let ok = s.insert_patch_canary(2, "h2", 20, 20, 40, "{}");
        assert!(ok.is_ok());
    }

    #[test]
    fn rolled_back_cohort_freed_for_reuse() {
        let s = store();
        let a = s.insert_patch_canary(1, "h1", 20, 0, 20, "{}").unwrap();
        s.set_patch_canary_status(a, "rolled_back").unwrap();
        // a 已 rolled_back,其 cohort 释放,新行可复用
        let ok = s.insert_patch_canary(2, "h2", 20, 0, 20, "{}");
        assert!(ok.is_ok());
    }

    #[test]
    fn out_of_bounds_cohort_rejected() {
        let s = store();
        assert!(matches!(
            s.insert_patch_canary(1, "h1", 20, 90, 110, "{}")
                .unwrap_err(),
            StoreError::Validation(_)
        ));
        assert!(matches!(
            s.insert_patch_canary(1, "h1", 20, 30, 30, "{}")
                .unwrap_err(),
            StoreError::Validation(_)
        ));
    }

    #[test]
    fn scale_excludes_self_from_overlap() {
        let s = store();
        let a = s.insert_patch_canary(1, "h1", 20, 0, 20, "{}").unwrap();
        // 扩量到 [0,60) 不应与"自身旧区间"冲突
        s.update_patch_canary_scale(a, 60, 0, 60).unwrap();
        let c = s.get_patch_canary(a).unwrap().unwrap();
        assert_eq!(c.percent, 60);
        assert_eq!((c.cohort_lo, c.cohort_hi), (0, 60));
    }

    #[test]
    fn scale_still_rejects_other_overlap() {
        let s = store();
        let a = s.insert_patch_canary(1, "h1", 20, 0, 20, "{}").unwrap();
        s.insert_patch_canary(2, "h2", 20, 60, 80, "{}").unwrap();
        // a 扩到 [0,70) 会撞 h2 的 [60,80)
        let err = s.update_patch_canary_scale(a, 70, 0, 70).unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));
    }

    #[test]
    fn set_status_invalid_rejected() {
        let s = store();
        let a = s.insert_patch_canary(1, "h1", 20, 0, 20, "{}").unwrap();
        assert!(matches!(
            s.set_patch_canary_status(a, "bogus").unwrap_err(),
            StoreError::Validation(_)
        ));
    }

    #[test]
    fn set_status_missing_id_not_found() {
        let s = store();
        assert!(matches!(
            s.set_patch_canary_status(999, "effective").unwrap_err(),
            StoreError::NotFound { .. }
        ));
    }

    #[test]
    fn promote_to_effective() {
        let s = store();
        let a = s.insert_patch_canary(1, "h1", 100, 0, 100, "{}").unwrap();
        s.set_patch_canary_status(a, "effective").unwrap();
        assert_eq!(s.get_patch_canary(a).unwrap().unwrap().status, "effective");
        assert!(s.get_active_patch_canaries().unwrap().is_empty());
    }

    #[test]
    fn patch_canary_serializes_camel_case() {
        let c = PatchCanary {
            id: 1,
            suggestion_id: 2,
            version_hash: "h".into(),
            percent: 20,
            cohort_lo: 0,
            cohort_hi: 20,
            status: "active".into(),
            baseline_metrics_json: "{}".into(),
            started_at: "t".into(),
            updated_at: "t".into(),
        };
        let v = serde_json::to_value(&c).unwrap();
        for k in [
            "suggestionId",
            "versionHash",
            "cohortLo",
            "cohortHi",
            "baselineMetricsJson",
            "startedAt",
            "updatedAt",
        ] {
            assert!(v.get(k).is_some(), "missing key {k}");
        }
    }
}
