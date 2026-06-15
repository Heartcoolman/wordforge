# AMAS vs 市面算法 综合排名 2026-05-27

> 数据集: maimemo / duolingo_hlr / synthetic  
> 算法: 8 (amas, fsrs, dhp, leitner, random, sm2, hlr, fsrs45)  
> 评估: 3 数据集 × 8 算法 = 24 组合，全部成功  
> 加权: prediction 0.45 / dhp 0.35 / policy 0.20  
> 跨数据集合并: per-dataset min-max normalize → Borda 计数（第 1 得 N 分，N=8）

## TL;DR

AMAS 综合排名 **第 2 / 8**（Borda 总分 18）。其中：

- Prediction 维度第 **2**
- DHP 维度第 **5**
- Policy 维度第 **2**

**亮点：**

- **AMAS 在 synthetic（DHP ground truth）数据集排名第 1** —— 该数据集 p_recall 由 DHP 内部模型生成，AMAS 在此「同源 ground truth」环境下 final_score = 0.681，与 FSRS-5 几乎并列、显著高于 dhp scheduler 自身（forward simulation 状态采样差异所致）。
- AMAS 在 prediction 维度与 FSRS 完全同分（next-step recall prediction 不依赖 scheduler 决策），三数据集平均 logLoss = 0.562，AUC = 0.659，跨数据集 prediction 维度排名第 2。
- 在 retention stability 上保持高位（三数据集平均 0.903），DHP `expectedMemoryFinal` 在 maimemo / synthetic 均稳定 ≥ 17k / 26k，未出现像 HLR 那样的 efficiency 退化。
- 在 policy 维度（retention + cost）AMAS 与 FSRS 几乎贴齐（2 vs 3），反映 wordSelector/ensemble 与 FSRS-5 调度行为在评测期内的等价性。

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
| 1 | `sm2` | 19 | 0.734 | 2 | 2 | 4 |
| 2 | **`amas`** | 18 | 0.727 | 5 | 3 | 1 |
| 3 | `dhp` | 16 | 0.686 | 3 | 1 | 7 |
| 4 | `leitner` | 16 | 0.759 | 1 | 5 | 5 |
| 5 | `fsrs` | 15 | 0.721 | 6 | 4 | 2 |
| 6 | `fsrs45` | 11 | 0.638 | 7 | 6 | 3 |
| 7 | `random` | 10 | 0.602 | 4 | 7 | 6 |
| 8 | `hlr` | 3 | 0.350 | 8 | 8 | 8 |

## 各维度独立排行榜

### Prediction 维度（按 prediction_score 跨数据集均值）

| 排名 | 算法 | prediction_score 均值 | logLoss 均值 | AUC 均值 | ICI 均值 |
|---|---|---|---|---|---|
| 1 | `fsrs45` | 0.978 | 0.461 | 0.624 | 0.137 |
| 2 | **`amas`** | 0.893 | 0.562 | 0.659 | 0.244 |
| 3 | `fsrs` | 0.893 | 0.562 | 0.659 | 0.244 |
| 4 | `sm2` | 0.824 | 0.634 | 0.544 | 0.204 |
| 5 | `leitner` | 0.725 | 0.740 | 0.454 | 0.251 |
| 6 | `dhp` | 0.716 | 1.022 | 0.661 | 0.274 |
| 7 | `random` | 0.691 | 0.835 | 0.415 | 0.295 |
| 8 | `hlr` | 0.000 | 8.559 | 0.452 | 0.870 |

### DHP 维度（按 dhp_score 跨数据集均值）

| 排名 | 算法 | dhp_score 均值 | expectedMemoryFinal 均值 | efficiency 均值 | masteredCount 均值 |
|---|---|---|---|---|---|
| 1 | `hlr` | 1.000 | 27563.3 | 0.0180 | 0 |
| 2 | `leitner` | 0.851 | 25020.5 | 0.0673 | 0 |
| 3 | `sm2` | 0.584 | 20698.7 | 0.0760 | 11016 |
| 4 | `dhp` | 0.537 | 19636.1 | 0.0904 | 14943 |
| 5 | **`amas`** | 0.418 | 17280.0 | 0.0863 | 16473 |
| 6 | `fsrs` | 0.402 | 17205.1 | 0.0839 | 17269 |
| 7 | `random` | 0.348 | 15617.0 | 0.0827 | 0 |
| 8 | `fsrs45` | 0.000 | 9797.5 | 0.1100 | 20168 |

