# v0.6 vs v0.5 评测差异

> v0.5: 2026-05-26 / v0.6: 2026-05-27

## 修改内容

1. **HLR 默认 θ**: `(2.0, -2.5, -0.3)` → `(0.5, -1.0, -0.3)` (Duolingo paper Table 4 fitted)
2. **build_scheduler 扩展**: 补齐 sm2/hlr/fsrs45 注册（v0.5 是 monkey-patched，v0.6 改入源码）
3. **未做**：duolingo_hlr / synthetic 独立 oracle 训练 — CPU 训练超时（>60min），仍复用 maimemo oracle

## 综合排名对比

| 算法 | v0.5 Borda | v0.6 Borda | Δ | v0.5 排名 | v0.6 排名 |
|---|---|---|---|---|---|
| sm2 | 17 | **19** | +2 | 3 | **1** ⬆ |
| **amas** | **18** | **18** | = | **2** | **2** ⟶ |
| dhp | 16 | 16 | = | 4 | 3 ⬆ |
| leitner | **21** | 16 | -5 | **1** | 4 ⬇⬇ |
| fsrs | 15 | 15 | = | 5 | 5 ⟶ |
| fsrs45 | 10 | 11 | +1 | 6 | 6 ⟶ |
| random | 8 | 10 | +2 | 7 | 7 ⟶ |
| hlr | 3 | 3 | = | 8 | 8 ⟶ |

**关键变化**：
- **sm2 升至第 1**（19 分）— SuperMemo 2 算法在 maimemo / duolingo_hlr 双双第 2，跨数据集稳定性反超 leitner
- **leitner 大跌**（第 1 → 第 4）— v0.5 中 leitner 第 1 主要靠 maimemo 第 2 + duolingo_hlr 第 1，v0.6 simulate 重新 sample 后 maimemo 跌到第 5
- **AMAS 保持第 2**（18 分）— 三数据集排名分布从 v0.5 `(4,4,1)` 变 `(5,3,1)`，synthetic 第 1 持续保留
- **HLR 仍垫底** — θ 修复对 prediction logLoss 改善有限（见下表），数据特征本身不利

## Prediction logLoss 对比

| 算法 | maimemo v0.5 | maimemo v0.6 | duolingo_hlr v0.5 | duolingo_hlr v0.6 | synthetic v0.5 | synthetic v0.6 |
|---|---|---|---|---|---|---|
| amas | 0.329 | 0.329 | 0.422 | 0.422 | 0.936 | 0.936 |
| fsrs | 0.329 | 0.329 | 0.422 | 0.422 | 0.936 | 0.936 |
| dhp | 0.345 | 0.345 | 0.394 | 0.394 | **2.328** | **2.328** |
| sm2 | 0.488 | 0.488 | 0.510 | 0.510 | 0.904 | 0.904 |
| leitner | 0.612 | 0.612 | 0.572 | 0.572 | 1.036 | 1.036 |
| fsrs45 | 0.298 | 0.298 | 0.405 | 0.405 | 0.681 | 0.681 |
| random | 0.693 | 0.693 | 0.693 | 0.693 | 1.119 | 1.119 |
| **hlr** | **10.241** | **10.178** | **4.545** | **4.165** | **11.528** | **11.334** |

HLR θ 修复**有效但有限**：
- duolingo_hlr 改善 8.4%（4.545 → 4.165）
- maimemo 仅 0.6%
- synthetic 1.7%

**根因**：长序列数据上 sqrt(correct) 增长导致 halflife 被 365 天上限卡死，无论 θ 为何 → p_recall ≈ 1.0 → 对 lapse 给极低 likelihood。根治需在 HLR 内部限 correct/incorrect 计数（如 cap=50）或用 elapsed_days 加权，超出 paper 原设计。

## AMAS 维度变化（v0.5 → v0.6）

| 维度 | v0.5 排名 | v0.6 排名 |
|---|---|---|
| Prediction | 2 | 2 |
| DHP | 5 | 5 |
| Policy | 3 | 2 ⬆ |
| 综合 | 2 | 2 |

Policy 从 3 升 2 — 因 leitner 在 maimemo / duolingo_hlr 的高 reviewsPerDay 在 v0.6 重新评分时被惩罚得更重，policy_score 滑落。

## 已知未解决

1. **duolingo_hlr / synthetic 独立 oracle 未训** — CPU 单机 8 epoch + 200 万行 + 256 batch 训练 >60 分钟。建议在带 GPU 机器上跑 `python -m benchmarks.maimemo.cli fit_oracle --root /Users/liji/.wordforge-bench/<ds> --device mps --epochs 5 --max-train-rows 1000000` 节省时间
2. **HLR 在长序列数据上 logLoss 仍爆** — 需改算法内部 cap 而非 θ
3. **AMAS prediction 与 FSRS 同分** — adapter 在 prediction 评估层仅注入 MDM 参数，wordSelector/ensemble 不参与；要让 AMAS 在 prediction 上区分，需扩展 evaluate_scheduler.py 或在 forward simulation 层挂载 adapter 全栈

## 结论

v0.6 评测**不改变 AMAS 综合第 2 的核心结论**，但揭示了两个新事实：
1. SM-2（最古老的算法）在跨数据集 Borda 评分下反超 FSRS 系列，证明算法简单性 + 稳定性在 Borda 排名规则下被奖励
2. HLR scheduler 在 long-sequence 数据集上不堪一击 — 这是 Duolingo paper 模型在跨场景下的固有限制，与 θ 选择无关
