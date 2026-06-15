# 详细原始指标 2026-05-26

> 24 个 (algo, dataset) 组合的全量原始指标 + 归一化分数。  
> 由 `benchmarks/maimemo/leaderboard.py` 从 `benchmarks/results/2026-05-26/*.json` 生成。

## duolingo_hlr

### `leitner` × `duolingo_hlr` — 排名 1 / 8

**Prediction：**

- `logLoss` = 0.314299
- `ici` = 0.111311
- `auc` = 0.541405
- `maeP` = 0.197483
- `prediction_raw` = 0.766168  →  `prediction_score` = 0.969930

**DHP：**

- `expectedMemoryFinal` = 11238.4212
- `masteredCount` = 0
- `totalReviews` = 162031
- `efficiency` = 0.069360
- `dhp_raw` = 5653.8906  →  `dhp_score` = 0.996918

**Policy：**

- `finalRecallRate` = 0.912200
- `reviewsPerDay` = 1800.3444
- `retentionStability` = 0.956462
- `policy_raw` = 0.888214  →  `policy_score` = 0.671489

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.919688**
- `dataset_rank` = **1** / 8
- `borda_points` = **8**
- `runtime_seconds` = 14.66

### `sm2` × `duolingo_hlr` — 排名 2 / 8

**Prediction：**

- `logLoss` = 0.291132
- `ici` = 0.084821
- `auc` = 0.540387
- `maeP` = 0.166071
- `prediction_raw` = 0.778443  →  `prediction_score` = 0.991455

**DHP：**

- `expectedMemoryFinal` = 8752.0663
- `masteredCount` = 6247
- `totalReviews` = 108378
- `efficiency` = 0.080755
- `dhp_raw` = 4416.4106  →  `dhp_score` = 0.569961

**Policy：**

- `finalRecallRate` = 0.804400
- `reviewsPerDay` = 1204.2000
- `retentionStability` = 0.944763
- `policy_raw` = 0.912172  →  `policy_score` = 0.931303

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.831902**
- `dataset_rank` = **2** / 8
- `borda_points` = **7**
- `runtime_seconds` = 14.66

### `dhp` × `duolingo_hlr` — 排名 3 / 8

**Prediction：**

- `logLoss` = 0.422126
- `ici` = 0.169570
- `auc` = 0.538558
- `maeP` = 0.239823
- `prediction_raw` = 0.726271  →  `prediction_score` = 0.899967

**DHP：**

- `expectedMemoryFinal` = 8562.9221
- `masteredCount` = 7295
- `totalReviews` = 87531
- `efficiency` = 0.097827
- `dhp_raw` = 4330.3745  →  `dhp_score` = 0.540277

**Policy：**

- `finalRecallRate` = 0.784300
- `reviewsPerDay` = 972.5667
- `retentionStability` = 0.934269
- `policy_raw` = 0.918506  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.794082**
- `dataset_rank` = **3** / 8
- `borda_points` = **6**
- `runtime_seconds` = 14.66

### `amas` × `duolingo_hlr` — 排名 4 / 8

**Prediction：**

- `logLoss` = 0.422241
- `ici` = 0.202638
- `auc` = 0.540354
- `maeP` = 0.274010
- `prediction_raw` = 0.716866  →  `prediction_score` = 0.883475

**DHP：**

- `expectedMemoryFinal` = 8162.9205
- `masteredCount` = 8329
- `totalReviews` = 87756
- `efficiency` = 0.093018
- `dhp_raw` = 4127.9693  →  `dhp_score` = 0.470442

**Policy：**

- `finalRecallRate` = 0.794400
- `reviewsPerDay` = 975.0667
- `retentionStability` = 0.915579
- `policy_raw` = 0.909036  →  `policy_score` = 0.897301

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.741679**
- `dataset_rank` = **4** / 8
- `borda_points` = **5**
- `runtime_seconds` = 14.66

### `fsrs` × `duolingo_hlr` — 排名 5 / 8

**Prediction：**

