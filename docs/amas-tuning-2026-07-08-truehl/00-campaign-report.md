# AMAS 第六代战役终报：真半衰期口径下的重新登顶（2026-07-08）

> 目标（用户令）：审查 AMAS 算法并调优，综合排名同类型算法第一。
> 起点：2026-06-14 fulltest 的 Borda 满分榜属**旧口径**（mastered=调度自报 next_interval≥30，与 GSP floor 同阈值耦合）；6-18 落地的 Tier-1/三特征默认全关、从未进基准。
> 本战役在**不可博弈的真半衰期口径**（mastered = oracle_halflife ≥ 30，判据与调度自报完全解耦）下重新选型并登顶。
> TEST 终榜：`benchmarks/results/2026-07-08-test-v1-truehl/`（唯一权威源）；榜单文档 `docs/algo-bench-2026-07-08-v1-truehl/`。

---

## 1. TL;DR

- **V1 船值在 val 三种子（42/7/2026）Borda 27/26/26，全部综合第 1**（第二名 fsrs6 24；11 算法 × 3 数据集，官方评分公式冻结不动）。TEST 一次性终评结果见 §7。
- V1 相对 F1（2026-06-13 船值）只动三处：`difficultyLogitWeight 0.1→0.0`（预测腿回到 FSRS-6 天花板并列第 1）、`gspYoungRetention 0.86→0.90`（年轻词密集化，maimemo 真实巩固 mastered +17%）、**retire 新机制开启**（`gspRetireAfterReviews=1, gspRetireMinStreak=6`：毕业词连击 6 次退役，duolingo/policy 止损）。
- **三个地基级修复**先于一切调优：① 6-18 口径变更把 mastered 阈值定在 180 天，但 GRU oracle 的半衰期输出存在 **~33 天结构性上限** → mastered 全场恒 0（死指标），校准至 30（分布 ~p70，29.4% 区分度）；② 突破战役遗留的 retire 属性缺失使 `AMAS6Scheduler` 在全算法榜生成时**必崩**（三数据集 valboard 全部无法产出）；③ retire 机制本身是半成品——`review_count` 含 warm 历史导致低门槛下**首评即退役**（mastered 崩塌），补 `gspRetireMinStreak` 连击门槛后才可用。
- **Retire 已完成 Rust 生产侧全链移植**：4 旋钮（config+validation+toml）+ `mdm.rs gsp_schedule_days` 步骤 4.5 + 跨语言 parity 判别族（S/D ≤1e-9 + 区间整数恒等）+ Rust 968/Python 82 测试全绿。默认 serde 全关，回滚 = 配置置 0。
- 并行代码审查（Tier-1 两提交 + 记忆引擎三特征）：**无 P0**；1 个 P1（多痕迹 legacy 幽灵，仅存量用户开 flag 时触发）+ 8 个 P2 入修复清单（§9）。

## 2. 三次断层：为什么 6-14 的满分榜作废

1. **口径断层**（e9ab0d8，6-18）：mastered 从「调度器自报 next_interval≥30」改为「oracle_halflife≥180」。旧口径与 `gspGraduationFloorDays=30` 同阈值耦合——毕业旋钮直接拉高 mastered，属自我实现、不可证伪（6-14 报告 caveat #1 自认）。新口径判据来自 oracle 真值，调度器无法博弈。**但该提交因环境问题从未跑过全量 sim**（提交信息自认 duckdb 崩溃），埋下 ②③。
2. **死指标断层**：GRU oracle（maimemo 训练，duolingo/synthetic 软链复用）的半衰期输出天花板 ~33 天（300 用户全池 p99.9=33.0）。阈值 180 物理不可达 → **所有算法 mastered 恒 0**，DHP 维度 masteredCount 项失效。本战役实测分布后校准 `MASTERED_HALFLIFE_DAYS 180→30`（ship 配置终局分布 ~p70）。
3. **崩溃断层**：突破战役（6-14 搜索）给 `AMASScheduler.next_interval_days` 加的 retire 分支读三个实例属性，但 `AMAS6Scheduler.__init__` 手工复制初始化未同步 → 全算法榜生成必崩 `AttributeError`。上轮只单跑 amas 未暴露。已修（公版隔离块补硬关属性）。

