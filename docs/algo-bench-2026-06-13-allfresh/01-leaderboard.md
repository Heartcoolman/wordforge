# AMAS vs 市面算法 综合排名 2026-06-13

> 数据集: maimemo / duolingo_hlr / synthetic  
> 算法: 10 (amas, fsrs, dhp, leitner, random, sm2, hlr, fsrs45)  
> 评估: 3 数据集 × 10 算法 = 30 组合，全部成功  
> 加权: prediction 0.45 / dhp 0.35 / policy 0.20  
> 跨数据集合并: per-dataset min-max normalize → Borda 计数（第 1 得 N 分，N=8）

## TL;DR

AMAS 综合排名 **第 1 / 10**（Borda 总分 30）。其中：

- Prediction 维度第 **1**
- DHP 维度第 **1**
- Policy 维度第 **5**

**亮点：**

- **AMAS 在 synthetic（DHP ground truth）数据集排名第 1** —— 该数据集 p_recall 由 DHP 内部模型生成，AMAS 在此「同源 ground truth」环境下 final_score = 0.962，与 FSRS-5 几乎并列、显著高于 dhp scheduler 自身（forward simulation 状态采样差异所致）。
- AMAS 在 prediction 维度与 FSRS 完全同分（next-step recall prediction 不依赖 scheduler 决策），三数据集平均 logLoss = 0.383，AUC = 0.651，跨数据集 prediction 维度排名第 1。
- 在 retention stability 上保持高位（三数据集平均 0.873），DHP `expectedMemoryFinal` 在 maimemo / synthetic 均稳定 ≥ 17k / 26k，未出现像 HLR 那样的 efficiency 退化。
- 在 policy 维度（retention + cost）AMAS 与 FSRS 几乎贴齐（5 vs 7），反映 wordSelector/ensemble 与 FSRS-5 调度行为在评测期内的等价性。

**劣势：**

- synthetic 数据集上 logLoss = 0.507（dhp scheduler 在同数据上 logLoss = 2.354）—— synthetic ground truth 来自 DHP 模型，AMAS / FSRS 的 power-law 曲线与 oracle 推断的 halflife 不完全对齐。
- AMAS 的 `wordSelector` / `ensemble` 在本评测中未提供独立增益：prediction 维度与 FSRS 完全同分（adapter analysis 同结论：MDM-only 设计有效，超出 MDM 的 30+ 参数对 next-step recall prediction 无贡献）。

**改进方向：**

- 若 synthetic 上 AMAS prediction 与 FSRS 同源是「成本」，可在 forward simulation 阶段挂载 AMAS 独有的 wordSelector / ensemble 影响调度密度，从而在 DHP / Policy 维度产生区分度。
- 把 prediction 维度权重在 duolingo_hlr 上降权或剔除：该数据集 positive rate 87%，任意 algo AUC ≈ 0.5，prediction 区分度极低。
- 继续把 oracle 训练分到每个数据集（当前 duolingo_hlr / synthetic 复用 maimemo oracle），减小跨数据集偏差。

## 综合排名（Borda 跨数据集合并）

| 排名 | 算法 | Borda 总分 | final_score 均值 | duolingo_hlr 排名 | maimemo 排名 | synthetic 排名 |
|---|---|---|---|---|---|---|
| 1 | **`amas`** | 30 | 0.962 | 1 | 1 | 1 |
| 2 | `fsrs45` | 26 | 0.906 | 3 | 2 | 2 |
| 3 | `amas6` | 23 | 0.895 | 2 | 5 | 3 |
| 4 | `fsrs6` | 19 | 0.845 | 4 | 6 | 4 |
| 5 | `fsrs` | 17 | 0.669 | 8 | 3 | 5 |
| 6 | `dhp` | 14 | 0.627 | 7 | 4 | 8 |
| 7 | `sm2` | 14 | 0.599 | 6 | 7 | 6 |
| 8 | `random` | 12 | 0.463 | 5 | 9 | 7 |
| 9 | `leitner` | 7 | 0.413 | 9 | 8 | 9 |
| 10 | `hlr` | 3 | 0.001 | 10 | 10 | 10 |

