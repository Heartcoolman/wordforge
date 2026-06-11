# AMAS 调参第四代：预注册搜索空间、目标函数与闸门（2026-06-12）

> **预注册纪律**：本文档在搜索启动前写定并提交 git，搜索启动后不得修改任何闸门数值或公式。
> 选择信号 = maimemo **val** split 唯一；duolingo / synthetic 仅作为对**单一胜者**的最终二元
> 接受/拒绝闸门进入。TEST split 在 Phase 5 一次性评估，本阶段绝不触碰。

## 1. 搜索空间（15 维）

实现：`benchmarks/maimemo/pipeline.py` `SEARCH_WINDOWS`。基座配置 = 当前生产冠军
（NM0 w + alphaRampTau 0.0，`config.py DEFAULT_MEMORY_MODEL_CONFIG`，与 `amas_config.toml` 同步）。

| 维度 | 窗口 | 变更 | 理由 |
|---|---|---|---|
| w_0 | (0.05, 0.60) | 不变 | — |
| w_2 | (1.00, **8.50**) | 5.00→8.50 拓宽 | 见下 |
| w_4 | (3.50, 9.50) | 不变 | — |
| w_5 | (0.30, 2.00) | 不变 | — |
| w_6 | (1.00, 4.00) | 不变 | — |
| w_7 | (0.001, 0.15) | 不变 | — |
| w_8 | (1.00, **3.50**) | 3.00→3.50 拓宽 | 见下 |
| w_9 | (0.05, 0.40) | 不变 | — |
| w_10 | (0.30, **2.00**) | 1.50→2.00 拓宽 | 见下 |
| w_11 | (0.50, 3.00) | 不变 | — |
| w_12 | (0.01, 0.25) | 不变 | — |
| w_13 | (0.05, 0.70) | 不变 | — |
| w_14 | (0.50, 3.50) | 不变 | — |
| w_20 | (0.10, 0.80) | 不变 | — |
| **alphaRampTau** | **(1.5, 4.0)** | **新增（非 w 结构旋钮）** | 见下 |

**拓宽理由（w_2 / w_8 / w_10）**：上一代"公版即最优"结论是在 alpha=0.3 冻结语义下得出的，
D1（重放保真连击动态 alpha，f9d506d）+ D2（alphaRampTau 证据递减阻尼，f3c1a6 系）落地后该
结论失效；duo/syn 的 mastered 残差（vs fsrs45 x1.68 / vs amas6 x1.57）需要稳定性增长侧
headroom。记忆质量塌方由 §3 的 per-trial 守门下限与 §4 的胜者级二元闸门双层兜底。

**alphaRampTau 入搜索理由**：val tau 网格（stock NM0 w）显示 tau=3.0 为内点最优（tau=4.0
全腿回落；tau∈{1.5, 2.0} 触 duo expMemFinal 塌方线 −52.7%/−35.7% 被拒）；峰值可能随 w 移动。
tau=0.0（关闭）不在 TPE 分布内，仅作为锚点种子在窗口外注入（optuna 4.8.0 对窗口外 fixed
param 仅发 UserWarning 并逐字透传，已实测 30-trial TPE 不崩，并由
`test_anchor_seed_out_of_window_passes_verbatim` pin 住）。

**教师种子（14 个）**：trial 0 = stock 锚点（NM0 w + tau=0.0，约束接线自检靶）；11 个性能
教师统一 tau=3.0；2 个 tau 阶梯教师（stock w + tau∈{2.5, 3.5}）。

## 2. 目标函数（预注册，不得事后修改）

- **Stage 1 / Stage 2 / D10 精炼**（实现常量 `MASTERED_OBJECTIVE_WEIGHT=0.5`、
  `MASTERED_OBJECTIVE_RATIO_CAP=1.5`）：

  ```
  objective = prediction_composite(ratio_cap=1.5)
            + 0.5 × min(masteredProxy / baseline_masteredProxy − 1.0, 1.5)
  ```

  其中 prediction_composite = 0.35·logLoss 比率 + 0.30·ICI 比率 + 0.20·AUC 比率 +
  0.15·maeP 比率（vs 基线，分量截断 1.5）；masteredProxy 来自 maimemo DHP 90 天闭环
  （`run_wordforge_reference`），基线 = (NM0, tau=0.0)，与三条 DHP 腿共用同一备忘缓存。

- **Stage 3**：保持纯 prediction composite（uncapped）不变；masteredProxy 仅记录不进目标。
- nullReplicates 诊断行保持 prediction-composite-only 口径（stock 的 mastered 项恒为 0）。

**masteredProxy 定义**（`dhp_reference.py`，镜像 `simulate.py` 排行榜口径）：闭环终态
调度器自报 next-interval ≥ 30 天的卡片数（引入即复习，`last_date is not None`）。

## 3. Per-trial 守门下限（DHP 闭环，重校准）

基线 = (NM0 w, tau=0.0) = 当前生产冠军。2026-06-12 tau 网格实测（deck 5000 / 90d /
budget 200 / seed 42）：

| tau | expectedMemory | 比率 | nextDayMemory | 比率 | targetCount | 比率 | masteredProxy | 比率 | avgDueRecall |
|---|---|---|---|---|---|---|---|---|---|
| 0.0（基线） | 2147.8 | 1.0000 | 2121.9 | 1.0000 | 691 | 1.0000 | 833 | 1.0000 | 0.8216 |
| 2.5 | 2497.7 | 1.1629 | 2456.2 | 1.1576 | 696 | 1.0072 | 1627 | 1.9532 | 0.7632 |
| 3.0 | 2583.2 | 1.2027 | 2524.9 | 1.1899 | 684 | 0.9899 | 1628 | 1.9544 | 0.7698 |
| 3.5 | 2539.9 | 1.1825 | 2488.8 | 1.1729 | 709 | 1.0260 | 1631 | 1.9580 | 0.7741 |

