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

> **⚠️ 本节结论已被 §8 推翻（2026-06-10 当日晚些时候）**：上述"全拒"是 DHP 盲目标函数 + TPE
> 单变量羊群效应的搜索假象，不是前沿证据。把 DHP 守门约束编码进搜索循环并修复搜索空间病灶后，
> 同一个校准角落剥离毒维即过全门（+29.1% val / +27.8% test，记忆量三腿合规），已写回生产。详见 §8。

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
5. **DHP 守门镜像的保真度缺口**（§8 勘察确认，刻意未在本轮修复）：镜像把成功映射为 grade 4（每次成功吃 w16 easy-bonus、首评读 w3 而非 w2）、无 alpha 平滑（等效 α=1.0，稳定性演化比生产激进 ~3 倍）、难度冻结、无 maxIntervalDays 上限、基线锚定 bench retention 0.92（生产 0.90）。守门是相对比较（候选与基线同镜像），系统性失真大体对消，且 §8 胜者在对称 0.90 复检下三腿全过；但绝对量级（如 targetCount 613）不能直接解读为生产预期。镜像对齐 adapter 语义（grade 3 映射）留作后续单独变更——会重置守门基线，需与历史结果隔离。

## 8. 约束搜索修复与胜者落地（同日第二轮）

§2.3 的"守门正确拒绝 = 公版即前沿"结论存在方法论缺口：**搜索目标函数对 DHP 完全失明**（DHP 只在
4 个决赛者上事后计算），"全拒"只能证明"纯 prediction 最优方向必然伤记忆"，不能排除折中点存在。
本节为缺口闭合全过程（多智能体勘察 → 设计评审团 → 实现+对抗审查 → 约束搜索 → 验证落地）。

### 8.1 逐维探针勘察：旧搜索空间的四类病灶

对 12 个搜索维做确定性探针（单维扰动 → adapter 预测 / interval 效率项 / DHP 模拟三路 bit 级对比）：

| 病灶 | 维度 | 机制 |
|---|---|---|
| 三路全死 | w1, w15 | bench 把对错二值映射为 Good/Again（adapter SUCCESS_QUALITY=0.7），Hard/Easy 分支在 adapter 与镜像中均不可达 |
| 守门盲区互补 | w3, w16（守门活/目标盲）vs w2（目标活/守门盲） | 镜像把成功记为 grade 4（首评读 w3、每次成功吃 w16 bonus），adapter 记为 grade 3（读 w2）——目标函数与守门各看半边，TPE 可零反馈破坏 w3/w16（near-miss 的 w3 腰斩即崩塌主因之一，目标函数全程不知情） |
| 目标盲+守门致命 | baseDesiredRetention | 预测路径 bit 级不可见（探针证实），却直接驱动镜像调度；near-miss 0.8255 → 0.92 单维即 targetCount 8→319 |
| 采样器羊群效应 | 全部 12 维 | TPESampler 默认 multivariate=False，零信号维被早期高分 trial 的取值冻结成针尖（连三路全死的 w15 都聚在 ±0.003）——retention 聚在 0.825 是 seed-42 偶然事故而非信号 |

另发现独立潜在缺陷：`ceil(0.02×10)==ceil(0.10×10)==1`，旧 stage2 的"10% 用户"与 stage1 的"2%"解析到
**同一个十分位桶**，且 stage2 的 max_rows=2M 使其成为 stage1 数据的严格子集——晋级从未见过新数据。

### 8.2 重设计（评审团定稿 D1-D14 要点）

- **搜索空间 12 → 6 维**（w0/w2/w8/w9/w10/w20，窗口不变）：病灶维全部钉死公版值。
- **DHP 约束进循环**：每 trial 先跑 DHP 模拟（实测 0.07 s、完全确定、bit 级可复现守门基线 613/3127.75），
  三条 0.9× 守门腿编码为 optuna 4.8 `constraints_func`（归一化违约幅度喂排序；可行性布尔用守门原表达式
  判定，与幅度解耦消除 1-ulp 边界风险）；不可行 trial 短路跳过 ~14 s 预测评估（违约 trial 的目标值
  不进 TPE 分裂，经安装源码核实）。约束与守门共享同一 memo 缓存，结构上不可能分叉。
