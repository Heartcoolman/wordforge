# AMAS 第四代调参战役终报：冲榜 Borda 第 1（2026-06-11 → 06-12）

> 分支：`tune/amas-fsrs6`
> 战役 commit 链：aa7e366 → f9d506d → f3cb1a6 → 90ebbcb → 3a367f4 → 4287a78 → b2b0601 → feb1f74（test 终评）
> 预注册文档：[00-pre-registered-gates.md](./00-pre-registered-gates.md)（搜索启动前提交，闸门数值冻结）
> 终榜：[docs/algo-bench-2026-06-12-d5-ship/01-leaderboard.md](../algo-bench-2026-06-12-d5-ship/01-leaderboard.md)
> 战役起点基线：[docs/algo-bench-2026-06-11-constrained-mirror/01-leaderboard.md](../algo-bench-2026-06-11-constrained-mirror/01-leaderboard.md)
> 上代背景：[docs/amas-tuning-2026-06-10/01-final-report.md §9](../amas-tuning-2026-06-10/01-final-report.md)（镜像对齐、amas 历史数字作废、alpha 平滑 4× ICI 课题）

---

## 1. TL;DR

**目标**：amas（生产语义条目）在 10 条目排行榜（含参考实现条目）Borda 计数登顶第 1。

**结果**：第 1 未达成。所有已探索通道（w 空间组合、count 挂靠 ramp、RGP 评分提升、
策略层 MGR/保持率方向）逐一触发预注册止损；最终以 D5 双腿信任调度作为船值收官，
TEST split 一次性终评定格：

| 排名 | 条目 | Borda（test 终榜） | 战役起点 |
|---|---|---|---|
| 1 | fsrs45 | 28 | 29 |
| 2 | amas6（纯 FSRS-6 参考） | 25 | 26 |
| **3** | **amas（生产语义）** | **24** | **18（第 4）** |
| 4 | fsrs6（纯参考） | 21 | 21 |
| 5 | fsrs | 20 | 14（第 7） |

**战役净增量**：amas Borda 18→24（第 4→第 3）；**maimemo 数据集排名 3→1，生产语义条目
首次在最大、最真实的数据集（232M reviews）登顶**，fsrs45 让位第 2（final_score 0.971 vs
0.970）。amas maimemo TEST 全指标同向改善：

| 指标 | 战役起点 | 终评 | 变化 |
|---|---|---|---|
| mastered | 14787 | 16999 | +15.0%，且 > fsrs45 的 15922 |
| logLoss | 0.3659 | 0.3364 | −8.1% |
| ICI | 0.1091 | 0.0795 | −27.1% |
| retentionStability | 0.8992 | 0.9070 | +0.9% |
| reviewsPerDay | 1996 | 1785 | −10.6%（更少复习换更多掌握） |

duolingo mastered 387→2770（×7.2）、synthetic 5102→12512（×2.45）、synthetic AUC
0.4784→0.5441——**首次高于随机线**。

**诚实上限**：与 fsrs45 的 Borda 差距（24 vs 28）不是"再调一轮 w"能填的——组合墙
（§2.5）证明 mastered 与 calibration 在 w 空间梯度相反，策略通道闭环符号翻转（§2.6）
关闭了调度侧捷径；且 maimemo logLoss 腿对 fsrs45 的 val 微弱优势在 test 上翻转
（§2.8，预警兑现，照实披露不回调）。残余差距属架构与评测口径层面，已记入遗产清单（§6）。

---

## 2. 时间线与决策链

### 2.1 D1：bench 保真对齐（aa7e366 + f9d506d）—— maimemo 腿翻转

上代（§9，06-10/11）镜像对齐已让 amas 条目如实反映 mdm.rs 生产语义，但重放侧仍残留
两处失真：

1. **评估采样扫描序抖动**（aa7e366）：ARG_MAX 采样依赖扫描序，改内容序复合键后样本
   规格可逐位复现（pinned sample specs，§3-4 的源头）。
