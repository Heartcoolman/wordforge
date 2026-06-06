//! AMAS per-patch 真灰度(canary)的 Store 层(C6)。
//!
//! 区别于 amas_canary_config(单 active 配置版本灰度):本表支持多条 active patch 并行灰度,
//! 每条占据 cohort 区间 [cohort_lo, cohort_hi) ⊂ 0..100,active 行之间互不重叠(落库前校验)。
//! engine.effective_config_for_user 遍历 active 行,按 hash(user_id)%100 命中其一。

use rusqlite::{params, Connection, OptionalExtension};
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
    /// 「读 active+重叠校验+INSERT」收进单事务同一连接,消除并发 TOCTOU(WAL 下写事务串行提交,
    /// 后者必然看到前者已落库的 active 行)。
    pub fn insert_patch_canary(
        &self,
        suggestion_id: i64,
        version_hash: &str,
        percent: u32,
        cohort_lo: u32,
        cohort_hi: u32,
        baseline_metrics_json: &str,
    ) -> Result<i64, StoreError> {
        self.with_transaction(|conn| {
            validate_cohort_in_conn(conn, cohort_lo, cohort_hi, None)?;
            insert_patch_canary_in_conn(
                conn,
                suggestion_id,
                version_hash,
                percent,
                cohort_lo,
                cohort_hi,
                baseline_metrics_json,
            )
        })
    }

    /// 原子新建:在同一事务内读 active 行算出下一空闲 cohort 槽([0,a)、[a,a+b)…)、校验、再 INSERT。
    /// 替代路由层「算 cohort_lo」与「落库」拆两次 run_store_task 的窗口。落地后返回完整行。
    pub fn create_active_patch_canary(
        &self,
        suggestion_id: i64,
        version_hash: &str,
        percent: u32,
        baseline_metrics_json: &str,
    ) -> Result<PatchCanary, StoreError> {
        self.with_transaction(|conn| {
            let cohort_lo = active_canaries_in_conn(conn, None)?
                .iter()
                .map(|c| c.cohort_hi)
                .max()
                .unwrap_or(0);
            let cohort_hi = cohort_lo + percent;
            if cohort_hi > 100 {
                return Err(StoreError::Validation(format!(
                    "灰度配额已满：已占用 {cohort_lo}%，再加 {percent}% 超过 100%"
                )));
            }
            let id = insert_patch_canary_in_conn(
                conn,
                suggestion_id,
                version_hash,
                percent,
                cohort_lo,
                cohort_hi,
                baseline_metrics_json,
            )?;
            get_patch_canary_in_conn(conn, id)?.ok_or_else(|| StoreError::NotFound {
                entity: "amas_patch_canary".into(),
                key: id.to_string(),
            })
        })
    }

    /// 原子建灰度 + 迁出 suggestion(#9)。单事务内：
    /// 1) 拒绝同一 suggestion_id 已有 active canary（防一条建议反复占额 / 重复灰度，等价 active 行唯一约束）；
    /// 2) 落 canary version snapshot（hash 幂等，补写 note/parent）；
    /// 3) 分配空闲 cohort 槽 + 配额校验 + INSERT active canary 行；
    /// 4) CAS 把 suggestion 从 Pending 迁到 InCanary（affected==0→Conflict，已被并发 approve/灰度）。
    /// 使「建灰度」与「建议状态迁移」要么整体成功要么整体不发生，杜绝灰度 + 全量双重生效。
    #[allow(clippy::too_many_arguments)]
    pub fn create_canary_and_claim_suggestion(
        &self,
        suggestion_id: i64,
        snapshot_json: &str,
        author_admin_id: &str,
        note: Option<&str>,
        parent_version_hash: Option<&str>,
        percent: u32,
        baseline_metrics_json: &str,
    ) -> Result<PatchCanary, StoreError> {
        use crate::store::operations::amas_suggestions::SuggestionStatus;
        use crate::store::operations::amas_versions::{
            insert_amas_config_version_in_conn, ConfigVersionSource,
        };
        self.with_transaction(|conn| {
            // 1) 同一 suggestion 已有 active 灰度 → 拒绝（防重复占额）。
            let dup: i64 = conn.query_row(
                "SELECT COUNT(*) FROM amas_patch_canary WHERE suggestion_id = ?1 AND status = 'active'",
                params![suggestion_id],
                |r| r.get(0),
            )?;
            if dup > 0 {
                return Err(StoreError::Validation(format!(
                    "建议 #{suggestion_id} 已有进行中的灰度，请勿重复创建"
                )));
            }
            // 2) 落 version snapshot。
            let (_vid, vhash) = insert_amas_config_version_in_conn(
                conn,
                snapshot_json,
                author_admin_id,
                ConfigVersionSource::LlmSuggested,
                note,
                parent_version_hash,
            )?;
            // 3) 分配 cohort 槽 + 配额校验 + INSERT。
            let cohort_lo = active_canaries_in_conn(conn, None)?
                .iter()
                .map(|c| c.cohort_hi)
                .max()
                .unwrap_or(0);
            let cohort_hi = cohort_lo + percent;
            if cohort_hi > 100 {
                return Err(StoreError::Validation(format!(
                    "灰度配额已满：已占用 {cohort_lo}%，再加 {percent}% 超过 100%"
                )));
            }
            let id = insert_patch_canary_in_conn(
                conn,
                suggestion_id,
                &vhash,
                percent,
                cohort_lo,
                cohort_hi,
                baseline_metrics_json,
            )?;
            // 4) CAS 迁出 Pending（同事务，建议不再能被 approve 二次全量应用）。
            let now = chrono::Utc::now().to_rfc3339();
            let affected = conn.execute(
                "UPDATE amas_tuning_suggestions
                 SET status = ?1, decided_by = ?2, decided_at = ?3
                 WHERE id = ?4 AND status = ?5",
                params![
                    SuggestionStatus::InCanary.as_str(),
                    author_admin_id,
                    now,
                    suggestion_id,
                    SuggestionStatus::Pending.as_str(),
                ],
            )?;
            if affected == 0 {
                return Err(StoreError::Conflict {
                    entity: "amas_tuning_suggestions".into(),
                    key: suggestion_id.to_string(),
                });
            }
            get_patch_canary_in_conn(conn, id)?.ok_or_else(|| StoreError::NotFound {
                entity: "amas_patch_canary".into(),
                key: id.to_string(),
            })
        })
    }

    /// 扩量:更新 percent + cohort 区间;校验不与其它 active 行重叠(排除自身)。
    /// 同上,「读 active+排重叠校验+UPDATE」收进单事务同一连接。
    pub fn update_patch_canary_scale(
        &self,
        id: i64,
        percent: u32,
        cohort_lo: u32,
        cohort_hi: u32,
    ) -> Result<(), StoreError> {
        self.with_transaction(|conn| {
            validate_cohort_in_conn(conn, cohort_lo, cohort_hi, Some(id))?;
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
        })
    }

    /// 原子扩量:同一事务内读当前行取 cohort_lo、按目标 percent 扩 hi、排重叠校验、再 UPDATE。
    /// 替代路由层「读 cohort_lo」与「update」拆两次 run_store_task 的窗口。
    pub fn scale_active_patch_canary(&self, id: i64, percent: u32) -> Result<PatchCanary, StoreError> {
        self.with_transaction(|conn| {
            let cur = get_patch_canary_in_conn(conn, id)?
                .ok_or_else(|| StoreError::Validation("canary 不存在".into()))?;
            let cohort_lo = cur.cohort_lo;
            let cohort_hi = cohort_lo + percent;
            validate_cohort_in_conn(conn, cohort_lo, cohort_hi, Some(id))?;
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE amas_patch_canary
                 SET percent = ?1, cohort_lo = ?2, cohort_hi = ?3, updated_at = ?4
                 WHERE id = ?5",
                params![percent as i64, cohort_lo as i64, cohort_hi as i64, now, id],
            )?;
            get_patch_canary_in_conn(conn, id)?.ok_or_else(|| StoreError::NotFound {
                entity: "amas_patch_canary".into(),
                key: id.to_string(),
            })
        })
    }

    /// 置状态(active/effective/rolled_back)，带源状态门禁(CAS)。
    /// `expected` 为 Some 时 UPDATE 仅在当前 status 匹配才生效，affected==0 → Conflict(说明期间被
    /// 并发改写，如自动回滚把 active→rolled_back)；None 退化为无条件 UPDATE(仅 affected==0→NotFound)。
    /// 杜绝 promote 把 rolled_back 覆盖回 effective 这类非法状态机迁移(#16)。
    pub fn set_patch_canary_status_cas(
        &self,
        id: i64,
        status: &str,
        expected: Option<&str>,
    ) -> Result<(), StoreError> {
        if !matches!(status, "active" | "effective" | "rolled_back") {
            return Err(StoreError::Validation(format!(
                "invalid canary status: {status}"
            )));
        }
        let conn = self.conn()?;
        set_patch_canary_status_in_conn(&conn, id, status, expected)
    }

    /// 无条件置状态(兼容旧调用)。手动回滚/手动置位等无并发竞态的场景沿用。
    pub fn set_patch_canary_status(&self, id: i64, status: &str) -> Result<(), StoreError> {
        self.set_patch_canary_status_cas(id, status, None)
    }

    /// 原子提升 canary 为 effective + 把 canary version snapshot 提升为 stable(#8/#41)。
    /// 单事务内：CAS status active→effective（affected==0→Conflict，说明已被自动/并发回滚），
    /// 落 stable 版本行(insert_amas_config_version)。状态翻转与版本落库要么整体成功要么整体不发生，
    /// 消除「stable 已变但 canary 仍 active」的悬挂态。返回落库后的 (version_id, version_hash)。
    /// 注意：reload_config(改 live 内存)由调用方在本函数成功提交后再做(内存态无法纳入 DB 事务)。
    #[allow(clippy::too_many_arguments)]
    pub fn promote_patch_canary_tx(
        &self,
        id: i64,
        snapshot_json: &str,
        author_admin_id: &str,
        source: crate::store::operations::amas_versions::ConfigVersionSource,
        note: Option<&str>,
        parent_version_hash: Option<&str>,
    ) -> Result<(i64, String), StoreError> {
        self.with_transaction(|conn| {
            // CAS：仅当仍 active 才提升；期间被回滚则 affected==0 → Conflict，放弃提升。
            set_patch_canary_status_in_conn(conn, id, "effective", Some("active"))?;
            crate::store::operations::amas_versions::insert_amas_config_version_in_conn(
                conn,
                snapshot_json,
                author_admin_id,
                source,
                note,
                parent_version_hash,
            )
        })
    }

    /// 取单条 canary;不存在返 None。
    pub fn get_patch_canary(&self, id: i64) -> Result<Option<PatchCanary>, StoreError> {
        let conn = self.conn()?;
        get_patch_canary_in_conn(&conn, id)
    }
}

