# AMAS vs 市面算法 综合排名 2026-05-26

> 数据集: maimemo / duolingo_hlr / synthetic  
> 算法: 8 (amas, fsrs, dhp, leitner, random, sm2, hlr, fsrs45)  
> 评估: 3 数据集 × 8 算法 = 24 组合，全部成功  
> 加权: prediction 0.45 / dhp 0.35 / policy 0.20  
> 跨数据集合并: per-dataset min-max normalize → Borda 计数（第 1 得 N 分，N=8）

## TL;DR

AMAS 综合排名 **第 2 / 8**（Borda 总分 18）。其中：

- Prediction 维度第 **2**
- DHP 维度第 **5**
- Policy 维度第 **3**

**亮点：**

- **AMAS 在 synthetic（DHP ground truth）数据集排名第 1** —— 该数据集 p_recall 由 DHP 内部模型生成，AMAS 在此「同源 ground truth」环境下 final_score = 0.687，与 FSRS-5 几乎并列、显著高于 dhp scheduler 自身（forward simulation 状态采样差异所致）。
- AMAS 在 prediction 维度与 FSRS 完全同分（next-step recall prediction 不依赖 scheduler 决策），三数据集平均 logLoss = 0.562，AUC = 0.660，跨数据集 prediction 维度排名第 2。
- 在 retention stability 上保持高位（三数据集平均 0.940），DHP `expectedMemoryFinal` 在 maimemo / synthetic 均稳定 ≥ 17k / 26k，未出现像 HLR 那样的 efficiency 退化。
- 在 policy 维度（retention + cost）AMAS 与 FSRS 几乎贴齐（3 vs 4），反映 wordSelector/ensemble 与 FSRS-5 调度行为在评测期内的等价性。

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
| 1 | `leitner` | 21 | 0.778 | 1 | 2 | 3 |
| 2 | **`amas`** | 18 | 0.719 | 4 | 4 | 1 |
| 3 | `sm2` | 17 | 0.743 | 2 | 3 | 5 |
| 4 | `dhp` | 16 | 0.699 | 3 | 1 | 7 |
| 5 | `fsrs` | 15 | 0.711 | 5 | 5 | 2 |
| 6 | `fsrs45` | 10 | 0.625 | 7 | 6 | 4 |
| 7 | `random` | 8 | 0.563 | 6 | 7 | 6 |
| 8 | `hlr` | 3 | 0.350 | 8 | 8 | 8 |

## 各维度独立排行榜

### Prediction 维度（按 prediction_score 跨数据集均值）

| 排名 | 算法 | prediction_score 均值 | logLoss 均值 | AUC 均值 | ICI 均值 |
|---|---|---|---|---|---|
| 1 | `fsrs45` | 0.977 | 0.461 | 0.624 | 0.137 |
| 2 | **`amas`** | 0.891 | 0.562 | 0.660 | 0.244 |
| 3 | `fsrs` | 0.891 | 0.562 | 0.660 | 0.244 |
| 4 | `sm2` | 0.821 | 0.634 | 0.544 | 0.204 |
| 5 | `leitner` | 0.718 | 0.740 | 0.454 | 0.251 |
| 6 | `dhp` | 0.714 | 1.022 | 0.661 | 0.274 |
| 7 | `random` | 0.678 | 0.835 | 0.415 | 0.295 |
| 8 | `hlr` | 0.000 | 8.771 | 0.507 | 0.856 |

### DHP 维度（按 dhp_score 跨数据集均值）

| 排名 | 算法 | dhp_score 均值 | expectedMemoryFinal 均值 | efficiency 均值 | masteredCount 均值 |
|---|---|---|---|---|---|
| 1 | `hlr` | 1.000 | 25846.8 | 0.0358 | 9 |
| 2 | `leitner` | 0.954 | 25021.2 | 0.0673 | 0 |
| 3 | `sm2` | 0.604 | 20697.5 | 0.0760 | 10972 |
| 4 | `dhp` | 0.544 | 19633.7 | 0.0906 | 14969 |
| 5 | **`amas`** | 0.387 | 17280.5 | 0.0863 | 16485 |
| 6 | `fsrs` | 0.367 | 17208.4 | 0.0841 | 17279 |
| 7 | `random` | 0.305 | 15619.0 | 0.0827 | 0 |
| 8 | `fsrs45` | 0.000 | 11901.0 | 0.1026 | 20132 |