2. **重放 alpha 冻结**（f9d506d）：旧重放钉死 alpha=0.3，而生产 mastery.rs 是连击动态
   alpha；对齐生产连击动态 alpha，并移除评估侧 interval_scale 分带 clamp（算法中性，
   amas6 实测 val 零影响，见 §5-4）。

**结果**：保真对齐后 maimemo 腿盈亏翻转——amas 在主战场数据集上由守转攻，此后所有
通道的胜负都在这条对齐后的口径上判定。这是战役的地基：先让尺子准，再谈冲榜。

### 2.2 D2：count 挂靠 ramp 被双尺度矛盾否决（f3cb1a6 + 90ebbcb + 3a367f4）

针对上代最大课题（alpha 平滑长历史端 ~4× ICI 差距），引入证据递减阻尼
`alphaRampTau`——alpha 随**累计复习次数**指数爬坡（count 挂靠），并完成生产化三件套：
结构旋钮默认 0.0 关闭（f3cb1a6）、Rust 直测 + 入口 clamp 镜像 + parity 判别族（90ebbcb）、
调参管道升级（tau 入搜索 + mastered 感知目标 + 预注册闸门，3a367f4）。

**否决证据（双尺度矛盾）**：

| 测量尺度 | count 挂靠 ramp 的效果 |
|---|---|
| 尾步采样（每词最长前缀） | **+41.8%** |
| 全前缀展开 | **−24.6%，ICI 恶化 3.3×** |

同一份改动，两把尺子给出符号相反的结论——尾步采样只看长历史端点，掩盖了 ramp 在
短中前缀端对失败证据的同步阻尼灾难。count 挂靠形态被否决，**双尺度测量自此成为硬性
要求**（§3-1）。

### 2.3 D5：根因诊断（单侧性领悟）与双腿修复（4287a78）

**诊断**：count 挂靠的致命缺陷是**无差别**——它把成功证据与失败证据一起阻尼。长历史
端"预测系统性偏低"的病灶只需要阻尼成功侧的过度增益，而阻尼失败侧等于削弱遗忘惩罚、
直接破坏校准。

**因果证明（对称反转实验）**：把阻尼分别单独挂在成功侧 / 失败侧测量，两腿贡献符号
相反——增益全部来自成功侧阻尼，损失全部来自失败侧阻尼。这不是相关性叙事，是消融级
因果定位。

**修复 = 双腿信任调度**：

- **成功腿**挂靠 correct_streak（失败清零→阻尼重启）：连击越长，新一次成功的边际证据
  越接近满额；一次失败后重新从低 alpha 爬坡——"信任要重新挣"。
- **失败腿**挂靠累计 lapse（首次失败 no-op）：偶发失误全额保护，反复失败（leech）才
  加速压低 S——"失败不被阻尼，只被甄别"。

形式化定义见 §4。镜像与 Rust 同步移植，elif 互斥（成功永不吃失败腿 ramp）、
advance-before-update 记账时序与 Python 镜像逐位一致。

### 2.4 RGP 评分提升通道：mai 优秀，但揭开 D-drift 暗通道——拒收

RGP（R 门控评分提升，`easyPromotionRThreshold`：预测保持率 R 高于阈值 θ 时把成功
评分 Good 提升为 Easy）在 maimemo 上单看预测腿表现优秀，但深查发现其增益路径经由
**难度漂移（D-drift）暗通道**：Easy 评分持续压低 D，等效于给 (11−D) 增长项开后门，
属守门体系未覆盖的自由度；且 θ 落入 retention band 以下时存在 hazard zone（低 R 词
被错误提升，加速崩坏）。**拒收，`easyPromotionRThreshold` 不随船**（4287a78 commit
message 留痕："RGP 止损"）。

### 2.5 组合墙：mastered ⊣ calibration，w 空间梯度对立

带 mastered 感知目标（3a367f4 预注册：`prediction_composite + 0.5×min(masteredProxy
比率−1, 1.5)`）的 w 探针扫描显示：**推高 mastered 的 w 方向与改善 calibration（ICI/
logLoss）的 w 方向系统性相反**。不存在同时推进两腿的 w 组合——这堵墙宣判了"靠 w 搜索
继续逼近 fsrs45"路线的终结，剩余差距必须来自结构（如 D5 双腿）或架构（§6 遗产）。

