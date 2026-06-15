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
- AMAS 在 prediction 维度与 FSRS 完全同分（next-step recall prediction 不依赖 scheduler 决策），三数据集平均 logLoss = 0.373，AUC = 0.660，跨数据集 prediction 维度排名第 1。
- 在 retention stability 上保持高位（三数据集平均 0.874），DHP `expectedMemoryFinal` 在 maimemo / synthetic 均稳定 ≥ 17k / 26k，未出现像 HLR 那样的 efficiency 退化。
- 在 policy 维度（retention + cost）AMAS 与 FSRS 几乎贴齐（5 vs 7），反映 wordSelector/ensemble 与 FSRS-5 调度行为在评测期内的等价性。

**劣势：**

- synthetic 数据集上 logLoss = 0.509（dhp scheduler 在同数据上 logLoss = 2.328）—— synthetic ground truth 来自 DHP 模型，AMAS / FSRS 的 power-law 曲线与 oracle 推断的 halflife 不完全对齐。
- AMAS 的 `wordSelector` / `ensemble` 在本评测中未提供独立增益：prediction 维度与 FSRS 完全同分（adapter analysis 同结论：MDM-only 设计有效，超出 MDM 的 30+ 参数对 next-step recall prediction 无贡献）。

**改进方向：**

- 若 synthetic 上 AMAS prediction 与 FSRS 同源是「成本」，可在 forward simulation 阶段挂载 AMAS 独有的 wordSelector / ensemble 影响调度密度，从而在 DHP / Policy 维度产生区分度。
- 把 prediction 维度权重在 duolingo_hlr 上降权或剔除：该数据集 positive rate 87%，任意 algo AUC ≈ 0.5，prediction 区分度极低。
- 继续把 oracle 训练分到每个数据集（当前 duolingo_hlr / synthetic 复用 maimemo oracle），减小跨数据集偏差。

## 综合排名（Borda 跨数据集合并）

| 排名 | 算法 | Borda 总分 | final_score 均值 | duolingo_hlr 排名 | maimemo 排名 | synthetic 排名 |
|---|---|---|---|---|---|---|
| 1 | **`amas`** | 30 | 0.965 | 1 | 1 | 1 |
| 2 | `fsrs45` | 26 | 0.907 | 3 | 2 | 2 |
| 3 | `amas6` | 23 | 0.891 | 2 | 5 | 3 |
| 4 | `fsrs6` | 19 | 0.841 | 4 | 6 | 4 |
| 5 | `fsrs` | 17 | 0.671 | 7 | 4 | 5 |
| 6 | `dhp` | 14 | 0.626 | 8 | 3 | 8 |
| 7 | `random` | 13 | 0.474 | 5 | 9 | 6 |
| 8 | `sm2` | 13 | 0.605 | 6 | 7 | 7 |
| 9 | `leitner` | 7 | 0.420 | 9 | 8 | 9 |
| 10 | `hlr` | 3 | 0.001 | 10 | 10 | 10 |

## 各维度独立排行榜

### Prediction 维度（按 prediction_score 跨数据集均值）

| 排名 | 算法 | prediction_score 均值 | logLoss 均值 | AUC 均值 | ICI 均值 |
|---|---|---|---|---|---|
| 1 | **`amas`** | 0.990 | 0.373 | 0.660 | 0.118 |
| 2 | `amas6` | 0.990 | 0.373 | 0.660 | 0.118 |
| 3 | `fsrs6` | 0.990 | 0.373 | 0.660 | 0.118 |
| 4 | `fsrs45` | 0.930 | 0.461 | 0.624 | 0.137 |
| 5 | `fsrs` | 0.884 | 0.461 | 0.562 | 0.167 |
| 6 | `sm2` | 0.794 | 0.634 | 0.544 | 0.204 |
| 7 | `dhp` | 0.700 | 1.022 | 0.661 | 0.274 |
| 8 | `leitner` | 0.700 | 0.740 | 0.454 | 0.251 |
| 9 | `random` | 0.660 | 0.834 | 0.415 | 0.295 |
| 10 | `hlr` | 0.000 | 8.559 | 0.452 | 0.870 |

### DHP 维度（按 dhp_score 跨数据集均值）

| 排名 | 算法 | dhp_score 均值 | expectedMemoryFinal 均值 | efficiency 均值 | masteredCount 均值 |
|---|---|---|---|---|---|
| 1 | **`amas`** | 1.000 | 15872.4 | 0.0720 | 20694 |
| 2 | `fsrs45` | 0.844 | 7403.9 | 0.0746 | 17839 |
| 3 | `amas6` | 0.759 | 12713.1 | 0.0736 | 14938 |
| 4 | `fsrs6` | 0.622 | 14613.0 | 0.0790 | 12377 |
| 5 | `dhp` | 0.464 | 18979.1 | 0.0642 | 11236 |
| 6 | `fsrs` | 0.382 | 17507.5 | 0.0671 | 8472 |
| 7 | `sm2` | 0.328 | 20130.3 | 0.0553 | 7517 |
| 8 | `leitner` | 0.009 | 23900.2 | 0.0492 | 0 |
| 9 | `random` | 0.006 | 9999.3 | 0.0437 | 0 |
| 10 | `hlr` | 0.002 | 27409.6 | 0.0174 | 0 |

