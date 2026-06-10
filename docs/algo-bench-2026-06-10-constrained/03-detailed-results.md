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
- `runtime_seconds` = 6.96
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

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
- `runtime_seconds` = 6.88
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

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
- `runtime_seconds` = 6.88
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `fsrs` × `duolingo_hlr` — 排名 4 / 10

**Prediction：**

- `logLoss` = 0.411377
- `ici` = 0.197090
- `auc` = 0.538065
- `maeP` = 0.255241
- `prediction_raw` = 0.720017  →  `prediction_score` = 0.892553

**DHP：**

- `expectedMemoryFinal` = 4048.5357
- `masteredCount` = 3423
- `totalReviews` = 147649
- `efficiency` = 0.027420
- `dhp_raw` = 2478.3600  →  `dhp_score` = 0.527594

**Policy：**

- `finalRecallRate` = 0.241700
- `reviewsPerDay` = 1640.5444
- `retentionStability` = 0.808194
- `policy_raw` = 0.822070  →  `policy_score` = 0.829326

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.752172**
- `dataset_rank` = **4** / 10
- `borda_points` = **7**
- `runtime_seconds` = 6.92
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `amas` × `duolingo_hlr` — 排名 5 / 10

**Prediction：**

- `logLoss` = 0.411377
- `ici` = 0.197090
- `auc` = 0.538065
- `maeP` = 0.255241
- `prediction_raw` = 0.720017  →  `prediction_score` = 0.892553

**DHP：**

- `expectedMemoryFinal` = 4288.2494
- `masteredCount` = 3302
- `totalReviews` = 140856
- `efficiency` = 0.030444
- `dhp_raw` = 2402.7320  →  `dhp_score` = 0.511488

**Policy：**

- `finalRecallRate` = 0.232800
- `reviewsPerDay` = 1565.0667
- `retentionStability` = 0.810293
- `policy_raw` = 0.826893  →  `policy_score` = 0.840078

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.748685**
- `dataset_rank` = **5** / 10
- `borda_points` = **6**
- `runtime_seconds` = 6.94
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `random` × `duolingo_hlr` — 排名 6 / 10

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
- `dataset_rank` = **6** / 10
- `borda_points` = **5**
- `runtime_seconds` = 6.91
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `sm2` × `duolingo_hlr` — 排名 7 / 10

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
- `dataset_rank` = **7** / 10
- `borda_points` = **4**
- `runtime_seconds` = 6.86
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `dhp` × `duolingo_hlr` — 排名 8 / 10

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
- `dataset_rank` = **8** / 10
- `borda_points` = **3**
- `runtime_seconds` = 6.86
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `leitner` × `duolingo_hlr` — 排名 9 / 10

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
- `dataset_rank` = **9** / 10
- `borda_points` = **2**
- `runtime_seconds` = 6.85
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

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
- `runtime_seconds` = 6.87
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

---

## maimemo

### `amas` × `maimemo` — 排名 1 / 10

**Prediction：**

- `logLoss` = 0.307168
- `ici` = 0.016370
- `auc` = 0.783053
- `maeP` = 0.184161
- `prediction_raw` = 0.868571  →  `prediction_score` = 0.992573

**DHP：**

- `expectedMemoryFinal` = 13827.8029
- `masteredCount` = 17376
- `totalReviews` = 96130
- `efficiency` = 0.143845
- `dhp_raw` = 12594.7350  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.460300
- `reviewsPerDay` = 1068.1111
- `retentionStability` = 0.870580
- `policy_raw` = 0.881884  →  `policy_score` = 0.978576

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.992373**
- `dataset_rank` = **1** / 10
- `borda_points` = **10**
- `runtime_seconds` = 15.86
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `fsrs45` × `maimemo` — 排名 2 / 10

**Prediction：**

- `logLoss` = 0.323099
- `ici` = 0.048712
- `auc` = 0.781524
- `maeP` = 0.156331
- `prediction_raw` = 0.855224  →  `prediction_score` = 0.974660

**DHP：**

- `expectedMemoryFinal` = 11738.6822
- `masteredCount` = 15907
- `totalReviews` = 72727
- `efficiency` = 0.161407
- `dhp_raw` = 11619.1210  →  `dhp_score` = 0.922309

**Policy：**

- `finalRecallRate` = 0.467700
- `reviewsPerDay` = 808.0778
- `retentionStability` = 0.863559
- `policy_raw` = 0.891375  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.961405**
- `dataset_rank` = **2** / 10
- `borda_points` = **9**
- `runtime_seconds` = 15.88
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `fsrs` × `maimemo` — 排名 3 / 10

