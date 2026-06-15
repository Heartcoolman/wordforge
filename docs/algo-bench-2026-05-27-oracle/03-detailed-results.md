# 详细原始指标 2026-05-27-oracle

> 24 个 (algo, dataset) 组合的全量原始指标 + 归一化分数。  
> 由 `benchmarks/maimemo/leaderboard.py` 从 `benchmarks/results/2026-05-27-oracle/*.json` 生成。

## duolingo_hlr

### `sm2` × `duolingo_hlr` — 排名 1 / 8

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
- `dhp_raw` = 4054.5699  →  `dhp_score` = 0.655594

**Policy：**

- `finalRecallRate` = 0.441300
- `reviewsPerDay` = 3523.2889
- `retentionStability` = 0.858994
- `policy_raw` = 0.753333  →  `policy_score` = 0.676109

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.810958**
- `dataset_rank` = **1** / 8
- `borda_points` = **8**
- `runtime_seconds` = 0.14
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `leitner` × `duolingo_hlr` — 排名 2 / 8

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
- `dhp_raw` = 4788.3697  →  `dhp_score` = 0.774656

**Policy：**

- `finalRecallRate` = 0.353700
- `reviewsPerDay` = 4907.5111
- `retentionStability` = 0.837910
- `policy_raw` = 0.673580  →  `policy_score` = 0.498336

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.807698**
- `dataset_rank` = **2** / 8
- `borda_points` = **7**
- `runtime_seconds` = 0.13
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `dhp` × `duolingo_hlr` — 排名 3 / 8

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
- `dhp_raw` = 4355.1678  →  `dhp_score` = 0.704367

**Policy：**

- `finalRecallRate` = 0.319700
- `reviewsPerDay` = 3711.7778
- `retentionStability` = 0.845456
- `policy_raw` = 0.737139  →  `policy_score` = 0.640013

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.780957**
- `dataset_rank` = **3** / 8
- `borda_points` = **6**
- `runtime_seconds` = 0.14
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `fsrs` × `duolingo_hlr` — 排名 4 / 8

**Prediction：**

- `logLoss` = 0.422241
- `ici` = 0.202638
- `auc` = 0.540354
- `maeP` = 0.274010
- `prediction_raw` = 0.716866  →  `prediction_score` = 0.887205

**DHP：**

- `expectedMemoryFinal` = 7546.1824
- `masteredCount` = 1145
- `totalReviews` = 309658
- `efficiency` = 0.024369
- `dhp_raw` = 3785.2757  →  `dhp_score` = 0.611901

**Policy：**

- `finalRecallRate` = 0.325400
- `reviewsPerDay` = 3440.6444
- `retentionStability` = 0.838333
- `policy_raw` = 0.747134  →  `policy_score` = 0.662292

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.745866**
- `dataset_rank` = **4** / 8
- `borda_points` = **5**
- `runtime_seconds` = 0.18
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `amas` × `duolingo_hlr` — 排名 5 / 8

**Prediction：**

- `logLoss` = 0.422241
- `ici` = 0.202638
- `auc` = 0.540354
- `maeP` = 0.274010
- `prediction_raw` = 0.716866  →  `prediction_score` = 0.887205

**DHP：**

- `expectedMemoryFinal` = 7352.0008
- `masteredCount` = 1024
- `totalReviews` = 298224
- `efficiency` = 0.024653
- `dhp_raw` = 3688.3269  →  `dhp_score` = 0.596170

**Policy：**

- `finalRecallRate` = 0.307000
- `reviewsPerDay` = 3313.6000
- `retentionStability` = 0.834016
- `policy_raw` = 0.751328  →  `policy_score` = 0.671641

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.742230**
- `dataset_rank` = **5** / 8
- `borda_points` = **4**
- `runtime_seconds` = 0.25
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `fsrs45` × `duolingo_hlr` — 排名 6 / 8

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
- `dhp_raw` = 1452.8562  →  `dhp_score` = 0.233459

**Policy：**

- `finalRecallRate` = 0.163100
- `reviewsPerDay` = 1057.8333
- `retentionStability` = 0.813326
- `policy_raw` = 0.853771  →  `policy_score` = 0.899991

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.685955**
- `dataset_rank` = **6** / 8
- `borda_points` = **3**
- `runtime_seconds` = 0.15
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `random` × `duolingo_hlr` — 排名 7 / 8

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
- `dhp_raw` = 13.9998  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 977.0444
- `retentionStability` = 0.894980
- `policy_raw` = 0.898638  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.650000**
- `dataset_rank` = **7** / 8
- `borda_points` = **2**
- `runtime_seconds` = 0.26
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `hlr` × `duolingo_hlr` — 排名 8 / 8

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
- `dhp_raw` = 6177.2156  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.874700
- `reviewsPerDay` = 11053.4444
- `retentionStability` = 0.900028
- `policy_raw` = 0.450014  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.350000**
- `dataset_rank` = **8** / 8
- `borda_points` = **1**
- `runtime_seconds` = 0.14
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

