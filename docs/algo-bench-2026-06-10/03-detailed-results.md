# 详细原始指标 2026-06-10

> 24 个 (algo, dataset) 组合的全量原始指标 + 归一化分数。  
> 由 `benchmarks/maimemo/leaderboard.py` 从 `benchmarks/results/2026-06-10/*.json` 生成。

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
- `runtime_seconds` = 6.88
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

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
- `runtime_seconds` = 6.80
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

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
- `runtime_seconds` = 6.81
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `fsrs` × `duolingo_hlr` — 排名 4 / 10

**Prediction：**

- `logLoss` = 0.302769
- `ici` = 0.116324
- `auc` = 0.538928
- `maeP` = 0.187668
- `prediction_raw` = 0.766228  →  `prediction_score` = 0.970993

**DHP：**

- `expectedMemoryFinal` = 5371.9917
- `masteredCount` = 3414
- `totalReviews` = 163910
- `efficiency` = 0.032774
- `dhp_raw` = 2488.1220  →  `dhp_score` = 0.529673

**Policy：**

- `finalRecallRate` = 0.213900
- `reviewsPerDay` = 1821.2222
- `retentionStability` = 0.817897
- `policy_raw` = 0.817887  →  `policy_score` = 0.820004

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.786333**
- `dataset_rank` = **4** / 10
- `borda_points` = **7**
- `runtime_seconds` = 6.84
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `amas` × `duolingo_hlr` — 排名 5 / 10

**Prediction：**

- `logLoss` = 0.302769
- `ici` = 0.116324
- `auc` = 0.538928
- `maeP` = 0.187668
- `prediction_raw` = 0.766228  →  `prediction_score` = 0.970993

**DHP：**

- `expectedMemoryFinal` = 4497.5746
- `masteredCount` = 3336
- `totalReviews` = 158430
- `efficiency` = 0.028388
- `dhp_raw` = 2420.3640  →  `dhp_score` = 0.515243

**Policy：**

- `finalRecallRate` = 0.165800
- `reviewsPerDay` = 1760.3333
- `retentionStability` = 0.815368
- `policy_raw` = 0.819667  →  `policy_score` = 0.823971

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.782076**
- `dataset_rank` = **5** / 10
- `borda_points` = **6**
- `runtime_seconds` = 6.87
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

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
- `runtime_seconds` = 6.84
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

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
- `runtime_seconds` = 6.80
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

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
- `runtime_seconds` = 6.79
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

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
- `runtime_seconds` = 6.78
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

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
- `runtime_seconds` = 6.79
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

---

## maimemo

### `fsrs45` × `maimemo` — 排名 1 / 10

**Prediction：**

- `logLoss` = 0.323133
- `ici` = 0.048762
- `auc` = 0.781697
- `maeP` = 0.156348
- `prediction_raw` = 0.855254  →  `prediction_score` = 0.974720

**DHP：**

- `expectedMemoryFinal` = 11732.4716
- `masteredCount` = 15840
- `totalReviews` = 72927
- `efficiency` = 0.160880
- `dhp_raw` = 11570.6400  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.457900
- `reviewsPerDay` = 810.3000
- `retentionStability` = 0.865589
- `policy_raw` = 0.892279  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.988624**
- `dataset_rank` = **1** / 10
- `borda_points` = **10**
- `runtime_seconds` = 15.80
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `amas` × `maimemo` — 排名 2 / 10

**Prediction：**

- `logLoss` = 0.308272
- `ici` = 0.024328
- `auc` = 0.788909
- `maeP` = 0.174886
- `prediction_raw` = 0.867720  →  `prediction_score` = 0.991450

**DHP：**

- `expectedMemoryFinal` = 13787.4932
- `masteredCount` = 15501
- `totalReviews` = 92335
- `efficiency` = 0.149320
- `dhp_raw` = 11298.6600  →  `dhp_score` = 0.976418

**Policy：**

- `finalRecallRate` = 0.527400
- `reviewsPerDay` = 1025.9444
- `retentionStability` = 0.880823
- `policy_raw` = 0.889114  →  `policy_score` = 0.992870

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.986473**
- `dataset_rank` = **2** / 10
- `borda_points` = **9**
- `runtime_seconds` = 16.08
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `dhp` × `maimemo` — 排名 3 / 10

**Prediction：**

- `logLoss` = 0.315797
- `ici` = 0.060322
- `auc` = 0.813217
- `maeP` = 0.221800
- `prediction_raw` = 0.862709  →  `prediction_score` = 0.984725

**DHP：**

- `expectedMemoryFinal` = 20248.9627
- `masteredCount` = 15433
- `totalReviews` = 192079
- `efficiency` = 0.105420
- `dhp_raw` = 11119.3600  →  `dhp_score` = 0.960872

