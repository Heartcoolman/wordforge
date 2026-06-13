# AMAS v5 加固战役：抗过拟合验证预注册协议（2026-06-13）

> 分支：`tune/amas-fsrs6`
> 性质：**验证，非选型**。F1 船值配置（4091e9a/a42ab76 落地）全程冻结——无论本战役任何结果如何，
> 不回调任何参数；负面发现照实写入终报告，无豁免语言。
> 本文档在全部新增评估运行启动前提交，判据与种子清单冻结，启动后不得修改。
> 上代终报告：[../amas-tuning-2026-06-12-rank1/01-campaign-final-report.md](../amas-tuning-2026-06-12-rank1/01-campaign-final-report.md)

---

## 1. 动机与范围

终榜（TEST Borda 30/30）的残余过拟合风险有四条（终报告 §5）。本战役在开发阶段可压缩其中三条：

| 工作流 | 打击的风险 | 状态 |
|---|---|---|
| W1 新鲜队列终验 | TEST 二次观察 | 本协议预注册后执行 |
| W2a alt-board oracle 口径复算 | mastered 自报口径博弈 | **已执行**（旧数据离线复算，见 §5） |
| W2b 平台期分析 | 搜索面针尖过拟合 | **已执行**（旧数据离线分析，见 §5） |
| W2c 种子 3→10 | G5 豁免统计瑕疵 | 本协议预注册后执行 |
| W3 生产日志重放 | 基准 ≠ 生产 | 工具落仓；实测阻塞（§6） |
| 盲点探针 ×3 | W2b 遗留覆盖缺口 | 诊断性，非闸门（§4.3） |

「真实用户对调度变更的行为反应」只能由灰度回答，超出本战役边界。

## 2. W1 新鲜队列终验（冻结判据）

### 2.1 队列构造

- **maimemo / duolingo_hlr**：官方仪器 `run_leaderboard.py` 不变协议（test split、n_users=300、
  seed=42、10 算法），新增算法中性 `exclude_users` 参数排除**全部历史 board 协议运行消耗的
  队列**。侦察已验证：两条采样路径对 (seed, n_users, split) 完全确定，全部历史 board 运行
  （2026-06-13-gsp-ship、2026-06-12-d5-ship、06-10/06-11 三榜）坍缩为同一个 seed-42 300 人集合；
  仅换种子不能保证不相交（duo 期望重叠 ≈142 人），必须显式排除。
- **synthetic**：test 池 85 人已全量消耗 ≥4 次，无不相交子集。改为**新种子重造宇宙**：
  `benchmarks/synthetic/generate.py --seed 777` 生成到独立 root，**复制** seed-42 root 的冻结
  GRU oracle 与 DHP 缓存（oracle 特征 user-agnostic，仅 history；不重训，避免"新用户"与
  "新模拟器"混淆）。通过 schema/规模/取值域完整性校验后用官方仪器评估。

### 2.2 判据（冻结）

- **主判据**：all-fresh 三数据集榜（fresh mai + fresh duo + fresh syn-777）上 amas **严格
  Borda 第一，且三数据集各 rank 1**。
- **副观测**（照实报告，不构成回调依据）：各 final_score 余量；amas 各腿相对 ship 榜的漂移；
  闭环腿 ≤0.7% 既有噪声带内的波动不作过拟合解读，rank 变动一律照实报告。
- 任何数据集 rank 跌出第 1 → 该结果按预注册措辞写入终报告："F1 在新鲜队列上未复现该数据集
  第一"，F1 保持冻结，处置（是否影响发版）交用户决策。

### 2.3 预披露的构造性限制

1. mai test split 曾被前代 **pipeline 级**评估触碰（`cli.py evaluate --split test`，100%@4M 行，
   精确用户集不可重构）——故新鲜性声明为「与全部 board 协议运行不相交」，**不是**「从未被任何
   评估观察」。
2. duo fresh 闭环队列为近普查（632−300=332 人中取 300），pred 腿阈值将拉入 bucket 19——
   协议忠实但 sim/pred 队列不再重合（ship 运行中重合），照实报告。
3. syn-777 与 seed-42 宇宙同 DGP 同 oracle，验证的是「同分布新用户」，不验证分布迁移；
   85 人小样本的 day-0 方差结构性限制沿用 G4/G5 豁免论证。

## 3. W2c 种子稳定性 3→10（冻结协议）

- **种子清单**：`S10 = {42, 7, 2026, 0, 1, 2, 3, 4, 5, 6}`（新增 7 个 = 规范自然数序列，
  可审计无挑拣）。运行后不得增删替换，10 个全部报告。
- **Tier 1（方法延续）**：`gsp_search.py` amas-only + what-if 注入固定 seed-42 val 榜，
  复用 3 个缓存 G5 行 + 新跑 7 种子。度量 amas 侧噪声。