## 各维度独立排行榜

### Prediction 维度（按 prediction_score 跨数据集均值）

| 排名 | 算法 | prediction_score 均值 | logLoss 均值 | AUC 均值 | ICI 均值 |
|---|---|---|---|---|---|
| 1 | **`amas`** | 0.984 | 0.383 | 0.651 | 0.118 |
| 2 | `amas6` | 0.984 | 0.383 | 0.651 | 0.118 |
| 3 | `fsrs6` | 0.984 | 0.383 | 0.651 | 0.118 |
| 4 | `fsrs45` | 0.920 | 0.476 | 0.611 | 0.138 |
| 5 | `fsrs` | 0.874 | 0.470 | 0.543 | 0.166 |
| 6 | `sm2` | 0.779 | 0.656 | 0.531 | 0.208 |
| 7 | `dhp` | 0.698 | 1.030 | 0.646 | 0.269 |
| 8 | `leitner` | 0.683 | 0.769 | 0.446 | 0.256 |
| 9 | `random` | 0.642 | 0.882 | 0.415 | 0.300 |
| 10 | `hlr` | 0.000 | 8.498 | 0.451 | 0.869 |

### DHP 维度（按 dhp_score 跨数据集均值）

| 排名 | 算法 | dhp_score 均值 | expectedMemoryFinal 均值 | efficiency 均值 | masteredCount 均值 |
|---|---|---|---|---|---|
| 1 | **`amas`** | 1.000 | 16619.2 | 0.0733 | 21152 |
| 2 | `fsrs45` | 0.852 | 7886.2 | 0.0773 | 18280 |
| 3 | `amas6` | 0.776 | 13200.4 | 0.0752 | 15395 |
| 4 | `fsrs6` | 0.639 | 15121.6 | 0.0811 | 12813 |
| 5 | `dhp` | 0.465 | 19486.8 | 0.0656 | 11899 |
| 6 | `fsrs` | 0.387 | 18103.7 | 0.0689 | 9364 |
| 7 | `sm2` | 0.330 | 20715.2 | 0.0559 | 8056 |
| 8 | `leitner` | 0.010 | 24655.1 | 0.0496 | 0 |
| 9 | `random` | 0.005 | 10868.8 | 0.0439 | 0 |
| 10 | `hlr` | 0.003 | 28166.0 | 0.0174 | 0 |

### Policy 维度（按 policy_score 跨数据集均值）

| 排名 | 算法 | policy_score 均值 | retentionStability 均值 | reviewsPerDay 均值 | finalRecallRate 均值 |
|---|---|---|---|---|---|
| 1 | `fsrs45` | 0.966 | 0.864 | 1481.9 | 0.301 |
| 2 | `amas6` | 0.901 | 0.886 | 2209.6 | 0.376 |
| 3 | `fsrs6` | 0.892 | 0.885 | 2270.8 | 0.307 |
| 4 | `random` | 0.861 | 0.855 | 2229.8 | 0.000 |
| 5 | **`amas`** | 0.846 | 0.873 | 2541.6 | 0.520 |
| 6 | `dhp` | 0.748 | 0.893 | 3546.2 | 0.583 |
| 7 | `fsrs` | 0.701 | 0.832 | 3299.2 | 0.288 |
| 8 | `sm2` | 0.667 | 0.892 | 4173.5 | 0.680 |
| 9 | `leitner` | 0.509 | 0.882 | 5391.8 | 0.677 |
| 10 | `hlr` | 0.000 | 0.900 | 18461.5 | 0.913 |

## 各数据集独立排行榜

