# WordForge 核心算法文档

## 概述

WordForge 是一个算法驱动的自适应英语词汇学习平台，后端采用 Rust (Axum) 编写，前端采用 SolidJS。其核心是 **AMAS（Adaptive Mastery Acquisition System）** 引擎，融合认知科学与机器学习，实现个性化的间隔复习调度。

---

## 一、MDM — 多维记忆模型（Multi-Dimensional Memory Model）

### 1.1 状态结构

每个单词维护一个 `MdmState`，包含三条独立的记忆时间轴：

| 字段 | 说明 |
|---|---|
| `short_term` | 短期记忆强度 |
| `medium_term` | 中期记忆强度 |
| `long_term` | 长期记忆强度 |
| `consolidation` | 记忆巩固度 |
| `memory_strength` | 综合记忆强度（最终输出） |
| `correct_streak` | 连续正确次数 |
| `recent_results` | 最近 20 次作答记录 |

### 1.2 综合强度计算

三维记忆分量通过固定权重线性融合：

$$composite = 0.20 \times S_{short} + 0.30 \times S_{medium} + 0.50 \times S_{long}$$

权重设计体现认知科学原理：长期记忆对掌握度的贡献最大（50%），短期记忆权重最低（20%）。

### 1.3 强度更新（指数移动平均）

每次作答后，各维度以不同学习率向答题质量 $q \in [0,1]$ 收敛：

$$S_{short} \mathrel{+}= 0.50 \times (q - S_{short})$$
$$S_{medium} \mathrel{+}= 0.20 \times (q - S_{medium})$$
$$S_{long} \mathrel{+}= 0.12 \times (q - S_{long})$$

巩固度更新：

$$\Delta c = 0.14 \times (q - 0.5) \times 2.0$$

综合强度最终更新（虚拟增益叠加巩固效果后再做 EMA）：

$$composite_{virtual} = composite \times (1 + c \times 0.6)$$
$$memory\_strength \mathrel{+}= \alpha \times (composite_{virtual} - memory\_strength)$$

### 1.4 被动衰减（Passive Decay）

未复习期间，各维度自然衰减，但速率不同（长期记忆衰减更慢）：

$$decay = \left(1 + \frac{elapsed\_days}{30}\right)^{-0.30}$$

$$S_{short} \mathrel{\times}= decay, \quad S_{medium} \mathrel{\times}= decay$$
$$slow\_decay = 1 - 0.7 \times (1 - decay)$$
$$S_{long} \mathrel{\times}= slow\_decay, \quad c \mathrel{\times}= slow\_decay$$

---

## 二、遗忘曲线与复习调度

### 2.1 回忆概率（Forgetting Curve）

采用指数衰减模型，时间常数由记忆强度决定：

$$tc = (ms + 0.3)^{1.5} \times 1{,}296{,}000 \text{ (s)}$$

$$R(t) = e^{-\Delta t / tc}$$

其中 $1{,}296{,}000$ 秒 = 15 天，为基础时间单位。

各强度对应的半衰期：

| 记忆强度 $ms$ | 时间常数 $tc$ | 半衰期 |
|---|---|---|
| 0.2 | ~91,652 s | ~17.7 小时 |
| 0.5 | ~185,474 s | ~2.1 天 |
| 0.9 | ~340,623 s | ~3.9 天 |

### 2.2 最优复习间隔

以目标保留率 $r_0 = 0.85$ 反推最佳复习时间点：

$$interval = -tc \times \ln(r_0)$$

结果 clamp 至 $[60\text{ s},\ 90\text{ 天}]$。

---

## 三、智能选词算法（Word Selector）

核心得分公式融合了四个认知/信息论因子：

$$score = \underbrace{(1 - R + bonus_{risk})}_{\text{遗忘风险}} \times \underbrace{cooldown}_{\text{冷却}} \times \underbrace{bonus_{zone}}_{\text{最佳区间}} + \underbrace{bonus_{UCB}}_{\text{探索}}$$

### 3.1 各因子定义

**① 风险奖励**（sigmoid，recall 低时放大紧迫性）：

$$bonus_{risk} = \frac{0.2}{1 + e^{-(0.55 - R) \times 8.0}}$$

**② 冷却因子**（防止短时间内重复推送同一词）：

$$cooldown = 1 - e^{-elapsed\_secs / 300}$$

