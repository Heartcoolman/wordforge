# amas-advisor 页全栈对齐设计图 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `/admin/amas-advisor` 页完整对齐设计图 `admin后端/amas-advisor.html`（整页 + 全部后端，一次做完）。

**Architecture:** SolidJS 前端重写为 12 栅格双栏 + 9 区块组件；后端在既有 6 个 advisor 端点基础上新增 6 组端点 + `canary_monitor` worker + 3 项数据模型变更（`amas_tuning_whitelist` / `amas_patch_canary` 表 + `system_settings.llm_advisor_enabled` 列）。成本统一 ¥/月度；灰度为真·per-patch canary（多并行 + reward/anomaly 自动回滚）。

**Tech Stack:** Rust（axum / rusqlite / tokio-cron-scheduler）后端；SolidJS + Vite + vitest + playwright 前端；TDD + 频繁提交。

**Spec:** `docs/superpowers/specs/2026-05-29-amas-advisor-redesign-design.md`

**执行顺序：** 模块 **A**（数据模型/Store）→ **B / C / D**（后端，依赖 A 的 store 方法）→ **F**（前端组件 + `admin.ts` API client 底座）→ **E**（前端整页装配，Task E5 依赖 F 的组件，须在 F 之后）。B/C/D 三模块互相独立可并行。

> **⚠️ 跨模块重叠协调（并行起草产生，执行前必读，避免重复建文件）：**
> 1. **`amas_tuning_whitelist` 表 + store CRUD（`amas_tuning_whitelist.rs`）**：以**模块 A（Task A1/A2）为唯一权威**。模块 C 的 Task C1/C2（同名迁移 + seed）改为「校验 A 已建、若缺则补」的幂等校验，不重复创建 store 文件；C3 起的白名单端点直接复用 A 的 store 方法。
> 2. **`amas_patch_canary` 表 + store CRUD（`amas_patch_canary.rs`）+ `PatchCanary` 结构**：以**模块 A（Task A1/A3）为唯一权威**。模块 D 的 Task D1/D2（同名迁移 + store CRUD）改为「复用 A 的表与 store 方法」；D 从 **Task D3（引擎 `effective_config_for_user` 多路由改造）** 起为真正新增内容（引擎 / monitor worker / 端点）。若 D 需要 A 未提供的 store 方法，在 A 的 `amas_patch_canary.rs` 内补充而非另建文件。
> 3. 所有迁移均 `CREATE TABLE IF NOT EXISTS` / `ADD COLUMN` 守卫、幂等安全；上述协调是为避免两个模块各建一份同名 store 文件导致合并冲突。

---

## 文件结构总览

**后端（Rust）**
- 新建：`src/store/operations/amas_tuning_whitelist.rs`（白名单表 CRUD + seed）、`src/store/operations/amas_patch_canary.rs`（per-patch canary 表 CRUD + cohort 区间校验）、`src/workers/canary_monitor.rs`（自动回滚 worker）
- 修改：`src/store/migrate.rs`（建表/列迁移）、`src/store/operations/mod.rs`（挂载新 operations）、`src/store/operations/system_settings.rs`（`llm_advisor_enabled` 读写）、`src/store/operations/amas_suggestions.rs`（list 加 offset/q + 聚合）、`src/amas/tuning_whitelist.rs`（`validate_patch`/`find` 改 store 读、const fallback）、`src/amas/engine.rs`（`effective_config_for_user` 多 canary 路由）、`src/routes/admin/amas.rs`（新增全部 advisor 端点）、`src/workers/mod.rs`（注册 `canary_monitor`）、`src/workers/llm_advisor.rs`（`build_system_prompt` 改 store 读白名单）
- 测试：`tests/admin_amas_http.rs`（端点集成）+ 各 `operations`/`engine`/`worker` 内联 `#[cfg(test)]` 单测

**前端（SolidJS）**
- 新建：`admin-ui/src/pages/amas-advisor/{PageHeaderOps,CostRow,PatchTabs,CostChart,SuggestionCard,PatchCanaryCard,AdvisorConfigPanel,WhitelistPanel,HistoryTable}.tsx`
- 修改：`admin-ui/src/api/admin.ts`（新增 advisor/canary/whitelist 方法 + TS 类型）、`admin-ui/src/pages/AmasAdvisorPage.tsx`（整页 12 栅格双栏重写）
- 测试：`admin-ui/tests/pages/amas-advisor/*.test.tsx`、重写 `admin-ui/tests/pages/AmasAdvisorPage.test.tsx`

> 任务编号按模块前缀（A1.. / B1.. / C1.. / D1.. / E1.. / F1..），同一模块内按依赖顺序执行。每个任务遵循 TDD：写失败测试 → 跑确认失败 → 最小实现 → 跑确认通过 → 提交。

---


## 模块 A — 数据模型与 Store 层（迁移 + 白名单/canary/聚合 store 方法）

### Task A1: 数据模型迁移 — 新增 amas_tuning_whitelist / amas_patch_canary 两表 + system_settings.llm_advisor_enabled 列

**Files:**
- Modify `src/store/migrate.rs:31-60`（migrations() 注册表追加一项）
- Modify `src/store/migrate.rs:65-100`（migrations_down() 注册表追加一项）
- Modify `src/store/migrate.rs:1330`（在 m024_client_extras 与其 down 之后追加 m025 up/down 函数）

- [ ] 在 migrate.rs 末尾（`m024_client_extras_down` 之后）追加 m025 迁移函数的**失败占位测试**先行：在 migrate.rs 的 `#[cfg(test)] mod tests` 里加一条断言新表/新列存在的测试。先定位测试 mod（`grep -n "mod tests" src/store/migrate.rs`），在其内追加：
```rust
    #[test]
    fn m025_creates_advisor_tables_and_column() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        let conn = store.conn().unwrap();
        // 两张新表存在
        for tbl in ["amas_tuning_whitelist", "amas_patch_canary"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    rusqlite::params![tbl],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {tbl} missing");
        }
        // system_settings.llm_advisor_enabled 列存在
        let has_col = conn
            .prepare("PRAGMA table_info(system_settings)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(Result::ok)
            .any(|c| c == "llm_advisor_enabled");
        assert!(has_col, "llm_advisor_enabled column missing");
    }
```
- [ ] 跑确认失败：`cargo test -p learning-backend --lib store::migrate::tests::m025_creates_advisor_tables_and_column`。预期失败——编译报 `m025` 未注册（仍是 024 顶点，新表不存在），或断言 `table amas_tuning_whitelist missing` panic。
- [ ] 在 `migrations()` 注册表 `("024_client_extras", m024_client_extras),`（migrate.rs:59）后追加一行：
```rust
        ("025_amas_advisor", m025_amas_advisor),
```
- [ ] 在 `migrations_down()` 注册表对应的 `("024_client_extras", m024_client_extras_down),`（约 migrate.rs:99）后追加一行：
```rust
        ("025_amas_advisor", m025_amas_advisor_down),
```
- [ ] 在 migrate.rs 的 `m024_client_extras_down` 函数结束（约 migrate.rs:1330 之后）追加 up 实现。表结构样板对齐 schema.rs 的 amas_canary_config，cohort/percent 用 CHECK 守卫，列守卫仿 m024：
```rust
/// m025:amas-advisor 全栈对齐所需数据模型 ——
///   1) amas_tuning_whitelist 新表(LLM 调参白名单,启动 seed 自 TIER_A_WHITELIST)
///   2) amas_patch_canary 新表(per-patch 真灰度,多条 active,cohort [lo,hi) 不重叠)
///   3) system_settings.llm_advisor_enabled 列(运行时巡查开关)
fn m025_amas_advisor(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS amas_tuning_whitelist (
            path        TEXT NOT NULL,
            min_safe    REAL NOT NULL,
            max_safe    REAL NOT NULL,
            created_at  TEXT NOT NULL,
            created_by  TEXT NOT NULL,
            PRIMARY KEY (path)
        );

        CREATE TABLE IF NOT EXISTS amas_patch_canary (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,
            suggestion_id         INTEGER NOT NULL,
            version_hash          TEXT NOT NULL,
            percent               INTEGER NOT NULL CHECK (percent BETWEEN 0 AND 100),
            cohort_lo             INTEGER NOT NULL CHECK (cohort_lo BETWEEN 0 AND 100),
            cohort_hi             INTEGER NOT NULL CHECK (cohort_hi BETWEEN 0 AND 100),
            status                TEXT NOT NULL DEFAULT 'active'
                                  CHECK (status IN ('active','effective','rolled_back')),
            baseline_metrics_json TEXT NOT NULL,
            started_at            TEXT NOT NULL,
            updated_at            TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_amas_patch_canary_active
            ON amas_patch_canary(status) WHERE status = 'active';
        CREATE INDEX IF NOT EXISTS idx_amas_patch_canary_started
            ON amas_patch_canary(started_at DESC);",
    )?;

    // system_settings.llm_advisor_enabled —— 列守卫(幂等)
    let has_col: bool = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|c| c == "llm_advisor_enabled");
    if !has_col {
        conn.execute(
            "ALTER TABLE system_settings ADD COLUMN llm_advisor_enabled
                INTEGER NOT NULL DEFAULT 0 CHECK (llm_advisor_enabled IN (0, 1))",
            [],
        )?;
    }

    Ok(())
}

/// m025 down:DROP 两表;llm_advisor_enabled 列借 SQLite ALTER DROP 单删。生产严禁 down。
fn m025_amas_advisor_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP TABLE IF EXISTS amas_patch_canary;
         DROP TABLE IF EXISTS amas_tuning_whitelist;",
    )?;
    let has_col: bool = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|c| c == "llm_advisor_enabled");
    if has_col {
        conn.execute(
            "ALTER TABLE system_settings DROP COLUMN llm_advisor_enabled",
            [],
        )?;
    }
    Ok(())
}
```
- [ ] 跑确认通过：`cargo test -p learning-backend --lib store::migrate::tests::m025_creates_advisor_tables_and_column`。预期 `test result: ok. 1 passed`。
- [ ] 同时把新列加入 schema.rs 的初始 `system_settings` CREATE（保证全新库直建即有列，与迁移路径一致）。Edit `src/store/schema.rs:490`，在 `llm_advisor_max_cost_per_month_yuan REAL NOT NULL DEFAULT 100.0,` 行后追加：
```sql
    llm_advisor_enabled INTEGER NOT NULL DEFAULT 0 CHECK (llm_advisor_enabled IN (0, 1)),
```
- [ ] 跑全量迁移回归确认无破坏：`cargo test -p learning-backend --lib store::migrate`。预期全 pass。
- [ ] commit：`git add src/store/migrate.rs src/store/schema.rs && git commit -m "feat(store): m025 迁移新增 amas_tuning_whitelist/amas_patch_canary 两表 + llm_advisor_enabled 列"`

### Task A2: amas_tuning_whitelist 的 Store CRUD + seed_if_empty + WhitelistRow

**Files:**
- Create `src/store/operations/amas_tuning_whitelist.rs`
- Modify `src/store/operations/mod.rs:3-4`（在 amas_canary 与 amas_suggestions 之间按字母序插入 `pub mod amas_tuning_whitelist;`）

- [ ] Create `src/store/operations/amas_tuning_whitelist.rs`，**先写测试 + 仅声明类型/方法签名使其编译失败**。文件内容（含 WhitelistRow 定义、四个方法的 `unimplemented!()` 占位 + 测试）：
```rust
//! AMAS LLM 调参白名单的 Store 层(C4)。
//!
//! `amas_tuning_whitelist` 表替代 const `TIER_A_WHITELIST`:启动时若空则 seed 自 const,
//! 之后 admin 可经 /advisor/whitelist 增删。validate_patch / llm_advisor build_system_prompt
//! 改为从本表读(const 仅作 seed 源 + fallback)。

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::amas::tuning_whitelist::TIER_A_WHITELIST;
use crate::store::{Store, StoreError};

/// 一条白名单条目。camelCase 序列化:path / minSafe / maxSafe。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhitelistRow {
    pub path: String,
    pub min_safe: f64,
    pub max_safe: f64,
}

impl Store {
    /// 列出全部白名单条目,按 path 升序。
    pub fn list_tuning_whitelist(&self) -> Result<Vec<WhitelistRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT path, min_safe, max_safe FROM amas_tuning_whitelist ORDER BY path ASC",
        )?;
        let rows: Result<Vec<_>, _> = stmt
            .query_map([], |r| {
                Ok(WhitelistRow {
                    path: r.get::<_, String>(0)?,
                    min_safe: r.get::<_, f64>(1)?,
                    max_safe: r.get::<_, f64>(2)?,
                })
            })?
            .collect();
        Ok(rows?)
    }

    /// 新增/覆盖一条白名单条目(upsert by path)。min_safe < max_safe,否则 Validation。
    pub fn insert_tuning_whitelist(
        &self,
        path: &str,
        min_safe: f64,
        max_safe: f64,
        created_by: &str,
    ) -> Result<WhitelistRow, StoreError> {
        if !(min_safe < max_safe) {
            return Err(StoreError::Validation(format!(
                "min_safe ({min_safe}) must be < max_safe ({max_safe})"
            )));
        }
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO amas_tuning_whitelist (path, min_safe, max_safe, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET min_safe=?2, max_safe=?3",
            params![path, min_safe, max_safe, now, created_by],
        )?;
        Ok(WhitelistRow {
            path: path.to_string(),
            min_safe,
            max_safe,
        })
    }

    /// 删除一条白名单条目;返回是否真的删掉一行。
    pub fn delete_tuning_whitelist(&self, path: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let affected =
            conn.execute("DELETE FROM amas_tuning_whitelist WHERE path = ?1", params![path])?;
        Ok(affected > 0)
    }

    /// 若表为空,用 TIER_A_WHITELIST seed(created_by='system')。返回 seed 进的条数(已有则 0)。
    pub fn seed_tuning_whitelist_if_empty(&self) -> Result<usize, StoreError> {
        let mut conn = self.conn()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM amas_tuning_whitelist", [], |r| r.get(0))?;
        if count > 0 {
            return Ok(0);
        }
        let now = chrono::Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        for e in TIER_A_WHITELIST {
            tx.execute(
                "INSERT OR IGNORE INTO amas_tuning_whitelist
                    (path, min_safe, max_safe, created_at, created_by)
                 VALUES (?1, ?2, ?3, ?4, 'system')",
                params![e.path, e.min_safe, e.max_safe, now],
            )?;
        }
        tx.commit()?;
        Ok(TIER_A_WHITELIST.len())
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
    fn empty_then_seed_loads_eleven() {
        let s = store();
        assert!(s.list_tuning_whitelist().unwrap().is_empty());
        let n = s.seed_tuning_whitelist_if_empty().unwrap();
        assert_eq!(n, 11);
        let rows = s.list_tuning_whitelist().unwrap();
        assert_eq!(rows.len(), 11);
        // seed 内容对齐 const(任取一条核对)
        let ret = rows
            .iter()
            .find(|r| r.path == "memoryModel.baseDesiredRetention")
            .expect("must contain baseDesiredRetention");
        assert!((ret.min_safe - 0.75).abs() < 1e-9);
        assert!((ret.max_safe - 0.95).abs() < 1e-9);
    }

    #[test]
    fn seed_is_idempotent() {
        let s = store();
        assert_eq!(s.seed_tuning_whitelist_if_empty().unwrap(), 11);
        // 二次 seed 不重复插入
        assert_eq!(s.seed_tuning_whitelist_if_empty().unwrap(), 0);
        assert_eq!(s.list_tuning_whitelist().unwrap().len(), 11);
    }

    #[test]
    fn insert_then_list_includes_new_path() {
        let s = store();
        let row = s
            .insert_tuning_whitelist("memoryModel.w[5]", 0.1, 2.0, "admin-1")
            .unwrap();
        assert_eq!(row.path, "memoryModel.w[5]");
        let list = s.list_tuning_whitelist().unwrap();
        assert!(list.iter().any(|r| r.path == "memoryModel.w[5]"));
    }

    #[test]
    fn insert_upserts_existing_path() {
        let s = store();
        s.insert_tuning_whitelist("memoryModel.w[5]", 0.1, 2.0, "admin-1")
            .unwrap();
        s.insert_tuning_whitelist("memoryModel.w[5]", 0.2, 3.0, "admin-2")
            .unwrap();
        let list = s.list_tuning_whitelist().unwrap();
        let hit = list.iter().find(|r| r.path == "memoryModel.w[5]").unwrap();
        assert!((hit.min_safe - 0.2).abs() < 1e-9);
        assert!((hit.max_safe - 3.0).abs() < 1e-9);
        // 仍只有一行
        assert_eq!(list.iter().filter(|r| r.path == "memoryModel.w[5]").count(), 1);
    }

    #[test]
    fn insert_rejects_inverted_range() {
        let s = store();
        let err = s
            .insert_tuning_whitelist("memoryModel.w[5]", 2.0, 1.0, "admin")
            .unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));
    }

    #[test]
    fn delete_returns_true_then_false() {
        let s = store();
        s.insert_tuning_whitelist("memoryModel.w[5]", 0.1, 2.0, "admin")
            .unwrap();
        assert!(s.delete_tuning_whitelist("memoryModel.w[5]").unwrap());
        assert!(!s.delete_tuning_whitelist("memoryModel.w[5]").unwrap());
    }

    #[test]
    fn whitelist_row_serializes_camel_case() {
        let row = WhitelistRow {
            path: "memoryModel.w[0]".into(),
            min_safe: 0.05,
            max_safe: 3.0,
        };
        let v = serde_json::to_value(&row).unwrap();
        assert!(v.get("minSafe").is_some());
        assert!(v.get("maxSafe").is_some());
        assert!(v.get("path").is_some());
    }
}
```
- [ ] 在 `src/store/operations/mod.rs` 的 `pub mod amas_canary;`（mod.rs:3）后插入一行（保持字母序，紧接其后即可）：
```rust
pub mod amas_tuning_whitelist;
```
- [ ] 跑确认通过（此 Task 实现与测试一并提交，但分步验证）：`cargo test -p learning-backend --lib store::operations::amas_tuning_whitelist`。预期 7 个测试全 pass。若 seed 测试因 `TIER_A_WHITELIST.len() != 11` 失败，回看 tuning_whitelist.rs 确认仍是 11 条。
- [ ] commit：`git add src/store/operations/amas_tuning_whitelist.rs src/store/operations/mod.rs && git commit -m "feat(store): amas_tuning_whitelist CRUD + seed_if_empty + WhitelistRow"`

### Task A3: amas_patch_canary 的 Store CRUD + PatchCanary + cohort 区间不重叠校验

**Files:**
- Create `src/store/operations/amas_patch_canary.rs`
- Modify `src/store/operations/mod.rs:3`（在 amas_canary 之后追加 `pub mod amas_patch_canary;`）

- [ ] 先写测试驱动失败：Create `src/store/operations/amas_patch_canary.rs`，含 PatchCanary 结构、五个方法实现与 cohort 重叠校验，以及 `#[cfg(test)]` 测试。完整内容：
```rust
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
        let rows: Result<Vec<_>, _> = match status {
            Some(s) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COLS} FROM amas_patch_canary WHERE status = ?1 ORDER BY started_at DESC"
                ))?;
                stmt.query_map(params![s], row_to_canary)?.collect()
            }
            None => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COLS} FROM amas_patch_canary ORDER BY started_at DESC"
                ))?;
                stmt.query_map([], row_to_canary)?.collect()
            }
        };
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
    fn validate_cohort(
        &self,
        lo: u32,
        hi: u32,
        exclude_id: Option<i64>,
    ) -> Result<(), StoreError> {
        if !(lo < hi) || hi > 100 {
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
            s.insert_patch_canary(1, "h1", 20, 90, 110, "{}").unwrap_err(),
            StoreError::Validation(_)
        ));
        assert!(matches!(
            s.insert_patch_canary(1, "h1", 20, 30, 30, "{}").unwrap_err(),
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
        for k in ["suggestionId", "versionHash", "cohortLo", "cohortHi", "baselineMetricsJson", "startedAt", "updatedAt"] {
            assert!(v.get(k).is_some(), "missing key {k}");
        }
    }
}
```
- [ ] 在 `src/store/operations/mod.rs` 中（Task A2 已插入 amas_tuning_whitelist 行后）追加：
```rust
pub mod amas_patch_canary;
```
- [ ] 跑确认失败先于实现：若严格 TDD 需要分离，先把上面文件里所有方法体替换成 `unimplemented!()` 跑 `cargo test -p learning-backend --lib store::operations::amas_patch_canary`（预期 panic `not implemented`），再贴回上面完整实现。否则直接跑下一步。
- [ ] 跑确认通过：`cargo test -p learning-backend --lib store::operations::amas_patch_canary`。预期 12 个测试全 pass，重点确认 `overlapping_cohort_rejected`、`adjacent_cohort_allowed`、`scale_excludes_self_from_overlap`。
- [ ] commit：`git add src/store/operations/amas_patch_canary.rs src/store/operations/mod.rs && git commit -m "feat(store): amas_patch_canary CRUD + cohort 半开区间不重叠校验 + PatchCanary"`

### Task A4: 成本/接受率/状态计数聚合方法

**Files:**
- Modify `src/store/operations/amas_suggestions.rs:296`（在 `aggregate_amas_suggestion_spend_today` 之后、`impl Store` 块内追加三个聚合方法）
- Modify `src/store/operations/amas_suggestions.rs:397`（在 `#[cfg(test)] mod tests` 块内追加测试）

- [ ] 先在 amas_suggestions.rs 的 tests mod（文件末 `spend_today_aggregates` 测试之后、`}` 之前）追加失败测试：
```rust
    #[test]
    fn daily_cost_groups_by_date_and_converts() {
        let store = fresh_store();
        // 两条 today,各 cost_usd=0.01 → 合计 0.02 USD;汇率 7.0 → 0.14 元
        store.insert_amas_suggestion(&ins(SuggestionStatus::Pending)).unwrap();
        store.insert_amas_suggestion(&ins(SuggestionStatus::AutoApplied)).unwrap();
        let daily = store.aggregate_daily_suggestion_cost_yuan(30, 7.0).unwrap();
        assert_eq!(daily.len(), 1, "all rows same day");
        assert!((daily[0].1 - 0.14).abs() < 1e-6, "got {}", daily[0].1);
        // date 形如 YYYY-MM-DD
        assert_eq!(daily[0].0.len(), 10);
    }

    #[test]
    fn acceptance_counts_approved_vs_rejected() {
        let store = fresh_store();
        store.insert_amas_suggestion(&ins(SuggestionStatus::Approved)).unwrap();
        store.insert_amas_suggestion(&ins(SuggestionStatus::AutoApplied)).unwrap();
        store.insert_amas_suggestion(&ins(SuggestionStatus::Rejected)).unwrap();
        store.insert_amas_suggestion(&ins(SuggestionStatus::Pending)).unwrap();
        let (approved, rejected) = store.aggregate_suggestion_acceptance().unwrap();
        // approved + auto_applied 计入 approved
        assert_eq!(approved, 2);
        assert_eq!(rejected, 1);
    }

    #[test]
    fn count_by_status_buckets() {
        let store = fresh_store();
        store.insert_amas_suggestion(&ins(SuggestionStatus::Pending)).unwrap();
        store.insert_amas_suggestion(&ins(SuggestionStatus::Pending)).unwrap();
        store.insert_amas_suggestion(&ins(SuggestionStatus::Rejected)).unwrap();
        let counts = store.count_suggestions_by_status().unwrap();
        let pending = counts.iter().find(|(s, _)| s == "pending").map(|(_, n)| *n);
        let rejected = counts.iter().find(|(s, _)| s == "rejected").map(|(_, n)| *n);
        assert_eq!(pending, Some(2));
        assert_eq!(rejected, Some(1));
    }
```
- [ ] 跑确认失败：`cargo test -p learning-backend --lib store::operations::amas_suggestions`。预期编译失败 `no method named aggregate_daily_suggestion_cost_yuan / aggregate_suggestion_acceptance / count_suggestions_by_status found for ... Store`。
- [ ] 在 amas_suggestions.rs 的 `impl Store` 块内、`aggregate_amas_suggestion_spend_today` 方法（结束于第 295 行 `}`）之后追加三个方法：
```rust
    /// 近 days 天按 date(created_at) 聚合 cost_usd,折算人民币。返回 (date, costYuan) 升序。
    /// date 为本地零时区 UTC 日期串(YYYY-MM-DD),与 created_at 的 rfc3339 前缀一致。
    pub fn aggregate_daily_suggestion_cost_yuan(
        &self,
        days: i64,
        usd_to_cny: f64,
    ) -> Result<Vec<(String, f64)>, StoreError> {
        let cutoff = (Utc::now() - Duration::days(days.max(0))).to_rfc3339();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT substr(created_at, 1, 10) AS d, COALESCE(SUM(cost_usd), 0.0) * ?2
             FROM amas_tuning_suggestions
             WHERE created_at >= ?1
             GROUP BY d
             ORDER BY d ASC",
        )?;
        let rows: Result<Vec<_>, _> = stmt
            .query_map(params![cutoff, usd_to_cny], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
            })?
            .collect();
        Ok(rows?)
    }

    /// 累计接受率分子分母:approved/auto_applied 计 approved,rejected 计 rejected。
    pub fn aggregate_suggestion_acceptance(&self) -> Result<(i64, i64), StoreError> {
        let conn = self.conn()?;
        let (approved, rejected) = conn.query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN status IN ('approved','auto_applied') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'rejected' THEN 1 ELSE 0 END), 0)
             FROM amas_tuning_suggestions",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        Ok((approved, rejected))
    }

    /// 各 status 的条数(供 PatchTabs 角标)。返回 (status, count),仅含有记录的 status。
    pub fn count_suggestions_by_status(&self) -> Result<Vec<(String, i64)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT status, COUNT(*) FROM amas_tuning_suggestions GROUP BY status ORDER BY status",
        )?;
        let rows: Result<Vec<_>, _> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect();
        Ok(rows?)
    }
```
- [ ] 跑确认通过：`cargo test -p learning-backend --lib store::operations::amas_suggestions`。预期原 4 个 + 新 3 个共 7 个测试全 pass。
- [ ] commit：`git add src/store/operations/amas_suggestions.rs && git commit -m "feat(store): 建议成本日聚合(¥)+接受率+按状态计数三聚合方法"`

### Task A5: system_settings.llm_advisor_enabled 读写

**Files:**
- Modify `src/store/operations/system_settings.rs:7-25`（SystemSettings 加 `llm_advisor_enabled` 字段 + default）
- Modify `src/store/operations/system_settings.rs:39-55`（Default impl 补字段）
- Modify `src/store/operations/system_settings.rs:57-115`（get/save SQL 增列 + 新增 set_llm_advisor_enabled）
- Modify `src/store/operations/system_settings.rs:118-216`（tests 补断言 + 修正既有 raw INSERT 列数 + 新增测试）

- [ ] 先在 tests mod 末尾（`deserialize_missing_fields_uses_defaults` 之后、`}` 之前）追加失败测试：
```rust
    #[test]
    fn advisor_enabled_default_false_then_set_true() {
        let store = test_store();
        assert!(!store.get_system_settings().unwrap().llm_advisor_enabled);
        store.set_llm_advisor_enabled(true).unwrap();
        assert!(store.get_system_settings().unwrap().llm_advisor_enabled);
        store.set_llm_advisor_enabled(false).unwrap();
        assert!(!store.get_system_settings().unwrap().llm_advisor_enabled);
    }
```
- [ ] 跑确认失败：`cargo test -p learning-backend --lib store::operations::system_settings::tests::advisor_enabled_default_false_then_set_true`。预期 `no field llm_advisor_enabled on type SystemSettings` 或 `no method set_llm_advisor_enabled`。
- [ ] 给 SystemSettings 结构加字段。Edit system_settings.rs，把 `llm_advisor_max_cost_per_month_yuan: f64,` 字段定义（第 24 行）后追加：
```rust
    /// C2/C3:LLM 顾问运行时巡查开关(env ENABLE_LLM_ADVISOR_WORKER 与本列取或)。
    #[serde(default)]
    pub llm_advisor_enabled: bool,
```
- [ ] 给 Default impl 补字段。把 `llm_advisor_max_cost_per_month_yuan: 100.0,`（第 52 行，Default impl 内）后追加：
```rust
            llm_advisor_enabled: false,
```
- [ ] 改 get SQL:把 SELECT 列尾 `llm_advisor_max_cost_per_month_yuan`（第 64 行）改为追加新列：
```rust
                        llm_advisor_max_cost_per_month_yuan, llm_advisor_enabled
```
- [ ] 改 get 行映射:把 `llm_advisor_max_cost_per_month_yuan: r.get::<_, f64>(8).unwrap_or(100.0),`（第 77 行）后追加：
```rust
                        llm_advisor_enabled: r.get::<_, i64>(9).unwrap_or(0) != 0,
```
- [ ] 改 save SQL 列清单:把 INSERT 列尾 `llm_advisor_max_cost_per_month_yuan)`（第 96 行）改为 `llm_advisor_max_cost_per_month_yuan, llm_advisor_enabled)`;VALUES 占位 `?9)`（第 97 行）改为 `?9, ?10)`;ON CONFLICT SET 尾 `llm_advisor_max_cost_per_month_yuan=?9`（第 101 行）改为 `llm_advisor_max_cost_per_month_yuan=?9, llm_advisor_enabled=?10`。逐条用 Edit 精确替换：
  - `(singleton_id, max_users, ... llm_advisor_max_cost_per_month_yuan)` → 末尾加 `, llm_advisor_enabled`
  - `VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)` → `VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)`
  - `llm_advisor_max_cost_per_month_yuan=?9` → `llm_advisor_max_cost_per_month_yuan=?9, llm_advisor_enabled=?10`
- [ ] 改 save params:把 `settings.llm_advisor_max_cost_per_month_yuan,`（第 111 行）后追加：
```rust
                settings.llm_advisor_enabled as i64,
```
- [ ] 在 `impl Store` 块内 `save_system_settings` 之后追加便捷写方法（避免改窗读改写时丢字段——先 get 再 save）：
```rust
    /// 仅切换 llm_advisor_enabled,其余 settings 保留。
    pub fn set_llm_advisor_enabled(&self, enabled: bool) -> Result<(), StoreError> {
        let mut s = self.get_system_settings()?;
        s.llm_advisor_enabled = enabled;
        self.save_system_settings(&s)
    }
```
- [ ] 修正既有测试构造体:`save_and_get_roundtrip` 里 `SystemSettings { ... llm_advisor_max_cost_per_month_yuan: 200.0, }`（第 154 行）后追加 `llm_advisor_enabled: true,`;并在该测试断言区追加 `assert!(got.llm_advisor_enabled);`。
- [ ] 修正 `get_replaces_null_wordbook_center_url_with_default` 的 raw INSERT:列清单尾追加 `, llm_advisor_enabled`,VALUES 尾 `100.0)` 改为 `100.0, 0)`（保持列/值数对齐,否则迁移后该 raw SQL 列数不匹配）。Edit 把 `..., llm_advisor_max_cost_per_month_yuan)` → 加 `, llm_advisor_enabled`，`..., 100.0)` → `..., 100.0, 0)`。
- [ ] 跑确认通过：`cargo test -p learning-backend --lib store::operations::system_settings`。预期原 6 个 + 新 1 个全 pass（重点 `advisor_enabled_default_false_then_set_true`、`save_and_get_roundtrip`、`get_replaces_null_*`）。
- [ ] commit：`git add src/store/operations/system_settings.rs && git commit -m "feat(store): system_settings.llm_advisor_enabled 读写 + set_llm_advisor_enabled"`

### Task A6: 模块 A 整体回归 + clippy 闸口

**Files:** 无新增（验证现有改动）

- [ ] 跑模块 A 全量 store 测试：`cargo test -p learning-backend --lib store::`。预期全 pass（含 migrate / amas_tuning_whitelist / amas_patch_canary / amas_suggestions / system_settings）。
- [ ] 跑 clippy 闸口（CI 等价）：`cargo clippy -p learning-backend --lib --all-features -- -D warnings`。预期 0 warning。重点排查：
  - `insert_tuning_whitelist`/`validate_cohort` 里的 `if !(min_safe < max_safe)` / `if !(lo < hi)` 可能触发 `clippy::nonminimal_bool` —— 若报，改为 `if min_safe >= max_safe` / `if lo >= hi || hi > 100`。
  - 浮点字面量避开近似常量（已用 0.14/0.55/200.0 等）。
- [ ] 若 clippy 报上述 bool 简化，按其建议替换后重跑 `cargo clippy -p learning-backend --lib -- -D warnings` 至 0 warning。
- [ ] commit（仅当上一步有改动）：`git add -A && git commit -m "chore(store): 模块 A clippy 闸口对齐(bool 简化)"`

---

