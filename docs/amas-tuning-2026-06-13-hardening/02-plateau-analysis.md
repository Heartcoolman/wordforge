# 02 — F1 邻域平台分析（plateau vs needle）

> Hardening campaign 2026-06-13。问题：F1 是坐在搜索面的**平台**上（邻居也赢榜），还是**针尖**上（旋钮微动即丢榜，过拟合信号）？
> 数据源：`benchmarks/results/2026-06-12-rank1-search/results.jsonl`（72 行）+ `seed_stability.json` + `results_grade3_diagnostic.jsonl`。纯离线复盘，**未跑任何新模拟**。
> 复现：`python -m benchmarks.maimemo.plateau_analysis`（`benchmarks/maimemo/plateau_analysis.py`，确定性、只读）。

## 结论（TL;DR）

**F1 在连续旋钮轴上是宽平台，不是针尖**；但存在两条**机制悬崖**（毕业 streak=2、毕业 floor≥30 天）和一个**校准悬崖**（gspSuccessGrade=4），它们是"机制开关选对了"而非"连续值调到了针尖"。唯一的针尖式纹理是 duolingo 数据集的 final_score 胜出余量（~0.005–0.04，与种子噪声同量级）——这影响 G1 余量闸门的通过位，但不影响 Borda 排名第一（三种子 strict gap 恒 +4）。

| 判定维度 | 结果 |
|---|---|
| 连续旋钮（cap / young / mature / band / fuzz） | **平台**：获胜区域跨多档值 |
| 机制旋钮（streak / floor / grade） | **悬崖**：邻档直接丢榜，但属结构选择非精调 |
| 余量纹理 | duolingo 胜出余量薄且随种子波动（G5 字面判 FAIL 的已知项） |
| 总体判定 | **plateau**——无连续旋钮过拟合签名；附三条机制边界须保持 |

## 1. 数据与方法

- 每行 = 一个候选 GSP 配置代入 VAL 全量 10 算法榜（`2026-06-12-rank1-val-board`）经 whatif 重算的 Borda + 预注册闸门判定（`gsp_search.py:144-230`）。Borda 满分 30 = 3 数据集全部 rank 1。
- 闸门：G1 = Borda 严格第一且获胜数据集 final_score 余量 ≥ 0.005（`gsp_search.py:56,180-189`）；G2 = maimemo mastered ≥ 17000（`:52`）；G3 = pred LL/ICI ≤ amas6×1.02（`:53`）；G4 = retStab 地板（F1 持授权豁免，全网格无解，不计入本分析）。
- GSP 旋钮不触状态更新 → 预测腿在网格上恒定（`gsp_search.py:7-8`），邻居间的胜负差异**全部由 sim 腿（dhp 0.35 + policy 0.20 权重）驱动**，G3 在网格内不变。
- 与 F1 的距离 = 6 个 GSP 旋钮 + fuzz（从 label 解析，不在 config 字典里）中不同的个数。
- 行数说明：72 行含多种子复测（S3/G5）与 fuzz 扫描；seed-42 去重后 G1∧G2∧G3 通过的**不同配置共 34 个**（任务简报估的 ~27 偏低），其中 21 个 Borda=30。

## 2. F1 复现行（dist=0）

7 行精确同配置（3 种子 + 无缓存 fresh 复跑）**全部 Borda 30 / rank 1**：

| label | seed | Borda | 余量 mai/duo/syn |
|---|---|---|---|
| stageE_cap40_y86_m92_b14 = G5_F1_seed42 = G6_F1_FRESH_nocache | 42 | 30 | +0.0708 / +0.0051 / +0.0270 |
| S3_F1_seed7 / G5_verify_F1_seed7 | 7 | 30 | +0.0965~0.0928 / +0.0153~0.0088 / +0.0269 |
| S3_F1_seed2026 / G5_F1_seed2026 | 2026 | 30 | +0.0433~0.0494 / +0.0483~0.0401 / +0.0270 |