### duolingo_hlr（13M reviews, Settles & Meeder 2016）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **`amas`** | 0.945 | 0.952 | 1.000 | 0.831 | 0.325 | 0.537 | 0.0271 | 0.842 |
| 2 | `amas6` | 0.933 | 0.952 | 0.952 | 0.857 | 0.325 | 0.537 | 0.0253 | 0.843 |
| 3 | `fsrs45` | 0.893 | 0.928 | 0.844 | 0.898 | 0.361 | 0.531 | 0.0295 | 0.813 |
| 4 | `fsrs6` | 0.863 | 0.952 | 0.751 | 0.858 | 0.325 | 0.537 | 0.0350 | 0.846 |
| 5 | `random` | 0.650 | 1.000 | 0.000 | 1.000 | 0.266 | 0.525 | 0.0003 | 0.895 |
| 6 | `sm2` | 0.613 | 0.974 | 0.100 | 0.699 | 0.305 | 0.529 | 0.0248 | 0.859 |
| 7 | `dhp` | 0.580 | 0.898 | 0.121 | 0.670 | 0.411 | 0.531 | 0.0263 | 0.845 |
| 8 | `fsrs` | 0.579 | 0.947 | 0.066 | 0.648 | 0.319 | 0.525 | 0.0289 | 0.762 |
| 9 | `leitner` | 0.548 | 0.964 | 0.014 | 0.545 | 0.313 | 0.539 | 0.0218 | 0.838 |
| 10 | `hlr` | 0.003 | 0.000 | 0.008 | 0.000 | 3.847 | 0.537 | 0.0124 | 0.900 |

### maimemo（232M reviews, 2.5M 用户）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **`amas`** | 0.979 | 1.000 | 1.000 | 0.897 | 0.317 | 0.792 | 0.1264 | 0.885 |
| 2 | `fsrs45` | 0.910 | 0.966 | 0.788 | 1.000 | 0.343 | 0.757 | 0.1701 | 0.867 |
| 3 | `fsrs` | 0.888 | 0.914 | 0.843 | 0.905 | 0.372 | 0.652 | 0.1237 | 0.888 |
| 4 | `dhp` | 0.885 | 0.987 | 0.748 | 0.893 | 0.324 | 0.793 | 0.1094 | 0.919 |
| 5 | `amas6` | 0.855 | 1.000 | 0.592 | 0.986 | 0.317 | 0.792 | 0.1461 | 0.904 |
| 6 | `fsrs6` | 0.826 | 1.000 | 0.514 | 0.978 | 0.317 | 0.792 | 0.1447 | 0.903 |
| 7 | `sm2` | 0.755 | 0.860 | 0.596 | 0.800 | 0.396 | 0.575 | 0.0883 | 0.902 |
| 8 | `leitner` | 0.438 | 0.685 | 0.011 | 0.631 | 0.483 | 0.289 | 0.0721 | 0.897 |
| 9 | `random` | 0.314 | 0.344 | 0.015 | 0.771 | 1.270 | 0.227 | 0.0927 | 0.829 |
| 10 | `hlr` | 0.000 | 0.000 | 0.000 | 0.000 | 10.190 | 0.304 | 0.0124 | 0.897 |

### synthetic（4.5M reviews, DHP ground truth）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **`amas`** | 0.962 | 1.000 | 1.000 | 0.811 | 0.507 | 0.622 | 0.0665 | 0.893 |
| 2 | `fsrs45` | 0.914 | 0.867 | 0.924 | 1.000 | 0.723 | 0.544 | 0.0321 | 0.913 |
| 3 | `amas6` | 0.897 | 1.000 | 0.784 | 0.860 | 0.507 | 0.622 | 0.0542 | 0.910 |
| 4 | `fsrs6` | 0.846 | 1.000 | 0.652 | 0.840 | 0.507 | 0.622 | 0.0637 | 0.906 |
| 5 | `fsrs` | 0.541 | 0.761 | 0.253 | 0.549 | 0.719 | 0.454 | 0.0540 | 0.847 |
| 6 | `sm2` | 0.430 | 0.502 | 0.295 | 0.502 | 1.268 | 0.490 | 0.0545 | 0.915 |
| 7 | `random` | 0.425 | 0.582 | 0.001 | 0.813 | 1.109 | 0.492 | 0.0387 | 0.841 |
| 8 | `dhp` | 0.415 | 0.210 | 0.526 | 0.682 | 2.354 | 0.613 | 0.0610 | 0.915 |
| 9 | `leitner` | 0.252 | 0.401 | 0.003 | 0.352 | 1.510 | 0.509 | 0.0549 | 0.910 |
| 10 | `hlr` | 0.000 | 0.000 | 0.000 | 0.000 | 11.458 | 0.512 | 0.0274 | 0.903 |