- **8 个边界教师种子**：公版锚点（约束自检必须恰为 (-0.1,-0.1,-0.1)）+ 3 个"修复版 near-miss"
  （活维投影、毒维回退公版）+ 半步/w20 阶梯/w0 探针。
- **stage2 改不相交十分位桶**（修 8.1 末缺陷）；搜索期目标 = 1.5 倍封顶的纯预测复合分
  （防 ICI 单指标劫持；0.15 效率项随毒维钉死一并移除）；**最终守门逐字节不变**（uncapped、100%@4M）。
- **否定性裁决协议**：verdict ∈ {winner, conclusive_negative(可行≥30), inconclusive_explore}，
  附可行率分布、零假设噪声标定（公版 ×10 桶）、边界尸检、retention 0.90 敏感性、逐指标回归红旗。
- 实现经 3 路对抗审查零缺陷放行；新增 14 个单测（套件 27→41）；接线自检复现 near-miss 8/14/14
  且判不可行。顺手修复：adapter 二进制无条件重建（本轮真抓到陈旧二进制）、test split 数据卫生
  （全库 2.31 亿行唯一一行负 delta_t 脏数据恰在 test，会杀死 adapter server，SQL 层过滤）。

### 8.3 结果：winner（4 决赛者全过门）

128 trial：58 可行 / 70 不可行（短路），可行率逐季稳定 ~44%（采样器学会边界而未塌缩）。

| 候选 | prediction gain (100%@4M) | targetCount | 身份 |
|---|---|---|---|
| **selected** | **+29.1%** | 571（−6.9%，合规） | **= 修复版 NM2 种子**：上轮被处决的校准角落，剥离毒维后完整存活 |
| finalist 2 | +26.0% | **671（+9.5%）** | TPE 自主发现 |
| finalist 3 | +23.0% | 655（+6.9%） | TPE 自主发现 |
| finalist 4 | +19.2% | 638（+4.1%） | TPE 自主发现 |

可行改进区是一片真实高原而非孤针。胜者 expectedMemory **+2.7%**、nextDayMemory **+2.5%**（优于基线），
avgDueRecall 0.688→0.666（−3.2%，未触 0.95 警戒）；maeP −2.5% 劣化（守门复合分本身接受的权衡，
已按 WARN 级记录 anyMetricWorse=true）。增益解剖：ICI −48.7%（主导）+ logLoss −2.7% + AUC +0.46%。

**验证链**：
- test split（同水位 100%@4M，留出用户）：+27.8%，逐指标模式完美复现（ICI −47.5% / logLoss −2.6% /
  maeP −2.5%）——无 val 特异过拟合。
- 对称 retention 0.90 复检（候选与基线同锚，修正实现侧 0.92 vs 0.90 不对称比较）：targetCount
  243 vs 215（**反超 +13%**），三腿全过（最紧余量 −0.0966）。
- 零假设噪声：公版 2% 桶间复合分 0.9994 ± 0.0216 —— +29% 信号远超噪声。

### 8.4 落地

胜者 6 维写回生产 `amas_config.toml` 与 bench `DEFAULT_MEMORY_MODEL_CONFIG`（w[0]=0.0920 /
w[2]=2.5732 / w[8]=1.6065 / w[9]=0.2455 / w[10]=1.1282 / w[20]=0.2711，全精度见文件）；Rust 代码
default_w 保持 FSRS-6 公版（toml 承载调优值，沿 5-15 惯例）；fsrs45/fsrs6/amas6 对照臂锁定公版不受影响。
回归：bench pytest 41/41、cargo test --lib 831/831。leaderboard 以胜者配置复跑（结果
`benchmarks/results/2026-06-10-constrained/`，报告 `docs/algo-bench-2026-06-10-constrained/`）：见 8.5。

部署提示（叠加 §6）：w 变化改变间隔分布（decay 0.154→0.271 使同 S 下超期惩罚更陡、临期更缓；
初始稳定性 w0 下调让首评失败词更早复习）。建议随既有 beta 通道一并观察 AMAS 看板 7 天。