**Policy：**

- `finalRecallRate` = 0.756200
- `reviewsPerDay` = 2134.2111
- `retentionStability` = 0.918225
- `policy_raw` = 0.852402  →  `policy_score` = 0.910170

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.961466**
- `dataset_rank` = **3** / 10
- `borda_points` = **8**
- `runtime_seconds` = 15.97
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `fsrs` × `maimemo` — 排名 4 / 10

**Prediction：**

- `logLoss` = 0.308201
- `ici` = 0.024299
- `auc` = 0.788880
- `maeP` = 0.174850
- `prediction_raw` = 0.867734  →  `prediction_score` = 0.991469

**DHP：**

- `expectedMemoryFinal` = 13088.4366
- `masteredCount` = 13780
- `totalReviews` = 93082
- `efficiency` = 0.140612
- `dhp_raw` = 10067.8360  →  `dhp_score` = 0.869701

**Policy：**

- `finalRecallRate` = 0.521700
- `reviewsPerDay` = 1034.2444
- `retentionStability` = 0.880513
- `policy_raw` = 0.888544  →  `policy_score` = 0.991586

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.948874**
- `dataset_rank` = **4** / 10
- `borda_points` = **7**
- `runtime_seconds` = 17.32
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `amas6` × `maimemo` — 排名 5 / 10

**Prediction：**

- `logLoss` = 0.302583
- `ici` = 0.025297
- `auc` = 0.807322
- `maeP` = 0.175909
- `prediction_raw` = 0.874091  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 15844.9556
- `masteredCount` = 11573
- `totalReviews` = 113074
- `efficiency` = 0.140129
- `dhp_raw` = 8521.4870  →  `dhp_score` = 0.735626

**Policy：**

- `finalRecallRate` = 0.416200
- `reviewsPerDay` = 1256.3778
- `retentionStability` = 0.904712
- `policy_raw` = 0.889537  →  `policy_score` = 0.993822

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.906234**
- `dataset_rank` = **5** / 10
- `borda_points` = **6**
- `runtime_seconds` = 15.51
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `fsrs6` × `maimemo` — 排名 6 / 10

**Prediction：**

- `logLoss` = 0.302656
- `ici` = 0.025327
- `auc` = 0.807335
- `maeP` = 0.175947
- `prediction_raw` = 0.874071  →  `prediction_score` = 0.999974

**DHP：**

- `expectedMemoryFinal` = 16361.2565
- `masteredCount` = 9901
- `totalReviews` = 117966
- `efficiency` = 0.138695
- `dhp_raw` = 7346.7850  →  `dhp_score` = 0.633775

**Policy：**

- `finalRecallRate` = 0.348900
- `reviewsPerDay` = 1310.7333
- `retentionStability` = 0.902286
- `policy_raw` = 0.885606  →  `policy_score` = 0.984968

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.868803**
- `dataset_rank` = **6** / 10
- `borda_points` = **5**
- `runtime_seconds` = 15.61
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `sm2` × `maimemo` — 排名 7 / 10

**Prediction：**

- `logLoss` = 0.386757
- `ici` = 0.096938
- `auc` = 0.587552
- `maeP` = 0.211762
- `prediction_raw` = 0.769833  →  `prediction_score` = 0.860083

**DHP：**

- `expectedMemoryFinal` = 21032.9051
- `masteredCount` = 12131
- `totalReviews` = 243987
- `efficiency` = 0.086205
- `dhp_raw` = 8750.3150  →  `dhp_score` = 0.755467

**Policy：**

- `finalRecallRate` = 0.869000
- `reviewsPerDay` = 2710.9667
- `retentionStability` = 0.901008
- `policy_raw` = 0.814956  →  `policy_score` = 0.825815

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.816614**
- `dataset_rank` = **7** / 10
- `borda_points` = **4**
- `runtime_seconds` = 15.24
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `leitner` × `maimemo` — 排名 8 / 10

**Prediction：**

- `logLoss` = 0.463602
- `ici` = 0.172139
- `auc` = 0.293704
- `maeP` = 0.305430
- `prediction_raw` = 0.643749  →  `prediction_score` = 0.690875

**DHP：**

- `expectedMemoryFinal` = 25387.1918
- `masteredCount` = 0
- `totalReviews` = 355369
- `efficiency` = 0.071439
- `dhp_raw` = 214.3170  →  `dhp_score` = 0.015361

**Policy：**

- `finalRecallRate` = 0.894300
- `reviewsPerDay` = 3948.5444
- `retentionStability` = 0.896654
- `policy_raw` = 0.750900  →  `policy_score` = 0.681519

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.452574**
- `dataset_rank` = **8** / 10
- `borda_points` = **3**
- `runtime_seconds` = 15.89
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `random` × `maimemo` — 排名 9 / 10