**实测推翻"tau 降 expectedMemory 致旧腿全不可行"假设**：maimemo 闭环内 tau>0 反而抬升
expectedMemory/nextDayMemory（+16~20%，预算约束下 mastered 卡片释放复习预算的二阶效应）；
旧 0.9x 下限对 tau 候选全部可行，故 expectedMemory/nextDayMemory 腿**不放松**。

**选定下限**（`DHP_GUARDRAIL_FLOORS`，约束分量 = floor − value/base，锚点自检靶
= (−0.10, −0.10, −0.05, 0.00)）：

| 腿 | 下限 | tau=3.0 stock 实测比率（余量） | 理由 |
|---|---|---|---|
| expectedMemory | ≥ 0.90× | 1.2027（+0.30） | 沿用历史纪律；防 w 侧记忆质量塌方 |
| nextDayMemory | ≥ 0.90× | 1.1899（+0.29） | 同上 |
| targetCount | ≥ **0.95×**（0.90 收紧） | 0.9899（+0.04） | tau 对该腿近中性（0.99–1.03），更大跌幅只能来自 w 侧用真实半衰期（DHP 学生 oracle 侧）换自报 mastered 的套利；任务书草案 1.0× 经实测否决（tau=3.0 stock = 0.9899 < 1.0 会把锚定 tau 区间整体判死） |
| masteredProxy | ≥ **1.00×**（新腿） | 1.9544（+0.95） | 冲榜指标本体：胜者不得低于现冠军自报掌握量；自报口径的防套利由 targetCount 0.95× + §4 胜者级 oracle 效率/expMemFinal/retStab 闸门兜底 |

## 4. 胜者级二元闸门（Phase-4 Validate 逐字应用）

对搜索产出的**单一胜者**（含写回 tau 值）在 worktree val harness
（strategies=['amas','fsrs45','amas6']，seed 42）上逐条判定，全过才进入 Phase 5 test 一次性评估：

1. **maimemo val 全量**：
   - predictionGain ≥ 0.5%（vs post-fix tau3-stock 参照）；
   - ICI ≤ 0.08；
   - maeP 不劣于 tau3-stock（≤ 0.2188）。
2. **duolingo val**：
   - mastered > 5350（fsrs45 锚点）；
   - efficiency ≥ 0.95 × 0.0298 = 0.02831（fsrs45 锚点）；
   - expMemFinal ≥ 0.65 × 6096 = 3962.4（tau0-stock；塌方线证据：tau=3.0 −20.9% 过线，tau=1.5 −52.7% 拒）。
3. **synthetic val**：
   - mastered > 34089（amas6 锚点）；
   - AUC ≥ 0.45，且 AUC < 0.50 时随结果显式披露"低于随机线"；
   - expMemFinal ≥ 0.65 × 34822 = 22634.3（tau0-stock）。
4. **全三数据集**：retentionStability ≥ tau3-stock 同数据集值 − 0.01，即
   maimemo ≥ 0.8529、duolingo ≥ 0.7475、synthetic ≥ 0.8330。
5. **种子稳定性**：胜出腿（各数据集 mastered 超越锚点的差值）≥ 2× 跨种子标准差。

闸门锚点数值冻结来源：Phase-4 VAL 实测（tau 网格，2026-06-11/12），
post-fix tau3-stock = (NM0 w, tau=3.0)。

## 5. 披露清单（随最终报告一并发布）

1. **tau 先验选择已见 duo/syn**：alphaRampTau 窗口 (1.5, 4.0) 与 tau∈{1.5, 2.0} 的排除依据
   来自 duolingo/synthetic val 的 tau 网格——duo/syn 对"tau 这一维的先验"非盲；对"本轮 w×tau
   联合搜索的胜者"仍仅以 §4 二元闸门进入。
2. **masteredCount 自报口径**：mastered = 调度器自报 next_interval ≥ 30d，存在自利偏差；
   防御 = oracle 效率下限 + expMemFinal 下限 + retentionStability 下限（§4，全部入闸，不只叙事）。
3. **oracle 复用**：效率指标依赖各数据集既有 GRU-HLR oracle（与前代评估同一 artifacts），
   非为本轮重训；oracle 误差对所有策略同向作用。
4. **amas6 条目继承分带 clamp 移除**（f9d506d 评估侧变更，算法中性）：已实测对 amas6 val
   数字零影响；leaderboard 终表将以单次运行整体再生成。
5. **fsrs 条目 lockstep-by-design**：`FSRS_BASELINE_CONFIG` 显式钉死 alphaRampTau=0.0 与
   FSRS-5 官方 w/曲线，不随 DEFAULT 写回漂移（竞品隔离）。
6. **历史 leaderboard amas 数字作废**：口径修正（镜像对齐）非性能退化（2026-06-11 备忘）。

## 6. 评估卫生

- 所有评估侧改动算法中性；10 条目 leaderboard 最终一次运行整体再生成。
- 写回纪律：胜者 w + tau 由人工写回 `amas_config.toml` 与 `config.py` DEFAULT；本文档所在
  commit **不**改动二者。
- 测试副作用：提交前 `git diff` 核查 `amas_config.toml` / `openapi.yaml` / `schema.json`。