模块 A 共定义/导出以下供下游模块（端点层、引擎、worker、前端）引用的契约符号，全部已落地：
- 类型：`store::operations::amas_tuning_whitelist::WhitelistRow`（camelCase: path/minSafe/maxSafe）、`store::operations::amas_patch_canary::PatchCanary`（camelCase 全字段）。
- 白名单 Store 方法：`list_tuning_whitelist` / `insert_tuning_whitelist` / `delete_tuning_whitelist` / `seed_tuning_whitelist_if_empty`。
- canary Store 方法：`list_patch_canaries(Option<&str>)` / `get_active_patch_canaries` / `get_patch_canary(i64)` / `insert_patch_canary` / `update_patch_canary_scale` / `set_patch_canary_status`（cohort 半开区间 `[lo,hi)`，active 行不重叠，校验在 `validate_cohort`）。
- 聚合：`aggregate_daily_suggestion_cost_yuan(days,usd_to_cny)->Vec<(String,f64)>` / `aggregate_suggestion_acceptance()->(i64,i64)` / `count_suggestions_by_status()->Vec<(String,i64)>`；复用既有 `get_llm_cost_this_month(month)`。
- settings：`SystemSettings.llm_advisor_enabled: bool` + `set_llm_advisor_enabled(bool)`，`get_system_settings` 已读出该列。
- 迁移：`025_amas_advisor`（amas_tuning_whitelist + amas_patch_canary + system_settings.llm_advisor_enabled），schema.rs 全新库 CREATE 已同步该列。

下游需注意：`seed_tuning_whitelist_if_empty()` 应在应用启动时调用（非本模块职责，归端点/启动装配模块）；`get_active_patch_canaries()` 是引擎路由与 canary_monitor worker 的唯一 active 源。

## 模块 B — 后端 C1/C2/C3（成本统计 + 巡查控制 + 顾问配置端点）

### Task B1: 后端 C1 成本/统计端点 — store 聚合方法 + `GET /advisor/cost` + `GET /advisor/cost/daily`

**Files:**
- Modify `src/store/operations/amas_suggestions.rs`（新增聚合方法挂 `impl Store`，插在现有 `aggregate_amas_suggestion_spend_today` 之后约 296 行；`#[cfg(test)]` 追加到现有 `mod tests`）
- Modify `src/routes/admin/amas.rs`（在 `admin_router()` 内 56-59 行附近加路由；handler 加在文件末尾 advisor 段）
- Modify `tests/admin_amas_http.rs`（新增 `#[tokio::test]`）

- [ ] 在 `src/store/operations/amas_suggestions.rs` 现有 `mod tests` 末尾（约 396 行 `}` 前，`spend_today_aggregates` 之后）加失败 store 单测：
```rust
    #[test]
    fn daily_cost_aggregates_by_date_in_yuan() {
        let store = fresh_store();
        // 注：cost_usd=0.01，usd_to_cny=7.3 → 当日 costYuan ≈ 0.073
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::Pending))
            .unwrap();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::AutoApplied))
            .unwrap();
        let rows = store.aggregate_daily_suggestion_cost_yuan(30, 7.3).unwrap();
        assert_eq!(rows.len(), 1, "两条同日建议聚合成一行");
        let (date, cost_yuan) = &rows[0];
        assert_eq!(date.len(), 10, "date 形如 YYYY-MM-DD");
        assert!((cost_yuan - 0.146).abs() < 1e-6, "0.02 usd * 7.3 = 0.146 yuan");
    }

    #[test]
    fn acceptance_counts_approved_vs_rejected() {
        let store = fresh_store();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::Approved))
            .unwrap();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::Approved))
            .unwrap();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::Rejected))
            .unwrap();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::Pending))
            .unwrap();
        let (approved, rejected) = store.aggregate_suggestion_acceptance().unwrap();
        // auto_applied 也计入接受口径，此处仅 approved
        assert_eq!(approved, 2);
        assert_eq!(rejected, 1);
    }

    #[test]
    fn count_by_status_groups_correctly() {
        let store = fresh_store();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::Pending))
            .unwrap();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::Pending))
            .unwrap();
        store
            .insert_amas_suggestion(&ins(SuggestionStatus::Rejected))
            .unwrap();
        let counts = store.count_suggestions_by_status().unwrap();
        let pending = counts.iter().find(|(s, _)| s == "pending").map(|(_, c)| *c);
        let rejected = counts.iter().find(|(s, _)| s == "rejected").map(|(_, c)| *c);
        assert_eq!(pending, Some(2));
        assert_eq!(rejected, Some(1));
    }
```

- [ ] 跑确认编译失败（方法未定义）：
```bash
cargo test --lib amas_suggestions:: 2>&1 | tail -20
```
预期：`error[E0599]: no method named 'aggregate_daily_suggestion_cost_yuan' found` / `aggregate_suggestion_acceptance` / `count_suggestions_by_status`。

- [ ] 在 `src/store/operations/amas_suggestions.rs` 的 `impl Store` 块内（紧跟 `aggregate_amas_suggestion_spend_today` 结束的 `}` 之后，约 295 行）加最小实现：
```rust
    /// 近 `days` 天按 `date(created_at)` 聚合人民币成本（cost_usd * usd_to_cny）。
    /// 返回 `(date, costYuan)` 升序；空表返回空数组。
    pub fn aggregate_daily_suggestion_cost_yuan(
        &self,
        days: i64,
        usd_to_cny: f64,
    ) -> Result<Vec<(String, f64)>, StoreError> {
        let cutoff = (Utc::now() - Duration::days(days.max(1))).to_rfc3339();
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT substr(created_at, 1, 10) AS d, COALESCE(SUM(cost_usd), 0.0) AS c
             FROM amas_tuning_suggestions
             WHERE created_at >= ?1
             GROUP BY d
             ORDER BY d ASC",
        )?;
        let rows: Result<Vec<(String, f64)>, _> = stmt
            .query_map(params![cutoff], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)? * usd_to_cny))
            })?
            .collect();
        Ok(rows?)
    }

    /// 累计接受率口径：approved + auto_applied 计入 approved，rejected 单列。
    pub fn aggregate_suggestion_acceptance(&self) -> Result<(i64, i64), StoreError> {
        let conn = self.conn()?;
        let (approved, rejected) = conn.query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN status IN ('approved','auto_applied') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'rejected' THEN 1 ELSE 0 END), 0)
             FROM amas_tuning_suggestions",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        Ok((approved, rejected))
    }

    /// 按 status 分组计数，供 PatchTabs 角标。返回 `(status, count)`。
    pub fn count_suggestions_by_status(&self) -> Result<Vec<(String, i64)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT status, COUNT(*) FROM amas_tuning_suggestions GROUP BY status",
        )?;
        let rows: Result<Vec<(String, i64)>, _> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect();
        Ok(rows?)
    }
```

- [ ] 跑确认 store 单测通过：
```bash
cargo test --lib amas_suggestions:: 2>&1 | tail -20
```
预期：`test result: ok.`，含 `daily_cost_aggregates_by_date_in_yuan`、`acceptance_counts_approved_vs_rejected`、`count_by_status_groups_correctly` 三项 passed。

- [ ] commit：
```bash
git add src/store/operations/amas_suggestions.rs
git commit -m "feat(amas-advisor): store 聚合方法 — 按日成本¥ + 接受率 + 状态计数"
```

- [ ] 在 `tests/admin_amas_http.rs` 末尾（最后一个 `}` 后）加失败集成测试：
```rust
#[tokio::test]
async fn it_advisor_cost_endpoints() {
    let app = spawn_test_server().await;
    let admin_token = common::auth::setup_admin_and_get_token(&app.app).await;

    let cost = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/advisor/cost",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (cost_status, _, cost_body) = response_json(cost).await;
    assert_eq!(cost_status, StatusCode::OK);
    // 空表兜底：所有数值字段为 0，acceptanceRate=0
    assert!(cost_body["data"]["monthYuan"].is_number());
    assert!(cost_body["data"]["monthCapYuan"].is_number());
    assert!(cost_body["data"]["quotaPct"].is_number());
    assert!(cost_body["data"]["acceptedCount"].is_number());
    assert!(cost_body["data"]["rejectedCount"].is_number());
    assert!(cost_body["data"]["acceptanceRate"].is_number());

    let daily = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/advisor/cost/daily?days=30",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (daily_status, _, daily_body) = response_json(daily).await;
    assert_eq!(daily_status, StatusCode::OK);
    assert!(daily_body["data"].is_array());

    // 鉴权：普通用户 401
    let user_token = login_and_get_token(&app.app).await;
    let denied = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/advisor/cost",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (denied_status, _, _) = response_json(denied).await;
    assert_eq!(denied_status, StatusCode::UNAUTHORIZED);
}
```

- [ ] 跑确认集成测试失败（路由 404）：
```bash
cargo test --test admin_amas_http it_advisor_cost_endpoints 2>&1 | tail -20
```
预期：`assertion ... left: 404, right: 200`（路由未注册）。

- [ ] 在 `src/routes/admin/amas.rs` 的 `admin_router()` 内（59 行 `.route("/suggestions/:id/reject", ...)` 之后）加路由：
```rust
        // C1: advisor 成本/统计
        .route("/advisor/cost", get(advisor_cost))
        .route("/advisor/cost/daily", get(advisor_cost_daily))
```

- [ ] 在 `src/routes/admin/amas.rs` 文件末尾（最后 `}` 之后）追加 C1 handler + 响应结构：
```rust
// ─────────── C1: advisor 成本 / 统计 ───────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AdvisorCostStats {
    month_yuan: f64,
    month_cap_yuan: f64,
    quota_pct: f64,
    forecast_yuan: f64,
    avg7d_cost_yuan: f64,
    month_calls: i64,
    accepted_count: i64,
    rejected_count: i64,
    acceptance_rate: f64,
}

async fn advisor_cost(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let usd_to_cny = state.config().llm.usd_to_cny_rate;
    let month = chrono::Utc::now().format("%Y-%m").to_string();
    let stats = state
        .run_store_task("admin.amas.advisor_cost", move |store| {
            let month_yuan = store.get_llm_cost_this_month(&month)?;
            let settings = store.get_system_settings()?;
            let (approved, rejected) = store.aggregate_suggestion_acceptance()?;
            // 本月调用次数 + 近 7 天¥成本（用于均单次 + 预测）
            let daily = store.aggregate_daily_suggestion_cost_yuan(7, usd_to_cny)?;
            let counts = store.count_suggestions_by_status()?;
            let month_calls: i64 = counts.iter().map(|(_, c)| *c).sum();
            Ok::<_, crate::store::StoreError>((
                month_yuan,
                settings.llm_advisor_max_cost_per_month_yuan,
                approved,
                rejected,
                daily,
                month_calls,
            ))
        })
        .await??;

    let (month_yuan, month_cap_yuan, approved, rejected, daily7, month_calls) = stats;
    let quota_pct = if month_cap_yuan > 0.0 {
        (month_yuan / month_cap_yuan * 100.0).min(999.0)
    } else {
        0.0
    };
    // 月末预测：按当前 day-of-month 线性外推
    let now = chrono::Utc::now();
    let day = now.day().max(1) as f64;
    let days_in_month = days_in_month(now.year(), now.month()) as f64;
    let forecast_yuan = month_yuan / day * days_in_month;
    let total7: f64 = daily7.iter().map(|(_, c)| *c).sum();
    let avg7d_cost_yuan = if month_calls > 0 {
        total7 / (month_calls as f64).max(1.0)
    } else {
        0.0
    };
    let decided = approved + rejected;
    let acceptance_rate = if decided > 0 {
        approved as f64 / decided as f64
    } else {
        0.0
    };

    Ok(ok(AdvisorCostStats {
        month_yuan,
        month_cap_yuan,
        quota_pct,
        forecast_yuan,
        avg7d_cost_yuan,
        month_calls,
        accepted_count: approved,
        rejected_count: rejected,
        acceptance_rate,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CostDailyQuery {
    days: Option<i64>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CostDailyPoint {
    date: String,
    cost_yuan: f64,
}

async fn advisor_cost_daily(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<CostDailyQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(30).clamp(1, 90);
    let usd_to_cny = state.config().llm.usd_to_cny_rate;
    let rows = state
        .run_store_task("admin.amas.advisor_cost_daily", move |store| {
            store.aggregate_daily_suggestion_cost_yuan(days, usd_to_cny)
        })
        .await??;
    let points: Vec<CostDailyPoint> = rows
        .into_iter()
        .map(|(date, cost_yuan)| CostDailyPoint { date, cost_yuan })
        .collect();
    Ok(ok(points))
}

/// 给定年月返回该月天数（用于月末成本线性外推）。
fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let first_next = chrono::NaiveDate::from_ymd_opt(ny, nm, 1).unwrap_or_default();
    let first_this = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap_or_default();
    (first_next - first_this).num_days() as u32
}
```

- [ ] 在 `src/routes/admin/amas.rs` 顶部确认 `use chrono::Datelike;`（`now.day()/year()/month()` 需要 trait）。若未引入，在第 12 行 `use crate::store::operations::amas_versions::ConfigVersionSource;` 之后加：
```rust
use chrono::Datelike;
```

- [ ] 跑确认集成测试通过：
```bash
cargo test --test admin_amas_http it_advisor_cost_endpoints 2>&1 | tail -20
```
预期：`test it_advisor_cost_endpoints ... ok`。

- [ ] commit：
```bash
git add src/routes/admin/amas.rs tests/admin_amas_http.rs
git commit -m "feat(amas-advisor): C1 端点 GET /advisor/cost + /advisor/cost/daily（¥月度口径 + 接受率）"
```

---

### Task B2: 后端 C2 巡查控制 — `system_settings.llm_advisor_enabled` 列 + store 写读 + `POST /advisor/run` + `POST /suggestions/approve-all`

**Files:**
- Modify `src/store/migrate.rs`（`run_migrations` 内 system_settings ADD COLUMN 守卫，仿 417-419 的 `amas_auto_apply_enabled` 写法）
- Modify `src/store/operations/system_settings.rs`（`SystemSettings` 加字段 1-25 行 + 读 58-88 行 + 写 90-115 行 + 新增 `set_llm_advisor_enabled`；`#[cfg(test)]`）
- Modify `src/routes/admin/amas.rs`（`admin_router()` 加路由 + handler）
- Modify `tests/admin_amas_http.rs`（新增 `#[tokio::test]`）

- [ ] 先看现有 system_settings 列迁移写法定锚点：
```bash
sed -n '410,425p' src/store/migrate.rs
```
预期：看到 `("amas_auto_apply_enabled", "INTEGER NOT NULL DEFAULT 0"),` 等 `(列名, 类型)` 列表 + 循环 `ADD COLUMN` 守卫。

- [ ] 在 `src/store/operations/system_settings.rs` 现有 `mod tests` 内（约 215 行 `}` 前）加失败 store 单测：
```rust
    #[test]
    fn llm_advisor_enabled_defaults_false_and_toggles() {
        let store = test_store();
        assert!(!store.get_system_settings().unwrap().llm_advisor_enabled);
        store.set_llm_advisor_enabled(true).unwrap();
        assert!(store.get_system_settings().unwrap().llm_advisor_enabled);
        store.set_llm_advisor_enabled(false).unwrap();
        assert!(!store.get_system_settings().unwrap().llm_advisor_enabled);
    }
```

- [ ] 跑确认失败：
```bash
cargo test --lib system_settings:: 2>&1 | tail -20
```
预期：`error[E0560]: struct ... has no field named llm_advisor_enabled` 与 `no method named set_llm_advisor_enabled`。

- [ ] 在 `src/store/operations/system_settings.rs` 的 `SystemSettings` 结构体内（第 24 行 `pub llm_advisor_max_cost_per_month_yuan: f64,` 之后）加字段：
```rust
    /// C2: advisor worker 运行时开关（与 env ENABLE_LLM_ADVISOR_WORKER 取或）
    #[serde(default)]
    pub llm_advisor_enabled: bool,
```

- [ ] 在 `impl Default for SystemSettings`（第 52 行 `llm_advisor_max_cost_per_month_yuan: 100.0,` 之后）加：
```rust
            llm_advisor_enabled: false,
```

- [ ] 在 `get_system_settings` 的 SELECT 列尾补 `llm_advisor_enabled`（第 64 行）、构造体里加读取（第 77 行之后）：
```rust
                        llm_advisor_max_cost_per_month_yuan: r.get::<_, f64>(8).unwrap_or(100.0),
                        llm_advisor_enabled: r.get::<_, i64>(9).unwrap_or(0) != 0,
```
SELECT 改为：
```rust
                "SELECT max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url,
                        amas_auto_apply_enabled, amas_auto_apply_max_per_day, amas_auto_apply_min_confidence,
                        llm_advisor_max_cost_per_month_yuan, llm_advisor_enabled
                 FROM system_settings WHERE singleton_id=1",
```

- [ ] 在 `save_system_settings` 的 INSERT 列 + VALUES + ON CONFLICT + params 加第 10 列（仿现有第 9 列）：
```rust
        conn.execute(
            "INSERT INTO system_settings
                (singleton_id, max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url,
                 amas_auto_apply_enabled, amas_auto_apply_max_per_day, amas_auto_apply_min_confidence,
                 llm_advisor_max_cost_per_month_yuan, llm_advisor_enabled)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(singleton_id) DO UPDATE SET
                max_users=?1, registration_enabled=?2, maintenance_mode=?3, default_daily_words=?4, wordbook_center_url=?5,
                amas_auto_apply_enabled=?6, amas_auto_apply_max_per_day=?7, amas_auto_apply_min_confidence=?8,
                llm_advisor_max_cost_per_month_yuan=?9, llm_advisor_enabled=?10",
            params![
                settings.max_users as i64,
                settings.registration_enabled as i64,
                settings.maintenance_mode as i64,
                settings.default_daily_words as i64,
                settings.wordbook_center_url.as_deref(),
                settings.amas_auto_apply_enabled as i64,
                settings.amas_auto_apply_max_per_day as i64,
                settings.amas_auto_apply_min_confidence,
                settings.llm_advisor_max_cost_per_month_yuan,
                settings.llm_advisor_enabled as i64,
            ],
        )?;
```

- [ ] 在 `impl Store` 块内（`save_system_settings` 之后，第 115 行 `}` 之后、`impl` 闭合 `}` 之前）加 `set_llm_advisor_enabled`：
```rust
    /// C2: 仅切换 advisor 运行时开关（读现有 settings → 改单字段 → upsert）。
    pub fn set_llm_advisor_enabled(&self, enabled: bool) -> Result<(), StoreError> {
        let mut settings = self.get_system_settings()?;
        settings.llm_advisor_enabled = enabled;
        self.save_system_settings(&settings)
    }
```

- [ ] 更新现有受影响测试：`save_and_get_roundtrip` 的 `SystemSettings { ... }` 字面量加 `llm_advisor_enabled: true,`（第 154 行后），并在断言段加 `assert!(got.llm_advisor_enabled);`；`get_replaces_null_wordbook_center_url_with_default` 的 raw INSERT 列与 VALUES 各补一列 `llm_advisor_enabled` / `0`。改后：
```rust
            conn.execute(
                "INSERT INTO system_settings (singleton_id, max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url, amas_auto_apply_enabled, amas_auto_apply_max_per_day, amas_auto_apply_min_confidence, llm_advisor_max_cost_per_month_yuan, llm_advisor_enabled)
                 VALUES (1, 1, 1, 0, 1, NULL, 0, 1, 0.5, 100.0, 0)",
                [],
            ).unwrap();
```

- [ ] 在 `src/store/migrate.rs` 的 system_settings ADD COLUMN 守卫列表（约 417-419 行，与 `amas_auto_apply_enabled` 同列表）加一项：
```rust
        ("llm_advisor_enabled", "INTEGER NOT NULL DEFAULT 0"),
```

- [ ] 跑确认 store 单测通过：
```bash
cargo test --lib system_settings:: 2>&1 | tail -20
```
预期：`test result: ok.`，含 `llm_advisor_enabled_defaults_false_and_toggles`。

- [ ] commit：
```bash
git add src/store/migrate.rs src/store/operations/system_settings.rs
git commit -m "feat(amas-advisor): system_settings 加 llm_advisor_enabled 列 + set 方法（C2 运行时开关）"
```

- [ ] 在 `tests/admin_amas_http.rs` 末尾加失败集成测试（`POST /advisor/run` + `approve-all`）：
```rust
#[tokio::test]
async fn it_advisor_run_and_approve_all() {
    let app = spawn_test_server().await;
    let admin_token = common::auth::setup_admin_and_get_token(&app.app).await;

    // POST /advisor/run：LLM 默认 disabled（test config llm.enabled=false）→ produced=false
    let run = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/advisor/run",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (run_status, _, run_body) = response_json(run).await;
    assert_eq!(run_status, StatusCode::OK);
    assert_eq!(run_body["data"]["produced"], false);
    assert!(run_body["data"]["suggestionId"].is_null());

    // 预置一条 pending 建议，approve-all 应处理它
    app.state
        .store()
        .insert_amas_suggestion(
            &learning_backend::store::operations::amas_suggestions::InsertSuggestion {
                based_on_version_hash: "approveall-base".into(),
                patch_json: r#"{"memoryModel.baseDesiredRetention":0.85}"#.into(),
                rationale: "approve-all 测试".into(),
                evidence_json: "{}".into(),
                cost_usd: Some(0.01),
                tokens_input: Some(10),
                tokens_output: Some(5),
                confidence: Some(0.9),
                initial_status:
                    learning_backend::store::operations::amas_suggestions::SuggestionStatus::Pending,
                decided_by: None,
                decision_note: None,
                base_values_json: None,
            },
        )
        .expect("insert pending");

    let approve_all = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/suggestions/approve-all",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (aa_status, _, aa_body) = response_json(approve_all).await;
    assert_eq!(aa_status, StatusCode::OK);
    let results = aa_body["data"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["ok"], true);

    // 再 approve-all 一次：已无 pending → 空 results
    let again = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/suggestions/approve-all",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (again_status, _, again_body) = response_json(again).await;
    assert_eq!(again_status, StatusCode::OK);
    assert_eq!(again_body["data"]["results"].as_array().unwrap().len(), 0);
}
```

- [ ] 跑确认失败（路由 404）：
```bash
cargo test --test admin_amas_http it_advisor_run_and_approve_all 2>&1 | tail -20
```
预期：`assertion ... left: 404, right: 200`。

- [ ] 在 `src/routes/admin/amas.rs` 的 `admin_router()` 内（C1 两行之后）加路由：
```rust
        // C2: 巡查控制
        .route("/advisor/run", post(advisor_run))
        .route("/suggestions/approve-all", post(approve_all_suggestions))
```

- [ ] 在 `src/routes/admin/amas.rs` 文件末尾追加 C2 handler。`advisor_run` 直接调 `llm_advisor::run`，再读最新 pending/auto_applied 判定是否产出。为复用 approve 逻辑，把现有 `approve_suggestion` 的核心抽成 `approve_one`（见下一步），`approve_all_suggestions` 循环调用：
```rust
// ─────────── C2: 巡查控制 ───────────

async fn advisor_run(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;

    // 触发前记录最新 suggestion id，作为"是否新产出"的基准
    let before = state
        .run_store_task("admin.amas.advisor_run.before", |store| {
            store.list_amas_suggestions(None, 1)
        })
        .await??
        .first()
        .map(|r| r.id)
        .unwrap_or(0);

    let llm_cfg = state.config().llm.clone();
    crate::workers::llm_advisor::run(state.store(), Some(&llm_cfg), state.amas(), Some(&state))
        .await;

    let latest = state
        .run_store_task("admin.amas.advisor_run.after", |store| {
            store.list_amas_suggestions(None, 1)
        })
        .await??;
    let produced = latest.first().map(|r| r.id).unwrap_or(0) > before;
    let suggestion_id = if produced {
        latest.first().map(|r| r.id)
    } else {
        None
    };
    // SuggestionStatus 仅为保持 import 一致性，无额外逻辑
    let _ = SuggestionStatus::Pending;
    Ok(ok(serde_json::json!({
        "produced": produced,
        "suggestionId": suggestion_id,
    })))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ApproveAllItem {
    id: i64,
    ok: bool,
    error: Option<String>,
}

async fn approve_all_suggestions(
    admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;

    let pending = state
        .run_store_task("admin.amas.approve_all.list", |store| {
            store.list_amas_suggestions(Some(SuggestionStatus::Pending), 500)
        })
        .await??;

    let mut results = Vec::with_capacity(pending.len());
    for s in pending {
        let id = s.id;
        match approve_one(&state, &admin.admin_id, id).await {
            Ok(()) => results.push(ApproveAllItem { id, ok: true, error: None }),
            Err(e) => results.push(ApproveAllItem {
                id,
                ok: false,
                error: Some(e.message().to_string()),
            }),
        }
    }
    Ok(ok(serde_json::json!({ "results": results })))
}
```

- [ ] 把 `approve_suggestion`（587-656 行）的核心抽成可复用的 `approve_one(state, admin_id, id) -> Result<(), AppError>`，让单条 handler 与 approve-all 共用。新增 `approve_one` 函数（放在 `approve_suggestion` 之上或文件末尾 C2 段），并把现有 `approve_suggestion` 改为调用它。新 `approve_one`：
```rust
/// C2 复用核心：校验 pending → validate_patch → 应用 patch → 落版本 → 标记 approved。
/// approve_suggestion 单条端点与 approve-all 批量端点共用，确保白名单校验一致。
pub(crate) async fn approve_one(
    state: &AppState,
    admin_id: &str,
    id: i64,
) -> Result<(), AppError> {
    use crate::amas::tuning_whitelist::validate_patch;
    use crate::store::operations::amas_suggestions::SuggestionStatus;

    let suggestion = state
        .run_store_task("admin.amas.approve_lookup", move |store| {
            store.get_amas_suggestion(id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("建议不存在"))?;

    if !matches!(suggestion.status, SuggestionStatus::Pending) {
        return Err(AppError::bad_request("BAD_STATUS", "仅 pending 建议可被批准"));
    }

    let patch_obj = suggestion
        .patch_json
        .as_object()
        .ok_or_else(|| AppError::internal("patch_json 非对象"))?
        .clone();
    let errs = validate_patch(&patch_obj);
    if !errs.is_empty() {
        return Err(AppError::bad_request("PATCH_INVALID", &errs.join("；")));
    }

    let current = state.amas().get_config();
    let cfg_value =
        serde_json::to_value(&current).map_err(|e| AppError::internal(&format!("ser: {e}")))?;
    let mut cfg_value = cfg_value;
    for (path, value) in &patch_obj {
        write_path(&mut cfg_value, path, value.clone());
    }
    let new_cfg: crate::amas::config::AMASConfig =
        serde_json::from_value(cfg_value).map_err(|e| {
            AppError::bad_request("PATCH_INVALID", &format!("应用 patch 后反序列化失败: {e}"))
        })?;

    apply_and_persist_config(
        state,
        admin_id,
        new_cfg,
        ConfigVersionSource::LlmSuggested,
        Some(format!("approve suggestion#{}", id)),
    )
    .await?;

    let admin_id_owned = admin_id.to_string();
    state
        .run_store_task("admin.amas.approve_update", move |store| {
            store.update_amas_suggestion_status(
                id,
                SuggestionStatus::Approved,
                Some(&admin_id_owned),
                None,
            )
        })
        .await??;
    Ok(())
}
```
并把现有 `approve_suggestion` 替换为薄包装（保留原 `note` 行为：approve-all 不传 note，单条仍写 note）。最小改动版（复用 approve_one 后追加 note 更新）：
```rust
async fn approve_suggestion(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    JsonBody(body): JsonBody<DecisionBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;
    approve_one(&state, &admin.admin_id, id).await?;
    // 单条端点保留 note 审计（approve_one 内已置 approved，这里仅补 note）
    if body.note.is_some() {
        let admin_id = admin.admin_id.clone();
        state
            .run_store_task("admin.amas.approve_note", move |store| {
                store.update_amas_suggestion_status(
                    id,
                    SuggestionStatus::Approved,
                    Some(&admin_id),
                    body.note.as_deref(),
                )
            })
            .await??;
    }
    Ok(ok(serde_json::json!({"approved": true, "id": id})))
}
```

- [ ] 确认 `AppError` 暴露 `message()`（approve-all 收集 error 用）：
```bash
grep -n "pub fn message\|message:" src/response.rs | head
```
若无 `message()` 取数器，改用 `format!("{e:?}")` 或 `e.message` 字段直读；据实际签名调整 `approve_all_suggestions` 内 `e.message().to_string()`。预期：找到 `message` 字段或 getter。

- [ ] 跑确认集成测试通过：
```bash
cargo test --test admin_amas_http it_advisor_run_and_approve_all 2>&1 | tail -20
```
预期：`test it_advisor_run_and_approve_all ... ok`。

- [ ] 回归：原单条 approve/reject 与既有 AMAS 集成测试不破：
```bash
cargo test --test admin_amas_http 2>&1 | tail -20
```
预期：全部 `ok`，含 `it_amas_user_and_admin_endpoints`。

- [ ] commit：
```bash
git add src/routes/admin/amas.rs tests/admin_amas_http.rs
git commit -m "feat(amas-advisor): C2 端点 POST /advisor/run + /suggestions/approve-all（抽出 approve_one 复用）"
```

---

### Task B3: 后端 C3 顾问配置 — `GET /advisor/config` + `PUT /advisor/config`（system_settings 可写 + LLMConfig 只读脱敏）

**Files:**
- Modify `src/store/operations/system_settings.rs`（`SystemSettings` 加 `amas_grayscale_steps`/`llm_advisor_enabled` 写读已在 N+1 完成；本任务新增灰度策略字段）
- Modify `src/store/migrate.rs`（灰度策略列 ADD COLUMN 守卫）
- Modify `src/routes/admin/amas.rs`（路由 + handler）
- Modify `tests/admin_amas_http.rs`（新增 `#[tokio::test]`）

- [ ] 在 `src/store/operations/system_settings.rs` 的 `mod tests` 内加灰度策略往返失败单测：
```rust
    #[test]
    fn grayscale_steps_default_and_roundtrip() {
        let store = test_store();
        // 默认 20/60/100
        assert_eq!(store.get_system_settings().unwrap().amas_grayscale_steps, [20, 60, 100]);
        let mut s = store.get_system_settings().unwrap();
        s.amas_grayscale_steps = [10, 50, 100];
        store.save_system_settings(&s).unwrap();
        assert_eq!(store.get_system_settings().unwrap().amas_grayscale_steps, [10, 50, 100]);
    }
```

- [ ] 跑确认失败：
```bash
cargo test --lib system_settings::grayscale 2>&1 | tail -20
```
预期：`no field named amas_grayscale_steps`。

- [ ] 在 `SystemSettings` 结构体加字段（`llm_advisor_enabled` 之后）：
```rust
    /// C3: 灰度策略三档（20→60→100），存为逗号分隔字符串列，序列化为 [u32;3]
    #[serde(default = "default_grayscale_steps")]
    pub amas_grayscale_steps: [u32; 3],
```
并加默认 helper（与其他 `default_*` 并列）：
```rust
fn default_grayscale_steps() -> [u32; 3] {
    [20, 60, 100]
}
```
`impl Default` 内（`llm_advisor_enabled: false,` 之后）加：
```rust
            amas_grayscale_steps: [20, 60, 100],
```

- [ ] `get_system_settings`：SELECT 尾再加 `amas_grayscale_steps`（第 11 列，TEXT 存 "20,60,100"），读取解析。SELECT 改为追加列，构造体加：
```rust
                        llm_advisor_enabled: r.get::<_, i64>(9).unwrap_or(0) != 0,
                        amas_grayscale_steps: parse_steps(&r.get::<_, Option<String>>(10).ok().flatten().unwrap_or_default()),
```
SELECT 末尾列追加 `, amas_grayscale_steps`。在文件顶部 helper 段加解析函数：
```rust
fn parse_steps(s: &str) -> [u32; 3] {
    let mut it = s.split(',').filter_map(|p| p.trim().parse::<u32>().ok());
    let a = it.next().unwrap_or(20);
    let b = it.next().unwrap_or(60);
    let c = it.next().unwrap_or(100);
    [a, b, c]
}
```

- [ ] `save_system_settings`：INSERT 列 + VALUES `?11` + ON CONFLICT + params 加：
```rust
                settings.llm_advisor_enabled as i64,
                format!("{},{},{}", settings.amas_grayscale_steps[0], settings.amas_grayscale_steps[1], settings.amas_grayscale_steps[2]),
```
列名追加 `, amas_grayscale_steps`，VALUES 追加 `, ?11`，ON CONFLICT 追加 `, amas_grayscale_steps=?11`。

- [ ] 更新 N+1 已改的两处测试字面量/raw INSERT，补 `amas_grayscale_steps`。`save_and_get_roundtrip` 的字面量加 `amas_grayscale_steps: [10, 50, 100],` 并加断言 `assert_eq!(got.amas_grayscale_steps, [10, 50, 100]);`；`get_replaces_null_wordbook_center_url_with_default` 的 raw INSERT 列加 `, amas_grayscale_steps`、VALUES 末尾加 `, '20,60,100'`。

- [ ] 在 `src/store/migrate.rs` system_settings ADD COLUMN 守卫列表加：
```rust
        ("amas_grayscale_steps", "TEXT NOT NULL DEFAULT '20,60,100'"),
```

- [ ] 跑确认 store 单测通过：
```bash
cargo test --lib system_settings:: 2>&1 | tail -20
```
预期：`test result: ok.`，含 `grayscale_steps_default_and_roundtrip`。

- [ ] commit：
```bash
git add src/store/migrate.rs src/store/operations/system_settings.rs
git commit -m "feat(amas-advisor): system_settings 加 amas_grayscale_steps 灰度策略列（C3）"
```

