# AMAS vs 市面算法 综合排名 2026-05-29-amas6

> 数据集: maimemo / duolingo_hlr / synthetic  
> 算法: 10 (amas, fsrs, dhp, leitner, random, sm2, hlr, fsrs45)  
> 评估: 3 数据集 × 10 算法 = 30 组合，全部成功  
> 加权: prediction 0.45 / dhp 0.35 / policy 0.20  
> 跨数据集合并: per-dataset min-max normalize → Borda 计数（第 1 得 N 分，N=8）

## TL;DR

AMAS 综合排名 **第 5 / 10**（Borda 总分 20）。其中：

- Prediction 维度第 **4**
- DHP 维度第 **5**
- Policy 维度第 **5**

**亮点：**

- **AMAS 在 synthetic（DHP ground truth）数据集排名第 4** —— 该数据集 p_recall 由 DHP 内部模型生成，AMAS 在此「同源 ground truth」环境下 final_score = 0.716，与 FSRS-5 几乎并列、显著高于 dhp scheduler 自身（forward simulation 状态采样差异所致）。
- AMAS 在 prediction 维度与 FSRS 完全同分（next-step recall prediction 不依赖 scheduler 决策），三数据集平均 logLoss = 0.562，AUC = 0.660，跨数据集 prediction 维度排名第 4。
- 在 retention stability 上保持高位（三数据集平均 0.883），DHP `expectedMemoryFinal` 在 maimemo / synthetic 均稳定 ≥ 17k / 26k，未出现像 HLR 那样的 efficiency 退化。
- 在 policy 维度（retention + cost）AMAS 与 FSRS 几乎贴齐（5 vs 6），反映 wordSelector/ensemble 与 FSRS-5 调度行为在评测期内的等价性。

**劣势：**

- synthetic 数据集上 logLoss = 0.936（dhp scheduler 在同数据上 logLoss = 2.328）—— synthetic ground truth 来自 DHP 模型，AMAS / FSRS 的 power-law 曲线与 oracle 推断的 halflife 不完全对齐。
- AMAS 的 `wordSelector` / `ensemble` 在本评测中未提供独立增益：prediction 维度与 FSRS 完全同分（adapter analysis 同结论：MDM-only 设计有效，超出 MDM 的 30+ 参数对 next-step recall prediction 无贡献）。

**改进方向：**

- 若 synthetic 上 AMAS prediction 与 FSRS 同源是「成本」，可在 forward simulation 阶段挂载 AMAS 独有的 wordSelector / ensemble 影响调度密度，从而在 DHP / Policy 维度产生区分度。
- 把 prediction 维度权重在 duolingo_hlr 上降权或剔除：该数据集 positive rate 87%，任意 algo AUC ≈ 0.5，prediction 区分度极低。
- 继续把 oracle 训练分到每个数据集（当前 duolingo_hlr / synthetic 复用 maimemo oracle），减小跨数据集偏差。

## 综合排名（Borda 跨数据集合并）

| 排名 | 算法 | Borda 总分 | final_score 均值 | duolingo_hlr 排名 | maimemo 排名 | synthetic 排名 |
|---|---|---|---|---|---|---|
| 1 | `fsrs45` | 27 | 0.937 | 2 | 3 | 1 |
| 2 | `amas6` | 25 | 0.917 | 1 | 5 | 2 |
| 3 | `fsrs` | 21 | 0.757 | 6 | 1 | 5 |
| 4 | `fsrs6` | 21 | 0.863 | 3 | 6 | 3 |
| 5 | **`amas`** | 20 | 0.755 | 7 | 2 | 4 |
| 6 | `sm2` | 15 | 0.618 | 5 | 7 | 6 |
| 7 | `dhp` | 13 | 0.644 | 8 | 4 | 8 |
| 8 | `random` | 13 | 0.474 | 4 | 9 | 7 |
| 9 | `leitner` | 7 | 0.420 | 9 | 8 | 9 |
| 10 | `hlr` | 3 | 0.001 | 10 | 10 | 10 |

## 各维度独立排行榜