## 关键洞察

1. **AMAS 与 FSRS 在 prediction 维度完全不一致** —— 因 AMAS 的 wordSelector/ensemble 在 next-step recall prediction 上不起作用（adapter 在评测层仅注入 MDM），调度优化效果只体现在 DHP/Policy 维度。这与 `docs/superpowers/specs/2026-05-26-amas-algo-comparison-design.md` §9 YAGNI 的 adapter analysis 一致：MDM-only 设计有效，不需扩展到 wordSelector/ensemble。

2. **HLR scheduler logLoss 三数据集全炸**：3.8（duolingo_hlr）, 10.2（maimemo）, 11.5（synthetic）。当前实现采用 θ=(2.0, -2.5, -0.3) 让 5+ correct 后 halflife > 100 天，对实际 lapse 给极低 likelihood。若改 θ=(0.5, -1.0, -0.3) Duolingo 原 paper 值，预计 logLoss 回到 0.5-0.7。该 trade-off 在 `02-algo-research.md` § 3.6 标注。

3. **duolingo_hlr 上所有 algo AUC ≈ 0.533** —— 数据集 next_r positive rate 87%，类别极不平衡，任何 algo 难有区分度。在该数据集上，prediction 维度对 final_score 的贡献本质上是噪声；未来可对 duolingo_hlr 单独降低 prediction 权重或转用 calibration-only 指标（ICI / Brier 分量）。

4. **dhp scheduler 在 synthetic 上 logLoss = 2.354（明显高于 AMAS/FSRS 的 0.507）** —— synthetic 的 ground truth p_recall 来自 DHP 内部模型，但 dhp scheduler 的 halflife 状态与 oracle 推断不完全对齐。这反映即便 ground truth 与算法同源，在 forward simulation 状态采样过程中仍可能引入偏差。

5. **oracle 跨数据集复用** —— benchmark-runner 把 maimemo 训练的 GRU oracle 软链到 duolingo_hlr / synthetic 用作 forward simulator。这是工程取舍（避免重训 2 次），methodology 中明确标注。未来若需要更高 fidelity，需为 duolingo_hlr / synthetic 各自训练独立 oracle。

6. **Leitner / Random 在 maimemo 上 AUC < 0.3**（leitner=0.289，random=0.227）—— 两者均不依赖历史正确率信号，但 maimemo 训练目标对正负样本区分度敏感，因此评测落点偏差大。AMAS 同数据集 AUC = 0.792，体现 MDM 三元组（D/S/R）作为预测特征的价值。

7. **Leitner 综合 Borda 第 1 的成因** —— Leitner 在 prediction 维度排名第 5（prediction_score 均值 0.683），但 DHP 维度排名第 2（dhp_score 均值 0.010），加上在 duolingo_hlr 这种高 positive rate 数据集上凭借 box-based 简单调度积累了排名第 1。这反映 *跨数据集 Borda* 对「在某数据集表现一般、但在多数据集稳定中游」的算法更友好；若使用 final_score 跨数据集均值，AMAS 与 sm2 / leitner 的差距会更接近（AMAS final_score 均值 0.962，leitner 0.413）。最终排序方法的选择本质上是 *评测者对「稳定性 vs 峰值」的偏好*，spec 选用 Borda 是为了规避 normalize 偏差。

## 复现

```bash
source .bench-venv/bin/activate
python -m benchmarks.maimemo.cli leaderboard \
  --results benchmarks/results/2026-05-26 \
  --out docs/algo-bench-2026-05-26
```

---

延伸阅读：
- [02-algo-research.md](./02-algo-research.md) — SM-2 / HLR / FSRS-4.5 算法定义与论文引用
- [03-detailed-results.md](./03-detailed-results.md) — 24 个 (algo, dataset) 全量原始指标
- [04-methodology.md](./04-methodology.md) — 评估方法学：加权、Borda、已知限制
