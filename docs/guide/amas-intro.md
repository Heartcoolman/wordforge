# AMAS 入门

AMAS（**A**daptive **M**astery **A**cquisition **S**ystem）是 WordForge 的自适应核心。本文只讲"是什么 / 为什么这么拆 / 怎么调"。算法细节见 [2026-05-15 调参研究](/amas-tuning-2026-05-15/01-final-report)。

## 设计要点

- **事件驱动**：每次答题 → `process_event` → 状态机推进 → 下一次选词立刻反映。
- **多算法投票**：3 个决策器（heuristic、IGE、SWD）各自给出策略，按"信任分"加权融合；其中一个失败时 ensemble 自动 fallback。
- **记忆模型独立**：MDM / IAD / Mastery 计算 recall 概率，与决策层解耦，方便单独替换或 A/B。
- **可调即可回滚**：所有配置变更落 `amas_config_versions` 表，hash 寻址，一键 restore。
- **白名单调参**：可改的字段、安全区间硬编码在 `tuning_whitelist.rs`，越界一律拒（管理员手动、LLM 自动都拦）。

## 配置结构

`AMASConfig` 拆成 16 个子配置块（`src/amas/config.rs`）：

| 子配置 | 控制什么 |
|---|---|
| `feature_flags` | SSP / ensemble / LLM 等开关 |
| `ensemble` | 三个决策器的初始权重 + 信任分更新率 |
| `modeling` | 用户状态建模超参（学习率、衰减） |
| `constraints` | 单次会话最大词数、最小间隔等硬约束 |
| `monitoring` | 监控事件采样率 |
| `cold_start` | 冷启动阶段判定阈值 |
| `objective_weights` | 多目标加权（准确率 / 速度 / 保留率） |
| `reward` | 延迟奖励信号配置 |
| `feature` | 特征向量维度与归一化 |
| `elo` | ELO K-factor、ZPD 优先级带宽 |
| `fatigue_decay` | 疲劳信号对强度的衰减系数 |
| `heuristic` | 经验规则决策器参数 |
| `ige` | 智能梯度（Intelligent Gradient Estimator） |
| `swd` | 间隔加权（Spaced Weighted Decay） |
| `memory_model` | MDM（多模型记忆曲线）参数 |
| `iad` | 干扰感知衰减（Interference-Aware Decay） |

## 配置生命周期

```
启动:
  amas_config.toml 存在? ──是──▶ load_from_toml → AMASEngine::new
                       └─否──▶ 写默认值到 amas_config.toml

运行中（任一路径都进同一个 reload）:
  ┌─ 手动改文件 ───────▶ notify watcher (500ms 防抖)
  ├─ Admin PUT /admin/amas/config ─▶ apply_and_persist_config
  └─ LLM 自动应用（白名单+灰度） ───▶ apply_and_persist_config
                                         │
                                         ▼
                              validate() ──失败──▶ 拒绝 + 警告
                                   │成功
                                   ▼
                         engine.reload_config()
                                   │
                                   ▼
                       写回 toml + 落 amas_config_versions 表
```

`config_watcher` 在 500ms 防抖期内会清空积压事件，连续保存只触发一次 reload。所有 reload 路径都走 `validate() → reload_config()` 两步，任一失败回滚到旧配置。

## 调参的三档来源

`ConfigVersionSource` 枚举：

| 来源 | 触发方式 | 审计字段 |
|---|---|---|
| `Manual` | 管理员后台 PUT `/admin/amas/config` | `admin_id` + 可选 `note` |
| `LlmSuggested` | `llm_advisor` worker 产建议 → 人工 approve | LLM patch + 指标摘要 |
| `LlmAuto` | 同上，但走"灰度自动应用"开关 + 每日上限 + 置信度阈值 | 同上 + auto_apply 标志 |

三档共用一个 `apply_and_persist_config()` helper，确保版本表、TOML 文件、内存状态始终一致。任意一档都可走 `POST /admin/amas/config/versions/:hash/restore` 一键回滚。

## LLM 调参顾问（可选）

启用：`.env` 设 `LLM_ENABLED=true` + `ENABLE_LLM_ADVISOR_WORKER=true` + `LLM_API_KEY=...`。

工作机制（`src/workers/llm_advisor.rs`）：

1. 每 20 分钟拉近期 AMAS 指标 + 当前配置
2. 调 DeepSeek（OpenAI 兼容协议）生成 patch
3. **白名单校验**：patch 字段必须在 `TIER_A_WHITELIST` 内，值必须在安全区间
4. **成本封顶**：每日总花费 ≤ `llm.daily_cost_cap_usd`，超额跳过
5. 落 `amas_tuning_suggestions` 表，状态 `pending`
6. 灰度自动应用：管理员后台开关 `amas_auto_apply_enabled` + `min_confidence` + `max_per_day` 三道闸

`LLM_MOCK=true` 时不打外部 API，但仍会写一条 pending 建议，便于本地联调。

## 管理后台对接

**用户态 API**（`src/routes/amas.rs::router()`，普通登录）：

- `POST /api/amas/process-event` — 主流事件入口
- `GET /api/amas/state` `/strategy` `/phase` — 当前用户的 AMAS 状态
- `GET /api/amas/learning-curve` `/retention-curve` — 用于绘图
- `POST /api/amas/visual-fatigue` — MediaPipe 检测结果回传

**管理态 API**（`admin_router()`，管理员认证）：

- `GET /PUT /admin/amas/config` — 取/改全量配置
- `GET /admin/amas/config/versions` `/versions/:hash` — 历史版本
- `POST /admin/amas/config/versions/:hash/restore` — 一键回滚
- `GET /admin/amas/metrics` `/metrics/timeseries` — 指标
- `GET /admin/amas/monitoring` `/anomalies` — 监控事件 + 异常概览
- `GET /admin/amas/user-state/distribution` — 用户状态分布
- `GET /admin/amas/suggestions` — LLM 建议队列
- `POST /admin/amas/suggestions/explain` — 单参数解释（让 LLM 解释某次调整的依据）

## 一次完整的调参实例

最近一次系统性调参的全过程见 [2026-05-15 调参报告](/amas-tuning-2026-05-15/01-final-report)，结论：11 维 Tier-A 参数调整后预测准确率 +10.6%、记忆保留 +14%。

## 相关阅读

- [调参管理后台手册](/amas-admin-console) — 工具产品化（结构化编辑 + 可视化 + DeepSeek 助手）
- [FSRS-5 / 墨墨 DHP 研究](/amas-tuning-2026-05-15/02-fsrs-dhp-research) — memoryModel 调参的外部参考
- [Adapter 扩展分析](/amas-tuning-2026-05-15/03-adapter-analysis) — 为何 MDM-only 设计不需扩展