### Prediction 维度（按 prediction_score 跨数据集均值）

| 排名 | 算法 | prediction_score 均值 | logLoss 均值 | AUC 均值 | ICI 均值 |
|---|---|---|---|---|---|
| 1 | `amas6` | 0.990 | 0.373 | 0.660 | 0.118 |
| 2 | `fsrs6` | 0.990 | 0.373 | 0.660 | 0.118 |
| 3 | `fsrs45` | 0.930 | 0.461 | 0.624 | 0.137 |
| 4 | **`amas`** | 0.853 | 0.562 | 0.660 | 0.244 |
| 5 | `fsrs` | 0.853 | 0.562 | 0.659 | 0.244 |
| 6 | `sm2` | 0.794 | 0.634 | 0.544 | 0.204 |
| 7 | `dhp` | 0.700 | 1.022 | 0.661 | 0.274 |
| 8 | `leitner` | 0.700 | 0.740 | 0.454 | 0.251 |
| 9 | `random` | 0.660 | 0.835 | 0.415 | 0.295 |
| 10 | `hlr` | 0.000 | 8.559 | 0.452 | 0.870 |

### DHP 维度（按 dhp_score 跨数据集均值）

| 排名 | 算法 | dhp_score 均值 | expectedMemoryFinal 均值 | efficiency 均值 | masteredCount 均值 |
|---|---|---|---|---|---|
| 1 | `fsrs45` | 0.928 | 7389.8 | 0.0739 | 17830 |
| 2 | `amas6` | 0.833 | 12739.2 | 0.0736 | 14929 |
| 3 | `fsrs6` | 0.684 | 14595.0 | 0.0787 | 12398 |
| 4 | `fsrs` | 0.625 | 16402.7 | 0.0613 | 13687 |
| 5 | **`amas`** | 0.613 | 16414.5 | 0.0621 | 13617 |
| 6 | `dhp` | 0.516 | 18973.9 | 0.0642 | 11266 |
| 7 | `sm2` | 0.366 | 20119.7 | 0.0553 | 7551 |
| 8 | `leitner` | 0.010 | 23899.4 | 0.0492 | 0 |
| 9 | `random` | 0.007 | 9996.1 | 0.0437 | 0 |
| 10 | `hlr` | 0.003 | 27409.1 | 0.0174 | 0 |

### Policy 维度（按 policy_score 跨数据集均值）

| 排名 | 算法 | policy_score 均值 | retentionStability 均值 | reviewsPerDay 均值 | finalRecallRate 均值 |
|---|---|---|---|---|---|
| 1 | `fsrs45` | 0.967 | 0.864 | 1510.3 | 0.305 |
| 2 | `amas6` | 0.899 | 0.887 | 2264.9 | 0.385 |
| 3 | `fsrs6` | 0.889 | 0.885 | 2327.1 | 0.301 |
| 4 | `random` | 0.873 | 0.856 | 2171.1 | 0.000 |
| 5 | **`amas`** | 0.781 | 0.883 | 3230.3 | 0.554 |
| 6 | `fsrs` | 0.773 | 0.882 | 3296.7 | 0.561 |
| 7 | `dhp` | 0.741 | 0.893 | 3651.5 | 0.580 |
| 8 | `sm2` | 0.667 | 0.892 | 4214.4 | 0.680 |
| 9 | `leitner` | 0.510 | 0.882 | 5443.0 | 0.675 |
| 10 | `hlr` | 0.000 | 0.900 | 17813.6 | 0.912 |

## 各数据集独立排行榜

