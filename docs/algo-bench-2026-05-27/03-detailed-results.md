# 详细原始指标 2026-05-27

> 24 个 (algo, dataset) 组合的全量原始指标 + 归一化分数。  
> 由 `benchmarks/maimemo/leaderboard.py` 从 `benchmarks/results/2026-05-27/*.json` 生成。

## duolingo_hlr

### `leitner` × `duolingo_hlr` — 排名 1 / 8

**Prediction：**

- `logLoss` = 0.314299
- `ici` = 0.111311
- `auc` = 0.541405
- `maeP` = 0.197483
- `prediction_raw` = 0.766168  →  `prediction_score` = 0.970892

**DHP：**

- `expectedMemoryFinal` = 11238.4212
- `masteredCount` = 0
- `totalReviews` = 162031
- `efficiency` = 0.069360
- `dhp_raw` = 5653.8906  →  `dhp_score` = 0.851739

**Policy：**

- `finalRecallRate` = 0.912200
- `reviewsPerDay` = 1800.3444
- `retentionStability` = 0.899031
- `policy_raw` = 0.859498  →  `policy_score` = 0.909328

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.916876**
- `dataset_rank` = **1** / 8
- `borda_points` = **8**
- `runtime_seconds` = 0.13
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `sm2` × `duolingo_hlr` — 排名 2 / 8

**Prediction：**

- `logLoss` = 0.291132
- `ici` = 0.084821
- `auc` = 0.540387
- `maeP` = 0.166071
- `prediction_raw` = 0.778443  →  `prediction_score` = 0.991729

**DHP：**

- `expectedMemoryFinal` = 8752.0663
- `masteredCount` = 6247
- `totalReviews` = 108378
- `efficiency` = 0.080755
- `dhp_raw` = 4416.4106  →  `dhp_score` = 0.524524

**Policy：**

- `finalRecallRate` = 0.804400
- `reviewsPerDay` = 1204.2000
- `retentionStability` = 0.896621
- `policy_raw` = 0.888100  →  `policy_score` = 0.973238

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.824509**
- `dataset_rank` = **2** / 8
- `borda_points` = **7**
- `runtime_seconds` = 0.13
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `dhp` × `duolingo_hlr` — 排名 3 / 8

**Prediction：**

- `logLoss` = 0.422126
- `ici` = 0.169570
- `auc` = 0.538558
- `maeP` = 0.239823
- `prediction_raw` = 0.726271  →  `prediction_score` = 0.903169

**DHP：**

- `expectedMemoryFinal` = 8562.9221
- `masteredCount` = 7295
- `totalReviews` = 87531
- `efficiency` = 0.097827
- `dhp_raw` = 4330.3745  →  `dhp_score` = 0.501774

**Policy：**

- `finalRecallRate` = 0.784300
- `reviewsPerDay` = 972.5667
- `retentionStability` = 0.897412
- `policy_raw` = 0.900078  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.782047**
- `dataset_rank` = **3** / 8
- `borda_points` = **6**
- `runtime_seconds` = 0.13
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `random` × `duolingo_hlr` — 排名 4 / 8

**Prediction：**

- `logLoss` = 0.258367
- `ici` = 0.062165
- `auc` = 0.512130
- `maeP` = 0.162569
- `prediction_raw` = 0.783316  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 7562.7746
- `masteredCount` = 0
- `totalReviews` = 87934
- `efficiency` = 0.086005
- `dhp_raw` = 3824.3898  →  `dhp_score` = 0.367982

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 977.0444
- `retentionStability` = 0.846532
- `policy_raw` = 0.874414  →  `policy_score` = 0.942656

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.767325**
- `dataset_rank` = **4** / 8
- `borda_points` = **5**
- `runtime_seconds` = 0.18
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `amas` × `duolingo_hlr` — 排名 5 / 8

**Prediction：**

- `logLoss` = 0.422241
- `ici` = 0.202638
- `auc` = 0.540354
- `maeP` = 0.274010
- `prediction_raw` = 0.716866  →  `prediction_score` = 0.887205

**DHP：**

