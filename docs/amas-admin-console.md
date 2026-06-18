# AMAS 调参管理后台 —— 实施记录与运营手册

> 自适应记忆与算法系统（AMAS）的"运营级"调参工作台，使非作者也能基于数据安全地调整参数；
> 必要时由 LLM（默认 DeepSeek）综合数据给出调参建议、可经审批或灰度自动应用。

---

## 全景架构

```
┌────────────────────────────────────────────────────────────────────┐
│                       前端（SolidJS / Tailwind）                    │
│                                                                    │
│ ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐   │
│ │ /admin/amas-     │  │ /admin/amas-     │  │ /admin/amas-     │   │
│ │  config          │  │  metrics         │  │  advisor         │   │
│ │ Tier-A / 分节 /  │  │ C1 延迟时序      │  │ pending / history│   │
│ │ JSON 三 Tab +    │  │ C2 版本对比      │  │ approve / reject │   │
│ │ Preset + 版本    │  │ C3 异常表盘      │  │ 成本面板         │   │
│ │ 抽屉 + Diff +    │  │ C4 用户状态分布  │  │                  │   │
│ │ 回滚             │  │                  │  │                  │   │
│ └──────────────────┘  └──────────────────┘  └──────────────────┘   │
│                                                                    │
│ /admin/settings → 「AMAS 调参自动化」分区（灰度开关 + 阈值）       │
└────────────────────────────────────────────────────────────────────┘
                                  │  HTTPS + Bearer admin JWT
┌─────────────────────────────────▼──────────────────────────────────┐
│                        后端（Rust / Axum）                          │
│                                                                    │
│  /api/admin/amas/config        (GET / PUT)                         │
│  /api/admin/amas/config/versions      (GET / GET/:hash)            │
│  /api/admin/amas/config/versions/:hash/restore   (POST)            │
│  /api/admin/amas/metrics              (GET，snapshot)              │
│  /api/admin/amas/metrics/timeseries   (GET)                        │
│  /api/admin/amas/anomalies            (GET)                        │
│  /api/admin/amas/user-state/distribution (GET)                     │
│  /api/admin/amas/compare              (GET)                        │
│  /api/admin/amas/monitoring           (GET，原始事件列表)          │
│  /api/admin/amas/suggestions          (GET / POST/:id/approve /    │
│                                         POST/:id/reject /          │
│                                         POST/explain / GET/spend)  │
│  /api/admin/settings                  (GET / PUT —— 含 auto-apply) │
└──────────────────────────┬───────────────────┬─────────────────────┘
                           │                   │
                           ▼                   ▼
           ┌─────────────────────────┐  ┌────────────────────────┐
           │   SQLite (r2d2 pool)    │  │ Worker: llm_advisor    │
           │ ────────────────────── │  │ ────────────────────── │
           │ amas_config_versions    │  │ 每 20 分钟              │
           │ amas_tuning_suggestions │  │ 收集 evidence →        │
           │ engine_monitoring_events│  │ DeepSeek → 解析 patch  │
           │ algorithm_metrics_daily │  │ → 白名单守卫           │
           │ system_settings (auto)  │  │ → 入 suggestions 表    │
           └─────────────────────────┘  │ → 满足条件灰度自动应用 │
                                        └────────────────────────┘
```

---

## 数据库

### `amas_config_versions` —— 配置版本快照

| 列 | 类型 | 备注 |
|---|---|---|
| id | INTEGER PK AUTOINCREMENT | |
| version_hash | TEXT UNIQUE | 16 hex；DefaultHasher(serde_json)，与 `engine_monitoring_events.config_version` 同算法 |
| snapshot_json | TEXT | 完整 AMASConfig JSON |
| author_admin_id | TEXT | 人工写入则是 admin id；自动应用则为 `worker:llm_advisor` |
| source | TEXT CHECK IN (manual, llm_suggested, llm_auto) | |
| note | TEXT NULLABLE | 例如 `restore from <hash>` / `approve suggestion#42` |
| parent_version_hash | TEXT NULLABLE | 上一个版本 hash（线性链） |
| created_at | TEXT RFC3339 | |