### 8.5 leaderboard 复跑（胜者配置）

| 排名 | 算法 | Borda | duolingo_hlr | maimemo | synthetic |
|---|---|---|---|---|---|
| 1 | fsrs45 | 28 | 2 | 2 | 1 |
| **2** | **amas（胜者配置）** | **25** | 5 | **1** | 2 |
| 3 | amas6（锁公版对照） | 23 | 1 | 5 | 4 |
| 4 | fsrs（同配置纯调度） | 23 | 4 | 3 | 3 |
| 5 | fsrs6（锁公版对照） | 19 | 3 | 6 | 5 |
| 6-10 | dhp / sm2 / random / leitner / hlr | ≤13 | — | — | — |

对比上午公版快照（§5）：综合 Borda 25 第 2 不变，但成色质变——**maimemo 主数据集 2 → 1，首次反超
fsrs45**（mastered 17376 vs 15907、ICI 0.0164 全场最佳、logLoss 0.3072；对比公版 amas：mastered
15501→17376 即 **+12.1%**，与守门 expectedMemory/nextDayMemory 反超基线的读数一致）；synthetic 1 → 2
（decay 向 maimemo 真实回放偏移的小代价，其 ground truth 为 DHP 合成）；duolingo_hlr 5 不变（87%
正例率、全员 AUC≈0.54 的低区分数据集）。隔离确认：fsrs45/fsrs6/amas6 三个锁公版对照臂数字与上午
逐位一致。

### 8.6 结论修正（替代 §2.3）

1. **FSRS-6 公版不是 maimemo 回放上的 Pareto 前沿**——存在 +29% prediction 且记忆量三腿合规
   （其中两腿反超）的可行点，且可行改进区是高原（4 决赛者全过门）而非孤针。
2. §2.3 的"全拒"由两层方法论缺陷制造：DHP 盲目标 + TPE 单变量羊群效应冻结毒维。**"守门拒绝"
   永远不能直接解读为"前沿"，必须先证可行区被探索过**（本轮起 searchDiagnostics 强制此证明）。
3. 离线全局 w 调参在守门进环后仍有价值；下一步更高杠杆是镜像 grade 映射对齐（§7.5）与
   per-user 在线适配，以及把 maeP 非回归加入约束消除残余权衡。

## 9. 镜像保真度对齐 + 第三轮约束重搜（2026-06-11）

§8.6-3 点名的"镜像 grade 映射对齐"本轮落地，且勘察发现失真面远宽于 grade 一项；对齐后守门
基线重置、搜索空间扩容重搜，产出第三代胜者写回生产。

### 9.1 勘察：镜像与 Rust 真实模型的三个一级失真

守门模拟器 `WordforgeMirrorState`（dhp_reference.py）与目标函数真实路径（mdm.rs
`update_strength`，经 benchmark_adapter quality 0.7/0.0、alpha 0.3 调用）逐行对照：

| # | 失真 | 镜像（旧） | Rust 真值 | 影响 |
|---|---|---|---|---|
| A1 | grade 映射 | 答对→Easy(4) | 答对→**Good(3)** | 首评 S 走 w[3]（≈5× 膨胀）、增长乘 w[16] easy bonus、D0 偏移 |
| A2 | alpha 平滑 | 直接跳到 target | `S = prev + (target−prev)×0.3` | 遗忘惩罚被镜像放大 3 倍以上，全部动力学节奏失真 |
| A3 | 难度动力学 | D 初始化后冻结 | 每评 `−w6×(G−3)` + w7 均值回归（锚 d0(4)）+ 平滑 | (11−D) 增长项失真；w4-w7 守门盲 |
| A4/A5 | clamp | 无 S 上限、无 interval 顶帽 | S≤36500、interval≤90d | 高稳定词调度行为偏差 |

修复 = 镜像逐行复刻 adapter 路径，并以 **Python↔Rust 对拍测试**永久钉死等价性：
200 例随机 w/history（含 delta_t=0 同日分支）max err **7.1e-14**（容差 1e-9，余量 4 个量级），
interval 366 次比较全过；19 维 legacy 路径另补 60+1 例（修复审查发现的同日 G≥3 floor 缺失后
逐位 ==）。实现经 3 路对抗审查（数值等价/模拟协议回归/测试基建），6 findings 全闭环。

