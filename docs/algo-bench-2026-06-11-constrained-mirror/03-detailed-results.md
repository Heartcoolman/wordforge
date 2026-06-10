# 详细原始指标 2026-05-26

> 24 个 (algo, dataset) 组合的全量原始指标 + 归一化分数。  
> 由 `benchmarks/maimemo/leaderboard.py` 从 `benchmarks/results/2026-05-26/*.json` 生成。

## duolingo_hlr

### `amas6` × `duolingo_hlr` — 排名 1 / 10

**Prediction：**

- `logLoss` = 0.306789
- `ici` = 0.117698
- `auc` = 0.541248
- `maeP` = 0.185367
- `prediction_raw` = 0.765707  →  `prediction_score` = 0.970109

**DHP：**

- `expectedMemoryFinal` = 4362.9217
- `masteredCount` = 6595
- `totalReviews` = 163349
- `efficiency` = 0.026709
- `dhp_raw` = 4696.6270  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.256600
- `reviewsPerDay` = 1814.9889
- `retentionStability` = 0.843368
- `policy_raw` = 0.830935  →  `policy_score` = 0.849087

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.956367**
- `dataset_rank` = **1** / 10
- `borda_points` = **10**
- `runtime_seconds` = 7.69
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `fsrs45` × `duolingo_hlr` — 排名 2 / 10

**Prediction：**

- `logLoss` = 0.349752
- `ici` = 0.138136
- `auc` = 0.536638
- `maeP` = 0.192721
- `prediction_raw` = 0.749600  →  `prediction_score` = 0.942769

**DHP：**

- `expectedMemoryFinal` = 2875.5093
- `masteredCount` = 5855
- `totalReviews` = 95205
- `efficiency` = 0.030203
- `dhp_raw` = 4189.1090  →  `dhp_score` = 0.891918

**Policy：**

- `finalRecallRate` = 0.163100
- `reviewsPerDay` = 1057.8333
- `retentionStability` = 0.813326
- `policy_raw` = 0.853771  →  `policy_score` = 0.899991

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.916416**
- `dataset_rank` = **2** / 10
- `borda_points` = **9**
- `runtime_seconds` = 7.62
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `fsrs6` × `duolingo_hlr` — 排名 3 / 10

**Prediction：**

- `logLoss` = 0.306789
- `ici` = 0.117698
- `auc` = 0.541248
- `maeP` = 0.185367
- `prediction_raw` = 0.765707  →  `prediction_score` = 0.970109

**DHP：**

- `expectedMemoryFinal` = 5848.2985
- `masteredCount` = 5106
- `totalReviews` = 165318
- `efficiency` = 0.035376
- `dhp_raw` = 3680.3280  →  `dhp_score` = 0.783567

**Policy：**

- `finalRecallRate` = 0.213300
- `reviewsPerDay` = 1836.8667
- `retentionStability` = 0.845167
- `policy_raw` = 0.830740  →  `policy_score` = 0.848654

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.880529**
- `dataset_rank` = **3** / 10
- `borda_points` = **8**
- `runtime_seconds` = 7.62
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `random` × `duolingo_hlr` — 排名 4 / 10

**Prediction：**

- `logLoss` = 0.258367
- `ici` = 0.062165
- `auc` = 0.512130
- `maeP` = 0.162569
- `prediction_raw` = 0.783316  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 27.6845
- `masteredCount` = 0
- `totalReviews` = 87934
- `efficiency` = 0.000315
- `dhp_raw` = 0.9450  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 977.0444
- `retentionStability` = 0.894980
- `policy_raw` = 0.898638  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.650000**
- `dataset_rank` = **4** / 10
- `borda_points` = **7**
- `runtime_seconds` = 7.66
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `sm2` × `duolingo_hlr` — 排名 5 / 10

**Prediction：**

- `logLoss` = 0.291132
- `ici` = 0.084821
- `auc` = 0.540387
- `maeP` = 0.166071
- `prediction_raw` = 0.778443  →  `prediction_score` = 0.991729

**DHP：**

- `expectedMemoryFinal` = 8083.6468
- `masteredCount` = 605
- `totalReviews` = 317096
- `efficiency` = 0.025493
- `dhp_raw` = 499.9790  →  `dhp_score` = 0.106275

**Policy：**