**③ 最佳区间奖励**（高斯分布，中心 $R^* = 0.65$，$\sigma = 0.20$）：

$$bonus_{zone} = \exp\left(-\frac{(R - 0.65)^2}{2 \times 0.20^2}\right)$$

依据"可取难度（Desirable Difficulty）"原理：recall ≈ 0.65 时，复习带来的记忆增益最大化。

**④ UCB 探索奖励**（Upper Confidence Bound，平衡高低频词）：

$$bonus_{UCB} = \min\left(0.12 \times \sqrt{\frac{\ln(N+1)}{attempts+1}},\ 0.35\right)$$

$N$ 为总复习次数，$attempts$ 为该词被选中次数。

---

## 四、FSRS-5 记忆稳定性模型

FSRS-5（Free Spaced Repetition Scheduler v5）是学术界验证的间隔重复算法，作为系统内置基线和参数优化目标，含 **19 个可学习参数** $w[0..18]$。

### 4.1 遗忘概率（幂律衰减）

$$R(t, S) = \left(1 + factor \times \frac{t}{S}\right)^{decay}$$

其中 $factor = 0.30$，$decay = -0.50$（对应 $R = 0.9$ 时的期望复习间隔）。

### 4.2 稳定性更新

**首次学习：**

$$S_0 = w[grade - 1], \quad grade \in \{1,2,3,4\}$$

**同日复习（$elapsed < 1$ 天）：**

$$S' = S \times e^{\text{clamp}(w[17] \times (grade - 3 + w[18]),\ -20,\ 20)}$$

**常规成功复习：**

$$S' = S \times \left(1 + e^{w[8]} \times (11 - D) \times S^{-w[9]} \times (e^{w[10](1-R)} - 1) \times bonus\right)$$

**遗忘后重学：**

$$S' = w[11] \times D^{-w[12]} \times \left((S+1)^{w[13]} - 1\right) \times e^{w[14](1-R)}$$

### 4.3 难度更新（均值回归）

$$\Delta D = -w[6] \times (grade - 3)$$
$$D' = w[7] \times D_0(4) + (1 - w[7]) \times \left(D + \Delta D \times \frac{10 - D}{9}\right)$$

### 4.4 初始参数

```
w = [0.2, 0.6, 1.6, 6.0,           # 初始稳定性（Again/Hard/Good/Easy）
     7.1949, 0.5345, 1.4604, 0.0046, # 难度相关
     0.9, 0.18, 0.6,                  # 稳定性增长
     1.2, 0.08, 0.20, 1.3,            # 遗忘后稳定性
     0.2315, 2.9898,                  # Hard/Easy bonus
     0.51655, 0.6621]                 # 同日复习
```

---

## 五、GRU Oracle 神经网络（离线上界）

作为参数搜索的"上界参照"，GRU Oracle 基于真实历史序列预测遗忘概率。

### 5.1 模型架构

```
输入序列: [log1p(间隔天数), 答题结果, 归一化难度]  × T步
    ↓
GRU(input=3, hidden=64, batch_first=True)
    ↓
Linear(65 → 1)  +  softplus 激活
    ↓
半衰期预测 ĥ（天）
    ↓
遗忘概率: p = 2^(−next_t / ĥ)
```

### 5.2 训练策略

- **损失函数**：Binary Cross-Entropy
- **优化器**：Adam + Cosine 学习率调度（含 warmup）
- **数据**：墨墨背单词数据集（2.2 亿条，Harvard Dataverse DOI: 10.7910/DVN/VAGUL0）
  - 按用户哈希分割：80% train / 10% val / 10% test

### 5.3 概率校准

GRU 输出概率经三种校准器竞争选择（最小化 logLoss + MAE + AUC）：

| 校准器 | 方法 |
|---|---|
| IdentityCalibration | 恒等变换 |
| IsotonicCalibration | 保序回归 |
| LogisticCalibration | Platt 缩放 |

---

## 六、参数搜索（Optuna 三阶段漏斗）

### 6.1 目标函数

$$objective = 0.55 \times predictionScore + 0.45 \times policyScore$$

- **predictionScore**：候选参数在 logLoss/ICI/AUC/MAE 上相对基线的综合改善
- **policyScore**：$safety \times efficiency$，衡量调度决策质量（基于 GRU Oracle 定义最优区间）