**Prediction：**

- `logLoss` = 0.307157
- `ici` = 0.016468
- `auc` = 0.782814
- `maeP` = 0.184156
- `prediction_raw` = 0.868472  →  `prediction_score` = 0.992440

**DHP：**

- `expectedMemoryFinal` = 14461.7458
- `masteredCount` = 14894
- `totalReviews` = 99319
- `efficiency` = 0.145609
- `dhp_raw` = 10862.6270  →  `dhp_score` = 0.862067

**Policy：**

- `finalRecallRate` = 0.631900
- `reviewsPerDay` = 1103.5444
- `retentionStability` = 0.864941
- `policy_raw` = 0.877293  →  `policy_score` = 0.968212

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.941964**
- `dataset_rank` = **3** / 10
- `borda_points` = **8**
- `runtime_seconds` = 17.92
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `dhp` × `maimemo` — 排名 4 / 10

**Prediction：**

- `logLoss` = 0.315751
- `ici` = 0.060357
- `auc` = 0.813195
- `maeP` = 0.221775
- `prediction_raw` = 0.862701  →  `prediction_score` = 0.984695

**DHP：**

- `expectedMemoryFinal` = 20279.3671
- `masteredCount` = 15438
- `totalReviews` = 192432
- `efficiency` = 0.105385
- `dhp_raw` = 11122.7550  →  `dhp_score` = 0.882782

**Policy：**

- `finalRecallRate` = 0.751400
- `reviewsPerDay` = 2138.1333
- `retentionStability` = 0.918142
- `policy_raw` = 0.852164  →  `policy_score` = 0.911490

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.934384**
- `dataset_rank` = **4** / 10
- `borda_points` = **7**
- `runtime_seconds` = 15.64
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `amas6` × `maimemo` — 排名 5 / 10

**Prediction：**

- `logLoss` = 0.302710
- `ici` = 0.025425
- `auc` = 0.807568
- `maeP` = 0.175973
- `prediction_raw` = 0.874101  →  `prediction_score` = 0.999994

**DHP：**

- `expectedMemoryFinal` = 15804.9617
- `masteredCount` = 11566
- `totalReviews` = 113117
- `efficiency` = 0.139722
- `dhp_raw` = 8515.3660  →  `dhp_score` = 0.675147

**Policy：**

- `finalRecallRate` = 0.411700
- `reviewsPerDay` = 1256.8556
- `retentionStability` = 0.904809
- `policy_raw` = 0.889562  →  `policy_score` = 0.995906

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.885480**
- `dataset_rank` = **5** / 10
- `borda_points` = **6**
- `runtime_seconds` = 16.10
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `fsrs6` × `maimemo` — 排名 6 / 10

**Prediction：**

- `logLoss` = 0.302664
- `ici` = 0.025376
- `auc` = 0.807504
- `maeP` = 0.175953
- `prediction_raw` = 0.874105  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 16357.1678
- `masteredCount` = 9919
- `totalReviews` = 118134
- `efficiency` = 0.138463
- `dhp_raw` = 7358.6890  →  `dhp_score` = 0.583037

**Policy：**

- `finalRecallRate` = 0.357100
- `reviewsPerDay` = 1312.6000
- `retentionStability` = 0.902835
- `policy_raw` = 0.885787  →  `policy_score` = 0.987386

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.851540**
- `dataset_rank` = **6** / 10
- `borda_points` = **5**
- `runtime_seconds` = 15.89
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `sm2` × `maimemo` — 排名 7 / 10

**Prediction：**

- `logLoss` = 0.386660
- `ici` = 0.096907
- `auc` = 0.587528
- `maeP` = 0.211731
- `prediction_raw` = 0.769854  →  `prediction_score` = 0.860093

**DHP：**

- `expectedMemoryFinal` = 21039.5833
- `masteredCount` = 12146
- `totalReviews` = 243938
- `efficiency` = 0.086250
- `dhp_raw` = 8760.9500  →  `dhp_score` = 0.694704

**Policy：**

- `finalRecallRate` = 0.871700
- `reviewsPerDay` = 2710.4222
- `retentionStability` = 0.900623
- `policy_raw` = 0.814790  →  `policy_score` = 0.827127

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.795613**
- `dataset_rank` = **7** / 10
- `borda_points` = **4**
- `runtime_seconds` = 15.92
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `leitner` × `maimemo` — 排名 8 / 10

**Prediction：**

- `logLoss` = 0.463516
- `ici` = 0.172090
- `auc` = 0.293767
- `maeP` = 0.305396
- `prediction_raw` = 0.643800  →  `prediction_score` = 0.690925

