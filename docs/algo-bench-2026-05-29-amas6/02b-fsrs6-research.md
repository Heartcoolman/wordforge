# FSRS-6 与 SM-17 算法调研报告

调研日期：2026-05-27（今）｜目标：直接落地 Python 实现

---

## 一、FSRS-6（Free Spaced Repetition Scheduler v6）

### 1.1 核心结论速览

- **w 数组维度：21**（FSRS-5 是 19，FSRS-6 在末尾追加 `w19` 短期 saturation 项与 `w20` decay）
- **决定性差异 vs FSRS-5**：①遗忘曲线的 decay 变成 trainable 参数 `w20`；②同日复习公式新增 `S^(-w19)` 饱和项（小 S 增长快、大 S 增长慢）
- **forgetting curve 仍是 power 形式**：`R = (1 + factor·t/S)^(-w20)`，但 factor 不再是常数 `19/81`，而是 `0.9^(-1/w20) − 1`（保证 `R(S,S)=0.9`）

### 1.2 默认 21 参数（py-fsrs main / PyPI 当前版）

来源：[py-fsrs README](https://github.com/open-spaced-repetition/py-fsrs)（PR #133 后）

```python
DEFAULT_PARAMETERS = (
    0.212,    # w0  initial S (Again)
    1.2931,   # w1  initial S (Hard)
    2.3065,   # w2  initial S (Good)
    8.2956,   # w3  initial S (Easy)
    6.4133,   # w4  initial D base = D0(1) = D0(Again)；同时是 mean reversion 目标
    0.8334,   # w5  初始 D 的 grade 指数衰减系数
    3.0194,   # w6  ΔD per grade（线性 damping 前）
    0.001,    # w7  mean reversion 权重（接近 0 = 弱回归）
    1.8722,   # w8  SInc 总缩放 e^w8
    0.1666,   # w9  S 的饱和指数（S^(-w9)）
    0.796,    # w10 R 影响项 e^(w10·(1-R)) − 1
    1.4835,   # w11 post-lapse 总缩放
    0.0614,   # w12 post-lapse D 指数 D^(-w12)
    0.2629,   # w13 post-lapse S 指数 (S+1)^w13 − 1
    1.6483,   # w14 post-lapse R 指数 e^(w14·(1-R))
    0.6014,   # w15 Hard 惩罚系数（<1）
    1.8729,   # w16 Easy 奖励系数（>1）
    0.5425,   # w17 同日复习 grade 影响 e^(w17·(G−3+w18))
    0.0912,   # w18 同日复习 grade 偏移
    0.0658,   # w19 同日复习 S 饱和指数（FSRS-6 新增）
    0.1542,   # w20 forgetting curve decay（FSRS-6 新增；通常 0.1~0.8，多数用户 <0.2）
)
```

其他默认值：`desired_retention=0.9`，`maximum_interval=36500`（约 100 年），`enable_fuzzing=True`，`learning_steps=(1min, 10min)`，`relearning_steps=(10min,)`。

### 1.3 完整公式集

#### 1.3.1 遗忘曲线（FSRS-6 新形态）

$$R(t,S) = \left(1 + \mathrm{factor} \cdot \frac{t}{S}\right)^{-w_{20}}, \quad \mathrm{factor} = 0.9^{-1/w_{20}} - 1$$

约束 `R(S,S) = 0.9` 始终成立。

#### 1.3.2 下次区间（给定目标 retention r）

$$I(r, S) = \frac{S}{\mathrm{factor}} \cdot \left(r^{-1/w_{20}} - 1\right)$$

当 `r = 0.9` 时 `I = S`。

#### 1.3.3 初始 S 与 D（首次复习后）

$$S_0(G) = w_{G-1} \quad (G \in \{1,2,3,4\}, \text{即 Again/Hard/Good/Easy 对应 } w_0..w_3)$$

$$D_0(G) = w_4 - e^{w_5 \cdot (G-1)} + 1, \quad \text{clamp 到 } [1, 10]$$

#### 1.3.4 D 更新（次次复习后）

$$\Delta D(G) = -w_6 \cdot (G - 3)$$

$$D' = D + \Delta D \cdot \frac{10 - D}{9} \quad \text{（线性 damping）}$$

$$D'' = w_7 \cdot D_0(4) + (1 - w_7) \cdot D', \quad \text{clamp } [1, 10]$$

注意：mean reversion 目标是 `D_0(4)`（FSRS-5/6 改动，老版本是 `D_0(3)`）。

#### 1.3.5 成功复习后的 S（核心公式，G≥2）

$$S' = S \cdot \left[ e^{w_8} \cdot (11 - D) \cdot S^{-w_9} \cdot (e^{w_{10}(1-R)} - 1) \cdot \mathrm{hard\_penalty} \cdot \mathrm{easy\_bonus} + 1 \right]$$

其中 `hard_penalty = w15 if G==2 else 1`，`easy_bonus = w16 if G==4 else 1`。SInc = S'/S，恒 ≥ 1。

#### 1.3.6 Post-Lapse（G=1）

$$S'_f = \min\left( w_{11} \cdot D^{-w_{12}} \cdot \left[(S+1)^{w_{13}} - 1\right] \cdot e^{w_{14}(1-R)}, \; S \right)$$

`min(…, S)` 保证 post-lapse 不会 > pre-lapse。

#### 1.3.7 同日复习（FSRS-6 修订）

$$S' = S \cdot e^{w_{17}(G - 3 + w_{18})} \cdot S^{-w_{19}}$$

附加约束：当 `G ≥ 3` 时强制 `S' ≥ S`（Good/Easy 不可降 S，Hard/Again 可降）。

### 1.4 参考实现 URL

| 资源 | URL |
|---|---|
| 官方 Python 包（首选） | https://github.com/open-spaced-repetition/py-fsrs |
| 算法 wiki | https://github.com/open-spaced-repetition/awesome-fsrs/wiki/The-Algorithm |
| Expertium 技术讲解 | https://expertium.github.io/Algorithm.html |
| PyPI | https://pypi.org/project/fsrs/ |
| Rust 参考 | https://github.com/open-spaced-repetition/fsrs-rs |
| Benchmark 仓库 | https://github.com/open-spaced-repetition/srs-benchmark |

### 1.5 关键实现注意点

1. **D clamp**：每一步都要把 D clamp 到 `[1, 10]`；S clamp 到 `[0.001, 36500]`（极端值防溢出）
2. **初始 D 不要走 mean reversion**：`D_0` 只走 `w_4 - e^(...) + 1`，不要再乘 `w_7`
3. **w7 接近 0**：当前默认 `w_7 = 0.001`，几乎没有 mean reversion，新 D 主要靠 linear damping
4. **factor 计算**：每次 S/I 计算都要重新算 factor（依赖 `w_20`，但 w_20 在用户优化前是常数）；可以缓存
5. **同日复习判定**：依赖 day cutoff（一般以日历日为界），不是按 24h 滚动
6. **G 编码**：Again=1, Hard=2, Good=3, Easy=4（不是 0-3 也不是 1-5）
7. **post-lapse 的 D 不变**：post-lapse 公式不更新 D，但 D 在 lapse 前已经被 `ΔD = -w6·(1-3) = +2w6` 推高了
8. **first review 的 R 不存在**：所以 `S_0` 直接查表 `w_0..w_3`，不走主公式
9. **Anki fuzz**：生产实现要加 `±5%~25%` 随机化避免雪崩复习（py-fsrs 通过 `enable_fuzzing` 开关）
10. **训练时**：前 4 个参数（`w_0..w_3`）先用 grade-grouped retention curve fitting 估计，再用梯度下降统一优化所有 21 个

---

## 二、SM-17（SuperMemo Algorithm 17）

### 2.1 核心结论速览

- 用户提到的 "DHM" 和 "DTI" **不是 Wozniak 官方术语**——SuperMemo 文档全部用 **DSR 模型**（Difficulty, Stability, Retrievability）
- 与 SM-2 的本质差异：SM-2 用 **EF (Easiness Factor)** 单参数 + 固定 5 等级 grade；SM-17 用 **三维矩阵 SInc[D, S, R]** + 完整重复历史 + 山地爬坡优化
- **没有完整开源 Python 实现**——SuperMemo 本体闭源，只有 [fsrs-vs-sm17 仓库](https://github.com/open-spaced-repetition/fsrs-vs-sm17) 用 SuperMemo CSV 导出的 R 值做对比
- **复现可行性：低-中**，原因见 2.5

### 2.2 核心公式

#### 2.2.1 间隔更新（main loop）

$$\mathrm{Int}[n] = S_w \cdot \mathrm{SInc}[D, S_w, R_w] \cdot \frac{\ln(1 - \mathrm{rFI}/100)}{\ln(0.9)}$$

- `rFI` = requested forgetting index，默认 10%（即目标 retention 90%）
- 当 rFI=10% 时，比值因子 = 1，所以 `Int[n] = S_w · SInc`
- `S_w`、`R_w` 是 pre-review 加权估计

#### 2.2.2 理论 Retrievability（指数衰减）

$$R(t) = e^{-k \cdot t / S}$$

`k` 是 decay constant，标定使 `R(S) ≈ 0.9`，即 `k = -ln(0.9) ≈ 0.10536`。

#### 2.2.3 First Forgetting Curve（首次复习前的异质材料，幂律拟合）

$$R = 0.9906 \cdot \mathrm{interval}^{-0.07}$$

来自 80,399 个 SuperMemo 17 实测样本，跨度 ~10 年。在 `R = 0.9` 处的初始 startup interval ≈ **3.96 天**。

#### 2.2.4 理论 SInc 函数（baseline，新 collection 时用）

$$\mathrm{SInc} = (5 \cdot (1 - D) + 1) \cdot f(R, S) + 1$$

- `D ∈ [0, 1]`，0=最易，1=最难
- `f(R, S)` 由数据拟合，典型形式 `(SIncMax − 1) · S^(-0.529) + 1`，其中 S 指数 ≈ -0.529 是经验值
- f 在 R 中等（30%~80%）时最大，R 接近 100% 或 0% 时小（"desirable difficulty"）
- 难度上限 cutoff：`N = (6·f(R,S) + 1) / (f(R,S) + 1)`，决定 D=1 边界

#### 2.2.5 Difficulty 计算（简化版，可落地）

> 完整版需要 hill-climbing 全历史拟合，简化版（SM-18 起官方推荐）：

$$D[n] = \mathrm{sDF}(D[n-1], \mathrm{Grade}, R_r)$$

- `sDF` 是简化难度函数，类似 SM-8 的 EF 更新但带 R 调节
- 关键性质：近期 repetition 权重 > 远期
- 输出 clamp 到 `[0, 1]`

#### 2.2.6 Stability 三源加权

最终 `S_w` 由三源加权：

1. `S_r` = 从 R 估计反推（用 Recall 矩阵）
2. `S_e` = `S_w[n-1] · SInc[D, S_w[n-1], R_w]`（公式预测）
3. `S_i` = 从 interval 和 grade 反推

权重依赖各矩阵的数据量。

#### 2.2.7 Post-Lapse Stability

$$\mathrm{Int}[1] = \mathrm{PLS}[\mathrm{Lapses}, R]$$

`PLS` 是 2D 矩阵，索引为 lapse 次数和发生时的 R。典型 post-lapse interval 在 `1-4` 天（rFI=10%）。

### 2.3 默认参数清单

SM-17 是 **数据驱动的矩阵算法**，没有像 FSRS 那样的固定 21 维向量。但有以下常量：

| 参数 | 值 | 含义 |
|---|---|---|
| `rFI` (requested forgetting index) | 10% | 目标 forgetting，对应 90% retention |
| `k` (R 衰减常数) | -ln(0.9) ≈ 0.10536 | `R = exp(-k·t/S)` |
| First curve 常数 | `a=0.9906, b=-0.07` | `R = a · t^b` |
| Startup interval | ≈ 3.96 天 | 由 first curve 在 R=0.9 处求得 |
| `S0Max` (startup S 上限) | ~3 个月 | 防 perfect recall 时 S→∞ |
| `BGW` (binary vs grade weight) | 0.7 | `Dev = 0.7·fDev + 0.3·|gDev|` |
| SInc baseline 难度乘子 | `5·(1-D)+1` | D=0→6, D=1→1 |
| S 指数（f(R,S) 经验拟合） | ≈ -0.529 | 来自 SuperMemo 经验回归 |
| Grade 范围 | 0~5 | SuperMemo 6 等级（与 Anki/FSRS 4 等级不同） |
| 矩阵维度 | SInc[D, S, R]、Recall[D, S, R]、PLS[L, R] | 全部 hill-climbing 拟合 |

### 2.4 开源实现现状

| 资源 | URL | 完成度 |
|---|---|---|
| **fsrs-vs-sm17**（benchmark 用） | https://github.com/open-spaced-repetition/fsrs-vs-sm17 | 仅对比，不含完整 SM-17 scheduler；用 SuperMemo 导出 CSV 的 `R(SM17)` 列 |
| supermemo.guru wiki（算法文档） | https://supermemo.guru/wiki/Algorithm_SM-17（重定向到 SM-19） | 公式描述但无代码 |
| Anki 集成讨论 | https://supermemopedia.com/wiki/Adding_Algorithm_SM-17_to_Anki | 被官方拒绝 |
| **FSRS（推荐替代）** | https://github.com/open-spaced-repetition/py-fsrs | 完整 Python，FSRS-6 性能在 19 collection 基准上 **超越 SM-17**（Log Loss 0.367 vs 0.432，FSRS-6 对 SM-17 superiority 83.3%） |

### 2.5 复现可行性：**中下（偏低）**

**可以做**（理论部分）：
- 理论 SInc 公式 `SInc = (5·(1-D)+1) · f(R,S) + 1`，f 用 `(SIncMax − 1)·S^(-0.529) + 1` 近似
- 指数 R 衰减 + 幂律 first curve
- 简化难度函数 sDF（不走 hill-climbing）

**很难做**（数据/矩阵部分）：
- SInc[D, S, R] 三维矩阵需要 **数十万次 repetition** 才能填出 SuperMemo 论文里的形态（61k reps 才填到 Diff=0.8 一层）
- BestSInc() 的混合策略（theoretical + matrix + neighbor interpolation）权重未公开
- Recall[D, S, R] 矩阵与 R 三源加权的精确权重未公开
- PLS 矩阵的 lapse-R 二维查表数据未公开
- hill-climbing 优化 D 的具体步长、收敛条件未公开

**结论**：要做"教科书级 SM-17"复现 → 不现实；做"SM-17 风味的 DSR 简化版"（用理论公式 + 简化 D 更新，不走矩阵）→ 可行，大约 300~500 行 Python。但在 benchmark 数据上 **大概率打不过 FSRS-6**（fsrs-vs-sm17 已证明 FSRS-6 比真 SM-17 还强）。

### 2.6 关键实现注意点（若选择复现）

1. **Grade 系统映射**：SM-17 用 0~5（6 等级），FSRS/Anki 用 1~4（4 等级），实现时要明确约定一种
2. **D 范围**：SuperMemo 用 `[0, 1]`，反向于 FSRS 的 `[1, 10]`；切勿混用
3. **R 公式两套**：first curve 用幂律 `R = 0.9906·t^(-0.07)`，subsequent 用指数 `R = exp(-k·t/S)`，**不要混用**
4. **rFI vs retention**：`retention = 1 − rFI/100`；公式里出现 `ln(0.9)` 的地方都隐含 rFI=10% 假设
5. **simplified mode**：除非有海量真实数据，否则用简化难度（D[n] = f(D[n-1], grade, R)）+ 理论 SInc 公式即可
6. **S0Max 必加**：连续完美 recall 会导致 S→∞，必须钳制（SuperMemo 用 ~90 天）
7. **post-lapse S 不归零**：SM-17 不像 SM-2 完全重置，而是用 PLS 矩阵；简化版可以用 `S_new = max(S_min, w · S_old)`，`w ∈ [0.1, 0.5]`
8. **历史依赖**：完整 SM-17 每次新 repetition 都重算整个历史的 D 和 S 演化；简化版可只看上一步
9. **Universal Metric**：评估时用 SuperMemo 官方提出的 UM（按 R 预测分箱后看实际 recall 偏差），不要只看 Log Loss
10. **避免 reinvent**：实际项目中如果需要 SM-17 级别精度，**直接用 FSRS-6**——已被 SuperMemo 官方[供认击败](https://supermemopedia.com/wiki/SuperMemo_dethroned_by_FSRS)

---

## 三、推荐落地路径（针对 wordforge）

| 算法 | 复现难度 | 推荐选择 |
|---|---|---|
| FSRS-6 | 低（300 行 Python 可独立实现） | **首选**；直接 `pip install fsrs` 或自行复刻 21 参数公式 |
| SM-17（完整） | 极高（无开源 + 闭源矩阵） | **不推荐** |
| SM-17（DSR 风味简化版） | 中（理论 SInc + 简化 D + 指数 R） | 仅当需要"双算法对比"时考虑 |

**结论**：FSRS-6 的数学完整度、性能、可复现性、社区支持全面碾压 SM-17。若仅需"主流间隔重复算法"，独立实现 FSRS-6 即可。