### 6.2 三阶段筛选

| 阶段 | 试验数 | 数据量 | 保留数 |
|---|---|---|---|
| Stage 1 | 128 次 Optuna TPE | 2% 用户 | Top 16 |
| Stage 2 | 16 组配置 | 10% 用户 | Top 4 |
| Stage 3 | 4 组配置 | 100% 用户 | 通过门槛者 |

**Stage 3 通过条件**：
- $predictionGain \geq 2\%$
- $intervalGain \geq 5\%$
- DHP expectedMemory 退步不超过 10%

---

## 七、DHP 参考基线（墨墨算法）

DHP（Difficulty-Halflife-Performance）是墨墨背单词的核心算法，用作工业对照基准。

**成功复习（记忆增强）：**

$$h' = h \times \left(1 + e^{r_a} \times d^{r_b} \times h^{r_c} \times (1-p)^{r_d}\right)$$

**遗忘后重学：**

$$h' = e^{f_a} \times d^{f_b} \times h^{f_c} \times (1-p)^{f_d}$$
$$d' = \min(d + 2,\ 18)$$

其中 $h$ 为半衰期，$d$ 为难度，$p$ 为当前回忆概率，共 8 个参数。

---

## 八、疲劳检测（Fatigue Detection）

在用户界面层，系统通过摄像头实时检测学习疲劳并联动调整题目难度。

### 8.1 处理流水线

```
摄像头帧
  ↓ ImageBitmap
Web Worker（零主线程阻塞）
  ├── MediaPipe FaceLandmarker → 478 个面部关键点
  └── Rust WASM 疲劳算法
       ├── EAR（眼睛宽高比）→ PERCLOS（60s 窗口眼睑闭合比例）
       ├── MAR（嘴部宽高比）→ 哈欠检测
       ├── 头部姿态（pitch / yaw / roll）
       └── 五维加权融合 → 疲劳分 [0, 100]
```

### 8.2 疲劳联动策略

| 疲劳等级 | 阈值 | AMAS 响应 |
|---|---|---|
| 严重疲劳 | > 75 | 硬约束：最大出题难度 ≤ 0.55 |
| 中度疲劳 | 50-75 | 降低新词引入比例 |
| 正常 | < 50 | 正常调度 |

所有视频处理完全在浏览器 Worker 内完成，**零数据上传**。

---

## 九、A/B 测试框架（amas_ab_test.py）

对比三种调度策略（50 词 × 30 天 × 每天最多 20 轮 × 300 次随机试验）：

| 策略 | 核心机制 |
|---|---|
| **AMAS** | `score_review_word` 动态评分 |
| **Leitner** | 艾宾浩斯箱子（1/2/4/7/14 天固定间隔） |
| **Random** | 每天随机选词 |

**验收标准**：

- 隔夜 recall（24h 后）≥ 40%
- AMAS 比 Leitner 高 ≥ 10 个百分点
- 30 天后 ≥ 30% 词达到掌握（mastery）

**掌握判定**：

```python
is_mastered = (composite_strength > 0.5
               and mean(recent_results[-20:]) > 0.65
               and correct_streak >= 1)
```

---

## 十、系统关键参数总览

| 参数 | 值 | 说明 |
|---|---|---|
| `shortTermLearningRate` | 0.85 | 短期记忆学习率 |
| `mediumTermLearningRate` | 0.20 | 中期记忆学习率 |
| `longTermLearningRate` | 0.12 | 长期记忆学习率 |
| `compositeWeightShort/Med/Long` | 0.20 / 0.30 / 0.50 | 三维权重 |
| `halfLifeBaseEpsilon` | 0.30 | 半衰期公式基础偏移 |
| `halfLifePower` | 1.5 | 半衰期幂指数 |
| `halfLifeTimeUnitSecs` | 1,296,000 | 基础时间单位（15 天） |
| `baseDesiredRetention` | 0.85 | 调度目标保留率 |
| `passiveDecayPower` | 0.30 | 被动衰减幂次 |
| `zoneCenter / zoneSigma` | 0.65 / 0.20 | 最佳复习区间高斯参数 |
| `ucbMaxBonus` | 0.35 | UCB 探索奖励上限 |

---

*本文档由 **Tencent Cloud Code Assistant** IDE 插件配合 **腾讯云代码助手智能编程模型** 自动生成。*