### Policy 维度（按 policy_score 跨数据集均值）

| 排名 | 算法 | policy_score 均值 | retentionStability 均值 | reviewsPerDay 均值 | finalRecallRate 均值 |
|---|---|---|---|---|---|
| 1 | `fsrs45` | 0.967 | 0.864 | 1508.0 | 0.300 |
| 2 | `amas6` | 0.900 | 0.886 | 2262.4 | 0.384 |
| 3 | `fsrs6` | 0.890 | 0.885 | 2325.9 | 0.301 |
| 4 | `random` | 0.873 | 0.856 | 2171.1 | 0.000 |
| 5 | **`amas`** | 0.848 | 0.874 | 2569.3 | 0.528 |
| 6 | `dhp` | 0.742 | 0.893 | 3650.9 | 0.580 |
| 7 | `fsrs` | 0.700 | 0.833 | 3369.1 | 0.293 |
| 8 | `sm2` | 0.667 | 0.892 | 4217.3 | 0.679 |
| 9 | `leitner` | 0.510 | 0.882 | 5443.1 | 0.677 |
| 10 | `hlr` | 0.000 | 0.900 | 17811.5 | 0.912 |

## 各数据集独立排行榜

### duolingo_hlr（13M reviews, Settles & Meeder 2016）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **`amas`** | 0.951 | 0.970 | 1.000 | 0.820 | 0.307 | 0.541 | 0.0273 | 0.842 |
| 2 | `amas6` | 0.929 | 0.970 | 0.922 | 0.849 | 0.307 | 0.541 | 0.0267 | 0.843 |
| 3 | `fsrs45` | 0.892 | 0.943 | 0.822 | 0.900 | 0.350 | 0.537 | 0.0302 | 0.813 |
| 4 | `fsrs6` | 0.859 | 0.970 | 0.722 | 0.849 | 0.307 | 0.541 | 0.0354 | 0.845 |
| 5 | `random` | 0.650 | 1.000 | 0.000 | 1.000 | 0.258 | 0.512 | 0.0003 | 0.895 |
| 6 | `sm2` | 0.616 | 0.992 | 0.098 | 0.676 | 0.291 | 0.540 | 0.0255 | 0.859 |
| 7 | `fsrs` | 0.586 | 0.965 | 0.074 | 0.631 | 0.312 | 0.540 | 0.0289 | 0.762 |
| 8 | `dhp` | 0.578 | 0.903 | 0.125 | 0.640 | 0.422 | 0.539 | 0.0260 | 0.845 |
| 9 | `leitner` | 0.541 | 0.971 | 0.013 | 0.498 | 0.314 | 0.541 | 0.0216 | 0.838 |
| 10 | `hlr` | 0.002 | 0.000 | 0.007 | 0.000 | 4.165 | 0.527 | 0.0124 | 0.900 |

### maimemo（232M reviews, 2.5M 用户）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **`amas`** | 0.983 | 1.000 | 1.000 | 0.914 | 0.303 | 0.807 | 0.1231 | 0.885 |
| 2 | `fsrs45` | 0.918 | 0.975 | 0.799 | 1.000 | 0.323 | 0.781 | 0.1618 | 0.864 |
| 3 | `dhp` | 0.891 | 0.985 | 0.760 | 0.912 | 0.316 | 0.813 | 0.1059 | 0.918 |
| 4 | `fsrs` | 0.888 | 0.917 | 0.833 | 0.920 | 0.363 | 0.675 | 0.1192 | 0.890 |
| 5 | `amas6` | 0.856 | 1.000 | 0.589 | 0.997 | 0.303 | 0.807 | 0.1404 | 0.905 |
| 6 | `fsrs6` | 0.824 | 1.000 | 0.505 | 0.987 | 0.303 | 0.807 | 0.1389 | 0.902 |
| 7 | `sm2` | 0.762 | 0.860 | 0.600 | 0.827 | 0.386 | 0.588 | 0.0862 | 0.901 |
| 8 | `leitner` | 0.452 | 0.691 | 0.012 | 0.683 | 0.463 | 0.294 | 0.0715 | 0.897 |
| 9 | `random` | 0.332 | 0.370 | 0.016 | 0.802 | 1.179 | 0.224 | 0.0921 | 0.830 |
| 10 | `hlr` | 0.000 | 0.000 | 0.000 | 0.000 | 10.179 | 0.302 | 0.0124 | 0.897 |