- `finalRecallRate` = 0.441300
- `reviewsPerDay` = 3523.2889
- `retentionStability` = 0.858994
- `policy_raw` = 0.753333  →  `policy_score` = 0.676109

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.618696**
- `dataset_rank` = **5** / 10
- `borda_points` = **6**
- `runtime_seconds` = 7.60
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `dhp` × `duolingo_hlr` — 排名 6 / 10

**Prediction：**

- `logLoss` = 0.422126
- `ici` = 0.169570
- `auc` = 0.538558
- `maeP` = 0.239823
- `prediction_raw` = 0.726271  →  `prediction_score` = 0.903169

**DHP：**

- `expectedMemoryFinal` = 8684.3396
- `masteredCount` = 797
- `totalReviews` = 334060
- `efficiency` = 0.025996
- `dhp_raw` = 635.8880  →  `dhp_score` = 0.135218

**Policy：**

- `finalRecallRate` = 0.319700
- `reviewsPerDay` = 3711.7778
- `retentionStability` = 0.845456
- `policy_raw` = 0.737139  →  `policy_score` = 0.640013

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.581755**
- `dataset_rank` = **6** / 10
- `borda_points` = **5**
- `runtime_seconds` = 7.60
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `leitner` × `duolingo_hlr` — 排名 7 / 10

**Prediction：**

- `logLoss` = 0.314299
- `ici` = 0.111311
- `auc` = 0.541405
- `maeP` = 0.197483
- `prediction_raw` = 0.766168  →  `prediction_score` = 0.970892

**DHP：**

- `expectedMemoryFinal` = 9555.1054
- `masteredCount` = 0
- `totalReviews` = 441676
- `efficiency` = 0.021634
- `dhp_raw` = 64.9020  →  `dhp_score` = 0.013620

**Policy：**

- `finalRecallRate` = 0.353700
- `reviewsPerDay` = 4907.5111
- `retentionStability` = 0.837910
- `policy_raw` = 0.673580  →  `policy_score` = 0.498336

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.541336**
- `dataset_rank` = **7** / 10
- `borda_points` = **4**
- `runtime_seconds` = 7.59
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `amas` × `duolingo_hlr` — 排名 8 / 10

**Prediction：**

- `logLoss` = 0.422470
- `ici` = 0.216897
- `auc` = 0.536992
- `maeP` = 0.294273
- `prediction_raw` = 0.711535  →  `prediction_score` = 0.878154

**DHP：**

- `expectedMemoryFinal` = 8072.8141
- `masteredCount` = 387
- `totalReviews` = 309077
- `efficiency` = 0.026119
- `dhp_raw` = 349.2570  →  `dhp_score` = 0.074177

**Policy：**

- `finalRecallRate` = 0.195600
- `reviewsPerDay` = 3434.1889
- `retentionStability` = 0.772850
- `policy_raw` = 0.714716  →  `policy_score` = 0.590030

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.539137**
- `dataset_rank` = **8** / 10
- `borda_points` = **3**
- `runtime_seconds` = 7.69
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `fsrs` × `duolingo_hlr` — 排名 9 / 10

**Prediction：**

- `logLoss` = 0.422470
- `ici` = 0.216897
- `auc` = 0.536992
- `maeP` = 0.294273
- `prediction_raw` = 0.711535  →  `prediction_score` = 0.878154

**DHP：**

- `expectedMemoryFinal` = 8568.5311
- `masteredCount` = 301
- `totalReviews` = 330882
- `efficiency` = 0.025896
- `dhp_raw` = 288.3880  →  `dhp_score` = 0.061214

**Policy：**

- `finalRecallRate` = 0.231800
- `reviewsPerDay` = 3676.4667
- `retentionStability` = 0.783154
- `policy_raw` = 0.707754  →  `policy_score` = 0.574512

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.531497**
- `dataset_rank` = **9** / 10
- `borda_points` = **2**
- `runtime_seconds` = 7.63
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `hlr` × `duolingo_hlr` — 排名 10 / 10

**Prediction：**

- `logLoss` = 4.164875
- `ici` = 0.879993
- `auc` = 0.527324
- `maeP` = 0.886714
- `prediction_raw` = 0.194199  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 12342.0252
- `masteredCount` = 0
- `totalReviews` = 994810
- `efficiency` = 0.012406
- `dhp_raw` = 37.2180  →  `dhp_score` = 0.007725