### `amas_tuning_suggestions` —— LLM 建议

| 列 | 类型 | 备注 |
|---|---|---|
| id | INTEGER PK AUTOINCREMENT | |
| created_at | TEXT RFC3339 | |
| based_on_version_hash | TEXT | 建议生成时的运行配置 hash |
| patch_json | TEXT | `{"memoryModel.baseDesiredRetention":0.85, ...}` |
| rationale | TEXT | LLM 用中文给出的理由 |
| evidence_json | TEXT | 喂给 LLM 的指标摘要 |
| status | TEXT CHECK IN (pending, approved, rejected, superseded, expired, auto_applied) | |
| decided_by / decided_at / decision_note | | 决策审计 |
| cost_usd / tokens_input / tokens_output / confidence | | 计量 |

### `system_settings` —— 灰度自动应用开关（PR-6 加 3 列）

```sql
amas_auto_apply_enabled INTEGER DEFAULT 0     -- 默认关闭
amas_auto_apply_max_per_day INTEGER DEFAULT 1 -- 日额度
amas_auto_apply_min_confidence REAL DEFAULT 0.8
```

---

## 三档运营模式

### 模式 1：仅建议（推荐主路径）
- `amas_auto_apply_enabled = false`
- LLM advisor 产出的所有建议落 `pending`
- 管理员在 `/admin/amas-advisor` 看每条建议的 Diff + rationale + cost，逐条批准 / 拒绝
- 批准时自动新建 `source=llm_suggested` 的 config version

### 模式 2：灰度自动应用
- `amas_auto_apply_enabled = true` + 每日额度 + 最低置信度
- llm_advisor 在生成建议后，**额外**判断：
  - 仅含 1 个 patch 字段（避免一次性大改）
  - confidence ≥ `amas_auto_apply_min_confidence`
  - 当日 auto_applied 数 < `amas_auto_apply_max_per_day`
- 满足时直接写 `source=llm_auto` 的 config version + suggestions 状态 `auto_applied`
- 否则落 pending

### 模式 3：仅解释（不产生 patch）
- 在 Tier-A / 分节 / JSON 任一参数旁点 "问 AI"
- 后端 `POST /api/admin/amas/suggestions/explain` 同步调 DeepSeek
- 返回 80 字内的人话解释；不入 suggestions 表，不影响配置

---

## 新特性开启 v1.2.0-beta.14

beta.14 随发版落地的 AMAS 自研升级（FSRS-6 三特征记忆引擎先验 + Tier-1 调度引擎）**默认全部关闭**：所有 feature flag 默认 `false`、所有调制系数默认 `0.0`，未声明这些键的旧配置 / DB 历史快照反序列化后与旧行为 **逐位等价（bit-exact legacy）**。要启用必须**显式写键**，走人工渠道（`PUT /api/admin/amas/config`）—— 这批键全部**不在 Tier-A 白名单**内，LLM 渠道无法改。

下表配置项名为 TOML / 前端 schema 中的 camelCase 形态（源码对应字段见 `src/amas/config/{feature_flags,memory,elo,ssp}.rs`）。

### 记忆引擎三特征（`[memoryModel]`，源码 `config/memory.rs`）

记忆核已是公版 FSRS-6 逐位等价（21 维 `memoryModel.w`，含新增 `w[19]` 同日饱和、`w[20]` 可训练曲线 decay）。三特征作为**预测/调度读出层先验**叠加，**不入内部 S/D 更新**，逐项开启：