/// 把某 suggestion 关联的所有 active canary 行置 rolled_back(#19:rollback_suggestion 复用)。
/// 在 `conn` 给定的事务内执行，使灰度路由回退与 stable 回滚同事务。返回受影响行数。
pub fn rollback_active_canaries_for_suggestion_in_conn(
    conn: &Connection,
    suggestion_id: i64,
) -> Result<usize, StoreError> {
    let now = chrono::Utc::now().to_rfc3339();
    let affected = conn.execute(
        "UPDATE amas_patch_canary SET status = 'rolled_back', updated_at = ?1
         WHERE suggestion_id = ?2 AND status = 'active'",
        params![now, suggestion_id],
    )?;
    Ok(affected)
}

/// 事务内读全部 active canary(可排除 exclude_id),供 cohort 校验/槽分配在同一连接内 SELECT。
fn active_canaries_in_conn(
    conn: &Connection,
    exclude_id: Option<i64>,
) -> Result<Vec<PatchCanary>, StoreError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM amas_patch_canary WHERE status = 'active'"
    ))?;
    let rows: Result<Vec<PatchCanary>, rusqlite::Error> =
        stmt.query_map([], row_to_canary)?.collect();
    Ok(rows?
        .into_iter()
        .filter(|c| Some(c.id) != exclude_id)
        .collect())
}