**Policy：**

- `finalRecallRate` = 0.874700
- `reviewsPerDay` = 11053.4444
- `retentionStability` = 0.900028
- `policy_raw` = 0.450014  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.002704**
- `dataset_rank` = **10** / 10
- `borda_points` = **1**
- `runtime_seconds` = 7.60
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

---

## maimemo

### `fsrs45` × `maimemo` — 排名 1 / 10

**Prediction：**

- `logLoss` = 0.323145
- `ici` = 0.048751
- `auc` = 0.781663
- `maeP` = 0.156359
- `prediction_raw` = 0.855244  →  `prediction_score` = 0.974714

**DHP：**

- `expectedMemoryFinal` = 11747.6113
- `masteredCount` = 15855
- `totalReviews` = 72941
- `efficiency` = 0.161056
- `dhp_raw` = 11581.6680  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.453800
- `reviewsPerDay` = 810.4556
- `retentionStability` = 0.864010
- `policy_raw` = 0.891482  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.988621**
- `dataset_rank` = **1** / 10
- `borda_points` = **10**
- `runtime_seconds` = 15.58
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `dhp` × `maimemo` — 排名 2 / 10

**Prediction：**

- `logLoss` = 0.315765
- `ici` = 0.060308
- `auc` = 0.813262
- `maeP` = 0.221782
- `prediction_raw` = 0.862733  →  `prediction_score` = 0.984764

**DHP：**

- `expectedMemoryFinal` = 20336.5682
- `masteredCount` = 15374
- `totalReviews` = 192214
- `efficiency` = 0.105802
- `dhp_raw` = 11079.2060  →  `dhp_score` = 0.956476

**Policy：**

- `finalRecallRate` = 0.755600
- `reviewsPerDay` = 2135.7111
- `retentionStability` = 0.918212
- `policy_raw` = 0.852320  →  `policy_score` = 0.911623

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.960235**
- `dataset_rank` = **2** / 10
- `borda_points` = **9**
- `runtime_seconds` = 15.62
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `amas` × `maimemo` — 排名 3 / 10

**Prediction：**

- `logLoss` = 0.365884
- `ici` = 0.109137
- `auc` = 0.770556
- `maeP` = 0.270067
- `prediction_raw` = 0.825249  →  `prediction_score` = 0.934458

**DHP：**

- `expectedMemoryFinal` = 20473.7192
- `masteredCount` = 14787
- `totalReviews` = 179656
- `efficiency` = 0.113961
- `dhp_raw` = 10692.7830  →  `dhp_score` = 0.923004

**Policy：**

- `finalRecallRate` = 0.550600
- `reviewsPerDay` = 1996.1778
- `retentionStability` = 0.899180
- `policy_raw` = 0.849781  →  `policy_score` = 0.905893

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.924736**
- `dataset_rank` = **3** / 10
- `borda_points` = **8**
- `runtime_seconds` = 16.14
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `amas6` × `maimemo` — 排名 4 / 10

**Prediction：**

- `logLoss` = 0.302643
- `ici` = 0.025327
- `auc` = 0.807348
- `maeP` = 0.175941
- `prediction_raw` = 0.874078  →  `prediction_score` = 0.999989

**DHP：**

- `expectedMemoryFinal` = 15837.4887
- `masteredCount` = 11568
- `totalReviews` = 113313
- `efficiency` = 0.139768
- `dhp_raw` = 8516.9040  →  `dhp_score` = 0.734526

**Policy：**

- `finalRecallRate` = 0.418200
- `reviewsPerDay` = 1259.0333
- `retentionStability` = 0.904833
- `policy_raw` = 0.889465  →  `policy_score` = 0.995448

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.906169**
- `dataset_rank` = **4** / 10
- `borda_points` = **7**
- `runtime_seconds` = 15.74
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `fsrs` × `maimemo` — 排名 5 / 10

**Prediction：**

- `logLoss` = 0.365862
- `ici` = 0.109187
- `auc` = 0.770497
- `maeP` = 0.270056
- `prediction_raw` = 0.825221  →  `prediction_score` = 0.934421

**DHP：**

- `expectedMemoryFinal` = 21008.3659
- `masteredCount` = 13543
- `totalReviews` = 183469
- `efficiency` = 0.114506
- `dhp_raw` = 9823.6180  →  `dhp_score` = 0.847716

