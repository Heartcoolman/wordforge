# AMAS 第五代战役：解除约束后的夺冠设计 + 预注册闸门（2026-06-12）

> 分支：`tune/amas-fsrs6`
> 授权：用户解除全部架构约束——「可任意调整 amas 的架构、算法及其参数，目的只有一个：amas 系统必须第一」。
> 上代背景：[../amas-tuning-2026-06-12/01-campaign-final-report.md](../amas-tuning-2026-06-12/01-campaign-final-report.md)（D5 船值止损收官 Borda 24 第 3）。
> 本文档在搜索启动前提交，闸门数值冻结，启动后不得修改。

---

## 1. 上代止损结论的失效边界（为什么本轮可以重开）

上代「mastered ⊣ calibration 梯度对立」是**单状态架构**内的定理：预测与调度共用一条
被 alpha 平滑的 S 轨迹，推 mastered 的参数方向必毁校准。本轮解除架构约束后：

- 评测管线中预测腿（重放）与 dhp/policy 腿（闭环模拟）**物理正交**——预测由状态
  模型决定，mastered/efficiency/rpd 由调度策略决定；
- 「策略通道 NO-GO」（MGR 闭环符号翻转、retention-flip 双拒）是在**软 S 动力学 +
  保校准约束**下的判决，不适用于「换硬动力学 + 显式毕业策略」的新象限；
- 因此重开不违反「勿回捡」纪律：回捡的不是旧通道，是旧通道从未覆盖的象限。

## 2. 侦察实证基础（2026-06-12，val split + what-if 复算）

工具：`benchmarks/maimemo/whatif_board.py`（逐位复现官方 TEST 榜 fsrs45 28 / amas6 25 /
amas 24）、`benchmarks/maimemo/diagnose_sim.py`（val 闭环显微）、生产实现图谱（递交于
本战役工作流记录）。关键数字：

1. **最小严格夺冠前沿**（pred 升级到 amas6 级 + 非 mastered 闭环腿 = amas6 级）：
   mai mastered 保持 16999 时，duo ≥ 6597、syn ≥ 26545 即 Borda 28 > fsrs45 27。
   mai mastered 底线 **16133**（对 fsrs45 15922 仅 211 词，最脆弱腿）；跌到 14000 则
   syn 需求爆炸到 63397+。
2. **policy/efficiency 免费杠杆**：同 mastered 下把 duo/syn totalReviews 压到 fsrs45
   量级 → Borda 28→30（efficiency↑ + rpd↓ 双腿正交增益）。
3. **平局语义**：榜单排序对相等 final_score 按文件名/字母序任意拆分——任何依赖打平
   的「第一」都脆弱。本战役判据一律 **Borda 严格大于全部竞争者**。
4. **机制归因**（val）：
   - mai：amas 赢 = 软 S 不过冲（warm-start ≥90d 越窗 2397 vs amas6 10704，与
     never-reviewed 精确 1:1）→ 复习面 +8307 → mastered +5518；
   - duo：amas 输 = 软 S 在 61.5% 失败率下永远爬不过 30d（72.6% reviewed 词钉死
     <7d，28.5 次/词重刷）；预算与 never-reviewed 均非瓶颈（saturation=0）；
   - syn：同因（fsrs45 45% 词进 ≥90d 带 vs amas 1.8%）；
   - AMAS ensemble 自适应头惰性（interval_scale ≈ 1.02 恒定、adaptive retention ≈
     0.85 不动）——不是杠杆，本轮不调。
5. **预测腿已知锚点**（TEST，amas6 = 无平滑公版 FSRS-6）：mai LL .3025/ICI .0252、
   duo .3068/.1177、syn .5089/.2112，三数据集全面优于 D5 船值——平滑体系实测为纯
   预测税。

## 3. 架构决策：v5 = 未平滑 FSRS-6 核心 + 毕业制调度策略头（GSP）

**单轨双用**（非 shadow 双轨——平滑关闭后两轨恒等，shadow 字段失去存在意义）：

- **状态核心**：FSRS-6 动力学，alpha 平滑关闭（用现有旋钮 `alphaMin=alphaMax=1.0`
  钉死 alpha_eff=1；`alphaRampTau=alphaLapseRampTau=0`，D5 双腿 ramp 自动 no-op，
  serde 兼容零迁移）。w 起点 = FSRS-6 公版（预测腿 = amas6 级，已知免费）；是否重调
  w 由 G3 闸门余量决定（默认不调——上两代「公版即最优」证据 + 本轮预测腿已够用）。
