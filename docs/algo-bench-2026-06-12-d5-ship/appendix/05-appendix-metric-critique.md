# 非官方口径 · 披露性附录：dhp_raw 主指标的结构性偏好与替代口径重算

> **定位声明**：本附录是披露性材料，**不是官方排名的一部分**。官方榜以
> [01-leaderboard.md](../01-leaderboard.md) 为准（公式 = `benchmarks/maimemo/leaderboard.py` v0.9 现行口径）。
> 本文所有替代榜单均标注「非官方」，目的是论证**多指标阅读**的必要性，而非替换官方分数。
> 本文是对**指标设计**的观察，**不构成对任何参评算法的指控**——fsrs45 / amas6 没有"作弊"，
> 它们只是语义上恰好被该指标偏好（见 §2.4）。

## 1. 官方 dhp_raw 的构成：实质上的单指标

官方 v0.9 公式（`leaderboard.py` `DHP_W_*` 常量）：

```
dhp_raw = 0.7 × masteredCount + 0.3 × efficiency × 10000
```

两个关键事实：

1. **masteredCount 是调度器自报值**：定义为闭环终态"调度器自报 next_interval ≥ 30 天"的卡片数
   （`simulate.py` 排行榜口径，与预注册闸门文档
   [00-pre-registered-gates.md](../../amas-tuning-2026-06-12/00-pre-registered-gates.md) §2 的
   masteredProxy 镜像定义一致）。该文档披露清单第 2 条已明文承认："mastered = 调度器自报
   next_interval ≥ 30d，**存在自利偏差**"。它不经过 GRU oracle 核验——调度器把自己的 S（稳定性）
   估得越高，自报间隔越长，mastered 越多。
2. **mastered 项占 dhp_raw 的绝对主导**：对 `benchmarks/results/2026-06-12-d5-ship/` 全部 30 个
   结果 JSON 实测，`0.7·mastered` 项占 dhp_raw 总质量的 **97.1%**（质量加权；逐行中位数 96.0%，
   头部条目如 fsrs45×synthetic 达 99.7%）。efficiency 项（唯一经 oracle 的分量）只是零头。

dhp_score 在 final_score 中权重 0.35，仅次于 prediction 0.45——即综合榜约三分之一的话语权
落在一个未经真值核验的自报标量上。

## 2. 本轮战役的实测证据：该指标结构性奖励"校准受损的稳定性通胀"

以下数字全部来自本轮（2026-06-12 D4/D5 调参战役）实测，预注册闸门与基线锚点见
[00-pre-registered-gates.md](../../amas-tuning-2026-06-12/00-pre-registered-gates.md)；
基线 TEST 数字（synthetic mastered 5102 等）见提交 `feb1f74` 终评记录。

### 2.1 224-trial 约束搜索的 near-miss（tau=3.365）

15 维约束搜索（224 trial）产出的 near-miss 候选（alphaRampTau=3.365 + 搜索后 w）实测：

- synthetic 自报 mastered **5102 → 29829（×5.8）**——按官方 dhp_raw 这是碾压级提升；
- 同一配置下 oracle 真值 expectedMemoryFinal **34821.8 → 18651.5（−46%）**，
  跌穿预注册 0.65× 下限（22634.3），被胜者级二元闸门 G3c 拒绝。

即：**指标本体（自报 mastered）最大化的方向，恰好是 oracle 真值塌方的方向**。若没有
预注册的 oracle 下限闸门，这个候选会以"mastered ×5.8"的成绩直接写回生产。

### 2.2 通胀的组分：抬 S，不是改善记忆

对 near-miss 方向做全前缀分桶诊断（调参工作流 d5_fullprefix 族，val 全前缀重放）：

- **6-10 复习历史桶的 meanS 达基线 3.6×**——稳定性通胀集中在中长历史卡片，正是
  "自报间隔 ≥ 30d"判定的敏感区；
- 同时**全前缀 ICI 恶化 +42~75%**——S 被抬高后预测校准系统性变差，模型"自信"超过真实记忆。

### 2.3 反方向同样失败：梯度单调对立

尝试吸收方向（w16/w10 调低，压制 S 增长换校准回稳）实测：**duolingo 自报 mastered −65%**，
DHP 维度直接塌方。在本轮所有已探索的轴上，梯度单调对立——
要么"自报 mastered 升 + 校准恶化"，要么"校准回稳 + mastered 塌"，不存在两全的内点。
这说明问题不在某个参数取值，而在指标本身：**它把"把 S 估计抬高"与"把记忆调度做好"
赋予了同一个奖励信号**。