- `logLoss` = 0.422241
- `ici` = 0.202638
- `auc` = 0.540354
- `maeP` = 0.274010
- `prediction_raw` = 0.716866  →  `prediction_score` = 0.883475

**DHP：**

- `expectedMemoryFinal` = 7781.7099
- `masteredCount` = 8707
- `totalReviews` = 88380
- `efficiency` = 0.088048
- `dhp_raw` = 3934.8789  →  `dhp_score` = 0.403822

**Policy：**

- `finalRecallRate` = 0.693500
- `reviewsPerDay` = 982.0000
- `retentionStability` = 0.920824
- `policy_raw` = 0.911312  →  `policy_score` = 0.921982

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.723298**
- `dataset_rank` = **5** / 8
- `borda_points` = **4**
- `runtime_seconds` = 14.66

### `random` × `duolingo_hlr` — 排名 6 / 8

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
- `dhp_raw` = 3824.3898  →  `dhp_score` = 0.365701

**Policy：**

- `finalRecallRate` = 0.818400
- `reviewsPerDay` = 977.0444
- `retentionStability` = 0.858785
- `policy_raw` = 0.880540  →  `policy_score` = 0.588272

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.695650**
- `dataset_rank` = **6** / 8
- `borda_points` = **3**
- `runtime_seconds` = 14.66

### `fsrs45` × `duolingo_hlr` — 排名 7 / 8

**Prediction：**

- `logLoss` = 0.349752
- `ici` = 0.138136
- `auc` = 0.536638
- `maeP` = 0.192721
- `prediction_raw` = 0.749600  →  `prediction_score` = 0.940877

**DHP：**

- `expectedMemoryFinal` = 5427.1279
- `masteredCount` = 10945
- `totalReviews` = 53323
- `efficiency` = 0.101778
- `dhp_raw` = 2764.4530  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.591400
- `reviewsPerDay` = 592.4778
- `retentionStability` = 0.855045
- `policy_raw` = 0.897899  →  `policy_score` = 0.776518

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.578698**
- `dataset_rank` = **7** / 8
- `borda_points` = **2**
- `runtime_seconds` = 14.66

### `hlr` × `duolingo_hlr` — 排名 8 / 8

**Prediction：**

- `logLoss` = 4.545442
- `ici` = 0.834321
- `auc` = 0.544506
- `maeP` = 0.843790
- `prediction_raw` = 0.213055  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 11284.3272
- `masteredCount` = 26
- `totalReviews` = 273105
- `efficiency` = 0.041319
- `dhp_raw` = 5662.8231  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.828400
- `reviewsPerDay` = 3034.5000
- `retentionStability` = 0.956040
- `policy_raw` = 0.826295  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.350000**
- `dataset_rank` = **8** / 8
- `borda_points` = **1**
- `runtime_seconds` = 14.66

---

## maimemo

### `dhp` × `maimemo` — 排名 1 / 8

**Prediction：**

- `logLoss` = 0.315768
- `ici` = 0.060308
- `auc` = 0.813259
- `maeP` = 0.221783
- `prediction_raw` = 0.862732  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 20358.7651
- `masteredCount` = 15287
- `totalReviews` = 192003
- `efficiency` = 0.106034
- `dhp_raw` = 10232.3996  →  `dhp_score` = 0.505679

**Policy：**

- `finalRecallRate` = 0.751200
- `reviewsPerDay` = 2133.3667
- `retentionStability` = 0.973677
- `policy_raw` = 0.880170  →  `policy_score` = 0.975756

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.822139**
- `dataset_rank` = **1** / 8
- `borda_points` = **8**
- `runtime_seconds` = 50.09

### `leitner` × `maimemo` — 排名 2 / 8

**Prediction：**

- `logLoss` = 0.463606
- `ici` = 0.172157
- `auc` = 0.293677
- `maeP` = 0.305434
- `prediction_raw` = 0.643735  →  `prediction_score` = 0.683664

**DHP：**

- `expectedMemoryFinal` = 25388.9408
- `masteredCount` = 0
- `totalReviews` = 355222
- `efficiency` = 0.071473
- `dhp_raw` = 12730.2069  →  `dhp_score` = 0.930849

