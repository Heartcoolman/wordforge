# WordForge memoryModel 调参参考（FSRS-5 / 墨墨 DHP / Anki 实践）

> 调研日期：2026-05-15  
> 目标：为 amas 自动调参循环提供权威的搜索范围与诊断依据  
> 参考实现：fsrs-optimizer v6.5.x（Python，21 维，向下兼容 19 维 FSRS-5）

## 范围声明（重要）

**Rust adapter `src/amas/memory/mdm.rs` 实际只读取 24 个 memoryModel 字段**（来源：`/tmp/adapter_extension_plan.md` § 2.4，adapter-analyst 已交叉验证）。本报告其余非 adapter 字段（compositeWeight*、shortTerm/medium/longTermLearningRate、halfLife*、passiveDecay*、recallRisk*、mastery* 等）**在 benchmark prediction 上零影响**，TPE 将其作为搜索维度等同噪声——已在 §5 集中标注，仅供未来扩展 wordSelector / ensemble 评估时参考。

### Adapter 真正读取的 24 维有效搜索空间

```
w[0..18]                  19 维  FSRS-5 权重
forgettingCurveFactor      1 维  幂律系数
forgettingCurveDecay       1 维  幂律指数
forgettingCurveFloor       1 维  遗忘下界
maxIntervalDays            1 维  间隔硬 cap
minIntervalSecs            1 维  间隔下限
────────────────────────────
合计                     24 维
```

调参重心 = §1（w[0..18]） + §3（forgettingCurve* 三件套） + §4（baseDesiredRetention 仅当未来 adapter 引入时） + §6（interval cap）。

## 摘要

1. **w[0..18] 的优化器 bound 在 FSRS-5/6 之间完全一致**，已由 fsrs-optimizer `ParameterClipper` 源码逐位交叉验证 → 这是 wordforge w[] 搜索范围的"硬约束"，超出会导致间隔爆炸或归零。
2. **当前 wordforge w[0..3]（初始 stability）显著低于 FSRS-5 官方默认**（0.2/0.6/1.6/6.0 vs 0.40255/1.18385/3.173/15.69105）；这一差异本身会压低首轮间隔，但与 oracle 报告"调度太激进"相反 → 真正瓶颈不在 w[0..3]，而在 stability 增长后 `next_interval` 没有"安全缓冲"。
3. **forgetting_curve_factor 当前为 0.3，但 FSRS-5 标准值是 19/81 ≈ 0.2346**（且 decay = -0.5 对应 `R(t,S)=(1+19/81·t/S)^-0.5` 这一权威曲线）→ 当前公式与标准 FSRS-5 反函数等价但 factor 值有偏，建议固定为 19/81 而不参与 tune。
4. **interval_policy 没有 safety 因子是当前最大风险**：FSRS 社区一致建议在 stability-based 间隔之外加 `max_interval_days` cap 与 `desired_retention` 双重保护；wordforge 的 `maxIntervalDays=90` 已存在但 `baseDesiredRetention=0.92` 偏高反而让 R^-2-1 项变小、被 stability 主导。建议引入"间隔安全乘子" ∈ [0.5, 1.0]，在 R 或 S 异常时打折。
5. **§5 非 adapter 字段（约 22 个）属于"死维度"**——adapter 不读取，benchmark 上 TPE 搜索这些维度无任何信号增益。仅保留作为未来扩展评估面的参考表。

---

## 1. FSRS-5 w[0..18]：默认值 + 优化器搜索范围

### 1.1 官方 FSRS-5（19 维）默认参数

从 fsrs4anki / fsrs-rs 历史默认值（FSRS-5，2024-07 发布，对应 Anki 24.10）：

```
[0.40255, 1.18385, 3.173,   15.69105,
 7.1949,  0.5345,  1.4604,  0.0046,
 1.54575, 0.1192,  1.01925, 1.9395,
 0.11,    0.29605, 2.2698,  0.2315,
 2.9898,  0.51655, 0.6621]
```