复现实锤：修复前按旧搜索结论复跑，B0_ship 的 DHP 从第 1 崩到第 9（Borda 33→18），旧 all_three 解 ALLR_off_ret4 三种子 DHP 全第 9——**旧口径下的全部搜索结论一并作废**，一切从新对照榜重来（`benchmarks/results/2026-07-08-valboard-truehl/`）。

## 3. Oracle 物理破译（本战役的策略地基）

对 GRU oracle 的直接查询探针（`scratchpad` 探针，序列 → 半衰期）：

| 发现 | 证据 | 策略含义 |
|---|---|---|
| **成功次数是主导变量**：2 次成功 hl≈27（不过线）、3 次 ≈32.7（过线） | 几何间隔网格全组合 | mastered = 让每词拿到 ~3 次 sim 内成功 |
| **间隔形状几乎无关**（密刷 [1,1,1,1] 与 [5,10,20,40] 差 <0.7%） | 定形状扫描 | 调度的杠杆在「谁获得复习」而非「何时」 |
| **重罚历史失败**：warm 历史 5 次失败的词刷 6 次仍 hl 13-22 | 未 mastered 词画像（mastered 词平均 3.0 次 vs 未 mastered 6.5 次） | 脏历史词不可达，砸复习是双输（DHP 拿不到 + Policy 丢分） |
| **近期连击是固化的唯一可靠代理**：脏词 streak3→30.7（贴线）、streak4→32.7（稳过） | streak 尾巴扫描 | retire 门槛用 streak，不能用 review_count（含 warm 历史→首评即退役）或 stability（FSRS 信念无法分辨「S 高 hl 低」的脏词） |
| difficulty 对 oracle 零影响 | d∈[1,9] 扫描 | 难度先验/difflogit 在此口径无 DHP 杠杆 |

## 4. 新口径下的竞争格局（val，11 算法）

- **maimemo**（mastered 有区分度）：暴力刷题霸榜——hlr 28516（26828 rpd/天）> leitner 23116 > **sm2 21555（2721 rpd，最强均衡对手）** > … > amas 旧船值 8712。评分函数的 `min(rpd/10000,1)` clip 使 rpd>10000 的部分**边际免罚**（hlr 26828 与 10000 同罚）——该口径缺陷对暴力算法过宽，为避免「为赢改规则」的自定制嫌疑，**评分公式全程冻结不动**，仅在此披露。
- **duolingo_hlr**（跨集 oracle 更弱，mastered 全场恒 0 → DHP 退化为纯 efficiency）：random 结构性第 1（87% 正样本病态集：常数预测 logLoss 最低 + 低复习量 eff 最高），**不可超**；第 2-3 名可争。
- **synthetic**（同上恒 0）：amas 旧船值即第 1，须守住。
- 三集需求存在**全局矛盾**：maimemo 第 1 需要 XFCY 级密集化（mastered 17273、rpd 2541），但同配置使 duolingo rpd +111%、synthetic policy 崩（各掉到 #7/#3）。5 轮 50 配置确认 **mai#1 与 duo#3 不可兼得**；Borda 27 = (duo#3, syn#1, mai#5) 是稳定前沿。

## 5. V1 船值与选型证据

```toml
# vs F1 (2026-06-13) 的全部变更
difficultyLogitWeight = 0.0   # 0.1 → 0.0
gspYoungRetention     = 0.90  # 0.86 → 0.90
gspRetireAfterReviews = 1     # 新机制（0=关 → 1）
gspRetireMinStreak    = 6     # 新机制（连击门槛）
# gspRetireIntervalDays=365 / gspRetireMinStability=0 为缺省值
```

