# amas-advisor 页全栈对齐设计图 — 设计文档

状态：已确认（待用户复审）· 日期：2026-05-29 · 范围：整页 + 全部后端，一次做完（不分期）

## 1. 背景与目标

`/admin/amas-advisor`（LLM 调参顾问页）当前实现 `admin-ui/src/pages/AmasAdvisorPage.tsx` 只覆盖设计图 `admin后端/amas-advisor.html` 约 1/3：单列布局（HeroCard + 4 张日度 $ StatCard + 待审批/历史 2-tab + 时间轴）。差异以**结构未对齐为主**（约 2/3~3/4），数据空为次。本次目标：把该页**完整对齐设计图**，并连带补齐设计图依赖、当前缺位的后端能力。

后端 6 个 advisor 端点（`src/routes/admin/amas.rs:54-59`：list/get/approve/reject/explain/spend）已存在且健全，SQL 用 COALESCE 兜底空表返回 0/空数组。本次在其基础上**新增 6 组端点 + 1 个 canary 监测 worker + 3 项数据模型变更**。

### 1.1 已确认的关键决策

| 决策点 | 结论 |
|---|---|
| 范围 | 完整全栈对齐，**不分期**，整页一次做完 |
| 成本口径 | **¥/月度**（复用 `llm_advisor_cost_ledger` 月度人民币台账 + `system_settings` 月上限） |
| 灰度 canary | **真·per-patch canary 子系统**（多条并行灰度，非复用单 active config-version canary） |
| 自动回滚依据 | **先用现成 per-version 信号**（reward / fatigue / anomaly 率）；正确率/留存 per-cohort 量测列为后续增量 |

## 2. 设计图取舍（忠实 vs 虚构）

设计图是静态原型，含虚构数据，**以真实后端语义为准**：

- 白名单：设计图标"12/156 个、cold_start.*/ensemble.* 前缀"为**虚构**。真实是 `TIER_A_WHITELIST` 的 **11 条 `memoryModel.*` 条目**（`src/amas/tuning_whitelist.rs`，每条 `(path, min_safe, max_safe)`）。按真实白名单实现，不照搬虚构数字/前缀。
- 自动回滚阈值：设计图写"正确率≤-0.5pt / 留存≤-2pt"。当前 `aggregate_amas_version_slice` 仅能按 `config_version` 拿到 reward_value/fatigue/attention/anomaly；**正确率仅全局日聚合、d7 留存未按 cohort 切分**。故 v1 自动回滚以 reward 降幅 / anomaly 率升幅为阈值；正确率/留存 per-cohort 留作后续。
- 三联预估影响（疲劳率/正确率/留存）：取自 `evidence_json`；若无对应字段则显 "—"，不编造。
- 成本数字（¥4.21/¥10.00、47/53 等）均为占位，实际由真实端点驱动。

## 3. 前端架构

`AmasAdvisorPage.tsx` 重写为 12 栅格双栏布局，拆出子组件（SolidJS，沿用现有 `@/components/ui/*`、`createResource` 数据流、`adminApi` 客户端、深浅色主题 token）：

```
AmasAdvisorPage
├─ PageHeaderOps      自动巡查 toggle / 立即触发巡查 / 接受全部待审
├─ CostRow           4 卡：本月¥+配额条+预测 / 7天均单次 / 本月次数 / 累计接受率
├─ PatchTabs         待审/灰度中/已生效/已拒绝 计数角标 + 下次巡查倒计时
├─ (左栏 span-8)
│  ├─ SuggestionCard[]   rationale + TOML diff + 三联影响 + 白名单内外风险 + 多档操作
│  └─ CanaryCard[]       per-patch：百分比条 + 实测 stat-pill + 扩量/回滚
├─ (右栏 span-4)
│  ├─ CostChart         30 天成本柱图 + 参考线
│  ├─ AdvisorConfigPanel 模型/频率/¥上限/灰度策略/API Key/auto-apply
│  └─ WhitelistPanel    11 条 memoryModel.* + 增删
└─ HistoryTable        全宽：搜索 + 导出 CSV + 分页 + 行级回滚/查看
```