### 2.6 策略通道 NO-GO：MGR 闭环符号翻转 + D7 首事件发现

- **MGR（成熟度感知保持率调度）**：静态重放评估显示正收益，闭环模拟中**符号翻转**——
  静态尺子上的"更优调度"在闭环里因复习预算再分配而反噬 mastered。静态收益不可外推
  闭环（§3-3）。
- **D7 发现：闭环首事件恒为失败**（first-event-always-failure）——闭环模拟中每张卡的
  首次事件总是失败（引入即测、必不会），S0 初始化永远走 Again 分支，**w1-w3（Hard/
  Good/Easy 首评初始稳定性）在 S0 全死**。此前策略探针中 w1-w3 的"无效果"不是参数
  不重要，是闭环协议性死维——该发现把一批候选方案直接判定为不可测而非无效。

两项叠加，策略通道整体 NO-GO。MGR 本体以"校准中性、默认关闭"的生产可选旋钮形态
留存（§6-1），不作为本战役冲榜手段。

### 2.7 止损确认与船值选型（b2b0601）

至此所有已探索通道均触及预注册止损：count 挂靠 ramp 否决（§2.2）、RGP 拒收（§2.4）、
w 组合墙（§2.5）、策略通道 NO-GO（§2.6）。**止损是预注册纪律的执行，不是士气判断**——
通道关闭由数据触线决定。

唯一全须全尾通过闸门的结构改动 = D5 双腿信任调度。船值选型在两个决赛候选间按
预注册判据判定：

| 候选 | (τ_s, τ_f) | 判定 |
|---|---|---|
| dual(3,6) | (3.0, 6.0) | **胜出**，全闸门通过 |
| dual(3,5) | (3.0, 5.0) | **出局**：maimemo logLoss 硬下限 0.3328，实测 0.333237 越线 |

dual(3.0, 6.0) 写回 `amas_config.toml` + `config.py` DEFAULT（b2b0601），NM0 w 不动，
FSRS_BASELINE 保持双旋钮 0.0（竞品隔离）。

### 2.8 TEST 一次性终评与 val→test 迁移审计（feb1f74）

10 算法 × 3 数据集单次全量重生成（BENCH_RUN_DATE=2026-06-12，seed 42），TEST split
此前从未触碰，评估一次、不回调、不重跑。终榜见 §1。迁移审计逐腿核验：

| 腿 | val→test 迁移 | 判读 |
|---|---|---|
| mai mastered | **保持**：16999 > fsrs45 15922，相对 val −1.1% | 主胜负腿干净成立 |
| syn mastered | 名义 −23.2% | **规模伪影**：test 85 用户 vs val 110；人均归一后 −0.7%，干净 |
| duo mastered | +14.9% 正向漂移 | 顺风，照实记录 |
| mai logLoss vs fsrs45 | **翻转**：amas 0.3364 vs fsrs45 0.3230 | val 余量仅 +0.000193，选型时即预警为噪声级；按预先承诺照实披露，**不回调、不重调** |

mai logLoss 腿翻转不改变数据集排名（amas maimemo final_score 0.971 仍第 1——胜负由
mastered/DHP/policy 三腿合成扛住），但它是本战役"诚实上限"的具象：对 fsrs45 的预测腿
优势在 val 上就只有噪声级余量，test 如实暴露。

**amas 分数据集 / 分维度位次变化（战役起点 → 终评）**：

| 切面 | 起点 | 终评 | 备注 |
|---|---|---|---|
| maimemo 数据集 | 3 | **1** | final_score 0.925→0.971，挤掉 fsrs45（0.970）与 dhp |
| duolingo_hlr 数据集 | 8 | 4 | final_score 0.539→0.670，mastered ×7.2 驱动 |
| synthetic 数据集 | 4 | 4 | final_score 0.487→0.599，位次未动但与第 3 名差距收窄 |
| Prediction 维度 | 4 | 4 | logLoss 均值 0.506→0.498，ICI 均值 0.222→0.218 |
| DHP 维度 | 6 | 4 | dhp_score 均值 0.387→0.609，masteredCount 均值 6759→10760 |
| Policy 维度 | 7 | 6 | retentionStability 均值 0.841→0.881 |