**Policy：**

- `finalRecallRate` = 0.897300
- `reviewsPerDay` = 3946.9111
- `retentionStability` = 0.948600
- `policy_raw` = 0.776954  →  `policy_score` = 0.651522

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.763751**
- `dataset_rank` = **2** / 8
- `borda_points` = **7**
- `runtime_seconds` = 50.09

### `sm2` × `maimemo` — 排名 3 / 8

**Prediction：**

- `logLoss` = 0.386778
- `ici` = 0.096975
- `auc` = 0.587500
- `maeP` = 0.211779
- `prediction_raw` = 0.769802  →  `prediction_score` = 0.865765

**DHP：**

- `expectedMemoryFinal` = 21026.8575
- `masteredCount` = 12084
- `totalReviews` = 243977
- `efficiency` = 0.086184
- `dhp_raw` = 10556.5207  →  `dhp_score` = 0.560850

**Policy：**

- `finalRecallRate` = 0.867800
- `reviewsPerDay` = 2710.8556
- `retentionStability` = 0.948901
- `policy_raw` = 0.838908  →  `policy_score` = 0.846137

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.755119**
- `dataset_rank` = **3** / 8
- `borda_points` = **6**
- `runtime_seconds` = 50.09

### `amas` × `maimemo` — 排名 4 / 8

**Prediction：**

- `logLoss` = 0.329034
- `ici` = 0.070355
- `auc` = 0.789968
- `maeP` = 0.227377
- `prediction_raw` = 0.850077  →  `prediction_score` = 0.981721

**DHP：**

- `expectedMemoryFinal` = 17571.6687
- `masteredCount` = 17353
- `totalReviews` = 174551
- `efficiency` = 0.100668
- `dhp_raw` = 8836.1683  →  `dhp_score` = 0.268017

**Policy：**

- `finalRecallRate` = 0.752600
- `reviewsPerDay` = 1939.4556
- `retentionStability` = 0.945830
- `policy_raw` = 0.875942  →  `policy_score` = 0.962474

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.728075**
- `dataset_rank` = **4** / 8
- `borda_points` = **5**
- `runtime_seconds` = 50.09

### `fsrs` × `maimemo` — 排名 5 / 8

**Prediction：**

- `logLoss` = 0.329034
- `ici` = 0.070355
- `auc` = 0.789968
- `maeP` = 0.227377
- `prediction_raw` = 0.850077  →  `prediction_score` = 0.981721

**DHP：**

- `expectedMemoryFinal` = 17536.1614
- `masteredCount` = 18129
- `totalReviews` = 175334
- `efficiency` = 0.100016
- `dhp_raw` = 8818.0887  →  `dhp_score` = 0.264940

**Policy：**

- `finalRecallRate` = 0.707000
- `reviewsPerDay` = 1948.1556
- `retentionStability` = 0.937828
- `policy_raw` = 0.871506  →  `policy_score` = 0.948539

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.724211**
- `dataset_rank` = **5** / 8
- `borda_points` = **4**
- `runtime_seconds` = 50.09

### `fsrs45` × `maimemo` — 排名 6 / 8

**Prediction：**

- `logLoss` = 0.323191
- `ici` = 0.048821
- `auc` = 0.781786
- `maeP` = 0.156368
- `prediction_raw` = 0.855251  →  `prediction_score` = 0.989194

**DHP：**

- `expectedMemoryFinal` = 14377.5370
- `masteredCount` = 18399
- `totalReviews` = 98693
- `efficiency` = 0.145679
- `dhp_raw` = 7261.6080  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.483400
- `reviewsPerDay` = 1096.5889
- `retentionStability` = 0.885435
- `policy_raw` = 0.887888  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.645138**
- `dataset_rank` = **6** / 8
- `borda_points` = **3**
- `runtime_seconds` = 50.09

### `random` × `maimemo` — 排名 7 / 8

**Prediction：**

- `logLoss` = 1.179591
- `ici` = 0.423877
- `auc` = 0.223943
- `maeP` = 0.491806
- `prediction_raw` = 0.404101  →  `prediction_score` = 0.337521