保留并复用现有 approve/reject 通路（`ConfirmDialog` + `Modal`）。新增 API client 方法与类型（`admin-ui/src/api/admin.ts`）。

### 3.1 页面 → 数据映射

| 区块 | 数据来源（端点见 §4） |
|---|---|
| page-header ops | C2：自动巡查 toggle、立即触发、批量审批 |
| cost-row（¥月） | C1：`/advisor/cost` |
| 4-tab 计数 + 倒计时 | suggestions 按 status 计数 + canary 表计数；倒计时前端按 20min cron 客户端计算 |
| 待审 patch 卡 | 现有 `/suggestions?status=pending` + `base_values_json`；白名单内外查 C4；三联影响取 `evidence_json` |
| 灰度中 canary 卡 | C6：`/advisor/canary` 列表 + per-version slice 实测 |
| 30 天成本柱图 | C1：`/advisor/cost/daily?days=30` |
| 顾问配置 panel | C3：`/advisor/config`（system_settings 可写 + LLMConfig 只读，API Key 仅尾号） |
| 白名单 panel | C4：`/advisor/whitelist` |
| 已生效历史表 | C5：`/suggestions` 加 offset/filter、CSV、回滚 |

## 4. 后端增量（端点 + 数据模型）

均挂在 `src/routes/admin/amas.rs` 的 `admin_router()`，复用 `AdminAuthUser` 鉴权、`run_store_task`、`apply_and_persist_config`、`AppError`/`ok()` 约定。

### C1 成本/统计
- `GET /advisor/cost` → 本月¥成本（`get_llm_cost_this_month`）、月上限（`system_settings.llm_advisor_max_cost_per_month_yuan`）、配额%、月末预测、7 天平均单次成本、本月调用次数、累计 patch 接受率（suggestions 按 status 聚合）。
- `GET /advisor/cost/daily?days=30` → 按日聚合：`amas_tuning_suggestions` `GROUP BY date(created_at)` `SUM(cost_usd) * usd_to_cny_rate`。

### C2 巡查控制
- `POST /advisor/run` → 手动触发一次 `llm_advisor::run`（受 LLM_ENABLED/月日上限门禁约束；返回是否产出）。
- 自动巡查 toggle → 新增列 `system_settings.llm_advisor_enabled`（运行时开关）；worker 注册/执行条件改为 `env ENABLE_LLM_ADVISOR_WORKER || system_settings.llm_advisor_enabled`。`GET/PUT` 复用 settings 端点或在 C3 内。
- `POST /suggestions/approve-all` → 批量审批所有 pending（逐条复用 approve 逻辑 + 白名单校验，部分失败返回逐条结果）。

### C3 顾问配置
- `GET /advisor/config` → 可写：月¥上限、auto-apply 三项（`amas_auto_apply_enabled/max_per_day/min_confidence`）、灰度策略（20→60→100，新存 system_settings 或 canary 默认）；只读：model、巡查频率（cron 常量）、API Key 尾号（env-only，脱敏）。
- `PUT /advisor/config` → 仅更新可写字段，写 `system_settings`。

### C4 白名单 CRUD
- 新表 `amas_tuning_whitelist(path TEXT PK, min_safe REAL, max_safe REAL, created_at, created_by)`，启动时若空则 seed 自 `TIER_A_WHITELIST`。
- `tuning_whitelist::validate_patch` / `find` 改为接受 store 提供的条目（const 作 fallback/seed）；llm_advisor `build_system_prompt` 同步从 store 读。
- `GET /advisor/whitelist`、`POST /advisor/whitelist`（加条目，校验 path 合法 + 区间）、`DELETE /advisor/whitelist/:path`。

### C5 历史增强
- `list_suggestions` 加 `offset` 分页 + 可选字段过滤（status/参数关键字）。
- `GET /suggestions/export.csv` → 当前过滤集导出 CSV（参数/旧→新/Δ指标/成本/状态/时间）。
- `POST /suggestions/:id/rollback` → 基于 `amas_config_versions` 版本链 restore parent version（复用现有 restore 通路），suggestions 标记回滚状态 + 审计。