三个维度全部不降、两个上行——Borda +6 不是单点偏科，是三腿同向；但每个切面对 fsrs45/
amas6 的剩余差距也同表可见，与 §1"诚实上限"互为印证。

---

## 3. 方法论产出（可迁移资产）

本战役比船值更值钱的是六条已被实例验证的纪律：

1. **双尺度测量要求**。尾步采样与全前缀展开必须同报：D2 实例中两者结论符号相反
   （+41.8% vs −24.6%/ICI 3.3×）。任何只报单尺度的"长历史改善"主张默认不可信。
2. **预注册闸门 + 止损纪律**。搜索空间、目标函数、守门下限、胜者级二元闸门在搜索启动
   前写定提交（00-pre-registered-gates.md），启动后不得修改；止损同样预注册——通道
   关闭由触线决定，杜绝"再试一个变体"的沉没成本陷阱。dual(3,5) 被 0.000037 的硬下限
   差距淘汰，正是该纪律的执行样本。
3. **static vs closed-loop 符号翻转**。静态重放收益不可外推闭环（MGR 实例）：凡改变
   调度决策的方案，闭环模拟是唯一裁决尺度。闭环协议本身也要审计——D7 首事件恒失败
   的发现说明闭环可能存在协议性死维（w1-w3 S0-dead），"探针无效果"先排除"探针不可达"。
4. **样本规格钉死**（pinned sample specs）。评估采样改内容序复合键（aa7e366）后样本
   集合与扫描序解耦，跨进程、跨轮次逐位可复现，对拍测试才有意义。
5. **竞品隔离**。`FSRS_BASELINE_CONFIG` 显式钉死官方 w/曲线与双旋钮 0.0，不随 DEFAULT
   写回漂移；对照臂的任何移动都必须是显式决定而非继承副作用。
6. **test 一次性纪律 + 人均归一化迁移审计**。test 评估一次、结果照实披露（含不利翻转）、
   绝不回调；迁移审计必须区分真实漂移与规模伪影——syn mastered 名义 −23.2% 在人均
   归一后只剩 −0.7%，不做归一会误判为过拟合。

---

## 4. Ship 配置与生产语义

### 4.1 公式（D5 双腿信任调度）

记基础平滑系数 α（生产 alpha 体系给出），有效平滑系数 alpha_eff 按评分分腿：

- **成功腿**（grade ≥ 2，挂靠 correct_streak，失败清零）：

  ```
  alpha_eff = 1 − (1−α) · e^{−(max(streak,1)−1)/τ_s},   τ_s = alphaRampTau = 3.0
  ```

  streak 为本次记账后的连击数（advance-before-update）；失败清零 → 阻尼重启；lapse 后
  同日成功 streak=1 ⇒ 指数项 e^0=1 ⇒ no-op。

- **失败腿**（grade == 1 Again，挂靠累计 lapse）：

  ```
  alpha_eff = 1 − (1−α) · e^{−(max(lapses,1)−1)/τ_f},   τ_f = alphaLapseRampTau = 6.0
  ```

  首次失败 lapses=1 ⇒ no-op（偶发失误全额保护）；反复失败加速实化遗忘惩罚（leech 甄别）。

- 两腿 elif 互斥：成功路径永不进入失败腿 ramp。

### 4.2 旋钮与兼容性

- 船值：`alphaRampTau = 3.0`，`alphaLapseRampTau = 6.0`（amas_config.toml + config.py
  DEFAULT，commit b2b0601）。
- **serde 默认 0.0 = 精确冻结语义**：DB 旧快照、未声明配置反序列化即得旧行为（τ=0 时
  两腿恒等于基础 α），存量数据零迁移。