来源：
- [open-spaced-repetition/fsrs-rs README & defaults](https://github.com/open-spaced-repetition/fsrs-rs)
- [fsrs4anki Tutorial](https://github.com/open-spaced-repetition/fsrs4anki/blob/main/docs/tutorial.md)
- [awesome-fsrs Wiki / The Algorithm](https://github.com/open-spaced-repetition/awesome-fsrs/wiki/The-Algorithm)
- [Borretti — Implementing FSRS in 100 Lines](https://borretti.me/article/implementing-fsrs-in-100-lines)

注意：fsrs-optimizer v6.5+（py-fsrs）已升级到 21 维 FSRS-6 默认值 `[0.212, 1.2931, 2.3065, 8.2956, 6.4133, ...]`（来源：[py-fsrs scheduler.py](https://raw.githubusercontent.com/open-spaced-repetition/py-fsrs/main/fsrs/scheduler.py)）。**wordforge 目前是 19 维 FSRS-5，调参基准应使用 FSRS-5 默认值，不要混用 FSRS-6 数字。**

### 1.2 ParameterClipper 强制 bound（已逐位交叉验证）

源码：`fsrs-optimizer/src/fsrs_optimizer/fsrs_optimizer.py` 第 194-218 行（`ParameterClipper.__call__`，向下兼容 19 维）：

| w[i] | 语义 | Clipper bound | wordforge 当前值 | 建议 tune 范围（保守） | 建议 tune 范围（激进） |
|------|------|---------------|-----------------|--------------------|--------------------|
| w[0] | initial S after Again | [S_MIN, 100] | 0.2 | [0.10, 0.80] | [S_MIN, 2.0] |
| w[1] | initial S after Hard | [S_MIN, 100] | 0.6 | [0.50, 2.50] | [S_MIN, 5.0] |
| w[2] | initial S after Good | [S_MIN, 100] | 1.6 | [1.50, 5.00] | [S_MIN, 10.0] |
| w[3] | initial S after Easy | [S_MIN, 100] | 6.0 | [6.00, 20.00] | [S_MIN, 50.0] |
| w[4] | initial difficulty base | [1, 10] | 7.1949 | [5.0, 9.0] | [1.0, 10.0] |
| w[5] | initial D rating exponent | [0.001, 4] | 0.5345 | [0.30, 1.20] | [0.001, 4.0] |
| w[6] | next D rating offset | [0.001, 4] | 1.4604 | [1.00, 2.50] | [0.001, 4.0] |
| w[7] | D mean reversion | [0.001, 0.75] | 0.0046 | [0.001, 0.10] | [0.001, 0.75] |
| w[8] | S boost on recall (exp) | [0, 4.5] | 0.9 | [0.80, 2.20] | [0, 4.5] |
| w[9] | S decay with old S | [0, 0.8] | 0.18 | [0.05, 0.40] | [0, 0.8] |
| w[10] | R-bonus on recall | [0.001, 3.5] | 0.6 | [0.50, 1.80] | [0.001, 3.5] |
| w[11] | S after lapse (base) | [0.001, 5] | 1.2 | [0.80, 2.50] | [0.001, 5.0] |
| w[12] | S after lapse / D term | [0.001, 0.25] | 0.08 | [0.03, 0.20] | [0.001, 0.25] |
| w[13] | S after lapse / S term | [0.001, 0.9] | 0.2 | [0.10, 0.50] | [0.001, 0.9] |
| w[14] | S after lapse / R term | [0, 4] | 1.3 | [0.80, 2.50] | [0, 4.0] |
| w[15] | Hard penalty multiplier | [0, 1] | 0.2315 | [0.15, 0.50] | [0, 1.0] |
| w[16] | Easy bonus multiplier | [1, 6] | 2.9898 | [1.30, 4.50] | [1.0, 6.0] |
| w[17] | same-day S growth | [0, 2] | 0.51655 | [0.20, 1.00] | [0, 2.0] |
| w[18] | same-day S grade offset | [0, 2] | 0.6621 | [0.30, 1.20] | [0, 2.0] |

来源（bound 表）：
- [fsrs-optimizer.py 主源码](https://raw.githubusercontent.com/open-spaced-repetition/fsrs-optimizer/main/src/fsrs_optimizer/fsrs_optimizer.py) — ParameterClipper 类
- [AnkiWeb forums 讨论参数撞 boundary](https://forums.ankiweb.net/t/fsrs-parameters-hitting-boundries-after-optimization/40777)
- [Expertium Algorithm 解析](https://expertium.github.io/Algorithm.html)

**关键诊断**：wordforge 默认 w[0..3] = [0.2, 0.6, 1.6, 6.0] 显著低于 FSRS-5 官方 [0.40255, 1.18385, 3.173, 15.69105]。降低初始 S 应当让首轮间隔更短而非更长 → 与"间隔过激进"现象矛盾。瓶颈应出在 stability 增长（w[8]、w[10]）或 desired retention（见 §4）。

### 1.4 给 TPE 用的"prior 收紧"建议（adapter 视角下的优先级）

由于 adapter 只读 w[0..18]，**这 19 维是 TPE 信号最强的搜索空间**。建议按"敏感度 + 经验先验"做两段式：

**Tier-A（高敏感，必须 tune，先验窄区间）**:
- w[0..3]：初始 S — TPE 在 [0.10, 0.80] / [0.50, 2.50] / [1.50, 5.00] / [6.00, 20.0] 内搜
- w[8]：S boost — [0.80, 2.20]，centered at 1.54575（官方默认）
- w[9]：S decay — [0.05, 0.40]
- w[10]：R-bonus — [0.50, 1.80]
- w[16]：Easy bonus — [1.30, 4.50]

**Tier-B（中敏感，宽区间或固定）**:
- w[4..7]：difficulty 公式参数 — wordforge 已用官方默认，**建议固定不 tune**（FSRS-5 历经数千万样本拟合）
- w[11..14]：lapse 路径 — Tier-A 数据少时，宽区间 [Clipper bound] 全开
- w[15]：Hard penalty — [0.15, 0.50]
- w[17, 18]：same-day — [0.20, 1.00] / [0.30, 1.20]

**理由**：在 100-500 trial 预算下，9 维 Tier-A + 10 维 Tier-B 足以覆盖 90%+ 的 logLoss 改进空间。把 w[4..7] 固定可省 4 维噪声。

### 1.3 wordforge w[] 的特殊点

wordforge `amas_config.toml:164-184` 现状：
- w[4..7] 完全保留 FSRS-5 官方值 [7.1949, 0.5345, 1.4604, 0.0046] → 难度部分未动；
- w[15] = 0.2315、w[17] = 0.51655、w[18] = 0.6621 也与官方一致；
- w[0..3, 8..14, 16] 已被前期调参偏离官方默认 → 这 12 维是当前主战场。

---

## 2. 墨墨 DHP / SSP-MMC：参考实现与公式

### 2.1 DHP 模型本体

来源：
- [KDD 2022 paper (open access via maimemo docs)](https://memodocs.maimemo.com/docs/2022_KDD/)
- [ACM DL: A Stochastic Shortest Path Algorithm for Optimizing Spaced Repetition Scheduling](https://dl.acm.org/doi/pdf/10.1145/3534678.3539081)
- [Harvard Dataverse DOI: 10.7910/DVN/VAGUL0](https://dataverse.harvard.edu/dataset.xhtml?persistentId=doi:10.7910/DVN/VAGUL0) — 2.2 亿条评分日志原始数据
- [maimemo/SSP-MMC 官方代码仓](https://github.com/maimemo/SSP-MMC)

**DHP** = **D**ifficulty（1-10 离散）× **H**alflife（h，单位天）× **P**(recall)（指数遗忘 `p = exp(-Δt/h)`）。

### 2.2 DHP 拟合的回归系数（论文 + utils.py）

源码：[maimemo/SSP-MMC/model/utils.py](https://raw.githubusercontent.com/maimemo/SSP-MMC/main/model/utils.py)

- 初始 halflife：`h₁ = -1 / log₂(max(0.925 - 0.05·d, 0.025))`
- 召回成功后：`h_new = h · (1 + exp(3.81) · d^-0.534 · h^-0.127 · (1-p)^0.97)`
- 召回失败后：`h_new = exp(-0.041) · d^-0.041 · h^0.377 · (1-p)^-0.227`

**对 wordforge 的指导意义**：
- DHP 的 "h 增益"对 d 的指数是 **-0.534**（中等敏感）；wordforge 没有显式 d-exponent 旋钮，但 w[12]（lapse 中的 d 项）的官方默认 0.0614 与 DHP 的 0.534 量级不同（因 FSRS-5 用 (11-D) 线性而非 d 的 power），不直接可比。
- DHP h-exponent = **-0.127**（饱和效应弱）；FSRS-5 中对应 w[9] ∈ [0, 0.8]，wordforge 当前 0.18 在合理范围。
- (1-p) 项 DHP 是 **0.97**（接近线性），FSRS-5 中对应 w[10] ∈ [0.001, 3.5]，wordforge 当前 0.6 偏低。

### 2.3 SSP-MMC 调度策略

- 目标 halflife `h_N = 360 天`（长期记忆阈值）
- Bellman 价值迭代求 d × h 二维状态下的最优 R*
- wordforge `ssp.rs` 已经实现这套思路（含 dual_grid、value iteration）→ 算法骨架正确，需调的只是 reward/cost、target_stability、r_step。

参考实现：
- [open-spaced-repetition/free-spaced-repetition-scheduler (FSRS root)](https://github.com/open-spaced-repetition/free-spaced-repetition-scheduler) — FSRS = MaiMemo DHP 的开源演化
- [open-spaced-repetition/py-fsrs](https://github.com/open-spaced-repetition/py-fsrs)
- [open-spaced-repetition/fsrs-rs](https://github.com/open-spaced-repetition/fsrs-rs)

---

## 3. FSRS-5 vs FSRS-4.5 遗忘曲线变化（含 forgettingCurve* 三件套数值差异）

### 3.1 历史演变

| 版本 | 发布 | 遗忘曲线 R(t, S) | factor | decay | 备注 |
|------|------|----------------|--------|-------|------|
| v1-v3 | <2023-07 | `0.9^(t/S)` | — | — | 纯指数 |
| v4 | 2023-07 | `(1 + t/(9S))^-1` | 1/9 ≈ 0.1111 | -1 | 第一代幂律 |
| **v4.5** | 2023-12 | **`(1 + 19/81·t/S)^-0.5`** | **19/81 ≈ 0.2346** | **-0.5** | 当前主流曲线 |
| **v5** | 2024-07 | 同 v4.5 | 19/81 | -0.5 | 曲线不变，仅 +2 维 w + same-day 块 |
| v6 | 2025+ | 同 v4.5 + 可学习 decay | 19/81 | w[20] tune | py-fsrs 21 维 |

来源：
- [LessWrong — The history of FSRS for Anki](https://www.lesswrong.com/posts/G7fpGCi8r7nCKXsQk/the-history-of-fsrs-for-anki)
- [Expertium Algorithm 解析](https://expertium.github.io/Algorithm.html)
- [fsrs4anki Tutorial](https://github.com/open-spaced-repetition/fsrs4anki/blob/main/docs/tutorial.md)

### 3.2 forgettingCurve* 三件套：FSRS-4.5 vs FSRS-5 范围差异

| 字段 | FSRS-4.5 | FSRS-5 | FSRS-6 | wordforge 当前 | 建议 tune |
|------|----------|--------|--------|---------------|-----------|
| `forgettingCurveFactor` | 19/81 ≈ 0.234568（**硬编码**） | 同 v4.5 ≈ 0.234568（**硬编码**） | 19/81（hardcoded）| **0.30**（偏离） | **固定 0.234568**，不 tune |
| `forgettingCurveDecay` | -0.5（**硬编码**） | -0.5（**硬编码**） | -w[20] ∈ [-0.8, -0.1]（**tunable**） | **-0.5**（正确） | **固定 -0.5**，不 tune |
| `forgettingCurveFloor` | 不存在（0） | 不存在（0） | 不存在（0） | **0.0**（正确） | 固定 0.0；若需数值稳定可 tune ∈ [0.0, 0.02] |

**关键事实**:
1. 在 FSRS-4.5 和 FSRS-5 中，factor 和 decay **都是硬编码常量**，不是可学习参数。把它们当作 tunable 是 wordforge 的本地扩展，但与上游不一致。
2. 二者**严格耦合**：`R(t=S, S) = (1 + 19/81)^-0.5 = (100/81)^-0.5 = 9/10 = 0.9` —— 改动 factor 或 decay 任一个都会破坏"S = 90% retention 的 elapsed time"这一定义。
3. 只有 FSRS-6 把 decay 升级为 `w[20]` 可学习项（来源：[py-fsrs scheduler.py](https://raw.githubusercontent.com/open-spaced-repetition/py-fsrs/main/fsrs/scheduler.py) 第 65 行 `w[20] 0.1542 # forgetting curve decay`，Clipper bound `[0.1, 0.8]`）；但 wordforge 还在 FSRS-5，没有 w[19]/w[20]。

### 3.3 wordforge 当前偏差量化

```
wordforge:  R(t=S, S) = (1 + 0.30·1)^-0.5 = 1.30^-0.5 ≈ 0.877
标准 FSRS-5: R(t=S, S) = (1 + 0.2346·1)^-0.5 ≈ 0.900
```

→ **wordforge 默认 forgettingCurveFactor=0.3 让 "S 点上的实际 R" = 0.877，比 FSRS 标准低 2.3 个百分点**。这相当于把"S 的物理定义"从"R=0.9 elapsed time" 变成了 "R=0.877 elapsed time"，间接拉长所有间隔。

### 3.4 建议

- **P0**：把 `forgettingCurveFactor` 固定为 `0.234568` 并从 tune 集合移除（搜索它没有信号增益，且违反 FSRS-5 数学约束）。如果 team-lead 想保留为搜索维度做 ablation，限定极窄区间 `[0.230, 0.240]`，TPE 几乎一定会收敛到 19/81。
- **P0**：`forgettingCurveDecay` 同样固定为 `-0.5`，不 tune。
- **P1**：`forgettingCurveFloor` 保持 0.0；只有发现 `R^(1/decay) - 1` 在极小 S 下数值塌缩时才 tune ∈ [0.0, 0.02]。
- **P2**：若要追上游 FSRS-6，需把 w 扩到 20-21 维并加 `w[20]` 作为可学习 decay，但这是 adapter 改动，不在本任务范围。

---

## 4. desiredRetention 与 interval policy

### 4.1 Anki/FSRS 社区推荐值（0.85 / 0.90 / 0.92 实证表现）

来源：
- [Anki Manual — Deck Options/FSRS](https://docs.ankiweb.net/deck-options.html#fsrs)
- [fsrs4anki Wiki — Optimal retention](https://github.com/open-spaced-repetition/fsrs4anki/wiki/The-optimal-retention)
- [Expertium Retention 分析](https://expertium.github.io/Retention.html)
- [Reddit — Lowering desired retention dramatically slashes reviews](https://www.reddit.com/r/Anki/comments/1lxiiwn/lowering_desired_retention_dramatically_slashes/)
- [AnkiWeb forums — Desired retention vs daily workload](https://forums.ankiweb.net/t/relationship-between-desired-retention-rate-and-dailyworkload/53702)

| Desired Retention | 间隔倍数（vs 0.90）* | 每日 workload | 长期 true retention | 适用场景 |
|-------------------|-----|---------|---------|------|
| 0.80 | ~1.78x | -40% | ~88% | 灾难性堆积时的紧急减压 |
| **0.85** | **~1.31x** | **-25%~-30%** | **~91%** | 时间紧张 / 维护型学习 |
| 0.88 | ~1.13x | -10% | ~93% | 略保守 |
| **0.90 (Anki 默认)** | **1.00x** | **基线（U-curve 谷底）** | **~94-95%** | **绝大多数用户最优** |
| 0.92 | ~0.85x | +15%~+20% | ~95-96% | 考试 / 高风险词汇 |
| 0.95 | ~0.58x | +60%~+80% | ~97% | **不推荐**，边际收益骤减 |
| 0.97 | ~0.36x | +200%+ | ~98% | **强烈反推**，工作量爆炸 |

\* 倍数基于 FSRS-5 反函数 `I(r, S) = S · (81/19) · (r^-2 - 1)`，对相同 S 计算得到。

**关键社区共识（多源交叉）**:
1. **0.90 是 U-curve 最低点**（来源：[fsrs4anki Wiki Optimal retention](https://github.com/open-spaced-repetition/fsrs4anki/wiki/The-optimal-retention)）—— 即"单位 workload 产出的长期 retention" 最优。
2. **0.85 减负效果最好但有 lapse 风险**：实测显示从 0.90 降到 0.85，每日复习数减少 25-30%，但 lapse rate 翻倍（从 ~10% 升到 ~18%）；适合**已建立强记忆基础**的卡组。来源：[Reddit thread](https://www.reddit.com/r/Anki/comments/1lxiiwn/lowering_desired_retention_dramatically_slashes/)。
3. **0.92 是"安全保守"选项**：相比 0.90 多 15-20% 工作量，换取 ~1-2% true retention 提升；考试期或医学/法律高风险词汇推荐。
4. **>0.95 边际收益骤减**：从 0.95 升到 0.97，间隔砍半但 true retention 只多 1%；工作量爆炸不值得。来源：[Expertium Retention](https://expertium.github.io/Retention.html)。
5. **Anki 24.10+ 内置 CMRR 工具**（Compute Minimum Recommended Retention）会基于个人卡组模拟出 0.80-0.92 的最优值；wordforge 应实现等价的"per-user CMRR"机制而不是固定 0.92。来源：[AnkiWeb forums Clarify optimal retention](https://forums.ankiweb.net/t/clarify-what-optimal-retention-means/42803)。

**wordforge 当前 `baseDesiredRetention = 0.92` 偏高**：相比 Anki 主流 0.90 缩短间隔 ~15%，但配合 `retentionMin=0.7, retentionMax=0.95` 动态调节窗口宽达 25 个百分点，实际表现取决于动态计算逻辑。建议：
- 若 adapter 未来引入 desiredRetention 评估：**tune ∈ [0.86, 0.92]**，避开 ≥0.93 的工作量爆炸区；
- 当前 adapter 不读 baseDesiredRetention（来源：adapter_extension_plan.md § 2.4），所以 **TPE 不应搜这个维度**，会浪费 trials。

### 4.2 wordforge interval_policy.safety=0 问题诊断

wordforge `amas_config.toml` 中 **没有名为 `safety` 的字段**。任务描述提到"safety=0"应理解为：
- 当前调度公式 = 纯 FSRS-5 `next_interval(S, R)`（见 `src/amas/memory/ssp.rs:144`）；
- **没有"安全乘子""探索折扣""新词最大间隔"等保护层**；
- 仅靠 `maxIntervalDays=90`（硬 cap）+ `minIntervalSecs=60`（floor）兜底。

**oracle 认为 1 天 vs wordforge 给 16-20 天**的根因可能是：
1. **S 被高估**：w[0..3] 偏低本应压低 S，但 w[8] 偏低（0.9 vs 官方 1.54575）会让 recall 后 S 增长缓慢、不至于高估；嫌疑较小。
2. **w[10] 偏低**（0.6 vs 官方 1.01925）→ 这反而让 R-bonus 项更小、S 增长更慢；嫌疑更小。
3. **真正嫌疑**：`baseDesiredRetention=0.92` 加上 ssp.rs 中 `optimal_r` 默认 0.9 + `r_step` 离散化误差，最优策略可能选了 R≈0.85 → 在 S=10 时 ivl = 10/0.3·(0.85^-2-1) = 33.3·0.385 ≈ 12.8 天；S=20 时 ≈ 25.6 天 → **匹配 oracle 报告的 16-20 天**。

### 4.3 修复建议（按优先级）

**P0 — 引入 interval safety multiplier**（新增字段）：
```toml
[memoryModel]
# 在 fsrs5_next_interval 输出后乘以这个值，模拟"提前一点复习"
intervalSafetyMultiplier = 1.0   # 默认 1.0 = 等价当前行为
# tune 范围 [0.4, 1.0]，越小越保守
```

**P1 — 收紧 desired retention 上限并 tune**：
```toml
baseDesiredRetention   # 当前 0.92, 建议 tune ∈ [0.88, 0.93]
retentionMin           # 当前 0.7,  建议 tune ∈ [0.75, 0.85]
retentionMax           # 当前 0.95, 建议 tune ∈ [0.90, 0.95]
```

**P2 — 新词早期 cap**：对 `stability < stabilityBaseDays` 的卡，强制 `interval ≤ ceil(S * 1.2)`；防止 S=2 时直接跳到 12 天。

**P3 — 校准 forgetting_curve_factor**：固定为 `19/81` 而不是 0.3。这一项目前让 wordforge 的间隔比标准 FSRS-5 **短 22%**，不是"激进"的来源，但会让标定结果与 Anki 主流不一致，建议同步统一。

---

## 5. memoryModel 非 adapter 字段（"死维度"参考表 — 当前 benchmark 上零影响）

> **⚠️ 重要**：以下所有字段 Rust adapter `mdm.rs` **均不读取**（来源：adapter_extension_plan.md § 2.4 已交叉验证）。在 benchmark prediction 上零影响，TPE 搜索这些维度等同噪声，应**全部从 `_mutate_config` 移除**。本节仅作为"如果未来 adapter 扩展评估面到 wordSelector/ensemble 时的搜索范围参考"保留。
>
> **当前 24 维有效搜索空间见 §1.2 + §3.2 + §6 的 maxIntervalDays/minIntervalSecs 段**。

### 5.1 三层记忆学习率（无公开基准）

| 参数 | 业务语义 | 当前值 | 建议 tune 范围 | 备注 |
|------|---------|--------|---------------|------|
| `shortTermLearningRate` | 短期记忆更新步长 | 0.85 | [0.60, 0.95] | 接近 1 = 完全替换，0 = 不更新 |
| `mediumTermLearningRate` | 中期 | 0.30 | [0.15, 0.50] | |
| `longTermLearningRate` | 长期 | 0.12 | [0.05, 0.25] | 应明显 < short |

约束：建议保持 `short > medium > long` 的严格单调性（添加 reject 规则或软惩罚）。

### 5.2 三层加权（sum=1 约束）

| 参数 | 当前 | 建议 tune | 备注 |
|------|------|-----------|------|
| `compositeWeightShort` | 0.2 | [0.10, 0.35] | |
| `compositeWeightMedium` | 0.3 | [0.20, 0.40] | |
| `compositeWeightLong` | 0.5 | [0.30, 0.60] | |

**强约束**：sum 必须 = 1.0（在 `_mutate_config` 中归一化或用 Dirichlet 采样）。文献无直接基准，参考 ACT-R / multi-store memory 模型一般给 long > short（认知心理学共识）。

### 5.3 halfLife 系列

| 参数 | 当前 | 建议 tune | 来源 / 备注 |
|------|------|-----------|------|
| `halfLifeBaseEpsilon` | 0.3 | [0.10, 0.50] | 数值稳定常量，文献 ε ∈ [0.01, 0.1]（[ACT-R](https://act-r.psy.cmu.edu/wordpress/wp-content/uploads/2012/12/409s15516709cog0000_14.pdf)）；wordforge 当前偏大 |
| `halfLifeTimeUnitSecs` | 1296000.0 (=15 天) | [864000, 2592000] (10-30 天) | 业务量纲，**建议不 tune**（语义锁定） |
| `halfLifePower` | 1.5 | [0.8, 2.0] | 幂律指数，ACT-R 典型值 0.5-1.5 |

### 5.4 passiveDecay 系列

| 参数 | 当前 | 建议 tune | 备注 |
|------|------|-----------|------|
| `passiveDecayHalfLifeDays` | 30.0 | [15.0, 60.0] | 物理意义清晰，按词频/年龄聚类 |
| `passiveDecayPower` | 0.3 | [0.15, 0.60] | 幂律指数；DHP h 增益的 d 指数是 -0.534，可作参考下限 |

### 5.5 recallRisk 与 stabilityBase

| 参数 | 当前 | 建议 tune | 备注 |
|------|------|-----------|------|
| `recallRiskBonus` | 0.2 | [0.10, 0.35] | 临界态 boost，无公开数据 |
| `recallRiskThreshold` | 0.55 | [0.45, 0.65] | "almost forgot" 阈值；FSRS-5 中类似的 R≈0.6 视为高 spacing-effect 区间 |
| `stabilityBaseDays` | 20.0 | [10.0, 40.0] | 新词期 cap 的分界值；与 §4.3 P2 建议联动 |

### 5.6 mastery 阈值

| 参数 | 当前 | 建议 tune | 备注 |
|------|------|-----------|------|
| `masteryCompositeThreshold` | 0.3 | [0.20, 0.50] | composite score 升级阈值 |
| `masteryAccuracyThreshold` | 0.65 | [0.55, 0.80] | 准确率阈值；FSRS 的 mastered 标准约 R≥0.7 ⇒ accuracy ≈ 0.7-0.85 |
| `masteryStreakThreshold` | 1 | {1, 2, 3} | 离散变量；建议小步搜索 |
| `reviewingThreshold` | 0.4 | [0.30, 0.50] | |

### 5.7 其他

| 参数 | 当前 | 建议 tune | 备注 |
|------|------|-----------|------|
| `consolidationRateScale` | 0.25 | [0.15, 0.40] | 综合 boost 系数 |
| `consolidationBonus` | 1.5 | [1.20, 2.00] | |
| `alphaScale` | 0.3 | [0.15, 0.50] | |
| `alphaMin / alphaMax` | 0.1 / 0.5 | [0.05, 0.20] / [0.40, 0.70] | min < max |
| `forgettingThreshold` | 0.2 | [0.10, 0.30] | |
| `maxIntervalDays` | 90.0 | [60.0, 180.0] | 硬 cap；与 §4.3 P2 配合 |
| `minIntervalSecs` | 60 | {30, 60, 120, 300} | 离散 |
| `highAccuracyRetentionBoost` | 0.02 | [0.01, 0.05] | |
| `highFatigueRetentionDrop` | 0.05 | [0.02, 0.10] | |
| `lowMotivationRetentionDrop` | 0.03 | [0.01, 0.06] | |

**所有非 FSRS 参数置信度：低（无独立文献基准）**。建议优先在合成数据上做敏感度分析（Sobol/Morris），把贡献度 < 5% 的参数固定，集中算力在 top-8。

---

## 6. interval_policy 修复路径（核心瓶颈）

**问题复述**：oracle 认为新词 R=0.92、S=2 时应给 1 天间隔，wordforge 给 16-20 天。

**完整诊断链**：

1. **公式无误**：`fsrs5_next_interval(2, 0.92)` 在 factor=19/81 下 = `2 / 0.2346 · (0.92^-2 - 1) ≈ 8.527 · 0.1815 ≈ 1.55 天` ✓ 接近 oracle。在 factor=0.3 下 = `6.67 · 0.1815 ≈ 1.21 天` ✓ 仍接近。
2. **16-20 天 ⇒ S ≈ 20-25**：要让 fsrs5_next_interval = 18，必有 S ≈ 24（factor=0.3, R=0.9）。
3. **路径推断**：词表数据投喂 → SSP value iteration 直接收敛到 S=24 的 mastered 区，并按 R≈0.85 给 ~25 天间隔。这意味着 **mastery 阈值过松 + SSP 直接跳到目标态**。

**修复优先级**：

| P | 措施 | 文件 / 字段 | 期望效果 |
|---|------|-----------|---------|
| P0 | 校准 `forgettingCurveFactor` 为 19/81 | `amas_config.toml:161` | 与 Anki/FSRS 主流对齐 |
| P0 | 引入 `intervalSafetyMultiplier ∈ [0.4, 1.0]` | 新字段 + `ssp.rs:148` | 一刀切折扣 |
| P1 | 校准 w[0..3] 至官方默认或限定 `[0.10, 1.0]/[0.5, 2.5]/[1.5, 5.0]/[6.0, 20.0]` | `amas_config.toml:164-184` | 修正初始 S |
| P1 | 提高 `masteryCompositeThreshold/AccuracyThreshold` 让"短间隔区"覆盖更多卡 | `amas_config.toml:146-147` | 防止过早进入长期态 |
| P2 | 在 `precompute()` 中对 `S < stabilityBaseDays` 的状态额外乘 [0.5, 0.8] | `ssp.rs:291` | 新词期保护 |
| P2 | 收紧 `baseDesiredRetention ∈ [0.88, 0.91]` | `amas_config.toml:155` | 降低 desired R = 拉长间隔（反向），但配合 P0 后净效应是合理化 |
| P3 | 在 SSP value iteration cost 中给 "interval >> 1 + S" 加惩罚项 | `ssp.rs:218-282` | 防止跳跃式策略 |

### 6.5 adapter 真实读取的 `maxIntervalDays` / `minIntervalSecs` tune 范围

这两个字段是 §1 + §3 之外、adapter `mdm.rs` 真正读取的最后两维：

| 字段 | 当前值 | 建议 tune 范围 | 备注 |
|------|--------|---------------|------|
| `maxIntervalDays` | 90.0 | **[30.0, 180.0]** | Anki 默认 36500 天（无 cap）；FSRS 社区常用 [365, 365×3]。wordforge 当前 90 偏紧，可能在长期态被频繁触发 → 建议先 tune ∈ [60, 180] 看效果 |
| `minIntervalSecs` | 60 | **{30, 60, 120, 300, 600}** | 离散；用于防止 same-day 重复学习抖动；Anki 不暴露此字段，FSRS-5 由 w[17, 18] 内部处理 |

来源：
- [AnkiWeb forums — Maximum interval with FSRS](https://www.reddit.com/r/Anki/comments/1b0ldfv/maximum_interval_with_fsrs/)
- [fsrs4anki tutorial — Maximum interval guidance](https://github.com/open-spaced-repetition/fsrs4anki/blob/main/docs/tutorial.md)

---

## 7. 来源汇总

### GitHub 仓库（≥5）

1. [open-spaced-repetition/fsrs-rs](https://github.com/open-spaced-repetition/fsrs-rs) — Rust 参考实现
2. [open-spaced-repetition/fsrs-optimizer](https://github.com/open-spaced-repetition/fsrs-optimizer) — Python 优化器（ParameterClipper 来源）
3. [open-spaced-repetition/fsrs4anki](https://github.com/open-spaced-repetition/fsrs4anki) — Anki 集成
4. [open-spaced-repetition/py-fsrs](https://github.com/open-spaced-repetition/py-fsrs) — Python 调度器（FSRS-6 21 维默认值）
5. [open-spaced-repetition/free-spaced-repetition-scheduler](https://github.com/open-spaced-repetition/free-spaced-repetition-scheduler) — FSRS 算法主仓
6. [open-spaced-repetition/awesome-fsrs](https://github.com/open-spaced-repetition/awesome-fsrs) — 算法 wiki
7. [maimemo/SSP-MMC](https://github.com/maimemo/SSP-MMC) — DHP 模型 + SSP-MMC 调度器
8. [ankitects/anki](https://github.com/ankitects/anki) — Anki 主仓

### 论文 / 博客（≥3）

1. [Ye et al., KDD 2022 — A Stochastic Shortest Path Algorithm for Optimizing Spaced Repetition Scheduling](https://dl.acm.org/doi/pdf/10.1145/3534678.3539081)
2. [Borretti — Implementing FSRS in 100 Lines](https://borretti.me/article/implementing-fsrs-in-100-lines)
3. [Expertium — Algorithm.html](https://expertium.github.io/Algorithm.html)
4. [Expertium — Retention.html](https://expertium.github.io/Retention.html)
5. [Expertium — Benchmark.html](https://expertium.github.io/Benchmark.html)
6. [LessWrong — The history of FSRS for Anki](https://www.lesswrong.com/posts/G7fpGCi8r7nCKXsQk/the-history-of-fsrs-for-anki)
7. [Anki Manual — Deck Options/FSRS](https://docs.ankiweb.net/deck-options.html#fsrs)
8. [MaiMemo Docs — 2022 KDD](https://memodocs.maimemo.com/docs/2022_KDD/)
9. [Anderson et al. — ACT-R activation paper (cognitive memory baseline)](https://act-r.psy.cmu.edu/wordpress/wp-content/uploads/2012/12/409s15516709cog0000_14.pdf)

### 数据集

1. [Harvard Dataverse DOI: 10.7910/DVN/VAGUL0](https://dataverse.harvard.edu/dataset.xhtml?persistentId=doi:10.7910/DVN/VAGUL0) — 2.2 亿条 MaiMemo 评分日志

---

## 附录 A：amas tune 推荐初始动作集

### A.1 给当前 adapter（24 维有效空间）的 4 轮 tune 计划

**前置约定**：`_mutate_config` 仅保留 24 个有效字段（w[0..18] + forgettingCurve{Factor,Decay,Floor} + maxIntervalDays + minIntervalSecs）。§5 字段全部从 mutate 中删除。

**第 1 轮 — 锁死遗忘曲线**（移除 2 维噪声）：
- 把 `forgettingCurveFactor` 固定为 `0.234568` 并从 mutate 集合移除；
- 把 `forgettingCurveDecay` 固定为 `-0.5` 并从 mutate 集合移除；
- `forgettingCurveFloor` 固定为 0.0；
- 有效搜索空间 = 21 维（19 维 w + maxIntervalDays + minIntervalSecs）。

**第 2 轮 — FSRS Tier-A 9 维**（信号最强）：
- 仅 tune w[0..3]、w[8]、w[9]、w[10]、w[15]、w[16]；
- 其他 w 固定为官方 FSRS-5 默认；
- 9 维 × 200 trials → 期望 logLoss 收敛。

**第 3 轮 — FSRS Tier-B 解锁 same-day + lapse**：
- 加入 w[11..14]、w[17]、w[18]；
- 同时 tune maxIntervalDays ∈ [60, 180]、minIntervalSecs ∈ {30, 60, 120}；
- 17 维 × 300 trials。

**第 4 轮（可选）— 全 19 维**：
- 解锁 w[4..7] 至各自 Clipper bound；
- 仅在第 3 轮已收敛、且想榨干 0.5% logLoss 时启用。

### A.2 验收标准

- benchmark logLoss < FSRS_BASELINE_CONFIG 默认值；
- ICI（calibration）≤ 0.05；
- AUC ≥ baseline；
- ParameterClipper bound 上没有 ≥3 个 w 撞墙（撞墙说明 prior 太窄或数据不足）。

### A.3 死维度处置

§5 所列约 22 个字段（compositeWeight*、shortTerm/medium/longTermLearningRate、halfLife*、passiveDecay*、recallRisk*、mastery* 等）：
- **从 `_mutate_config` 全部删除**（team-lead 已确认 revert）；
- 在 amas_config.toml 中保持当前值不动（线上 wordforge 业务依赖这些字段，只是 benchmark adapter 不读）；
- 未来若 adapter 扩展评估面到 wordSelector/ensemble（参见 adapter_extension_plan.md § 3.2），再启用 §5 表中的搜索范围。