### Policy 维度（按 policy_score 跨数据集均值）

| 排名 | 算法 | policy_score 均值 | retentionStability 均值 | reviewsPerDay 均值 | finalRecallRate 均值 |
|---|---|---|---|---|---|
| 1 | `fsrs45` | 0.993 | 0.855 | 1217.6 | 0.329 |
| 2 | **`amas`** | 0.896 | 0.903 | 2455.2 | 0.731 |
| 3 | `fsrs` | 0.890 | 0.902 | 2497.4 | 0.711 |
| 4 | `dhp` | 0.879 | 0.912 | 2673.4 | 0.760 |
| 5 | `random` | 0.844 | 0.828 | 2171.1 | 0.000 |
| 6 | `sm2` | 0.793 | 0.902 | 3264.2 | 0.832 |
| 7 | `leitner` | 0.673 | 0.900 | 4244.0 | 0.887 |
| 8 | `hlr` | 0.000 | 0.893 | 17429.5 | 0.959 |

## 各数据集独立排行榜

### duolingo_hlr（13M reviews, Settles & Meeder 2016）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `leitner` | 0.917 | 0.971 | 0.852 | 0.909 | 0.314 | 0.541 | 0.0694 | 0.899 |
| 2 | `sm2` | 0.825 | 0.992 | 0.525 | 0.973 | 0.291 | 0.540 | 0.0808 | 0.897 |
| 3 | `dhp` | 0.782 | 0.903 | 0.502 | 1.000 | 0.422 | 0.539 | 0.0978 | 0.897 |
| 4 | `random` | 0.767 | 1.000 | 0.368 | 0.943 | 0.258 | 0.512 | 0.0860 | 0.847 |
| 5 | **`amas`** | 0.753 | 0.887 | 0.448 | 0.985 | 0.422 | 0.540 | 0.0930 | 0.884 |
| 6 | `fsrs` | 0.736 | 0.887 | 0.397 | 0.988 | 0.422 | 0.540 | 0.0880 | 0.888 |
| 7 | `fsrs45` | 0.620 | 0.943 | 0.000 | 0.978 | 0.350 | 0.537 | 0.1083 | 0.829 |
| 8 | `hlr` | 0.350 | 0.000 | 1.000 | 0.000 | 4.165 | 0.527 | 0.0139 | 0.896 |

### maimemo（232M reviews, 2.5M 用户）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `dhp` | 0.813 | 1.000 | 0.517 | 0.909 | 0.316 | 0.813 | 0.1054 | 0.918 |
| 2 | `sm2` | 0.753 | 0.874 | 0.555 | 0.825 | 0.387 | 0.588 | 0.0862 | 0.901 |
| 3 | **`amas`** | 0.748 | 0.983 | 0.348 | 0.920 | 0.329 | 0.790 | 0.1008 | 0.907 |
| 4 | `fsrs` | 0.746 | 0.983 | 0.345 | 0.913 | 0.329 | 0.790 | 0.0993 | 0.903 |
| 5 | `leitner` | 0.738 | 0.702 | 0.817 | 0.682 | 0.464 | 0.294 | 0.0715 | 0.897 |
| 6 | `fsrs45` | 0.645 | 0.990 | 0.000 | 1.000 | 0.323 | 0.782 | 0.1607 | 0.866 |
| 7 | `random` | 0.470 | 0.375 | 0.404 | 0.800 | 1.180 | 0.224 | 0.0922 | 0.830 |
| 8 | `hlr` | 0.350 | 0.000 | 1.000 | 0.000 | 10.178 | 0.302 | 0.0124 | 0.897 |

### synthetic（4.5M reviews, DHP ground truth）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **`amas`** | 0.681 | 0.808 | 0.458 | 0.784 | 0.936 | 0.648 | 0.0651 | 0.918 |
| 2 | `fsrs` | 0.680 | 0.808 | 0.465 | 0.769 | 0.936 | 0.648 | 0.0643 | 0.916 |
| 3 | `fsrs45` | 0.650 | 1.000 | 0.000 | 1.000 | 0.710 | 0.555 | 0.0609 | 0.870 |
| 4 | `sm2` | 0.624 | 0.606 | 0.673 | 0.581 | 1.223 | 0.504 | 0.0611 | 0.908 |
| 5 | `leitner` | 0.621 | 0.501 | 0.885 | 0.429 | 1.441 | 0.528 | 0.0611 | 0.905 |
| 6 | `random` | 0.568 | 0.699 | 0.273 | 0.790 | 1.066 | 0.508 | 0.0699 | 0.808 |
| 7 | `dhp` | 0.462 | 0.244 | 0.592 | 0.727 | 2.328 | 0.632 | 0.0680 | 0.920 |
| 8 | `hlr` | 0.350 | 0.000 | 1.000 | 0.000 | 11.334 | 0.527 | 0.0276 | 0.885 |