- [ ] 在 `tests/admin_amas_http.rs` 末尾加 C3 失败集成测试：
```rust
#[tokio::test]
async fn it_advisor_config_get_put() {
    let app = spawn_test_server().await;
    let admin_token = common::auth::setup_admin_and_get_token(&app.app).await;

    let get = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/advisor/config",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (get_status, _, get_body) = response_json(get).await;
    assert_eq!(get_status, StatusCode::OK);
    let cfg = &get_body["data"];
    assert!(cfg["model"].is_string());
    assert!(cfg["pollCron"].is_string());
    // API Key env-only 脱敏：test config api_key 为空 → 尾号空串
    assert!(cfg["apiKeyTail"].is_string());
    assert!(cfg["monthCapYuan"].is_number());
    assert_eq!(cfg["autoApplyEnabled"], false);
    assert!(cfg["grayscaleSteps"].is_array());
    assert_eq!(cfg["grayscaleSteps"].as_array().unwrap().len(), 3);
    assert_eq!(cfg["advisorEnabled"], false);

    // PUT 仅更新可写字段
    let put = request(
        &app.app,
        Method::PUT,
        "/api/admin/amas/advisor/config",
        Some(serde_json::json!({
            "monthCapYuan": 250.0,
            "autoApplyEnabled": true,
            "autoApplyMaxPerDay": 5,
            "autoApplyMinConfidence": 0.9,
            "grayscaleSteps": [10, 50, 100],
            "advisorEnabled": true
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (put_status, _, put_body) = response_json(put).await;
    assert_eq!(put_status, StatusCode::OK);
    let updated = &put_body["data"];
    assert!((updated["monthCapYuan"].as_f64().unwrap() - 250.0).abs() < 1e-9);
    assert_eq!(updated["autoApplyEnabled"], true);
    assert_eq!(updated["autoApplyMaxPerDay"], 5);
    assert!((updated["autoApplyMinConfidence"].as_f64().unwrap() - 0.9).abs() < 1e-9);
    assert_eq!(updated["grayscaleSteps"][0], 10);
    assert_eq!(updated["advisorEnabled"], true);

    // 持久化验证：再 GET 一次
    let get2 = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/advisor/config",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (_, _, get2_body) = response_json(get2).await;
    assert_eq!(get2_body["data"]["advisorEnabled"], true);
    assert_eq!(get2_body["data"]["grayscaleSteps"][1], 50);
}
```

- [ ] 跑确认失败（404）：
```bash
cargo test --test admin_amas_http it_advisor_config_get_put 2>&1 | tail -20
```
预期：`assertion ... left: 404, right: 200`。

- [ ] 在 `admin_router()` 加路由（C2 之后）：
```rust
        // C3: 顾问配置
        .route("/advisor/config", get(get_advisor_config).put(update_advisor_config))
```

- [ ] 在 `src/routes/admin/amas.rs` 文件末尾追加 C3 结构 + handler。`pollCron` 取 worker 注册的 cron 常量（advisor 每 20 分钟）；`apiKeyTail` 取 `state.config().llm.api_key` 末 4 位（空则空串）：
```rust
// ─────────── C3: 顾问配置 ───────────

/// advisor 巡查 cron（与 workers/mod.rs LlmAdvisor 注册一致，每 20 分钟）。
const ADVISOR_POLL_CRON: &str = "0 */20 * * * *";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AdvisorConfig {
    model: String,
    poll_cron: String,
    api_key_tail: String,
    month_cap_yuan: f64,
    auto_apply_enabled: bool,
    auto_apply_max_per_day: i64,
    auto_apply_min_confidence: f64,
    grayscale_steps: [u32; 3],
    advisor_enabled: bool,
}

fn build_advisor_config(
    llm: &crate::config::LLMConfig,
    settings: &crate::store::operations::system_settings::SystemSettings,
) -> AdvisorConfig {
    let tail = if llm.api_key.len() >= 4 {
        llm.api_key[llm.api_key.len() - 4..].to_string()
    } else {
        String::new()
    };
    AdvisorConfig {
        model: llm.model.clone(),
        poll_cron: ADVISOR_POLL_CRON.to_string(),
        api_key_tail: tail,
        month_cap_yuan: settings.llm_advisor_max_cost_per_month_yuan,
        auto_apply_enabled: settings.amas_auto_apply_enabled,
        auto_apply_max_per_day: settings.amas_auto_apply_max_per_day as i64,
        auto_apply_min_confidence: settings.amas_auto_apply_min_confidence,
        grayscale_steps: settings.amas_grayscale_steps,
        advisor_enabled: settings.llm_advisor_enabled,
    }
}

async fn get_advisor_config(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state
        .run_store_task("admin.amas.advisor_config.get", |store| {
            store.get_system_settings()
        })
        .await??;
    let llm = state.config().llm.clone();
    Ok(ok(build_advisor_config(&llm, &settings)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateAdvisorConfigBody {
    month_cap_yuan: Option<f64>,
    auto_apply_enabled: Option<bool>,
    auto_apply_max_per_day: Option<i64>,
    auto_apply_min_confidence: Option<f64>,
    grayscale_steps: Option<[u32; 3]>,
    advisor_enabled: Option<bool>,
}

async fn update_advisor_config(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(body): JsonBody<UpdateAdvisorConfigBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 校验灰度档位单调递增且末档 = 100
    if let Some(steps) = body.grayscale_steps {
        if !(steps[0] < steps[1] && steps[1] <= steps[2] && steps[2] == 100 && steps[0] >= 1) {
            return Err(AppError::bad_request(
                "INVALID_GRAYSCALE",
                "灰度档位需满足 1 ≤ s0 < s1 ≤ s2 且 s2 = 100",
            ));
        }
    }
    if let Some(c) = body.auto_apply_min_confidence {
        if !(0.0..=1.0).contains(&c) {
            return Err(AppError::bad_request(
                "INVALID_CONFIDENCE",
                "min_confidence 需在 0..=1",
            ));
        }
    }

    let settings = state
        .run_store_task("admin.amas.advisor_config.put", move |store| {
            let mut s = store.get_system_settings()?;
            if let Some(v) = body.month_cap_yuan {
                s.llm_advisor_max_cost_per_month_yuan = v.max(0.0);
            }
            if let Some(v) = body.auto_apply_enabled {
                s.amas_auto_apply_enabled = v;
            }
            if let Some(v) = body.auto_apply_max_per_day {
                s.amas_auto_apply_max_per_day = v.clamp(0, 100) as u32;
            }
            if let Some(v) = body.auto_apply_min_confidence {
                s.amas_auto_apply_min_confidence = v;
            }
            if let Some(v) = body.grayscale_steps {
                s.amas_grayscale_steps = v;
            }
            if let Some(v) = body.advisor_enabled {
                s.llm_advisor_enabled = v;
            }
            store.save_system_settings(&s)?;
            Ok::<_, crate::store::StoreError>(s)
        })
        .await??;

    let llm = state.config().llm.clone();
    Ok(ok(build_advisor_config(&llm, &settings)))
}
```

- [ ] 跑确认集成测试通过：
```bash
cargo test --test admin_amas_http it_advisor_config_get_put 2>&1 | tail -20
```
预期：`test it_advisor_config_get_put ... ok`。

- [ ] commit：
```bash
git add src/routes/admin/amas.rs tests/admin_amas_http.rs
git commit -m "feat(amas-advisor): C3 端点 GET/PUT /advisor/config（可写 settings + LLMConfig 只读脱敏）"
```

---

### Task B4: 模块 B 集成收尾 — 全量编译 + clippy + 完整集成测试回归

**Files:**
- 无新增；仅验证 + 必要的 import/lint 修复

- [ ] 全 crate 编译（非 --quiet，确保 Cargo.lock 同步、无 unused import）：
```bash
cargo check --tests 2>&1 | tail -30
```
预期：`Finished`，无 `error` / 无 `warning: unused import`。若 `Datelike`/`Query`/`post` 等 import 缺失或多余，据报错增删。

- [ ] clippy 零告警（项目惯例）：
```bash
cargo clippy --tests 2>&1 | tail -30
```
预期：无 `warning`。常见点：`approx_constant`（避免 3.14 类常量，测试金额已规避）、`needless_range_loop`（grayscale 用索引 OK）。

- [ ] 跑模块 B 三组集成测试 + AMAS 既有回归一次过：
```bash
cargo test --test admin_amas_http 2>&1 | tail -20
```
预期：全部 `ok`，含 `it_advisor_cost_endpoints`、`it_advisor_run_and_approve_all`、`it_advisor_config_get_put`、`it_amas_user_and_admin_endpoints`、`it_admin_auth_and_management_routes`。

- [ ] 跑受影响 store 单测整体回归：
```bash
cargo test --lib system_settings:: amas_suggestions:: 2>&1 | tail -20
```
预期：全部 `ok.`。

- [ ] commit（若收尾有 import/lint 修复）：
```bash
git add -A
git commit -m "chore(amas-advisor): 模块 B 后端 C1/C2/C3 编译 + clippy + 集成测试回归收尾"
```

---

模块 B 实现说明（供编排校对，非任务步骤）：
- `approve_one` 设为 `pub(crate)`，模块 D（C6 canary 的 approve→进灰度）可复用同一白名单校验通路，避免逻辑漂移。
- C2 `advisor_run` 依赖 `llm_advisor::run` 现签名 `run(&Store, Option<&LLMConfig>, &AMASEngine, Option<&AppState>)`（见 `src/workers/mod.rs:361`），test config `llm.enabled=false` → `produced=false`，无需 mock LLM。
- `AppError::message()` 取数器待 N+1 步骤核验 `src/response.rs` 实际签名（字段 vs getter），approve-all 的 error 文案据实调整。
- 契约里 C2 还提到 worker 注册条件改 `env || system_settings.llm_advisor_enabled` —— 该改动属 worker 注册层（`src/workers/mod.rs`），与模块 C（canary_monitor worker）同域，本模块 B 仅落 `set_llm_advisor_enabled` store 能力与 C3 配置读写，注册条件接线交由 worker 模块统一处理，避免双模块改同一文件冲突。

## 模块 C — 后端 C4/C5（白名单 CRUD + 历史 offset/q/CSV/回滚）

### Task C1: 迁移 `amas_tuning_whitelist` 表 + seed（store 支撑 C4 白名单 CRUD）

**Files:**
- Modify `src/store/migrate.rs`（在 `migrations()` 列表追加注册行，约 59 行后；新增 `m025_amas_tuning_whitelist` + down 函数，文件末尾追加）
- Create `src/store/operations/amas_tuning_whitelist.rs`
- Modify `src/store/operations/mod.rs`（约 1-12 行 `pub mod` 区，按字母序插入）

- [ ] 在 `src/store/operations/mod.rs` 的 `pub mod` 区，紧邻 `pub mod amas_telemetry;`（第 5 行）之后插入一行注册新模块：

```rust
pub mod amas_tuning_whitelist;
```

- [ ] 在 `src/store/operations/amas_tuning_whitelist.rs` 写**失败测试**（先建文件，仅 struct 占位 + `#[cfg(test)]`，让 store 方法缺失编译失败）。完整文件首版：

```rust
//! AMAS LLM 调参白名单的持久化存储（C4）。
//!
//! 取代纯 const `TIER_A_WHITELIST`：启动 seed 自 const，运行期可经 admin 端
//! 增删。`tuning_whitelist::validate_patch` / `build_system_prompt` 改为从此表读，
//! const 仅作 seed 源 + 空表 fallback。

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::amas::tuning_whitelist::TIER_A_WHITELIST;
use crate::store::{Store, StoreError};

/// 一条白名单：path + 安全区间。camelCase 序列化 path/minSafe/maxSafe。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WhitelistRow {
    pub path: String,
    pub min_safe: f64,
    pub max_safe: f64,
}

impl Store {
    /// 列出全部白名单条目，按 path 升序。
    pub fn list_tuning_whitelist(&self) -> Result<Vec<WhitelistRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT path, min_safe, max_safe FROM amas_tuning_whitelist ORDER BY path ASC",
        )?;
        let rows: Result<Vec<_>, _> = stmt
            .query_map([], |r| {
                Ok(WhitelistRow {
                    path: r.get::<_, String>(0)?,
                    min_safe: r.get::<_, f64>(1)?,
                    max_safe: r.get::<_, f64>(2)?,
                })
            })?
            .collect();
        Ok(rows?)
    }

    /// 插入或覆盖一条白名单（path 为主键，重复则 upsert 区间）。
    pub fn insert_tuning_whitelist(
        &self,
        path: &str,
        min_safe: f64,
        max_safe: f64,
        created_by: &str,
    ) -> Result<WhitelistRow, StoreError> {
        if min_safe > max_safe {
            return Err(StoreError::Validation(format!(
                "min_safe {min_safe} > max_safe {max_safe}"
            )));
        }
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO amas_tuning_whitelist (path, min_safe, max_safe, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET min_safe=?2, max_safe=?3",
            params![path, min_safe, max_safe, now, created_by],
        )?;
        Ok(WhitelistRow {
            path: path.to_string(),
            min_safe,
            max_safe,
        })
    }

    /// 删除一条白名单；返回是否真的删掉一行。
    pub fn delete_tuning_whitelist(&self, path: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "DELETE FROM amas_tuning_whitelist WHERE path = ?1",
            params![path],
        )?;
        Ok(affected > 0)
    }

    /// 启动 seed：仅当表为空时把 const `TIER_A_WHITELIST` 全量写入，幂等。
    pub fn seed_tuning_whitelist_if_empty(&self) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM amas_tuning_whitelist", [], |r| r.get(0))?;
        if count > 0 {
            return Ok(0);
        }
        let now = chrono::Utc::now().to_rfc3339();
        let mut seeded = 0usize;
        for e in TIER_A_WHITELIST {
            conn.execute(
                "INSERT OR IGNORE INTO amas_tuning_whitelist
                    (path, min_safe, max_safe, created_at, created_by)
                 VALUES (?1, ?2, ?3, ?4, 'seed')",
                params![e.path, e.min_safe, e.max_safe, now],
            )?;
            seeded += 1;
        }
        Ok(seeded)
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
    fn seed_populates_11_then_idempotent() {
        let s = store();
        let n = s.seed_tuning_whitelist_if_empty().unwrap();
        assert_eq!(n, 11);
        // 二次 seed 不重复写
        let again = s.seed_tuning_whitelist_if_empty().unwrap();
        assert_eq!(again, 0);
        assert_eq!(s.list_tuning_whitelist().unwrap().len(), 11);
    }

    #[test]
    fn insert_upsert_and_delete() {
        let s = store();
        s.seed_tuning_whitelist_if_empty().unwrap();
        let row = s
            .insert_tuning_whitelist("memoryModel.w[5]", 0.1, 2.0, "admin-1")
            .unwrap();
        assert_eq!(row.path, "memoryModel.w[5]");
        assert_eq!(s.list_tuning_whitelist().unwrap().len(), 12);
        // upsert 同 path 改区间，不新增行
        s.insert_tuning_whitelist("memoryModel.w[5]", 0.2, 3.0, "admin-1")
            .unwrap();
        assert_eq!(s.list_tuning_whitelist().unwrap().len(), 12);
        let got = s
            .list_tuning_whitelist()
            .unwrap()
            .into_iter()
            .find(|r| r.path == "memoryModel.w[5]")
            .unwrap();
        assert!((got.max_safe - 3.0).abs() < 1e-9);
        // 删除
        assert!(s.delete_tuning_whitelist("memoryModel.w[5]").unwrap());
        assert!(!s.delete_tuning_whitelist("memoryModel.w[5]").unwrap());
        assert_eq!(s.list_tuning_whitelist().unwrap().len(), 11);
    }

    #[test]
    fn insert_rejects_inverted_range() {
        let s = store();
        let err = s
            .insert_tuning_whitelist("memoryModel.w[0]", 5.0, 1.0, "a")
            .unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));
    }
}
```

- [ ] 跑确认失败（表尚未迁移，`seed`/`list` 在缺表上报错，`run_migrations` 无对应迁移）：

```
cargo test -p learning-backend --lib store::operations::amas_tuning_whitelist
```
预期：编译通过但测试 panic，错误形如 `no such table: amas_tuning_whitelist`（`run_migrations` 尚未建表）。

- [ ] 在 `src/store/migrate.rs` 的 `migrations()` 列表（约第 59 行 `("024_client_extras", m024_client_extras),` 之后）追加注册：

```rust
        ("025_amas_tuning_whitelist", m025_amas_tuning_whitelist),
```

- [ ] 在 `src/store/migrate.rs` 的 down 列表（约第 99 行 `("024_client_extras", m024_client_extras_down),` 之后）追加：

```rust
        ("025_amas_tuning_whitelist", m025_amas_tuning_whitelist_down),
```

- [ ] 在 `src/store/migrate.rs` 文件末尾追加迁移函数实现（幂等 `CREATE TABLE IF NOT EXISTS`）：

```rust
/// m025：AMAS 调参白名单表（C4）。const `TIER_A_WHITELIST` 改为 seed 源，运行期可增删。
/// seed 由 `seed_tuning_whitelist_if_empty()` 在启动时执行（非迁移内，避免与 const 漂移）。
fn m025_amas_tuning_whitelist(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS amas_tuning_whitelist (
            path        TEXT PRIMARY KEY,
            min_safe    REAL NOT NULL,
            max_safe    REAL NOT NULL,
            created_at  TEXT NOT NULL,
            created_by  TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// m025 down：DROP 白名单表。
fn m025_amas_tuning_whitelist_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch("DROP TABLE IF EXISTS amas_tuning_whitelist;")?;
    Ok(())
}
```

- [ ] 跑确认通过：

```
cargo test -p learning-backend --lib store::operations::amas_tuning_whitelist
```
预期：`test result: ok. 3 passed`。

- [ ] commit：

```
git add src/store/operations/amas_tuning_whitelist.rs src/store/operations/mod.rs src/store/migrate.rs
git commit -m "feat(amas-advisor): 新增 amas_tuning_whitelist 表 + CRUD/seed store 方法（C4）"
```

---

### Task C2: 启动时 seed 白名单（在迁移后挂钩）

**Files:**
- Modify `src/store/migrate.rs` 调用方 / 启动初始化处（先定位 `run_migrations()` 的调用点）

- [ ] 定位启动时调用 `run_migrations` 的位置（main/启动装配），确认 seed 挂钩点：

```
grep -rn "run_migrations\|seed_tuning_whitelist\|seed_" src/main.rs src/lib.rs src/state.rs src/app.rs 2>/dev/null
```
预期：找到一处 `store.run_migrations()` 调用（启动装配），其后即 seed 挂钩点。

- [ ] 在该 `run_migrations()` 成功之后，紧跟一行 seed 调用（失败仅 warn 不阻断启动）。把下面这段插入到 `run_migrations()?;`（或对应 `.expect(...)`）之后：

```rust
    if let Err(e) = store.seed_tuning_whitelist_if_empty() {
        tracing::warn!(error = %e, "启动 seed AMAS 调参白名单失败，回退 const fallback");
    }
```

- [ ] 跑全量确认未破坏启动路径（库测全绿）：

```
cargo test -p learning-backend --lib
```
预期：全部通过（无新增失败）。

- [ ] commit：

```
git add -A
git commit -m "feat(amas-advisor): 启动后 seed 调参白名单（空表才写，失败仅 warn）"
```

---

### Task C3: `tuning_whitelist::validate_patch` / `find` 改为 store 读、const fallback

**Files:**
- Modify `src/amas/tuning_whitelist.rs`（第 34-59 行 `find` + `validate_patch`，以及 `#[cfg(test)]` 块 61-96）
- Modify `src/routes/admin/amas.rs`（第 593、616 行 `validate_patch` 调用点）
- Modify `src/workers/llm_advisor.rs`（第 11、155 行 `validate_patch` 调用点 + 第 30-40 行 `build_system_prompt`）

- [ ] 在 `src/amas/tuning_whitelist.rs` 的 `#[cfg(test)]` 块内新增**失败测试**（验证 store 驱动版仍正确拒绝越界/非白名单；调用尚不存在的 `validate_patch(&store, ...)`）。把下面三个测试追加到 `mod tests` 内（紧接现有 `validate_rejects_out_of_range` 之后）：

```rust
    fn seeded_store() -> crate::store::Store {
        let s = crate::store::Store::open(":memory:", 5000, 1).unwrap();
        s.run_migrations().unwrap();
        s.seed_tuning_whitelist_if_empty().unwrap();
        s
    }

    #[test]
    fn store_backed_accepts_in_range() {
        let store = seeded_store();
        let patch = json!({ "memoryModel.baseDesiredRetention": 0.85 });
        let errs = validate_patch(&store, patch.as_object().unwrap());
        assert!(errs.is_empty(), "errors: {:?}", errs);
    }

    #[test]
    fn store_backed_rejects_unknown_and_out_of_range() {
        let store = seeded_store();
        let unknown = json!({ "ensemble.baseWeightHeuristic": 0.5 });
        assert_eq!(validate_patch(&store, unknown.as_object().unwrap()).len(), 1);
        let oob = json!({ "memoryModel.baseDesiredRetention": 0.5 });
        let e = validate_patch(&store, oob.as_object().unwrap());
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("越界"));
    }

    #[test]
    fn store_backed_honors_runtime_added_path() {
        let store = seeded_store();
        // 运行期新增一条白名单 → 之前越界的 path 现在合法
        store
            .insert_tuning_whitelist("memoryModel.w[5]", 0.0, 10.0, "admin")
            .unwrap();
        let patch = json!({ "memoryModel.w[5]": 5.0 });
        assert!(validate_patch(&store, patch.as_object().unwrap()).is_empty());
    }
```

- [ ] 改现有 4 个测试以匹配新签名（`whitelist_size_is_11` 保持不变；`validate_accepts_in_range_known_paths` / `validate_rejects_unknown_path` / `validate_rejects_out_of_range` 改用 store fallback —— 不 seed 时 const fallback 生效）。把这三个测试体替换为：

```rust
    #[test]
    fn validate_accepts_in_range_known_paths() {
        // 不 seed → const fallback 路径
        let store = crate::store::Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        let patch = json!({
            "memoryModel.baseDesiredRetention": 0.85,
            "memoryModel.w[2]": 3.0,
        });
        let errs = validate_patch(&store, patch.as_object().unwrap());
        assert!(errs.is_empty(), "errors: {:?}", errs);
    }

    #[test]
    fn validate_rejects_unknown_path() {
        let store = crate::store::Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        let patch = json!({"ensemble.baseWeightHeuristic": 0.5});
        let errs = validate_patch(&store, patch.as_object().unwrap());
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("白名单"));
    }

    #[test]
    fn validate_rejects_out_of_range() {
        let store = crate::store::Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        let patch = json!({"memoryModel.baseDesiredRetention": 0.5});
        let errs = validate_patch(&store, patch.as_object().unwrap());
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("越界"));
    }
```

- [ ] 跑确认失败（`validate_patch` 仍是旧单参签名，新调用编译错误）：

```
cargo test -p learning-backend --lib amas::tuning_whitelist 2>&1 | head -30
```
预期：`error[E0061]: this function takes 1 argument but 2 arguments were supplied`。

- [ ] 改 `src/amas/tuning_whitelist.rs` 的 `find` + `validate_patch`（第 34-59 行）为 store 驱动、const fallback。把这两个函数整体替换为：

```rust
/// 运行期可解析的白名单条目（owned，来自 store row 或 const）。
struct ResolvedEntry {
    min_safe: f64,
    max_safe: f64,
}

/// 从 store 读取白名单为 path→区间 map；表为空或出错时回退 const `TIER_A_WHITELIST`。
fn resolve_whitelist(store: &crate::store::Store) -> std::collections::HashMap<String, ResolvedEntry> {
    match store.list_tuning_whitelist() {
        Ok(rows) if !rows.is_empty() => rows
            .into_iter()
            .map(|r| {
                (
                    r.path,
                    ResolvedEntry {
                        min_safe: r.min_safe,
                        max_safe: r.max_safe,
                    },
                )
            })
            .collect(),
        Ok(_) => const_fallback(),
        Err(e) => {
            tracing::warn!(error = %e, "读取白名单表失败，回退 const TIER_A_WHITELIST");
            const_fallback()
        }
    }
}

fn const_fallback() -> std::collections::HashMap<String, ResolvedEntry> {
    TIER_A_WHITELIST
        .iter()
        .map(|e| {
            (
                e.path.to_string(),
                ResolvedEntry {
                    min_safe: e.min_safe,
                    max_safe: e.max_safe,
                },
            )
        })
        .collect()
}

/// const-only 查找（保留供无 store 上下文场景，如 build_system_prompt 的纯 const 渲染）。
pub fn find(path: &str) -> Option<&'static WhitelistEntry> {
    TIER_A_WHITELIST.iter().find(|e| e.path == path)
}

/// 校验 patch 中所有 path / value 都通过白名单 + 范围检查（store 驱动，const fallback）。
/// 返回错误描述 vec；为空表示通过。
pub fn validate_patch(
    store: &crate::store::Store,
    patch: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    let whitelist = resolve_whitelist(store);
    let mut errors = Vec::new();
    for (path, value) in patch {
        let Some(entry) = whitelist.get(path.as_str()) else {
            errors.push(format!("path 不在白名单：{path}"));
            continue;
        };
        let Some(v) = value.as_f64() else {
            errors.push(format!("path={path} 值非数字"));
            continue;
        };
        if v < entry.min_safe || v > entry.max_safe {
            errors.push(format!(
                "path={path} 值 {v} 越界（安全区间 [{}, {}]）",
                entry.min_safe, entry.max_safe
            ));
        }
    }
    errors
}
```

- [ ] 改 `src/routes/admin/amas.rs` `approve_suggestion` 调用点（第 616 行）—— 把 `validate_patch` 移入 store_task（需 `&Store`）。把第 610-619 行（patch 校验段）替换为：

```rust
    // 校验 patch（防止数据库篡改）—— 白名单从 store 读
    let patch_obj = suggestion
        .patch_json
        .as_object()
        .ok_or_else(|| AppError::internal("patch_json 非对象"))?
        .clone();
    let patch_for_validate = patch_obj.clone();
    let errs = state
        .run_store_task("admin.amas.approve_validate", move |store| {
            Ok::<_, crate::store::StoreError>(validate_patch(store, &patch_for_validate))
        })
        .await??;
    if !errs.is_empty() {
        return Err(AppError::bad_request("PATCH_INVALID", &errs.join("；")));
    }
```

- [ ] 改 `src/workers/llm_advisor.rs`：导入不变（第 11 行 `validate_patch` 仍用），把第 155 行调用改为传 `store`。把 `let validation_errors = validate_patch(&patch_obj);` 改为：

```rust
    let validation_errors = validate_patch(store, &patch_obj);
```

- [ ] 改 `src/workers/llm_advisor.rs` 的 `build_system_prompt`（第 30-40 行）同步从 store 读，const fallback。新签名加 `store: &Store`，把整个函数替换为：

```rust
fn build_system_prompt(store: &Store) -> String {
    let mut s = String::from(SYSTEM_PROMPT);
    match store.list_tuning_whitelist() {
        Ok(rows) if !rows.is_empty() => {
            for r in &rows {
                s.push_str(&format!("- {} ∈ [{}, {}]\n", r.path, r.min_safe, r.max_safe));
            }
        }
        _ => {
            for entry in TIER_A_WHITELIST {
                s.push_str(&format!(
                    "- {} ∈ [{}, {}]\n",
                    entry.path, entry.min_safe, entry.max_safe
                ));
            }
        }
    }
    s.push_str("\n要求：patch 至多包含 3 个参数；优先调整与 evidence 关联最强的字段；不确定时输出 {\"patch\":{}}。");
    s
}
```

- [ ] 改 `src/workers/llm_advisor.rs` 中 `build_system_prompt()` 的调用点（第 103 行）传 store：

```rust
            ChatMessage { role: "system".into(), content: build_system_prompt(store) },
```

- [ ] 跑确认通过（白名单单测 + worker/route 编译）：

```
cargo test -p learning-backend --lib amas::tuning_whitelist
```
预期：`test result: ok.`（6 passed：原 4 + 新 3 减去合并 = 实际计数以输出为准，全绿即可）。

- [ ] 跑全库确认 worker/route 调用点编译通过：

```
cargo test -p learning-backend --lib 2>&1 | tail -15
```
预期：无 `E0061`/`E0425` 编译错误，库测全绿。

- [ ] commit：

```
git add src/amas/tuning_whitelist.rs src/routes/admin/amas.rs src/workers/llm_advisor.rs
git commit -m "refactor(amas-advisor): validate_patch/build_system_prompt 改从白名单表读，const 作 fallback（C4）"
```

---

### Task C4: C4 端点 GET/POST/DELETE `/advisor/whitelist`

**Files:**
- Modify `src/routes/admin/amas.rs`（`admin_router()` 第 54-59 行区追加 3 条路由；文件 suggestions handler 区追加 3 个 handler + 1 个 request struct）

- [ ] 在 `tests/admin_amas_http.rs` 新增**集成测试**覆盖白名单 GET/POST/DELETE happy path + 边界（非法 path / 越界区间 / 删不存在）。在文件末尾追加：

```rust
async fn setup_amas_admin_token(app: &common::app::TestApp) -> String {
    let admin_email = format!("wl-admin-{}@test.com", uuid::Uuid::new_v4());
    let setup = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({ "email": admin_email, "password": "AdminPassw0rd!" })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(setup).await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["token"].as_str().expect("admin token").to_string()
}

#[tokio::test]
async fn it_amas_advisor_whitelist_crud() {
    let app = spawn_test_server().await;
    let token = setup_amas_admin_token(&app).await;

    // GET → seed 后应有 11 条
    let list = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/advisor/whitelist",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s, _, body) = response_json(list).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["data"].as_array().unwrap().len(), 11);

    // POST 新增一条合法 path
    let add = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/advisor/whitelist",
        Some(serde_json::json!({ "path": "memoryModel.w[5]", "minSafe": 0.1, "maxSafe": 2.0 })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s, _, body) = response_json(add).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["data"]["path"], "memoryModel.w[5]");
    assert!((body["data"]["minSafe"].as_f64().unwrap() - 0.1).abs() < 1e-9);

    // POST 越界区间 → 400
    let bad_range = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/advisor/whitelist",
        Some(serde_json::json!({ "path": "memoryModel.w[6]", "minSafe": 5.0, "maxSafe": 1.0 })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s, _, _) = response_json(bad_range).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // POST 非 memoryModel.* path → 400
    let bad_path = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/advisor/whitelist",
        Some(serde_json::json!({ "path": "ensemble.foo", "minSafe": 0.0, "maxSafe": 1.0 })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s, _, _) = response_json(bad_path).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // DELETE 存在 → deleted:true
    let del = request(
        &app.app,
        Method::DELETE,
        "/api/admin/amas/advisor/whitelist/memoryModel.w[5]",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s, _, body) = response_json(del).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["data"]["deleted"], true);

    // DELETE 不存在 → deleted:false
    let del2 = request(
        &app.app,
        Method::DELETE,
        "/api/admin/amas/advisor/whitelist/memoryModel.w[5]",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s, _, body) = response_json(del2).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["data"]["deleted"], false);
}
```

- [ ] 跑确认失败（路由未挂，404）：

```
cargo test -p learning-backend --test admin_amas_http it_amas_advisor_whitelist_crud
```
预期：断言 `assert_eq!(s, StatusCode::OK)` 失败，实际 `404 Not Found`。

- [ ] 在 `src/routes/admin/amas.rs` 的 `admin_router()`（第 59 行 `.route("/suggestions/:id/reject", ...)` 之后）追加 3 条路由：

```rust
        .route(
            "/advisor/whitelist",
            get(list_whitelist).post(add_whitelist),
        )
        .route("/advisor/whitelist/:path", axum::routing::delete(delete_whitelist))
```

- [ ] 在 `src/routes/admin/amas.rs` 文件 suggestions handler 区（`suggestion_spend` 之后、`write_path` 之前）追加 3 个 handler + request struct。`path` 合法性校验复用 const `TIER_A_WHITELIST` 前缀约定（仅允许 `memoryModel.*`，与设计 spec §2 一致）：

```rust
// ─────────── C4: 调参白名单 CRUD ───────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddWhitelistBody {
    path: String,
    min_safe: f64,
    max_safe: f64,
}

async fn list_whitelist(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let rows = state
        .run_store_task("admin.amas.list_whitelist", |store| {
            store.list_tuning_whitelist()
        })
        .await??;
    Ok(ok(rows))
}

async fn add_whitelist(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(body): JsonBody<AddWhitelistBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // path 必须是 memoryModel.* 命名空间（与 Tier-A 白名单语义一致，拒绝越界命名空间）
    if !body.path.starts_with("memoryModel.") {
        return Err(AppError::bad_request(
            "INVALID_PATH",
            "白名单 path 必须以 memoryModel. 开头",
        ));
    }
    if body.min_safe > body.max_safe {
        return Err(AppError::bad_request(
            "INVALID_RANGE",
            "minSafe 不得大于 maxSafe",
        ));
    }
    let admin_id = admin.admin_id.clone();
    let row = state
        .run_store_task("admin.amas.add_whitelist", move |store| {
            store.insert_tuning_whitelist(&body.path, body.min_safe, body.max_safe, &admin_id)
        })
        .await??;
    Ok(ok(row))
}

async fn delete_whitelist(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let deleted = state
        .run_store_task("admin.amas.delete_whitelist", move |store| {
            store.delete_tuning_whitelist(&path)
        })
        .await??;
    Ok(ok(serde_json::json!({ "deleted": deleted })))
}
```

- [ ] 跑确认通过：

```
cargo test -p learning-backend --test admin_amas_http it_amas_advisor_whitelist_crud
```
预期：`test result: ok. 1 passed`。

- [ ] commit：