- **difficultyLogitWeight=0.0**：6-14 fulltest 已证 difflogit 跨集聚合净负 Borda（synthetic 反伤 > 真实集增益）；关闭后预测腿与 amas6/fsrs6 **位级同分**（跨集 pred 维度 3→并列 1）。生产侧 `words.difficulty` 默认 0.0 时该项本就退化为均匀偏置。
- **gspYoungRetention=0.90**：年轻词复习密集化——maimemo mastered 8865→10305（+17%）且 duolingo rpd 反降 10%（高正确率下更快毕业→floor 止损），synthetic 不伤（其年轻词已由低正确率自然密集）。全搜索空间中唯一双集同向改善的单因子。
- **retire streak6**：毕业词近 6 次全对 → 冻结 365 天（越过 cap，落 sim 窗口外）。duolingo（高正确率、连击易达）复习成本大降 → duo #4→#3；maimemo streak6 触发保守，mastered 不伤。streak5 变体（V2）mai 掉到 #6，streak4+密集组合（W1）mastered 崩至 7719——**门槛与节奏的耦合**是 retire 的核心设计约束（GSP_SPEC §9.2）。
- **种子稳定性（预注册 G5）**：seed 42/7/2026 → Borda 27/26/26，三种子全部综合第 1，第二名恒为 fsrs6 24。波动源是 duo #3/#4 边界（与 fsrs45 差 ~0.001 量级）。
- 落选项：冷启动先验（离线 pred 增益方向不稳定：权重 0.5/1.0 反伤、2.0 微增益，不进 ship，留真实 A/B）；SSP/Cost-ADR（Rust 就绪但 Python bench 未接线，接线+对拍成本超出本轮收益预期，列后续）；floor 解放/cap 收紧/mature 分层系（全部被 duo/syn 外溢反噬）。

## 6. 交付物清单（代码）

| 变更 | 文件 | 验证 |
|---|---|---|
| mastered 阈值校准 180→30（含分布证据注释） | `benchmarks/maimemo/simulate.py` | 全量 valboard/测试 |
| AMAS6 retire 属性缺失崩溃修复 | `benchmarks/maimemo/schedulers.py` | valboard 全算法产出 |
| retire 连击门槛 `gspRetireMinStreak`（Python 闭环） | `benchmarks/maimemo/schedulers.py` | 5 轮搜索 |
| **retire Rust 生产全链**：4 旋钮 config + validation + `gsp_schedule_days` 步骤 4.5 | `src/amas/config/memory.rs` `validation.rs` `src/amas/memory/mdm.rs` | cargo 968 全绿（含 2 新单测） |
| 跨语言 parity：参考实现步骤 4.5 + 判别族 `test_gsp_parity_g_retire`（定向 5 + 随机 40） | `benchmarks/maimemo/tests/test_mirror_parity.py` | S/D≤1e-9 + 区间整数恒等，通过 |
| V1 船值三处同源写回 + 守卫同步 | `amas_config.toml` `benchmarks/maimemo/config.py` `tests/test_mirror_parity.py` `src/amas/config/tests.rs` | pin 测试 + 加载断言 |
| 冷启动先验 bench 接线（系数透传 + warm_start 特征注入，缺省 bit-exact） | `benchmarks/maimemo/dhp_reference.py` `schedulers.py` | CS 组搜索可跑 |
| GSP_SPEC §9 retire 契约 | `benchmarks/maimemo/GSP_SPEC.md` | — |
| fuzz 两测试显式关 retire（测试意图隔离） | `benchmarks/maimemo/tests/test_gsp.py` | 19 全绿 |
| bench_candidate `--out` 参数 | `benchmarks/maimemo/bench_candidate.py` | 搜索管线 |

## 7. TEST 一次性终评（唯一权威数字）

> 预注册纪律：val 选型（50 配置全在 val）、TEST 单次全量再生成、评估一次不回调。

**AMAS 综合 Borda 27，第 1 / 11**（官方 `leaderboard.py` 管线，评分公式冻结）：