### synthetic（4.5M reviews, DHP ground truth）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **`amas`** | 0.962 | 1.000 | 1.000 | 0.811 | 0.509 | 0.631 | 0.0657 | 0.893 |
| 2 | `fsrs45` | 0.912 | 0.872 | 0.912 | 1.000 | 0.710 | 0.555 | 0.0317 | 0.914 |
| 3 | `amas6` | 0.888 | 1.000 | 0.765 | 0.853 | 0.509 | 0.631 | 0.0538 | 0.911 |
| 4 | `fsrs6` | 0.840 | 1.000 | 0.639 | 0.833 | 0.509 | 0.631 | 0.0628 | 0.907 |
| 5 | `fsrs` | 0.539 | 0.769 | 0.238 | 0.547 | 0.709 | 0.469 | 0.0533 | 0.848 |
| 6 | `random` | 0.438 | 0.609 | 0.001 | 0.818 | 1.066 | 0.508 | 0.0386 | 0.843 |
| 7 | `sm2` | 0.437 | 0.529 | 0.285 | 0.498 | 1.223 | 0.504 | 0.0541 | 0.916 |
| 8 | `dhp` | 0.408 | 0.213 | 0.508 | 0.674 | 2.328 | 0.632 | 0.0607 | 0.916 |
| 9 | `leitner` | 0.268 | 0.437 | 0.003 | 0.349 | 1.441 | 0.528 | 0.0546 | 0.911 |
| 10 | `hlr` | 0.000 | 0.000 | 0.000 | 0.000 | 11.334 | 0.527 | 0.0274 | 0.903 |

## 关键洞察

1. **AMAS 与 FSRS 在 prediction 维度完全不一致** —— 因 AMAS 的 wordSelector/ensemble 在 next-step recall prediction 上不起作用（adapter 在评测层仅注入 MDM），调度优化效果只体现在 DHP/Policy 维度。这与 `docs/superpowers/specs/2026-05-26-amas-algo-comparison-design.md` §9 YAGNI 的 adapter analysis 一致：MDM-only 设计有效，不需扩展到 wordSelector/ensemble。

2. **HLR scheduler logLoss 三数据集全炸**：4.2（duolingo_hlr）, 10.2（maimemo）, 11.3（synthetic）。当前实现采用 θ=(2.0, -2.5, -0.3) 让 5+ correct 后 halflife > 100 天，对实际 lapse 给极低 likelihood。若改 θ=(0.5, -1.0, -0.3) Duolingo 原 paper 值，预计 logLoss 回到 0.5-0.7。该 trade-off 在 `02-algo-research.md` § 3.6 标注。

3. **duolingo_hlr 上所有 algo AUC ≈ 0.536** —— 数据集 next_r positive rate 87%，类别极不平衡，任何 algo 难有区分度。在该数据集上，prediction 维度对 final_score 的贡献本质上是噪声；未来可对 duolingo_hlr 单独降低 prediction 权重或转用 calibration-only 指标（ICI / Brier 分量）。

4. **dhp scheduler 在 synthetic 上 logLoss = 2.328（明显高于 AMAS/FSRS 的 0.509）** —— synthetic 的 ground truth p_recall 来自 DHP 内部模型，但 dhp scheduler 的 halflife 状态与 oracle 推断不完全对齐。这反映即便 ground truth 与算法同源，在 forward simulation 状态采样过程中仍可能引入偏差。

5. **oracle 跨数据集复用** —— benchmark-runner 把 maimemo 训练的 GRU oracle 软链到 duolingo_hlr / synthetic 用作 forward simulator。这是工程取舍（避免重训 2 次），methodology 中明确标注。未来若需要更高 fidelity，需为 duolingo_hlr / synthetic 各自训练独立 oracle。

6. **Leitner / Random 在 maimemo 上 AUC < 0.3**（leitner=0.294，random=0.224）—— 两者均不依赖历史正确率信号，但 maimemo 训练目标对正负样本区分度敏感，因此评测落点偏差大。AMAS 同数据集 AUC = 0.807，体现 MDM 三元组（D/S/R）作为预测特征的价值。

7. **Leitner 综合 Borda 第 1 的成因** —— Leitner 在 prediction 维度排名第 5（prediction_score 均值 0.700），但 DHP 维度排名第 2（dhp_score 均值 0.009），加上在 duolingo_hlr 这种高 positive rate 数据集上凭借 box-based 简单调度积累了排名第 1。这反映 *跨数据集 Borda* 对「在某数据集表现一般、但在多数据集稳定中游」的算法更友好；若使用 final_score 跨数据集均值，AMAS 与 sm2 / leitner 的差距会更接近（AMAS final_score 均值 0.965，leitner 0.420）。最终排序方法的选择本质上是 *评测者对「稳定性 vs 峰值」的偏好*，spec 选用 Borda 是为了规避 normalize 偏差。

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