**DHP：**

- `expectedMemoryFinal` = 18513.7761
- `masteredCount` = 0
- `totalReviews` = 200760
- `efficiency` = 0.092218
- `dhp_raw` = 9302.9970  →  `dhp_score` = 0.347479

**Policy：**

- `finalRecallRate` = 0.843000
- `reviewsPerDay` = 2230.6667
- `retentionStability` = 0.873110
- `policy_raw` = 0.825022  →  `policy_score` = 0.802517

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.434005**
- `dataset_rank` = **7** / 8
- `borda_points` = **2**
- `runtime_seconds` = 50.09

### `hlr` × `maimemo` — 排名 8 / 8

**Prediction：**

- `logLoss` = 10.241212
- `ici` = 0.874820
- `auc` = 0.442947
- `maeP` = 0.875314
- `prediction_raw` = 0.170438  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 26237.3845
- `masteredCount` = 0
- `totalReviews` = 738320
- `efficiency` = 0.035537
- `dhp_raw` = 13136.4608  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.836000
- `reviewsPerDay` = 8203.5556
- `retentionStability` = 0.959456
- `policy_raw` = 0.569550  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.350000**
- `dataset_rank` = **8** / 8
- `borda_points` = **1**
- `runtime_seconds` = 50.09

---

## synthetic

### `amas` × `synthetic` — 排名 1 / 8

**Prediction：**

- `logLoss` = 0.935562
- `ici` = 0.459007
- `auc` = 0.648181
- `maeP` = 0.545352
- `prediction_raw` = 0.569640  →  `prediction_score` = 0.807809

**DHP：**

- `expectedMemoryFinal` = 26106.7608
- `masteredCount` = 23773
- `totalReviews` = 400768
- `efficiency` = 0.065142
- `dhp_raw` = 13085.9514  →  `dhp_score` = 0.423953

**Policy：**

- `finalRecallRate` = 0.639900
- `reviewsPerDay` = 4452.9778
- `retentionStability` = 0.959118
- `policy_raw` = 0.756910  →  `policy_score` = 0.875346

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.686967**
- `dataset_rank` = **1** / 8
- `borda_points` = **8**
- `runtime_seconds` = 82.10

### `fsrs` × `synthetic` — 排名 2 / 8

**Prediction：**

- `logLoss` = 0.935562
- `ici` = 0.459007
- `auc` = 0.648181
- `maeP` = 0.545352
- `prediction_raw` = 0.569640  →  `prediction_score` = 0.807809

**DHP：**

- `expectedMemoryFinal` = 26307.4035
- `masteredCount` = 25000
- `totalReviews` = 409345
- `efficiency` = 0.064267
- `dhp_raw` = 13185.8353  →  `dhp_score` = 0.432245

**Policy：**

- `finalRecallRate` = 0.729200
- `reviewsPerDay` = 4548.2778
- `retentionStability` = 0.957618
- `policy_raw` = 0.751395  →  `policy_score` = 0.858484

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.686497**
- `dataset_rank` = **2** / 8
- `borda_points` = **7**
- `runtime_seconds` = 82.10

### `leitner` × `synthetic` — 排名 3 / 8

**Prediction：**

- `logLoss` = 1.441303
- `ici` = 0.468880
- `auc` = 0.527717
- `maeP` = 0.571587
- `prediction_raw` = 0.429391  →  `prediction_score` = 0.499525

**DHP：**

- `expectedMemoryFinal` = 38436.2114
- `masteredCount` = 0
- `totalReviews` = 628645
- `efficiency` = 0.061141
- `dhp_raw` = 19248.6762  →  `dhp_score` = 0.935589

**Policy：**

- `finalRecallRate` = 0.853800
- `reviewsPerDay` = 6984.9444
- `retentionStability` = 0.959988
- `policy_raw` = 0.630747  →  `policy_score` = 0.489621

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.650166**
- `dataset_rank` = **3** / 8
- `borda_points` = **6**
- `runtime_seconds` = 82.10