### 9.2 基线重置与上代胜者复检

新镜像下重测（seed=42，@0.92 守门口径基线 = bench DEFAULT = 上代胜者）：

| 配置 @0.90 | expectedMemory | nextDayMemory | targetCount | avgDueRecall |
|---|---|---|---|---|
| 上代胜者（生产） | 2217.5 | 2182.3 | 734 | 0.8114 |
| FSRS-6 公版 | 2218.0 | 2186.3 | 701 | 0.8205 |

**旧镜像高估了胜者的调度优势**（targetCount 优势收窄到 +4.7%、expectedMemory 持平）——与
"grade-4 错位是最大失真源"的预判一致；但预测主目标（test +27.8%）走 adapter 路径不受镜像
影响，依然成立。守门基线语义自此演进为"现任冠军"（DEFAULT 即胜者），新一轮搜索必须打败现任。

### 9.3 探针重判：6 → 14 双活维

对齐镜像下 21 维逐维 ×{0.5,0.75,1.33,2.0} 闭环探针（守门三腿峰值偏移）：

- **对齐解锁 8 维**：难度链路 w4（**40.7%，全空间最强杠杆**——均值回归锚 d0(4)=f(w4,w5)
  持续作用）/w5（4.7%）/w6（5.3%）/w7（3.0%）；遗忘分支 w11-w14（2.8-3.8%，G=1 双侧可达）
- 上轮 6 维全部仍活（w2 由守门盲升级为双活——首评成功现在真走 w[2]）
- 死维矩阵确认：w1/w15（Hard 不可达）、w3/w16（对齐后 Easy 不可达，由 gate-only 失真源变
  全死）、w17-w19（闭环无同日复习，守门盲）
- 搜索空间 6→14 维（全部双活维），教师种子 8→12（补 4 个新维边界种子），startup 16→24

### 9.4 重搜：224 trial，新 8 维数据驱动地回归公版

verdict=**winner**，152/224 可行（67.9%），锚点自检过，季度可行率 [0.61, 0.59, 0.77, 0.75]。
与 §2.3 "羊群假象"的本质区别——本轮有探索证明：paramDispersion 显示新 8 维真实方差（w4 跨
4.87-7.0、w13 跨 0.05-0.4），boundaryAutopsy 记录 72 个不可行点的真实拦截（如 w4=6.24 点
expectedMemory 贴边越线 +0.00017 被拒）。**难度/遗忘参数公版值在守门紧约束下即局部最优**是
数据结论而非搜索缺陷；胜者落在 6 维子空间 = 上轮 NM0 种子投影（同族近邻点在新基线/新镜像
口径下排序翻转，两代胜者 test 泛化方向一致）。

### 9.5 第三代胜者与写回

胜者 w[0,2,8,9,10,20] = (0.0798, 3.0250, 1.6259, 0.2534, 1.1125, 0.2633)，其余公版：

- **val +5.76% / test +5.39%（vs 上代胜者）**——模式完美复现，无过拟合；vs 公版 test
  **+38.08%**（上代 +27.76%）
- 增益由 **ICI 主导**（val 0.0301→0.0252 即 +19.4%；test 0.0296→0.0251）；logLoss/AUC 持平；
  maeP −0.43%（连同上代 −2.5% 的 WARN 续记，未来可作非回归约束）
- DHP（@0.92 守门口径）：targetCount **789 反超基线 762**，EM/NDM −0.4% 在门内；
  @0.90 生产口径对称复检 targetCount 809 vs 734（+10.2%），三腿全过
- 已写回 `amas_config.toml` + bench DEFAULT（派生 forgettingCurveFactor/Decay 同步）；
  cargo --lib 831/831、bench 43/43 全绿

### 9.6 leaderboard 复跑（第三代胜者配置）——口径断层，amas 历史数字作废