| 配置项 | 默认值 | 开启方式 |
|---|---|---|
| `memoryModel.difficultyLogitWeight` | `0.0`（关） | 设 >0 启用难度 logit 重校准；参考点 `difficultyLogitRef=5.0` |
| `memoryModel.predLogitBaseScale` / `predLogitIntercept` / `predLogitReviewCountWeight` | `1.0` / `0.0` / `0.0`（退化为纯 difficultyLogit） | 改 baseScale≠1 或 intercept≠0 或 reviewCountWeight>0 启用 v7 Platt/温度重校准 + 复习次数残差 |
| `memoryModel.coldStartDLenWeight` 等 6 个 `coldStart{D,S}{Len,Morph,Extd}Weight` | 全 `0.0`（关） | 任一设 ≠0 启用冷启动难度先验（仅首评 `review_count==0` 调 S₀/D₀）；中性 ref/scale 为 `coldStartLenRef=7.0`/`coldStartLenScale=4.0`/`coldStartExtdRef=5.0` |
| `memoryModel.alphaRampTau` / `alphaLapseRampTau` | 均 `0.0`（冻结） | 设 >0 启用双腿信任调度（成功腿 τ_s / 失败腿 τ_f） |

> ⚠️ 这批系数**按部署离线拟合、不跨域迁移**（synthetic 退化），生产启用前须用 prod_replay + 真实留存 A/B 复核，不能只看离线 RMSE/AUC。

### Tier-1 调度引擎

**Parallel Elo + 动态 K（`[elo]`，源码 `config/elo.rs`）**

| 配置项 | 默认值 | 开启方式 |
|---|---|---|
| `elo.parallelEloEnabled` | `false` | 设 `true` → ZPD 选词读选词链 `rating_select`、更新写估计链 `rating`（双链解耦）；`false` 时 bit-exact 单链 |
| `elo.parallelEloRefreshGames` | `8` | 选词链延迟刷新间隔（按该词全局对局数） |
| `elo.kDynamicEnabled` | `false` | 设 `true` → 按残差 EWMA 趋势动态调 K；`false` 时与固定 K 逐位等价 |
| `elo.kTrendWeight` / `kTrendGain` / `kTrendDamp` / `kMinFactor` / `kMaxFactor` | `0.3` / `1.0` / `0.2` / `0.5` / `2.0` | 动态 K 的 EWMA 权重、趋势增益、稳态阻尼、乘子上下界 |

**SSP 状态相关 DR 曲面 / Cost-ADR（`[ssp]` + `[ssp.costParams]`，源码 `config/ssp.rs`）**

SSP-MMC 后端总开关为 `featureFlags.sspEnabled=false`（须先开）。开启后：

| 配置项 | 默认值 | 开启方式 |
|---|---|---|
| `featureFlags.sspEnabled` | `false` | SSP 最优间隔后端总开关，须先设 `true` |
| `ssp.costParams.recallSCoeff` / `recallDCoeff` / `forgetSCoeff` / `forgetDCoeff` | 全 `0.0`（调制因子恒 1.0，bit-exact） | 任一设 ≠0 启用按 S/D 线性调制的代价函数（Cost-ADR） |
| `ssp.rMin` / `rMax` / `rStep` | `0.70` / `0.99` / `0.01` | 3D Bellman 求解器的目标保持率 R 搜索网格（即状态相关 DR 曲面的解空间） |
| `ssp.dualGridEnabled` | `true` | 小 S 用 log 间距、大 S 用线性间距的双网格离散化 |

> Cost-ADR 的 (difficulty, stability)→最优目标保持率 R 曲面在 DP 求解后被保留（`PrecomputeResult.optimal_r`），可经引擎查询 / 对拍；此前算出仅用于反演间隔即丢弃。

**真实留存 A/B 验证闭环（T1.3，无独立 feature flag）**

不靠配置开关，而是**注册 canary 实验**触发：写入 `amas_experiments`（预注册 `primary_metric` / `min_sample` / `alpha` / `power` / `mde`，单行 `status='running'` 唯一约束），用户进桶时在 `user_bucket_assignment` 冻结 arm + Day-0 锚点。无运行中实验时全链路 no-op。