---

## maimemo

### `dhp` × `maimemo` — 排名 1 / 8

**Prediction：**

- `logLoss` = 0.315770
- `ici` = 0.060420
- `auc` = 0.813072
- `maeP` = 0.221786
- `prediction_raw` = 0.862642  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 20293.6818
- `masteredCount` = 15333
- `totalReviews` = 191880
- `efficiency` = 0.105762
- `dhp_raw` = 10199.7219  →  `dhp_score` = 0.511923

**Policy：**

- `finalRecallRate` = 0.756900
- `reviewsPerDay` = 2132.0000
- `retentionStability` = 0.918051
- `policy_raw` = 0.852426  →  `policy_score` = 0.910725

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.811318**
- `dataset_rank` = **1** / 8
- `borda_points` = **8**
- `runtime_seconds` = 5.25
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `sm2` × `maimemo` — 排名 2 / 8

**Prediction：**

- `logLoss` = 0.386768
- `ici` = 0.096956
- `auc` = 0.587526
- `maeP` = 0.211771
- `prediction_raw` = 0.769817  →  `prediction_score` = 0.873482

**DHP：**

- `expectedMemoryFinal` = 21024.1345
- `masteredCount` = 12192
- `totalReviews` = 244023
- `efficiency` = 0.086156
- `dhp_raw` = 10555.1452  →  `dhp_score` = 0.554709

**Policy：**

- `finalRecallRate` = 0.870200
- `reviewsPerDay` = 2711.3667
- `retentionStability` = 0.900761
- `policy_raw` = 0.814812  →  `policy_score` = 0.825947

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.752404**
- `dataset_rank` = **2** / 8
- `borda_points` = **7**
- `runtime_seconds` = 5.15
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `amas` × `maimemo` — 排名 3 / 8

**Prediction：**

- `logLoss` = 0.328986
- `ici` = 0.070347
- `auc` = 0.790085
- `maeP` = 0.227363
- `prediction_raw` = 0.850124  →  `prediction_score` = 0.982939

**DHP：**

- `expectedMemoryFinal` = 17566.5292
- `masteredCount` = 17342
- `totalReviews` = 173924
- `efficiency` = 0.101001
- `dhp_raw` = 8833.7651  →  `dhp_score` = 0.347489

**Policy：**

- `finalRecallRate` = 0.751400
- `reviewsPerDay` = 1932.4889
- `retentionStability` = 0.907432
- `policy_raw` = 0.857091  →  `policy_score` = 0.921241

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.748192**
- `dataset_rank` = **3** / 8
- `borda_points` = **6**
- `runtime_seconds` = 5.39
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `fsrs` × `maimemo` — 排名 4 / 8

**Prediction：**

- `logLoss` = 0.329093
- `ici` = 0.070446
- `auc` = 0.789799
- `maeP` = 0.227414
- `prediction_raw` = 0.849987  →  `prediction_score` = 0.982752

**DHP：**

- `expectedMemoryFinal` = 17549.3707
- `masteredCount` = 18113
- `totalReviews` = 175551
- `efficiency` = 0.099967
- `dhp_raw` = 8824.6689  →  `dhp_score` = 0.346394

**Policy：**

- `finalRecallRate` = 0.699400
- `reviewsPerDay` = 1950.5667
- `retentionStability` = 0.902656
- `policy_raw` = 0.853800  →  `policy_score` = 0.913822

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.746241**
- `dataset_rank` = **4** / 8
- `borda_points` = **5**
- `runtime_seconds` = 5.99
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `leitner` × `maimemo` — 排名 5 / 8

**Prediction：**

- `logLoss` = 0.463512
- `ici` = 0.172072
- `auc` = 0.293794
- `maeP` = 0.305393
- `prediction_raw` = 0.643814  →  `prediction_score` = 0.701742

**DHP：**

- `expectedMemoryFinal` = 25390.8349
- `masteredCount` = 0
- `totalReviews` = 355435
- `efficiency` = 0.071436
- `dhp_raw` = 12731.1355  →  `dhp_score` = 0.816655

**Policy：**