**Prediction：**

- `logLoss` = 1.179835
- `ici` = 0.423962
- `auc` = 0.223823
- `maeP` = 0.491883
- `prediction_raw` = 0.403991  →  `prediction_score` = 0.369114

**DHP：**

- `expectedMemoryFinal` = 18506.2690
- `masteredCount` = 0
- `totalReviews` = 200760
- `efficiency` = 0.092181
- `dhp_raw` = 276.5430  →  `dhp_score` = 0.020757

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 2230.6667
- `retentionStability` = 0.830207
- `policy_raw` = 0.803570  →  `policy_score` = 0.800168

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.333400**
- `dataset_rank` = **9** / 10
- `borda_points` = **2**
- `runtime_seconds` = 15.33
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `hlr` × `maimemo` — 排名 10 / 10

**Prediction：**

- `logLoss` = 10.177444
- `ici` = 0.872410
- `auc` = 0.302239
- `maeP` = 0.874340
- `prediction_raw` = 0.128949  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 28494.1867
- `masteredCount` = 0
- `totalReviews` = 2301340
- `efficiency` = 0.012382
- `dhp_raw` = 37.1460  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.970900
- `reviewsPerDay` = 25570.4444
- `retentionStability` = 0.896722
- `policy_raw` = 0.448361  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.000000**
- `dataset_rank` = **10** / 10
- `borda_points` = **1**
- `runtime_seconds` = 15.35
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

---

## synthetic

### `amas` × `synthetic` — 排名 1 / 10

**Prediction：**

- `logLoss` = 0.493051
- `ici` = 0.182487
- `auc` = 0.608120
- `maeP` = 0.358944
- `prediction_raw` = 0.729080  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 10715.7556
- `masteredCount` = 31781
- `totalReviews` = 272264
- `efficiency` = 0.039358
- `dhp_raw` = 22364.7740  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.453500
- `reviewsPerDay` = 3025.1556
- `retentionStability` = 0.903767
- `policy_raw` = 0.800626  →  `policy_score` = 0.937054

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.987411**
- `dataset_rank` = **1** / 10
- `borda_points` = **10**
- `runtime_seconds` = 10.10
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `fsrs` × `synthetic` — 排名 2 / 10

**Prediction：**

- `logLoss` = 0.493051
- `ici` = 0.182487
- `auc` = 0.608120
- `maeP` = 0.358944
- `prediction_raw` = 0.729080  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 10894.8240
- `masteredCount` = 31242
- `totalReviews` = 271657
- `efficiency` = 0.040105
- `dhp_raw` = 21989.7150  →  `dhp_score` = 0.983168

**Policy：**

- `finalRecallRate` = 0.471000
- `reviewsPerDay` = 3018.4111
- `retentionStability` = 0.905363
- `policy_raw` = 0.801761  →  `policy_score` = 0.940099

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.982129**
- `dataset_rank` = **2** / 10
- `borda_points` = **9**
- `runtime_seconds` = 9.96
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `fsrs45` × `synthetic` — 排名 3 / 10

**Prediction：**

- `logLoss` = 0.710241
- `ici` = 0.224132
- `auc` = 0.554540
- `maeP` = 0.386900
- `prediction_raw` = 0.657074  →  `prediction_score` = 0.863743

**DHP：**

- `expectedMemoryFinal` = 7587.6593
- `masteredCount` = 31740
- `totalReviews` = 239374
- `efficiency` = 0.031698
- `dhp_raw` = 22313.0940  →  `dhp_score` = 0.997681

**Policy：**

- `finalRecallRate` = 0.280500
- `reviewsPerDay` = 2659.7111
- `retentionStability` = 0.914132
- `policy_raw` = 0.824081  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.937873**
- `dataset_rank` = **3** / 10
- `borda_points` = **8**
- `runtime_seconds` = 9.95
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `amas6` × `synthetic` — 排名 4 / 10

**Prediction：**

- `logLoss` = 0.508915
- `ici` = 0.211180
- `auc` = 0.630567
- `maeP` = 0.375861
- `prediction_raw` = 0.724033  →  `prediction_score` = 0.990450

**DHP：**

- `expectedMemoryFinal` = 18029.2602
- `masteredCount` = 26544
- `totalReviews` = 335345
- `efficiency` = 0.053763
- `dhp_raw` = 18742.0890  →  `dhp_score` = 0.837422

**Policy：**

- `finalRecallRate` = 0.472200
- `reviewsPerDay` = 3726.0556
- `retentionStability` = 0.911017
- `policy_raw` = 0.769206  →  `policy_score` = 0.852731

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.909346**
- `dataset_rank` = **4** / 10
- `borda_points` = **7**
- `runtime_seconds` = 10.12
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `fsrs6` × `synthetic` — 排名 5 / 10

