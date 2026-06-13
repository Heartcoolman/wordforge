# AMAS v5 加固战役终报告：抗过拟合验证（2026-06-13）

> 分支：`tune/amas-fsrs6`
> 预注册协议：[00-protocol.md](./00-protocol.md)（commit db05fd8，判据与种子清单在全部新增运行前冻结）
> 性质：验证非选型。F1 船值配置全程冻结，未因任何结果回调。
> 上代终报告：[../amas-tuning-2026-06-12-rank1/01-campaign-final-report.md](../amas-tuning-2026-06-12-rank1/01-campaign-final-report.md)

---

## 1. TL;DR 总判定

| 工作流 | 打击的风险 | 判定 |
|---|---|---|
| W1 新鲜队列终验 | TEST 二次观察 | **PASS**——all-fresh 榜 amas Borda 30/30 三数据集全 rank 1，且前五名 Borda 总分与 ship 终榜逐条相同 |
| W2a alt-board oracle 口径 | mastered 自报博弈 | **PASS**——oracle 加权榜 29/30 第一（无平局），纯 EM 榜 24 第一 |
| W2b 平台期分析 | 搜索面针尖 | **PASS**——plateau 非 needle；3 盲点探针证实悬崖全为离散机制且被闸门看护 |
| W2c 种子 3→10 双 tier | G5 豁免统计瑕疵 | **部分**——mai/syn 全过；duo：rank Tier-1 10/10、Tier-2 9/10，margin 两 tier 均 < 2σ（照实，无豁免） |
| W3 生产日志重放 | 基准 ≠ 生产 | 工具交付 + fixture 自测全过；**实测阻塞**（无真实用户数据，协议 §6） |

**结论**：「TEST 二次观察」「mastered 口径博弈」「搜索面针尖」三条风险被实证压缩至接近闭合；
残余 = ① duo 胜出幅度在种子噪声内不可分辨（名次本身在全耦合噪声下也会在刀刃种子上翻一次），
② 基准→真实用户分布迁移（只能灰度回答）。F1 的 mai/syn 优势与全榜 Borda 统治力
（**两 tier 合计 20/20 种子-运行全部全榜严格第一**）在所有验证轴上稳健。

## 2. W1 新鲜队列终验（主判据 PASS）

### 2.1 队列构造（协议 §2.1）

- mai/duo：`exclude_users` 排除全部历史 board 协议队列（侦察验证采样确定性 → 全部历史
  test 运行坍缩为同一 seed-42 300 人集合；清单入仓 + 采样后零重叠断言）。补丁默认路径
  改前/改后位级对拍通过（f6b59a0）。
- syn：seed-777 重造宇宙（同 DGP、复制冻结 seed-42 oracle 与 DHP 缓存），schema/规模/
  取值域完整性校验通过。

### 2.2 结果（一次性运行，不回调）

all-fresh 三数据集榜（`benchmarks/results/2026-06-13-hardening-fresh/` + `2026-06-13-fresh-synthetic/`，
榜文档 `docs/algo-bench-2026-06-13-allfresh/`）：

| 条目 | all-fresh Borda | mai/duo/syn rank | ship 终榜对照 |
|---|---|---|---|
| **amas** | **30** | **1/1/1** | 30（1/1/1） |
| fsrs45 | 26 | 2/3/2 | 26 |
| amas6 | 23 | 5/2/3 | 23 |
| fsrs6 | 19 | — | 19 |
| fsrs | 17 | — | 17 |

余量：mai +0.0691（ship +0.065）、duo +0.0116（ship +0.022）、syn +0.0484（ship +0.051）。
**整个榜面结构在全新队列上原样复现**——这是新鲜队列验证能给出的最强形态。

### 2.3 腿漂移审计（amas，ship→fresh）

| 数据集 | mastered | LL | ICI | retStab |
|---|---|---|---|---|
| mai | +12.7%（全场 +11~15% 共动） | +4.6% | −6.7% | ±0.0% |
| duo | −11.9%（全场 −8~12% 共动） | +5.8% | −4.4% | −0.1% |
| syn | −1.0% | −0.4% | +3.5% | −0.0% |

mastered 漂移与竞品同向共动（队列构成方差，非算法特异退化）；retStab 三数据集逐位持平。
duo amas 跌幅（−11.9%）略大于竞品（−8~−9.5%），余量由此从 0.022 收窄至 0.0116——照实记录。
无任何「val 选型 / test 已知差距塑形」的过拟合签名（其形态应为 amas 特异塌落）。