**Policy：**

- `finalRecallRate` = 0.567100
- `reviewsPerDay` = 2038.5444
- `retentionStability` = 0.900449
- `policy_raw` = 0.848297  →  `policy_score` = 0.902544

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.897699**
- `dataset_rank` = **5** / 10
- `borda_points` = **6**
- `runtime_seconds` = 17.82
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `fsrs6` × `maimemo` — 排名 6 / 10

**Prediction：**

- `logLoss` = 0.302683
- `ici` = 0.025376
- `auc` = 0.807452
- `maeP` = 0.175960
- `prediction_raw` = 0.874086  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 16334.3710
- `masteredCount` = 9867
- `totalReviews` = 117951
- `efficiency` = 0.138484
- `dhp_raw` = 7322.3520  →  `dhp_score` = 0.631053

**Policy：**

- `finalRecallRate` = 0.350300
- `reviewsPerDay` = 1310.5667
- `retentionStability` = 0.902920
- `policy_raw` = 0.885932  →  `policy_score` = 0.987475

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.868363**
- `dataset_rank` = **6** / 10
- `borda_points` = **5**
- `runtime_seconds` = 15.87
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `sm2` × `maimemo` — 排名 7 / 10

**Prediction：**

- `logLoss` = 0.386886
- `ici` = 0.097024
- `auc` = 0.587498
- `maeP` = 0.211818
- `prediction_raw` = 0.769765  →  `prediction_score` = 0.859997

**DHP：**

- `expectedMemoryFinal` = 21021.2787
- `masteredCount` = 12219
- `totalReviews` = 243158
- `efficiency` = 0.086451
- `dhp_raw` = 8812.6530  →  `dhp_score` = 0.760145

**Policy：**

- `finalRecallRate` = 0.868500
- `reviewsPerDay` = 2701.7556
- `retentionStability` = 0.900847
- `policy_raw` = 0.815336  →  `policy_score` = 0.828159

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.818681**
- `dataset_rank` = **7** / 10
- `borda_points` = **4**
- `runtime_seconds` = 15.44
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `leitner` × `maimemo` — 排名 8 / 10

**Prediction：**

- `logLoss` = 0.463345
- `ici` = 0.171991
- `auc` = 0.293893
- `maeP` = 0.305327
- `prediction_raw` = 0.643902  →  `prediction_score` = 0.691083

**DHP：**

- `expectedMemoryFinal` = 25387.2046
- `masteredCount` = 0
- `totalReviews` = 355729
- `efficiency` = 0.071367
- `dhp_raw` = 214.1010  →  `dhp_score` = 0.015328

**Policy：**

- `finalRecallRate` = 0.892200
- `reviewsPerDay` = 3952.5444
- `retentionStability` = 0.896646
- `policy_raw` = 0.750696  →  `policy_score` = 0.682286

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.452809**
- `dataset_rank` = **8** / 10
- `borda_points` = **3**
- `runtime_seconds` = 15.44
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `random` × `maimemo` — 排名 9 / 10

**Prediction：**

- `logLoss` = 1.179584
- `ici` = 0.423863
- `auc` = 0.223966
- `maeP` = 0.491799
- `prediction_raw` = 0.404114  →  `prediction_score` = 0.369278

**DHP：**

- `expectedMemoryFinal` = 18494.4597
- `masteredCount` = 0
- `totalReviews` = 200760
- `efficiency` = 0.092122
- `dhp_raw` = 276.3660  →  `dhp_score` = 0.020721

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 2230.6667
- `retentionStability` = 0.830193
- `policy_raw` = 0.803563  →  `policy_score` = 0.801592

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.333746**
- `dataset_rank` = **9** / 10
- `borda_points` = **2**
- `runtime_seconds` = 15.55
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `hlr` × `maimemo` — 排名 10 / 10

**Prediction：**

- `logLoss` = 10.177668
- `ici` = 0.872459
- `auc` = 0.302302
- `maeP` = 0.874389
- `prediction_raw` = 0.128953  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 28493.0247
- `masteredCount` = 0
- `totalReviews` = 2300843
- `efficiency` = 0.012384
- `dhp_raw` = 37.1520  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.970800
- `reviewsPerDay` = 25564.9222
- `retentionStability` = 0.896720
- `policy_raw` = 0.448360  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.000000**
- `dataset_rank` = **10** / 10
- `borda_points` = **1**
- `runtime_seconds` = 15.45
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