### C6 per-patch canary 子系统（最重）
- 新表 `amas_patch_canary(id, suggestion_id, version_hash, percent, cohort_lo, cohort_hi, status[active/scaling/effective/rolled_back], baseline_metrics_json, started_at, updated_at)`。支持多条 active，cohort 区间 `[lo,hi)` 互不重叠（落库前校验）。
- 改 `AMASEngine::effective_config_for_user`：由"单 active canary"改为"遍历 active canary，按 `hash(user_id)%100 ∈ [lo,hi)` 命中其一 → 加载该 version snapshot；否则 stable"。保留反序列化失败/version 缺失回退 stable + warn。
- 新 worker `canary_monitor`（cron，每 N 分钟）：对每条 active canary 调 `aggregate_amas_version_slice(version_hash)` 取 reward/fatigue/anomaly，与 `baseline_metrics_json`（灰度起始时 stable 切片）对比：reward 降幅 > 阈值 或 anomaly 率升幅 > 阈值 → 自动回滚（置 rolled_back + 从路由移除 + 审计 + SSE 通知）。worker 失败仅日志不抛。
- `POST /advisor/canary/:id/scale`（扩量到目标 percent，重算 cohort 区间）、`POST /advisor/canary/:id/rollback`（手动）、`POST /advisor/canary/:id/promote`（100% → 提升为 stable，置 effective）。
- approve 流程衔接：approve 一条 pending 时可选"直接生效"（现有）或"进灰度 20%"（新建 patch_canary 行）。

## 5. 数据模型变更汇总

- 新表：`amas_tuning_whitelist`、`amas_patch_canary`。
- 新列：`system_settings.llm_advisor_enabled INTEGER DEFAULT 0`、（如需）灰度策略列。
- 均走 `src/store/migrate.rs` 幂等迁移（`CREATE TABLE IF NOT EXISTS` / `ADD COLUMN` 守卫）。
- `amas_canary_config`（单 active）保留兼容，但 advisor 灰度统一走新 `amas_patch_canary`；`effective_config_for_user` 同时兼顾（迁移期）。

## 6. 测试策略（TDD）

先写测试再实现：

- **Store 单测**：白名单 CRUD + seed；canary cohort 区间不重叠校验 + 路由命中；自动回滚阈值判定（构造 baseline vs 退化切片）；按日成本聚合；月度成本/接受率聚合。
- **`validate_patch`-from-store**：store 驱动后仍正确拒绝越界/非白名单。
- **集成（axum）**：C1-C6 各端点 happy path + 边界（offset 越界、percent 越界、cohort 重叠、回滚不存在 id、CSV 内容）。
- **前端**：vitest 组件测试（各子组件空/满/错三态）、路由测试（沿用 `tests/App.routes.test.tsx` 的 `/admin/amas-advisor`）、e2e（playwright）覆盖审批 → 进灰度 → 扩量 → 回滚关键路径。
- **回归**：`effective_config_for_user` 多 canary 改造后，原单 active 行为 + stable 兜底不破。

## 7. 上线与风险

- canary 多版本路由：stable 兜底 + 反序列化/version 缺失回退（沿用现有容错）；cohort 区间互斥由落库校验保证。
- 自动回滚 worker：失败仅日志，不 disable 调度器（沿用 worker 容错惯例）。
- 门禁：沿用 `LLM_ENABLED` + worker flag；新 `system_settings.llm_advisor_enabled` 与 env 取或。
- API Key 永不返回明文，仅尾号。

## 8. 明确不在本次范围（后续增量）

- 正确率 / d7 留存的 per-cohort instrumentation（自动回滚 v2 阈值）——需改学习数据聚合层，单独立项。
- "沙箱试运行"（设计图 patch 卡的"在沙箱试运行"按钮）——v1 **不实现，按钮隐藏**（不占位编造）；沙箱评估能力作为后续增量。

## 9. 未决/实现期细化

- canary_monitor 的 cron 周期与 reward/anomaly 阈值具体数值（实现期定默认 + 设为可配）。
- 灰度策略档位（20→60→100）是否做成可配 vs 硬编码（倾向 system_settings 可配）。
- 历史表分页 page size 与 CSV 字段最终列集。