- **调度策略头 GSP**（Graduated Scheduling Policy，生产新旋钮，全部 serde 默认冻结
  旧语义）：
  1. `intervalCapDays`（区间帽，含 warm-start 重触语义）：单次区间上限从 90 下调
     （候选 {45, 60, 75, 89}）——把长寿词周期性拉回复习面，修 amas6 在 mai 的过冲
     失血（+~10k 复习面）；
  2. `graduationStreak` + `graduationFloorDays`：连击 ≥ k 且 S ≥ 下限时区间下限弹到
     ≥30d（候选 k∈{2,3}, floor∈{30,35}）——解 duo/syn 的 <7d 钉死；这是 Anki
     graduating interval 的标准设计，时间不变（禁止读模拟日/视界）；
  3. 成熟度分带保持率 `youngRetention`/`matureRetention`/`maturityBandDays`：年轻卡
     高保持率（密集巩固）、成熟卡低保持率（拉长间隔省复习量）——直攻 efficiency/rpd
     免费杠杆。
- **竞品隔离顺修**：fsrs 条目当前裸吃 DEFAULT（口径漂移：实为 FSRS-5+AMAS 旋钮），
  绑回 `FSRS_BASELINE_CONFIG`（新旋钮显式关闭）；fsrs45/fsrs6/amas6 硬编码公版 w，
  天然隔离，**一律不动**。fsrs 条目数字将因此变动，照实披露。

**完整性红线**：不伪造任何上报数字；策略必须时间不变（不得条件于模拟日期/剩余天数）；
不修改任何竞品实现与计分代码；评估侧只允许算法中性改动（split 参数等）。

## 4. 搜索协议

- **数据纪律**：迭代仅用 train/val（闭环 val split + 重放 val）。TEST 一次性终评。
  **披露**：TEST 已于上代 feb1f74 消耗一次，本轮目标即翻转该榜，属第二次观察——
  缓解 = 仅以 val 选型、闸门余量须覆盖已知 val→test 漂移带、终评仍一次性不回调。
- **目标函数**：候选 amas 腿值代入 **val 全量 10 算法榜**（val 重放 + val 闭环一次性
  建基线）的 what-if Borda，要求严格 #1；同分先比 Borda 余量，再比 mai final_score
  余量（最脆腿优先）。
- **分阶段**：S1 基线（公版 w + 平滑关，无 GSP）三数据集 val 全测 → S2 GSP 粗网格
  （闭环为唯一裁决，静态收益不外推）→ S3 决赛者精调 + 种子稳定性。

## 5. 预注册闸门（二元，全部通过才能进 TEST 终评）

| # | 闸门 | 数值 |
|---|---|---|
| G1 | val 榜严格第一 | 候选 Borda 严格 > 全部 9 竞争者，且每个贡献胜负的数据集 rank 的 final_score 余量 ≥ 0.005 |
| G2 | mai mastered 不退 | 候选 val mai mastered ≥ 17000（现 17183；这是全榜最脆的腿，禁止用其它腿补偿） |
| G3 | 预测腿守门 | 候选 val LL/ICI 每数据集 ≤ amas6 同 split 数字 × 1.02（amas6 = 无平滑公版锚） |
| G4 | policy 不塌 | 候选 val retentionStability ≥ {mai .895, duo .825, syn .885}（amas6 当前值 −1%） |
| G5 | 种子稳定 | 决赛候选在 seed {42, 7, 2026} 闭环重跑：G1 的胜负余量 ≥ 2× 跨种子 std |
| G6 | 镜像 parity | 生产化后 Python↔Rust 状态/区间对拍 max err ≤ 1e-9（含 GSP 判别族） |

**止损**：S2 网格 ≥ 60 个配置仍无候选同时过 G1+G2 → 停止加变体，向用户报告结构性
障碍与最近距离，不得静默放宽闸门。

## 6. 与生产的关系

胜者写回 `amas_config.toml` + `config.py` DEFAULT（amas 条目 = 生产语义不变式）。
生产行为实质变更：状态更新去平滑（预测更准）、调度引入毕业制（更少复习换更多长间隔）。
风险与回退：全部旋钮 serde 默认 = 冻结旧语义，回退 = 配置回滚，无 DB 迁移。