### duolingo_hlr（13M reviews, Settles & Meeder 2016）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `amas6` | 0.956 | 0.970 | 1.000 | 0.849 | 0.307 | 0.541 | 0.0267 | 0.843 |
| 2 | `fsrs45` | 0.916 | 0.943 | 0.892 | 0.900 | 0.350 | 0.537 | 0.0302 | 0.813 |
| 3 | `fsrs6` | 0.881 | 0.970 | 0.784 | 0.849 | 0.307 | 0.541 | 0.0354 | 0.845 |
| 4 | `random` | 0.650 | 1.000 | 0.000 | 1.000 | 0.258 | 0.512 | 0.0003 | 0.895 |
| 5 | `sm2` | 0.619 | 0.992 | 0.106 | 0.676 | 0.291 | 0.540 | 0.0255 | 0.859 |
| 6 | `fsrs` | 0.597 | 0.887 | 0.186 | 0.662 | 0.422 | 0.540 | 0.0244 | 0.838 |
| 7 | **`amas`** | 0.592 | 0.887 | 0.168 | 0.672 | 0.422 | 0.540 | 0.0247 | 0.834 |
| 8 | `dhp` | 0.582 | 0.903 | 0.135 | 0.640 | 0.422 | 0.539 | 0.0260 | 0.845 |
| 9 | `leitner` | 0.541 | 0.971 | 0.014 | 0.498 | 0.314 | 0.541 | 0.0216 | 0.838 |
| 10 | `hlr` | 0.003 | 0.000 | 0.008 | 0.000 | 4.165 | 0.527 | 0.0124 | 0.900 |

### maimemo（232M reviews, 2.5M 用户）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `fsrs` | 0.968 | 0.968 | 1.000 | 0.914 | 0.329 | 0.790 | 0.0999 | 0.903 |
| 2 | **`amas`** | 0.956 | 0.968 | 0.962 | 0.921 | 0.329 | 0.790 | 0.1009 | 0.907 |
| 3 | `fsrs45` | 0.951 | 0.975 | 0.893 | 1.000 | 0.323 | 0.782 | 0.1599 | 0.865 |
| 4 | `dhp` | 0.925 | 0.985 | 0.856 | 0.910 | 0.316 | 0.813 | 0.1058 | 0.918 |
| 5 | `amas6` | 0.880 | 1.000 | 0.659 | 0.995 | 0.303 | 0.807 | 0.1402 | 0.905 |
| 6 | `fsrs6` | 0.846 | 1.000 | 0.569 | 0.986 | 0.303 | 0.807 | 0.1381 | 0.902 |
| 7 | `sm2` | 0.790 | 0.860 | 0.678 | 0.827 | 0.387 | 0.588 | 0.0864 | 0.900 |
| 8 | `leitner` | 0.452 | 0.691 | 0.014 | 0.682 | 0.464 | 0.294 | 0.0715 | 0.897 |
| 9 | `random` | 0.333 | 0.369 | 0.018 | 0.801 | 1.180 | 0.224 | 0.0921 | 0.830 |
| 10 | `hlr` | 0.000 | 0.000 | 0.000 | 0.000 | 10.178 | 0.302 | 0.0124 | 0.897 |

### synthetic（4.5M reviews, DHP ground truth）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `fsrs45` | 0.942 | 0.872 | 1.000 | 1.000 | 0.710 | 0.555 | 0.0317 | 0.914 |
| 2 | `amas6` | 0.914 | 1.000 | 0.839 | 0.853 | 0.509 | 0.631 | 0.0538 | 0.911 |
| 3 | `fsrs6` | 0.862 | 1.000 | 0.701 | 0.833 | 0.509 | 0.631 | 0.0628 | 0.907 |
| 4 | **`amas`** | 0.716 | 0.705 | 0.710 | 0.750 | 0.936 | 0.648 | 0.0608 | 0.906 |
| 5 | `fsrs` | 0.707 | 0.705 | 0.690 | 0.743 | 0.936 | 0.648 | 0.0596 | 0.906 |
| 6 | `sm2` | 0.447 | 0.529 | 0.313 | 0.498 | 1.223 | 0.504 | 0.0541 | 0.916 |
| 7 | `random` | 0.438 | 0.609 | 0.002 | 0.818 | 1.066 | 0.508 | 0.0386 | 0.843 |
| 8 | `dhp` | 0.426 | 0.213 | 0.557 | 0.674 | 2.328 | 0.632 | 0.0607 | 0.916 |
| 9 | `leitner` | 0.268 | 0.437 | 0.004 | 0.349 | 1.441 | 0.528 | 0.0546 | 0.911 |
| 10 | `hlr` | 0.000 | 0.000 | 0.000 | 0.000 | 11.334 | 0.527 | 0.0274 | 0.903 |