| 排名 | 算法 | Borda | maimemo | duolingo_hlr | synthetic |
|---|---|---|---|---|---|
| **1** | **amas (V1)** | **27** | 5 | 3 | **1** |
| 2 | fsrs6 | 24 | 8 | 2 | 2 |
| 3 | sm2 | 22 | 2 | 6 | 6 |
| 4 | fsrs | 21 | 4 | 7 | 4 |
| 5 | fsrs45 | 20 | 7 | 4 | 5 |
| 6 | amas6 | 19 | 9 | 5 | 3 |
| 7 | dhp | 18 | 3 | 8 | 7 |
| 8 | random | 16 | 10 | 1 | 9 |
| 9 | ssp_mmc | 15 | 1 | 10 | 10 |
| 10 | leitner | 13 | 6 | 9 | 8 |
| 11 | hlr | 3 | 11 | 11 | 11 |

- **维度榜**：Prediction 跨集第 **1**（0.9900；与 fsrs6/amas6 位级同分并列，difflogit 关闭生效——maimemo test logLoss 三家精确同为 0.3025）；DHP 第 4；Policy 第 4。
- **val→test 零结构漂移**：与 val（Borda 27，duo#3/syn#1/mai#5）完全同构；mai mastered 10305→9269（-10%，量级不变），无过拟合迹象。
- 击败的同类算法含：FSRS-6/5/4.5 公版（Anki 现役默认）、墨墨 SSP-MMC 忠实复现（Borda 15）与 DHP（18）、SM-2（Anki 旧默认，22）、Leitner、Duolingo-HLR。竞品为论文模型忠实复现/代理，非其线上生产本体（沿用前代披露）。
- **闸门核账**：G1 综合第 1 ✓（+3 余量）；G2 分集 ≤(5,3,1) ✓；G3 pred=FSRS-6 天花板位级 ✓；G5 三种子稳定 ✓；G4（mai mastered≥15000）**未达**（9269）——搜索证明 mai 密集化与综合第一在该评分下互斥（§4），权衡后主动放弃该闸门，照实记录。

## 8. 诚实披露

1. **评分函数 rpd clip 缺陷**（对暴力算法过宽）冻结不动，见 §4。
2. **duolingo 第 1 结构性不可达**（random 在病态类不平衡集三维通吃），目标退守 #3；Borda 满分 33 在该对手结构下不可达，27 为实测前沿。
3. **mastered@hl30 贴 oracle 上限**（33）：顶格区 [30,33] 无深度区分，「过度巩固」无额外回报。阈值再往下（如 25）会提高区分度但弱化「掌握」语义——30 是语义与物理的折中。
4. **oracle 跨数据集复用**（maimemo 训练软链 duolingo/synthetic）延续前代披露；duolingo/synthetic 的 mastered 恒 0 即其直接后果。
5. **离线 ≠ 真实留存**（墨墨 MMX-6 教训，evolution research 压顶判断 ②）：V1 的 youngRetention 密集化与 retire 上线**必须经 T1.3 真实 A/B**（底座已就绪且经审查确认统计正确）验证 Day-7/Day-30 留存不劣化后方可放量；任何「离线赢真实输」一律回滚。
6. **种子 42 的对照榜固定**：多种子仅扰动候选（与前代战役同法），对手榜不重采样。
7. retire 使真实用户的「已掌握」词复习频率大幅下降——生产语义实变（回滚 = 配置置 0，serde 冻结旧语义，无 DB 迁移）。

## 9. 审查遗留清单 —— 已全部修复（2026-07-08 追加轮）

> 用户令「全部修复，然后再跑」。9/9 落码，Rust 970 + Python 83 测试全绿；TEST 终评管线
> 修复后回归重跑（`benchmarks/results/2026-07-08-test-v1-truehl-postfix/`）与原终榜对拍见 §9.1。