- `expectedMemoryFinal` = 8162.9205
- `masteredCount` = 8329
- `totalReviews` = 87756
- `efficiency` = 0.093018
- `dhp_raw` = 4127.9693  →  `dhp_score` = 0.448254

**Policy：**

- `finalRecallRate` = 0.794400
- `reviewsPerDay` = 975.0667
- `retentionStability` = 0.884183
- `policy_raw` = 0.893338  →  `policy_score` = 0.984941

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.753119**
- `dataset_rank` = **5** / 8
- `borda_points` = **4**
- `runtime_seconds` = 0.21
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `fsrs` × `duolingo_hlr` — 排名 6 / 8

**Prediction：**

- `logLoss` = 0.422241
- `ici` = 0.202638
- `auc` = 0.540354
- `maeP` = 0.274010
- `prediction_raw` = 0.716866  →  `prediction_score` = 0.887205

**DHP：**

- `expectedMemoryFinal` = 7781.7099
- `masteredCount` = 8707
- `totalReviews` = 88380
- `efficiency` = 0.088048
- `dhp_raw` = 3934.8789  →  `dhp_score` = 0.397197

**Policy：**

- `finalRecallRate` = 0.693500
- `reviewsPerDay` = 982.0000
- `retentionStability` = 0.887565
- `policy_raw` = 0.894682  →  `policy_score` = 0.987945

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.735850**
- `dataset_rank` = **6** / 8
- `borda_points` = **3**
- `runtime_seconds` = 0.17
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `fsrs45` × `duolingo_hlr` — 排名 7 / 8

**Prediction：**

- `logLoss` = 0.349752
- `ici` = 0.138136
- `auc` = 0.536638
- `maeP` = 0.192721
- `prediction_raw` = 0.749600  →  `prediction_score` = 0.942769

**DHP：**

- `expectedMemoryFinal` = 4757.1371
- `masteredCount` = 11121
- `totalReviews` = 43910
- `efficiency` = 0.108338
- `dhp_raw` = 2432.7375  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.339900
- `reviewsPerDay` = 487.8889
- `retentionStability` = 0.829288
- `policy_raw` = 0.890250  →  `policy_score` = 0.978040

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.619854**
- `dataset_rank` = **7** / 8
- `borda_points` = **2**
- `runtime_seconds` = 0.15
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `hlr` × `duolingo_hlr` — 排名 8 / 8

**Prediction：**

- `logLoss` = 4.164875
- `ici` = 0.879993
- `auc` = 0.527324
- `maeP` = 0.886714
- `prediction_raw` = 0.194199  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 12415.2645
- `masteredCount` = 0
- `totalReviews` = 892175
- `efficiency` = 0.013916
- `dhp_raw` = 6214.5902  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.959000
- `reviewsPerDay` = 9913.0556
- `retentionStability` = 0.896386
- `policy_raw` = 0.452540  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.350000**
- `dataset_rank` = **8** / 8
- `borda_points` = **1**
- `runtime_seconds` = 0.13
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

---

## maimemo

### `dhp` × `maimemo` — 排名 1 / 8

**Prediction：**

- `logLoss` = 0.315753
- `ici` = 0.060406
- `auc` = 0.813089
- `maeP` = 0.221776
- `prediction_raw` = 0.862654  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 20365.8786
- `masteredCount` = 15209
- `totalReviews` = 193298
- `efficiency` = 0.105360
- `dhp_raw` = 10235.6193  →  `dhp_score` = 0.516550

**Policy：**

- `finalRecallRate` = 0.752600
- `reviewsPerDay` = 2147.7556
- `retentionStability` = 0.918139
- `policy_raw` = 0.851682  →  `policy_score` = 0.908669

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.812526**
- `dataset_rank` = **1** / 8
- `borda_points` = **8**
- `runtime_seconds` = 3.75
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `sm2` × `maimemo` — 排名 2 / 8

**Prediction：**

- `logLoss` = 0.386541
- `ici` = 0.096840
- `auc` = 0.587556
- `maeP` = 0.211683
- `prediction_raw` = 0.769907  →  `prediction_score` = 0.873590

**DHP：**

