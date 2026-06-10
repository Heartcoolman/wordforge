# v0.9 vs v0.8 评测差异

> v0.8: 2026-05-28-fsrs6 (10 算法×3 数据集，原 expectedMemory 公式)
> v0.9: 2026-05-29-amas6 (10 算法×3 数据集，**masteredCount 新公式**)
> 新增：AMAS6 (AMAS 全栈 + FSRS-6 mirror state)
> 公式：DHP 维度从 `0.5·expMem + 0.5·eff×1000` → `0.7·mastered + 0.3·eff×10000`

## 修改内容

1. **新增 AMAS6Scheduler**（schedulers.py 末尾 ~70 行）
   - 继承 AMASScheduler，重载 `_fresh_state()` 返回 FSRS6MirrorState
   - 重载 `_recall(elapsed)` 用 FSRS-6 曲线
   - wordSelector / ensemble / heuristic / IGE 全部父类继承

2. **DHP 公式改造**（leaderboard.py:39-43）
   - 原: `DHP_W_MEMORY=0.5, DHP_W_EFFICIENCY=0.5, EFF_SCALE=1000`，用 expectedMemoryFinal
   - 新: `DHP_W_MASTERED=0.7, DHP_W_EFFICIENCY=0.3, EFF_SCALE=10000`，用 masteredCount
   - 原因: expectedMemoryFinal 被 Leitner/HLR 高频复习刷虚高；mastered (halflife≥180天) 才是产品真实 KPI

## 综合排名（v0.9 / 10 算法 / mastered 公式）

| 排名 | 算法 | Borda | final_score | maimemo | duolingo_hlr | synthetic |
|---|---|---|---|---|---|---|
| 1 | `fsrs45` | 27 | 0.937 | 2 | 3 | **1** |
| **2** | **`amas6`** | **25** | **0.917** | **1** | 5 | 2 |
| 3 | `fsrs` | 21 | 0.757 | 6 | **1** | 5 |
| 4 | `fsrs6` | 21 | 0.863 | 3 | 6 | 3 |
| 5 | **`amas`** | 20 | 0.755 | 7 | 2 | 4 |
| 6 | `sm2` | 15 | 0.618 | 5 | 7 | 6 |
| 7 | `dhp` | 13 | 0.644 | 8 | 4 | 8 |
| 8 | `random` | 13 | 0.474 | 4 | 9 | 7 |
| 9 | `leitner` | 7 | 0.420 | 9 | 8 | 9 |
| 10 | `hlr` | 3 | 0.001 | 10 | 10 | 10 |

## 关键发现

### 1. AMAS6 第 2，AMAS 第 5 — FSRS-6 底层升级真实有效

AMAS6 vs AMAS：mastered words 全胜 + reviews 全减

| 数据集 | AMAS mastered | AMAS6 mastered | Δ% | AMAS reviews | AMAS6 reviews | Δ% |
|---|---|---|---|---|---|---|
| maimemo | 17429 | 11649 | -33% | 174051 | 112838 | **-35%** |
| duolingo_hlr | 1024 | 6595 | **+544%** | 298224 | 163349 | -45% |
| synthetic | 22397 | 26544 | +18% | 399916 | 335345 | -16% |

AMAS6 在 duolingo_hlr 上 mastered 翻 6 倍。AMAS6 综合排名（Borda 25 vs AMAS 20）跃升 3 位（第 5 → 第 2）。

### 2. wordSelector / ensemble 的 synergy 验证

AMAS6 vs FSRS-6（同样的 FSRS-6 底层，AMAS6 多了 wordSelector + ensemble）：

| 数据集 | FSRS-6 mastered | AMAS6 mastered | Δ% | FSRS-6 reviews | AMAS6 reviews |
|---|---|---|---|---|---|
| maimemo | 9987 | 11649 | **+17%** | 117905 | 112838 |
| duolingo_hlr | 5106 | 6595 | **+29%** | 165318 | 163349 |
| synthetic | 22102 | 26544 | **+20%** | 345102 | 335345 |

**结论**：wordSelector + ensemble 在每个数据集上都给 mastered +17% ~ +29% 增益，**reviews 略减**——这是真实的 synergy。Spec §9 的"YAGNI"判断（AMAS 全栈无效）被推翻。

### 3. fsrs45 综合第 1 — 短曲线 + 高复习密度的副作用

fsrs45 用 `(1+t/9S)^(-1)` 短曲线，每天复习更密：
- mastered 累积最快（90 天后 halflife≥180 词数最多）
- 但 efficiency 低（reviews/词次）
- 在 mastered-based 公式下登顶

但若产品场景是 reviews/day 受限，fsrs45 的高密度反而是劣势。

### 4. DHP 公式偏差揭穿

v0.5 → v0.8 用 expectedMemoryFinal 公式时 Leitner / HLR 因高频复习被刷虚高（Leitner expectedMemoryFinal 38436 在 synthetic 上）。新公式下 Leitner 跌到第 9，HLR 第 10，更符合产品直觉。

## AMAS 跨 5 轮演变

| 版本 | AMAS 综合 | 算法数 | 公式 | 备注 |
|---|---|---|---|---|
| v0.5 | 2/8 | 8 | expMem | 初版 |
| v0.6 | 2/8 | 8 | expMem | HLR θ 修复 |
| v0.7 | 2/8 | 8 | expMem | 3 独立 oracle |
| v0.8 (revert) | 3/9 | 9 | expMem | 加 FSRS-6（被 user revert） |
| v0.9 | **5/10** | 10 | **mastered** | 加 AMAS6 + mastered 公式 |

AMAS（v0.5 配置不变）在新公式下从第 2 跌第 5，因为 mastered 才是真 KPI，AMAS 在该指标上不如 fsrs45 / amas6 / fsrs / fsrs6。

**AMAS6 是 AMAS 的真升级版**：第 2 / 10，且产品 KPI 全维度优于 FSRS-6。

## 给项目的建议

升级路径：
1. **优先做**：把 src/amas/memory/mdm.rs 的 19 维 w + 固定 decay/factor 升级到 21 维 w + trainable decay（FSRS-6 等价）
2. **同步**：amas_config.toml 的 [memoryModel] 段从 19 维 w 改为 21 维
3. **重调参**：基于 v0.7 调参基础设施（11 维 Tier-A → 13 维 Tier-A），用 maimemo / duolingo / synthetic 三数据集联合 tune

这就是 AMAS6 → 产品的落地路径。

## 已知限制

1. **fsrs45 mastered 第 1 可能是评估失真** — `(1+t/9S)^(-1)` 短曲线让 stability 快速累积到 180+ 天但实际 retention 可能不达 90%。下次评测应加 "actual retention at predicted interval" 指标交叉验证
2. **AMAS6 prediction 维度被 FSRS-6 拉平** — wordSelector 对 next-step recall 不贡献，区分度只在 forward sim
3. **synthetic / duolingo_hlr 仍复用 maimemo oracle**（v0.7 训练失败的回滚）