**Prediction：**

- `logLoss` = 0.508915
- `ici` = 0.211180
- `auc` = 0.630567
- `maeP` = 0.375861
- `prediction_raw` = 0.724033  →  `prediction_score` = 0.990450

**DHP：**

- `expectedMemoryFinal` = 21659.1838
- `masteredCount` = 22102
- `totalReviews` = 345102
- `efficiency` = 0.062762
- `dhp_raw` = 15659.6860  →  `dhp_score` = 0.699090

**Policy：**

- `finalRecallRate` = 0.340100
- `reviewsPerDay` = 3834.4667
- `retentionStability` = 0.907325
- `policy_raw` = 0.761939  →  `policy_score` = 0.833229

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.857030**
- `dataset_rank` = **5** / 10
- `borda_points` = **6**
- `runtime_seconds` = 9.97
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `sm2` × `synthetic` — 排名 6 / 10

**Prediction：**

- `logLoss` = 1.223161
- `ici` = 0.430807
- `auc` = 0.503970
- `maeP` = 0.535939
- `prediction_raw` = 0.477317  →  `prediction_score` = 0.523585

**DHP：**

- `expectedMemoryFinal` = 31263.2810
- `masteredCount` = 9824
- `totalReviews` = 577542
- `efficiency` = 0.054132
- `dhp_raw` = 7039.1960  →  `dhp_score` = 0.312221

**Policy：**

- `finalRecallRate` = 0.730700
- `reviewsPerDay` = 6417.1333
- `retentionStability` = 0.915510
- `policy_raw` = 0.636898  →  `policy_score` = 0.497652

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.444421**
- `dataset_rank` = **6** / 10
- `borda_points` = **5**
- `runtime_seconds` = 9.92
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `random` × `synthetic` — 排名 7 / 10

**Prediction：**

- `logLoss` = 1.065810
- `ici` = 0.398710
- `auc` = 0.507953
- `maeP` = 0.498653
- `prediction_raw` = 0.519611  →  `prediction_score` = 0.603619

**DHP：**

- `expectedMemoryFinal` = 11475.4044
- `masteredCount` = 0
- `totalReviews` = 297500
- `efficiency` = 0.038573
- `dhp_raw` = 115.7190  →  `dhp_score` = 0.001510

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 3305.5556
- `retentionStability` = 0.843411
- `policy_raw` = 0.756428  →  `policy_score` = 0.818437

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.435845**
- `dataset_rank` = **7** / 10
- `borda_points` = **4**
- `runtime_seconds` = 9.99
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `dhp` × `synthetic` — 排名 8 / 10

**Prediction：**

- `logLoss` = 2.328021
- `ici` = 0.592008
- `auc` = 0.631964
- `maeP` = 0.639759
- `prediction_raw` = 0.311987  →  `prediction_score` = 0.210729

**DHP：**

- `expectedMemoryFinal` = 27914.1134
- `masteredCount` = 17561
- `totalReviews` = 459674
- `efficiency` = 0.060726
- `dhp_raw` = 12474.8780  →  `dhp_score` = 0.556163

**Policy：**

- `finalRecallRate` = 0.667200
- `reviewsPerDay` = 5107.4889
- `retentionStability` = 0.915593
- `policy_raw` = 0.702422  →  `policy_score` = 0.673501

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.424185**
- `dataset_rank` = **8** / 10
- `borda_points` = **3**
- `runtime_seconds` = 9.92
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

### `leitner` × `synthetic` — 排名 9 / 10

**Prediction：**

- `logLoss` = 1.441303
- `ici` = 0.468880
- `auc` = 0.527717
- `maeP` = 0.571587
- `prediction_raw` = 0.429391  →  `prediction_score` = 0.432894

**DHP：**

- `expectedMemoryFinal` = 36758.2218
- `masteredCount` = 0
- `totalReviews` = 672766
- `efficiency` = 0.054637
- `dhp_raw` = 163.9110  →  `dhp_score` = 0.003673

**Policy：**

- `finalRecallRate` = 0.782100
- `reviewsPerDay` = 7475.1778
- `retentionStability` = 0.910600
- `policy_raw` = 0.581541  →  `policy_score` = 0.349089

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.265905**
- `dataset_rank` = **9** / 10
- `borda_points` = **2**
- `runtime_seconds` = 9.90
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

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
- `runtime_seconds` = 9.92
- `notes` = v1.0-fsrs6: AMAS 生产内核升级 FSRS-6 (21w+trainable decay) + tuned v3 决策层回写

---