- `expectedMemoryFinal` = 21030.4121
- `masteredCount` = 12217
- `totalReviews` = 244040
- `efficiency` = 0.086176
- `dhp_raw` = 10558.2941  →  `dhp_score` = 0.555385

**Policy：**

- `finalRecallRate` = 0.867600
- `reviewsPerDay` = 2711.5556
- `retentionStability` = 0.900579
- `policy_raw` = 0.814712  →  `policy_score` = 0.825377

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.752575**
- `dataset_rank` = **2** / 8
- `borda_points` = **7**
- `runtime_seconds` = 3.76
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `amas` × `maimemo` — 排名 3 / 8

**Prediction：**

- `logLoss` = 0.329041
- `ici` = 0.070376
- `auc` = 0.789945
- `maeP` = 0.227385
- `prediction_raw` = 0.850063  →  `prediction_score` = 0.982838

**DHP：**

- `expectedMemoryFinal` = 17570.1688
- `masteredCount` = 17317
- `totalReviews` = 174384
- `efficiency` = 0.100756
- `dhp_raw` = 8835.4624  →  `dhp_score` = 0.348038

**Policy：**

- `finalRecallRate` = 0.758200
- `reviewsPerDay` = 1937.6000
- `retentionStability` = 0.907321
- `policy_raw` = 0.856780  →  `policy_score` = 0.920156

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.748122**
- `dataset_rank` = **3** / 8
- `borda_points` = **6**
- `runtime_seconds` = 4.06
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `fsrs` × `maimemo` — 排名 4 / 8

**Prediction：**

- `logLoss` = 0.329024
- `ici` = 0.070404
- `auc` = 0.789869
- `maeP` = 0.227373
- `prediction_raw` = 0.850035  →  `prediction_score` = 0.982801

**DHP：**

- `expectedMemoryFinal` = 17526.2014
- `masteredCount` = 18099
- `totalReviews` = 176570
- `efficiency` = 0.099259
- `dhp_raw` = 8812.7302  →  `dhp_score` = 0.345302

**Policy：**

- `finalRecallRate` = 0.708800
- `reviewsPerDay` = 1961.8889
- `retentionStability` = 0.903042
- `policy_raw` = 0.853426  →  `policy_score` = 0.912599

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.745636**
- `dataset_rank` = **4** / 8
- `borda_points` = **5**
- `runtime_seconds` = 5.98
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `leitner` × `maimemo` — 排名 5 / 8

**Prediction：**

- `logLoss` = 0.463516
- `ici` = 0.172090
- `auc` = 0.293767
- `maeP` = 0.305396
- `prediction_raw` = 0.643800  →  `prediction_score` = 0.701712

**DHP：**

- `expectedMemoryFinal` = 25386.8759
- `masteredCount` = 0
- `totalReviews` = 355196
- `efficiency` = 0.071473
- `dhp_raw` = 12729.1745  →  `dhp_score` = 0.816655

**Policy：**

- `finalRecallRate` = 0.895600
- `reviewsPerDay` = 3946.6222
- `retentionStability` = 0.896672
- `policy_raw` = 0.751005  →  `policy_score` = 0.681847

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.737969**
- `dataset_rank` = **5** / 8
- `borda_points` = **4**
- `runtime_seconds` = 3.74
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `fsrs45` × `maimemo` — 排名 6 / 8

**Prediction：**

- `logLoss` = 0.323230
- `ici` = 0.048860
- `auc` = 0.781936
- `maeP` = 0.156393
- `prediction_raw` = 0.855277  →  `prediction_score` = 0.989945

**DHP：**

- `expectedMemoryFinal` = 11726.5744
- `masteredCount` = 15901
- `totalReviews` = 72978
- `efficiency` = 0.160686
- `dhp_raw` = 5943.6302  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.449600
- `reviewsPerDay` = 810.8667
- `retentionStability` = 0.865527
- `policy_raw` = 0.892220  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.645475**
- `dataset_rank` = **6** / 8
- `borda_points` = **3**
- `runtime_seconds` = 3.81
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `random` × `maimemo` — 排名 7 / 8