注意：G5_verify_F1_seed7 / G5_F1_seed2026 两行显示 G3 fail，是**锚点口径伪影**——它们在自己种子下 fresh 重测 pred，却与固定 seed-42 val board 的 amas6 锚点比 1.02 倍上限；同种子复用 seed-42 pred 的 S3 行 G3 通过。Borda 不受影响。

## 3. F1 的单旋钮邻居（dist=1，实测）

仅 **cap 与 fuzz** 两轴在 F1 处有 dist-1 实测：

| 旋钮变动 | label | Borda | rank | G1/G2/G3 | 备注 |
|---|---|---|---|---|---|
| cap 40→42 | stageH_cap42_y86_m92_b14 | **30** | 1/1/1 | G1✗(duo 余量 0.0018<0.005)/✓/✓ | 排名仍全胜，仅余量闸门差 0.003 |
| cap 40→45 | stageD_youngLo_matHi = G6_cap45_fuzz0.0 | **30** | 1/1/1 | ✓✓✓ | duo 余量 0.0062 |
| cap 40→50 | G6_cap50_fuzz0.0 | **30** | 1/1/1 | ✓✓✓ | duo 余量 0.0399 |
| fuzz 0→0.05 | G6_F1_fuzz0.05 | 29 | duo#2 | ✓✓✓（won 集内 G1 过） | duo 差 0.0032 落第二 |
| fuzz 0→0.10/0.15/0.20 | G6_F1_fuzz0.10/15/20 | **30** | 1/1/1 | ✓✓✓ | duo 余量 0.035/0.069/0.052 |

cap 38/42 处 G1 余量失败与 fuzz 0.05 的 -1 Borda 都发生在 duolingo 余量 ±0.005 的噪声带内（G5 测得 F1 自身 duo 余量跨种子 0.0051/0.0088/0.0401，std≈0.019），**沿 cap/fuzz 轴的余量起伏是噪声纹理而非斜坡**。

## 4. F1 处缺测轴的代理证据（dist≥2，诚实标注）

streak / floor / young / mature / band 五轴在 F1 处**没有** dist-1 行。最近代理 = cap45 锚点（本身是 Borda-30 G1-pass 的 F1 邻居）与 band-off 上下文：

| 轴 | 证据（上下文） | 结果 | 跌幅 |
|---|---|---|---|
| streak 2→3 | stageI_cap45_k3_band（cap45+F1 retention 带） | Borda 24, rank 3, duo#4(-0.203) | **-6** |
| streak 2→3 | stageI_cap45_k3_fl30（band-off） | Borda 25, rank 2 | -5 |
| streak 2→0（毕业关） | stageF_cap45_noGrad（band-off） | Borda 24, rank 3 | -6 |
| floor 30→28 | stageL_cap45_k2_fl28_band（cap45+F1 retention 带） | Borda **19**, rank 5, G2 崩（mastered 13181） | **-11** |
| floor 30→28/25 | stageL_cap45_k2_fl28/fl25（band-off） | Borda 24, rank 3 | -6 |
| floor 30→32 | stageK_fl32_y85_m93_b16（dist-4） | Borda 28, rank 1, G123✓ | -2（向上浅） |
| young/mature/band | stageC/D/E/G/J/K 共 20 个组合（cap45 上下文） | 27–30，其中 11 个 Borda-30 | 宽容 |
| grade 4→3 | results_grade3_diagnostic.jsonl S1_base（GSP 全关基点） | Borda 17 vs grade4 基点 21/24；mai ICI 0.0625 爆 G3 上限 0.025 | **-4~-7 + 校准崩** |

## 5. 获胜区域的旋钮覆盖谱（34 个 G1∧G2∧G3 passer）