**DHP：**

- `expectedMemoryFinal` = 25389.5461
- `masteredCount` = 0
- `totalReviews` = 355167
- `efficiency` = 0.071486
- `dhp_raw` = 214.4580  →  `dhp_score` = 0.014120

**Policy：**

- `finalRecallRate` = 0.892500
- `reviewsPerDay` = 3946.3000
- `retentionStability` = 0.896638
- `policy_raw` = 0.751004  →  `policy_score` = 0.683143

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.452487**
- `dataset_rank` = **8** / 10
- `borda_points` = **3**
- `runtime_seconds` = 15.65
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `random` × `maimemo` — 排名 9 / 10

**Prediction：**

- `logLoss` = 1.179465
- `ici` = 0.423828
- `auc` = 0.224014
- `maeP` = 0.491763
- `prediction_raw` = 0.404163  →  `prediction_score` = 0.369328

**DHP：**

- `expectedMemoryFinal` = 18503.6287
- `masteredCount` = 0
- `totalReviews` = 200760
- `efficiency` = 0.092168
- `dhp_raw` = 276.5040  →  `dhp_score` = 0.019061

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 2230.6667
- `retentionStability` = 0.830253
- `policy_raw` = 0.803593  →  `policy_score` = 0.801851

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.333239**
- `dataset_rank` = **9** / 10
- `borda_points` = **2**
- `runtime_seconds` = 15.77
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `hlr` × `maimemo` — 排名 10 / 10

**Prediction：**

- `logLoss` = 10.178008
- `ici` = 0.872508
- `auc` = 0.302373
- `maeP` = 0.874438
- `prediction_raw` = 0.128960  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 28493.8847
- `masteredCount` = 0
- `totalReviews` = 2301152
- `efficiency` = 0.012382
- `dhp_raw` = 37.1460  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.970900
- `reviewsPerDay` = 25568.3556
- `retentionStability` = 0.896726
- `policy_raw` = 0.448363  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.000000**
- `dataset_rank` = **10** / 10
- `borda_points` = **1**
- `runtime_seconds` = 15.75
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

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
- `runtime_seconds` = 10.30
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `amas` × `synthetic` — 排名 2 / 10

**Prediction：**

- `logLoss` = 0.627876
- `ici` = 0.250646
- `auc` = 0.570419
- `maeP` = 0.412656
- `prediction_raw` = 0.670357  →  `prediction_score` = 0.897448

**DHP：**

- `expectedMemoryFinal` = 11106.8564
- `masteredCount` = 30634
- `totalReviews` = 269890
- `efficiency` = 0.041153
- `dhp_raw` = 21567.2590  →  `dhp_score` = 0.966451

**Policy：**

- `finalRecallRate` = 0.492900
- `reviewsPerDay` = 2998.7778
- `retentionStability` = 0.907264
- `policy_raw` = 0.803693  →  `policy_score` = 0.945285

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.931167**
- `dataset_rank` = **2** / 10
- `borda_points` = **9**
- `runtime_seconds` = 10.45
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `fsrs` × `synthetic` — 排名 3 / 10

**Prediction：**

- `logLoss` = 0.627876
- `ici` = 0.250646
- `auc` = 0.570419
- `maeP` = 0.412656
- `prediction_raw` = 0.670357  →  `prediction_score` = 0.897448

**DHP：**

- `expectedMemoryFinal` = 10720.3639
- `masteredCount` = 29709
- `totalReviews` = 272445
- `efficiency` = 0.039349
- `dhp_raw` = 20914.3470  →  `dhp_score` = 0.937081

**Policy：**

- `finalRecallRate` = 0.472600
- `reviewsPerDay` = 3027.1667
- `retentionStability` = 0.908139
- `policy_raw` = 0.802711  →  `policy_score` = 0.942650

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.920360**
- `dataset_rank` = **3** / 10
- `borda_points` = **8**
- `runtime_seconds` = 10.34
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `amas6` × `synthetic` — 排名 4 / 10

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
- `dataset_rank` = **4** / 10
- `borda_points` = **7**
- `runtime_seconds` = 10.47
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

### `fsrs6` × `synthetic` — 排名 5 / 10

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
- `dataset_rank` = **5** / 10
- `borda_points` = **6**
- `runtime_seconds` = 10.32
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

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
- `runtime_seconds` = 10.27
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

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
- `runtime_seconds` = 10.34
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

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
- `runtime_seconds` = 10.27
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

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
- `runtime_seconds` = 10.25
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

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
- `runtime_seconds` = 10.27
- `notes` = constrained-tune winner (repaired NM2), production write-back 2026-06-10

---