**Prediction：**

- `logLoss` = 1.179584
- `ici` = 0.423863
- `auc` = 0.223966
- `maeP` = 0.491799
- `prediction_raw` = 0.404114  →  `prediction_score` = 0.375032

**DHP：**

- `expectedMemoryFinal` = 18507.5300
- `masteredCount` = 0
- `totalReviews` = 200760
- `efficiency` = 0.092187
- `dhp_raw` = 9299.8585  →  `dhp_score` = 0.403929

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 2230.6667
- `retentionStability` = 0.830220
- `policy_raw` = 0.803576  →  `policy_score` = 0.800289

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.470197**
- `dataset_rank` = **7** / 8
- `borda_points` = **2**
- `runtime_seconds` = 3.81
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `hlr` × `maimemo` — 排名 8 / 8

**Prediction：**

- `logLoss` = 10.177668
- `ici` = 0.872459
- `auc` = 0.302302
- `maeP` = 0.874389
- `prediction_raw` = 0.128953  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 28492.7790
- `masteredCount` = 0
- `totalReviews` = 2300798
- `efficiency` = 0.012384
- `dhp_raw` = 14252.5815  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.971000
- `reviewsPerDay` = 25564.4222
- `retentionStability` = 0.896723
- `policy_raw` = 0.448361  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.350000**
- `dataset_rank` = **8** / 8
- `borda_points` = **1**
- `runtime_seconds` = 3.75
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

---

## synthetic

### `amas` × `synthetic` — 排名 1 / 8

**Prediction：**

- `logLoss` = 0.935562
- `ici` = 0.459007
- `auc` = 0.648181
- `maeP` = 0.545352
- `prediction_raw` = 0.569640  →  `prediction_score` = 0.808446

**DHP：**

- `expectedMemoryFinal` = 26106.7608
- `masteredCount` = 23773
- `totalReviews` = 400768
- `efficiency` = 0.065142
- `dhp_raw` = 13085.9514  →  `dhp_score` = 0.457780

**Policy：**

- `finalRecallRate` = 0.639900
- `reviewsPerDay` = 4452.9778
- `retentionStability` = 0.917572
- `policy_raw` = 0.736137  →  `policy_score` = 0.783758

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.680775**
- `dataset_rank` = **1** / 8
- `borda_points` = **8**
- `runtime_seconds` = 0.36
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `fsrs` × `synthetic` — 排名 2 / 8

**Prediction：**

- `logLoss` = 0.935562
- `ici` = 0.459007
- `auc` = 0.648181
- `maeP` = 0.545352
- `prediction_raw` = 0.569640  →  `prediction_score` = 0.808446

**DHP：**

- `expectedMemoryFinal` = 26307.4035
- `masteredCount` = 25000
- `totalReviews` = 409345
- `efficiency` = 0.064267
- `dhp_raw` = 13185.8353  →  `dhp_score` = 0.464707

**Policy：**

- `finalRecallRate` = 0.729200
- `reviewsPerDay` = 4548.2778
- `retentionStability` = 0.915850
- `policy_raw` = 0.730511  →  `policy_score` = 0.768744

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.680197**
- `dataset_rank` = **2** / 8
- `borda_points` = **7**
- `runtime_seconds` = 0.23
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `fsrs45` × `synthetic` — 排名 3 / 8

**Prediction：**

- `logLoss` = 0.710241
- `ici` = 0.224132
- `auc` = 0.554540
- `maeP` = 0.386900
- `prediction_raw` = 0.657074  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 12908.7126
- `masteredCount` = 33483
- `totalReviews` = 211853
- `efficiency` = 0.060932
- `dhp_raw` = 6484.8223  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.199000
- `reviewsPerDay` = 2353.9222
- `retentionStability` = 0.869729
- `policy_raw` = 0.817168  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.650000**
- `dataset_rank` = **3** / 8
- `borda_points` = **6**
- `runtime_seconds` = 0.21
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `sm2` × `synthetic` — 排名 4 / 8

**Prediction：**

- `logLoss` = 1.223161
- `ici` = 0.430807
- `auc` = 0.503970
- `maeP` = 0.535939
- `prediction_raw` = 0.477317  →  `prediction_score` = 0.606181