- `finalRecallRate` = 0.891300
- `reviewsPerDay` = 3949.2778
- `retentionStability` = 0.896715
- `policy_raw` = 0.750894  →  `policy_score` = 0.681881

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.737989**
- `dataset_rank` = **5** / 8
- `borda_points` = **4**
- `runtime_seconds` = 5.19
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `fsrs45` × `maimemo` — 排名 6 / 8

**Prediction：**

- `logLoss` = 0.323247
- `ici` = 0.048871
- `auc` = 0.781863
- `maeP` = 0.156393
- `prediction_raw` = 0.855248  →  `prediction_score` = 0.989923

**DHP：**

- `expectedMemoryFinal` = 11733.3848
- `masteredCount` = 15878
- `totalReviews` = 72898
- `efficiency` = 0.160956
- `dhp_raw` = 5947.1704  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.457500
- `reviewsPerDay` = 809.9778
- `retentionStability` = 0.865067
- `policy_raw` = 0.892034  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.645465**
- `dataset_rank` = **6** / 8
- `borda_points` = **3**
- `runtime_seconds` = 5.22
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `random` × `maimemo` — 排名 7 / 8

**Prediction：**

- `logLoss` = 1.179458
- `ici` = 0.423814
- `auc` = 0.224038
- `maeP` = 0.491757
- `prediction_raw` = 0.404175  →  `prediction_score` = 0.375120

**DHP：**

- `expectedMemoryFinal` = 18487.8785
- `masteredCount` = 0
- `totalReviews` = 200760
- `efficiency` = 0.092089
- `dhp_raw` = 9289.9837  →  `dhp_score` = 0.402408

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 2230.6667
- `retentionStability` = 0.830227
- `policy_raw` = 0.803580  →  `policy_score` = 0.800631

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.469773**
- `dataset_rank` = **7** / 8
- `borda_points` = **2**
- `runtime_seconds` = 5.30
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `hlr` × `maimemo` — 排名 8 / 8

**Prediction：**

- `logLoss` = 10.177334
- `ici` = 0.872410
- `auc` = 0.302261
- `maeP` = 0.874340
- `prediction_raw` = 0.128955  →  `prediction_score` = 0.000000

**DHP：**

- `expectedMemoryFinal` = 28495.9941
- `masteredCount` = 0
- `totalReviews` = 2301099
- `efficiency` = 0.012384
- `dhp_raw` = 14254.1890  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.970800
- `reviewsPerDay` = 25567.7667
- `retentionStability` = 0.896725
- `policy_raw` = 0.448363  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.350000**
- `dataset_rank` = **8** / 8
- `borda_points` = **1**
- `runtime_seconds` = 5.15
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

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

- `expectedMemoryFinal` = 24328.0123
- `masteredCount` = 22397
- `totalReviews` = 399916
- `efficiency` = 0.060833
- `dhp_raw` = 12194.4226  →  `dhp_score` = 0.496173

**Policy：**

- `finalRecallRate` = 0.599700
- `reviewsPerDay` = 4443.5111
- `retentionStability` = 0.906348
- `policy_raw` = 0.730998  →  `policy_score` = 0.750192

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.687500**
- `dataset_rank` = **1** / 8
- `borda_points` = **8**
- `runtime_seconds` = 0.35
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `fsrs` × `synthetic` — 排名 2 / 8

**Prediction：**

- `logLoss` = 0.935562
- `ici` = 0.459007
- `auc` = 0.648181
- `maeP` = 0.545352
- `prediction_raw` = 0.569640  →  `prediction_score` = 0.808446

**DHP：**

- `expectedMemoryFinal` = 24119.2438
- `masteredCount` = 21785
- `totalReviews` = 404943
- `efficiency` = 0.059562
- `dhp_raw` = 12089.4029  →  `dhp_score` = 0.489958

**Policy：**

- `finalRecallRate` = 0.659900
- `reviewsPerDay` = 4499.3667
- `retentionStability` = 0.906237
- `policy_raw` = 0.728150  →  `policy_score` = 0.742548

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.683796**
- `dataset_rank` = **2** / 8
- `borda_points` = **7**
- `runtime_seconds` = 0.21
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `fsrs45` × `synthetic` — 排名 3 / 8

**Prediction：**

- `logLoss` = 0.710241
- `ici` = 0.224132
- `auc` = 0.554540
- `maeP` = 0.386900
- `prediction_raw` = 0.657074  →  `prediction_score` = 1.000000

**DHP：**

- `expectedMemoryFinal` = 7587.6593
- `masteredCount` = 31740
- `totalReviews` = 239374
- `efficiency` = 0.031698
- `dhp_raw` = 3809.6787  →  `dhp_score` = 0.000000