### 2.4 精确定性：指标设计观察，不是对算法的指控

fsrs45 / amas6 在官方榜的 DHP 优势来源于其曲线语义（更激进的 S 增长、更长的自报间隔）
**天然落在该指标的偏好区**。它们没有针对指标做任何事；是指标选择了它们的语义。
同理，AMAS 本轮 mastered 三连升（feb1f74）同样受益于同一指标——本附录的批评对己对人一致。
本轮的防套利完全依赖预注册闸门（oracle expMemFinal / efficiency / retentionStability 下限），
而非指标自身的健壮性。

## 3. 替代口径重算〔非官方〕

### 3.1 方法：除 dhp_raw 一列外零偏离

重算脚本 [appendix/alt_board.py](./alt_board.py)：`_load_results` /
`_compute_raw_scores`（prediction_raw、policy_raw）/ `_normalize_per_dataset` /
`_compute_final_score` / `_compute_dataset_rank_and_borda` / `aggregate_borda`
全部逐字 import 官方 `benchmarks/maimemo/leaderboard.py`，唯一替换点是覆写 `dhp_raw` 一列：

```
official : dhp_raw = 0.7·masteredCount + 0.3·efficiency·10000          (v0.9 现行)
alt-1    : dhp_raw = 0.5·expectedMemoryFinal + 0.3·efficiency·10000 + 0.2·masteredCount
alt-2    : dhp_raw = expectedMemoryFinal
```

- alt-1（oracle 加权）：主权重交给 GRU oracle 真值，自报 mastered 降为 0.2 次要项；
- alt-2（纯 oracle）：即 pre-v0.9 公式先例的主指标——`leaderboard.py`
  `_compute_raw_scores` docstring 至今保留 `dhp_raw = 0.5*expectedMemoryFinal +
  0.5*efficiency*1000` 的 pre-v0.9 形态（v0.9 切换理由见同文件注释）。
- official 变体作为零偏离自检锚点：其输出与 01-leaderboard.md 综合排名表**逐字一致**（已验证）。

数据：`benchmarks/results/2026-06-12-d5-ship/*.json`（30 文件，10 算法 × 3 数据集），
与官方榜同一份原始产物，未做任何修改。

### 3.2 官方榜（复算锚点，与 01-leaderboard.md 一致）

| 排名 | 算法 | Borda 总分 | final_score 均值 | duolingo_hlr 排名 | maimemo 排名 | synthetic 排名 |
|---|---|---|---|---|---|---|
| 1 | `fsrs45` | 28 | 0.943 | 2 | 2 | 1 |
| 2 | `amas6` | 25 | 0.922 | 1 | 5 | 2 |
| 3 | **`amas`** | 24 | 0.747 | 4 | 1 | 4 |
| 4 | `fsrs6` | 21 | 0.866 | 3 | 6 | 3 |
| 5 | `fsrs` | 20 | 0.733 | 5 | 3 | 5 |
| 6 | `dhp` | 13 | 0.649 | 8 | 4 | 8 |
| 7 | `sm2` | 13 | 0.622 | 7 | 7 | 6 |
| 8 | `random` | 11 | 0.474 | 6 | 9 | 7 |
| 9 | `leitner` | 7 | 0.421 | 9 | 8 | 9 |
| 10 | `hlr` | 3 | 0.001 | 10 | 10 | 10 |

### 3.3 alt-1：oracle 加权〔非官方〕

| 排名 | 算法 | Borda 总分 | final_score 均值 | duolingo_hlr 排名 | maimemo 排名 | synthetic 排名 |
|---|---|---|---|---|---|---|
| 1 | **`amas`** | 25 | 0.835 | 4 | 1 | 3 |
| 2 | `fsrs` | 25 | 0.826 | 2 | 2 | 4 |
| 3 | `fsrs6` | 25 | 0.800 | 1 | 6 | 1 |
| 4 | `amas6` | 20 | 0.787 | 6 | 5 | 2 |
| 5 | `sm2` | 20 | 0.753 | 3 | 4 | 6 |
| 6 | `dhp` | 15 | 0.740 | 7 | 3 | 8 |
| 7 | `leitner` | 14 | 0.690 | 5 | 7 | 7 |
| 8 | `fsrs45` | 12 | 0.697 | 8 | 8 | 5 |
| 9 | `random` | 5 | 0.472 | 9 | 10 | 9 |
| 10 | `hlr` | 4 | 0.350 | 10 | 9 | 10 |

