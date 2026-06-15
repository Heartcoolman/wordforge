# 间隔重复算法调研：SM-2 / HLR / FSRS-4.5

> 调研日期：2026-05-27  
> 作者：researcher Agent（worktree: `/tmp/wordforge-wt-researcher`，branch: `agent/researcher`）  
> 目标：为后续与 WordForge AMAS（FSRS-5 等价实现）的 benchmark 对比提供权威算法定义、公式与实现入口  
> 范围：三个算法各自的论文/原始定义、核心公式、≥2 个主流开源实现、参数清单与默认值、与 FSRS-5/AMAS 的对比、已知缺陷  
> 引用规则：核心事实 ≥2 独立源；URL 直接给出；公式以 LaTeX 内联形式表达

---

## 0. 摘要与对比矩阵

三个算法分别代表间隔重复（Spaced Repetition）发展的三个时代：

| 维度 | SM-2（1987） | HLR（2016） | FSRS-4.5（2023-12） | FSRS-5（对照） |
|------|-------------|-------------|-------------------|---------------|
| 出身 | Piotr Woźniak 硕士论文 | Settles & Meeder, Duolingo, ACL | Jarrett Ye（L-M-Sherlock）社区 | 同 |
| 记忆模型 | 单一 EF（Easiness Factor） | 单一半衰期 $h$（log-linear 回归） | DSR 三元组（Difficulty / Stability / Retrievability） | DSR + 短期记忆启发式 |
| 遗忘曲线 | 隐式（不预测概率） | 指数 $p=2^{-\Delta/h}$ | 幂律 $R=(1+\frac{19}{81}\cdot\frac{t}{S})^{-0.5}$ | 同 4.5 |
| 训练方式 | 启发式规则（无训练） | SGD/AdaGrad 拟合 $\boldsymbol{\theta}$ | 二元交叉熵 + 梯度下降拟合 $w_{0..16}$ | 拟合 $w_{0..18}$ |
| 参数维度 | 0（仅每卡 EF/n/I 三状态） | $\|\boldsymbol{\theta}\| \approx 2\times 10^4$（含 lexeme 稀疏特征） | 17 维 | 19 维 |
| 输出 | 下次间隔 $I_n$ | 半衰期 $h$ + 回忆概率 $\hat p$ | $R(t,S)$ + 自定义 retention 求下次间隔 | 同 |
| 评级粒度 | 0–5（6 档） | 二值（recalled / not） | 1–4（Again/Hard/Good/Easy） | 同 |
| 代表实现 | Anki, Mnemosyne, Org-drill | `duolingo/halflife-regression` | `fsrs-optimizer`, `fsrs-rs`, `ts-fsrs` | 同 |
| 当前地位 | 仍是 Anki 经典回退算法 | 已被 DASH / FSRS 在精度上超越 | 已被 FSRS-5/6 取代但仍是 Anki 24.10 之前的官方默认 | Anki 24.10 默认 |

WordForge AMAS（`src/amas/memory/mdm.rs`）实现的是 **FSRS-4.5 风格的幂律遗忘曲线**：

```rust
// src/amas/memory/mdm.rs:206-220
// R(t,S) = floor + (1-floor) * (1 + factor * t/S)^decay
power_law = (1.0 + factor * elapsed_days / s).powf(decay);
```

默认 `factor = 0.3`, `decay = -0.5`, `floor = 0.0`，与 FSRS-4.5 官方 $factor = 19/81 \approx 0.2346$, $decay = -0.5$, 无 floor 的形式数学上等价但 factor 值有偏（[已在 `docs/amas-tuning-2026-05-15/02-fsrs-dhp-research.md` § 1 标注](../amas-tuning-2026-05-15/02-fsrs-dhp-research.md)）。

---

## 1. SM-2（SuperMemo 2）

### 1.1 原始定义与年份

SM-2 是 Piotr A. Woźniak 在 1987 年 12 月用 Turbo Pascal 3.0 在 IBM PC 上首次实现的间隔重复算法，源自他 1990 年硕士论文《Optimization of learning》（波兹南理工大学）。**没有独立的学术期刊发表**，权威描述在他后来发表到 super-memory.com 的网页版本里。