- **Tier 2（方法改进）**：`run_val_board.py` 增加算法中性 `--seed`（默认 42 行为不变），
  10 种子各跑全量 10 算法 val 榜——修复既有 3 种子方法「竞品冻结、低估全板噪声」的缺陷。
- **G5X 公式**（每 tier × 每数据集，n=10）：
  - G5-rank：rank 1 计数 == 10/10（同时报告计数本身；10/10 对应零假设 P≈2⁻¹⁰）；
  - G5-margin：margin_mean ≥ 2 × margin_std（sample std，ddof=1；rank 1 种子记
    amas_to_next，失利种子记负缺口）；
  - 两 tier **永不混池**（估计量不同：amas-only 噪声 vs 全耦合噪声 + master-rng 抽取位次不同）。
- **预注册回报措辞**（若 duo 复现「rank 稳 / margin 噪」）：照实分别报告 G5-rank PASS 与
  G5-margin FAIL，解读固定为「duo 第一名次种子稳健；胜出幅度在种子噪声内不可分辨」。
  无豁免语言，不重掷，不换种子。
- **已知低信息腿**：syn 110 val 用户 < 300 采样 ⇒ pred 腿逐位同种子，margin 腿仅剩 Bernoulli
  噪声（std≈6.7e-5），其预期 PASS 标记为饱和、低信息量。

## 4. 实现与运行约束

### 4.1 评估侧改动（算法中性，默认行为不变）

1. `simulate.py::_sample_users` / `evaluate_scheduler.py::_sample_test_rows` /
   `run_leaderboard.py`：可选 `exclude_users`（默认 None ⇒ 与现状结果恒等；排除注入 bucket
   计数查询与主查询两处；选后断言零重叠）。
2. `run_val_board.py`：可选 `--seed`（默认 42 ⇒ 行为不变；统一馈入共享采样与全策略 rng 线程，
   无算法被特殊化）。
3. 竞品实现与计分代码零改动；fsrs45/fsrs6/amas6 公版硬编码不动；fsrs 保持 FSRS_BASELINE 绑定。

### 4.2 运行纪律

- 全部新增运行在本协议提交**之后**启动；结果目录：
  `benchmarks/results/2026-06-13-hardening-fresh/`（W1 mai/duo）、
  `benchmarks/results/2026-06-13-fresh-synthetic/`（W1 syn-777）、
  `benchmarks/results/2026-06-13-hardening-seeds/`（W2c）。
- 每类评估一次性运行，不重掷；运行失败（崩溃/中断）可重启，已完成的评估结果不丢弃。

### 4.3 盲点探针（诊断，非闸门）

W2b 平台期分析的三个 dist-1 覆盖缺口，各以 val seed-42 单点补测：`F1+streak3`、`F1+floor28`、
`F1+grade3`。仅补全敏感度图谱（预期 floor28/grade3 显著塌落 = 机制悬崖确认），结果不触发
任何 F1 变更。

## 5. 已执行项的预注册豁免说明

W2a（alt-board 复算）与 W2b（平台期分析）在本协议提交前已完成——两者均为**对既有已提交
数据的离线分析**，不产生新的数据观察，不存在「先看结果再定判据」的污染面。结果：

- W2a：amas 在 oracle 加权口径 Borda 29/30 第一（无平局；duo 腿 −0.0058 输 fsrs6 照实记录），
  纯 expectedMemoryFinal 口径 Borda 24 第一；自报 mastered 与 oracle 真值同向改善。
  产物：`benchmarks/results/2026-06-13-hardening/altboard/`。
- W2b：判定 plateau 非 needle（cap 40–50 全 Borda 30、fuzz 0–0.20 平、分带全关仍 30；
  离散机制悬崖 ≠ 连续精调针尖）。产物：`02-plateau-analysis.md`、
  `benchmarks/maimemo/plateau_analysis.py`。

## 6. W3 生产日志重放（工具交付，实测阻塞）

完整设计与工具落仓 `benchmarks/prod_replay/`（export.sql / configs.py / build_parquet.py /
run_prediction.py / interval_shift.py，fixture 自测通过）。实测阻塞于两个事实，照实声明：

1. 8.135.57.148 经用户 2026-06-01 澄清**非生产、无真实用户数据**（仅压测假数据）——
   「真实分布校准」前提不成立；
2. 本机当前 SSH 密钥未获该主机授权。

如未来存在真实用户数据源，按本规格执行：预测腿 LL/ICI（OLD=`git show 4091e9a^:amas_config.toml`
语义 vs NEW=F1）配对自举 95% CI（n≥500 才报点估计，n≥1000 才报 ICI），调度爆炸半径
（90→40 重召回人群、毕业下限触发数、区间偏移直方图）为普查可任意 N 报告。

---

*判据与种子清单自本提交起冻结。执行结果无论正负，照实写入 01-report.md，不得回调 F1。*