- 新入口 `update_strength_with_evidence(state, quality, alpha, streak, lapses, now,
  config)`；旧 5 参 `update_strength` 保留为**中性证据包装**（streak=0/lapses=0 ⇒ 两腿
  恒 no-op，即便 τ>0 也精确冻结），约 30 个既有调用点零改动。
- 生产接线：mastery.rs 传记账后 correct_streak 与 `total_attempts − total_correct`。

### 4.3 验证闸门

- parity：52 例双腿判别族（冻结 / 成功腿单开 / 失败腿单开 / 双开 ship 候选 / legacy
  19 维正交），Python↔Rust max state err **1.4e-14**（容差 1e-9，余量 5 个量级）。
- `cargo test --lib` 841 通过；pytest benchmarks 50 通过；Rust 门禁测试钉死仓库根
  amas_config.toml 加载 + validate + 双旋钮船值。
- 选型判据：dual(3,6) 按预注册闸门胜出；dual(3,5) 在 maimemo logLoss 硬下限出局
  （0.333237 > 0.3328）。

---

## 5. 完整披露清单

1. **tau 先验选择已消费 duo/syn val**：alphaRampTau 窗口 (1.5, 4.0) 与 tau∈{1.5, 2.0}
   的排除依据来自 duolingo/synthetic val 网格——这两个数据集对"tau 维先验"非盲；对
   最终胜者仍仅以预注册二元闸门进入。
2. **oracle 跨数据集复用**：maimemo 训练的 GRU oracle 软链到 duolingo/synthetic 作
   forward simulator。**duolingo mastered ×7.2 应解读为"该 harness 口径下的策略优势"，
   不是经验证的真实记忆改善**——oracle 对 duo 学习者动力学的保真度未独立验证。
3. **synthetic 与守门同源**：synthetic 数据集的 ground truth 动力学与调参守门用的
   DHPStudent 共享模型族，syn 增益存在同源放大的可能，权重应低于 maimemo。
4. **amas6 继承分带 clamp 移除**（f9d506d 评估侧变更）：实测对 amas6 val 数字零影响，
   终榜以单次运行整体再生成。
5. **fsrs 条目 lockstep-by-design**：fsrs 条目与 amas 共享生产镜像语义（仅 w/曲线为
   FSRS-5 官方值），D1/D5 的口径与结构改动同步作用于它——其 Borda 14→20 与 amas 同步
   上行是设计使然，不是独立旁证。
6. **mai logLoss 腿 test 翻转**：amas 0.3364 vs fsrs45 0.3230；val 余量 +0.000193 选型
   时即预警为噪声级，按预先承诺照实披露，未做任何 test 后重调（§2.8）。
7. **duo 6-10 桶 ICIeqf +17%**：duolingo 中等历史长度桶的等频校准已知瑕疵，随结果
   披露，未遮蔽。
8. **跨进程非确定性 ≤0.7%**：闭环 mastered 在 torch/BLAS 跨进程下存在 ≤0.7% 抖动；
   mai mastered 胜负余量（16999 vs 15922，+6.8%）远超该噪声带，duo/syn 倍数级差距
   不受影响。
9. **leaderboard md 生成器叙事文本过期**：终榜 md 的散文段落含陈旧模板（"第 1 得 N 分，
   N=8"等，实际 10 条目）；**以 `benchmarks/results/2026-06-12-d5-ship/*.json` 原始
   数字为准，勿引用 md 散文**。

---

## 6. 遗产清单（勿回捡 / 留存资产）