| 旋钮 | passer 取值域 | 其中 Borda-30 取值域 | 形态 |
|---|---|---|---|
| cap | 35–60 | **40 / 45 / 50** | 平台（≥75 才掉到 26） |
| streak | **仅 2** | 仅 2 | 悬崖（0/3 全灭；**1 未测**） |
| floor | 30 / 32 | 仅 30 | 向下悬崖（≤28 全灭），向上浅坡 |
| young | 0(关)–0.97 共 12 档 | 7 档（0, 0.86–0.97） | 宽平台 |
| mature | 0(关)–0.95 共 12 档 | 9 档（0, 0.80–0.92） | 宽平台（≥0.94 略掉） |
| band | 0–30 天共 13 档 | 8 档（0–30） | 宽平台 |
| fuzz | 0–0.20 | 0, 0.10–0.20 | 平台 |

关键事实：**retention 带整体关闭（y=m=b=0，stageBA_cap45_k2_fl30 / stageD_cap40_fl30 / stageF_cap50_noBand）也是 Borda-30 + G123 全过**。F1 的胜利由"毕业制 + cap 40–50 + floor 30"承载，retention 带形状贡献的是余量微调，不是胜负本身——这是平台判定的最强证据。

## 6. 每旋钮敏感度排序（高→低）

1. **gspSuccessGrade**（校准悬崖）：4→3 在基点 Borda -4~-7 且 mai ICI 0.0625 直接爆 G3（上限 0.025）。仅在 GSP-off 基点测过，未在 F1 处测。
2. **gspGraduationFloorDays**（向下悬崖）：30→28 在 cap45+band 上下文 -11（G2 崩），band-off -6；向上（32/35）浅坡 -2~0。
3. **gspGraduationStreak**（悬崖点）：已测 {0,2,3} 中仅 2 获胜，邻档 -5~-6；k=1 未测。
4. **gspIntervalCapDays**（平台）：40–50 全 Borda 30；35–60 仍 28–29 过闸；≥75 降到 26；89 触 G2（mastered 15631<17000）。
5. **gspIntervalFuzz**（平台）：0–0.20 内 29–30，唯一 -1 在 0.05 处且属 duo 余量噪声。
6. **young/mature/band**（最宽平台）：获胜区横跨 y 0.80–0.97、m 0.80–0.93、b 0–30（含整体关闭）。

## 7. 限定条件与已知缺口

- **网格覆盖缺口**（未跑新模拟，如实列出）：F1 处 dist-1 缺 streak / floor / young / mature / band 五轴及 grade=3、cap<40；敏感度靠 cap45 锚点（本身 Borda-30）外推。若后续要补盲，最高价值的三发是 `F1+streak3`、`F1+floor28`、`F1+grade3`（预期均按悬崖方向失败，证伪即推翻平台结论）。
- **全部邻域证据来自 VAL split、n_users=300、主种子 42**；TEST 榜（`2026-06-13-gsp-ship`）只跑过 F1 本身，邻居的 TEST 行为未知。
- **duolingo 余量薄**：F1 三种子 duo 胜出余量 0.0051/0.0088/0.0401，mean 0.018 < 2σ 0.038，G5 字面判 FAIL（`seed_stability.json` verdict）；Borda 排名层面三种子 strict gap 恒 +4、won-set 稳定。平台结论以**排名**为口径成立，以**duo final_score 余量**为口径则始终处于噪声边缘。
- **G3 合成集 ICI 头寸薄**（1.02 倍上限）：同配置 fresh 重测 pred 曾使基点行在 syn ICI 上翻车（0.22513 > 0.22366，S1_base_grade4 行），passer 集合的 G3 位对测量噪声敏感。
- G4（retStab）全网格无解、F1 持授权豁免，不构成本分析的区分信号。

## 8. 产物

- 分析脚本：`benchmarks/maimemo/plateau_analysis.py`（确定性只读，输出表 A–F）
- 本报告：`docs/amas-tuning-2026-06-13-hardening/02-plateau-analysis.md`
