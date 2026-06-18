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

## v1.2.0-beta.14 新增能力（默认全关，需显式开启）

beta.14 落地了一批 AMAS 自研升级：**FSRS-6 三特征记忆引擎（作为先验后端）** 与 **Tier-1 调度引擎**（Parallel Elo / 动态 K / SSP 状态相关 DR / 留存 A/B 闭环）。

::: warning 默认全部关闭
本批所有新特性的 feature flag / 调制系数默认值都是 **关闭 / 0.0**，未声明这些键的旧配置与 DB 历史快照反序列化后 **与旧行为逐位等价（bit-exact legacy）**。要启用必须在调参后台显式打开，开启方式见 [调参管理后台手册 §新特性开启](/amas-admin-console#新特性开启-v1-2-0-beta-14)。
:::

### 记忆引擎三特征（FSRS-6 先验后端）

记忆核已是 **公版 FSRS-6 逐位等价**（`src/amas/memory/mdm.rs`，21 维 `w`，新增 `w[19]` 同日饱和指数、`w[20]` 可训练遗忘曲线 decay）。三特征作为**预测/调度先验**叠加，**不进入内部 S/D 更新**，默认关：

| 特征 | 作用 | 关闭判据（默认值） |
|---|---|---|
| 难度 logit 重校准（v6/v7 预测读出层） | 预测 recall 在 logit 域按每词难度 + 复习次数做 Platt/温度重校准，补 FSRS 二元映射下「预测随难度扁平」的残差 | `memoryModel.difficultyLogitWeight=0.0`、`predLogitBaseScale=1.0`、`predLogitIntercept=0.0`、`predLogitReviewCountWeight=0.0` |
| 冷启动难度先验（Phase 1a） | 仅首评（`review_count==0`）按词长 / 词素透明度 / 外部难度调整 S₀·D₀，之后交还 FSRS | 6 个 `memoryModel.coldStart{D,S}{Len,Morph,Extd}Weight=0.0` |
| 双腿信任调度（成功腿 / 失败腿） | 成功复习与 lapse 分别按时间常数 τ 调制 mastery 信任更新率 | `memoryModel.alphaRampTau=0.0`、`alphaLapseRampTau=0.0`（0.0=冻结语义） |

> SSP-MMC 最优间隔后端（`featureFlags.sspEnabled`）、GSP 毕业制调度头（`memoryModel.gsp*`）属此前批次能力，仍由独立开关控制。

### Tier-1 调度引擎

| 能力 | 说明 | 开关（默认值） |
|---|---|---|
| **Parallel Elo（T1.1，双链解耦）** | 开启后 ZPD 选词读「选词链」`rating_select`（延迟快照），更新写「估计链」`rating`，消除「选择依赖被估计量」的耦合偏差；难度消费者恒读估计链 | `elo.parallelEloEnabled=false`、刷新间隔 `parallelEloRefreshGames=8` |
| **动态 K（T1.2，趋势自适应）** | 按近期带符号残差的 EWMA 趋势调 K：同向漂移增 K 追、震荡降 K 降噪；novice 期乘子下界钳 1.0 | `elo.kDynamicEnabled=false`（关闭时与固定 K bit-exact） |
| **SSP 状态相关 DR 曲面（T1.4 Cost-ADR）** | DP 求得的 (difficulty, stability)→最优目标保持率 R 曲面（此前仅用于反演间隔后丢弃，现保留并可查询/对拍）；代价函数可按 S/D 线性调制 | `ssp.costParams.*=0.0`（默认调制因子恒 1.0，bit-exact）；3D 求解 R 网格 `ssp.rMin=0.70`/`rMax=0.99`/`rStep=0.01` |
| **真实留存 A/B 闭环（T1.3）** | 实验注册 + 桶归属冻结 + 采纳门（样本量达标 + primary 显著且达 MDE + 无 guardrail 恶化，三条全满足才采纳；离线赢 ≠ 真实赢） | 无运行中实验时为 no-op；需管理员注册 canary 实验 |

数据库迁移 **m056–m058**（启动自动应用）：A/B 实验表（`amas_experiments` / `user_bucket_assignment` / `word_mastery_events`，后者支撑长期 masteredCount）、ELO 动态 K 趋势列（`trend`）与选词链列（`rating_select`）。

设计背景与路线图见 [AMAS 进化调研终稿（2026-06-17）](/amas-evolution-research-2026-06-17/00-final-report)。

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