**DHP：**

- `expectedMemoryFinal` = 32313.6670
- `masteredCount` = 14584
- `totalReviews` = 528912
- `efficiency` = 0.061095
- `dhp_raw` = 16187.3810  →  `dhp_score` = 0.672861

**Policy：**

- `finalRecallRate` = 0.824400
- `reviewsPerDay` = 5876.8000
- `retentionStability` = 0.907784
- `policy_raw` = 0.660052  →  `policy_score` = 0.580714

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.624425**
- `dataset_rank` = **4** / 8
- `borda_points` = **5**
- `runtime_seconds` = 0.19
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `leitner` × `synthetic` — 排名 5 / 8

**Prediction：**

- `logLoss` = 1.441303
- `ici` = 0.468880
- `auc` = 0.527717
- `maeP` = 0.571587
- `prediction_raw` = 0.429391  →  `prediction_score` = 0.501183

**DHP：**

- `expectedMemoryFinal` = 38436.2114
- `masteredCount` = 0
- `totalReviews` = 628645
- `efficiency` = 0.061141
- `dhp_raw` = 19248.6762  →  `dhp_score` = 0.885158

**Policy：**

- `finalRecallRate` = 0.853800
- `reviewsPerDay` = 6984.9444
- `retentionStability` = 0.904970
- `policy_raw` = 0.603238  →  `policy_score` = 0.429097

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.621157**
- `dataset_rank` = **5** / 8
- `borda_points` = **4**
- `runtime_seconds` = 0.16
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `random` × `synthetic` — 排名 6 / 8

**Prediction：**

- `logLoss` = 1.065810
- `ici` = 0.398710
- `auc` = 0.507953
- `maeP` = 0.498653
- `prediction_raw` = 0.519611  →  `prediction_score` = 0.698841

**DHP：**

- `expectedMemoryFinal` = 20780.5938
- `masteredCount` = 0
- `totalReviews` = 297500
- `efficiency` = 0.069851
- `dhp_raw` = 10425.2224  →  `dhp_score` = 0.273262

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 3305.5556
- `retentionStability` = 0.807836
- `policy_raw` = 0.738640  →  `policy_score` = 0.790437

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.568208**
- `dataset_rank` = **6** / 8
- `borda_points` = **3**
- `runtime_seconds` = 0.24
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `dhp` × `synthetic` — 排名 7 / 8

**Prediction：**

- `logLoss` = 2.328021
- `ici` = 0.592008
- `auc` = 0.631964
- `maeP` = 0.639759
- `prediction_raw` = 0.311987  →  `prediction_score` = 0.243972

**DHP：**

- `expectedMemoryFinal` = 29979.4026
- `masteredCount` = 22325
- `totalReviews` = 440999
- `efficiency` = 0.067981
- `dhp_raw` = 15023.6918  →  `dhp_score` = 0.592160

**Policy：**

- `finalRecallRate` = 0.743400
- `reviewsPerDay` = 4899.9889
- `retentionStability` = 0.919634
- `policy_raw` = 0.714818  →  `policy_score` = 0.726863

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.462416**
- `dataset_rank` = **7** / 8
- `borda_points` = **2**
- `runtime_seconds` = 0.18
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

### `hlr` × `synthetic` — 排名 8 / 8

**Prediction：**

- `logLoss` = 11.334093
- `ici` = 0.858481
- `auc` = 0.527237
- `maeP` = 0.858484
- `prediction_raw` = 0.200627  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 41781.7620
- `masteredCount` = 0
- `totalReviews` = 1513000
- `efficiency` = 0.027615
- `dhp_raw` = 20904.6885  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.947200
- `reviewsPerDay` = 16811.1111
- `retentionStability` = 0.884890
- `policy_raw` = 0.442445  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.350000**
- `dataset_rank` = **8** / 8
- `borda_points` = **1**
- `runtime_seconds` = 0.18
- `notes` = v0.6: HLR theta=paper(0.5,-1.0,-0.3); oracle 跨数据集复用

---