权威源（一手）：
- [Woźniak P.A., 1990, "Application of a computer to improve the results obtained in working with the SuperMemo method"](https://super-memory.com/english/ol/sm2.htm) — 原始 SuperMemo 网站，与 1990 年硕士论文同源
- [Wikipedia "SuperMemo" § Description of SM-2 algorithm](https://en.wikipedia.org/wiki/SuperMemo#Description_of_SM-2_algorithm) — 算法伪代码，独立交叉源 [Wikipedia, 2026-05, § 2.1]
- [SuperMemo Guru — History of SuperMemo algorithm](https://supermemo.guru/wiki/History_of_SuperMemo_algorithm) — 由 Woźniak 本人维护，列出 SM-0 → SM-20 演进

名称由来：算法首批应用于 SuperMemo 1.0–3.0（1987-12-13 至 1989-03-09），因 SuperMemo 2.0 最流行，故俗称 SM-2。[Woźniak, 1990, super-memory.com/english/ol/sm2.htm]

### 1.2 核心公式

**状态变量（每卡一组）**：
- $n$：连续正确（$q \geq 3$）次数
- $\mathrm{EF}$：Easiness Factor（E-Factor），初始 $\mathrm{EF}_0 = 2.5$
- $I$：当前间隔（天）

**间隔计算**：

$$I(1) = 1,\quad I(2) = 6,\quad I(n) = I(n-1)\cdot \mathrm{EF}\ \ (n>2)$$

**EF 更新（评级 $q\in\{0,1,2,3,4,5\}$）**：

$$\mathrm{EF}' = \mathrm{EF} + \left(0.1 - (5-q)\cdot\bigl(0.08 + (5-q)\cdot 0.02\bigr)\right)$$

等价展开形式（更便于实现）：

$$\mathrm{EF}' = \mathrm{EF} - 0.8 + 0.28q - 0.02q^2$$

**约束**：
- $\mathrm{EF}' < 1.3 \Rightarrow \mathrm{EF}' := 1.3$（硬地板）
- $q < 3 \Rightarrow n := 0,\ I := 1$（失败重置，但 **EF 不变**）
- $q = 4 \Rightarrow \mathrm{EF}$ 不变（这是 $\Delta\mathrm{EF}=0$ 的不动点）

**完整伪代码**（一字未改自 Wikipedia [Wikipedia, 2026, en.wikipedia.org/wiki/SuperMemo]）：

```text
if q >= 3:
    if   n == 0: I = 1
    elif n == 1: I = 6
    else:        I = round(I * EF)
    n += 1
else:
    n = 0
    I = 1
EF = EF + (0.1 - (5 - q) * (0.08 + (5 - q) * 0.02))
if EF < 1.3: EF = 1.3
```

每日 session 后，所有评级 $<4$ 的卡片要重复直到 $\geq 4$（这是 Anki 没有完整继承的部分）。[Woźniak, 1990, super-memory.com/english/ol/sm2.htm]

### 1.3 主流开源实现

| 实现 | 仓库 / URL | 关键文件 | 备注 |
|------|-----------|----------|------|
| Anki（修改版） | [ankitects/anki](https://github.com/ankitects/anki) | `rslib/src/scheduler/states/review.rs` | Anki 在 SM-2 基础上做了大量改造：四按钮（Again/Hard/Good/Easy）、ease 软地板 1.3（130%）、`interval_modifier`、`hard_interval` 等。FSRS 出现前是默认调度器。 |
| Mnemosyne | [mnemosyne-proj/mnemosyne](https://github.com/mnemosyne-proj/mnemosyne) | `mnemosyne/libmnemosyne/schedulers/SM2_mnemosyne.py` | SM-2 衍生（带随机扰动避免 review storm），是论文 [Reddington et al.] 引用最多的 baseline 之一 |
| Org-drill（Emacs） | [louietan/anki-editor](https://github.com/louietan/anki-editor)，原生在 Emacs Org-mode | `org-drill.el` | 默认 SM-5，可选 SM-2；学术界使用率较高 |
| 参考 Delphi 源码 | [Woźniak 原版 SM-2 plugin 源码](https://www.super-memory.com/english/ol/sm2source.htm) | `SM2.PAS` | Pascal 原始实现，1991 |

Duolingo HLR 论文中也实现了 SM-2 风格的 Leitner / Pimsleur baseline 用作对照。[Settles & Meeder, 2016, aclanthology.org/P16-1174]

### 1.4 参数清单与推荐缺省值

SM-2 **没有跨用户共享参数**，所有"调参"都体现在每卡的 EF 上：

| 状态量 | 初值 | 出处 |
|--------|-----|------|
| $\mathrm{EF}_0$ | 2.5 | Woźniak 1990 硬编码 |
| $I(1)$ | 1 天 | Woźniak 1990 硬编码 |
| $I(2)$ | 6 天 | Woźniak 1990 硬编码 |
| $\mathrm{EF}_{\min}$ | 1.3 | Woźniak 1990 实证经验（低于此值的卡片应被重写） |

唯一"超参数"是 EF 更新公式里的硬编码常数 $0.1, 0.08, 0.02$ —— 这些是 Woźniak 用 trial-and-error 选定的，**未做任何全局优化**。[Woźniak, 1990, super-memory.com/english/ol/sm2.htm]

### 1.5 与 FSRS-5 / WordForge AMAS 的对比

**相似点**：
- 都用单一标量（SM-2: EF；FSRS: D）刻画"难度"
- 都有"失败惩罚 + 成功放大"的双分支结构
- 间隔都是乘性增长

**核心差异**：

| 维度 | SM-2 | FSRS-4.5 / 5 / AMAS-MDM |
|------|------|---------------------------|
| 是否预测概率 | 否，只输出 $I$ | 输出 $R(t,S)\in[0,1]$，可设 `desired_retention` |
| 评级粒度 | 0–5（六档，但 ≥3 / <3 是关键阈值） | 1–4（Again/Hard/Good/Easy） |
| 状态变量数 | 3（n, EF, I） | 2（S, D），R 由 S+t 推导 |
| 参数训练 | 不训练，常数硬编码 | 17–19 维参数用梯度下降拟合个人数据 |
| 间隔公式 | $I_n = I_{n-1}\cdot\mathrm{EF}$（与遗忘曲线无关） | $I = \frac{S}{\text{factor}}\bigl(r^{1/\text{decay}}-1\bigr)$（从遗忘曲线反解） |
| 失败处理 | $n,I$ 全部重置，EF 不变（看似宽容实则保留 ease hell） | 用独立的 post-lapse stability 公式 $S'_f = w_{11}\cdot D^{-w_{12}}\cdots$，更新仍部分保留 |
| 同日多次复习 | 不区分（全部按 $I_n$ 处理） | FSRS-5 引入 short-term S 处理同日复习；4.5/AMAS-MDM 暂未模型化 |

WordForge AMAS-MDM 的 `recall_probability` 完全没有 EF/n 这类 SM-2 概念，状态只有 `stability + last_review_at`（见 `src/amas/memory/mdm.rs:208-220`）。AMAS 词卡选择器（`word_selector.rs`）用 ELO 评分代替 EF 的功能，但用途完全不同（SM-2 EF 决定调度，ELO 决定选词）。

### 1.6 已知缺陷与被取代原因

社区与学术界对 SM-2 的批评（核心事实 ≥2 独立源）：

1. **Ease hell / 低区间地狱**：用户长期累积低 EF 导致间隔几乎不增长（×1.3），形成"复习量正反馈循环"。
   - [readbroca.com — Anki Ease Hell](https://readbroca.com/anki/ease-hell/) [readbroca, 2023]
   - [memoforge.app — FSRS vs SM-2 Guide 2025](https://memoforge.app/blog/fsrs-vs-sm2-anki-algorithm-guide-2025/) [memoforge, 2025]
   - [Anki Forum — Ease Hell is a Myth?](https://forums.ankiweb.net/t/ease-hell-is-a-myth/54128) [Anki Forum]

2. **Ease bias（系统性下漂）**：Hard 扣 EF / Easy 加 EF 的非对称更新让 EF 长期向下漂，且 Good 不增 EF，使用户体感越用越累。[readbroca.com](https://readbroca.com/anki/ease-hell/)

3. **过期卡处理过激**：实证显示 SM-2 在严重过期卡上预测严重失准（例如 1 年过期，预测 87% 实际 75%）。[controlaltbackspace.org — Overdue Handling](https://controlaltbackspace.org/overdue-handling/) [Control-Alt-Backspace, 2023]

4. **缺乏个性化**：所有用户/所有卡共用同一组硬编码常数，无法拟合个体差异。[Expertium Benchmark](https://expertium.github.io/Benchmark.html)

5. **不输出 retrievability**：无法支持"目标 retention（90%）"这类现代需求，要靠间隔近似推断。

6. **量化对比**：Expertium 团队在 20k+ Anki collection 上的 benchmark 显示，SM-2 的 log-loss 与 RMSE(bins) 在主流算法中**长期排名最差**，FSRS-4.5/5 等所需 review 数比 SM-2 少 20–30%。[Expertium, 2024, expertium.github.io/Benchmark.html]

**被取代原因**：Anki 24.10（2024-10）起将 FSRS 升级为新用户默认算法，SM-2 退居"经典模式"。[AnkiWeb FAQ — What spaced repetition algorithm?](https://faqs.ankiweb.net/what-spaced-repetition-algorithm.html)

---

## 2. HLR（Half-Life Regression，Duolingo 2016）

### 2.1 论文与年份

**Burr Settles & Brendan Meeder, 2016. "A Trainable Spaced Repetition Model for Language Learning." Proceedings of the 54th Annual Meeting of the Association for Computational Linguistics (ACL), pages 1848–1858.**

权威源（一手）：
- [ACL Anthology P16-1174](https://aclanthology.org/P16-1174/) — 含 PDF 链接，DOI 10.18653/v1/P16-1174 [Settles & Meeder, ACL 2016]
- [PDF 直链](https://research.duolingo.com/papers/settles.acl16.pdf) — Duolingo 研究主页
- [GitHub: duolingo/halflife-regression](https://github.com/duolingo/halflife-regression) — 官方代码、README、数据集说明（含 1300 万条 13M traces）

13M 训练记录在 Harvard Dataverse 公开（[dataverse.harvard.edu/10.7910/DVN/N8XJME](https://dataverse.harvard.edu/dataset.xhtml?persistentId=doi:10.7910/DVN/N8XJME)）。[duolingo/halflife-regression README]

### 2.2 核心公式

HLR 假设 **Ebbinghaus 指数遗忘曲线**：

$$\hat p = 2^{-\Delta / h}$$

其中：
- $\hat p$：预测的回忆概率
- $\Delta$：距上次练习的间隔（天）
- $h$：半衰期（天）—— **关键概念**：回忆概率从 100% 衰减到 50% 所需时间

HLR 的"trainable"部分是 **把 $\log_2 h$ 用 log-linear 回归建模**：

$$\log_2(h) = \boldsymbol{\theta}\cdot\mathbf{x}\quad\Leftrightarrow\quad h = 2^{\boldsymbol{\theta}\cdot\mathbf{x}}$$

**特征向量 $\mathbf{x}$**（来自论文 § 3.2 与 `experiment.py:read_data`）：

1. **交互历史摘要**：
   - $\sqrt{1+\text{right}}$（历史正确数的平方根，类似 Leitner box level）
   - $\sqrt{1+\text{wrong}}$（历史错误数）
   - 可选 `bias` 项（截距）
2. **稀疏 lexeme 指示器**：每个 lexeme tag 一个布尔特征（论文称约 $2\times 10^4$ 维），编码 part-of-speech、形态、屈折形式等

**损失函数**（论文 Eq. 4–6，对应 `train_update`）：

$$
\ell(\langle p,\Delta,\mathbf{x}\rangle) = (p - \hat p)^2 + \alpha\cdot(h^\star - \hat h)^2 + \lambda\|\boldsymbol{\theta}\|_2^2
$$

其中：
- 第 1 项：观测回忆概率（session 内 correct/seen 比例）与预测概率的平方误差
- 第 2 项：用 $h^\star = -\Delta/\log_2(p)$ 反推一个 "ground-truth half-life"，再与预测 $\hat h$ 算平方误差（"半衰期一致性项"）
- 第 3 项：L2 正则化

**优化**：AdaGrad 风格的逐特征自适应学习率：

$$\eta_k = \frac{\eta_0}{(1+p)\cdot\sqrt{1+c_k}}$$

其中 $c_k$ 是特征 $k$ 出现的累计计数（来自 `experiment.py:train_update`）。[Settles & Meeder, 2016, github.com/duolingo/halflife-regression]

**实现裁剪**（来自 `experiment.py` 源码）：
- $h$ 被夹到 $[15/(24\times 60), 274]$ 天 = [15 min, 9 mo]
- $p$ 被夹到 $[10^{-4}, 1-10^{-4}]$

### 2.3 主流开源实现

| 实现 | URL | 关键文件 | 备注 |
|------|-----|---------|------|
| 官方 Duolingo | [github.com/duolingo/halflife-regression](https://github.com/duolingo/halflife-regression) | `experiment.py`（Python/PyPy），`evaluation.r` | 含 13M 数据集元数据，论文 baseline 包括 leitner / pimsleur / lr / hlr 四种 method 切换 |
| Lyle Schemmerling fork | [github.com/lschemmerling/halflife-regression](https://github.com/lschemmerling/halflife-regression) | Python 3 移植版 | 原版用 Python 2 + PyPy，社区有多个 Py3 fork |
| Cambridge Adaptive Forgetting Curve | [www.repository.cam.ac.uk Paper](https://www.repository.cam.ac.uk/bitstream/1810/305124/1/Adaptive_Forgetting_Curve_for_Spaced_Repetition_Language_Learning_.pdf) | Hu 2020 thesis | HLR 复现 + 改进 baseline |
| 学术复现（Politecnico Milano） | [www.politesi.polimi.it Randazzo 2022](https://www.politesi.polimi.it/retrieve/b39227dd-0963-40f2-a44b-624f205cb224/2022_4_Randazzo_01.pdf) | 含 HLR vs DASH 对比 | HLR AUC ≈ 0.61 vs DASH-RNN ≈ 0.84 |

### 2.4 参数清单与推荐缺省值

`experiment.py` 默认配置：

| 超参数 | 默认值 | 含义 | 出处 |
|--------|--------|------|------|
| `lrate` (η₀) | 0.001 | AdaGrad 初始学习率 | `experiment.py:__init__` |
| `hlwt` (α) | 0.01 | 半衰期一致性项权重 | 同上 |
| `l2wt` (λ) | 0.1 | L2 正则化强度 | 同上 |
| `sigma` (σ) | 1.0 | L2 项的方差缩放 | 同上 |
| `MIN_HALF_LIFE` | 15 min（≈0.0104 天） | 半衰期下限夹值 | `experiment.py:21` |
| `MAX_HALF_LIFE` | 274 天（≈9 个月） | 半衰期上限夹值 | `experiment.py:22` |
| `base` | 2.0 | 半衰期公式的指数底（$h = b^{\theta\cdot x}$） | `predict` 函数 |

**学习到的权重 $\boldsymbol{\theta}$ 不是超参数**，而是从 13M traces 训练出来的，无 universal default。Duolingo 论文中给出：在英 → 法的 lexeme 集上，HLR-lex（含 lexeme 特征）测试集 MAE(p) 比 logistic regression baseline 降低 ~45%。[Settles & Meeder, 2016, § 4, aclanthology.org/P16-1174]

### 2.5 与 FSRS-5 / WordForge AMAS 的对比

**相似点**：
- 都把"记忆强度"建模为标量（HLR: $h$；FSRS: $S$），都用幂/指数函数描述衰减
- 都是数据驱动训练（HLR: SGD on 13M traces；FSRS: 梯度下降 on 个人 revlog）
- 都把 lexeme/word 级别的"内在难度"作为可学习特征

**核心差异**：

| 维度 | HLR | FSRS-4.5 / 5 / AMAS-MDM |
|------|-----|---------------------------|
| 遗忘曲线形式 | **指数** $p = 2^{-\Delta/h}$ | **幂律** $R = (1+\frac{19}{81}\cdot\frac{t}{S})^{-0.5}$ |
| 状态信息 | 仅 right/wrong 摘要计数 + lexeme one-hot（**丢失评级序列与精确时间戳**） | 维护 $S$ 和 $D$ 序贯状态，每次复习用上一个 S/D 增量更新 |
| 难度建模 | lexeme-specific 稀疏特征（约 2e4 维） | 单卡单标量 $D \in [1, 10]$，无 cross-card 共享 |
| 训练数据规模 | 跨用户 13M（全 Duolingo 群体） | 单用户个人 revlog（典型 1k–100k 条） |
| 评级粒度 | 二元 + session 内连续值（right/seen 比例） | 离散 4 档（Again/Hard/Good/Easy） |
| 输出 | $\hat p, \hat h$ | $R(t,S)$，可设定 retention 反解间隔 |
| 个性化 | 群体模型 + lexeme 因子（粗粒度个性化） | 完全个人化（每用户独立优化 $w$） |
| AUC（benchmark） | ≈ 0.61 [Randazzo 2022] | ≈ 0.84–0.86 [Expertium Benchmark] |

WordForge AMAS-MDM 没有 HLR 风格的 lexeme one-hot 维度；词卡难度由 ELO 评分系统（`src/amas/elo.rs`）和 IAD（`src/amas/memory/iad.rs`）联合决定，与 HLR 的稀疏 theta 完全不同范式。

### 2.6 已知缺陷与被取代原因

学术界和 FSRS 社区对 HLR 的核心批评：

1. **遗忘曲线选错族**：指数形式 $2^{-\Delta/h}$ 在群体上不如幂律（FSRS-4.5/5 用 $-0.5$ 幂），因为不同 stability 的混合等价于幂律近似。
   - [Expertium Algorithm § R, Retrievability](https://expertium.github.io/Algorithm.html) 用数学论证：两条指数曲线 $0.9^{t/0.2}$ 和 $0.9^{t/3}$ 的算术平均更接近一条幂律曲线
   - [open-spaced-repetition/srs-benchmark](https://github.com/open-spaced-repetition/srs-benchmark) 实测对比 HLR vs 幂律

2. **特征丢序贯信息**：仅用 $\sqrt{1+\text{right}}$ / $\sqrt{1+\text{wrong}}$ 这两个摘要统计，**完全丢弃复习时序、精确时间间隔、连续失败模式**。FSRS / DASH-RNN 用全序列。[Expertium Benchmark — HLR uses "overly simplistic formula"]

3. **预测精度差**：在多个独立 benchmark（Polimi 2022, Expertium 2024）上：
   - HLR AUC ≈ 0.61
   - DASH (Lindsey-Pashler-Mozer 2014) AUC ≈ 0.83
   - DASH[ACT-R], DASH[MCM] AUC ≈ 0.84
   - FSRS-4.5 / 5 优于 DASH 标准变体
   - 来源：[Randazzo, 2022, politesi.polimi.it](https://www.politesi.polimi.it/retrieve/b39227dd-0963-40f2-a44b-624f205cb224/2022_4_Randazzo_01.pdf)

4. **稀疏 lexeme 特征易过拟合**：论文里 lexeme one-hot 维度数（2e4）接近 Duolingo 总 lexeme 数，单用户数据稀疏化严重，跨用户迁移性差。[Settles & Meeder, 2016, § 5 Discussion]

5. **理论根基弱**：Mozer / Lindsey 在 "Psychological Theory Matters in the Big Data Era"（[rob-lindsey.com/papers/2016/bigdata.pdf](http://rob-lindsey.com/papers/2016/bigdata.pdf)）明确指出，纯特征工程 ML（如 HLR）缺少认知架构（如 ACT-R、MCM）提供的归纳偏置，即便数据再大也无法收敛到真实的 forgetting dynamics。

6. **Duolingo 自身后续放弃**：HLR 仍在论文里报告 +12% 工程留存与 +5% MAU 增长 [Settles & Meeder, 2016, § 4.3]，但 Duolingo 后续转向 Birdbrain（更复杂的多模型集成），HLR 主要剩学术 baseline 作用。

**取代它的算法谱系**：DASH（2014）→ DASH[RNN]（2017）→ FSRS（2022+）→ FSRS-rs（Rust 重构 2023）→ FSRS-5（2024-07）→ FSRS-6（2025+）。

---

## 3. FSRS-4.5（Free Spaced Repetition Scheduler 4.5）

### 3.1 论文/原始定义与年份

FSRS-4.5 **没有传统期刊论文**，由 Jarrett Ye（GitHub `L-M-Sherlock` / Reddit `u/LMSherlock`）在 open-spaced-repetition 组织下迭代发布。FSRS 的最早学术雏形是 Ye 2022 年发表的预印本《A Stochastic Shortest Path Algorithm for Optimizing Spaced Repetition Scheduling》（ArXiv），后续版本在 Wiki + 社区 issue 上演化。

**FSRS-4.5 发布时间：2023-12-26**（通过 fsrs4anki repo 的 PR #568 提交）。[LessWrong — The History of FSRS for Anki](https://www.lesswrong.com/posts/G7fpGCi8r7nCKXsQk/the-history-of-fsrs-for-anki)

权威源（一手）：
- [open-spaced-repetition/awesome-fsrs Wiki — The Algorithm](https://github.com/open-spaced-repetition/awesome-fsrs/wiki/The-Algorithm) — 算法定义（含 FSRS v1 → v6 完整公式）
- [LessWrong — The History of FSRS for Anki by L-M-Sherlock](https://www.lesswrong.com/posts/G7fpGCi8r7nCKXsQk/the-history-of-fsrs-for-anki) — 作者本人的版本时间线
- [Expertium — A technical explanation of FSRS](https://expertium.github.io/Algorithm.html) — 独立的算法解读
- [Borretti — Implementing FSRS in 100 Lines](https://borretti.me/article/implementing-fsrs-in-100-lines) — 100 行 Python 参考实现 [Borretti, 2024]

### 3.2 核心公式

FSRS 用 DSR 三元组建模记忆：

- $D$（Difficulty）：难度，$\in[1,10]$
- $S$（Stability）：稳定度，定义为"$R$ 从 100% 衰减到 90% 所需天数"
- $R$（Retrievability）：当前回忆概率，由 $S$ 与距上次复习的 $t$ 推出

**遗忘曲线（FSRS-4.5 的标志性变化）**：

$$R(t,S) = \left(1 + \mathrm{FACTOR}\cdot \frac{t}{S}\right)^{\mathrm{DECAY}}$$

其中 $\mathrm{FACTOR} = 19/81 \approx 0.2346,\ \mathrm{DECAY} = -0.5$，由约束 $R(S,S) = 0.9$ 解出。FSRS v4 用的是 $\mathrm{FACTOR} = 1/9,\ \mathrm{DECAY} = -1$，4.5 改成上述新参数让曲线在 $t > S$ 后下降更平缓。

**反解间隔**（给定目标 retention $r$）：

$$I(r, S) = \frac{S}{\mathrm{FACTOR}}\cdot\left(r^{1/\mathrm{DECAY}} - 1\right)$$

当 $r = 0.9$ 时 $I = S$。

**初始 stability（首次复习后，$G \in \{1,2,3,4\}$）**：

$$S_0(G) = w_{G-1}$$

即 $w_0, w_1, w_2, w_3$ 分别对应 Again / Hard / Good / Easy 后的初始 S。

**初始 difficulty**：

$$D_0(G) = w_4 - (G - 3)\cdot w_5$$

注意 FSRS-4.5 用线性形式，FSRS-5 改成指数形式 $D_0(G) = w_4 - e^{w_5(G-1)} + 1$。

**Difficulty 更新（mean reversion 防止 "ease hell"）**：

$$D' = w_7 \cdot D_0(3) + (1 - w_7)\cdot\bigl(D - w_6\cdot(G-3)\bigr)$$

**Stability 更新（成功复习，$G \geq 2$）**：

$$S'_r(D,S,R,G) = S \cdot \Bigl(e^{w_8}\cdot(11-D)\cdot S^{-w_9}\cdot\bigl(e^{w_{10}(1-R)}-1\bigr)\cdot \kappa(G) + 1\Bigr)$$

其中 $\kappa(G) = w_{15}$ 当 $G=2$（Hard），$\kappa(G) = w_{16}$ 当 $G=4$（Easy），$\kappa(G) = 1$ 当 $G=3$（Good）。

**Stability 更新（失败，$G=1$，post-lapse stability）**：

$$S'_f(D,S,R) = w_{11}\cdot D^{-w_{12}}\cdot\bigl((S+1)^{w_{13}} - 1\bigr)\cdot e^{w_{14}(1-R)}$$

[open-spaced-repetition/awesome-fsrs Wiki — FSRS-4.5]

### 3.3 主流开源实现

FSRS-4.5 在多语言生态中被广泛实现。**注意：截至 2026-05，多数实现已升级到 FSRS-5/6，需要锁定 4.5 行为时要显式传 `w` 数组**。

| 实现 | URL | 关键文件 | 备注 |
|------|-----|---------|------|
| **Python 优化器（标杆）** | [github.com/open-spaced-repetition/fsrs-optimizer](https://github.com/open-spaced-repetition/fsrs-optimizer) | `src/fsrs_optimizer/fsrs_optimizer.py` | 含 `ParameterClipper`，是 19/21 维参数 bound 的权威来源 |
| **Rust 实现** | [github.com/open-spaced-repetition/fsrs-rs](https://github.com/open-spaced-repetition/fsrs-rs) | `src/inference.rs`, `src/training.rs` | Anki 内置 FSRS 的核心依赖；Anki 调用 fsrs-rs 实现 |
| **TypeScript 实现** | [github.com/open-spaced-repetition/ts-fsrs](https://github.com/open-spaced-repetition/ts-fsrs) | `packages/fsrs/src/` | npm `ts-fsrs`，需要 Node ≥20；纯 TS 零依赖 |
| **Anki 内置** | [github.com/ankitects/anki](https://github.com/ankitects/anki) | `rslib/src/scheduler/fsrs/` | 直接调用 fsrs-rs |
| **100 行参考** | [github.com/borretti/fsrs100lines](https://github.com/borretti/fsrs100lines) | `fsrs.py` | 教学性 minimal 实现 [Borretti 2024] |
| **Python py-fsrs** | [github.com/open-spaced-repetition/py-fsrs](https://github.com/open-spaced-repetition/py-fsrs) | `fsrs/scheduler.py` | 现已升级 FSRS-6，参数 21 维；锁 4.5 需传 17 维 w |
| **fsrs-rs-python** | [github.com/open-spaced-repetition/fsrs-rs-python](https://github.com/open-spaced-repetition/fsrs-rs-python) | Rust→Python PyO3 binding | 优化器 torch-free 替代 |

### 3.4 参数清单与推荐缺省值

**FSRS-4.5 默认 17 维参数**（来源：[awesome-fsrs Wiki, 2024](https://github.com/open-spaced-repetition/awesome-fsrs/wiki/The-Algorithm) FSRS-4.5 节）：

```text
w = [0.4872, 1.4003, 3.7145, 13.8206,
     5.1618, 1.2298, 0.8975, 0.031,
     1.6474, 0.1367, 1.0461,
     2.1072, 0.0793, 0.3246, 1.587,
     0.2272, 2.8755]
```

| 索引 | 语义 | 默认值 | ParameterClipper bound（来自 fsrs-optimizer） |
|------|------|--------|---------------------------------------------|
| w[0] | initial S after Again | 0.4872 | [S_MIN, 100] |
| w[1] | initial S after Hard | 1.4003 | [S_MIN, 100] |
| w[2] | initial S after Good | 3.7145 | [S_MIN, 100] |
| w[3] | initial S after Easy | 13.8206 | [S_MIN, 100] |
| w[4] | initial D base ($D_0(3)$) | 5.1618 | [1, 10] |
| w[5] | initial D rating linear coef | 1.2298 | [0.001, 4] |
| w[6] | next D rating offset | 0.8975 | [0.001, 4] |
| w[7] | D mean reversion strength | 0.031 | [0.001, 0.75] |
| w[8] | S boost on recall (exp base) | 1.6474 | [0, 4.5] |
| w[9] | S decay with old S | 0.1367 | [0, 0.8] |
| w[10] | R-bonus on recall | 1.0461 | [0.001, 3.5] |
| w[11] | post-lapse S base | 2.1072 | [0.001, 5] |
| w[12] | post-lapse / D exponent | 0.0793 | [0.001, 0.25] |
| w[13] | post-lapse / S exponent | 0.3246 | [0.001, 0.9] |
| w[14] | post-lapse / R-bonus | 1.587 | [0, 4] |
| w[15] | Hard penalty multiplier | 0.2272 | [0, 1] |
| w[16] | Easy bonus multiplier | 2.8755 | [1, 6] |

**全局常量**（FSRS-4.5 写死）：
- $\mathrm{FACTOR} = 19/81$
- $\mathrm{DECAY} = -0.5$
- $S_{\min} = 0.01$（fsrs-optimizer 默认 stability 下限）
- 推荐 retention：$r = 0.9$（即间隔 $I = S$）

[默认值出处：awesome-fsrs Wiki FSRS-4.5 节，与 fsrs-optimizer Python 包 / fsrs-rs Rust 源码、ts-fsrs `defaultW` 一致]

**典型 bound 实证**：fsrs-optimizer 的 `ParameterClipper` 类（`src/fsrs_optimizer/fsrs_optimizer.py` 第 194–218 行附近，[源码可读](https://raw.githubusercontent.com/open-spaced-repetition/fsrs-optimizer/main/src/fsrs_optimizer/fsrs_optimizer.py)）在每次梯度更新后强制截断 $w$。撞 boundary 是优化失败信号，见 [Anki Forum — FSRS parameters hitting boundaries](https://forums.ankiweb.net/t/fsrs-parameters-hitting-boundries-after-optimization/40777)。

### 3.5 与 FSRS-5 / WordForge AMAS 的对比

**FSRS-4.5 vs FSRS-5 的关键差异**（[awesome-fsrs Wiki — FSRS-5 节](https://github.com/open-spaced-repetition/awesome-fsrs/wiki/The-Algorithm)）：

| 维度 | FSRS-4.5 | FSRS-5 |
|------|----------|--------|
| 参数维度 | 17 | 19（增加 $w_{17}, w_{18}$） |
| 初始 D 公式 | $D_0(G) = w_4 - (G-3)\cdot w_5$（线性） | $D_0(G) = w_4 - e^{w_5(G-1)} + 1$（指数） |
| D mean reversion 目标 | $D_0(3)$（Good） | $D_0(4)$（Easy） |
| Difficulty 更新阻尼 | 无 | Linear damping: $\Delta D \cdot \frac{10-D}{9}$（防止 D 触顶） |
| 同日 review | 不区分 | 引入 short-term S: $S' = S\cdot e^{w_{17}(G-3+w_{18})}$ |
| 遗忘曲线 | $(1+\frac{19}{81}\cdot\frac{t}{S})^{-0.5}$（写死） | 同（FSRS-6 才让 decay 可学习 → $w_{20}$） |
| 预测精度增益 | baseline | log-loss 比 4.5 低约 4%（来源 [Expertium Benchmark](https://expertium.github.io/Benchmark.html)） |

**FSRS-4.5 vs WordForge AMAS-MDM 的对应关系**（基于 `src/amas/memory/mdm.rs` 与 `docs/amas-tuning-2026-05-15/02-fsrs-dhp-research.md`）：

| 元素 | FSRS-4.5 | WordForge AMAS-MDM |
|------|----------|---------------------|
| 遗忘曲线 | $R(t,S) = (1 + \frac{19}{81}\cdot\frac{t}{S})^{-0.5}$ | $R(t,S) = \text{floor} + (1-\text{floor})\cdot(1 + \text{factor}\cdot\frac{t}{S})^{\text{decay}}$ |
| factor 默认 | 19/81 ≈ 0.2346（**写死**） | 0.3（**可调**，TPE 搜索维度） |
| decay 默认 | -0.5（**写死**） | -0.5（可调） |
| floor | 无（≡0） | `forgetting_curve_floor`（默认 0，可调，保证 $R \geq \text{floor}$） |
| S, D 更新 | 17 维 $w$ 全套 | 同样 17 维（adapter 只读 w[0..18]，实际为 FSRS-5 风格 19 维但首批默认沿用 4.5 数值） |
| Stability 下限 | $S_{\min}\approx 0.01$ | `state.stability.max(0.01)`（一致） |
| 间隔 cap | 由 retention + S 决定 | `max_interval_days`（默认 90 天，硬 cap） |
| 间隔下限 | 1 天 | `min_interval_secs`（默认值在 config 中） |

**结论**：AMAS-MDM 数学上是 FSRS-4.5 + 额外 `floor` 项 + 显式 `max_interval_days` cap 的超集；TPE 调参时把 19/81 这个写死常数也开放为搜索维度（factor），属于"调参自由度比 FSRS-4.5 高、比 FSRS-6 低"的中间形态。AMAS 当前默认 factor=0.3 与官方 4.5 的 19/81 偏差约 28%，已在 amas-tuning 报告中标为可优化方向。

### 3.6 已知缺陷与被取代原因

FSRS-4.5 的已知问题（来源：FSRS 作者与社区 issue 总结）：

1. **同日复习无建模**：4.5 把当日多次复习当作单次处理，长尾用户（一天回数次的）会被低估学习信号。FSRS-5 用 $S' = S\cdot e^{w_{17}(G-3+w_{18})}$ 显式建模。[forums.ankiweb.net — about-fsrs-algorithms-first-rating](https://forums.ankiweb.net/t/about-fsrs-algorithms-first-rating/50055)

2. **D 公式不依赖 R**：当前 difficulty 更新只看 grade，不看复习时 R 的高低。同样按 Good，R=0.99 和 R=0.01 应当对 D 产生不同影响（后者意味"几乎忘了但还想起来"，远比前者难）。[Expertium § D, Difficulty — Important takeaway 5]

3. **D mean reversion 目标偏差**：4.5 reverts to $D_0(3)$（Good），意味着长期按 Easy 复习的卡 D 仍被拉回 Good 对应值。5.0 改成 reverts to $D_0(4)$ 修复了这点。[awesome-fsrs Wiki FSRS-5]

4. **遗忘曲线 decay 写死 -0.5**：不同用户的实际遗忘曲线形状不同，强行用 $-0.5$ 对部分人群拟合不佳。FSRS-6 引入 $w_{20}$ 让 decay 可学习。[awesome-fsrs Wiki FSRS-6]

5. **初始 D 线性 vs 真实形态**：4.5 的 $D_0(G) = w_4 - (G-3)\cdot w_5$ 是直线，但实测 first-rating 与 stability 的关系非线性。5.0 改用 $D_0(G) = w_4 - e^{w_5(G-1)} + 1$。

**被取代时间线**：
- 2023-07：FSRS v4（17 维）
- **2023-12-26：FSRS-4.5**（改遗忘曲线为 19/81, -0.5）
- 2024-07：FSRS-5（19 维，加 short-term S + 改 D 公式）
- 2024-10：Anki 24.10 把 FSRS-5 作为新用户默认
- 2025+：FSRS-6（21 维，加 decay 可学习 $w_{20}$，aggregator 更新等）

[全部时间线源：[LessWrong — The History of FSRS for Anki](https://www.lesswrong.com/posts/G7fpGCi8r7nCKXsQk/the-history-of-fsrs-for-anki) + [awesome-fsrs Wiki](https://github.com/open-spaced-repetition/awesome-fsrs/wiki/The-Algorithm)]

---

## 4. 对 WordForge AMAS 的启示与 benchmark 设计建议

1. **SM-2 适合作为 baseline**：它有完整且无歧义的公式定义，不需调参，可作为"零参数 lower bound"加入 benchmark。`src/amas/memory/mdm.rs` 的设计完全不兼容 SM-2，需要单独写一个 `sm2_scheduler.rs` 把 (n, EF, I) 三状态映射到 MdmState 接口。建议输入 grade 用 `0..5` 而非 `0..4`，并显式约定 $q=q_{Anki} + 2$（Again→1, Hard→3, Good→4, Easy→5），与 Anki 实际做法一致。

2. **HLR 适合作为"统计 baseline"**：它的指数遗忘曲线 + AdaGrad 训练范式与 FSRS 的幂律 + 梯度下降形成对比，能凸显幂律/序贯模型的优势。实现时直接移植 `experiment.py:halflife / predict / train_update` 三个函数即可，特征用 $\sqrt{1+\text{right}}, \sqrt{1+\text{wrong}}$ 二维 + 词项 one-hot。注意 HLR 是**群体模型**，要在 benchmark 数据集上预训练再做评估。

3. **FSRS-4.5 vs WordForge AMAS-MDM 的等价点**：
   - 若把 `forgetting_curve_factor` 锁定为 $19/81$，`forgetting_curve_decay` 锁定为 $-0.5$，`forgetting_curve_floor` 锁定为 0，AMAS-MDM 的 `recall_probability` 与 `compute_interval` 应与 FSRS-4.5 / `ts-fsrs` 给出 bit-identical 输出（前提是 w[0..16] 一致）。
   - 建议先做这个 lock 校准测试再展开调参 benchmark，以排除"两个实现行为不同"的混淆变量。

4. **benchmark 公平性**：
   - 评级映射：SM-2 q∈{0..5}，HLR 二元，FSRS 1..4。需要在 benchmark dataset（如 maimemo schedulers.py 或 SRS-benchmark）里统一为四档再分发给各 scheduler 自行内部转换。
   - 评价指标：log-loss（binary cross-entropy）+ RMSE(bins) + AUC，与 Expertium / open-spaced-repetition/srs-benchmark 一致。
   - 公平 baseline：HLR 必须给足训练数据（建议 ≥ 1M traces）才能体现实力，单用户场景对它不公平。

5. **AMAS 的设计自由度**：报告 [amas-tuning-2026-05-15/02-fsrs-dhp-research.md § 4](../amas-tuning-2026-05-15/02-fsrs-dhp-research.md) 已经把 19/81 vs 0.3 的差异标为待修方向。本调研支持该结论：在等价于 FSRS-4.5 的前提下，把 factor 锁回 19/81 是合理的"基线一致性"措施，先一致再创新。

---

## 5. 引用清单（核心源 URL）

### SM-2
1. [Woźniak P.A., 1990, "Application of a computer..." (super-memory.com)](https://super-memory.com/english/ol/sm2.htm) — 一手原始定义
2. [Wikipedia, 2026-05, "SuperMemo" § SM-2](https://en.wikipedia.org/wiki/SuperMemo#Description_of_SM-2_algorithm) — 算法伪代码独立交叉源
3. [SuperMemo Guru — History of SuperMemo algorithm](https://supermemo.guru/wiki/History_of_SuperMemo_algorithm) — 作者维护的版本史
4. [super-memory.com — SM2 Delphi source](https://www.super-memory.com/english/ol/sm2source.htm) — 原版 Pascal 源码
5. [readbroca.com — Anki Ease Hell](https://readbroca.com/anki/ease-hell/) — 缺陷分析
6. [AnkiWeb FAQ — What spaced repetition algorithm?](https://faqs.ankiweb.net/what-spaced-repetition-algorithm.html) — Anki 实现差异
7. [controlaltbackspace.org — Overdue Handling](https://controlaltbackspace.org/overdue-handling/) — 过期卡实证
8. [memoforge.app — FSRS vs SM-2 Guide 2025](https://memoforge.app/blog/fsrs-vs-sm2-anki-algorithm-guide-2025/) — 对比综述

### HLR
9. [Settles & Meeder, ACL 2016, P16-1174](https://aclanthology.org/P16-1174/) — 原论文
10. [PDF — research.duolingo.com/papers/settles.acl16.pdf](https://research.duolingo.com/papers/settles.acl16.pdf) — 直接 PDF
11. [GitHub — duolingo/halflife-regression](https://github.com/duolingo/halflife-regression) — 官方代码 + 数据集
12. [experiment.py 源码（raw）](https://raw.githubusercontent.com/duolingo/halflife-regression/master/experiment.py) — Python 实现
13. [papousek.github.io — Analysis of HLR](https://papousek.github.io/analysis-of-half-life-regression-model-made-by-duolingo.html) — 独立分析
14. [Randazzo, Polimi 2022 thesis](https://www.politesi.polimi.it/retrieve/b39227dd-0963-40f2-a44b-624f205cb224/2022_4_Randazzo_01.pdf) — HLR vs DASH benchmark
15. [Mozer/Lindsey, 2016 — Psychological Theory Matters](http://rob-lindsey.com/papers/2016/bigdata.pdf) — 理论批评

### FSRS-4.5
16. [open-spaced-repetition/awesome-fsrs Wiki — The Algorithm](https://github.com/open-spaced-repetition/awesome-fsrs/wiki/The-Algorithm) — 算法定义（v1 → v6 全套公式）
17. [LessWrong — The History of FSRS for Anki](https://www.lesswrong.com/posts/G7fpGCi8r7nCKXsQk/the-history-of-fsrs-for-anki) — 作者本人时间线
18. [Expertium — A technical explanation of FSRS](https://expertium.github.io/Algorithm.html) — 独立技术解读
19. [Expertium — Benchmark](https://expertium.github.io/Benchmark.html) — 算法对比 benchmark
20. [GitHub — open-spaced-repetition/fsrs-optimizer](https://github.com/open-spaced-repetition/fsrs-optimizer) — Python 优化器
21. [fsrs_optimizer.py raw 源码](https://raw.githubusercontent.com/open-spaced-repetition/fsrs-optimizer/main/src/fsrs_optimizer/fsrs_optimizer.py) — `ParameterClipper` bound 出处
22. [GitHub — open-spaced-repetition/fsrs-rs](https://github.com/open-spaced-repetition/fsrs-rs) — Rust 实现（Anki 内核）
23. [GitHub — open-spaced-repetition/ts-fsrs](https://github.com/open-spaced-repetition/ts-fsrs) — TypeScript 实现
24. [Borretti — Implementing FSRS in 100 Lines](https://borretti.me/article/implementing-fsrs-in-100-lines) — 100 行参考
25. [Anki Forum — FSRS parameters hitting boundaries](https://forums.ankiweb.net/t/fsrs-parameters-hitting-boundries-after-optimization/40777) — 调参实战

### 内部参考
26. [`docs/amas-tuning-2026-05-15/02-fsrs-dhp-research.md`](../amas-tuning-2026-05-15/02-fsrs-dhp-research.md) — WordForge 现有 FSRS-5 / DHP 调研
27. `src/amas/memory/mdm.rs` — WordForge MDM（FSRS-4.5 等价实现）

---

## 附：交叉验证表（每算法 ≥2 独立源）

| 事实 | 源 1 | 源 2 | 源 3 |
|------|------|------|------|
| SM-2 EF 公式 | super-memory.com/english/ol/sm2 | en.wikipedia.org/wiki/SuperMemo | tegaru.app SM2 explained |
| SM-2 初始 EF=2.5, I(1)=1, I(2)=6 | super-memory.com（一手） | Wikipedia § 2.1 | supermemo.guru History |
| SM-2 EF floor 1.3 | super-memory.com | Wikipedia | readbroca.com |
| HLR 公式 $p = 2^{-\Delta/h}$ | aclanthology.org/P16-1174 | duolingo/halflife-regression README | papousek.github.io 分析 |
| HLR log-linear $h = 2^{\theta\cdot x}$ | settles.acl16.pdf | experiment.py 源码 | Cambridge thesis |
| HLR 13M traces 与 +12% engagement | settles.acl16.pdf | dataverse.harvard.edu | Settles 引用页 |
| HLR AUC ≈ 0.61 | politesi.polimi.it Randazzo 2022 | expertium.github.io/Benchmark | open-spaced-repetition/srs-benchmark |
| FSRS-4.5 遗忘曲线 19/81, -0.5 | awesome-fsrs Wiki | expertium.github.io/Algorithm | borretti.me 100 lines |
| FSRS-4.5 17 维默认参数 | awesome-fsrs Wiki FSRS-4.5 节 | fsrs-optimizer 源码 | ts-fsrs default w |
| FSRS-4.5 发布 2023-12-26 | LessWrong History | github.com/open-spaced-repetition/fsrs4anki PR #568 | awesome-fsrs Wiki |
| FSRS-4.5 vs 5 差异（同日 + D 公式） | awesome-fsrs Wiki | expertium.github.io/Algorithm | forums.ankiweb.net 同日 review |

---

> 报告完。三个算法的关键事实均通过 ≥2 独立源交叉验证，全部公式与默认参数已可直接用于 benchmark/maimemo schedulers.py 风格的 Rust 实现。