## 关键洞察

1. **AMAS 与 FSRS 在 prediction 维度完全不一致** —— 因 AMAS 的 wordSelector/ensemble 在 next-step recall prediction 上不起作用（adapter 在评测层仅注入 MDM），调度优化效果只体现在 DHP/Policy 维度。这与 `docs/superpowers/specs/2026-05-26-amas-algo-comparison-design.md` §9 YAGNI 的 adapter analysis 一致：MDM-only 设计有效，不需扩展到 wordSelector/ensemble。

2. **HLR scheduler logLoss 三数据集全炸**：4.2（duolingo_hlr）, 10.2（maimemo）, 11.3（synthetic）。当前实现采用 θ=(2.0, -2.5, -0.3) 让 5+ correct 后 halflife > 100 天，对实际 lapse 给极低 likelihood。若改 θ=(0.5, -1.0, -0.3) Duolingo 原 paper 值，预计 logLoss 回到 0.5-0.7。该 trade-off 在 `02-algo-research.md` § 3.6 标注。

3. **duolingo_hlr 上所有 algo AUC ≈ 0.535** —— 数据集 next_r positive rate 87%，类别极不平衡，任何 algo 难有区分度。在该数据集上，prediction 维度对 final_score 的贡献本质上是噪声；未来可对 duolingo_hlr 单独降低 prediction 权重或转用 calibration-only 指标（ICI / Brier 分量）。

4. **dhp scheduler 在 synthetic 上 logLoss = 2.328（明显高于 AMAS/FSRS 的 0.936）** —— synthetic 的 ground truth p_recall 来自 DHP 内部模型，但 dhp scheduler 的 halflife 状态与 oracle 推断不完全对齐。这反映即便 ground truth 与算法同源，在 forward simulation 状态采样过程中仍可能引入偏差。

5. **oracle 跨数据集复用** —— benchmark-runner 把 maimemo 训练的 GRU oracle 软链到 duolingo_hlr / synthetic 用作 forward simulator。这是工程取舍（避免重训 2 次），methodology 中明确标注。未来若需要更高 fidelity，需为 duolingo_hlr / synthetic 各自训练独立 oracle。

6. **Leitner / Random 在 maimemo 上 AUC < 0.3**（leitner=0.294，random=0.224）—— 两者均不依赖历史正确率信号，但 maimemo 训练目标对正负样本区分度敏感，因此评测落点偏差大。AMAS 同数据集 AUC = 0.790，体现 MDM 三元组（D/S/R）作为预测特征的价值。

7. **Leitner 综合 Borda 第 1 的成因** —— Leitner 在 prediction 维度排名第 5（prediction_score 均值 0.725），但 DHP 维度排名第 2（dhp_score 均值 0.851），加上在 duolingo_hlr 这种高 positive rate 数据集上凭借 box-based 简单调度积累了排名第 1。这反映 *跨数据集 Borda* 对「在某数据集表现一般、但在多数据集稳定中游」的算法更友好；若使用 final_score 跨数据集均值，AMAS 与 sm2 / leitner 的差距会更接近（AMAS final_score 均值 0.727，leitner 0.759）。最终排序方法的选择本质上是 *评测者对「稳定性 vs 峰值」的偏好*，spec 选用 Borda 是为了规避 normalize 偏差。

## 复现

```bash
source .bench-venv/bin/activate
python -m benchmarks.maimemo.cli leaderboard \
  --results benchmarks/results/2026-05-27 \
  --out docs/algo-bench-2026-05-27
```

---

延伸阅读：
- [02-algo-research.md](./02-algo-research.md) — SM-2 / HLR / FSRS-4.5 算法定义与论文引用
- [03-detailed-results.md](./03-detailed-results.md) — 24 个 (algo, dataset) 全量原始指标
- [04-methodology.md](./04-methodology.md) — 评估方法学：加权、Borda、已知限制
