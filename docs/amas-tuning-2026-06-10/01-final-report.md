# AMAS 全系统算法调优报告

> 日期: 2026-06-10
> 项目: WordForge (v1.1.4 开发线, 分支 `tune/amas-fsrs6`)
> 范围: 结构升级（MDM → FSRS-6）+ memoryModel 12 维重调参 + 决策层 tuned v3 回写 + 10 算法 × 3 数据集 leaderboard 复验

## 1. 背景与动机

三个事实叠加触发本轮全系统调优：

1. **5-15 调参成果已失效**：commit c01a744（2026-06-08）把 memoryModel 的 11 维 Tier-A 调参值（prediction +10.6% / expectedMemory +14%）回滚到 FSRS-5 公版近似值；且 PR#62/#63 修复 50+9 项 AMAS 内核缺陷后（engine/ssp/evm 共 2400+ 行），旧调参值的数学环境已不存在。
2. **结构升级路径已被实验证明**：5-29 bench v0.9 的 10 算法 × 3 数据集对比中，实验性 amas6（AMAS 全栈 + FSRS-6 底层）Borda 排名第 2/10，prediction 维度得分 0.853 → 0.990，mastered +17~29% vs 纯 FSRS-6；落地路径明确为升级 `src/amas/memory/mdm.rs`。
3. **决策层调参成果从未回写**：bench v0.5→v0.9 迭代在真实数据 forward simulation 上调优的决策层参数（"tuned v3"）只存在于 Python 镜像常量里，生产 Rust/amas_config.toml 从未吸收。

## 2. 改动总览

### 2.1 结构升级：MDM → FSRS-6（生产代码）

`src/amas/memory/mdm.rs`（AMAS v2 → v3）：

| 项 | FSRS-5（旧） | FSRS-6（新） |
|---|---|---|
| w 维度 | 19 | **21**（新增 w[19] 同日饱和指数、w[20] 曲线 decay） |
| 遗忘曲线 | `R = floor + (1-floor)(1 + 0.3·t/S)^(-0.5)`，factor/decay/floor 为独立配置 | `R = floor + (1-floor)(1 + factor·t/S)^(-decay)`，**decay = w[20]（可训练）**，factor = `0.9^(-1/decay) − 1` 保证 R(S,S)=0.9 |
| 同日复习 | `S' = S·e^(w17·(G-3+w18))` | `S' = S·e^(w17·(G-3+w18))·S^(-w19)`，且 **G≥3 时强制 S'≥S** |
| S 上限 | 无（仅 interval 钳制） | clamp 至 36500 天（FSRS-6 参考实现，防极端 w 组合溢出） |
| floor 默认 | 0.10 | **0.0**（FSRS-6 标准无渐近线；保留为可调项） |

旧 `forgettingCurveFactor` / `forgettingCurveDecay` 字段标记 DEPRECATED：serde 保留（兼容旧配置/DB 快照反序列化）但运行时不再读取，曲线参数统一经 `MemoryModelConfig::curve_decay()/curve_factor()` 派生。消费方 `word_selector.rs`、`ssp.rs`（SSP-MMC 的 fsrs6_* 纯函数）同步切换。

**存量兼容（零运维介入）**：`w` 反序列化兼容 19 维旧值——自动迁移 append `w[19]=0`（旧公式无饱和项，行为保持）、`w[20]=0.5`（旧固定 |decay|，曲线形状连续）。DB 中 amas_versions/canary/suggestions 的历史快照全部经 `AMASConfig` 类型往返，自动获得 21 维。其他长度直接报错。

配套闭环：

- `config/validation.rs`：新增 w[0..4]>0、w[19]∈[0,1]、w[20]∈[0.05,2] 校验
- `tuning_whitelist.rs`：Tier-A 白名单 11 → **12 条**（新增 `memoryModel.w[20]` ∈ [0.1, 0.8]）；**迁移 m052** 给已 seed 的存量库幂等补行（表空时留给启动 seed 全量 12 条）
- `tests/amas_schema_export.rs` 重导出 schema（w minItems/maxItems 21）+ admin-ui codegen 同步
- admin GUI：参数字典升级 FSRS-6 默认值与说明、新增 w[19]/w[20] 条目、TIER_A_PATHS 加 w[20]
- property 测试适配：c23 生成器扩 21 维；同日递推改逐步期望（饱和项使步进因子依赖 S）；**删除 pt_sameday_commutativity**——S^(-w19) 非线性 + G≥3 下限使同日更新数学上不再可交换（官方 FSRS-6 亦如此），交换律不再是算法不变量

### 2.2 决策层 tuned v3 回写（9 字段）

bench v0.5→v0.9 在真实数据 forward simulation 上调优、与 FSRS-6 底层组合已被 v0.9 amas6 实验验证（mastered +17~29%）的决策层参数，本轮正式回写 Rust 默认值 + amas_config.toml：