---

## synthetic

### `fsrs45` × `synthetic` — 排名 1 / 10

**Prediction：**

- `logLoss` = 0.710241
- `ici` = 0.224132
- `auc` = 0.554540
- `maeP` = 0.386900
- `prediction_raw` = 0.657074  →  `prediction_score` = 0.872072

**DHP：**

- `expectedMemoryFinal` = 7587.6593
- `masteredCount` = 31740
- `totalReviews` = 239374
- `efficiency` = 0.031698
- `dhp_raw` = 22313.0940  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.280500
- `reviewsPerDay` = 2659.7111
- `retentionStability` = 0.914132
- `policy_raw` = 0.824081  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.942432**
- `dataset_rank` = **1** / 10
- `borda_points` = **10**
- `runtime_seconds` = 11.44
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `amas6` × `synthetic` — 排名 2 / 10

**Prediction：**

- `logLoss` = 0.508915
- `ici` = 0.211180
- `auc` = 0.630567
- `maeP` = 0.375861
- `prediction_raw` = 0.724033  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 18029.2602
- `masteredCount` = 26544
- `totalReviews` = 335345
- `efficiency` = 0.053763
- `dhp_raw` = 18742.0890  →  `dhp_score` = 0.839368

**Policy：**

- `finalRecallRate` = 0.472200
- `reviewsPerDay` = 3726.0556
- `retentionStability` = 0.911017
- `policy_raw` = 0.769206  →  `policy_score` = 0.852731

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.914325**
- `dataset_rank` = **2** / 10
- `borda_points` = **9**
- `runtime_seconds` = 11.61
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `fsrs6` × `synthetic` — 排名 3 / 10

**Prediction：**

- `logLoss` = 0.508915
- `ici` = 0.211180
- `auc` = 0.630567
- `maeP` = 0.375861
- `prediction_raw` = 0.724033  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 21659.1838
- `masteredCount` = 22102
- `totalReviews` = 345102
- `efficiency` = 0.062762
- `dhp_raw` = 15659.6860  →  `dhp_score` = 0.700715

**Policy：**

- `finalRecallRate` = 0.340100
- `reviewsPerDay` = 3834.4667
- `retentionStability` = 0.907325
- `policy_raw` = 0.761939  →  `policy_score` = 0.833229

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.861896**
- `dataset_rank` = **3** / 10
- `borda_points` = **8**
- `runtime_seconds` = 11.45
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `amas` × `synthetic` — 排名 4 / 10

**Prediction：**

- `logLoss` = 0.728252
- `ici` = 0.339993
- `auc` = 0.478362
- `maeP` = 0.486077
- `prediction_raw` = 0.595860  →  `prediction_score` = 0.755118

**DHP：**

- `expectedMemoryFinal` = 28253.0181
- `masteredCount` = 5102
- `totalReviews` = 552107
- `efficiency` = 0.051173
- `dhp_raw` = 3724.9190  →  `dhp_score` = 0.163863

**Policy：**

- `finalRecallRate` = 0.402600
- `reviewsPerDay` = 6134.5222
- `retentionStability` = 0.851306
- `policy_raw` = 0.618927  →  `policy_score` = 0.449422

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.487040**
- `dataset_rank` = **4** / 10
- `borda_points` = **7**
- `runtime_seconds` = 11.62
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `fsrs` × `synthetic` — 排名 5 / 10

**Prediction：**

- `logLoss` = 0.728252
- `ici` = 0.339993
- `auc` = 0.478362
- `maeP` = 0.486077
- `prediction_raw` = 0.595860  →  `prediction_score` = 0.755118

**DHP：**

- `expectedMemoryFinal` = 29387.1283
- `masteredCount` = 4708
- `totalReviews` = 571825
- `efficiency` = 0.051392
- `dhp_raw` = 3449.7760  →  `dhp_score` = 0.151487

**Policy：**

- `finalRecallRate` = 0.346500
- `reviewsPerDay` = 6353.6111
- `retentionStability` = 0.858685
- `policy_raw` = 0.611662  →  `policy_score` = 0.429925

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.478809**
- `dataset_rank` = **5** / 10
- `borda_points` = **6**
- `runtime_seconds` = 11.47
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `sm2` × `synthetic` — 排名 6 / 10

