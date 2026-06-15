# v0.7 vs v0.6 评测差异（v0.7 final）

> v0.6: 2026-05-27 / v0.7: 2026-05-27-oracle
> 状态: ✅ duolingo_hlr 与 synthetic 均训成独立 oracle

## 修改内容

| 数据集 | v0.6 oracle | v0.7 oracle | 训练参数 |
|---|---|---|---|
| maimemo | 原始（保留） | 原始（保留）| n/a |
| synthetic | maimemo 软链 | **独立训成** ✅ | MPS, 5 epochs, 1M rows, batch 512, logLoss 0.804 / AUC 0.635 |
| duolingo_hlr | maimemo 软链 | **独立训成** ✅ | MPS, 3 epochs, 200K rows, batch 128, PYTHONUNBUFFERED, logLoss 0.247 / AUC 0.579 |

**注**：duolingo_hlr 第一次尝试 (1M rows, 5 epochs, batch 512) 和第二次 (500K rows, 3 epochs, batch 256) 均 25 / 20 分钟 timeout。第三次缩小到 200K rows + batch 128 + unbuffered 才 4 分钟内训完。

## 综合排名对比

| 算法 | v0.6 Borda | v0.7 Borda | Δ | 主因 |
|---|---|---|---|---|
| sm2 | 19 | **20** | +1 | 独立 duolingo oracle 让 sm2 升至 duolingo 第 1 |
| **amas** | **18** | **18** | = | 排名稳定，维度内部变化 |
| fsrs | 15 | 17 | +2 | duolingo 排名从第 6 升第 4 |
| dhp | 16 | 16 | = | |
| leitner | 16 | 15 | -1 | duolingo 排名从第 1 跌第 2 |
| fsrs45 | 11 | 12 | +1 | |
| random | 10 | **7** | **-3** | 独立 oracle 揭穿虚高，duolingo expectedMemory 7563→28 |
| hlr | 3 | 3 | = | 仍垫底 |

## AMAS 维度内部变化

| 维度 | v0.6 排名 | v0.7 排名 |
|---|---|---|
| Prediction | 2 | 2 |
| DHP | 5 | 6 (-1) |
| Policy | 2 | 3 (-1) |
| 综合 | 2 | **2** |

DHP / Policy 各降 1 位但综合 Borda 未变（18 / 18）— per-dataset 排名分布从 v0.6 `(5,3,1)` 微调为 v0.7 `(5,3,1)` 完全不变。综合排名稳定。

## duolingo_hlr 上的 expectedMemory 变化（独立 oracle 揭穿效应）

| 算法 | v0.6 mem | v0.7 mem | Δ% |
|---|---|---|---|
| **random** | 7563 | **28** | **-99.6%** ⚠ |
| fsrs45 | 4757 | 2876 | -39.5% |
| leitner | 11238 | 9555 | -15.0% |
| amas | 8163 | 7352 | -9.9% |
| sm2 | 8752 | 8084 | -7.6% |
| fsrs | 7782 | 7546 | -3.0% |
| hlr | 12415 | 12342 | -0.6% |
| dhp | 8563 | 8684 | +1.4% |

**Random 大跌**反映 maimemo oracle 在 duolingo 上把所有词预测为高 retention（数据特征 87% positive），独立 oracle 学到 duolingo 真实模式后给 random scheduler 几乎零 retention 估计。

## synthetic 上的 expectedMemory 变化

| 算法 | v0.6 mem | v0.7 mem | Δ% |
|---|---|---|---|
| random | 20781 | 11475 | -44.8% |
| fsrs45 | 12909 | 7588 | -41.2% |
| fsrs | 26307 | 24119 | -8.3% |
| dhp | 29979 | 27914 | -6.9% |
| amas | 26107 | 24328 | -6.8% |
| leitner | 38436 | 36758 | -4.4% |
| sm2 | 32314 | 31263 | -3.3% |
| hlr | 41782 | 41390 | -0.9% |

主流算法（amas/fsrs/dhp/sm2/leitner）3-9% 下降按比例平移，相对排序稳定。

## Prediction 维度未受影响

`evaluate_scheduler_prediction` 直接用 scheduler 内部 p_recall 对比数据集 ground truth (next_r)，**不经过 oracle**。所以 logLoss / AUC / ICI / MAE 在 v0.6 → v0.7 完全相同（小数点 3 位全等）。

## 核心结论

1. **AMAS 综合排名稳定第 2 / 8**（v0.5 / v0.6 / v0.7 三轮全部第 2）
2. **sm2 综合升至第 1**（Borda 20）— 独立 duolingo oracle 强化了 sm2 在该数据集的优势
3. **Random 是 oracle 选择最敏感的算法** — duolingo expectedMemory 跌 99.6%，揭示其本质上的无效性
4. **HLR 仍垫底** — oracle 修复对它无救（数据特征导致）
5. **AMAS 在 synthetic 数据集第 1 持续保留** — 跨 v0.5/v0.6/v0.7 三轮稳定