| 排名 | 算法 | Borda | duolingo_hlr | maimemo | synthetic |
|---|---|---|---|---|---|
| 1 | fsrs45 | 29 | 2 | 1 | 1 |
| 2 | amas6（纯 FSRS-6 参考） | 26 | 1 | 4 | 2 |
| 3 | fsrs6（纯参考） | 21 | 3 | 6 | 3 |
| 4 | **amas（生产语义+胜者 w）** | 18 | 8 | 3 | 4 |
| 5 | dhp | 17 | 6 | 2 | 8 |
| 6-10 | sm2 / fsrs / leitner / hlr / random | ≤15 | — | — | — |

**表面上 amas 从第 2 跌到第 4（maimemo 1→3），实为口径修正而非退化**——本轮镜像对齐使
amas/fsrs 条目（走 `WordforgeMirrorState`）**第一次如实反映 mdm.rs 生产语义**（同样本
20291 行逐位对拍 Rust adapter，max diff 2.2e-16）。§8.5 及更早所有轮次的 amas
prediction/DHP 数字（ICI 0.0164"全场最佳"、mastered 17376"+12.1%"）均为失真镜像产物，
**就生产系统而言作废**，§8.5 表仅作历史记录保留。

根因是一个**架构级发现**而非调参问题——长历史尾步样本（每词最长前缀，p50 历史长度 4、
p90 11）三配置经 Rust 真值对照：

| 配置（Rust adapter 同样本） | ICI | logLoss | meanP 偏置 |
|---|---|---|---|
| FSRS-6 公版 w | 0.1042 | 0.3654 | −0.064 |
| NM2（上代胜者） | 0.1064 | 0.3639 | −0.103 |
| NM0（本代胜者） | 0.1087 | 0.3658 | −0.108 |
| amas6 条目（纯 FSRS-6 参考实现，无 alpha 平滑） | **0.0253** | — | — |

三个 w 配置在该端**几乎并列**（写回无回归，NM0−NM2 差 +0.002 为家族内噪声；本代增益
+5.4% 集中在短中前缀端，被 tune 的前缀展开口径正确计入）；而 **mdm.rs 的 alpha=0.3 EMA
平滑机制本身在长历史端造成 ~4× ICI 校准差距**（0.105 vs 纯参考 0.025）——旧镜像 S 直跳
target 恰好近似无平滑行为，这正是旧 amas 数字"虚好"的机制。DHP 维度同理换轨：对齐后
amas 调度更保守（totalReviews 96k→180k、expectedMemoryFinal 13.8k→20.5k、mastered
17376→14787），为生产真实行为。对照臂 fsrs45/amas6/fsrs6/dhp 原始指标仅第 4 位小数微漂
（样本 20292→20291，test split 负 delta 脏行过滤），相互次序不变；Borda 重排全部由 amas
条目口径修正传导。

### 9.7 残余与方向

1. **alpha 平滑长历史校准课题（本轮最大新发现，杠杆 ~4× ICI）**：mdm 的 EMA 平滑在
   长历史端系统性压低预测（meanP 偏置 −0.10），是比调 w 大一个量级的杠杆。但 alpha 体系
   （alphaScale/streak bonus/interval_scale 联动，mastery.rs）是生产模型语义，动它=
   结构变更，须独立评审：候选方向①把 alphaScale 纳入调参（需 adapter 读 config 而非
   硬编码 0.3）②长历史端 alpha 自适应衰减③评估平滑的产品价值（抗噪/稳定）后再定。
2. maeP 两代累计 ≈−3%：下一轮把 maeP 非回归编入 constraints_func（与 DHP 腿同机制）。
3. w17-w19（同日短期记忆）objective-live 但守门盲：闭环模拟引入同日复习场景后方可入空间。
4. per-user 在线适配（离线全局 w 的下一台阶）。
5. 镜像等价性由对拍测试守护：任何 mdm.rs 公式变更会在 bench 套件红灯，不再静默漂移。
   leaderboard 的 amas6/fsrs6 条目语义自本轮起明确为"纯 FSRS 参考实现对照"，与 amas
   （生产语义）分属不同系统层，跨层比较仅用于量化架构差距。