### 2.4 构造性限制（协议 §2.3 重申）

mai 的新鲜性 =「与全部 board 协议运行不相交」（前代 pipeline 级 test 评估的用户集不可重构）；
duo fresh sim 队列为近普查（332 取 300）、pred 腿拉入 bucket 19（sim/pred 队列不再重合）；
syn-777 验证同分布新用户、不验证分布迁移。

## 3. W2 证据三件套

### 3.1 W2a alt-board oracle 真值口径（PASS）

复用上代附录工具（`alt_board.py`）对 2026-06-13-gsp-ship 终榜复算，official 变体先逐位
复现官方榜（锚定零偏差）：

- **oracle 加权**（dhp_raw = 0.5·EMF + 0.3·eff + 0.2·mastered）：amas **29/30 第一**（无平局，
  比上代三方并列 25 实质强化）；上代冠军 fsrs45 在此口径 1→8 的坍塌本代复现。
- **纯 expectedMemoryFinal**：amas **24 第一**（无平局）。
- amas oracle EMF 三数据集全面领先 fsrs45（mai 19710 vs 11748、duo 5072 vs 2876、syn 22834
  vs 7588）——自报 mastered 与 oracle 真值同向，**指标博弈风险证伪**。
- 诚实负项：oracle 加权 duo 腿 −0.0058 输 fsrs6；纯 EM duo rank 6（该口径奖励复习密度，
  amas duo −47% 复习量被结构性惩罚）。上代附录的口径局限（oracle 同源、权重作者自选）沿用。

产物：`benchmarks/results/2026-06-13-hardening/altboard/`。

### 3.2 W2b 平台期 + 盲点探针（PASS）

64 配置搜索面分析（[02-plateau-analysis.md](./02-plateau-analysis.md)）：**plateau 非 needle**。
cap 40–50 全 Borda 30、fuzz 0–0.20 平坦、分带全关仍 Borda 30（赢面由毕业+帽+下限承载）；
34 配置过 G1∧G2∧G3、21 个 Borda 30，winning region 横跨 cap 35–60 / young 0–0.97 /
mature 0–0.95 / band 0–30。

3 个 dist-1 盲点探针（val seed-42 单点，诊断非闸门）补齐覆盖缺口，**悬崖全为离散机制选择
且全部被既有闸门捕获**：

| 探针 | Borda | 拦截闸门 | 机制确认 |
|---|---|---|---|
| F1+streak3 | 24（−6） | G1 | 毕业连击 k=2 是悬崖点 |
| F1+floor28 | 17（−13） | G2（mai mastered 11259）+G4 | 毕业下限 <30 即跌穿 30d mastered 线 |
| F1+grade3 | 30（但失格） | G3（mai ICI 0.0625 = 2.5× 顶）+G4 | grade-4 忠实路径承载校准 |

连续旋钮平坦 + 离散机制有闸门看护 = 非过拟合针尖签名。

### 3.3 W2c 种子稳定性 3→10 双 tier（mai/syn PASS；duo 照实 FAIL）

S10 = {42, 7, 2026, 0, 1, 2, 3, 4, 5, 6}（预注册）。两 tier 估计量不同，永不混池
（Tier-1 = amas-only 噪声注入冻结 seed-42 榜；Tier-2 = 全板逐种子重模拟，修正既有 3 种子
方法「竞品冻结低估全板噪声」缺陷）。产物：`seed_stability_10.json`、`compute_g5x.py`、
`tier1_seed_runs.jsonl`、`val-board-seed*/`（300 JSON）。

| 数据集 | Tier-1 rank | Tier-1 margin | Tier-2 rank | Tier-2 margin |
|---|---|---|---|---|
| mai | 10/10 | 0.0714±0.0268 **PASS** | 10/10 | 0.0747±0.0050 **PASS**（≈15σ） |
| duo | 10/10 | 0.0159±0.0127 FAIL | **9/10** | 0.0175±0.0100 FAIL |
| syn | 10/10 | 0.0270±0.0005 PASS（饱和低信息，预注册声明） | 10/10 | 0.0290±0.0014 PASS |