/// cohort 校验:[lo,hi) ⊂ 0..100 且 lo<hi,且不与现存 active 行(可排除 exclude_id)重叠。
/// 接收事务连接在同事务内 SELECT,确保校验与后续写为单一原子操作。
fn validate_cohort_in_conn(
    conn: &Connection,
    lo: u32,
    hi: u32,
    exclude_id: Option<i64>,
) -> Result<(), StoreError> {
    if lo >= hi || hi > 100 {
        return Err(StoreError::Validation(format!(
            "cohort range invalid: [{lo}, {hi}) must satisfy 0<=lo<hi<=100"
        )));
    }
    for c in active_canaries_in_conn(conn, exclude_id)? {
        if overlaps(lo, hi, c.cohort_lo, c.cohort_hi) {
            return Err(StoreError::Validation(format!(
                "cohort [{lo}, {hi}) overlaps active canary #{} [{}, {})",
                c.id, c.cohort_lo, c.cohort_hi
            )));
        }
    }
    Ok(())
}

/// 事务内 INSERT 一条 active canary,返回 rowid。调用方负责先做 cohort 校验。
fn insert_patch_canary_in_conn(
    conn: &Connection,
    suggestion_id: i64,
    version_hash: &str,
    percent: u32,
    cohort_lo: u32,
    cohort_hi: u32,
    baseline_metrics_json: &str,
) -> Result<i64, StoreError> {
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

/// 事务内置 canary 状态，带源状态门禁(CAS)。
/// `expected`=Some 时 WHERE 叠加 status=expected，affected==0 区分「行不存在」(NotFound)与
/// 「源状态不符」(Conflict);None 退化为仅按 id 的无条件 UPDATE。
fn set_patch_canary_status_in_conn(
    conn: &Connection,
    id: i64,
    status: &str,
    expected: Option<&str>,
) -> Result<(), StoreError> {
    let now = chrono::Utc::now().to_rfc3339();
    let affected = match expected {
        Some(exp) => conn.execute(
            "UPDATE amas_patch_canary SET status = ?1, updated_at = ?2
             WHERE id = ?3 AND status = ?4",
            params![status, now, id, exp],
        )?,
        None => conn.execute(
            "UPDATE amas_patch_canary SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, now, id],
        )?,
    };
    if affected == 0 {
        // 行存在但源状态不符 → Conflict；行不存在 → NotFound。
        if expected.is_some() && get_patch_canary_in_conn(conn, id)?.is_some() {
            return Err(StoreError::Conflict {
                entity: "amas_patch_canary".into(),
                key: id.to_string(),
            });
        }
        return Err(StoreError::NotFound {
            entity: "amas_patch_canary".into(),
            key: id.to_string(),
        });
    }
    Ok(())
}

/// 事务内取单条 canary;不存在返 None。
fn get_patch_canary_in_conn(conn: &Connection, id: i64) -> Result<Option<PatchCanary>, StoreError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM amas_patch_canary WHERE id = ?1"),
            params![id],
            row_to_canary,
        )
        .optional()?)
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
    fn cas_status_guards_source_state() {
        // #16：CAS 仅在源状态匹配时迁移；已 rolled_back 不能再被 active→effective 覆盖。
        let s = store();
        let a = s.insert_patch_canary(1, "h1", 100, 0, 100, "{}").unwrap();
        s.set_patch_canary_status(a, "rolled_back").unwrap();
        // 期望源状态 active，实际 rolled_back → Conflict（不覆盖回 effective）
        let err = s
            .set_patch_canary_status_cas(a, "effective", Some("active"))
            .unwrap_err();
        assert!(matches!(err, StoreError::Conflict { .. }));
        assert_eq!(s.get_patch_canary(a).unwrap().unwrap().status, "rolled_back");
        // 不存在的 id（带 expected）→ NotFound
        let err2 = s
            .set_patch_canary_status_cas(999, "effective", Some("active"))
            .unwrap_err();
        assert!(matches!(err2, StoreError::NotFound { .. }));
    }

    #[test]
    fn rollback_active_canaries_for_suggestion_scopes_by_id_and_active() {
        // #19：仅置该 suggestion 的 active 行为 rolled_back，不动其它 suggestion / 非 active 行。
        let s = store();
        let a = s.insert_patch_canary(5, "h1", 20, 0, 20, "{}").unwrap();
        let b = s.insert_patch_canary(5, "h2", 20, 20, 40, "{}").unwrap();
        let c = s.insert_patch_canary(9, "h3", 20, 40, 60, "{}").unwrap();
        s.set_patch_canary_status(b, "effective").unwrap(); // 非 active，不应被回滚
        let n = s
            .with_transaction(|conn| {
                super::rollback_active_canaries_for_suggestion_in_conn(conn, 5)
            })
            .unwrap();
        assert_eq!(n, 1); // 仅 a（suggestion 5 的 active 行）
        assert_eq!(s.get_patch_canary(a).unwrap().unwrap().status, "rolled_back");
        assert_eq!(s.get_patch_canary(b).unwrap().unwrap().status, "effective");
        assert_eq!(s.get_patch_canary(c).unwrap().unwrap().status, "active");
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