| 参数 | 旧值 | tuned v3 |
|---|---|---|
| ensemble.baseWeightHeuristic / Ige / Swd | 0.40 / 0.30 / 0.30 | **0.35 / 0.40 / 0.25** |
| ensemble.warmupHeuristicBoost | 0.20 | **0.10** |
| heuristic.confidenceDecayScale | 200 | **500** |
| heuristic.confidenceDecayCap | 0.5 | **0.3** |
| ige.defaultConfidence | 0.60 | **0.65** |
| memoryModel.retentionMin | 0.70 | **0.75** |
| memoryModel.highAccuracyRetentionBoost | 0.02 | **0.03** |

镜像专有的简化逻辑常量（IGE compress/expand 阈值等，与 Rust bin-UCB 结构不同构）不回写。

另：`baseDesiredRetention` 统一为 **0.9**（FSRS 官方推荐，与 R(S,S)=0.9 语义自洽；旧 0.92 为历史值，被回滚的 0.849 属 FSRS-5 调参空间）。

### 2.3 memoryModel 12 维重调参（TPE，val split）

搜索空间从上轮 11 维扩为 12 维（w[0..3]、w[8..10]、w[15..16]、**w[20] decay**、baseDesiredRetention、maxIntervalDays），窗口围绕 FSRS-6 公版；128 trials@2% → top16@10% → top4@100%（漏斗各级加 max_rows 预算 4M/2M/4M，确定性头部截断，baseline 同水位公平对比）。

**结果：守门正确拒绝全部候选，保留 FSRS-6 公版默认。**

| 候选 | prediction composite | 致命退化 |
|---|---|---|
| baseline（FSRS-6 公版） | 1.000（logLoss 0.5420 / ICI 0.0587 / AUC 0.6543） | — |
| nearMiss[0] | +40.4%（ICI −57%） | expectedMemory −12.6%，nextDayMemory −14.8%，**targetCount 613 → 8（−98.7%）** |
| nearMiss[1] | +36.2% | targetCount → 14（−97.7%） |
| nearMiss[2] | +29.2% | targetCount → 14（−97.7%） |

三个候选是同一过拟合模式：把曲线 decay 拉大（0.26~0.27 vs 公版 0.154）+ retention 压到 0.825，换取短期预测校准（ICI），代价是 SSP-MMC 模拟下 360 天稳态词数崩塌——**与 5-15 轮"memory +14% 仅 targetCount −13%"的良性 trade-off 性质完全相反，不可用于生产**。

结论：**FSRS-6 公版参数（数亿评测记录训练）在 maimemo 回放上已近 Pareto 前沿**。5-15 能 +10.6% 是因为旧 baseline（手工混合默认）远离前沿；前沿收益本轮已由结构升级本身兑现。单数据集 TPE 在公版之上找不到不伤记忆量的改进方向，这与 FSRS 社区"per-user 优化才有显著增益"的经验一致——AMAS 的 per-user 适配由 alpha 平滑 + adaptive_desired_retention + 在线调参白名单承担。

### 2.4 admin GUI preset 体系清理

"已调优 2026-05-15" preset 整体移除（schema.ts 16 处标注 / PresetBar / TierAPanel 卡片 / ParamField chip / 相关测试）：其值属 FSRS-5 19 维空间，在 FSRS-6 公式下套用有害（如 w[3]=5.94、retention 0.849 都是旧空间的最优点）。出厂默认卡片升级为"出厂默认 (FSRS-6)"并标记推荐。

## 3. 验证矩阵

| 验证项 | 结果 |
|---|---|
| Rust lib 单测 | 831/831 全绿（含新增 19→21 迁移、curve helper 钳制测试） |
| property_memory_models | 12/12（FSRS-6 不变量适配后） |
| AMAS 集成 8 suite（http/canary/dashboard/effectiveness/poison/invariants/analytics） | 全绿 |
| Monte Carlo 全系统模拟 9/9 | 全绿：调度效率 AMAS 5.03 vs Leitner 0.66 / Random 0.94；7 算法合成对比第 2（仅次于参数 ML 优化的 FSRS 对照） |
| store 迁移（含 m052） | 382/382 全绿 |
| clippy | 无新增警告（仅 main 既有 6 条） |
| admin-ui amas 页面测试 | 78/78（preset 清理后） |
| bench pytest | 27/27 |
| bench 冒烟（21 维经 Rust adapter 端到端） | predictionScore 自洽 = 1.0 |
| test split 泛化 | 见 §4 |
| leaderboard 复跑 | 见 §5 |

已知无关失败：admin-ui 全量 vitest 中 4 个文件因本地新版 jsdom 把 `URL.createObjectURL` 设为只读而失败（文件与 main 完全相同，CI 环境应复核）。

## 4. test split 泛化验证