- **duo 失利种子 = seed 42 本身**（Tier-2）：amas6 0.9466 vs amas 0.9431（−0.0035）。amas 该
  种子 Borda 仍 29、仍全榜严格第一。预注册解读照录：**duo 第一名次在 amas-only 噪声下种子
  稳健，但在全耦合噪声下会在刀刃种子上翻转；胜出幅度两 tier 均在种子噪声内不可分辨**。
  无豁免，不重掷。佐证：两次独立 test-split 观察（ship 终榜 +0.022、fresh 队列 +0.0116）
  amas 均胜 duo。
- 正面发现：Tier-2 mai margin std（0.0050）远小于 Tier-1（0.0268）——全板重模拟下队列噪声
  对全体算法共动、余量配对对消；旧方法实际上**高估**了 mai 余量噪声。
- **board 级**：两 tier 合计 20/20 种子-运行 amas 全榜严格 Borda 第一（Tier-2 Borda 序列
  29,30×9）。

## 4. W3 生产日志重放（工具交付；实测阻塞）

`benchmarks/prod_replay/` 五件套（export.sql / configs.py / build_parquet.py /
run_prediction.py / interval_shift.py，1132 行）落仓，fixture 自测全过：

- 功效分级如设计触发（n<500 → 仅方向性；n≥500 → logLoss + 配对 user-level bootstrap 95% CI；
  n≥1000 → ICI）；
- grade-aware 敏感性路径验证 **F1 坍缩不变量**：grade-aware NEW ≡ binary NEW 逐行 p 偏差
  0.0（本地 FSRS-6 忠实镜像与 WordforgeMirrorState 互证）；
- interval_shift 爆炸半径字段全可用（90→40 重召回数 / cap 钉住数 / 毕业下限触发数 /
  偏移直方图 / 分带 / 分位数），3 状态手算对拍通过；
- 测试中抓出并修复 pandas 3.0 `datetime64[us]` 精度真 bug（否则日差全零）。

实测阻塞（协议 §6 预声明）：8.135.57.148 经用户 2026-06-01 澄清非生产、无真实用户数据；
本机 SSH 密钥未获授权。未来任何真实数据源接入即可按规格执行。

## 5. 更新后的残余风险清单

1. ~~TEST 二次观察~~ → 新鲜队列全复现，风险实证压缩至近零（残余：mai 新鲜性限于 board
   协议口径，§2.4）。
2. ~~mastered 自报博弈~~ → oracle 口径双榜第一，证伪。
3. ~~搜索面针尖~~ → plateau + 探针 + 闸门看护，证伪。
4. **duo 胜出幅度**：名次多数稳健但幅度在噪声内（两 tier margin FAIL + Tier-2 一个种子
   翻转）。解读边界：amas duo 第一是「概率性优势」而非「确定性统治」；mai/syn 为确定性统治。
5. **基准 → 真实用户**：未变（无真实数据可测）；调度语义实变（区间帽 90→40、连击毕业 30d）
   的行为效应只能灰度回答，工具已就绪。

## 6. 产物索引与 commit 链

| 产物 | 位置 |
|---|---|
| 预注册协议 | `00-protocol.md`（db05fd8） |
| all-fresh 榜 JSON | `benchmarks/results/2026-06-13-hardening-fresh/`（20）+ `2026-06-13-fresh-synthetic/`（10） |
| all-fresh 榜文档 | `docs/algo-bench-2026-06-13-allfresh/` |
| alt-board 复算 | `benchmarks/results/2026-06-13-hardening/altboard/` |
| 平台期分析 | `02-plateau-analysis.md` + `benchmarks/maimemo/plateau_analysis.py` |
| 种子稳定性 10 种子 | `benchmarks/results/2026-06-13-hardening-seeds/`（tier1_seed_runs.jsonl + val-board-seed×10 + seed_stability_10.json + compute_g5x.py） |
| prod_replay 工具 | `benchmarks/prod_replay/`（f6b59a0） |
| 队列清单 | `benchmarks/results/2026-06-13-hardening-fresh/excluded_users__*.txt` |

| commit | 内容 |
|---|---|
| db05fd8 | 预注册协议 + alt-board 复算 + 平台期分析 |
| f6b59a0 | exclude_users + run_val_board --seed + prod_replay 套件 |
| （本提交） | W1 fresh 榜 + W2c 10 种子证据 + 终报告 |

---

*本报告数字为一次性验证运行，不再更新。F1 配置未因本战役任何结果变更。*