**采纳门**（`src/amas/experiment/evaluate.rs`）：离线赢 ≠ 真实赢，采纳须**同时**满足三条，任一不满足即不采纳：
1. 两臂在 primary 指标上样本量均 ≥ `min_sample`；
2. primary 两臂检验 `p < alpha`、方向为「更好」、且效应 ≥ MDE；
3. 无 guardrail 指标向「更差」方向显著移动。

北极星指标：`day7_retention` / `day30_retention` / `session_completion` / `mastered_hold`（均越大越好）、`reviews_per_day`（越小越好）。`mastered_hold` 依赖 `word_mastery_events` 记录的 mastered/forgotten 转移边沿支撑「长期 masteredCount」。

### 数据库迁移

m056–m058（启动自动应用）新增：A/B 实验三表（`amas_experiments` / `user_bucket_assignment` / `word_mastery_events`）、ELO 动态 K 趋势列（`trend`）、选词链列（`rating_select`，默认 1200.0）。

## 白名单（Tier-A 11 维）

仅这 11 个参数允许通过 LLM 渠道修改（人工渠道可改全部 295 参数）：

| Path | min_safe | max_safe |
|---|---|---|
| memoryModel.w[0..3] | 见 `src/amas/tuning_whitelist.rs` | 初始稳定性 |
| memoryModel.w[8/9/10] | | 稳定性增长 |
| memoryModel.w[15/16] | | Hard/Easy 评级 bonus |
| memoryModel.baseDesiredRetention | 0.75 | 0.95 |
| memoryModel.maxIntervalDays | 30 | 365 |

任何越界或非白名单 path 都会让 advisor 把建议 status 改为 `rejected` 并写明原因。

---

## LLM provider 配置（环境变量）

```dotenv
LLM_ENABLED=true                              # 默认 false
LLM_MOCK=false                                # 设 true 跑端到端，不消耗实际 token
LLM_API_URL=https://api.deepseek.com          # OpenAI 兼容协议
LLM_API_KEY=sk-...
LLM_MODEL=deepseek-reasoner                   # advisor 主路径；hover 可换 deepseek-chat
LLM_TIMEOUT_SECS=60
LLM_DAILY_COST_CAP_USD=1.0                    # 超限当天 advisor 跳过
LLM_INPUT_PRICE_PER_MTOK_USD=0.55             # DeepSeek 当前价格
LLM_OUTPUT_PRICE_PER_MTOK_USD=2.19
ENABLE_LLM_ADVISOR_WORKER=true                # cron 调度开关
```

---

## SSE 事件扩展（PR-7 D4）

`SseEvent` 枚举新增 `NewLlmSuggestion { suggestion_id }`，便于前端 advisor 页实时刷新（前端订阅）。

---

## 验证清单

| 类别 | 命令 | 期望 |
|---|---|---|
| 后端 lib | `cargo test --lib` | 221 passed |
| 后端集成 | `cargo test` | 全套 passed |
| 前端 | `pnpm -C admin-ui test` | 347 passed |
| LLM mock 端到端 | `LLM_ENABLED=true LLM_MOCK=true ENABLE_LLM_ADVISOR_WORKER=true cargo run` → 等 cron tick → `GET /api/admin/amas/suggestions?status=pending` | 看到一条 mock pending |

---

## 影响与边界

- ✅ 不动 AMAS 核心算法（mdm / ensemble / heuristic）
- ✅ 不动用户端 UI / 不动用户侧 AMAS 调用
- ✅ 现有 amas_config.toml 兼容；新写入仍走 atomic rename + version 表同步
- ✅ DefaultHasher 16 hex 哈希算法对齐，确保 monitoring_events.config_version 与 amas_config_versions.version_hash 互相 JOIN
- ✅ pre-existing `pt_sameday_recursive_consistency` 与实际 mdm.rs 实现不匹配，修复测试对齐"线性插值"语义
- ⚠️ 自动应用模式默认关闭；启用前建议先观察至少 7 天的 pending 流
- ⚠️ DeepSeek prompt caching 自动按前缀去重，但 Tier-A 白名单变更会破坏缓存