### `fsrs45` × `synthetic` — 排名 4 / 8

**Prediction：**

- `logLoss` = 0.710241
- `ici` = 0.224132
- `auc` = 0.554540
- `maeP` = 0.386900
- `prediction_raw` = 0.657074  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 15898.4014
- `masteredCount` = 31053
- `totalReviews` = 263319
- `efficiency` = 0.060377
- `dhp_raw` = 7979.3892  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.441400
- `reviewsPerDay` = 2925.7667
- `retentionStability` = 0.887941
- `policy_raw` = 0.797682  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.650000**
- `dataset_rank` = **4** / 8
- `borda_points` = **5**
- `runtime_seconds` = 82.10

### `sm2` × `synthetic` — 排名 5 / 8

**Prediction：**

- `logLoss` = 1.223161
- `ici` = 0.430807
- `auc` = 0.503970
- `maeP` = 0.535939
- `prediction_raw` = 0.477317  →  `prediction_score` = 0.604872

**DHP：**

- `expectedMemoryFinal` = 32313.6670
- `masteredCount` = 14584
- `totalReviews` = 528912
- `efficiency` = 0.061095
- `dhp_raw` = 16187.3810  →  `dhp_score` = 0.681437

**Policy：**

- `finalRecallRate` = 0.824400
- `reviewsPerDay` = 5876.8000
- `retentionStability` = 0.961910
- `policy_raw` = 0.687115  →  `policy_score` = 0.661958

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.643087**
- `dataset_rank` = **5** / 8
- `borda_points` = **4**
- `runtime_seconds` = 82.10

### `random` × `synthetic` — 排名 6 / 8

**Prediction：**

- `logLoss` = 1.065810
- `ici` = 0.398710
- `auc` = 0.507953
- `maeP` = 0.498653
- `prediction_raw` = 0.519611  →  `prediction_score` = 0.697840

**DHP：**

- `expectedMemoryFinal` = 20780.5938
- `masteredCount` = 0
- `totalReviews` = 297500
- `efficiency` = 0.069851
- `dhp_raw` = 10425.2224  →  `dhp_score` = 0.203056

**Policy：**

- `finalRecallRate` = 0.734100
- `reviewsPerDay` = 3305.5556
- `retentionStability` = 0.845849
- `policy_raw` = 0.757647  →  `policy_score` = 0.877598

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.560617**
- `dataset_rank` = **6** / 8
- `borda_points` = **3**
- `runtime_seconds` = 82.10

### `dhp` × `synthetic` — 排名 7 / 8

**Prediction：**

- `logLoss` = 2.328021
- `ici` = 0.592008
- `auc` = 0.631964
- `maeP` = 0.639759
- `prediction_raw` = 0.311987  →  `prediction_score` = 0.241458

**DHP：**

- `expectedMemoryFinal` = 29979.4026
- `masteredCount` = 22325
- `totalReviews` = 440999
- `efficiency` = 0.067981
- `dhp_raw` = 15023.6918  →  `dhp_score` = 0.584826

**Policy：**

- `finalRecallRate` = 0.743400
- `reviewsPerDay` = 4899.9889
- `retentionStability` = 0.975857
- `policy_raw` = 0.742929  →  `policy_score` = 0.832601

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.479865**
- `dataset_rank` = **7** / 8
- `borda_points` = **2**
- `runtime_seconds` = 82.10

### `hlr` × `synthetic` — 排名 8 / 8

**Prediction：**

- `logLoss` = 11.527703
- `ici` = 0.858747
- `auc` = 0.532545
- `maeP` = 0.858747
- `prediction_raw` = 0.202140  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 40018.5896
- `masteredCount` = 0
- `totalReviews` = 1314747
- `efficiency` = 0.030438
- `dhp_raw` = 20024.5138  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.894700
- `reviewsPerDay` = 14608.3000
- `retentionStability` = 0.941202
- `policy_raw` = 0.470601  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.350000**
- `dataset_rank` = **8** / 8
- `borda_points` = **1**
- `runtime_seconds` = 82.10

---