### Policy 维度（按 policy_score 跨数据集均值）

| 排名 | 算法 | policy_score 均值 | retentionStability 均值 | reviewsPerDay 均值 | finalRecallRate 均值 |
|---|---|---|---|---|---|
| 1 | `dhp` | 0.936 | 0.961 | 2668.6 | 0.760 |
| 2 | `fsrs45` | 0.926 | 0.876 | 1538.3 | 0.505 |
| 3 | **`amas`** | 0.912 | 0.940 | 2455.8 | 0.729 |
| 4 | `fsrs` | 0.910 | 0.939 | 2492.8 | 0.710 |
| 5 | `sm2` | 0.813 | 0.952 | 3264.0 | 0.832 |
| 6 | `random` | 0.756 | 0.859 | 2171.1 | 0.799 |
| 7 | `leitner` | 0.604 | 0.955 | 4244.1 | 0.888 |
| 8 | `hlr` | 0.000 | 0.952 | 8615.5 | 0.853 |

## 各数据集独立排行榜

### duolingo_hlr（13M reviews, Settles & Meeder 2016）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `leitner` | 0.920 | 0.970 | 0.997 | 0.671 | 0.314 | 0.541 | 0.0694 | 0.956 |
| 2 | `sm2` | 0.832 | 0.991 | 0.570 | 0.931 | 0.291 | 0.540 | 0.0808 | 0.945 |
| 3 | `dhp` | 0.794 | 0.900 | 0.540 | 1.000 | 0.422 | 0.539 | 0.0978 | 0.934 |
| 4 | **`amas`** | 0.742 | 0.883 | 0.470 | 0.897 | 0.422 | 0.540 | 0.0930 | 0.916 |
| 5 | `fsrs` | 0.723 | 0.883 | 0.404 | 0.922 | 0.422 | 0.540 | 0.0880 | 0.921 |
| 6 | `random` | 0.696 | 1.000 | 0.366 | 0.588 | 0.258 | 0.512 | 0.0860 | 0.859 |
| 7 | `fsrs45` | 0.579 | 0.941 | 0.000 | 0.777 | 0.350 | 0.537 | 0.1018 | 0.855 |
| 8 | `hlr` | 0.350 | 0.000 | 1.000 | 0.000 | 4.545 | 0.545 | 0.0413 | 0.956 |

### maimemo（232M reviews, 2.5M 用户）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `dhp` | 0.822 | 1.000 | 0.506 | 0.976 | 0.316 | 0.813 | 0.1060 | 0.974 |
| 2 | `leitner` | 0.764 | 0.684 | 0.931 | 0.652 | 0.464 | 0.294 | 0.0715 | 0.949 |
| 3 | `sm2` | 0.755 | 0.866 | 0.561 | 0.846 | 0.387 | 0.587 | 0.0862 | 0.949 |
| 4 | **`amas`** | 0.728 | 0.982 | 0.268 | 0.962 | 0.329 | 0.790 | 0.1007 | 0.946 |
| 5 | `fsrs` | 0.724 | 0.982 | 0.265 | 0.949 | 0.329 | 0.790 | 0.1000 | 0.938 |
| 6 | `fsrs45` | 0.645 | 0.989 | 0.000 | 1.000 | 0.323 | 0.782 | 0.1457 | 0.885 |
| 7 | `random` | 0.434 | 0.338 | 0.347 | 0.803 | 1.180 | 0.224 | 0.0922 | 0.873 |
| 8 | `hlr` | 0.350 | 0.000 | 1.000 | 0.000 | 10.241 | 0.443 | 0.0355 | 0.959 |

### synthetic（4.5M reviews, DHP ground truth）

| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **`amas`** | 0.687 | 0.808 | 0.424 | 0.875 | 0.936 | 0.648 | 0.0651 | 0.959 |
| 2 | `fsrs` | 0.686 | 0.808 | 0.432 | 0.858 | 0.936 | 0.648 | 0.0643 | 0.958 |
| 3 | `leitner` | 0.650 | 0.500 | 0.936 | 0.490 | 1.441 | 0.528 | 0.0611 | 0.960 |
| 4 | `fsrs45` | 0.650 | 1.000 | 0.000 | 1.000 | 0.710 | 0.555 | 0.0604 | 0.888 |
| 5 | `sm2` | 0.643 | 0.605 | 0.681 | 0.662 | 1.223 | 0.504 | 0.0611 | 0.962 |
| 6 | `random` | 0.561 | 0.698 | 0.203 | 0.878 | 1.066 | 0.508 | 0.0699 | 0.846 |
| 7 | `dhp` | 0.480 | 0.241 | 0.585 | 0.833 | 2.328 | 0.632 | 0.0680 | 0.976 |
| 8 | `hlr` | 0.350 | 0.000 | 1.000 | 0.000 | 11.528 | 0.533 | 0.0304 | 0.941 |

## 关键洞察

1. **AMAS 与 FSRS 在 prediction 维度完全一致** —— 因 AMAS 的 wordSelector/ensemble 在 next-step recall prediction 上不起作用（adapter 在评测层仅注入 MDM），调度优化效果只体现在 DHP/Policy 维度。这与 `docs/superpowers/specs/2026-05-26-amas-algo-comparison-design.md` §9 YAGNI 的 adapter analysis 一致：MDM-only 设计有效，不需扩展到 wordSelector/ensemble。

2. **HLR scheduler logLoss 三数据集全炸**：4.5（duolingo_hlr）, 10.2（maimemo）, 11.5（synthetic）。当前实现采用 θ=(2.0, -2.5, -0.3) 让 5+ correct 后 halflife > 100 天，对实际 lapse 给极低 likelihood。若改 θ=(0.5, -1.0, -0.3) Duolingo 原 paper 值，预计 logLoss 回到 0.5-0.7。该 trade-off 在 `02-algo-research.md` § 3.6 标注。

3. **duolingo_hlr 上所有 algo AUC ≈ 0.537** —— 数据集 next_r positive rate 87%，类别极不平衡，任何 algo 难有区分度。在该数据集上，prediction 维度对 final_score 的贡献本质上是噪声；未来可对 duolingo_hlr 单独降低 prediction 权重或转用 calibration-only 指标（ICI / Brier 分量）。

4. **dhp scheduler 在 synthetic 上 logLoss = 2.328（明显高于 AMAS/FSRS 的 0.936）** —— synthetic 的 ground truth p_recall 来自 DHP 内部模型，但 dhp scheduler 的 halflife 状态与 oracle 推断不完全对齐。这反映即便 ground truth 与算法同源，在 forward simulation 状态采样过程中仍可能引入偏差。

5. **oracle 跨数据集复用** —— benchmark-runner 把 maimemo 训练的 GRU oracle 软链到 duolingo_hlr / synthetic 用作 forward simulator。这是工程取舍（避免重训 2 次），methodology 中明确标注。未来若需要更高 fidelity，需为 duolingo_hlr / synthetic 各自训练独立 oracle。

6. **Leitner / Random 在 maimemo 上 AUC < 0.3**（leitner=0.294，random=0.224）—— 两者均不依赖历史正确率信号，但 maimemo 训练目标对正负样本区分度敏感，因此评测落点偏差大。AMAS 同数据集 AUC = 0.790，体现 MDM 三元组（D/S/R）作为预测特征的价值。

7. **Leitner 综合 Borda 第 1 的成因** —— Leitner 在 prediction 维度排名第 5（prediction_score 均值 0.718），但 DHP 维度排名第 2（dhp_score 均值 0.954），加上在 duolingo_hlr 这种高 positive rate 数据集上凭借 box-based 简单调度积累了排名第 1。这反映 *跨数据集 Borda* 对「在某数据集表现一般、但在多数据集稳定中游」的算法更友好；若使用 final_score 跨数据集均值，AMAS 与 sm2 / leitner 的差距会更接近（AMAS final_score 均值 0.719，leitner 0.778）。最终排序方法的选择本质上是 *评测者对「稳定性 vs 峰值」的偏好*，spec 选用 Borda 是为了规避 normalize 偏差。

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