最终配置（FSRS-6 公版）在 maimemo test split（5% 用户，250k 行）：

| 指标 | val | test | 漂移 |
|---|---|---|---|
| logLoss | 0.5420 | 0.5418 | −0.04% |
| ICI | 0.0587 | 0.0573 | −2.4% |
| AUC | 0.6543 | 0.6551 | +0.12% |
| maeP | 0.3181 | 0.3197 | +0.5% |

val/test 几乎一致，无过拟合迹象（公版参数本就不是在本数据集上拟合的，符合预期），补上了 5-15 轮"未跑 test split"的已知限制。

## 5. leaderboard 复验（10 算法 × 3 数据集）

升级后的**生产 AMAS** 复跑完整 leaderboard（结果 `benchmarks/results/2026-06-10/`，报告 `docs/algo-bench-2026-06-10/`）：

| 排名 | 算法 | Borda | final_score 均值 | duolingo_hlr | maimemo | synthetic |
|---|---|---|---|---|---|---|
| 1 | fsrs45 | 27 | 0.948 | 2 | 1 | 3 |
| **2** | **amas（本轮升级后）** | **25** | 0.919 | 5 | 2 | **1** |
| 3 | amas6（实验对照） | 23 | 0.924 | 1 | 5 | 4 |
| 4 | fsrs | 23 | 0.906 | 4 | 4 | 2 |
| 5 | fsrs6 | 19 | 0.869 | 3 | 6 | 5 |
| 6-10 | dhp / sm2 / random / leitner / hlr | ≤14 | — | — | — | — |

对照 5-29（v0.9，升级前）：**amas 综合 Borda 20 分第 5 → 25 分第 2**，prediction 维度得分 0.853 → 0.987（logLoss 三数据集均值 0.562 → 0.368），synthetic 数据集第 1，达成并替代了当初 amas6 实验原型的位置。fsrs45 仍居榜首（其 FSRS-4.5 短曲线在 90 天窗口的 mastered 累积占优，Borda 差距 2 分）。

决策层 synergy 在新内核上复验成立：duolingo_hlr 上 amas6 mastered 6595 vs 纯 fsrs6 5106（**+29.2%**），与 v0.9 结论（+17~29%）一致。

口径说明：
- `amas` = 生产实现（FSRS-6 底层 + 同日 w19 公式 + tuned v3 决策层 + DEFAULT 21 维配置）。
- `amas6` = 5-29 的实验原型条目（独立 _FSRS6_DEFAULT_W、无同日分支），保留作交叉验证；其与 amas 的微小分差源于同日复习处理差异，原型使命已由 amas 吸收。
- `fsrs` 条目语义历史上即"与 AMAS 同配置的纯调度对照"（共享 DEFAULT 配置），本轮其 prediction 与 amas 完全同分，再次验证 next-step prediction 不依赖调度器决策（adapter 分析的 MDM-only 结论）。

## 6. 生产部署注意事项

1. **配置自动迁移**：升级后首次加载 19 维旧配置（toml 或 DB 快照）自动迁移到 21 维（w19=0/w20=0.5，曲线行为连续）；amas_config.toml 已直接写入 FSRS-6 公版 21 维。
2. **白名单**：新部署 seed 12 条；存量库由 m052 幂等补 `memoryModel.w[20]` 行。
3. **学习状态无迁移**：MdmState 仍是 (D,S) 语义，DB 用户记忆状态不变；曲线参数变化使同一 S 的解读略有偏移（旧 factor 0.3/floor 0.1 → 标准 R(S,S)=0.9），属算法修正而非破坏。
4. **行为变化点**（用户可感知）：floor 0.1→0 使久未复习词的 recall 估计更低（urgency 更高、更早安排复习）；retentionMin 0.75 抬高自适应留存下限；Easy/Hard bonus 改 FSRS-6 公版后间隔分布会变化。建议发版走 beta 通道观察 AMAS 看板 7 天。

## 7. 已知限制

1. **maimemo 调参信号是二元天级回放**：同日饱和项 w[19]、同日公式只能靠公版默认，bench 数据不含同日复习粒度。
2. **决策层 tuned v3 的评估器是 Python 镜像**：与 Rust 实现在 ensemble/heuristic 结构上同语义但 IGE 不同构；回写仅限数值语义一一对应的 9 字段，并经 Rust Monte Carlo 全系统回归验证。
3. **oracle 跨数据集复用**：duolingo_hlr/synthetic 的 GRU oracle 独立训练（v0.7 起），但 prediction 维度在 duolingo_hlr 上仍因 87% 正例率而区分度低（全员 AUC≈0.54）。
4. **本轮未动子系统**：iad/mtp/ssp featureFlags 仍 disabled（生产默认）；EVM/wordSelector 数值未调（wordSelector 的有效性已由 simulate 的 mastered 增益背书，单参数级调优需在线 A/B）。