| # | 项目 | 处置 | 关键数字 / 备注 |
|---|---|---|---|
| 1 | **MGR 成熟度-保持率调度** | **资产**：生产可选旋钮，默认 0 | 校准中性已位级验证；开启可降复习负担 −12~−18%；因闭环符号翻转**不作为冲榜手段回捡**，作为"省复习量"产品旋钮独立评审后可用 |
| 2 | **D7 难度先验发现** | **资产**：未来通道 | 首事件恒失败暴露的难度先验偏差是真实偏差修正机会，属 prediction 腿通道，下一代候选 |
| 3 | RGP 评分提升 | **勿回捡** | D-drift 暗通道未被守门覆盖 + θ < retention band 的 hazard zone（§2.4） |
| 4 | retention-flip 方向 | **勿回捡** | 两轮独立评估两次拒收，证据闭合 |
| 5 | count 挂靠 ramp | **勿回捡** | 双尺度矛盾否决（§2.2）；任何"挂靠累计次数"的 ramp 变体先过双尺度测量再谈 |
| 6 | 死维地图 w1/w3/w15/w16 | **资产**：协议级事实 | w1/w3 闭环 S0-dead（D7 首事件发现）、w15/w16 评分不可达；改闭环协议（引入前测/Easy 路径）之前对这些维度的搜索是浪费 |
| 7 | 指标体系批判附录 | **资产** | [05-appendix-metric-critique.md](../algo-bench-2026-06-12-d5-ship/appendix/05-appendix-metric-critique.md)（含 alt_board.py 替代加权复算）——下一代评测口径修订的输入 |

**下一代的真实杠杆**（按本战役证据排序）：D7 难度先验偏差修正（prediction 腿，未消耗）、
闭环协议修订解锁 w1-w3（先修协议再搜参数）、alpha 体系结构评审（上代 §9.7-1 课题在
D5 双腿后剩余量待重测）。w 空间常规重搜**不在**此列——组合墙（§2.5）与"公版即最优"
（上代 §9.4）两代证据一致。

---

## 7. 产物与复现索引

| 产物 | 位置 |
|---|---|
| 预注册闸门（搜索前冻结） | `docs/amas-tuning-2026-06-12/00-pre-registered-gates.md` |
| TEST 终榜（md，叙事文本见 §5-9 警告） | `docs/algo-bench-2026-06-12-d5-ship/01-leaderboard.md` |
| TEST 原始指标 JSON（**引用数字的唯一权威源**） | `benchmarks/results/2026-06-12-d5-ship/*.json`（30 个 algo×dataset 组合） |
| 战役起点基线榜 | `docs/algo-bench-2026-06-11-constrained-mirror/01-leaderboard.md` |
| 指标体系批判附录 + 替代加权复算 | `docs/algo-bench-2026-06-12-d5-ship/appendix/` |
| 船值配置 | `amas_config.toml` / `benchmarks/maimemo/config.py` DEFAULT（commit b2b0601） |
| 双腿实现 + parity 套件 | `src/amas/memory/mdm.rs`、`benchmarks/maimemo/dhp_reference.py`（commit 4287a78） |

```bash
# 终榜再生成（与 feb1f74 等价；TEST 已消耗，仅作核对用，结果不具新证据效力）
source .bench-venv/bin/activate
BENCH_RUN_DATE=2026-06-12 python -m benchmarks.maimemo.cli leaderboard \
  --results benchmarks/results/2026-06-12-d5-ship \
  --out docs/algo-bench-2026-06-12-d5-ship
```

战役 commit 链与各步对应关系：

| commit | 战役步骤 |
|---|---|
| aa7e366 | D1：评估采样内容序复合键（样本规格钉死） |
| f9d506d | D1：重放保真对齐生产连击动态 alpha + 分带 clamp 移除 |
| f3cb1a6 | D2：alphaRampTau 结构旋钮（count 挂靠，默认关闭） |
| 90ebbcb | D2：Rust 直测 + 入口 clamp 镜像 + parity 补盲 |
| 3a367f4 | D2/D4：调参管道升级 + mastered 感知目标 + 预注册闸门 |
| 4287a78 | D5：双腿信任调度（语义替换 + 失败腿新旋钮） |
| b2b0601 | D5：dual(3.0, 6.0) 选型写回 |
| feb1f74 | Phase 5：TEST 一次性终评 |

---

*本报告记录 2026-06-11 至 06-12 战役全程。TEST 数字为一次性终评，不再更新；后续任何
重评估须开新一代文档，不得覆写本文。*