**Policy：**

- `finalRecallRate` = 0.280500
- `reviewsPerDay` = 2659.7111
- `retentionStability` = 0.914132
- `policy_raw` = 0.824081  →  `policy_score` = 1.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.650000**
- `dataset_rank` = **3** / 8
- `borda_points` = **6**
- `runtime_seconds` = 0.21
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `sm2` × `synthetic` — 排名 4 / 8

**Prediction：**

- `logLoss` = 1.223161
- `ici` = 0.430807
- `auc` = 0.503970
- `maeP` = 0.535939
- `prediction_raw` = 0.477317  →  `prediction_score` = 0.606181

**DHP：**

- `expectedMemoryFinal` = 31263.2810
- `masteredCount` = 9824
- `totalReviews` = 577542
- `efficiency` = 0.054132
- `dhp_raw` = 15658.7065  →  `dhp_score` = 0.701174

**Policy：**

- `finalRecallRate` = 0.730700
- `reviewsPerDay` = 6417.1333
- `retentionStability` = 0.915510
- `policy_raw` = 0.636898  →  `policy_score` = 0.497652

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.617723**
- `dataset_rank` = **4** / 8
- `borda_points` = **5**
- `runtime_seconds` = 0.17
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `leitner` × `synthetic` — 排名 5 / 8

**Prediction：**

- `logLoss` = 1.441303
- `ici` = 0.468880
- `auc` = 0.527717
- `maeP` = 0.571587
- `prediction_raw` = 0.429391  →  `prediction_score` = 0.501183

**DHP：**

- `expectedMemoryFinal` = 36758.2218
- `masteredCount` = 0
- `totalReviews` = 672766
- `efficiency` = 0.054637
- `dhp_raw` = 18406.4294  →  `dhp_score` = 0.863773

**Policy：**

- `finalRecallRate` = 0.782100
- `reviewsPerDay` = 7475.1778
- `retentionStability` = 0.910600
- `policy_raw` = 0.581541  →  `policy_score` = 0.349089

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.597671**
- `dataset_rank` = **5** / 8
- `borda_points` = **4**
- `runtime_seconds` = 0.15
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `random` × `synthetic` — 排名 6 / 8

**Prediction：**

- `logLoss` = 1.065810
- `ici` = 0.398710
- `auc` = 0.507953
- `maeP` = 0.498653
- `prediction_raw` = 0.519611  →  `prediction_score` = 0.698841

**DHP：**

- `expectedMemoryFinal` = 11475.4044
- `masteredCount` = 0
- `totalReviews` = 297500
- `efficiency` = 0.038573
- `dhp_raw` = 5756.9887  →  `dhp_score` = 0.115233

**Policy：**

- `finalRecallRate` = 0.000000
- `reviewsPerDay` = 3305.5556
- `retentionStability` = 0.843411
- `policy_raw` = 0.756428  →  `policy_score` = 0.818437

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.518498**
- `dataset_rank` = **6** / 8
- `borda_points` = **3**
- `runtime_seconds` = 0.26
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `dhp` × `synthetic` — 排名 7 / 8

**Prediction：**

- `logLoss` = 2.328021
- `ici` = 0.592008
- `auc` = 0.631964
- `maeP` = 0.639759
- `prediction_raw` = 0.311987  →  `prediction_score` = 0.243972

**DHP：**

- `expectedMemoryFinal` = 27914.1134
- `masteredCount` = 17561
- `totalReviews` = 459674
- `efficiency` = 0.060726
- `dhp_raw` = 13987.4197  →  `dhp_score` = 0.602275

**Policy：**

- `finalRecallRate` = 0.667200
- `reviewsPerDay` = 5107.4889
- `retentionStability` = 0.915593
- `policy_raw` = 0.702422  →  `policy_score` = 0.673501

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.455284**
- `dataset_rank` = **7** / 8
- `borda_points` = **2**
- `runtime_seconds` = 0.17
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

### `hlr` × `synthetic` — 排名 8 / 8

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
- `dhp_raw` = 20708.5104  →  `dhp_score` = 1.000000

**Policy：**

- `finalRecallRate` = 0.890000
- `reviewsPerDay` = 16811.1111
- `retentionStability` = 0.902931
- `policy_raw` = 0.451465  →  `policy_score` = 0.000000

**汇总：**

- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **0.350000**
- `dataset_rank` = **8** / 8
- `borda_points` = **1**
- `runtime_seconds` = 0.17
- `notes` = v0.7: 独立 oracle (duolingo/synthetic MPS 重训)

---