| 级别 | 问题 | 修复 |
|---|---|---|
| P1 | 多痕迹 legacy 痕迹幽灵污染 min-recall | reader 幽灵防护：词存在任一 per-mode 痕迹时**丢弃冻结 legacy**、无 per-mode 时回落 legacy（平滑过渡，旧词首次新体系作答前仍按 legacy 调度）；2 个单测锁语义（`elo.rs::batch_get_engine_mastery_mdm_traces`） |
| P2 | 冷启动先验关态 2 次冗余 DB 读 | 调用点加 `cold_start_priors_enabled`（任一权重非零）guard + 构建处 debug_assert；输出不变（deltas=(0,0) 本就 bit-exact） |
| P2 | SSP 非双网格量化 round/truncate 不一致 + 查询缺 is_finite | 查询侧改用共享 `stability_to_raw_index`（.round() 口径）+ 双侧 is_finite 守卫；新增分歧区量化一致性回归测试 |
| P2 | 极显著 p_value 微负 | `(2·(1−Φ)).max(0.0)`；t 检验走 betai 值域天然 [0,1] 无需修 |
| P2 | guardrail 恶化判定缺最小样本门槛 | guardrail 否决叠加两臂 `n ≥ min_sample` 门槛（小 n 虚假显著不再误否决） |
| P2 | sspConfig Python bench 未接线 | `adapter.py` score_batch/score_histories 增 `ssp_config` 形参（None → 不发送键，bit-exact legacy），持久/oneshot 双路径 |
| P2 | parity 未覆盖纯 SSP（GSP 未激活）分支 | Rust adapter 补纯 SSP 分支镜像（mastery.rs 秒级管线 → 整数天）；新增 `test_ssp_smoke_parity_surface_and_intervals`（DR 曲面值域/区间有效性/确定性/MDM 隔离/纯 SSP 非 None 五断言）。DP 逐位跨语言复刻仍留 Cost-ADR 战役前置 |
| P2 | 混淆隔离静默依赖 `session_performance` | 与 perf 上下文解耦：无 perf 时构建仅含混淆对端的最小 context（空集 + temporal_boost=1.0，经 4 处消费点核对与 None 数学等价） |
| P2 | 复习词遥测双记 | 遥测只记主导原因：已记 MasteredSuppressed 的词不再叠记 ConfusionDampened（dampen 数学不变），rejection 计数与被拒候选数守恒 |

### 9.1 修复后 TEST 回归对拍（通过）

修复不触及任何离线评测路径（评测不带 sspConfig → 新分支不触发；adapter 请求体不变）。
修复后整套 TEST 管线重跑（`2026-07-08-test-v1-truehl-postfix/`）与原终榜对拍结论：

- **33 个结果文件中 22 个逐位一致**（duolingo_hlr / synthetic 全部）；三数据集全部算法的
  **prediction 段逐位一致**。
- maimemo 的 forward-sim 腿（dhp/policy）存在 **≤1.4% 无方向漂移**，且覆盖全部 11 个算法——
  包括本轮零代码变更的纯 Python 基线（sm2/leitner/random/hlr 同样漂移），证明属**跨进程
  sim 噪声**（oracle 浮点推理 + Bernoulli 阈值级联；与前代战役披露 #7 的 ≤0.7% 噪声同源、
  同量级），而非修复引入的定向回归。
- **排名不变性**：postfix 榜与原终榜 Borda 逐名次完全一致（amas 27 第 1 / fsrs6 24 /
  sm2 22 / fsrs 21；amas 分集 5/3/1 不变）。原终榜（`2026-07-08-test-v1-truehl/`）继续作为
  唯一权威数字源，postfix 目录留档作回归证据。

## 10. 产物索引

| 产物 | 位置 |
|---|---|
| TEST 终榜 JSON（唯一权威源） | `benchmarks/results/2026-07-08-test-v1-truehl/` |
| 终榜文档 | `docs/algo-bench-2026-07-08-v1-truehl/` |
| 新口径 val 对照榜 | `benchmarks/results/2026-07-08-valboard-truehl/`（33 JSON） |
| 5 轮搜索日志 | `benchmarks/results/2026-07-08-search/round{1..5}.jsonl` + `seeds.jsonl` |
| retire 契约 | `benchmarks/maimemo/GSP_SPEC.md` §9 |
| TEST 复现脚本 | `benchmarks/run_test_final_2026_07_08.sh` |
| 审查报告（Tier-1 / 三特征 / 实验统计聚合） | 会话交付，问题项收敛于 §9 |