**并列披露**：amas / fsrs / fsrs6 Borda 同为 25，构成三方并列第 1 区间；表内次序为官方
`aggregate_borda` 的稳定排序结果（恰与 final_score 均值降序一致），不宜解读为严格胜出。

### 3.4 alt-2：纯 expectedMemoryFinal〔非官方〕

| 排名 | 算法 | Borda 总分 | final_score 均值 | duolingo_hlr 排名 | maimemo 排名 | synthetic 排名 |
|---|---|---|---|---|---|---|
| 1 | **`amas`** | 23 | 0.761 | 5 | 2 | 3 |
| 2 | `fsrs` | 23 | 0.758 | 3 | 3 | 4 |
| 3 | `sm2` | 22 | 0.713 | 1 | 4 | 6 |
| 4 | `fsrs6` | 21 | 0.759 | 6 | 5 | 1 |
| 5 | `dhp` | 19 | 0.675 | 4 | 1 | 9 |
| 6 | `amas6` | 18 | 0.730 | 7 | 6 | 2 |
| 7 | `leitner` | 17 | 0.703 | 2 | 7 | 7 |
| 8 | `fsrs45` | 12 | 0.639 | 8 | 8 | 5 |
| 9 | `random` | 7 | 0.532 | 9 | 9 | 8 |
| 10 | `hlr` | 3 | 0.350 | 10 | 10 | 10 |

**并列披露**：amas / fsrs Borda 同为 23，双方并列第 1 区间（同上排序说明）。

### 3.5 跨口径观察

- **fsrs45：第 1 → 第 8 / 第 8**。官方榜冠军的优势几乎完全由自报 mastered 承载；按 oracle 真值
  其 expectedMemoryFinal 在三数据集均靠后（如 synthetic 9662.8，仅为 amas6 的 41%）。
- **amas6：第 2 → 第 4 / 第 6**。同向但程度较轻——其 oracle expMemFinal 本身不弱（synthetic 头名），
  跌幅主要来自 duolingo。
- **amas：第 3 → 并列第 1 / 并列第 1**。本附录作者即 AMAS 维护方，此结果**利益相关**，
  这正是全文标注「非官方」并保留官方榜原位的原因。
- mastered 弱势条目（sm2 / dhp / leitner）普遍上浮；prediction / policy 两维分数三套口径完全不变，
  位次变动全部由 DHP 单维替换驱动。

## 4. 诚实的局限性（替代口径同样不中立）

1. **oracle 自身有偏**：expectedMemoryFinal 由 maimemo 训练的 GRU-HLR oracle 计算，
   duolingo_hlr / synthetic 上是跨数据集复用（官方 04-methodology §5.3 同一 caveat）。
   用它当"真值"只是把信任从调度器自报转移到一个有分布偏移的代理模型。
2. **偏置换向而非消除**：expectedMemoryFinal 机械偏好密集复习——复习越频繁、期末期望记忆量
   越容易堆高，HLR / Leitner 型高频调度器在该指标下结构性获利。v0.9 切换到 mastered 的原始动机
   正是这一点（`leaderboard.py` 注释："expectedMemoryFinal 被 Leitner/HLR 等高频复习算法刷虚高"）。
   alt 口径只是把"奖励自报稳定性"换成"奖励复习密度"，不存在更中立的单标量。
3. **alt-1 权重是作者选择**：0.5/0.3/0.2 没有预注册，也未做敏感性扫描；换一组权重可得到不同榜首。
   它的论证价值在于"位次对 DHP 公式高度敏感"这一事实本身，而非任何一张替代榜的具体名次。
4. **Borda 并列**：两套替代榜的榜首均为并列区间（§3.3 / §3.4），任何"AMAS 第 1"的表述
   在替代口径下都必须带并列限定。

**结论**：没有单一标量能同时免疫"自报通胀"与"密度刷分"。本附录主张的是**多指标阅读**
——官方榜 + oracle 视角 + prediction/policy 分维表（01-leaderboard.md 已提供）合并判断，
而不是用任何一套替代公式替换官方分数。本轮战役的实践答案也不是改指标，
而是预注册闸门（oracle 下限 + 校准下限 + 留存稳定性下限）兜底。

## 5. 复现

```bash
source .bench-venv/bin/activate
python docs/algo-bench-2026-06-12-d5-ship/appendix/alt_board.py \
    --results benchmarks/results/2026-06-12-d5-ship
```

官方产物（`leaderboard.py`、01/03/04 报告、结果 JSON）未被本附录修改。