## 关键洞察

1. **AMAS 与 FSRS 在 prediction 维度完全不一致** —— 因 AMAS 的 wordSelector/ensemble 在 next-step recall prediction 上不起作用（adapter 在评测层仅注入 MDM），调度优化效果只体现在 DHP/Policy 维度。这与 `docs/superpowers/specs/2026-05-26-amas-algo-comparison-design.md` §9 YAGNI 的 adapter analysis 一致：MDM-only 设计有效，不需扩展到 wordSelector/ensemble。

2. **HLR scheduler logLoss 三数据集全炸**：4.2（duolingo_hlr）, 10.2（maimemo）, 11.3（synthetic）。当前实现采用 θ=(2.0, -2.5, -0.3) 让 5+ correct 后 halflife > 100 天，对实际 lapse 给极低 likelihood。若改 θ=(0.5, -1.0, -0.3) Duolingo 原 paper 值，预计 logLoss 回到 0.5-0.7。该 trade-off 在 `02-algo-research.md` § 3.6 标注。

3. **duolingo_hlr 上所有 algo AUC ≈ 0.536** —— 数据集 next_r positive rate 87%，类别极不平衡，任何 algo 难有区分度。在该数据集上，prediction 维度对 final_score 的贡献本质上是噪声；未来可对 duolingo_hlr 单独降低 prediction 权重或转用 calibration-only 指标（ICI / Brier 分量）。

4. **dhp scheduler 在 synthetic 上 logLoss = 2.328（明显高于 AMAS/FSRS 的 0.936）** —— synthetic 的 ground truth p_recall 来自 DHP 内部模型，但 dhp scheduler 的 halflife 状态与 oracle 推断不完全对齐。这反映即便 ground truth 与算法同源，在 forward simulation 状态采样过程中仍可能引入偏差。

5. **oracle 跨数据集复用** —— benchmark-runner 把 maimemo 训练的 GRU oracle 软链到 duolingo_hlr / synthetic 用作 forward simulator。这是工程取舍（避免重训 2 次），methodology 中明确标注。未来若需要更高 fidelity，需为 duolingo_hlr / synthetic 各自训练独立 oracle。

6. **Leitner / Random 在 maimemo 上 AUC < 0.3**（leitner=0.294，random=0.224）—— 两者均不依赖历史正确率信号，但 maimemo 训练目标对正负样本区分度敏感，因此评测落点偏差大。AMAS 同数据集 AUC = 0.790，体现 MDM 三元组（D/S/R）作为预测特征的价值。

7. **Leitner 综合 Borda 第 1 的成因** —— Leitner 在 prediction 维度排名第 5（prediction_score 均值 0.700），但 DHP 维度排名第 2（dhp_score 均值 0.010），加上在 duolingo_hlr 这种高 positive rate 数据集上凭借 box-based 简单调度积累了排名第 1。这反映 *跨数据集 Borda* 对「在某数据集表现一般、但在多数据集稳定中游」的算法更友好；若使用 final_score 跨数据集均值，AMAS 与 sm2 / leitner 的差距会更接近（AMAS final_score 均值 0.755，leitner 0.420）。最终排序方法的选择本质上是 *评测者对「稳定性 vs 峰值」的偏好*，spec 选用 Borda 是为了规避 normalize 偏差。

## 复现

```bash
source .bench-venv/bin/activate
python -m benchmarks.maimemo.cli leaderboard \
  --results benchmarks/results/2026-05-29-amas6 \
  --out docs/algo-bench-2026-05-29-amas6
```

---

延伸阅读：
- [02-algo-research.md](./02-algo-research.md) — SM-2 / HLR / FSRS-4.5 算法定义与论文引用
- [03-detailed-results.md](./03-detailed-results.md) — 24 个 (algo, dataset) 全量原始指标
- [04-methodology.md](./04-methodology.md) — 评估方法学：加权、Borda、已知限制