```
git add src/routes/admin/amas.rs tests/admin_amas_http.rs
git commit -m "feat(amas-advisor): C4 端点 GET/POST/DELETE /advisor/whitelist（path memoryModel.* 校验 + 区间校验）"
```

---

### Task C5: store 层 —— `list_amas_suggestions` 加 offset + q + CSV/回滚数据支撑

**Files:**
- Modify `src/store/operations/amas_suggestions.rs`（第 208-237 行 `list_amas_suggestions`；`#[cfg(test)]` 块第 298-397 行）

- [ ] 在 `src/store/operations/amas_suggestions.rs` 的 `mod tests` 内新增**失败测试**（offset 分页 + q 模糊匹配 rationale/path）。把下面追加到 `mod tests` 末尾（在 `spend_today_aggregates` 之后）：

```rust
    fn ins_with(rationale: &str, patch_json: &str) -> InsertSuggestion {
        let mut s = ins(SuggestionStatus::Pending);
        s.rationale = rationale.to_string();
        s.patch_json = patch_json.to_string();
        s
    }

    #[test]
    fn list_supports_offset_pagination() {
        let store = fresh_store();
        for i in 0..5 {
            store
                .insert_amas_suggestion(&ins_with(&format!("r{i}"), r#"{"memoryModel.w[0]":1.0}"#))
                .unwrap();
        }
        // limit=2 offset=0 → 2 条；offset=4 → 1 条；offset=10 → 0 条
        assert_eq!(store.list_amas_suggestions_paged(None, 2, 0, None).unwrap().len(), 2);
        assert_eq!(store.list_amas_suggestions_paged(None, 2, 4, None).unwrap().len(), 1);
        assert_eq!(store.list_amas_suggestions_paged(None, 2, 10, None).unwrap().len(), 0);
    }

    #[test]
    fn list_supports_keyword_filter() {
        let store = fresh_store();
        store
            .insert_amas_suggestion(&ins_with("提升留存", r#"{"memoryModel.baseDesiredRetention":0.85}"#))
            .unwrap();
        store
            .insert_amas_suggestion(&ins_with("降低疲劳", r#"{"memoryModel.w[2]":3.0}"#))
            .unwrap();
        // q 命中 rationale
        let by_rationale = store.list_amas_suggestions_paged(None, 50, 0, Some("留存")).unwrap();
        assert_eq!(by_rationale.len(), 1);
        // q 命中 patch_json 中的 path 关键字
        let by_path = store
            .list_amas_suggestions_paged(None, 50, 0, Some("baseDesiredRetention"))
            .unwrap();
        assert_eq!(by_path.len(), 1);
        // q 无命中
        assert_eq!(store.list_amas_suggestions_paged(None, 50, 0, Some("nomatch")).unwrap().len(), 0);
    }
```

- [ ] 跑确认失败（`list_amas_suggestions_paged` 不存在）：

```
cargo test -p learning-backend --lib store::operations::amas_suggestions
```
预期：`error[E0599]: no method named list_amas_suggestions_paged found`。

- [ ] 在 `src/store/operations/amas_suggestions.rs` 的 `impl Store` 内（紧接现有 `list_amas_suggestions` 之后，第 237 行后）新增分页 + 关键字版本。`q` 用 `LIKE` 同时匹配 `rationale` 与 `patch_json`：

```rust
    /// C5：分页 + 可选状态 + 可选关键字（模糊匹配 rationale / patch_json）。
    /// 现有 `list_amas_suggestions(status, limit)` 保留不动（offset=0、q=None 的特例）。
    pub fn list_amas_suggestions_paged(
        &self,
        status: Option<SuggestionStatus>,
        limit: usize,
        offset: usize,
        q: Option<&str>,
    ) -> Result<Vec<TuningSuggestionRow>, StoreError> {
        let limit = limit.min(500) as i64;
        let offset = offset as i64;
        let conn = self.conn()?;
        let mut where_clauses: Vec<String> = Vec::new();
        if status.is_some() {
            where_clauses.push("status = :status".to_string());
        }
        if q.is_some() {
            where_clauses.push("(rationale LIKE :q OR patch_json LIKE :q)".to_string());
        }
        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };
        let sql = format!(
            "SELECT {COLS} FROM amas_tuning_suggestions {where_sql}
             ORDER BY created_at DESC LIMIT :limit OFFSET :offset"
        );
        let like = q.map(|s| format!("%{s}%"));
        let mut stmt = conn.prepare(&sql)?;
        let mut named: Vec<(&str, &dyn rusqlite::ToSql)> = Vec::new();
        let status_str = status.map(|s| s.as_str());
        if let Some(ref st) = status_str {
            named.push((":status", st));
        }
        if let Some(ref l) = like {
            named.push((":q", l));
        }
        named.push((":limit", &limit));
        named.push((":offset", &offset));
        let raw: Result<Vec<_>, _> = stmt
            .query_map(named.as_slice(), row_to_suggestion)?
            .collect();
        raw?.into_iter().map(build).collect()
    }
```

- [ ] 跑确认通过：

```
cargo test -p learning-backend --lib store::operations::amas_suggestions
```
预期：`test result: ok.`（原 4 + 新 2 全绿）。

- [ ] commit：

```
git add src/store/operations/amas_suggestions.rs
git commit -m "feat(amas-advisor): list_amas_suggestions_paged 加 offset + 关键字模糊（C5 store 层）"
```

---

### Task C6: C5 端点 —— `GET /suggestions` 扩展 query（offset + q）

**Files:**
- Modify `src/routes/admin/amas.rs`（第 537-565 行 `ListSuggestionsQuery` + `list_suggestions`）

- [ ] 在 `tests/admin_amas_http.rs` 末尾新增**集成测试**覆盖 offset/q（先 seed 几条 suggestion）：

```rust
#[tokio::test]
async fn it_amas_suggestions_offset_and_q() {
    let app = spawn_test_server().await;
    let token = setup_amas_admin_token(&app).await;

    // 直接经 store 落 3 条 pending
    for (r, p) in [
        ("提升留存目标", r#"{"memoryModel.baseDesiredRetention":0.85}"#),
        ("降低疲劳阈值", r#"{"memoryModel.w[2]":3.0}"#),
        ("调整初始稳定性", r#"{"memoryModel.w[0]":1.0}"#),
    ] {
        app.state
            .store()
            .insert_amas_suggestion(
                &learning_backend::store::operations::amas_suggestions::InsertSuggestion {
                    based_on_version_hash: "h".into(),
                    patch_json: p.into(),
                    rationale: r.into(),
                    evidence_json: "{}".into(),
                    cost_usd: Some(0.01),
                    tokens_input: Some(10),
                    tokens_output: Some(5),
                    confidence: Some(0.7),
                    initial_status:
                        learning_backend::store::operations::amas_suggestions::SuggestionStatus::Pending,
                    decided_by: None,
                    decision_note: None,
                    base_values_json: None,
                },
            )
            .expect("insert suggestion");
    }

    // limit=1 offset=0 → 1 条
    let p1 = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions?limit=1&offset=0",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s, _, body) = response_json(p1).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // q=留存 → 命中 1 条
    let pq = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions?q=%E7%95%99%E5%AD%98",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s, _, body) = response_json(pq).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
}
```

- [ ] 跑确认失败（offset/q 未生效，offset 不分页 → limit=1 仍返回 1 条会偶过；q 未实现会返回 3 条）：

```
cargo test -p learning-backend --test admin_amas_http it_amas_suggestions_offset_and_q
```
预期：`q=留存` 断言失败，实际返回 3 条（`assert_eq! left:3 right:1`）。

- [ ] 改 `src/routes/admin/amas.rs` 的 `ListSuggestionsQuery`（第 537-542 行）加 `offset` + `q`：

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListSuggestionsQuery {
    status: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    q: Option<String>,
}
```

- [ ] 改 `list_suggestions` handler（第 544-565 行）走分页方法：

```rust
async fn list_suggestions(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<ListSuggestionsQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let offset = q.offset.unwrap_or(0);
    let status = if let Some(s) = q.status.as_deref() {
        Some(
            SuggestionStatus::parse(s)
                .map_err(|e| AppError::bad_request("BAD_STATUS", &e.to_string()))?,
        )
    } else {
        None
    };
    let keyword = q.q.clone();
    let rows = state
        .run_store_task("admin.amas.list_suggestions", move |store| {
            store.list_amas_suggestions_paged(status, limit, offset, keyword.as_deref())
        })
        .await??;
    Ok(ok(rows))
}
```

- [ ] 跑确认通过：

```
cargo test -p learning-backend --test admin_amas_http it_amas_suggestions_offset_and_q
```
预期：`test result: ok. 1 passed`。

- [ ] commit：

```
git add src/routes/admin/amas.rs tests/admin_amas_http.rs
git commit -m "feat(amas-advisor): GET /suggestions 扩展 offset + q 关键字（C5）"
```

---

### Task C7: C5 端点 —— `GET /suggestions/export.csv`

**Files:**
- Modify `src/routes/admin/amas.rs`（`admin_router()` 加路由；新增 `export_suggestions_csv` handler + 文件顶部 import）

- [ ] 在 `tests/admin_amas_http.rs` 末尾新增**集成测试**校验 CSV 头 + Content-Type + 行内容：

```rust
#[tokio::test]
async fn it_amas_suggestions_export_csv() {
    let app = spawn_test_server().await;
    let token = setup_amas_admin_token(&app).await;

    app.state
        .store()
        .insert_amas_suggestion(
            &learning_backend::store::operations::amas_suggestions::InsertSuggestion {
                based_on_version_hash: "vhash-csv".into(),
                patch_json: r#"{"memoryModel.w[0]":1.0}"#.into(),
                rationale: "csv 测试理由".into(),
                evidence_json: "{}".into(),
                cost_usd: Some(0.02),
                tokens_input: Some(10),
                tokens_output: Some(5),
                confidence: Some(0.7),
                initial_status:
                    learning_backend::store::operations::amas_suggestions::SuggestionStatus::Pending,
                decided_by: None,
                decision_note: None,
                base_values_json: None,
            },
        )
        .expect("insert suggestion");

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions/export.csv",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let status = resp.status();
    let ct = resp
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();

    assert_eq!(status, StatusCode::OK);
    assert!(ct.starts_with("text/csv"), "content-type: {ct}");
    let first_line = text.lines().next().unwrap();
    assert_eq!(
        first_line,
        "id,created_at,based_on_version_hash,patch,rationale,cost_usd,status,decided_by"
    );
    assert!(text.contains("vhash-csv"));
    assert!(text.contains("csv 测试理由"));
}
```

- [ ] 跑确认失败（路由未挂 → 404，content-type 断言失败）：

```
cargo test -p learning-backend --test admin_amas_http it_amas_suggestions_export_csv
```
预期：`assert!(ct.starts_with("text/csv"))` 失败（实际为空或 json，status 404）。

- [ ] 在 `src/routes/admin/amas.rs` 文件顶部 import 区（第 1-12 行）补充 csv 响应所需类型。把第 1-3 行替换为：

```rust
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
```

- [ ] 在 `admin_router()`（C4 白名单路由之后）追加 CSV 路由：

```rust
        .route("/suggestions/export.csv", get(export_suggestions_csv))
```

- [ ] 在 `src/routes/admin/amas.rs` 新增 `export_suggestions_csv` handler（复用 `ListSuggestionsQuery` 的过滤，导全集 limit=500）。CSV 单元格用双引号转义。把下面追加到 `list_suggestions` 之后：

```rust
// ─────────── C5: 历史导出 CSV ───────────

/// CSV 字段转义：含逗号/引号/换行的值用双引号包裹，内部引号翻倍。
fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