**Prediction：**

- `logLoss` = 1.223161
- `ici` = 0.430807
- `auc` = 0.503970
- `maeP` = 0.535939
- `prediction_raw` = 0.477317  →  `prediction_score` = 0.528633

**DHP：**

- `expectedMemoryFinal` = 31263.2810
- `masteredCount` = 9824
- `totalReviews` = 577542
- `efficiency` = 0.054132
- `dhp_raw` = 7039.1960  →  `dhp_score` = 0.312947

**Policy：**

- `finalRecallRate` = 0.730700
- `reviewsPerDay` = 6417.1333
- `retentionStability` = 0.915510
- `policy_raw` = 0.636898  →  `policy_score` = 0.497652

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.446947**
- `dataset_rank` = **6** / 10
- `borda_points` = **5**
- `runtime_seconds` = 11.41
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `random` × `synthetic` — 排名 7 / 10

**Prediction：**

- `logLoss` = 1.065810
- `ici` = 0.398710
- `auc` = 0.507953
- `maeP` = 0.498653
- `prediction_raw` = 0.519611  →  `prediction_score` = 0.609440

**DHP：**

- `expectedMemoryFinal` = 11475.4044
- `masteredCount` = 0
- `totalReviews` = 297500
- `efficiency` = 0.038573
- `dhp_raw` = 115.7190  →  `dhp_score` = 0.001514

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 3305.5556
- `retentionStability` = 0.843411
- `policy_raw` = 0.756428  →  `policy_score` = 0.818437

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.438465**
- `dataset_rank` = **7** / 10
- `borda_points` = **4**
- `runtime_seconds` = 11.47
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `dhp` × `synthetic` — 排名 8 / 10

**Prediction：**

- `logLoss` = 2.328021
- `ici` = 0.592008
- `auc` = 0.631964
- `maeP` = 0.639759
- `prediction_raw` = 0.311987  →  `prediction_score` = 0.212761

**DHP：**

- `expectedMemoryFinal` = 27914.1134
- `masteredCount` = 17561
- `totalReviews` = 459674
- `efficiency` = 0.060726
- `dhp_raw` = 12474.8780  →  `dhp_score` = 0.557456

**Policy：**

- `finalRecallRate` = 0.667200
- `reviewsPerDay` = 5107.4889
- `retentionStability` = 0.915593
- `policy_raw` = 0.702422  →  `policy_score` = 0.673501

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.425552**
- `dataset_rank` = **8** / 10
- `borda_points` = **3**
- `runtime_seconds` = 11.41
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `leitner` × `synthetic` — 排名 9 / 10

**Prediction：**

- `logLoss` = 1.441303
- `ici` = 0.468880
- `auc` = 0.527717
- `maeP` = 0.571587
- `prediction_raw` = 0.429391  →  `prediction_score` = 0.437068

**DHP：**

- `expectedMemoryFinal` = 36758.2218
- `masteredCount` = 0
- `totalReviews` = 672766
- `efficiency` = 0.054637
- `dhp_raw` = 163.9110  →  `dhp_score` = 0.003681

**Policy：**

- `finalRecallRate` = 0.782100
- `reviewsPerDay` = 7475.1778
- `retentionStability` = 0.910600
- `policy_raw` = 0.581541  →  `policy_score` = 0.349089

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.267787**
- `dataset_rank` = **9** / 10
- `borda_points` = **2**
- `runtime_seconds` = 11.39
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

### `hlr` × `synthetic` — 排名 10 / 10

**Prediction：**

- `logLoss` = 11.334093
- `ici` = 0.858481
- `auc` = 0.527237
- `maeP` = 0.858484
- `prediction_raw` = 0.200627  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 41389.6648
- `masteredCount` = 0
- `totalReviews` = 1513000
- `efficiency` = 0.027356
- `dhp_raw` = 82.0680  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.890000
- `reviewsPerDay` = 16811.1111
- `retentionStability` = 0.902931
- `policy_raw` = 0.451465  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.000000**
- `dataset_rank` = **10** / 10
- `borda_points` = **1**
- `runtime_seconds` = 11.40
- `notes` = constrained re-tune winner under parity-aligned DHP mirror (repaired NM0), production write-back 2026-06-11

---