async fn export_suggestions_csv(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<ListSuggestionsQuery>,
) -> Result<Response, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;
    let status = if let Some(s) = q.status.as_deref() {
        Some(
            SuggestionStatus::parse(s)
                .map_err(|e| AppError::bad_request("BAD_STATUS", &e.to_string()))?,
        )
    } else {
        None
    };
    let keyword = q.q.clone();
    let rows = state
        .run_store_task("admin.amas.export_csv", move |store| {
            store.list_amas_suggestions_paged(status, 500, 0, keyword.as_deref())
        })
        .await??;

    let mut out =
        String::from("id,created_at,based_on_version_hash,patch,rationale,cost_usd,status,decided_by\n");
    for r in &rows {
        let patch = serde_json::to_string(&r.patch_json).unwrap_or_default();
        let cost = r.cost_usd.map(|c| c.to_string()).unwrap_or_default();
        let decided_by = r.decided_by.clone().unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            r.id,
            csv_cell(&r.created_at.to_rfc3339()),
            csv_cell(&r.based_on_version_hash),
            csv_cell(&patch),
            csv_cell(&r.rationale),
            csv_cell(&cost),
            r.status.as_str(),
            csv_cell(&decided_by),
        ));
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"amas-suggestions.csv\"",
        )
        .body(Body::from(out))
        .map_err(|e| AppError::internal(&e.to_string()))
}
```

- [ ] 跑确认通过：

```
cargo test -p learning-backend --test admin_amas_http it_amas_suggestions_export_csv
```
预期：`test result: ok. 1 passed`。

- [ ] commit：

```
git add src/routes/admin/amas.rs tests/admin_amas_http.rs
git commit -m "feat(amas-advisor): GET /suggestions/export.csv 当前过滤集导出（C5）"
```

---

### Task C8: C5 端点 —— `POST /suggestions/:id/rollback`（版本链 restore parent）

**Files:**
- Modify `src/routes/admin/amas.rs`（`admin_router()` 加路由；新增 `rollback_suggestion` handler）
- 依赖：`store.get_amas_config_version(hash)` 返回的版本含 `parent_hash`（已在 restore 通路使用）；`apply_and_persist_config`（第 378 行，`pub(crate)`）

- [ ] 先核对版本行字段名（`parent_hash` / `snapshot_json`）以贴合 restore 通路：

```
grep -n "parent_hash\|snapshot_json\|struct .*Version\|pub fn get_amas_config_version" src/store/operations/amas_versions.rs | head
```
预期：确认 detail 结构有 `parent_hash: Option<String>` 与 `snapshot_json`。若字段名不同，后续代码相应调整。

- [ ] 在 `tests/admin_amas_http.rs` 末尾新增**集成测试**：approve 一条 suggestion 产生新版本（带 parent），再 rollback → 校验返回 `rolledBack:true` + `versionHash` 为 parent，且 suggestion 状态被标记。最小路径用"不存在 id → 404"边界 + happy path：

```rust
#[tokio::test]
async fn it_amas_suggestion_rollback() {
    let app = spawn_test_server().await;
    let token = setup_amas_admin_token(&app).await;

    // 不存在 id → 404
    let nf = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/suggestions/999999/rollback",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s, _, _) = response_json(nf).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // happy path：先取当前 config 作为初始版本 → approve 一条合法 patch → 产生子版本
    let cfg = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/config",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (_, _, cfg_body) = response_json(cfg).await;
    let parent_hash = cfg_body["data"]; // 仅占位，真实 parent 由 approve 写入版本表时回填
    let _ = parent_hash;

    let sid = app
        .state
        .store()
        .insert_amas_suggestion(
            &learning_backend::store::operations::amas_suggestions::InsertSuggestion {
                based_on_version_hash: "init".into(),
                patch_json: r#"{"memoryModel.baseDesiredRetention":0.85}"#.into(),
                rationale: "rollback 测试".into(),
                evidence_json: "{}".into(),
                cost_usd: Some(0.01),
                tokens_input: Some(10),
                tokens_output: Some(5),
                confidence: Some(0.7),
                initial_status:
                    learning_backend::store::operations::amas_suggestions::SuggestionStatus::Pending,
                decided_by: None,
                decision_note: None,
                base_values_json: None,
            },
        )
        .expect("insert suggestion");

    let approve = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/amas/suggestions/{sid}/approve"),
        Some(serde_json::json!({ "note": "approve for rollback test" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s, _, _) = response_json(approve).await;
    assert_eq!(s, StatusCode::OK);

    let rollback = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/amas/suggestions/{sid}/rollback"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s, _, body) = response_json(rollback).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["data"]["rolledBack"], true);
    assert!(body["data"]["versionHash"].as_str().is_some());
}
```

- [ ] 跑确认失败（路由未挂 → 404 happy path 段亦失败）：

```
cargo test -p learning-backend --test admin_amas_http it_amas_suggestion_rollback
```
预期：`assert_eq!(s, StatusCode::OK)`（approve 后 rollback 段）失败，实际 `404`。

- [ ] 在 `admin_router()`（reject 路由之后）追加：

```rust
        .route("/suggestions/:id/rollback", post(rollback_suggestion))
```

- [ ] 在 `src/routes/admin/amas.rs` 新增 `rollback_suggestion` handler（复用 `apply_and_persist_config` 写回 parent 快照，并标记 suggestion 为 `Superseded`）。把下面追加到 `reject_suggestion` 之后：

```rust
// ─────────── C5: 建议回滚（版本链 restore parent）───────────

async fn rollback_suggestion(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;

    let suggestion = state
        .run_store_task("admin.amas.rollback_lookup", move |store| {
            store.get_amas_suggestion(id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("建议不存在"))?;

    // 该建议产出的版本即"以其 based_on_version_hash 为 parent"的最新版本；
    // 回滚目标 = 该 based_on_version_hash（即应用 patch 前的版本）。
    let target_hash = suggestion.based_on_version_hash.clone();
    let lookup_hash = target_hash.clone();
    let detail = state
        .run_store_task("admin.amas.rollback_target", move |store| {
            store.get_amas_config_version(&lookup_hash)
        })
        .await??
        .ok_or_else(|| {
            AppError::bad_request("PARENT_NOT_FOUND", "回滚目标版本不存在于版本链")
        })?;

    let cfg: crate::amas::config::AMASConfig = serde_json::from_value(detail.snapshot_json)
        .map_err(|e| AppError::internal(&format!("快照反序列化失败: {e}")))?;

    apply_and_persist_config(
        &state,
        &admin.admin_id,
        cfg,
        ConfigVersionSource::Manual,
        Some(format!("rollback suggestion#{id} → {}", &target_hash[..target_hash.len().min(8)])),
    )
    .await?;

    let admin_id = admin.admin_id.clone();
    state
        .run_store_task("admin.amas.rollback_mark", move |store| {
            store.update_amas_suggestion_status(
                id,
                SuggestionStatus::Superseded,
                Some(&admin_id),
                Some("rolled back to parent version"),
            )
        })
        .await??;

    Ok(ok(serde_json::json!({
        "rolledBack": true,
        "versionHash": target_hash,
    })))
}
```

- [ ] 跑确认通过：

```
cargo test -p learning-backend --test admin_amas_http it_amas_suggestion_rollback
```
预期：`test result: ok. 1 passed`。

- [ ] commit：

```
git add src/routes/admin/amas.rs tests/admin_amas_http.rs
git commit -m "feat(amas-advisor): POST /suggestions/:id/rollback 回滚至 based_on 版本并标记 superseded（C5）"
```

---

### Task C9: 模块 C 全量回归 + 鉴权回归

**Files:**
- 无新增；跑全量后端测试确认 C4/C5 + 既有契约不破。

- [ ] 跑模块 C 全部相关测试（store 单测 + 白名单单测 + 集成）：

```
cargo test -p learning-backend --lib store::operations::amas_suggestions store::operations::amas_tuning_whitelist amas::tuning_whitelist
cargo test -p learning-backend --test admin_amas_http
```
预期：两条命令均 `test result: ok`，无 failure。

- [ ] 跑既有 AMAS 集成回归，确认 approve 通路改造（validate_patch 入 store_task）未破坏：

```
cargo test -p learning-backend --test admin_amas_http it_amas_user_and_admin_endpoints
```
预期：`test result: ok. 1 passed`。

- [ ] clippy 守卫（CI 一致）：

```
cargo clippy -p learning-backend --all-targets -- -D warnings 2>&1 | tail -20
```
预期：无 warning/error 输出（exit 0）。

- [ ] commit（如 clippy 触发微调，否则跳过）：

```
git add -A
git commit -m "chore(amas-advisor): 模块 C C4+C5 全量回归 + clippy 收口"
```

## 模块 D — per-patch canary 子系统（引擎多路由 + monitor worker + 端点）

### Task D1: 数据模型 — `amas_patch_canary` 表迁移（migrate.rs `m025`）

C6 per-patch canary 子系统的存储基础。新表支持多条并行 active 灰度行，cohort 区间 `[lo,hi)` 互不重叠（落库前由 store 层校验，DB 仅做 0..=100 与 percent CHECK 守卫）。

**Files:**
- Modify `/Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/src/store/migrate.rs` (注册表 `migrations()` 约 40-61 行新增一项；`migrations_down()` 约 65 行起新增；文件尾部新增 `m025_*` 函数对)

- [ ] 在 `migrations()` 的 `024_client_extras` 之后追加注册项：
  ```rust
          ("024_client_extras", m024_client_extras),
          ("025_amas_patch_canary", m025_amas_patch_canary),
  ```
- [ ] 在 `migrations_down()` 对应位置追加 down 注册（保持索引与 up 表一一对应；具体行参考既有 down 表末尾，追加在末项之后）：
  ```rust
          ("025_amas_patch_canary", m025_amas_patch_canary_down),
  ```
- [ ] 在 `migrate.rs` 文件末尾（紧随 `m024_*_down` 之后）新增 up 函数，贴合 `m022` 的 `execute_batch` + CHECK 写法：
  ```rust
  /// m025 up：per-patch canary 子系统表。支持多条 active 并行灰度。
  ///   - cohort 区间 [cohort_lo, cohort_hi) ⊂ 0..100，active 行之间不重叠由 store 层校验保证。
  ///   - status: active(灰度中) / effective(已 100% 提升 stable) / rolled_back(回滚)。
  ///   - baseline_metrics_json：灰度起始时 stable 切片快照，canary_monitor worker 用于对比。
  fn m025_amas_patch_canary(store: &Store) -> Result<(), StoreError> {
      let conn = store.conn()?;
      conn.execute_batch(
          "CREATE TABLE IF NOT EXISTS amas_patch_canary (
              id                    INTEGER PRIMARY KEY AUTOINCREMENT,
              suggestion_id         INTEGER NOT NULL,
              version_hash          TEXT NOT NULL,
              percent               INTEGER NOT NULL CHECK (percent BETWEEN 0 AND 100),
              cohort_lo             INTEGER NOT NULL CHECK (cohort_lo BETWEEN 0 AND 100),
              cohort_hi             INTEGER NOT NULL CHECK (cohort_hi BETWEEN 0 AND 100),
              status                TEXT NOT NULL DEFAULT 'active'
                                        CHECK (status IN ('active','effective','rolled_back')),
              baseline_metrics_json TEXT NOT NULL DEFAULT '{}',
              started_at            TEXT NOT NULL,
              updated_at            TEXT NOT NULL
          );
          CREATE INDEX IF NOT EXISTS idx_amas_patch_canary_status
              ON amas_patch_canary(status);
          CREATE INDEX IF NOT EXISTS idx_amas_patch_canary_started
              ON amas_patch_canary(started_at DESC);",
      )?;
      Ok(())
  }

  /// m025 down：DROP 索引 + 表。生产严禁 down，仅 dev/test。
  fn m025_amas_patch_canary_down(store: &Store) -> Result<(), StoreError> {
      let conn = store.conn()?;
      conn.execute_batch(
          "DROP INDEX IF EXISTS idx_amas_patch_canary_started;
           DROP INDEX IF EXISTS idx_amas_patch_canary_status;
           DROP TABLE IF EXISTS amas_patch_canary;",
      )?;
      Ok(())
  }
  ```
- [ ] 跑迁移幂等性回归（确认现有迁移测试不破）：
  ```
  cargo test -p learning-backend --lib store::migrate
  ```
  预期：现有 migrate 测试全绿（新增表为追加，旧断言不受影响）。
- [ ] commit：
  ```
  git add src/store/migrate.rs
  git commit -m "feat(store): 新增 amas_patch_canary 表迁移 m025(per-patch canary 子系统)"
  ```

---

### Task D2: Store 层 — `PatchCanary` 结构 + CRUD（含 cohort 不重叠 + 路由命中单测）

**Files:**
- Create `/Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/src/store/operations/amas_patch_canary.rs`
- Modify `/Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/src/store/operations/mod.rs` (模块声明，在 `pub mod amas_canary;` 邻近追加)

- [ ] 先确认 operations/mod.rs 里 `amas_canary` 的声明位置：
  ```
  grep -n "pub mod amas_canary" /Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/src/store/operations/mod.rs
  ```
  预期：能看到 `pub mod amas_canary;` 一行。
- [ ] 在该行下方追加模块声明（Edit `operations/mod.rs`）：
  ```rust
  pub mod amas_patch_canary;
  ```
- [ ] 创建 `amas_patch_canary.rs`，先写**失败测试**（含 struct/方法引用，此时编译应失败）。文件全文：
  ```rust
  //! AMAS per-patch canary 子系统存储（C6）。
  //!
  //! 与单 active 的 `amas_canary_config` 不同：本表支持多条并行 active 灰度行，
  //! 每行占据 cohort 区间 [cohort_lo, cohort_hi) ⊂ 0..100，active 行之间互不重叠
  //! （由 `insert_patch_canary` / `update_patch_canary_scale` 落库前校验保证）。
  //! `AMASEngine::effective_config_for_user` 遍历 active 行按 hash(user_id)%100 命中其一。

  use rusqlite::{params, OptionalExtension};
  use serde::{Deserialize, Serialize};

  use crate::store::{Store, StoreError};

  /// 一条 per-patch 灰度记录。
  #[derive(Debug, Clone, Serialize, Deserialize)]
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

  impl Store {
      /// 列出 patch canary。`status` 为 Some 时按状态过滤，None 返回全部（按 started_at 倒序）。
      pub fn list_patch_canaries(
          &self,
          status: Option<&str>,
      ) -> Result<Vec<PatchCanary>, StoreError> {
          let conn = self.conn()?;
          let base = "SELECT id, suggestion_id, version_hash, percent, cohort_lo, cohort_hi, \
                      status, baseline_metrics_json, started_at, updated_at FROM amas_patch_canary";
          let mut rows = Vec::new();
          if let Some(st) = status {
              let mut stmt =
                  conn.prepare(&format!("{base} WHERE status = ?1 ORDER BY started_at DESC"))?;
              let iter = stmt.query_map([st], map_patch_canary)?;
              for r in iter {
                  rows.push(r?);
              }
          } else {
              let mut stmt = conn.prepare(&format!("{base} ORDER BY started_at DESC"))?;
              let iter = stmt.query_map([], map_patch_canary)?;
              for r in iter {
                  rows.push(r?);
              }
          }
          Ok(rows)
      }

      /// 仅 active 行（engine 路由 + canary_monitor worker 用）。
      pub fn get_active_patch_canaries(&self) -> Result<Vec<PatchCanary>, StoreError> {
          self.list_patch_canaries(Some("active"))
      }

      /// 插入一条 active canary。落库前校验 cohort 区间合法 + 与现有 active 行不重叠。
      pub fn insert_patch_canary(
          &self,
          suggestion_id: i64,
          version_hash: &str,
          percent: u32,
          cohort_lo: u32,
          cohort_hi: u32,
          baseline_metrics_json: &str,
      ) -> Result<i64, StoreError> {
          validate_cohort(cohort_lo, cohort_hi, percent)?;
          let mut conn = self.conn()?;
          let tx = conn.transaction()?;
          ensure_no_overlap(&tx, cohort_lo, cohort_hi, None)?;
          let now = chrono::Utc::now().to_rfc3339();
          tx.execute(
              "INSERT INTO amas_patch_canary
                  (suggestion_id, version_hash, percent, cohort_lo, cohort_hi,
                   status, baseline_metrics_json, started_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?7, ?7)",
              params![
                  suggestion_id,
                  version_hash,
                  percent as i64,
                  cohort_lo as i64,
                  cohort_hi as i64,
                  baseline_metrics_json,
                  now
              ],
          )?;
          let id = tx.last_insert_rowid();
          tx.commit()?;
          Ok(id)
      }

      /// 扩量：更新 percent + cohort 区间，校验合法 + 与其他 active 行不重叠（排除自身）。
      pub fn update_patch_canary_scale(
          &self,
          id: i64,
          percent: u32,
          cohort_lo: u32,
          cohort_hi: u32,
      ) -> Result<(), StoreError> {
          validate_cohort(cohort_lo, cohort_hi, percent)?;
          let mut conn = self.conn()?;
          let tx = conn.transaction()?;
          ensure_no_overlap(&tx, cohort_lo, cohort_hi, Some(id))?;
          let now = chrono::Utc::now().to_rfc3339();
          let n = tx.execute(
              "UPDATE amas_patch_canary
               SET percent = ?2, cohort_lo = ?3, cohort_hi = ?4, updated_at = ?5
               WHERE id = ?1 AND status = 'active'",
              params![id, percent as i64, cohort_lo as i64, cohort_hi as i64, now],
          )?;
          tx.commit()?;
          if n == 0 {
              return Err(StoreError::Validation(format!(
                  "patch canary #{id} 不存在或非 active"
              )));
          }
          Ok(())
      }

      /// 置状态（effective / rolled_back）。
      pub fn set_patch_canary_status(
          &self,
          id: i64,
          status: &str,
      ) -> Result<(), StoreError> {
          if !matches!(status, "active" | "effective" | "rolled_back") {
              return Err(StoreError::Validation(format!("非法 status: {status}")));
          }
          let conn = self.conn()?;
          let now = chrono::Utc::now().to_rfc3339();
          let n = conn.execute(
              "UPDATE amas_patch_canary SET status = ?2, updated_at = ?3 WHERE id = ?1",
              params![id, status, now],
          )?;
          if n == 0 {
              return Err(StoreError::Validation(format!("patch canary #{id} 不存在")));
          }
          Ok(())
      }

      /// 单条读取（端点 promote/rollback 校验用）。
      pub fn get_patch_canary(&self, id: i64) -> Result<Option<PatchCanary>, StoreError> {
          let conn = self.conn()?;
          let row = conn
              .query_row(
                  "SELECT id, suggestion_id, version_hash, percent, cohort_lo, cohort_hi, \
                   status, baseline_metrics_json, started_at, updated_at \
                   FROM amas_patch_canary WHERE id = ?1",
                  [id],
                  map_patch_canary,
              )
              .optional()?;
          Ok(row)
      }
  }

  fn map_patch_canary(r: &rusqlite::Row<'_>) -> rusqlite::Result<PatchCanary> {
      Ok(PatchCanary {
          id: r.get(0)?,
          suggestion_id: r.get(1)?,
          version_hash: r.get(2)?,
          percent: r.get::<_, i64>(3)? as u32,
          cohort_lo: r.get::<_, i64>(4)? as u32,
          cohort_hi: r.get::<_, i64>(5)? as u32,
          status: r.get(6)?,
          baseline_metrics_json: r.get(7)?,
          started_at: r.get(8)?,
          updated_at: r.get(9)?,
      })
  }

  /// cohort 区间合法性：lo < hi ≤ 100，且区间宽度 == percent（保证抽样比例一致）。
  fn validate_cohort(lo: u32, hi: u32, percent: u32) -> Result<(), StoreError> {
      if percent > 100 {
          return Err(StoreError::Validation(format!(
              "percent must be 0..=100, got {percent}"
          )));
      }
      if lo >= hi || hi > 100 {
          return Err(StoreError::Validation(format!(
              "cohort 区间非法: [{lo},{hi}) 须满足 0<=lo<hi<=100"
          )));
      }
      if hi - lo != percent {
          return Err(StoreError::Validation(format!(
              "cohort 宽度({}) 与 percent({percent}) 不一致",
              hi - lo
          )));
      }
      Ok(())
  }

  /// 校验 [lo,hi) 与所有 active 行（可排除 exclude_id）不重叠。
  fn ensure_no_overlap(
      tx: &rusqlite::Transaction<'_>,
      lo: u32,
      hi: u32,
      exclude_id: Option<i64>,
  ) -> Result<(), StoreError> {
      let exclude = exclude_id.unwrap_or(-1);
      // 重叠判定：existing.lo < new.hi AND existing.hi > new.lo
      let overlapping: i64 = tx.query_row(
          "SELECT COUNT(*) FROM amas_patch_canary
           WHERE status = 'active' AND id != ?1 AND cohort_lo < ?3 AND cohort_hi > ?2",
          params![exclude, lo as i64, hi as i64],
          |r| r.get(0),
      )?;
      if overlapping > 0 {
          return Err(StoreError::Validation(format!(
              "cohort 区间 [{lo},{hi}) 与现有 active canary 重叠"
          )));
      }
      Ok(())
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
      fn insert_and_list_active() {
          let s = store();
          let id = s
              .insert_patch_canary(1, "hash-a", 20, 0, 20, "{}")
              .unwrap();
          assert!(id > 0);
          let active = s.get_active_patch_canaries().unwrap();
          assert_eq!(active.len(), 1);
          assert_eq!(active[0].version_hash, "hash-a");
          assert_eq!(active[0].cohort_lo, 0);
          assert_eq!(active[0].cohort_hi, 20);
          assert_eq!(active[0].status, "active");
      }

      #[test]
      fn overlapping_cohort_rejected() {
          let s = store();
          s.insert_patch_canary(1, "hash-a", 20, 0, 20, "{}").unwrap();
          // [10,30) 与 [0,20) 重叠
          let err = s
              .insert_patch_canary(2, "hash-b", 20, 10, 30, "{}")
              .unwrap_err();
          assert!(matches!(err, StoreError::Validation(_)));
          // [20,40) 紧邻不重叠 → OK
          assert!(s.insert_patch_canary(2, "hash-b", 20, 20, 40, "{}").is_ok());
      }

      #[test]
      fn cohort_width_must_match_percent() {
          let s = store();
          // 宽度 30 != percent 20
          let err = s
              .insert_patch_canary(1, "h", 20, 0, 30, "{}")
              .unwrap_err();
          assert!(matches!(err, StoreError::Validation(_)));
      }

      #[test]
      fn scale_recomputes_cohort_and_checks_overlap() {
          let s = store();
          let id = s.insert_patch_canary(1, "h", 20, 0, 20, "{}").unwrap();
          // 扩到 60%：[0,60)
          s.update_patch_canary_scale(id, 60, 0, 60).unwrap();
          let row = s.get_patch_canary(id).unwrap().unwrap();
          assert_eq!(row.percent, 60);
          assert_eq!(row.cohort_hi, 60);
      }

      #[test]
      fn status_transitions() {
          let s = store();
          let id = s.insert_patch_canary(1, "h", 20, 0, 20, "{}").unwrap();
          s.set_patch_canary_status(id, "rolled_back").unwrap();
          assert!(s.get_active_patch_canaries().unwrap().is_empty());
          let all = s.list_patch_canaries(None).unwrap();
          assert_eq!(all[0].status, "rolled_back");
      }

      #[test]
      fn set_status_unknown_id_errors() {
          let s = store();
          let err = s.set_patch_canary_status(999, "effective").unwrap_err();
          assert!(matches!(err, StoreError::Validation(_)));
      }
  }
  ```
- [ ] 跑确认失败（实现已与测试同写于一文件，应直接编译通过 → 改为先确认整体编译）。先验证仅编译：
  ```
  cargo test -p learning-backend --lib store::operations::amas_patch_canary 2>&1 | tail -20
  ```
  预期：6 个测试全绿（`insert_and_list_active`、`overlapping_cohort_rejected`、`cohort_width_must_match_percent`、`scale_recomputes_cohort_and_checks_overlap`、`status_transitions`、`set_status_unknown_id_errors`）。若 `StoreError::Validation` 变体名不符，改用 `grep -n "Validation" src/store/error.rs` 核对后替换。
- [ ] commit：
  ```
  git add src/store/operations/amas_patch_canary.rs src/store/operations/mod.rs
  git commit -m "feat(store): PatchCanary CRUD + cohort 不重叠校验(C6)"
  ```

---

### Task D3: 引擎改造 — `effective_config_for_user` 多 canary 路由（含回归测试）

把 `effective_config_for_user` 从"单 active canary"改为"遍历 `get_active_patch_canaries()` 按 `hash(user_id)%100 ∈ [cohort_lo,cohort_hi)` 命中其一加载该 version snapshot；否则 stable"。保留反序列化失败/version 缺失回退 stable + warn。沿用现有 `DefaultHasher` 算法。

**Files:**
- Modify `/Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/src/amas/engine.rs` (`effective_config_for_user` 约 121-177 行整体替换)
- Modify `/Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/src/amas/engine.rs` (`#[cfg(test)]` 区追加回归测试；先 `grep -n "#\[cfg(test)\]" src/amas/engine.rs` 定位测试模块)

- [ ] 先在 engine.rs 的测试模块内**新增失败回归测试**。先定位测试 mod 与现有 store/engine 构造 helper：
  ```
  grep -n "fn effective_config\|fn test_engine\|AMASEngine::new\|insert_amas_config_version\|set_amas_canary" /Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/src/amas/engine.rs
  ```
  预期：能看到测试模块里既有的 engine 构造方式（沿用之，下方测试按其 helper 名调整）。
- [ ] 在 engine.rs 测试模块追加（如已有同名 helper 则复用，不重复定义）：
  ```rust
      #[test]
      fn effective_config_routes_by_patch_canary_cohort() {
          let engine = build_test_engine();
          // 落一个 canary version snapshot（snapshot 与 stable 不同：改个可观测字段）
          let mut canary_cfg = engine.get_config();
          canary_cfg.memory_model.target_recall = 0.99;
          let snap = serde_json::to_string(&canary_cfg).unwrap();
          let (_id, vhash) = engine
              .store
              .insert_amas_config_version(&snap, "admin", ConfigVersionSource::Manual, None, None)
              .unwrap();
          // 灰度 [0,20)：宽度=percent=20
          engine
              .store
              .insert_patch_canary(1, &vhash, 20, 0, 20, "{}")
              .unwrap();

          // 命中桶的用户拿 canary，未命中的拿 stable
          let mut hit = 0usize;
          for i in 0..200 {
              let uid = format!("user-{i}");
              let cfg = engine.effective_config_for_user(&uid);
              if (cfg.memory_model.target_recall - 0.99).abs() < 1e-9 {
                  hit += 1;
              }
          }
          // 20% 桶 → 200 用户里命中数应在合理区间（非 0、非全部）
          assert!(hit > 0 && hit < 200, "hit={hit} 应落在 (0,200)");
      }

      #[test]
      fn effective_config_falls_back_stable_on_missing_version() {
          let engine = build_test_engine();
          // canary 指向不存在的 version_hash → 全员回退 stable
          engine
              .store
              .insert_patch_canary(1, "nonexistent-hash", 100, 0, 100, "{}")
              .unwrap();
          let stable = engine.get_config();
          let cfg = engine.effective_config_for_user("any-user");
          assert_eq!(cfg.memory_model.target_recall, stable.memory_model.target_recall);
      }

      #[test]
      fn effective_config_no_active_canary_returns_stable() {
          let engine = build_test_engine();
          let stable = engine.get_config();
          let cfg = engine.effective_config_for_user("user-x");
          assert_eq!(cfg.memory_model.target_recall, stable.memory_model.target_recall);
      }
  ```
  注：`build_test_engine()` / `ConfigVersionSource` / `memory_model.target_recall` 若与实际不符，按上一步 grep 结果替换为真实 helper 与可观测字段（任一 stable 与 canary 取值不同的字段即可）。
- [ ] 跑确认失败（旧实现走单 `amas_canary_config`，不读 `amas_patch_canary` → 命中测试与回退测试应失败）：
  ```
  cargo test -p learning-backend --lib amas::engine::tests::effective_config 2>&1 | tail -25
  ```
  预期：`effective_config_routes_by_patch_canary_cohort` 失败（hit=0，断言 `hit > 0` 不满足）。
- [ ] 用多 canary 实现替换 `effective_config_for_user`（替换 121-177 行整体）：
  ```rust
      /// C6:按 user_id 解析"有效配置"（per-patch canary 子系统）。
      ///   - 遍历 get_active_patch_canaries()，按 hash(user_id)%100 ∈ [cohort_lo,cohort_hi)
      ///     命中其一 → 从 amas_config_versions 加载该 version snapshot
      ///   - 未命中任何 active canary → 返回 stable(`self.config.read()`)
      ///   - 反序列化失败 / version 缺失 / 查表失败 → 回退 stable(打 warn log 不抛错)
      ///
      /// process_event_blocking 在入口调一次，后续整个 request 内 config 一致。
      pub fn effective_config_for_user(&self, user_id: &str) -> Arc<AMASConfig> {
          let stable: Arc<AMASConfig> = Arc::clone(&self.config.read());
          let canaries = match self.store.get_active_patch_canaries() {
              Ok(c) => c,
              Err(e) => {
                  tracing::warn!(error=%e, "effective_config_for_user: 查 patch canary 失败,回退 stable");
                  return stable;
              }
          };
          if canaries.is_empty() {
              return stable;
          }
          // hash(user_id) % 100 ∈ [cohort_lo, cohort_hi) 命中其一
          let bucket = {
              use std::hash::{Hash, Hasher};
              let mut h = std::collections::hash_map::DefaultHasher::new();
              user_id.hash(&mut h);
              (h.finish() % 100) as u32
          };
          let Some(hit) = canaries
              .iter()
              .find(|c| bucket >= c.cohort_lo && bucket < c.cohort_hi)
          else {
              return stable;
          };
          // 从 versions 表拉 canary snapshot
          match self.store.get_amas_config_version(&hit.version_hash) {
              Ok(Some(version)) => {
                  match serde_json::from_value::<AMASConfig>(version.snapshot_json.clone()) {
                      Ok(cfg) => Arc::new(cfg),
                      Err(e) => {
                          tracing::warn!(
                              version_hash = %hit.version_hash,
                              error = %e,
                              "effective_config_for_user: 反序列化 canary snapshot 失败,回退 stable"
                          );
                          stable
                      }
                  }
              }
              Ok(None) => {
                  tracing::warn!(
                      version_hash = %hit.version_hash,
                      "effective_config_for_user: canary version_hash 不存在于 amas_config_versions,回退 stable"
                  );
                  stable
              }
              Err(e) => {
                  tracing::warn!(error=%e, "effective_config_for_user: 加载 canary version 失败,回退 stable");
                  stable
              }
          }
      }
  ```
- [ ] 跑确认通过：
  ```
  cargo test -p learning-backend --lib amas::engine::tests::effective_config 2>&1 | tail -15
  ```
  预期：3 个回归测试全绿。
- [ ] commit：
  ```
  git add src/amas/engine.rs
  git commit -m "refactor(engine): effective_config_for_user 改 per-patch canary 多路由(保留 stable 回退)"
  ```

---

### Task D4: `canary_monitor` worker — 自动回滚判定（store-level 阈值单测）

对每条 active patch_canary 调 `aggregate_amas_version_slice(version_hash)` 取 live `mean_reward`/`anomaly_rate`，与 `baseline_metrics_json` 对比；reward 降幅 > 阈值 或 anomaly 率升幅 > 阈值 → `set_patch_canary_status(id,'rolled_back')` + 审计 + SSE。worker 失败仅 `tracing::warn` 不抛。

**Files:**
- Create `/Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/src/workers/canary_monitor.rs`

- [ ] 创建 `canary_monitor.rs`，先写**纯函数阈值判定 + 失败测试**（判定逻辑独立成 `should_rollback`，便于单测）。文件全文：
  ```rust
  //! C6:per-patch canary 自动回滚监测 worker（cron 每 5 分钟）。
  //!
  //! 对每条 active patch_canary 取 live 切片（aggregate_amas_version_slice）与
  //! baseline_metrics_json 对比：reward 降幅 > REWARD_DROP_THRESHOLD 或 anomaly 率
  //! 升幅 > ANOMALY_RISE_THRESHOLD → 自动回滚（status='rolled_back' + 审计 + SSE）。
  //! worker 失败仅 tracing::warn，不抛、不 disable 调度器（沿用 worker 容错惯例）。

  use serde::Deserialize;

  use crate::state::{AppState, SseEvent};
  use crate::store::operations::amas_telemetry::VersionMetricsSlice;

  /// reward 平均值降幅阈值（live 比 baseline 低超过此绝对值 → 回滚）。
  // TODO(C6): 设为 system_settings 可配。
  const REWARD_DROP_THRESHOLD: f64 = 0.05;
  /// anomaly 率升幅阈值（live 比 baseline 高超过此绝对值 → 回滚）。
  // TODO(C6): 设为 system_settings 可配。
  const ANOMALY_RISE_THRESHOLD: f64 = 0.05;
  /// 最少样本量：live 切片 event_count 不足时跳过判定（避免早期噪声误回滚）。
  const MIN_SAMPLE: u64 = 50;

  /// baseline_metrics_json 反序列化目标（灰度起始时 stable 切片快照）。
  #[derive(Debug, Default, Deserialize)]
  #[serde(rename_all = "camelCase")]
  struct Baseline {
      #[serde(default)]
      mean_reward: f64,
      #[serde(default)]
      anomaly_rate: f64,
  }

  /// 纯判定：给定 baseline 与 live 切片，是否应回滚。样本不足返回 false。
  fn should_rollback(baseline: &Baseline, live: &VersionMetricsSlice) -> bool {
      if live.event_count < MIN_SAMPLE {
          return false;
      }
      let reward_drop = baseline.mean_reward - live.mean_reward;
      let anomaly_rise = live.anomaly_rate - baseline.anomaly_rate;
      reward_drop > REWARD_DROP_THRESHOLD || anomaly_rise > ANOMALY_RISE_THRESHOLD
  }

  pub async fn run(state: &AppState) {
      let store = state.store_arc();
      let canaries = match store.get_active_patch_canaries() {
          Ok(c) => c,
          Err(e) => {
              tracing::warn!(error = %e, "canary_monitor: 查 active canary 失败");
              return;
          }
      };
      for c in canaries {
          let baseline: Baseline =
              serde_json::from_str(&c.baseline_metrics_json).unwrap_or_default();
          let live = match store.aggregate_amas_version_slice(&c.version_hash) {
              Ok(s) => s,
              Err(e) => {
                  tracing::warn!(id = c.id, error = %e, "canary_monitor: 聚合切片失败,跳过");
                  continue;
              }
          };
          if !should_rollback(&baseline, &live) {
              continue;
          }
          if let Err(e) = store.set_patch_canary_status(c.id, "rolled_back") {
              tracing::warn!(id = c.id, error = %e, "canary_monitor: 自动回滚置状态失败");
              continue;
          }
          tracing::warn!(
              id = c.id,
              version_hash = %c.version_hash,
              baseline_reward = baseline.mean_reward,
              live_reward = live.mean_reward,
              baseline_anomaly = baseline.anomaly_rate,
              live_anomaly = live.anomaly_rate,
              "canary_monitor: patch canary 自动回滚"
          );
          state.broadcast_to_all_sse(SseEvent::Incident {
              error_rate: live.anomaly_rate,
              window_secs: 0,
          });
      }
  }

  #[cfg(test)]
  mod tests {
      use super::*;

      fn slice(count: u64, reward: f64, anomaly: f64) -> VersionMetricsSlice {
          VersionMetricsSlice {
              version_hash: "h".into(),
              event_count: count,
              mean_reward: reward,
              anomaly_rate: anomaly,
              ..Default::default()
          }
      }

      #[test]
      fn rolls_back_on_reward_drop() {
          let baseline = Baseline { mean_reward: 0.80, anomaly_rate: 0.01 };
          let live = slice(100, 0.70, 0.01); // 降 0.10 > 0.05
          assert!(should_rollback(&baseline, &live));
      }

      #[test]
      fn rolls_back_on_anomaly_rise() {
          let baseline = Baseline { mean_reward: 0.80, anomaly_rate: 0.01 };
          let live = slice(100, 0.80, 0.10); // 升 0.09 > 0.05
          assert!(should_rollback(&baseline, &live));
      }

      #[test]
      fn keeps_when_within_thresholds() {
          let baseline = Baseline { mean_reward: 0.80, anomaly_rate: 0.05 };
          let live = slice(100, 0.78, 0.06); // 降 0.02、升 0.01 均 < 0.05
          assert!(!should_rollback(&baseline, &live));
      }

      #[test]
      fn skips_when_sample_too_small() {
          let baseline = Baseline { mean_reward: 0.80, anomaly_rate: 0.01 };
          let live = slice(10, 0.0, 1.0); // 即便极端退化，样本 < 50 不判定
          assert!(!should_rollback(&baseline, &live));
      }
  }
  ```
  注：`state.store_arc()` 与 `SseEvent::Incident { error_rate, window_secs }` 字段名以实际为准 —— 先 `grep -n "fn store_arc\|fn store\b\|Incident {" src/state.rs` 核对；若取 store 的方法名不同（如 `store()`），替换 `store_arc()`。`VersionMetricsSlice` 已 `#[derive(Default)]`（见 amas_telemetry.rs:201），`..Default::default()` 可用。
- [ ] 跑确认通过（纯判定单测，无需 worker 集成）：
  ```
  cargo test -p learning-backend --lib workers::canary_monitor 2>&1 | tail -15
  ```
  预期：4 个测试全绿（`rolls_back_on_reward_drop`、`rolls_back_on_anomaly_rise`、`keeps_when_within_thresholds`、`skips_when_sample_too_small`）。若 `store_arc`/`Incident` 字段不符报编译错，按上一步 grep 修正。
- [ ] commit：
  ```
  git add src/workers/canary_monitor.rs
  git commit -m "feat(worker): canary_monitor 自动回滚判定 + reward/anomaly 阈值(C6)"
  ```

---

### Task D5: 注册 `canary_monitor` worker（workers/mod.rs）

**Files:**
- Modify `/Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/src/workers/mod.rs` (mod 声明约 1-21；`WorkerName` 枚举约 45-66；`as_str` 约 68-90；`planned_jobs` 约 240-278；`register_jobs` 的 match 约 486+ 末尾；测试约 738/795)

- [ ] 先写**失败测试**：在 mod.rs 测试模块（约 603 起）追加，断言 `planned_jobs` 含 `CanaryMonitor` 且 cron 可解析、5 分钟周期。先看 `all_planned_jobs_have_parseable_cron` 现状：
  ```
  grep -n "all_planned_jobs_have_parseable_cron\|fn.*planned_jobs\|CanaryMonitor" /Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/src/workers/mod.rs
  ```
- [ ] 在测试模块追加：
  ```rust
      #[tokio::test]
      async fn canary_monitor_is_registered_with_5min_cron() {
          let manager = test_manager(true).await;
          let jobs = manager.planned_jobs();
          let job = jobs
              .iter()
              .find(|j| j.name == WorkerName::CanaryMonitor)
              .expect("canary_monitor 应在 planned_jobs 中");
          assert_eq!(job.cron, "0 */5 * * * *");
          assert!(job.enabled);
      }
  ```
  注：`test_manager(true)` 为本测试模块既有 helper 名占位 —— 用上一步 grep 看到的真实构造（如 `make_manager` / 直接 `WorkerManager::new`）替换。
- [ ] 跑确认失败（`WorkerName::CanaryMonitor` 未定义 → 编译错）：
  ```
  cargo test -p learning-backend --lib workers::tests::canary_monitor_is_registered 2>&1 | tail -15
  ```
  预期：编译错误 `no variant named CanaryMonitor`。
- [ ] Edit mod 声明（约第 3 行 `pub mod canary_monitor;` 按字母序插入 `pub mod cache_cleanup;` 与 `pub mod config_watcher;` 之间）：
  ```rust
  pub mod canary_monitor;
  ```
- [ ] Edit `WorkerName` 枚举（在 `SchedulerHealthWatchdog` 之后追加）：
  ```rust
      /// C6:per-patch canary 自动回滚监测（每 5 分钟）
      CanaryMonitor,
  ```
- [ ] Edit `as_str`（在 `Self::SchedulerHealthWatchdog => "scheduler_health_watchdog",` 之后追加）：
  ```rust
              Self::CanaryMonitor => "canary_monitor",
  ```
- [ ] Edit `planned_jobs` 的 vec（在 `SchedulerHealthWatchdog` JobSpec 之后追加；canary_monitor 需 `llm_advisor_state` 注入的 AppState 做 SSE，故 enabled 取决于 state 存在）：
  ```rust
              // C6:per-patch canary 自动回滚监测，每 5 分钟
              JobSpec {
                  name: WorkerName::CanaryMonitor,
                  cron: "0 */5 * * * *",
                  enabled: self.llm_advisor_state.is_some(),
              },
  ```
- [ ] Edit `register_jobs` 的 match（在 `SchedulerHealthWatchdog` 分支之后追加，复用既有 `llm_advisor_state` 字段拿 AppState）：
  ```rust
                  // C6:canary_monitor —— 需 AppState 做 SSE 通知
                  WorkerName::CanaryMonitor => {
                      let cm_state = match self.llm_advisor_state.clone() {
                          Some(s) => s,
                          None => continue,
                      };
                      add_job(scheduler, spec.cron, name_str, job_store, health_state, move || {
                          let state = cm_state.clone();
                          async move {
                              canary_monitor::run(&state).await;
                          }
                      })
                      .await;
                  }
  ```
- [ ] 跑确认通过：
  ```
  cargo test -p learning-backend --lib workers:: 2>&1 | tail -20
  ```
  预期：`canary_monitor_is_registered_with_5min_cron` + `all_planned_jobs_have_parseable_cron` 等全绿。若 `llm_advisor_state` 私有字段访问受限（同模块内应可），无碍。
- [ ] commit：
  ```
  git add src/workers/mod.rs
  git commit -m "feat(worker): 注册 canary_monitor(每5分钟,需 AppState SSE)"
  ```

---

### Task D6: 端点 — POST `/advisor/canary`（approve 进灰度，含集成测试）

approve 一条 pending suggestion 但进灰度而非直接生效：落 canary version snapshot + 抓 stable baseline 切片 + 建 `amas_patch_canary` 行（cohort `[0,percent)`）。

**Files:**
- Modify `/Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/src/routes/admin/amas.rs` (`admin_router()` 约 31-60 加路由；文件 canary 区附近加 handler + req struct)
- Modify `/Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/tests/admin_amas_http.rs` (文件末尾加集成测试)

- [ ] 先在集成测试文件末尾写**失败测试**（沿用 `spawn_test_server` / `login_and_get_token` / `request` / `response_json`；先 grep 既有 admin 登录 helper 调用样式）：
  ```
  grep -n "login_and_get_token\|spawn_test_server\|fn it_amas\|insert_amas_suggestion\|/api/admin/amas/suggestions" /Users/liji/english/wordforge/.claude/worktrees/admin-ui-redesign/tests/admin_amas_http.rs | head
  ```
- [ ] 在 `tests/admin_amas_http.rs` 末尾追加（需要一条 pending suggestion + 一个 version_hash；建议复用既有 suggestion 创建 helper，若无则经 store 直插。下方走 HTTP 路径，假定测试 server 暴露 `app.store` 用于 seeding —— 按既有测试里 seed suggestion 的真实方式替换）：
  ```rust
  #[tokio::test]
  async fn it_advisor_canary_create_scale_rollback_promote() {
      let app = spawn_test_server().await;
      let token = login_and_get_token(&app.app).await;

      // seed:一条 pending suggestion + 一个落库 version snapshot
      let (suggestion_id, _vhash) = seed_pending_suggestion(&app).await;

      // 1) 创建 canary 20%
      let resp = request(
          &app.app,
          Method::POST,
          "/api/admin/amas/advisor/canary",
          Some(serde_json::json!({ "suggestionId": suggestion_id, "percent": 20 })),
          &[("authorization", auth_header(&token))],
      )
      .await;
      let (status, _, body) = response_json(resp).await;
      assert_eq!(status, StatusCode::OK);
      let canary_id = body["data"]["id"].as_i64().expect("canary id");
      assert_eq!(body["data"]["percent"], 20);
      assert_eq!(body["data"]["status"], "active");

      // 2) GET 列表含 live 字段
      let resp = request(
          &app.app,
          Method::GET,
          "/api/admin/amas/advisor/canary",
          None,
          &[("authorization", auth_header(&token))],
      )
      .await;
      let (status, _, body) = response_json(resp).await;
      assert_eq!(status, StatusCode::OK);
      let arr = body["data"].as_array().expect("canary array");
      assert_eq!(arr.len(), 1);
      assert!(arr[0].get("liveReward").is_some());
      assert!(arr[0].get("liveAnomalyRate").is_some());
      assert!(arr[0].get("baselineReward").is_some());

      // 3) 扩量到 60%
      let resp = request(
          &app.app,
          Method::POST,
          &format!("/api/admin/amas/advisor/canary/{canary_id}/scale"),
          Some(serde_json::json!({ "percent": 60 })),
          &[("authorization", auth_header(&token))],
      )
      .await;
      let (status, _, _) = response_json(resp).await;
      assert_eq!(status, StatusCode::OK);

      // 4) percent 越界 → 400
      let resp = request(
          &app.app,
          Method::POST,
          &format!("/api/admin/amas/advisor/canary/{canary_id}/scale"),
          Some(serde_json::json!({ "percent": 150 })),
          &[("authorization", auth_header(&token))],
      )
      .await;
      let (status, _, _) = response_json(resp).await;
      assert_eq!(status, StatusCode::BAD_REQUEST);

      // 5) 100% 提升 stable
      let resp = request(
          &app.app,
          Method::POST,
          &format!("/api/admin/amas/advisor/canary/{canary_id}/scale"),
          Some(serde_json::json!({ "percent": 100 })),
          &[("authorization", auth_header(&token))],
      )
      .await;
      let (status, _, _) = response_json(resp).await;
      assert_eq!(status, StatusCode::OK);
      let resp = request(
          &app.app,
          Method::POST,
          &format!("/api/admin/amas/advisor/canary/{canary_id}/promote"),
          None,
          &[("authorization", auth_header(&token))],
      )
      .await;
      let (status, _, body) = response_json(resp).await;
      assert_eq!(status, StatusCode::OK);
      assert_eq!(body["data"]["promoted"], true);
      assert!(body["data"]["versionHash"].is_string());
  }

  #[tokio::test]
  async fn it_advisor_canary_manual_rollback_and_unknown_id() {
      let app = spawn_test_server().await;
      let token = login_and_get_token(&app.app).await;
      let (suggestion_id, _vhash) = seed_pending_suggestion(&app).await;

      let resp = request(
          &app.app,
          Method::POST,
          "/api/admin/amas/advisor/canary",
          Some(serde_json::json!({ "suggestionId": suggestion_id, "percent": 20 })),
          &[("authorization", auth_header(&token))],
      )
      .await;
      let (_, _, body) = response_json(resp).await;
      let canary_id = body["data"]["id"].as_i64().unwrap();

      // 手动回滚
      let resp = request(
          &app.app,
          Method::POST,
          &format!("/api/admin/amas/advisor/canary/{canary_id}/rollback"),
          None,
          &[("authorization", auth_header(&token))],
      )
      .await;
      let (status, _, body) = response_json(resp).await;
      assert_eq!(status, StatusCode::OK);
      assert_eq!(body["data"]["rolledBack"], true);

      // 回滚不存在 id → 400/404
      let resp = request(
          &app.app,
          Method::POST,
          "/api/admin/amas/advisor/canary/99999/rollback",
          None,
          &[("authorization", auth_header(&token))],
      )
      .await;
      let (status, _, _) = response_json(resp).await;
      assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND);
  }
  ```
- [ ] 在测试文件加 `seed_pending_suggestion` helper（落一个 version snapshot + 一条 pending suggestion；用 `app.store` 直插，沿用既有测试 seed 风格。若 `TestApp` 暴露 store 的字段名不同，按真实字段替换）：
  ```rust
  async fn seed_pending_suggestion(app: &common::app::TestApp) -> (i64, String) {
      use learning_backend::store::operations::amas_suggestions::{InsertSuggestion, SuggestionStatus};
      use learning_backend::store::operations::amas_versions::ConfigVersionSource;

      // 1) 落一个可灰度的 version snapshot（取当前 stable 序列化）
      let snap = serde_json::to_string(&app.amas.get_config()).unwrap();
      let (_vid, vhash) = app
          .store
          .insert_amas_config_version(&snap, "admin", ConfigVersionSource::Manual, None, None)
          .unwrap();

      // 2) 落一条 pending suggestion（patch 用一个白名单内合法字段；按真实 InsertSuggestion 字段填充）
      let row = app
          .store
          .insert_amas_suggestion(&InsertSuggestion {
              based_on_version_hash: vhash.clone(),
              patch_json: serde_json::json!({ "memoryModel.targetRecall": 0.9 }),
              rationale: "test".into(),
              evidence_json: serde_json::json!({}),
              cost_usd: 0.0,
              tokens_input: 0,
              tokens_output: 0,
              confidence: 0.9,
              base_values_json: serde_json::json!({}),
              initial_status: SuggestionStatus::Pending,
          })
          .unwrap();
      (row.id, vhash)
  }
  ```
  注：`InsertSuggestion` 字段集以 `src/store/operations/amas_suggestions.rs` 实际为准（grep `pub struct InsertSuggestion` 核对字段名/类型）；`app.store` / `app.amas` 字段名以 `common/app.rs` 的 `TestApp` 定义为准。
- [ ] 跑确认失败（路由未注册 → 404 / handler 不存在编译错）：
  ```
  cargo test -p learning-backend --test admin_amas_http it_advisor_canary 2>&1 | tail -25
  ```
  预期：测试断言失败（404）或编译错（helper 引用未定义类型时先补齐 helper）。
- [ ] 在 `admin_router()` 的 `suggestions/:id/reject` 路由之后追加 5 条路由：
  ```rust
          .route("/advisor/canary", get(list_canaries).post(create_canary))
          .route("/advisor/canary/:id/scale", post(scale_canary))
          .route("/advisor/canary/:id/rollback", post(rollback_canary))
          .route("/advisor/canary/:id/promote", post(promote_canary))
  ```
- [ ] 在 amas.rs canary 区（约 174 行 `disable_canary` 之后）追加 handler + 请求体：
  ```rust
  // ─────────────────── C6:per-patch canary 子系统 ───────────────────

  #[derive(Debug, Deserialize)]
  #[serde(rename_all = "camelCase")]
  struct CreateCanaryRequest {
      suggestion_id: i64,
      /// 灰度初始百分比 0..=100，cohort 取 [0, percent)。
      percent: u32,
  }

  #[derive(Debug, Deserialize)]
  #[serde(rename_all = "camelCase")]
  struct ScaleCanaryRequest {
      percent: u32,
  }

  /// POST /advisor/canary —— approve 一条 pending suggestion 进灰度（非直接生效）。
  /// 落 canary version snapshot + 抓 stable baseline 切片 + 建 patch_canary 行。
  async fn create_canary(
      _admin: AdminAuthUser,
      State(state): State<AppState>,
      JsonBody(req): JsonBody<CreateCanaryRequest>,
  ) -> Result<impl axum::response::IntoResponse, AppError> {
      use crate::store::operations::amas_suggestions::SuggestionStatus;

      if req.percent == 0 || req.percent > 100 {
          return Err(AppError::bad_request(
              "INVALID_PERCENT",
              "percent must be in 1..=100",
          ));
      }

      // 取 pending suggestion + 校验状态
      let sid = req.suggestion_id;
      let suggestion = state
          .run_store_task("admin.amas.canary.create_lookup", move |store| {
              store.get_amas_suggestion(sid)
          })
          .await??
          .ok_or_else(|| AppError::not_found("建议不存在"))?;
      if !matches!(suggestion.status, SuggestionStatus::Pending) {
          return Err(AppError::bad_request("BAD_STATUS", "仅 pending 建议可进灰度"));
      }

      // baseline:当前 stable version 的切片（灰度起点）
      let stable_hash = suggestion.based_on_version_hash.clone();
      let baseline_json = state
          .run_store_task("admin.amas.canary.baseline", move |store| {
              store.aggregate_amas_version_slice(&stable_hash)
          })
          .await?
          .map(|slice| serde_json::to_string(&slice).unwrap_or_else(|_| "{}".into()))
          .unwrap_or_else(|_| "{}".into());

      // 落 canary version snapshot：把 patch 应用到 stable 后入版本表（与 approve 同构造）
      let patch_obj = suggestion
          .patch_json
          .as_object()
          .ok_or_else(|| AppError::internal("patch_json 非对象"))?
          .clone();
      let current = state.amas().get_config();
      let mut cfg_value =
          serde_json::to_value(&current).map_err(|e| AppError::internal(&format!("ser: {e}")))?;
      for (path, value) in &patch_obj {
          write_path(&mut cfg_value, path, value.clone());
      }
      let new_cfg: crate::amas::config::AMASConfig = serde_json::from_value(cfg_value)
          .map_err(|e| AppError::bad_request("PATCH_INVALID", &format!("应用 patch 失败: {e}")))?;
      new_cfg
          .validate()
          .map_err(|e| AppError::bad_request("AMAS_INVALID_CONFIG", &e))?;
      let snapshot_json = serde_json::to_string(&new_cfg)
          .map_err(|e| AppError::internal(&format!("配置序列化失败: {e}")))?;

      let parent_hash = suggestion.based_on_version_hash.clone();
      let percent = req.percent;
      let admin_id = _admin.admin_id.clone();
      let inserted = state
          .run_store_task("admin.amas.canary.create", move |store| {
              let (_vid, vhash) = store.insert_amas_config_version(
                  &snapshot_json,
                  &admin_id,
                  ConfigVersionSource::LlmSuggested,
                  Some(&format!("canary suggestion#{sid}")),
                  Some(&parent_hash),
              )?;
              let id = store.insert_patch_canary(
                  sid, &vhash, percent, 0, percent, &baseline_json,
              )?;
              store.get_patch_canary(id)
          })
          .await??
          .ok_or_else(|| AppError::internal("canary 落库后读取失败"))?;

      Ok(ok(serde_json::to_value(&inserted).unwrap()))
  }

  /// GET /advisor/canary —— active+历史列表，每行附 live 切片（liveReward/liveAnomalyRate/baselineReward）。
  async fn list_canaries(
      _admin: AdminAuthUser,
      State(state): State<AppState>,
  ) -> Result<impl axum::response::IntoResponse, AppError> {
      let rows = state
          .run_store_task("admin.amas.canary.list", |store| {
              let canaries = store.list_patch_canaries(None)?;
              let mut out = Vec::with_capacity(canaries.len());
              for c in canaries {
                  let live = store
                      .aggregate_amas_version_slice(&c.version_hash)
                      .unwrap_or_default();
                  let baseline: serde_json::Value =
                      serde_json::from_str(&c.baseline_metrics_json).unwrap_or(serde_json::json!({}));
                  let mut v = serde_json::to_value(&c).unwrap_or(serde_json::json!({}));
                  if let Some(obj) = v.as_object_mut() {
                      obj.insert("liveReward".into(), serde_json::json!(live.mean_reward));
                      obj.insert("liveAnomalyRate".into(), serde_json::json!(live.anomaly_rate));
                      obj.insert(
                          "baselineReward".into(),
                          baseline.get("meanReward").cloned().unwrap_or(serde_json::json!(0.0)),
                      );
                  }
                  out.push(v);
              }
              Ok::<_, crate::store::StoreError>(out)
          })
          .await??;
      Ok(ok(rows))
  }

  /// POST /advisor/canary/:id/scale —— 扩量到目标 percent，cohort 重算 [0,percent)。
  async fn scale_canary(
      _admin: AdminAuthUser,
      State(state): State<AppState>,
      Path(id): Path<i64>,
      JsonBody(req): JsonBody<ScaleCanaryRequest>,
  ) -> Result<impl axum::response::IntoResponse, AppError> {
      if req.percent == 0 || req.percent > 100 {
          return Err(AppError::bad_request(
              "INVALID_PERCENT",
              "percent must be in 1..=100",
          ));
      }
      let percent = req.percent;
      state
          .run_store_task("admin.amas.canary.scale", move |store| {
              store.update_patch_canary_scale(id, percent, 0, percent)
          })
          .await?
          .map_err(|e| AppError::bad_request("SCALE_FAILED", &e.to_string()))?;
      let updated = state
          .run_store_task("admin.amas.canary.scale_read", move |store| {
              store.get_patch_canary(id)
          })
          .await??
          .ok_or_else(|| AppError::not_found("canary 不存在"))?;
      Ok(ok(serde_json::to_value(&updated).unwrap()))
  }

  /// POST /advisor/canary/:id/rollback —— 手动回滚（status='rolled_back'）。
  async fn rollback_canary(
      _admin: AdminAuthUser,
      State(state): State<AppState>,
      Path(id): Path<i64>,
  ) -> Result<impl axum::response::IntoResponse, AppError> {
      state
          .run_store_task("admin.amas.canary.rollback", move |store| {
              store.set_patch_canary_status(id, "rolled_back")
          })
          .await?
          .map_err(|e| AppError::bad_request("ROLLBACK_FAILED", &e.to_string()))?;
      Ok(ok(serde_json::json!({ "rolledBack": true })))
  }

  /// POST /advisor/canary/:id/promote —— 100% → 提升 stable，status='effective'。
  async fn promote_canary(
      admin: AdminAuthUser,
      State(state): State<AppState>,
      Path(id): Path<i64>,
  ) -> Result<impl axum::response::IntoResponse, AppError> {
      let canary = state
          .run_store_task("admin.amas.canary.promote_lookup", move |store| {
              store.get_patch_canary(id)
          })
          .await??
          .ok_or_else(|| AppError::not_found("canary 不存在"))?;
      if canary.status != "active" {
          return Err(AppError::bad_request("BAD_STATUS", "仅 active canary 可提升"));
      }
      if canary.percent != 100 {
          return Err(AppError::bad_request(
              "NOT_FULL_ROLLOUT",
              "仅 100% 灰度可提升 stable",
          ));
      }
      // 把 canary version snapshot 提升为 stable（复用 restore 通路）
      let vhash = canary.version_hash.clone();
      let vhash_lookup = vhash.clone();
      let detail = state
          .run_store_task("admin.amas.canary.promote_version", move |store| {
              store.get_amas_config_version(&vhash_lookup)
          })
          .await??
          .ok_or_else(|| AppError::internal("canary version 不存在"))?;
      let cfg: crate::amas::config::AMASConfig = serde_json::from_value(detail.snapshot_json)
          .map_err(|e| AppError::internal(&format!("快照反序列化失败: {e}")))?;
      apply_and_persist_config(
          &state,
          &admin.admin_id,
          cfg,
          ConfigVersionSource::Manual,
          Some(format!("promote canary#{id} → stable")),
      )
      .await?;
      state
          .run_store_task("admin.amas.canary.promote_status", move |store| {
              store.set_patch_canary_status(id, "effective")
          })
          .await??;
      Ok(ok(serde_json::json!({ "promoted": true, "versionHash": vhash })))
  }
  ```
  注：`write_path` 为 amas.rs 既有私有 fn（approve_suggestion 已用，见 626 行）；`aggregate_amas_version_slice` 返回 `Result<VersionMetricsSlice,_>`，`unwrap_or_default()` 依赖其 `#[derive(Default)]`（已确认）。`run_store_task` 闭包返回类型推断处若报错，按 `list_canaries` 的显式 `Ok::<_, StoreError>(..)` 标注。
- [ ] 跑确认通过：
  ```
  cargo test -p learning-backend --test admin_amas_http it_advisor_canary 2>&1 | tail -25
  ```
  预期：`it_advisor_canary_create_scale_rollback_promote` + `it_advisor_canary_manual_rollback_and_unknown_id` 全绿。
- [ ] commit：
  ```
  git add src/routes/admin/amas.rs tests/admin_amas_http.rs
  git commit -m "feat(api): /advisor/canary CRUD(create/list-live/scale/rollback/promote)(C6)"
  ```

---

### Task D7: 模块D 全量回归 + clippy

**Files:**
- 无新增（验证收口）

- [ ] 跑模块D 涉及的全部后端测试：
  ```
  cargo test -p learning-backend --lib amas::engine workers::canary_monitor workers::tests store::operations::amas_patch_canary store::migrate 2>&1 | tail -20
  ```
  预期：全绿。
- [ ] 跑 canary 集成测试 + 既有 amas canary 回归（确认旧单 active 路径未破）：
  ```
  cargo test -p learning-backend --test admin_amas_http 2>&1 | tail -20
  ```
  预期：新增 2 个 + 既有 amas http 测试全绿。
- [ ] clippy 收口（仅本模块改动文件）：
  ```
  cargo clippy -p learning-backend --lib --tests 2>&1 | grep -E "warning|error" | head -20
  ```
  预期：无 error；新增代码无 clippy warning（如有 `needless_return`/`redundant_clone` 就地修）。
- [ ] commit（若 clippy 有修正）：
  ```
  git add -A
  git commit -m "chore(amas-canary): clippy 收口 + 模块D 回归通过"
  ```

---

附：模块D 关键路径与外部依赖（供编排重排时核对）
- 依赖共享契约里的 `PatchCanary` 结构与方法名（Task N+1 定义，模块 C6 其他端点如 `GET /advisor/canary` 复用本模块 store 方法）。
- 引用现有约定锚点（已核验）：`Store::aggregate_amas_version_slice`→`VersionMetricsSlice`（`amas_telemetry.rs:201/220`，已 `#[derive(Default)]`）；`apply_and_persist_config`/`write_path`（`amas.rs:378/626`，promote 复用 restore 通路）；`SseEvent::Incident { error_rate, window_secs }`（`state.rs:88`）；`add_job`/`WorkerName`/`planned_jobs`（`workers/mod.rs`）。
- 实现期需 grep 核对的占位（已在对应步骤标注）：engine 测试 helper 名（`build_test_engine`）、`state.store_arc()` 取 store 方法名、`TestApp` 的 `store`/`amas` 字段名、`InsertSuggestion` 字段集、`StoreError::Validation` 变体名。

## 模块 F — 前端 API client 底座 + 卡片/面板/历史组件

### Task F1: 前端 API client 方法 + TS interface（Module F 依赖底座）

新增 `admin-ui/src/api/admin.ts` 里 advisor/whitelist/canary/history 全套方法与 camelCase 类型，镜像现有 `amas*` 方法风格（`useAdminToken:true`），CSV 导出走 `buildUrl` + `tokenManager.getAdminToken()` 拿 `text/csv` 原文。所有组件测试 mock 这些方法名，故先落地签名。

**Files:**
- Modify: `admin-ui/src/api/admin.ts`（在 `amasSuggestionSpend`（行 299-300）之后、`// ─────────── m022:新增端点全集` 注释（行 302）之前插入新方法；类型 interface 追加到文件末尾 `AmasVersionSlice`（行 574-587）之后）
- Modify: `admin-ui/src/api/admin.ts`（顶部 import：行 1 `import { api } from './http';` 改为同时引入 `buildUrl`；新增 `import { tokenManager } from '@/lib/token';`）

步骤：

- [ ] 写失败测试：新建 `admin-ui/tests/api/amasAdvisor.api.test.ts`，断言 `adminApi` 暴露全部新方法且 `amasExportSuggestionsCsv` 用 fetch 取 text：

```ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { adminApi } from '@/api/admin';

describe('adminApi advisor/canary/whitelist 方法签名', () => {
  it('暴露 advisor 全套方法', () => {
    for (const m of [
      'amasAdvisorCost', 'amasAdvisorCostDaily', 'amasAdvisorRun', 'amasApproveAllSuggestions',
      'amasAdvisorConfig', 'amasUpdateAdvisorConfig', 'amasListWhitelist', 'amasAddWhitelist',
      'amasDeleteWhitelist', 'amasExportSuggestionsCsv', 'amasRollbackSuggestion',
      'amasListCanaries', 'amasCreateCanary', 'amasScaleCanary', 'amasRollbackCanary', 'amasPromoteCanary',
    ]) {
      expect(typeof (adminApi as unknown as Record<string, unknown>)[m]).toBe('function');
    }
  });

  describe('amasExportSuggestionsCsv', () => {
    const origFetch = globalThis.fetch;
    beforeEach(() => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: true,
        text: () => Promise.resolve('id,created_at\n1,2026-05-29'),
      } as unknown as Response);
    });
    afterEach(() => { globalThis.fetch = origFetch; });

    it('用 fetch 拿 csv 原文并带 status/q query', async () => {
      const csv = await adminApi.amasExportSuggestionsCsv('approved', 'memoryModel');
      expect(csv).toContain('id,created_at');
      const url = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
      expect(url).toContain('/api/admin/amas/suggestions/export.csv');
      expect(url).toContain('status=approved');
      expect(url).toContain('q=memoryModel');
    });
  });
});
```

- [ ] 跑确认失败：`cd admin-ui && npx vitest run tests/api/amasAdvisor.api.test.ts`
  预期：`TypeError: adminApi.amasAdvisorCost is not a function` / `expect(typeof ...).toBe('function')` 失败（方法未定义）。

- [ ] 改 import 头：把 `admin-ui/src/api/admin.ts` 行 1 的 `import { api } from './http';` 替换为：

```ts
import { api, buildUrl } from './http';
import { tokenManager } from '@/lib/token';
```

- [ ] 实现方法：在 `amasSuggestionSpend: () => ... (行 299-300)` 之后插入：

```ts
  // ─────────── advisor 成本/统计/巡查（C1-C2） ───────────
  amasAdvisorCost: () =>
    api.get<AdvisorCostStats>('/api/admin/amas/advisor/cost', undefined, { useAdminToken: true }),
  amasAdvisorCostDaily: (days = 30) =>
    api.get<AdvisorCostDaily[]>('/api/admin/amas/advisor/cost/daily', { days }, { useAdminToken: true }),
  amasAdvisorRun: () =>
    api.post<{ produced: boolean; suggestionId: number | null }>('/api/admin/amas/advisor/run', undefined, { useAdminToken: true }),
  amasApproveAllSuggestions: () =>
    api.post<{ results: Array<{ id: number; ok: boolean; error: string | null }> }>(
      '/api/admin/amas/suggestions/approve-all', undefined, { useAdminToken: true }),

  // ─────────── advisor 配置（C3） ───────────
  amasAdvisorConfig: () =>
    api.get<AdvisorConfig>('/api/admin/amas/advisor/config', undefined, { useAdminToken: true }),
  amasUpdateAdvisorConfig: (payload: Partial<Pick<AdvisorConfig,
    'monthCapYuan' | 'autoApplyEnabled' | 'autoApplyMaxPerDay' | 'autoApplyMinConfidence' | 'grayscaleSteps' | 'advisorEnabled'>>) =>
    api.put<AdvisorConfig>('/api/admin/amas/advisor/config', payload, { useAdminToken: true }),

  // ─────────── 白名单 CRUD（C4） ───────────
  amasListWhitelist: () =>
    api.get<WhitelistRow[]>('/api/admin/amas/advisor/whitelist', undefined, { useAdminToken: true }),
  amasAddWhitelist: (payload: { path: string; minSafe: number; maxSafe: number }) =>
    api.post<WhitelistRow>('/api/admin/amas/advisor/whitelist', payload, { useAdminToken: true }),
  amasDeleteWhitelist: (path: string) =>
    api.delete<{ deleted: boolean }>(`/api/admin/amas/advisor/whitelist/${encodeURIComponent(path)}`, { useAdminToken: true }),

  // ─────────── 历史增强（C5） ───────────
  amasRollbackSuggestion: (id: number) =>
    api.post<{ rolledBack: boolean; versionHash: string }>(`/api/admin/amas/suggestions/${id}/rollback`, undefined, { useAdminToken: true }),
  /** CSV 导出走原始 fetch（响应是 text/csv 非 JSON envelope，api.get 的 unwrap 会误解析）。 */
  amasExportSuggestionsCsv: async (status?: AmasSuggestionStatus, q?: string): Promise<string> => {
    const url = buildUrl('/api/admin/amas/suggestions/export.csv', { status, q });
    const token = tokenManager.getAdminToken();
    const res = await fetch(url, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
      credentials: 'include',
    });
    if (!res.ok) throw new Error(`导出失败 HTTP ${res.status}`);
    return res.text();
  },

  // ─────────── per-patch canary（C6） ───────────
  amasListCanaries: () =>
    api.get<PatchCanary[]>('/api/admin/amas/advisor/canary', undefined, { useAdminToken: true }),
  amasCreateCanary: (payload: { suggestionId: number; percent: number }) =>
    api.post<PatchCanary>('/api/admin/amas/advisor/canary', payload, { useAdminToken: true }),
  amasScaleCanary: (id: number, percent: number) =>
    api.post<PatchCanary>(`/api/admin/amas/advisor/canary/${id}/scale`, { percent }, { useAdminToken: true }),
  amasRollbackCanary: (id: number) =>
    api.post<{ rolledBack: boolean }>(`/api/admin/amas/advisor/canary/${id}/rollback`, undefined, { useAdminToken: true }),
  amasPromoteCanary: (id: number) =>
    api.post<{ promoted: boolean; versionHash: string }>(`/api/admin/amas/advisor/canary/${id}/promote`, undefined, { useAdminToken: true }),
```

- [ ] 扩展 `amasListSuggestions` 签名（兼容现调用，追加 offset/q）：把行 289-290 替换为：

```ts
  amasListSuggestions: (status?: AmasSuggestionStatus, limit = 50, offset = 0, q?: string) =>
    api.get<AmasSuggestion[]>('/api/admin/amas/suggestions', { status, limit, offset, q }, { useAdminToken: true }),
```

- [ ] 追加类型：在文件末尾 `AmasVersionSlice` interface（行 587 `}` 收尾）之后插入：

```ts
// ─────────── advisor 全栈对齐类型（camelCase 镜像后端序列化） ───────────
export interface AdvisorCostStats {
  monthYuan: number;
  monthCapYuan: number;
  quotaPct: number;
  forecastYuan: number;
  avg7dCostYuan: number;
  monthCalls: number;
  acceptedCount: number;
  rejectedCount: number;
  acceptanceRate: number;
}

export interface AdvisorCostDaily {
  date: string;
  costYuan: number;
}

export interface AdvisorConfig {
  model: string;
  pollCron: string;
  apiKeyTail: string;
  monthCapYuan: number;
  autoApplyEnabled: boolean;
  autoApplyMaxPerDay: number;
  autoApplyMinConfidence: number;
  grayscaleSteps: [number, number, number];
  advisorEnabled: boolean;
}

export interface WhitelistRow {
  path: string;
  minSafe: number;
  maxSafe: number;
}

export type PatchCanaryStatus = 'active' | 'effective' | 'rolled_back';

export interface PatchCanary {
  id: number;
  suggestionId: number;
  versionHash: string;
  percent: number;
  cohortLo: number;
  cohortHi: number;
  status: PatchCanaryStatus;
  baselineMetricsJson: string;
  startedAt: string;
  updatedAt: string;
  /** GET /advisor/canary 端点联表附带的实测口径 */
  liveReward: number;
  liveAnomalyRate: number;
  baselineReward: number;
}
```

- [ ] 确认 `api.delete` 存在；若 http.ts 无 `delete` 助手，则改用 `api.del`/补一行（先 grep）。跑：`cd admin-ui && grep -n "delete:\|del:" src/api/http.ts`。若都无，把 `amasDeleteWhitelist` 改为：`(path: string) => api.request<{ deleted: boolean }>('DELETE', \`/api/admin/amas/advisor/whitelist/${encodeURIComponent(path)}\`, undefined, { useAdminToken: true })`（对齐既有 http 导出的方法名）。

- [ ] 跑确认通过：`cd admin-ui && npx vitest run tests/api/amasAdvisor.api.test.ts`
  预期：`Test Files 1 passed (1)` / `Tests 2 passed`。

- [ ] 类型检查：`cd admin-ui && npx tsc --noEmit`，预期无 error（确认 `grayscaleSteps` 元组与现调用兼容）。

- [ ] commit：
```
git add admin-ui/src/api/admin.ts admin-ui/tests/api/amasAdvisor.api.test.ts
git commit -m "feat(admin-ui): advisor/canary/whitelist/history API client 方法 + camelCase 类型"
```

---

### Task F2: SuggestionCard 扩展（三联影响 + 白名单内外风险 + 进灰度按钮）

把现有 `AmasAdvisorPage.tsx` 内联的 `SuggestionCard`（行 287-376）抽到独立文件 `admin-ui/src/pages/amas-advisor/SuggestionCard.tsx` 并扩展：①从 `evidenceJson` 读疲劳率/正确率/留存三联预估影响（缺字段显 "—"）②每个 patch path 查 `WhitelistRow[]`（props 传入）判断是否在白名单内、新值是否越界 `[minSafe,maxSafe]`，越界标红风险 badge ③保留 approve/reject，新增"进灰度 20%"按钮触发 `onCanary`。

**Files:**
- Create: `admin-ui/src/pages/amas-advisor/SuggestionCard.tsx`
- Create: `admin-ui/tests/pages/amas-advisor/SuggestionCard.test.tsx`

步骤：

- [ ] 写失败测试 `admin-ui/tests/pages/amas-advisor/SuggestionCard.test.tsx`：

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { SuggestionCard } from '@/pages/amas-advisor/SuggestionCard';
import type { AmasSuggestion, WhitelistRow } from '@/api/admin';

const base: AmasSuggestion = {
  id: 7, createdAt: '2026-05-29T10:00:00Z',
  basedOnVersionHash: 'abc1234567def890',
  patchJson: { 'memoryModel.baseDesiredRetention': 0.97 },
  rationale: '提升目标留存',
  evidenceJson: { fatigueDelta: -0.03, accuracyDelta: 0.012, retentionDelta: -0.005 },
  status: 'pending', decidedBy: null, decidedAt: null, decisionNote: null,
  costUsd: 0.01, tokensInput: 100, tokensOutput: 80, confidence: 0.9,
  baseValuesJson: { 'memoryModel.baseDesiredRetention': 0.92 },
};
const whitelist: WhitelistRow[] = [
  { path: 'memoryModel.baseDesiredRetention', minSafe: 0.8, maxSafe: 0.95 },
];

function noop() {}

describe('SuggestionCard 扩展', () => {
  it('渲染三联预估影响（疲劳/正确率/留存）', () => {
    render(() => (
      <SuggestionCard s={base} whitelist={whitelist} busy={false}
        onApprove={noop} onReject={noop} onCanary={noop} />
    ));
    expect(screen.getByText('疲劳率')).toBeInTheDocument();
    expect(screen.getByText('正确率')).toBeInTheDocument();
    expect(screen.getByText('留存')).toBeInTheDocument();
    // accuracyDelta 0.012 → +1.2%
    expect(screen.getByText(/\+1\.2/)).toBeInTheDocument();
  });

  it('evidence 缺字段时三联显 —', () => {
    render(() => (
      <SuggestionCard s={{ ...base, evidenceJson: {} }} whitelist={whitelist} busy={false}
        onApprove={noop} onReject={noop} onCanary={noop} />
    ));
    expect(screen.getAllByText('—').length).toBeGreaterThanOrEqual(3);
  });

  it('新值越白名单上界 → 标"越界"风险', () => {
    // 0.97 > maxSafe 0.95
    render(() => (
      <SuggestionCard s={base} whitelist={whitelist} busy={false}
        onApprove={noop} onReject={noop} onCanary={noop} />
    ));
    expect(screen.getByText(/越界/)).toBeInTheDocument();
  });

  it('白名单外参数 → 标"白名单外"', () => {
    render(() => (
      <SuggestionCard s={{ ...base, patchJson: { 'ensemble.weight': 0.5 } }}
        whitelist={whitelist} busy={false}
        onApprove={noop} onReject={noop} onCanary={noop} />
    ));
    expect(screen.getByText(/白名单外/)).toBeInTheDocument();
  });

  it('点击"进灰度"触发 onCanary', () => {
    const onCanary = vi.fn();
    render(() => (
      <SuggestionCard s={base} whitelist={whitelist} busy={false}
        onApprove={noop} onReject={noop} onCanary={onCanary} />
    ));
    fireEvent.click(screen.getByText(/进灰度/));
    expect(onCanary).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] 跑确认失败：`cd admin-ui && npx vitest run tests/pages/amas-advisor/SuggestionCard.test.tsx`
  预期：`Failed to resolve import "@/pages/amas-advisor/SuggestionCard"`（文件不存在）。

- [ ] 实现 `admin-ui/src/pages/amas-advisor/SuggestionCard.tsx`：

```tsx
import { createMemo, createSignal, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import type { AmasSuggestion, AmasSuggestionStatus, WhitelistRow } from '@/api/admin';
import { formatMoney } from '@/utils/formatters';

const STATUS_LABEL: Record<AmasSuggestionStatus, string> = {
  pending: '待审批', approved: '已批准', rejected: '已拒绝',
  superseded: '已被覆盖', expired: '已过期', auto_applied: '自动应用',
};
const STATUS_VARIANT: Record<AmasSuggestionStatus, 'default' | 'success' | 'error' | 'warning' | 'info'> = {
  pending: 'warning', approved: 'success', rejected: 'error',
  superseded: 'default', expired: 'default', auto_applied: 'info',
};

/** evidence_json 三联影响字段（缺失显 "—"，不编造）。 */
const IMPACT_FIELDS: Array<{ key: string; label: string; goodWhenNegative?: boolean }> = [
  { key: 'fatigueDelta', label: '疲劳率', goodWhenNegative: true },
  { key: 'accuracyDelta', label: '正确率' },
  { key: 'retentionDelta', label: '留存' },
];

function fmtTime(iso: string): string {
  try { return new Date(iso).toLocaleString('zh-CN', { hour12: false }); } catch { return iso; }
}

function fmtPatchValue(value: unknown): string {
  if (typeof value !== 'number' || !Number.isFinite(value)) return String(value);
  if (value === 0) return '0';
  if (Math.abs(value) < 1e-4) return value.toExponential(2);
  return value.toPrecision(6).replace(/\.?0+$/, '');
}

type Risk = { kind: 'ok' | 'outside' | 'breach'; label: string };

function riskFor(path: string, value: unknown, whitelist: WhitelistRow[]): Risk {
  const row = whitelist.find((w) => w.path === path);
  if (!row) return { kind: 'outside', label: '白名单外' };
  if (typeof value === 'number' && Number.isFinite(value) && (value < row.minSafe || value > row.maxSafe)) {
    return { kind: 'breach', label: `越界 [${row.minSafe}, ${row.maxSafe}]` };
  }
  return { kind: 'ok', label: '白名单内' };
}

export function SuggestionCard(props: {
  s: AmasSuggestion;
  whitelist: WhitelistRow[];
  busy: boolean;
  onApprove: () => void;
  onReject: () => void;
  onCanary: () => void;
}) {
  const [showEvidence, setShowEvidence] = createSignal(false);

  // 整卡风险等级：任一 breach→error、任一 outside→warning、否则 ok
  const cardRisk = createMemo<Risk['kind']>(() => {
    const risks = Object.entries(props.s.patchJson).map(([p, v]) => riskFor(p, v, props.whitelist).kind);
    if (risks.includes('breach')) return 'breach';
    if (risks.includes('outside')) return 'outside';
    return 'ok';
  });

  function impactValue(key: string): { text: string; cls: string } {
    const raw = (props.s.evidenceJson as Record<string, unknown>)[key];
    if (typeof raw !== 'number' || !Number.isFinite(raw)) return { text: '—', cls: 'text-content-tertiary' };
    const pct = raw * 100;
    const field = IMPACT_FIELDS.find((f) => f.key === key)!;
    const good = field.goodWhenNegative ? raw <= 0 : raw >= 0;
    return {
      text: `${raw >= 0 ? '+' : ''}${pct.toFixed(1)}%`,
      cls: good ? 'text-success' : 'text-error',
    };
  }

  return (
    <Card variant="elevated">
      <div class="flex items-start justify-between gap-3 mb-2">
        <div class="flex items-center gap-2 flex-wrap">
          <Badge variant={STATUS_VARIANT[props.s.status]} size="sm">{STATUS_LABEL[props.s.status]}</Badge>
          <span class="text-xs font-mono text-content-tertiary">基于 {props.s.basedOnVersionHash.slice(0, 10)}</span>
          <span class="text-xs text-content-tertiary">{fmtTime(props.s.createdAt)}</span>
          <Show when={props.s.confidence != null}>
            <Badge variant="info" size="sm">置信 {(props.s.confidence! * 100).toFixed(0)}%</Badge>
          </Show>
          <Show when={cardRisk() === 'breach'}>
            <Badge variant="error" size="sm" dot>含越界参数</Badge>
          </Show>
          <Show when={cardRisk() === 'outside'}>
            <Badge variant="warning" size="sm" dot>含白名单外参数</Badge>
          </Show>
        </div>
        <div class="flex gap-2 shrink-0">
          <Button size="sm" variant="outline" loading={props.busy} onClick={props.onReject}>拒绝</Button>
          <Button size="sm" variant="secondary" loading={props.busy} onClick={props.onCanary}>进灰度 20%</Button>
          <Button size="sm" loading={props.busy} onClick={props.onApprove}>批准并应用</Button>
        </div>
      </div>

      <div class="text-sm text-content leading-relaxed mb-3">{props.s.rationale}</div>

      {/* 三联预估影响 */}
      <div class="grid grid-cols-3 gap-2 mb-3">
        <For each={IMPACT_FIELDS}>
          {(f) => {
            const v = impactValue(f.key);
            return (
              <div class="rounded-lg bg-surface-secondary px-3 py-2">
                <p class="text-[11px] text-content-tertiary">{f.label}</p>
                <p class={`text-sm font-medium tabular-nums ${v.cls}`}>{v.text}</p>
              </div>
            );
          }}
        </For>
      </div>

      {/* patch diff + 每行白名单内外 / 越界标记 */}
      <div class="space-y-1.5">
        <h4 class="text-xs font-medium text-content-secondary">
          Patch diff（{Object.keys(props.s.patchJson).length} 项 · 基于 {props.s.basedOnVersionHash.slice(0, 8)}）
        </h4>
        <table class="w-full text-xs font-mono">
          <thead>
            <tr class="text-content-tertiary border-b border-border-hairline">
              <th class="text-left py-1 pr-2">字段</th>
              <th class="text-right py-1 pr-2 w-20">旧值</th>
              <th class="text-center py-1 w-6"></th>
              <th class="text-right py-1 pl-2 w-20">建议值</th>
              <th class="text-left py-1 pl-3">风险</th>
            </tr>
          </thead>
          <tbody>
            <For each={Object.entries(props.s.patchJson)}>
              {([path, value]) => {
                const old = props.s.baseValuesJson?.[path];
                const oldNum = typeof old === 'number' && Number.isFinite(old) ? old : null;
                const risk = riskFor(path, value, props.whitelist);
                const riskCls = risk.kind === 'breach'
                  ? 'text-error' : risk.kind === 'outside' ? 'text-warning' : 'text-success';
                return (
                  <tr class="border-b border-border-hairline">
                    <td class="py-1 pr-2 text-content">{path}</td>
                    <td class="py-1 pr-2 text-right text-content-tertiary tabular-nums">
                      {oldNum != null ? fmtPatchValue(oldNum) : '—'}
                    </td>
                    <td class="py-1 text-center text-content-tertiary">→</td>
                    <td class="py-1 pl-2 text-right text-success tabular-nums">{fmtPatchValue(value)}</td>
                    <td class={`py-1 pl-3 ${riskCls}`}>{risk.label}</td>
                  </tr>
                );
              }}
            </For>
          </tbody>
        </table>
      </div>

      <div class="mt-2">
        <Button size="xs" variant="ghost" onClick={() => setShowEvidence(!showEvidence())}>
          {showEvidence() ? '隐藏' : '查看'} evidence
        </Button>
        <Show when={showEvidence()}>
          <pre class="mt-2 p-2 bg-surface-secondary rounded text-[10px] overflow-auto font-mono max-h-64">
            {JSON.stringify(props.s.evidenceJson, null, 2)}
          </pre>
        </Show>
      </div>
    </Card>
  );
}
```

- [ ] 跑确认通过：`cd admin-ui && npx vitest run tests/pages/amas-advisor/SuggestionCard.test.tsx`
  预期：`Tests 5 passed`。

- [ ] commit：
```
git add admin-ui/src/pages/amas-advisor/SuggestionCard.tsx admin-ui/tests/pages/amas-advisor/SuggestionCard.test.tsx
git commit -m "feat(admin-ui): SuggestionCard 扩展三联影响 + 白名单内外风险 + 进灰度按钮"
```

---

### Task F3: PatchCanaryCard（百分比条 + live stat-pill + 扩量/回滚/promote）

per-patch 灰度卡，区别于 `src/pages/amas/CanaryCard.tsx`（配置版本级单 active canary）。展示 `PatchCanary` 的百分比进度条、live reward/anomaly stat-pill（对比 baseline）、扩量（按 grayscaleSteps 下一档）/回滚/promote（仅 100% 可 promote）。

**Files:**
- Create: `admin-ui/src/pages/amas-advisor/PatchCanaryCard.tsx`
- Create: `admin-ui/tests/pages/amas-advisor/PatchCanaryCard.test.tsx`

步骤：

- [ ] 写失败测试 `admin-ui/tests/pages/amas-advisor/PatchCanaryCard.test.tsx`：

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { PatchCanaryCard } from '@/pages/amas-advisor/PatchCanaryCard';
import type { PatchCanary } from '@/api/admin';

const c: PatchCanary = {
  id: 3, suggestionId: 7, versionHash: 'deadbeef1234',
  percent: 20, cohortLo: 0, cohortHi: 20, status: 'active',
  baselineMetricsJson: '{"reward":0.5}',
  startedAt: '2026-05-29T10:00:00Z', updatedAt: '2026-05-29T10:05:00Z',
  liveReward: 0.55, liveAnomalyRate: 0.02, baselineReward: 0.5,
};

function noop() {}

describe('PatchCanaryCard', () => {
  it('渲染百分比 + live stat-pill', () => {
    render(() => (
      <PatchCanaryCard c={c} steps={[20, 60, 100]} busy={false}
        onScale={noop} onRollback={noop} onPromote={noop} />
    ));
    expect(screen.getByText('20%')).toBeInTheDocument();
    expect(screen.getByText(/实测 reward/)).toBeInTheDocument();
    // liveReward 0.55 > baseline 0.5 → 升幅 +0.05
    expect(screen.getByText(/\+0\.05/)).toBeInTheDocument();
  });

  it('扩量按钮传下一档百分比', () => {
    const onScale = vi.fn();
    render(() => (
      <PatchCanaryCard c={c} steps={[20, 60, 100]} busy={false}
        onScale={onScale} onRollback={noop} onPromote={noop} />
    ));
    fireEvent.click(screen.getByText(/扩量到 60%/));
    expect(onScale).toHaveBeenCalledWith(60);
  });

  it('100% 时显示 promote、隐藏扩量', () => {
    const onPromote = vi.fn();
    render(() => (
      <PatchCanaryCard c={{ ...c, percent: 100, cohortHi: 100 }} steps={[20, 60, 100]} busy={false}
        onScale={noop} onRollback={noop} onPromote={onPromote} />
    ));
    expect(screen.queryByText(/扩量到/)).toBeNull();
    fireEvent.click(screen.getByText(/提升为 stable/));
    expect(onPromote).toHaveBeenCalledTimes(1);
  });

  it('回滚按钮触发 onRollback', () => {
    const onRollback = vi.fn();
    render(() => (
      <PatchCanaryCard c={c} steps={[20, 60, 100]} busy={false}
        onScale={noop} onRollback={onRollback} onPromote={noop} />
    ));
    fireEvent.click(screen.getByText('回滚'));
    expect(onRollback).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] 跑确认失败：`cd admin-ui && npx vitest run tests/pages/amas-advisor/PatchCanaryCard.test.tsx`
  预期：`Failed to resolve import "@/pages/amas-advisor/PatchCanaryCard"`。

- [ ] 实现 `admin-ui/src/pages/amas-advisor/PatchCanaryCard.tsx`：

```tsx
import { createMemo, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import type { PatchCanary, PatchCanaryStatus } from '@/api/admin';

const STATUS_LABEL: Record<PatchCanaryStatus, string> = {
  active: '灰度中', effective: '已生效', rolled_back: '已回滚',
};
const STATUS_VARIANT: Record<PatchCanaryStatus, 'warning' | 'success' | 'error'> = {
  active: 'warning', effective: 'success', rolled_back: 'error',
};

function fmtDelta(v: number): string {
  return `${v >= 0 ? '+' : ''}${v.toFixed(2)}`;
}

export function PatchCanaryCard(props: {
  c: PatchCanary;
  steps: [number, number, number];
  busy: boolean;
  onScale: (percent: number) => void;
  onRollback: () => void;
  onPromote: () => void;
}) {
  // 下一档：steps 里第一个严格大于当前 percent 的档位
  const nextStep = createMemo<number | null>(() => {
    const s = props.steps.find((p) => p > props.c.percent);
    return s ?? null;
  });
  const rewardDelta = createMemo(() => props.c.liveReward - props.c.baselineReward);

  return (
    <Card variant="elevated">
      <div class="flex items-baseline justify-between gap-3 mb-2 flex-wrap">
        <div class="flex items-center gap-2">
          <span class="font-mono text-sm text-content">{props.c.versionHash.slice(0, 10)}</span>
          <Badge variant={STATUS_VARIANT[props.c.status]} size="sm" dot>{STATUS_LABEL[props.c.status]}</Badge>
          <span class="text-xs text-content-tertiary">建议 #{props.c.suggestionId}</span>
        </div>
        <span class="text-xs text-content-tertiary tabular-nums">
          cohort [{props.c.cohortLo}, {props.c.cohortHi})
        </span>
      </div>

      {/* 百分比条 */}
      <div class="mb-3">
        <div class="flex items-baseline justify-between mb-1">
          <span class="text-xs text-content-secondary">灰度覆盖</span>
          <span class="text-sm font-medium text-content tabular-nums">{props.c.percent}%</span>
        </div>
        <div class="h-2 rounded-full bg-surface-tertiary overflow-hidden">
          <div
            class="h-full rounded-full bg-gradient-accent-strong transition-[width] duration-base"
            style={{ width: `${props.c.percent}%` }}
          />
        </div>
      </div>

      {/* live stat-pill 对比 baseline */}
      <div class="grid grid-cols-2 gap-2 mb-3">
        <div class="rounded-lg bg-surface-secondary px-3 py-2">
          <p class="text-[11px] text-content-tertiary">实测 reward</p>
          <p class="text-sm font-medium tabular-nums text-content">
            {props.c.liveReward.toFixed(3)}
            <span class={`ml-1 text-xs ${rewardDelta() >= 0 ? 'text-success' : 'text-error'}`}>
              {fmtDelta(rewardDelta())}
            </span>
          </p>
        </div>
        <div class="rounded-lg bg-surface-secondary px-3 py-2">
          <p class="text-[11px] text-content-tertiary">实测异常率</p>
          <p class={`text-sm font-medium tabular-nums ${props.c.liveAnomalyRate > 0.05 ? 'text-error' : 'text-content'}`}>
            {(props.c.liveAnomalyRate * 100).toFixed(2)}%
          </p>
        </div>
      </div>

      <Show when={props.c.status === 'active'}>
        <div class="flex gap-2 justify-end">
          <Button size="sm" variant="outline" loading={props.busy} onClick={props.onRollback}>回滚</Button>
          <Show when={nextStep() != null && props.c.percent < 100}>
            <Button size="sm" variant="secondary" loading={props.busy} onClick={() => props.onScale(nextStep()!)}>
              扩量到 {nextStep()}%
            </Button>
          </Show>
          <Show when={props.c.percent >= 100}>
            <Button size="sm" loading={props.busy} onClick={props.onPromote}>提升为 stable</Button>
          </Show>
        </div>
      </Show>
    </Card>
  );
}
```

- [ ] 跑确认通过：`cd admin-ui && npx vitest run tests/pages/amas-advisor/PatchCanaryCard.test.tsx`
  预期：`Tests 4 passed`。

- [ ] commit：
```
git add admin-ui/src/pages/amas-advisor/PatchCanaryCard.tsx admin-ui/tests/pages/amas-advisor/PatchCanaryCard.test.tsx
git commit -m "feat(admin-ui): PatchCanaryCard 百分比条 + live stat-pill + 扩量/回滚/promote"
```

---

### Task F4: AdvisorConfigPanel（读写 advisor config，API Key 只读尾号）

读 `amasAdvisorConfig()` 渲染：只读字段（model、pollCron、apiKeyTail 脱敏）+ 可写字段（monthCapYuan、autoApply 三项、grayscaleSteps、advisorEnabled toggle）。保存调 `amasUpdateAdvisorConfig`，成功 toast + 回填。

**Files:**
- Create: `admin-ui/src/pages/amas-advisor/AdvisorConfigPanel.tsx`
- Create: `admin-ui/tests/pages/amas-advisor/AdvisorConfigPanel.test.tsx`

步骤：

- [ ] 写失败测试 `admin-ui/tests/pages/amas-advisor/AdvisorConfigPanel.test.tsx`：

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

vi.mock('@/api/admin', () => ({
  adminApi: { amasAdvisorConfig: vi.fn(), amasUpdateAdvisorConfig: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { AdvisorConfigPanel } from '@/pages/amas-advisor/AdvisorConfigPanel';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const cfg = {
  model: 'deepseek-chat', pollCron: '0 */20 * * * *', apiKeyTail: 'a1b2',
  monthCapYuan: 10, autoApplyEnabled: false, autoApplyMaxPerDay: 2,
  autoApplyMinConfidence: 0.85, grayscaleSteps: [20, 60, 100], advisorEnabled: true,
};

describe('AdvisorConfigPanel', () => {
  beforeEach(() => vi.clearAllMocks());

  it('渲染只读 model / 脱敏 API Key 尾号', async () => {
    mockApi.amasAdvisorConfig.mockResolvedValue(cfg);
    render(() => <AdvisorConfigPanel />);
    await waitFor(() => expect(screen.getByText('deepseek-chat')).toBeInTheDocument());
    expect(screen.getByText(/••••a1b2/)).toBeInTheDocument();
  });

  it('保存调 amasUpdateAdvisorConfig 带改动后的 monthCapYuan', async () => {
    mockApi.amasAdvisorConfig.mockResolvedValue(cfg);
    mockApi.amasUpdateAdvisorConfig.mockResolvedValue({ ...cfg, monthCapYuan: 20 });
    render(() => <AdvisorConfigPanel />);
    await waitFor(() => expect(screen.getByText('deepseek-chat')).toBeInTheDocument());
    const cap = screen.getByLabelText('月成本上限（¥）') as HTMLInputElement;
    fireEvent.input(cap, { target: { value: '20' } });
    fireEvent.click(screen.getByText('保存配置'));
    await waitFor(() => expect(mockApi.amasUpdateAdvisorConfig).toHaveBeenCalledWith(
      expect.objectContaining({ monthCapYuan: 20 }),
    ));
  });
});
```

- [ ] 跑确认失败：`cd admin-ui && npx vitest run tests/pages/amas-advisor/AdvisorConfigPanel.test.tsx`
  预期：`Failed to resolve import "@/pages/amas-advisor/AdvisorConfigPanel"`。

- [ ] 实现 `admin-ui/src/pages/amas-advisor/AdvisorConfigPanel.tsx`：

```tsx
import { createResource, createSignal, createEffect, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Switch } from '@/components/ui/Switch';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi, type AdvisorConfig } from '@/api/admin';
import { uiStore } from '@/stores/ui';

export function AdvisorConfigPanel() {
  const [cfg, { refetch }] = createResource(() => adminApi.amasAdvisorConfig());
  const [saving, setSaving] = createSignal(false);

  // 可写字段本地草稿，cfg 加载后同步一次
  const [monthCap, setMonthCap] = createSignal(0);
  const [autoApply, setAutoApply] = createSignal(false);
  const [maxPerDay, setMaxPerDay] = createSignal(0);
  const [minConf, setMinConf] = createSignal(0);
  const [steps, setSteps] = createSignal('20,60,100');
  const [enabled, setEnabled] = createSignal(false);

  createEffect(() => {
    const c = cfg();
    if (!c) return;
    setMonthCap(c.monthCapYuan);
    setAutoApply(c.autoApplyEnabled);
    setMaxPerDay(c.autoApplyMaxPerDay);
    setMinConf(c.autoApplyMinConfidence);
    setSteps(c.grayscaleSteps.join(','));
    setEnabled(c.advisorEnabled);
  });

  async function save() {
    const parsedSteps = steps().split(',').map((s) => parseInt(s.trim(), 10)).filter((n) => Number.isFinite(n));
    setSaving(true);
    try {
      await adminApi.amasUpdateAdvisorConfig({
        monthCapYuan: monthCap(),
        autoApplyEnabled: autoApply(),
        autoApplyMaxPerDay: maxPerDay(),
        autoApplyMinConfidence: minConf(),
        grayscaleSteps: parsedSteps.length === 3
          ? (parsedSteps as [number, number, number]) : undefined,
        advisorEnabled: enabled(),
      });
      uiStore.toast.success('顾问配置已保存');
      void refetch();
    } catch (e) {
      uiStore.toast.error('保存失败', e instanceof Error ? e.message : '');
    } finally {
      setSaving(false);
    }
  }

  return (
    <Card variant="elevated">
      <h2 class="text-headline text-content mb-3">顾问配置</h2>
      <Show when={!cfg.error} fallback={<Empty title="配置加载失败" description={cfg.error instanceof Error ? cfg.error.message : ''} />}>
        <Show when={cfg()} fallback={<div class="flex justify-center py-8"><Spinner size="sm" /></div>}>
          {(c) => (
            <div class="space-y-4">
              {/* 只读区 */}
              <div class="grid grid-cols-1 gap-2 text-sm">
                <div class="flex justify-between">
                  <span class="text-content-tertiary">模型</span>
                  <span class="text-content font-mono">{c().model}</span>
                </div>
                <div class="flex justify-between">
                  <span class="text-content-tertiary">巡查频率</span>
                  <span class="text-content font-mono">{c().pollCron}</span>
                </div>
                <div class="flex justify-between">
                  <span class="text-content-tertiary">API Key</span>
                  <span class="text-content font-mono">••••{c().apiKeyTail}</span>
                </div>
              </div>

              <div class="border-t border-border-hairline pt-3 space-y-3">
                <Switch checked={enabled()} onChange={setEnabled} label="启用自动巡查" />
                <Switch checked={autoApply()} onChange={setAutoApply} label="启用 auto-apply" />

                <label class="block">
                  <span class="text-xs text-content-secondary">月成本上限（¥）</span>
                  <input
                    type="number" step="0.01" min="0"
                    aria-label="月成本上限（¥）"
                    value={monthCap()}
                    onInput={(e) => setMonthCap(parseFloat(e.currentTarget.value) || 0)}
                    class="mt-1 w-full h-9 px-3 rounded-lg text-sm bg-surface text-content border border-border-hairline focus-ring-soft focus:border-accent"
                  />
                </label>
                <label class="block">
                  <span class="text-xs text-content-secondary">auto-apply 每日上限</span>
                  <input
                    type="number" step="1" min="0"
                    aria-label="auto-apply 每日上限"
                    value={maxPerDay()}
                    onInput={(e) => setMaxPerDay(parseInt(e.currentTarget.value, 10) || 0)}
                    class="mt-1 w-full h-9 px-3 rounded-lg text-sm bg-surface text-content border border-border-hairline focus-ring-soft focus:border-accent"
                  />
                </label>
                <label class="block">
                  <span class="text-xs text-content-secondary">auto-apply 最低置信度</span>
                  <input
                    type="number" step="0.01" min="0" max="1"
                    aria-label="auto-apply 最低置信度"
                    value={minConf()}
                    onInput={(e) => setMinConf(parseFloat(e.currentTarget.value) || 0)}
                    class="mt-1 w-full h-9 px-3 rounded-lg text-sm bg-surface text-content border border-border-hairline focus-ring-soft focus:border-accent"
                  />
                </label>
                <label class="block">
                  <span class="text-xs text-content-secondary">灰度档位（逗号分隔，3 档）</span>
                  <input
                    type="text"
                    aria-label="灰度档位"
                    value={steps()}
                    onInput={(e) => setSteps(e.currentTarget.value)}
                    class="mt-1 w-full h-9 px-3 rounded-lg text-sm bg-surface text-content border border-border-hairline font-mono focus-ring-soft focus:border-accent"
                  />
                </label>
              </div>

              <div class="flex justify-end">
                <Button size="sm" loading={saving()} onClick={save}>保存配置</Button>
              </div>
            </div>
          )}
        </Show>
      </Show>
    </Card>
  );
}
```

- [ ] 跑确认通过：`cd admin-ui && npx vitest run tests/pages/amas-advisor/AdvisorConfigPanel.test.tsx`
  预期：`Tests 2 passed`。

- [ ] commit：
```
git add admin-ui/src/pages/amas-advisor/AdvisorConfigPanel.tsx admin-ui/tests/pages/amas-advisor/AdvisorConfigPanel.test.tsx
git commit -m "feat(admin-ui): AdvisorConfigPanel 读写 advisor config(API Key 只读尾号)"
```

---

### Task F5: WhitelistPanel（列表 + 增删）

读 `amasListWhitelist()` 列出 11 条 `memoryModel.*`（path / minSafe / maxSafe），表单新增（path + 区间）、行级删除（带 `ConfirmDialog`）。

**Files:**
- Create: `admin-ui/src/pages/amas-advisor/WhitelistPanel.tsx`
- Create: `admin-ui/tests/pages/amas-advisor/WhitelistPanel.test.tsx`

步骤：

- [ ] 写失败测试 `admin-ui/tests/pages/amas-advisor/WhitelistPanel.test.tsx`：

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

vi.mock('@/api/admin', () => ({
  adminApi: { amasListWhitelist: vi.fn(), amasAddWhitelist: vi.fn(), amasDeleteWhitelist: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { WhitelistPanel } from '@/pages/amas-advisor/WhitelistPanel';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const rows = [
  { path: 'memoryModel.baseDesiredRetention', minSafe: 0.8, maxSafe: 0.95 },
  { path: 'memoryModel.w0', minSafe: 0.1, maxSafe: 2 },
];

describe('WhitelistPanel', () => {
  beforeEach(() => vi.clearAllMocks());

  it('渲染白名单条目', async () => {
    mockApi.amasListWhitelist.mockResolvedValue(rows);
    render(() => <WhitelistPanel />);
    await waitFor(() => expect(screen.getByText('memoryModel.baseDesiredRetention')).toBeInTheDocument());
    expect(screen.getByText('memoryModel.w0')).toBeInTheDocument();
  });

  it('新增条目调 amasAddWhitelist', async () => {
    mockApi.amasListWhitelist.mockResolvedValue(rows);
    mockApi.amasAddWhitelist.mockResolvedValue({ path: 'memoryModel.x', minSafe: 0, maxSafe: 1 });
    render(() => <WhitelistPanel />);
    await waitFor(() => expect(screen.getByText('memoryModel.w0')).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText('memoryModel.xxx'), { target: { value: 'memoryModel.x' } });
    fireEvent.input(screen.getByLabelText('min'), { target: { value: '0' } });
    fireEvent.input(screen.getByLabelText('max'), { target: { value: '1' } });
    fireEvent.click(screen.getByText('添加'));
    await waitFor(() => expect(mockApi.amasAddWhitelist).toHaveBeenCalledWith(
      { path: 'memoryModel.x', minSafe: 0, maxSafe: 1 },
    ));
  });

  it('删除走 ConfirmDialog 确认后调 amasDeleteWhitelist', async () => {
    mockApi.amasListWhitelist.mockResolvedValue(rows);
    mockApi.amasDeleteWhitelist.mockResolvedValue({ deleted: true });
    render(() => <WhitelistPanel />);
    await waitFor(() => expect(screen.getByText('memoryModel.w0')).toBeInTheDocument());
    fireEvent.click(screen.getAllByText('删除')[0]);
    fireEvent.click(screen.getByText('确认删除'));
    await waitFor(() => expect(mockApi.amasDeleteWhitelist).toHaveBeenCalledWith('memoryModel.baseDesiredRetention'));
  });
});
```

- [ ] 跑确认失败：`cd admin-ui && npx vitest run tests/pages/amas-advisor/WhitelistPanel.test.tsx`
  预期：`Failed to resolve import "@/pages/amas-advisor/WhitelistPanel"`。

- [ ] 实现 `admin-ui/src/pages/amas-advisor/WhitelistPanel.tsx`：

```tsx
import { createResource, createSignal, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { adminApi, type WhitelistRow } from '@/api/admin';
import { uiStore } from '@/stores/ui';

export function WhitelistPanel() {
  const [rows, { refetch }] = createResource(() => adminApi.amasListWhitelist());
  const [path, setPath] = createSignal('');
  const [minSafe, setMinSafe] = createSignal('');
  const [maxSafe, setMaxSafe] = createSignal('');
  const [adding, setAdding] = createSignal(false);
  const [delTarget, setDelTarget] = createSignal<WhitelistRow | null>(null);
  const [deleting, setDeleting] = createSignal(false);

  async function add() {
    const p = path().trim();
    const lo = parseFloat(minSafe());
    const hi = parseFloat(maxSafe());
    if (!p || !Number.isFinite(lo) || !Number.isFinite(hi)) {
      uiStore.toast.error('请填写合法 path 与区间');
      return;
    }
    if (lo > hi) {
      uiStore.toast.error('minSafe 不能大于 maxSafe');
      return;
    }
    setAdding(true);
    try {
      await adminApi.amasAddWhitelist({ path: p, minSafe: lo, maxSafe: hi });
      uiStore.toast.success('已添加白名单条目');
      setPath(''); setMinSafe(''); setMaxSafe('');
      void refetch();
    } catch (e) {
      uiStore.toast.error('添加失败', e instanceof Error ? e.message : '');
    } finally {
      setAdding(false);
    }
  }

  async function confirmDelete() {
    const t = delTarget();
    if (!t) return;
    setDeleting(true);
    try {
      await adminApi.amasDeleteWhitelist(t.path);
      uiStore.toast.success('已删除');
      setDelTarget(null);
      void refetch();
    } catch (e) {
      uiStore.toast.error('删除失败', e instanceof Error ? e.message : '');
    } finally {
      setDeleting(false);
    }
  }

  const inputCls = 'h-9 px-2 rounded-lg text-sm bg-surface text-content border border-border-hairline focus-ring-soft focus:border-accent';

  return (
    <Card variant="elevated">
      <h2 class="text-headline text-content mb-3">调参白名单</h2>
      <Show when={!rows.error} fallback={<Empty title="白名单加载失败" description={rows.error instanceof Error ? rows.error.message : ''} />}>
        <Show when={rows()} fallback={<div class="flex justify-center py-8"><Spinner size="sm" /></div>}>
          <Show when={(rows() ?? []).length > 0} fallback={<Empty title="白名单为空" description="启动 seed 应填充 TIER_A_WHITELIST" />}>
            <ul class="space-y-1.5 mb-4">
              <For each={rows() ?? []}>
                {(r) => (
                  <li class="flex items-center justify-between gap-2 text-sm py-1 border-b border-border-hairline last:border-b-0">
                    <span class="font-mono text-content truncate">{r.path}</span>
                    <span class="text-xs text-content-tertiary tabular-nums shrink-0">
                      [{r.minSafe}, {r.maxSafe}]
                    </span>
                    <Button size="xs" variant="ghost" onClick={() => setDelTarget(r)}>删除</Button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </Show>
      </Show>

      <div class="border-t border-border-hairline pt-3 grid grid-cols-[1fr_auto_auto_auto] gap-2 items-end">
        <input
          class={inputCls}
          placeholder="memoryModel.xxx"
          value={path()}
          onInput={(e) => setPath(e.currentTarget.value)}
        />
        <input
          class={`${inputCls} w-20`} type="number" step="0.01"
          aria-label="min" placeholder="min"
          value={minSafe()}
          onInput={(e) => setMinSafe(e.currentTarget.value)}
        />
        <input
          class={`${inputCls} w-20`} type="number" step="0.01"
          aria-label="max" placeholder="max"
          value={maxSafe()}
          onInput={(e) => setMaxSafe(e.currentTarget.value)}
        />
        <Button size="sm" loading={adding()} onClick={add}>添加</Button>
      </div>

      <ConfirmDialog
        open={!!delTarget()}
        title="确认删除白名单条目"
        message={<>将移除 <span class="font-mono">{delTarget()?.path}</span>，advisor 后续 patch 将拒绝该参数。</>}
        confirmText="确认删除"
        variant="danger"
        loading={deleting()}
        onConfirm={confirmDelete}
        onCancel={() => setDelTarget(null)}
      />
    </Card>
  );
}
```

- [ ] 跑确认通过：`cd admin-ui && npx vitest run tests/pages/amas-advisor/WhitelistPanel.test.tsx`
  预期：`Tests 3 passed`。

- [ ] commit：
```
git add admin-ui/src/pages/amas-advisor/WhitelistPanel.tsx admin-ui/tests/pages/amas-advisor/WhitelistPanel.test.tsx
git commit -m "feat(admin-ui): WhitelistPanel 列表 + 增删(danger 二次确认)"
```

---

### Task F6: HistoryTable（全宽表 + 搜索 q + 分页 offset + 导出 CSV + 行级回滚/查看）

全宽历史表：搜索框（q）、分页（offset，PAGE_SIZE=50）、导出 CSV（`amasExportSuggestionsCsv` 拿 text → Blob 下载）、行级"回滚"（`ConfirmDialog` → `amasRollbackSuggestion`）/"查看"（`Modal` 显示 patch/rationale）。用 `@/components/ui/Table` 渲染。

**Files:**
- Create: `admin-ui/src/pages/amas-advisor/HistoryTable.tsx`
- Create: `admin-ui/tests/pages/amas-advisor/HistoryTable.test.tsx`

步骤：

- [ ] 写失败测试 `admin-ui/tests/pages/amas-advisor/HistoryTable.test.tsx`：

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasListSuggestions: vi.fn(),
    amasRollbackSuggestion: vi.fn(),
    amasExportSuggestionsCsv: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { HistoryTable } from '@/pages/amas-advisor/HistoryTable';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const item = {
  id: 5, createdAt: '2026-05-29T10:00:00Z', basedOnVersionHash: 'abc1234567def890',
  patchJson: { 'memoryModel.w0': 0.5 }, rationale: '历史一条',
  evidenceJson: {}, status: 'approved' as const, decidedBy: 'admin@x.com',
  decidedAt: '2026-05-29T11:00:00Z', decisionNote: null,
  costUsd: 0.02, tokensInput: 100, tokensOutput: 80, confidence: 0.9, baseValuesJson: null,
};

describe('HistoryTable', () => {
  beforeEach(() => vi.clearAllMocks());

  it('渲染历史行 + 默认 offset=0 查询', async () => {
    mockApi.amasListSuggestions.mockResolvedValue([item]);
    render(() => <HistoryTable />);
    await waitFor(() => expect(screen.getByText('历史一条')).toBeInTheDocument());
    expect(mockApi.amasListSuggestions).toHaveBeenCalledWith(undefined, 50, 0, undefined);
  });

  it('搜索框输入 q 后重新查询', async () => {
    mockApi.amasListSuggestions.mockResolvedValue([item]);
    render(() => <HistoryTable />);
    await waitFor(() => expect(screen.getByText('历史一条')).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText('搜索参数 / rationale…'), { target: { value: 'w0' } });
    fireEvent.click(screen.getByText('搜索'));
    await waitFor(() => expect(mockApi.amasListSuggestions).toHaveBeenLastCalledWith(undefined, 50, 0, 'w0'));
  });

  it('行级回滚走 ConfirmDialog → amasRollbackSuggestion', async () => {
    mockApi.amasListSuggestions.mockResolvedValue([item]);
    mockApi.amasRollbackSuggestion.mockResolvedValue({ rolledBack: true, versionHash: 'newhash' });
    render(() => <HistoryTable />);
    await waitFor(() => expect(screen.getByText('历史一条')).toBeInTheDocument());
    fireEvent.click(screen.getByText('回滚'));
    fireEvent.click(screen.getByText('确认回滚'));
    await waitFor(() => expect(mockApi.amasRollbackSuggestion).toHaveBeenCalledWith(5));
  });

  it('导出 CSV 调 amasExportSuggestionsCsv', async () => {
    mockApi.amasListSuggestions.mockResolvedValue([item]);
    mockApi.amasExportSuggestionsCsv.mockResolvedValue('id,created_at\n5,2026-05-29');
    // jsdom 无 URL.createObjectURL，桩掉避免下载触发报错
    (globalThis.URL as unknown as Record<string, unknown>).createObjectURL = vi.fn(() => 'blob:x');
    (globalThis.URL as unknown as Record<string, unknown>).revokeObjectURL = vi.fn();
    render(() => <HistoryTable />);
    await waitFor(() => expect(screen.getByText('历史一条')).toBeInTheDocument());
    fireEvent.click(screen.getByText('导出 CSV'));
    await waitFor(() => expect(mockApi.amasExportSuggestionsCsv).toHaveBeenCalled());
  });
});
```

- [ ] 跑确认失败：`cd admin-ui && npx vitest run tests/pages/amas-advisor/HistoryTable.test.tsx`
  预期：`Failed to resolve import "@/pages/amas-advisor/HistoryTable"`。

- [ ] 实现 `admin-ui/src/pages/amas-advisor/HistoryTable.tsx`：

```tsx
import { createResource, createSignal, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Table } from '@/components/ui/Table';
import { Modal } from '@/components/ui/Modal';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { adminApi, type AmasSuggestion, type AmasSuggestionStatus } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { formatMoney } from '@/utils/formatters';

const PAGE_SIZE = 50;

const STATUS_LABEL: Record<AmasSuggestionStatus, string> = {
  pending: '待审批', approved: '已批准', rejected: '已拒绝',
  superseded: '已被覆盖', expired: '已过期', auto_applied: '自动应用',
};
const STATUS_VARIANT: Record<AmasSuggestionStatus, 'default' | 'success' | 'error' | 'warning' | 'info'> = {
  pending: 'warning', approved: 'success', rejected: 'error',
  superseded: 'default', expired: 'default', auto_applied: 'info',
};

function fmtTime(iso: string): string {
  try { return new Date(iso).toLocaleString('zh-CN', { hour12: false }); } catch { return iso; }
}

export function HistoryTable() {
  const [q, setQ] = createSignal('');
  const [applied, setApplied] = createSignal<{ q: string; offset: number }>({ q: '', offset: 0 });
  const [viewTarget, setViewTarget] = createSignal<AmasSuggestion | null>(null);
  const [rbTarget, setRbTarget] = createSignal<AmasSuggestion | null>(null);
  const [rbBusy, setRbBusy] = createSignal(false);
  const [exporting, setExporting] = createSignal(false);

  const [rows, { refetch }] = createResource(
    applied,
    (a) => adminApi.amasListSuggestions(undefined, PAGE_SIZE, a.offset, a.q || undefined),
  );

  function search() {
    setApplied({ q: q().trim(), offset: 0 });
  }
  function prevPage() {
    setApplied((a) => ({ ...a, offset: Math.max(0, a.offset - PAGE_SIZE) }));
  }
  function nextPage() {
    setApplied((a) => ({ ...a, offset: a.offset + PAGE_SIZE }));
  }

  async function confirmRollback() {
    const t = rbTarget();
    if (!t) return;
    setRbBusy(true);
    try {
      const r = await adminApi.amasRollbackSuggestion(t.id);
      uiStore.toast.success('已回滚', `恢复到 ${r.versionHash.slice(0, 10)}`);
      setRbTarget(null);
      void refetch();
    } catch (e) {
      uiStore.toast.error('回滚失败', e instanceof Error ? e.message : '');
    } finally {
      setRbBusy(false);
    }
  }

  async function exportCsv() {
    setExporting(true);
    try {
      const csv = await adminApi.amasExportSuggestionsCsv(undefined, applied().q || undefined);
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `amas-suggestions-${Date.now()}.csv`;
      a.click();
      URL.revokeObjectURL(url);
      uiStore.toast.success('已导出 CSV');
    } catch (e) {
      uiStore.toast.error('导出失败', e instanceof Error ? e.message : '');
    } finally {
      setExporting(false);
    }
  }

  return (
    <Card variant="elevated">
      <div class="flex items-center justify-between gap-3 mb-3 flex-wrap">
        <h2 class="text-headline text-content">建议历史</h2>
        <div class="flex items-center gap-2">
          <input
            class="h-9 px-3 rounded-lg text-sm bg-surface text-content border border-border-hairline focus-ring-soft focus:border-accent"
            placeholder="搜索参数 / rationale…"
            value={q()}
            onInput={(e) => setQ(e.currentTarget.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') search(); }}
          />
          <Button size="sm" variant="outline" onClick={search}>搜索</Button>
          <Button size="sm" variant="secondary" loading={exporting()} onClick={exportCsv}>导出 CSV</Button>
        </div>
      </div>

      <Table<AmasSuggestion>
        data={rows() ?? []}
        loading={rows.loading}
        emptyText="尚无历史建议"
        aria-label="AMAS 建议历史"
        columns={[
          { key: 'status', title: '状态', render: (r) => <Badge variant={STATUS_VARIANT[r.status]} size="sm">{STATUS_LABEL[r.status]}</Badge> },
          { key: 'createdAt', title: '时间', render: (r) => <span class="text-xs tabular-nums text-content-tertiary">{fmtTime(r.createdAt)}</span> },
          { key: 'basedOnVersionHash', title: '基于版本', render: (r) => <span class="font-mono text-xs">{r.basedOnVersionHash.slice(0, 10)}</span> },
          { key: 'rationale', title: '理由', render: (r) => <span class="text-xs text-content truncate block max-w-[20rem]" title={r.rationale}>{r.rationale}</span> },
          { key: 'costUsd', title: '成本', render: (r) => <span class="text-xs tabular-nums">{r.costUsd != null ? formatMoney(r.costUsd, 4) : '—'}</span> },
          { key: 'decidedBy', title: '决策人', render: (r) => <span class="text-xs text-content-tertiary">{r.decidedBy ?? '—'}</span> },
          {
            key: '_ops', title: '操作', render: (r) => (
              <div class="flex gap-1">
                <Button size="xs" variant="ghost" onClick={() => setViewTarget(r)}>查看</Button>
                <Show when={r.status === 'approved' || r.status === 'auto_applied'}>
                  <Button size="xs" variant="ghost" onClick={() => setRbTarget(r)}>回滚</Button>
                </Show>
              </div>
            ),
          },
        ]}
      />

      <div class="flex items-center justify-between mt-3">
        <span class="text-xs text-content-tertiary">offset {applied().offset}</span>
        <div class="flex gap-2">
          <Button size="sm" variant="ghost" disabled={applied().offset === 0} onClick={prevPage}>上一页</Button>
          <Button size="sm" variant="ghost" disabled={(rows() ?? []).length < PAGE_SIZE} onClick={nextPage}>下一页</Button>
        </div>
      </div>

      {/* 查看详情 */}
      <Modal open={!!viewTarget()} onClose={() => setViewTarget(null)} title="建议详情" size="lg">
        <Show when={viewTarget()}>
          {(t) => (
            <div class="space-y-3 text-sm">
              <p class="text-content">{t().rationale}</p>
              <pre class="p-2 bg-surface-secondary rounded text-[11px] overflow-auto font-mono max-h-64">
                {JSON.stringify(t().patchJson, null, 2)}
              </pre>
            </div>
          )}
        </Show>
      </Modal>

      {/* 回滚确认 */}
      <ConfirmDialog
        open={!!rbTarget()}
        title="确认回滚该建议"
        message={<>将基于版本链 restore 回滚到 <span class="font-mono">{rbTarget()?.basedOnVersionHash.slice(0, 10)}</span> 的父版本。</>}
        confirmText="确认回滚"
        variant="warning"
        loading={rbBusy()}
        onConfirm={confirmRollback}
        onCancel={() => setRbTarget(null)}
      />
    </Card>
  );
}
```

- [ ] 跑确认通过：`cd admin-ui && npx vitest run tests/pages/amas-advisor/HistoryTable.test.tsx`
  预期：`Tests 4 passed`。

- [ ] commit：
```
git add admin-ui/src/pages/amas-advisor/HistoryTable.tsx admin-ui/tests/pages/amas-advisor/HistoryTable.test.tsx
git commit -m "feat(admin-ui): HistoryTable 全宽表 + 搜索/分页/导出 CSV/行级回滚查看"
```

---

### Task F7: Module F 关键路径 features 测试（审批 → 进灰度 → 扩量 → 回滚）

跨组件 features 测试，仿 `AmasAdvisorPage.features.test.tsx`，验证 SuggestionCard 的"进灰度"→ PatchCanaryCard"扩量"→"回滚"在装配上下文里的回调串联（用一个轻量 harness 组件直接组合三组件 + mock API，不依赖 AmasAdvisorPage 重写完成，便于本模块独立交付）。

**Files:**
- Create: `admin-ui/tests/pages/amas-advisor/AdvisorFlow.features.test.tsx`

步骤：

- [ ] 写测试 `admin-ui/tests/pages/amas-advisor/AdvisorFlow.features.test.tsx`：

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import { createSignal, Show } from 'solid-js';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasCreateCanary: vi.fn(),
    amasScaleCanary: vi.fn(),
    amasRollbackCanary: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi, type AmasSuggestion, type PatchCanary, type WhitelistRow } from '@/api/admin';
import { SuggestionCard } from '@/pages/amas-advisor/SuggestionCard';
import { PatchCanaryCard } from '@/pages/amas-advisor/PatchCanaryCard';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const sug: AmasSuggestion = {
  id: 9, createdAt: '2026-05-29T10:00:00Z', basedOnVersionHash: 'abc1234567def890',
  patchJson: { 'memoryModel.baseDesiredRetention': 0.9 }, rationale: '关键路径建议',
  evidenceJson: {}, status: 'pending', decidedBy: null, decidedAt: null, decisionNote: null,
  costUsd: 0.01, tokensInput: 1, tokensOutput: 1, confidence: 0.9,
  baseValuesJson: { 'memoryModel.baseDesiredRetention': 0.88 },
};
const whitelist: WhitelistRow[] = [{ path: 'memoryModel.baseDesiredRetention', minSafe: 0.8, maxSafe: 0.95 }];

// 轻量装配 harness：进灰度 → 出现 canary 卡 → 扩量/回滚
function Harness() {
  const [canary, setCanary] = createSignal<PatchCanary | null>(null);
  async function onCanary() {
    const c = await adminApi.amasCreateCanary({ suggestionId: sug.id, percent: 20 });
    setCanary(c);
  }
  async function onScale(percent: number) {
    const c = await adminApi.amasScaleCanary(canary()!.id, percent);
    setCanary(c);
  }
  async function onRollback() {
    await adminApi.amasRollbackCanary(canary()!.id);
    setCanary((c) => (c ? { ...c, status: 'rolled_back' } : c));
  }
  return (
    <div>
      <SuggestionCard s={sug} whitelist={whitelist} busy={false}
        onApprove={() => {}} onReject={() => {}} onCanary={onCanary} />
      <Show when={canary()}>
        {(c) => (
          <PatchCanaryCard c={c()} steps={[20, 60, 100]} busy={false}
            onScale={onScale} onRollback={onRollback} onPromote={() => {}} />
        )}
      </Show>
    </div>
  );
}

const baseCanary: PatchCanary = {
  id: 11, suggestionId: 9, versionHash: 'deadbeef1234', percent: 20,
  cohortLo: 0, cohortHi: 20, status: 'active', baselineMetricsJson: '{}',
  startedAt: '2026-05-29T10:00:00Z', updatedAt: '2026-05-29T10:00:00Z',
  liveReward: 0.5, liveAnomalyRate: 0.01, baselineReward: 0.5,
};

describe('Advisor 关键路径：进灰度 → 扩量 → 回滚', () => {
  beforeEach(() => vi.clearAllMocks());

  it('完整串联', async () => {
    mockApi.amasCreateCanary.mockResolvedValue(baseCanary);
    mockApi.amasScaleCanary.mockResolvedValue({ ...baseCanary, percent: 60, cohortHi: 60 });
    mockApi.amasRollbackCanary.mockResolvedValue({ rolledBack: true });

    render(() => <Harness />);

    // 1. 进灰度
    fireEvent.click(screen.getByText(/进灰度/));
    await waitFor(() => expect(screen.getByText('灰度中')).toBeInTheDocument());
    expect(mockApi.amasCreateCanary).toHaveBeenCalledWith({ suggestionId: 9, percent: 20 });

    // 2. 扩量到 60%
    fireEvent.click(screen.getByText(/扩量到 60%/));
    await waitFor(() => expect(screen.getByText('60%')).toBeInTheDocument());
    expect(mockApi.amasScaleCanary).toHaveBeenCalledWith(11, 60);

    // 3. 回滚
    fireEvent.click(screen.getByText('回滚'));
    await waitFor(() => expect(mockApi.amasRollbackCanary).toHaveBeenCalledWith(11));
  });
});
```

- [ ] 跑确认通过（组件已在前序 task 落地）：`cd admin-ui && npx vitest run tests/pages/amas-advisor/AdvisorFlow.features.test.tsx`
  预期：`Tests 1 passed`。

- [ ] 全量回归 Module F：`cd admin-ui && npx vitest run tests/pages/amas-advisor tests/api/amasAdvisor.api.test.ts`
  预期：6 个测试文件全 passed（SuggestionCard 5 + PatchCanaryCard 4 + AdvisorConfigPanel 2 + WhitelistPanel 3 + HistoryTable 4 + AdvisorFlow 1 + api 2）。

- [ ] commit：
```
git add admin-ui/tests/pages/amas-advisor/AdvisorFlow.features.test.tsx
git commit -m "test(admin-ui): advisor 关键路径 features 测试(进灰度→扩量→回滚)"
```

## 模块 E — 前端骨架与顶部（PageHeaderOps/CostRow/PatchTabs/CostChart + 整页装配）

### Task E1: PageHeaderOps 组件（自动巡查 toggle / 立即触发巡查 / 接受全部待审）

**Files:**
- Create: `admin-ui/src/pages/amas-advisor/PageHeaderOps.tsx`
- Test: `admin-ui/tests/pages/amas-advisor/PageHeaderOps.test.tsx`

> 前置：模块 F 的「Task: 前端 API client」已在 `admin-ui/src/api/admin.ts` 定义 `amasAdvisorRun()`、`amasApproveAllSuggestions()`、`amasUpdateAdvisorConfig({advisorEnabled})`、类型 `AdvisorConfig`。本组件是展示+回调型，不直接调 API，由父页注入回调，便于测试。

- [ ] **Step 1: 写失败测试**

```tsx
// admin-ui/tests/pages/amas-advisor/PageHeaderOps.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { screen, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';
import { PageHeaderOps } from '@/pages/amas-advisor/PageHeaderOps';

describe('PageHeaderOps', () => {
  it('渲染三个操作并回调', () => {
    const onToggle = vi.fn();
    const onRun = vi.fn();
    const onApproveAll = vi.fn();
    renderWithProviders(() => (
      <PageHeaderOps
        advisorEnabled={true}
        running={false}
        pendingCount={2}
        onToggleAutoScan={onToggle}
        onRunNow={onRun}
        onApproveAll={onApproveAll}
      />
    ));
    fireEvent.click(screen.getByRole('switch', { name: /自动巡查/ }));
    expect(onToggle).toHaveBeenCalledWith(false);
    fireEvent.click(screen.getByRole('button', { name: /立即触发巡查/ }));
    expect(onRun).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: /接受全部待审/ }));
    expect(onApproveAll).toHaveBeenCalled();
  });

  it('pendingCount 为 0 时禁用"接受全部待审"', () => {
    renderWithProviders(() => (
      <PageHeaderOps advisorEnabled={false} running={false} pendingCount={0}
        onToggleAutoScan={() => {}} onRunNow={() => {}} onApproveAll={() => {}} />
    ));
    expect(screen.getByRole('button', { name: /接受全部待审/ })).toBeDisabled();
  });
});
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd admin-ui && npx vitest run tests/pages/amas-advisor/PageHeaderOps.test.tsx`
Expected: FAIL — `Failed to resolve import "@/pages/amas-advisor/PageHeaderOps"`。

- [ ] **Step 3: 实现组件**

```tsx
// admin-ui/src/pages/amas-advisor/PageHeaderOps.tsx
import { Button } from '@/components/ui/Button';

export interface PageHeaderOpsProps {
  advisorEnabled: boolean;
  running: boolean;
  pendingCount: number;
  onToggleAutoScan: (next: boolean) => void;
  onRunNow: () => void;
  onApproveAll: () => void;
}

export function PageHeaderOps(props: PageHeaderOpsProps) {
  return (
    <div class="flex flex-wrap items-center gap-3">
      <button
        type="button"
        role="switch"
        aria-checked={props.advisorEnabled}
        aria-label="自动巡查"
        onClick={() => props.onToggleAutoScan(!props.advisorEnabled)}
        class={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
          props.advisorEnabled ? 'bg-accent' : 'bg-surface-secondary border border-border-hairline'
        }`}
      >
        <span class="text-[11px] absolute -top-4 left-0 whitespace-nowrap text-content-tertiary">自动巡查</span>
        <span class={`inline-block size-4 transform rounded-full bg-white transition-transform ${
          props.advisorEnabled ? 'translate-x-6' : 'translate-x-1'
        }`} />
      </button>
      <Button size="sm" variant="outline" loading={props.running} onClick={() => props.onRunNow()}>
        立即触发巡查
      </Button>
      <Button size="sm" disabled={props.pendingCount === 0} onClick={() => props.onApproveAll()}>
        接受全部待审{props.pendingCount > 0 ? ` (${props.pendingCount})` : ''}
      </Button>
    </div>
  );
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd admin-ui && npx vitest run tests/pages/amas-advisor/PageHeaderOps.test.tsx`
Expected: PASS（2 passed）。

- [ ] **Step 5: 提交**

```bash
git add admin-ui/src/pages/amas-advisor/PageHeaderOps.tsx admin-ui/tests/pages/amas-advisor/PageHeaderOps.test.tsx
git commit -m "feat(amas-advisor): PageHeaderOps 顶部操作区（自动巡查/触发/批量接受）"
```

---

### Task E2: CostRow 组件（4 卡 ¥月度 + 配额条 + 预测 + 接受率）

**Files:**
- Create: `admin-ui/src/pages/amas-advisor/CostRow.tsx`
- Test: `admin-ui/tests/pages/amas-advisor/CostRow.test.tsx`

> 数据来自 `amasAdvisorCost(): AdvisorCostStats { monthYuan, monthCapYuan, quotaPct, forecastYuan, avg7dCostYuan, monthCalls, acceptedCount, rejectedCount, acceptanceRate }`（模块 F 已定义类型与方法）。首卡含配额条（StatCard 无此槽，用自定义 Card），其余 3 张用 StatCard。

- [ ] **Step 1: 写失败测试**

```tsx
// admin-ui/tests/pages/amas-advisor/CostRow.test.tsx
import { describe, it, expect } from 'vitest';
import { screen } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';
import { CostRow } from '@/pages/amas-advisor/CostRow';
import type { AdvisorCostStats } from '@/api/admin';

const stats: AdvisorCostStats = {
  monthYuan: 4.21, monthCapYuan: 10, quotaPct: 42.1, forecastYuan: 6.84,
  avg7dCostYuan: 0.14, monthCalls: 31, acceptedCount: 47, rejectedCount: 6, acceptanceRate: 0.887,
};

describe('CostRow', () => {
  it('渲染本月成本/配额/接受率', () => {
    renderWithProviders(() => <CostRow stats={stats} />);
    expect(screen.getByText('¥4.21')).toBeInTheDocument();
    expect(screen.getByText(/¥10/)).toBeInTheDocument();
    expect(screen.getByText(/42\.1%/)).toBeInTheDocument();
    expect(screen.getByText('47/53')).toBeInTheDocument(); // accepted/(accepted+rejected)
  });

  it('配额条 width 反映 quotaPct', () => {
    const { container } = renderWithProviders(() => <CostRow stats={stats} />);
    const bar = container.querySelector('[data-testid="quota-bar-fill"]') as HTMLElement;
    expect(bar.style.width).toBe('42.1%');
  });
});
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd admin-ui && npx vitest run tests/pages/amas-advisor/CostRow.test.tsx`
Expected: FAIL — 无法解析 `@/pages/amas-advisor/CostRow`。

- [ ] **Step 3: 实现组件**

```tsx
// admin-ui/src/pages/amas-advisor/CostRow.tsx
import { Card } from '@/components/ui/Card';
import { StatCard } from '@/components/ui/StatCard';
import type { AdvisorCostStats } from '@/api/admin';

function yuan(v: number, d = 2): string {
  return `¥${v.toFixed(d)}`;
}

export function CostRow(props: { stats: AdvisorCostStats }) {
  const s = () => props.stats;
  const decided = () => s().acceptedCount + s().rejectedCount;
  return (
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
      {/* 首卡：本月成本 + 配额条 + 预测（自定义，StatCard 无配额条槽） */}
      <Card variant="elevated">
        <div class="flex flex-col gap-2">
          <span class="text-sm text-content-secondary">本月调用成本</span>
          <span class="text-2xl font-semibold tabular-nums text-accent">
            {yuan(s().monthYuan)}<span class="text-sm text-content-tertiary"> / {yuan(s().monthCapYuan)}</span>
          </span>
          <div class="h-1.5 rounded-full bg-surface-secondary overflow-hidden">
            <div
              data-testid="quota-bar-fill"
              class="h-full bg-accent transition-[width]"
              style={{ width: `${Math.min(s().quotaPct, 100)}%` }}
            />
          </div>
          <span class="text-[11.5px] text-content-tertiary tabular-nums">
            {s().quotaPct.toFixed(1)}% · 预计月底 {yuan(s().forecastYuan)}
          </span>
        </div>
      </Card>

      <StatCard
        title="7 天平均单次成本"
        value={yuan(s().avg7dCostYuan)}
        color="info"
        icon="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"
      />
      <StatCard
        title="本月调用次数"
        value={`${s().monthCalls}`}
        color="accent"
        icon="M13 10V3L4 14h7v7l9-11h-7z"
      />
      <StatCard
        title="累计 patch · 接受率"
        value={`${s().acceptedCount}/${decided()}`}
        color="success"
        icon="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
        trend={{ value: Math.round(s().acceptanceRate * 100), label: '接受率', showZero: true }}
      />
    </div>
  );
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd admin-ui && npx vitest run tests/pages/amas-advisor/CostRow.test.tsx`
Expected: PASS（2 passed）。

- [ ] **Step 5: 提交**

```bash
git add admin-ui/src/pages/amas-advisor/CostRow.tsx admin-ui/tests/pages/amas-advisor/CostRow.test.tsx
git commit -m "feat(amas-advisor): CostRow ¥月度成本看板（配额条+预测+接受率）"
```

---

### Task E3: PatchTabs 组件（4-tab 计数角标 + 下次巡查倒计时）

**Files:**
- Create: `admin-ui/src/pages/amas-advisor/PatchTabs.tsx`
- Test: `admin-ui/tests/pages/amas-advisor/PatchTabs.test.tsx`

> 通用 `Tabs` 的 `Tab` 无 count 字段、也无右侧倒计时槽，故 PatchTabs 自定义。倒计时按 20min cron 客户端计算（接收 `nowMs` 便于测试注入，默认 `Date.now()`）。

- [ ] **Step 1: 写失败测试**

```tsx
// admin-ui/tests/pages/amas-advisor/PatchTabs.test.tsx
import { describe, it, expect, vi } from 'vitest';
import { screen, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';
import { PatchTabs, type PatchTabId } from '@/pages/amas-advisor/PatchTabs';

const counts = { pending: 2, canary: 3, effective: 42, rejected: 6 };

describe('PatchTabs', () => {
  it('四 tab 含计数角标', () => {
    renderWithProviders(() => (
      <PatchTabs active="pending" counts={counts} onChange={() => {}} nowMs={0} />
    ));
    expect(screen.getByRole('tab', { name: /待审.*2/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /灰度中.*3/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /已生效.*42/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /已拒绝.*6/ })).toBeInTheDocument();
  });

  it('点击切换回调', () => {
    const onChange = vi.fn();
    renderWithProviders(() => (
      <PatchTabs active="pending" counts={counts} onChange={onChange} nowMs={0} />
    ));
    fireEvent.click(screen.getByRole('tab', { name: /灰度中/ }));
    expect(onChange).toHaveBeenCalledWith('canary' satisfies PatchTabId);
  });

  it('倒计时按 20min 周期显示剩余', () => {
    // nowMs = 第 12 分钟 → 距下次巡查 8 分 0 秒
    const twelveMin = 12 * 60 * 1000;
    renderWithProviders(() => (
      <PatchTabs active="pending" counts={counts} onChange={() => {}} nowMs={twelveMin} />
    ));
    expect(screen.getByText(/下次巡查/)).toHaveTextContent(/8 分/);
  });
});
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd admin-ui && npx vitest run tests/pages/amas-advisor/PatchTabs.test.tsx`
Expected: FAIL — 无法解析 `@/pages/amas-advisor/PatchTabs`。

- [ ] **Step 3: 实现组件**

```tsx
// admin-ui/src/pages/amas-advisor/PatchTabs.tsx
import { For } from 'solid-js';

export type PatchTabId = 'pending' | 'canary' | 'effective' | 'rejected';

export interface PatchCounts {
  pending: number;
  canary: number;
  effective: number;
  rejected: number;
}

const POLL_PERIOD_MS = 20 * 60 * 1000;

const TABS: Array<{ id: PatchTabId; label: string }> = [
  { id: 'pending', label: '待审' },
  { id: 'canary', label: '灰度中' },
  { id: 'effective', label: '已生效' },
  { id: 'rejected', label: '已拒绝' },
];

function countdownText(nowMs: number): string {
  const remain = POLL_PERIOD_MS - (nowMs % POLL_PERIOD_MS);
  const totalSec = Math.ceil(remain / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m} 分 ${s} 秒`;
}

export function PatchTabs(props: {
  active: PatchTabId;
  counts: PatchCounts;
  onChange: (id: PatchTabId) => void;
  /** 测试可注入；默认 Date.now() */
  nowMs?: number;
}) {
  const now = () => props.nowMs ?? Date.now();
  return (
    <div class="flex items-center justify-between border-b border-border-hairline">
      <div role="tablist" class="flex gap-1">
        <For each={TABS}>
          {(t) => (
            <button
              type="button"
              role="tab"
              aria-selected={props.active === t.id}
              onClick={() => props.onChange(t.id)}
              class={`px-3 py-2 text-sm flex items-center gap-1.5 border-b-2 -mb-px transition-colors ${
                props.active === t.id
                  ? 'border-accent text-accent'
                  : 'border-transparent text-content-secondary hover:text-content'
              }`}
            >
              <span>{t.label}</span>
              <span class="text-[11px] tabular-nums px-1.5 rounded-full bg-surface-secondary text-content-tertiary">
                {props.counts[t.id]}
              </span>
            </button>
          )}
        </For>
      </div>
      <span class="text-[11.5px] text-content-tertiary tabular-nums pr-1">
        下次巡查 · {countdownText(now())}
      </span>
    </div>
  );
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd admin-ui && npx vitest run tests/pages/amas-advisor/PatchTabs.test.tsx`
Expected: PASS（3 passed）。

- [ ] **Step 5: 提交**

```bash
git add admin-ui/src/pages/amas-advisor/PatchTabs.tsx admin-ui/tests/pages/amas-advisor/PatchTabs.test.tsx
git commit -m "feat(amas-advisor): PatchTabs 四态 tab 计数 + 巡查倒计时"
```

---

### Task E4: CostChart 组件（30 天成本柱图 + 参考线）

**Files:**
- Create: `admin-ui/src/pages/amas-advisor/CostChart.tsx`
- Test: `admin-ui/tests/pages/amas-advisor/CostChart.test.tsx`

> 数据来自 `amasAdvisorCostDaily(30): AdvisorCostDaily[] { date, costYuan }`（模块 F 已定义）。用内联 SVG 柱状（不引 echarts，保持轻量、可测）。

- [ ] **Step 1: 写失败测试**

```tsx
// admin-ui/tests/pages/amas-advisor/CostChart.test.tsx
import { describe, it, expect } from 'vitest';
import { screen } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';
import { CostChart } from '@/pages/amas-advisor/CostChart';
import type { AdvisorCostDaily } from '@/api/admin';

const data: AdvisorCostDaily[] = Array.from({ length: 30 }, (_, i) => ({
  date: `2026-05-${String(i + 1).padStart(2, '0')}`,
  costYuan: (i % 5) * 0.05,
}));

describe('CostChart', () => {
  it('渲染 30 根柱 + 参考线 + footer', () => {
    const { container } = renderWithProviders(() => (
      <CostChart data={data} avg7dYuan={0.14} capYuan={10} refLineYuan={0.3} />
    ));
    expect(container.querySelectorAll('rect[data-bar]').length).toBe(30);
    expect(screen.getByText(/7 天平均/)).toHaveTextContent('¥0.14');
    expect(screen.getByText(/月度上限/)).toHaveTextContent('¥10');
  });

  it('空数据显示占位', () => {
    renderWithProviders(() => <CostChart data={[]} avg7dYuan={0} capYuan={10} refLineYuan={0.3} />);
    expect(screen.getByText(/暂无成本数据/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd admin-ui && npx vitest run tests/pages/amas-advisor/CostChart.test.tsx`
Expected: FAIL — 无法解析 `@/pages/amas-advisor/CostChart`。

- [ ] **Step 3: 实现组件**

```tsx
// admin-ui/src/pages/amas-advisor/CostChart.tsx
import { For, Show, createMemo } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Empty } from '@/components/ui/Empty';
import type { AdvisorCostDaily } from '@/api/admin';

const W = 300;
const H = 140;
const PAD = 4;

export function CostChart(props: {
  data: AdvisorCostDaily[];
  avg7dYuan: number;
  capYuan: number;
  refLineYuan: number;
}) {
  const max = createMemo(() => Math.max(props.refLineYuan, ...props.data.map((d) => d.costYuan), 0.01));
  const barW = createMemo(() => (W - PAD * 2) / Math.max(props.data.length, 1));
  const refY = createMemo(() => H - PAD - (props.refLineYuan / max()) * (H - PAD * 2));
  return (
    <Card variant="elevated">
      <h4 class="text-sm font-medium text-content-secondary mb-2">调用成本 · 30 天</h4>
      <Show when={props.data.length > 0} fallback={<Empty title="暂无成本数据" description="" />}>
        <svg viewBox={`0 0 ${W} ${H}`} class="w-full" role="img" aria-label="30 天调用成本柱状图">
          <line x1={PAD} x2={W - PAD} y1={refY()} y2={refY()}
            stroke="var(--border-hairline)" stroke-dasharray="3 3" />
          <For each={props.data}>
            {(d, i) => {
              const h = () => (d.costYuan / max()) * (H - PAD * 2);
              return (
                <rect
                  data-bar
                  x={PAD + i() * barW() + 0.5}
                  y={H - PAD - h()}
                  width={Math.max(barW() - 1, 0.5)}
                  height={h()}
                  fill="var(--accent)"
                  rx="0.5"
                />
              );
            }}
          </For>
        </svg>
        <div class="flex justify-between text-[11px] text-content-tertiary tabular-nums mt-1">
          <span>7 天平均 ¥{props.avg7dYuan.toFixed(2)}</span>
          <span>月度上限 ¥{props.capYuan.toFixed(0)}</span>
        </div>
      </Show>
    </Card>
  );
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd admin-ui && npx vitest run tests/pages/amas-advisor/CostChart.test.tsx`
Expected: PASS（2 passed）。

- [ ] **Step 5: 提交**

```bash
git add admin-ui/src/pages/amas-advisor/CostChart.tsx admin-ui/tests/pages/amas-advisor/CostChart.test.tsx
git commit -m "feat(amas-advisor): CostChart 30 天成本柱图 + 参考线"
```

---

### Task E5: AmasAdvisorPage 重写为 12 栅格双栏外壳（装配全部组件）

**Files:**
- Modify: `admin-ui/src/pages/AmasAdvisorPage.tsx`（整文件重写）
- Test: `admin-ui/tests/pages/AmasAdvisorPage.test.tsx`（重写以适配新结构）

> 装配：HeroCard + PageHeaderOps（顶部）→ CostRow（全宽）→ PatchTabs（4 态）→ 主体 12 栅格双栏（左 span-8：SuggestionCard 流 / PatchCanaryCard 流；右 span-4：CostChart / AdvisorConfigPanel / WhitelistPanel）→ HistoryTable（全宽）。SuggestionCard/PatchCanaryCard/AdvisorConfigPanel/WhitelistPanel/HistoryTable 来自模块 F。counts 由本页从 resource 派生传入 PatchTabs。
> 依赖模块 F 的组件与模块 F/全部 API 方法。本任务在模块 F 完成后执行（plan 末尾排序保证）。

- [ ] **Step 1: 写失败测试（覆盖新结构关键渲染）**

```tsx
// admin-ui/tests/pages/AmasAdvisorPage.test.tsx —— 重写
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasListSuggestions: vi.fn(),
    amasListCanaries: vi.fn(),
    amasAdvisorCost: vi.fn(),
    amasAdvisorCostDaily: vi.fn(),
    amasAdvisorConfig: vi.fn(),
    amasListWhitelist: vi.fn(),
    amasAdvisorRun: vi.fn(),
    amasApproveAllSuggestions: vi.fn(),
    amasUpdateAdvisorConfig: vi.fn(),
    amasApproveSuggestion: vi.fn(),
    amasRejectSuggestion: vi.fn(),
    amasCreateCanary: vi.fn(),
    amasScaleCanary: vi.fn(),
    amasRollbackCanary: vi.fn(),
    amasPromoteCanary: vi.fn(),
    amasRollbackSuggestion: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import AmasAdvisorPage from '@/pages/AmasAdvisorPage';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const cost = {
  monthYuan: 4.21, monthCapYuan: 10, quotaPct: 42.1, forecastYuan: 6.84,
  avg7dCostYuan: 0.14, monthCalls: 31, acceptedCount: 47, rejectedCount: 6, acceptanceRate: 0.887,
};
const cfg = {
  model: 'deepseek-v2', pollCron: '0 */20 * * * *', apiKeyTail: 'f3a8', monthCapYuan: 10,
  autoApplyEnabled: false, autoApplyMaxPerDay: 1, autoApplyMinConfidence: 0.8,
  grayscaleSteps: [20, 60, 100], advisorEnabled: true,
};

describe('AmasAdvisorPage（重设计）', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApi.amasListSuggestions.mockResolvedValue([]);
    mockApi.amasListCanaries.mockResolvedValue([]);
    mockApi.amasAdvisorCost.mockResolvedValue(cost);
    mockApi.amasAdvisorCostDaily.mockResolvedValue([]);
    mockApi.amasAdvisorConfig.mockResolvedValue(cfg);
    mockApi.amasListWhitelist.mockResolvedValue([]);
  });

  it('渲染 hero + 成本行 + 四态 tab', async () => {
    renderWithProviders(() => <AmasAdvisorPage />);
    expect(await screen.findByText('LLM 调参顾问')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText('¥4.21')).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: /待审/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /灰度中/ })).toBeInTheDocument();
  });

  it('成本接口失败时成本行降级而不整页崩', async () => {
    mockApi.amasAdvisorCost.mockRejectedValue(new Error('boom'));
    renderWithProviders(() => <AmasAdvisorPage />);
    expect(await screen.findByText('LLM 调参顾问')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText(/成本信息加载失败/)).toBeInTheDocument());
  });
});
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd admin-ui && npx vitest run tests/pages/AmasAdvisorPage.test.tsx`
Expected: FAIL —旧页无 `amasAdvisorCost` 调用、无四态 tab、找不到 `¥4.21`/`灰度中`。

- [ ] **Step 3: 重写 AmasAdvisorPage.tsx**

```tsx
// admin-ui/src/pages/AmasAdvisorPage.tsx —— 整文件重写
import { createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { HeroCard } from '@/components/ui/HeroCard';
import { uiStore } from '@/stores/ui';
import { adminApi, type AmasSuggestion } from '@/api/admin';
import { PageHeaderOps } from '@/pages/amas-advisor/PageHeaderOps';
import { CostRow } from '@/pages/amas-advisor/CostRow';
import { PatchTabs, type PatchTabId } from '@/pages/amas-advisor/PatchTabs';
import { CostChart } from '@/pages/amas-advisor/CostChart';
import { SuggestionCard } from '@/pages/amas-advisor/SuggestionCard';
import { PatchCanaryCard } from '@/pages/amas-advisor/PatchCanaryCard';
import { AdvisorConfigPanel } from '@/pages/amas-advisor/AdvisorConfigPanel';
import { WhitelistPanel } from '@/pages/amas-advisor/WhitelistPanel';
import { HistoryTable } from '@/pages/amas-advisor/HistoryTable';

export default function AmasAdvisorPage() {
  const [tab, setTab] = createSignal<PatchTabId>('pending');
  const [running, setRunning] = createSignal(false);

  const [cost, { refetch: refetchCost }] = createResource(() => adminApi.amasAdvisorCost());
  const [costDaily] = createResource(() => adminApi.amasAdvisorCostDaily(30));
  const [config, { refetch: refetchConfig }] = createResource(() => adminApi.amasAdvisorConfig());
  const [pending, { refetch: refetchPending }] = createResource(() => adminApi.amasListSuggestions('pending', 50));
  const [canaries, { refetch: refetchCanaries }] = createResource(() => adminApi.amasListCanaries());

  const counts = createMemo(() => ({
    pending: (pending() ?? []).length,
    canary: (canaries() ?? []).length,
    effective: cost()?.acceptedCount ?? 0,
    rejected: cost()?.rejectedCount ?? 0,
  }));

  async function onRunNow() {
    setRunning(true);
    try {
      const r = await adminApi.amasAdvisorRun();
      uiStore.toast.success(r.produced ? '巡查完成，产出新建议' : '巡查完成，无新建议');
      void refetchPending(); void refetchCost();
    } catch (e) {
      uiStore.toast.error('触发失败', e instanceof Error ? e.message : '');
    } finally {
      setRunning(false);
    }
  }

  async function onToggleAutoScan(next: boolean) {
    try {
      await adminApi.amasUpdateAdvisorConfig({ advisorEnabled: next });
      uiStore.toast.success(next ? '已启用自动巡查' : '已关闭自动巡查');
      void refetchConfig();
    } catch (e) {
      uiStore.toast.error('设置失败', e instanceof Error ? e.message : '');
    }
  }

  async function onApproveAll() {
    try {
      const r = await adminApi.amasApproveAllSuggestions();
      const ok = r.results.filter((x) => x.ok).length;
      uiStore.toast.success(`已批准 ${ok}/${r.results.length} 条`);
      void refetchPending(); void refetchCost();
    } catch (e) {
      uiStore.toast.error('批量批准失败', e instanceof Error ? e.message : '');
    }
  }

  return (
    <div class="space-y-4">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <HeroCard
          eyebrow="每 20 分钟 · 白名单"
          eyebrowVariant="info"
          title="LLM 调参顾问"
          desc="每 20 分钟跑一次 DeepSeek，对照 7 日运营指标输出参数 patch。白名单内自动灰度，超出白名单待人工审核。所有 patch 可一键回滚，写入审计日志。"
        />
        <Show when={config()}>
          {(c) => (
            <PageHeaderOps
              advisorEnabled={c().advisorEnabled}
              running={running()}
              pendingCount={counts().pending}
              onToggleAutoScan={onToggleAutoScan}
              onRunNow={onRunNow}
              onApproveAll={onApproveAll}
            />
          )}
        </Show>
      </div>

      {/* 成本行（全宽，失败降级不崩页） */}
      <Show
        when={!cost.error}
        fallback={<Card variant="elevated"><Empty title="成本信息加载失败" description={cost.error instanceof Error ? cost.error.message : '请稍后重试'} /></Card>}
      >
        <Show when={cost()} fallback={<Card variant="elevated"><Spinner size="sm" /></Card>}>
          {(c) => <CostRow stats={c()} />}
        </Show>
      </Show>

      <PatchTabs active={tab()} counts={counts()} onChange={setTab} />

      {/* 主体 12 栅格双栏 */}
      <div class="grid grid-cols-1 lg:grid-cols-12 gap-4">
        <div class="lg:col-span-8 space-y-3">
          <Show when={tab() === 'pending'}>
            <Show when={(pending() ?? []).length > 0} fallback={<Card variant="elevated"><Empty title="暂无待审批建议" description="LLM advisor worker 每 20 分钟产出一次" /></Card>}>
              <For each={pending() ?? []}>
                {(s: AmasSuggestion) => (
                  <SuggestionCard s={s} onDecided={() => { void refetchPending(); void refetchCost(); void refetchCanaries(); }} />
                )}
              </For>
            </Show>
          </Show>
          <Show when={tab() === 'canary'}>
            <Show when={(canaries() ?? []).length > 0} fallback={<Card variant="elevated"><Empty title="暂无灰度中 patch" description="批准建议时选择"进灰度"即在此监测" /></Card>}>
              <For each={canaries() ?? []}>
                {(c) => <PatchCanaryCard canary={c} onChanged={() => { void refetchCanaries(); void refetchCost(); }} />}
              </For>
            </Show>
          </Show>
        </div>
        <div class="lg:col-span-4 space-y-3">
          <Show when={costDaily()}>
            {(d) => <CostChart data={d()} avg7dYuan={cost()?.avg7dCostYuan ?? 0} capYuan={cost()?.monthCapYuan ?? 0} refLineYuan={0.3} />}
          </Show>
          <Show when={config()}>
            {(c) => <AdvisorConfigPanel config={c()} onSaved={() => void refetchConfig()} />}
          </Show>
          <WhitelistPanel />
        </div>
      </div>

      {/* 历史表（全宽）：仅在 effective/rejected tab 或始终展示，这里始终展示已决策历史 */}
      <HistoryTable statusFilter={tab() === 'rejected' ? 'rejected' : undefined} />
    </div>
  );
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd admin-ui && npx vitest run tests/pages/AmasAdvisorPage.test.tsx`
Expected: PASS（2 passed）。

- [ ] **Step 5: 跑全量前端测试 + 类型检查**

Run: `cd admin-ui && npx tsc --noEmit && npx vitest run`
Expected: 全绿（含模块 F 的组件测试 + 既有路由测试 `tests/App.routes.test.tsx` 的 `/admin/amas-advisor`）。

- [ ] **Step 6: 提交**

```bash
git add admin-ui/src/pages/AmasAdvisorPage.tsx admin-ui/tests/pages/AmasAdvisorPage.test.tsx
git commit -m "feat(amas-advisor): 整页 12 栅格双栏重写，装配全部子组件"
```
