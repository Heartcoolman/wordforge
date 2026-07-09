# GSP 规格契约（Graduated Scheduling Policy）

> 本文件是 bench mirror（Python）GSP 实现的**精确契约**，也是后续 Rust 生产移植的对拍基准。
> 所有公式、运算次序、入口 clamp 一律以本文为准；Python↔Rust max err ≤ 1e-9（预注册闸门 G6）。
>
> 实现位置：
> - banded retention 求解 → `dhp_reference.py::WordforgeMirrorState.interval_days()` / `_gsp_banded_retention()`
> - 毕业下限 + 区间帽 → `schedulers.py::AMASScheduler.next_interval_days()`
> - config → state 入口 clamp → `dhp_reference.py::_gsp_band_kwargs_from_config()` + `AMASScheduler.__init__`

---

## 1. 配置键（camelCase，candidate dict 内）

全部默认 = **关闭** = 与不带该键的 `DEFAULT_MEMORY_MODEL_CONFIG` 逐位等价（bit-exact legacy）。

| 键 | 类型 | 默认 | 关闭条件 | 语义 |
|---|---|---|---|---|
| `gspIntervalCapDays` | float | `0.0` | `== 0` | 调度区间硬帽（天）。作用于 ensemble interval_scale **之后**，与既有 90 天硬帽复合为 `min(90, cap)`。 |
| `gspGraduationStreak` | int | `0` | `== 0` | 毕业连击阈值 k。当词的 `correct_streak >= k` 时，调度区间获得 `gspGraduationFloorDays` 下限。 |
| `gspGraduationFloorDays` | float | `30.0` | （随 streak 关闭而失效） | 毕业下限（天）。在 scale 之后、cap 之前施加 `max(interval, floor)`。 |
| `gspYoungRetention` | float | `0.0` | （随 band 关闭而失效） | 年轻卡（`stability < band`）目标保持率。 |
| `gspMatureRetention` | float | `0.0` | （随 band 关闭而失效） | 成熟卡（`stability >= band`）目标保持率。 |
| `gspMaturityBandDays` | float | `0.0` | `<= 0` | 成熟度分带阈值（天，按 stability）。`> 0` 时，interval 求解的目标保持率由 young/mature 替换自适应 `desired_retention`。 |
| `gspIntervalFuzz` | float | `0.0` | `== 0` | 复习负载平滑：确定性区间抖动幅度。`> 0` 时在 cap 之后施加 `days·(1 + fuzz·u)`，`u∈[-1,1)` 由 in-state 量派生（见 §7）。错开同步到 cap/floor 的复习波，降低每日 recall 方差（提升 retentionStability / G4）。 |
| `gspSuccessGrade` | int | `3` | `== 3`（即旧默认 Good） | 二元成功复习映射的 FSRS grade。`3`=Good（bit-exact legacy=benchmark_adapter `SUCCESS_QUALITY=0.7`）；`4`=Easy。**v5 架构关键修正**：FSRS-6 公版 21 维 w 与 grade=4 共拟合，grade=3 会令预测腿 ICI 失守（mai 2.5×/syn +16% over amas6）。设 `4` 时进入「FSRS-6 faithful」状态路径（见 §3.5），逐位等价 `FSRS6MirrorState`（amas6 公版核心）→ 预测腿 = amas6 级（G3 满余量过门）。失败恒映射 `1`=Again。 |

---

## 2. 入口 clamp（镜像纪律）

bench adapter **不调** `validate()`，故越界旋钮值在**进入 state 前**夹紧（与 Rust 入口 clamp 对齐）：

- `gspIntervalCapDays`：`max(0.0, x)`（负数 → 0 → 视为关闭）。`AMASScheduler.__init__`。
- `gspGraduationStreak`：`max(0, int(x))`（负数 → 0 → 关闭）。`AMASScheduler.__init__`。
- `gspGraduationFloorDays`：`max(0.0, float(x))`。`AMASScheduler.__init__`。
- `gspMaturityBandDays`：`max(0.0, x)`（负数 → 0 → 关闭）。`_gsp_band_kwargs_from_config`。
- `gspIntervalFuzz`：`max(0.0, min(1.0 - 1e-9, x))`（负数 → 0 → 关闭；>=1 会令 `(1+fuzz·u)` 在 `u→-1` 时触及非正区间，故顶侧夹至 `1-1e-9`）。`AMASScheduler.__init__`。
- `gspYoungRetention` / `gspMatureRetention`：`max(0.0, min(1.0, x))`（夹至曲线合法保持率域）。`_gsp_band_kwargs_from_config`。
- `gspSuccessGrade`：`x if x in {3,4} else 3`（非合法成功成绩域一律夹回 3=Good）。`_gsp_band_kwargs_from_config`。

banded retention 求解时，young/mature 进一步夹至 `max(1e-6, min(1.0, target))`（避免 `pow` 域错误），见 `_gsp_banded_retention()`。

---

## 3. 运算次序（单次 `next_interval_days()` 求值）

严格按以下顺序，**不可交换**：

```
1. banded retention 求解目标保持率 R_target:
       若 gspMaturityBandDays > 0:
           R_target = clamp(gspYoungRetention,  1e-6, 1.0)  当 stability <  gspMaturityBandDays
           R_target = clamp(gspMatureRetention, 1e-6, 1.0)  当 stability >= gspMaturityBandDays
       否则:
           R_target = desired_retention（自适应口径，不变）

2. base interval（天，浮点）= 解曲线 R(t,S)=R_target 后顶侧夹 maxIntervalDays(=90):
       adjusted = clamp((R_target - floor)/(1 - floor), 1e-6, 1.0)
       days_raw = S / factor * (adjusted^(1/decay) - 1)
       base_days = min(days_raw, maxIntervalDays)          # WordforgeMirrorState._interval_days_raw
       base_days_int = max(1, ceil(base_days))             # WordforgeMirrorState.interval_days（整数）
       # 注：AMASScheduler 读取的是 base_days_int（已 ceil）。

3. ensemble interval_scale:
       scale = max(0.1, ensemble_interval_scale())         # ensemble.rs:131 保真，无上界、无分带
       scaled_days = max(1.0, base_days_int * scale)

4. 毕业下限（floor）—— scale 之后、cap 之前:
       若 gspGraduationStreak > 0 且 correct_streak >= gspGraduationStreak:
           scaled_days = max(scaled_days, gspGraduationFloorDays)

5. 区间帽（cap）:
       cap = 90.0
       若 gspIntervalCapDays > 0: cap = min(cap, gspIntervalCapDays)
       scaled_days = min(scaled_days, cap)

5.5 区间抖动（fuzz）—— cap 之后、取整之前（见 §7，默认关闭即 no-op）:
       若 gspIntervalFuzz > 0:
           u        = 2·frac(stability·12.9898 + review_count·78.233) - 1   # ∈ [-1, 1)
           fuzzed   = scaled_days · (1 + gspIntervalFuzz · u)
           fuzz_cap = min(90, cap · (1 + gspIntervalFuzz))
           lo       = gspGraduationFloorDays  当 graduated   否则 1.0
           scaled_days = clamp(fuzzed, lo, fuzz_cap)

6. 取整 + 底侧夹:
       return max(1, round(scaled_days))                   # round=banker's? 见下
```

**取整语义**：`int(round(scaled_days))` —— Python 3 `round` 为 banker's rounding（round-half-to-even）。
Rust 移植须用等价语义（如 `(scaled_days).round()` 为 round-half-away-from-zero，会与 Python 在 .5 边界差 1，
对拍时须显式对齐——推荐 Rust 端用 round-half-to-even 或在测试中规避恰好 .5 的输入）。
`base_days` 的 `ceil` 与既有 mdm.rs `compute_interval` 一致，不变。

**时间不变性（红线）**：步骤 4 仅读 `correct_streak`（状态量），**不读模拟日、视界、剩余天数**。
banded retention 仅读 `stability`（状态量）。步骤 5.5 fuzz 仅读 `stability`/`review_count`（状态量）。
GSP 全链路时间不变，满足 G1 完整性要求。

---

## 3.5 FSRS-6 faithful 状态路径（`gspSuccessGrade == 4`）

`WordforgeMirrorState.update()` 在 `success_grade == 4 且 review_count > 0` 时切换到 FSRS-6 参考次序，
逐位等价 `schedulers.py::FSRS6MirrorState`（amas6 公版核心）。与旧 mdm.rs 镜像（grade=3 路径）的差异：

1. **难度先更新**：先算 `self.difficulty`（mean-reversion 锚 D0(Easy=4)），S 公式用**更新后**的 `self.difficulty`（而非 `prev_difficulty`）。旧路径用 `prev_difficulty`，post-lapse 处发散（D 跳变时差异显著）。
2. **无同日特化分支**：FSRS-6 参考对 `elapsed < 1` 仍走常规成功/遗忘公式（`w17/w18/w19` 同日分支不参与）。
3. **alpha 平滑**：v5 候选 alpha 钉死 1.0（`alphaMin=alphaMax=1.0`），平滑为 no-op；故 grade=4 路径直接赋值 target，与 FSRS6MirrorState 一致。

**实证**：grade=4 + 本次序下三数据集预测腿（LL/ICI/AUC）与 amas6 **逐位相同**（max err ≤ 1e-9，见 `tests/test_gsp.py::test_success_grade4_matches_fsrs6_mirror`，300 随机序列）。
这是 v5「未平滑 FSRS-6 核心 = amas6 预测腿（免费）」契约的兑现条件——**仅当 grade=4 才成立**；grade=3（旧默认）会令 mai/syn 预测腿 ICI 失守 G3。

**Rust 移植契约**：生产化时，`gspSuccessGrade=4` 须令 mdm.rs `update_strength` 对二元成功映射到 Easy(4) 并采用 FSRS-6 参考次序（D 先更新、S 用新 D、无同日特化）。G6 镜像 parity 要求 Python↔Rust 双侧同步此路径。
**时间不变**：仅依赖成绩（`recalled`），不读模拟日/视界，满足 G1。

---

## 4. 状态量来源

- `correct_streak`：`WordforgeMirrorState` D5 已跟踪（成功且 gap_ok 自增、失败清零、同日成功冻结）。
  `AMASScheduler.next_interval_days` 经 `getattr(self._state, "correct_streak", 0)` 读取——
  `AMAS6Scheduler` 底层为 `FSRS6MirrorState`（无此字段），getattr 兜底 0 → 即便误配 GSP 也恒不触发毕业。
- `stability`：mirror state 的 `stability` 字段。

---

## 5. 隔离边界（哪些 scheduler 吃 GSP）

| scheduler | 吃 GSP？ | 机制 |
|---|---|---|
| `amas`（AMASScheduler） | **是** | 从 candidate `memory_config` 读 `gsp*` 键。 |
| `amas6`（AMAS6Scheduler） | **否** | `__init__` 硬置 `_gsp_*=off`（含 `_gsp_interval_fuzz=0.0`）；state 为 FSRS6MirrorState（结构上无 banded retention 字段）。 |
| `fsrs`（FSRSScheduler） | **否** | 固定绑 `FSRS_BASELINE_CONFIG`（`gsp*=0` 显式声明）；其 state 虽为 WordforgeMirrorState，但 cap/floor/fuzz 在 FSRSScheduler 路径不参与（FSRSScheduler.next_interval_days 直接返回 state.interval_days()，无 GSP 包裹）。 |
| `fsrs45` / `fsrs6` / 其余竞品 | **否** | 硬编码公版 w，独立 state，天然隔离。 |

---

## 6. Candidate 基础配置（v5 架构起点）

未平滑 FSRS-6 核心 + 公版 21 维 w + GSP 旋钮（下表为**基础态**，各 GSP 旋钮在搜索中赋值）：

```python
{
    # —— 未平滑 FSRS-6 核心：alpha 平滑钉死 alpha_eff=1，D5 双腿 ramp no-op ——
    "alphaScale": 1.0,
    "alphaMin":   1.0,
    "alphaMax":   1.0,
    "alphaRampTau":      0.0,
    "alphaLapseRampTau": 0.0,
    # —— FSRS-6 公版 21 维 w（_FSRS6_DEFAULT_W，预测腿 = amas6 级，已知免费）——
    "w": [
        0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001,
        1.8722, 0.1666, 0.796, 1.4835, 0.0614, 0.2629, 1.6483, 0.6014,
        1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
    ],
    # 曲线参数由 w[20] 派生（兜底未升级消费方；AMASScheduler len(w)>=21 时自派生忽略）
    "forgettingCurveFactor": 0.9 ** (-1.0 / 0.1542) - 1.0,
    "forgettingCurveDecay":  -0.1542,
    "forgettingCurveFloor":  0.0,
    "baseDesiredRetention":  0.85,
    # —— v5 关键修正：成功成绩带 Easy(4)，进入 FSRS-6 faithful 路径（§3.5）——
    # grade=4 → 预测腿逐位 = amas6（G3 满余量过门）；grade=3（默认）会令 mai/syn ICI 失守。
    "gspSuccessGrade":        4,
    # —— GSP 调度策略头（基础态全关；搜索中按候选网格赋值）——
    "gspIntervalCapDays":     0.0,   # 候选 {40,45,60,75,89}
    "gspGraduationStreak":    0,     # 候选 {2, 3}
    "gspGraduationFloorDays": 30.0,  # 候选 {30, 35}（< 30 会令 duo/syn 掉出 mastered≥30 阈值）
    "gspYoungRetention":      0.0,   # 分带：年轻卡高保持率
    "gspMatureRetention":     0.0,   # 分带：成熟卡低保持率
    "gspMaturityBandDays":    0.0,   # 候选 > 0 启用分带
    "gspIntervalFuzz":        0.0,   # 候选 {0.05, 0.10, 0.15, 0.20}（复习负载平滑 / G4）
}
```

> 其余 `DEFAULT_MEMORY_MODEL_CONFIG` 键（mastery 阈值、fatigue、recall risk 等）保持 DEFAULT，
> 不影响 FSRS-6 核心动力学与 GSP；candidate dict 以 `{**DEFAULT, **上表}` 形式构造即可。

---

## 7. 区间抖动 / 复习负载平滑（`gspIntervalFuzz`）

### 7.1 动机（G4 机制）

F1 冠军（cap=40, floor=30）下 maimemo 90 天 sim 的 retentionStability 失守 G4（0.8866 < 0.895）。
波形诊断（见 campaign Step 1）确认根因：**毕业/帽词的到期日同步**——最终 next_interval 分布里
**10018 词钉在 cap=40、10789 词落在 30-39 floor 带**（共 69% 活跃词），导致 day 30/40/70/80
（30 与 40 的倍数）出现 8-9× 中位数的复习波，当日 recall_rate 暴跌（day 40: reviews_done=13084,
recall=0.456；day 30: 7649, recall=0.386）→ 每日 recall_rate 方差被这些同步波拉高。

`gspIntervalFuzz` 在 cap 之后对最终调度区间施加**确定性、词级**的乘性抖动，把钉在同一天的词
错峰散开到 cap±fuzz 邻域，削平复习波 → 降低每日 recall_rate stdev → 抬高 retentionStability。

### 7.2 抖动量公式（运算次序固定，G6 Rust 对拍基准）

```
u = 2.0 * frac(stability * 12.9898 + review_count * 78.233) - 1.0      # ∈ [-1, 1)
```

精确实现（`AMASScheduler._fuzz_u`，Python float = IEEE-754 f64）：

```python
h = stability * 12.9898 + float(review_count) * 78.233   # 先各自乘，再加（f64 fma 顺序）
f = h - math.floor(h)                                     # 小数部分 ∈ [0, 1)
u = 2.0 * f - 1.0                                         # 线性映射到 [-1, 1)
```

- **常数**：`12.9898` / `78.233` 取自经典 GLSL `fract(sin(dot(...)))` hash 家族，
  但**去掉 `sin`** —— 直接对线性组合取小数部分。去 `sin` 是为保证 Python↔Rust 完全可移植
  （跨平台 `sin` 末位可能差 1 ulp，破坏 G6 ≤1e-9 对拍）；`frac` 对大 mantissa 同样产生均匀散布。
- **状态量来源**：`stability`（mirror state 浮点字段）+ `review_count`（mirror state 整数计数）。
  两者皆 in-state、身份无关（不含 user_id/word_id）、时间不变（不读模拟日/视界/日历）。
- **确定性**：同一 (stability, review_count) → 同一 u。无 RNG、无全局状态。

### 7.3 施加点与重夹纪律

在 `next_interval_days()` **步骤 5（cap）之后、步骤 6（取整）之前**：

```
fuzzed   = scaled_days * (1.0 + gspIntervalFuzz * u)
fuzz_cap = min(90.0, cap * (1.0 + gspIntervalFuzz))     # cap = min(90, gspIntervalCapDays)
lo       = gspGraduationFloorDays  当 graduated 为真    否则 1.0
scaled_days = max(lo, min(fuzz_cap, fuzzed))
```

- **顶侧夹 `fuzz_cap`**：允许抖动把 cap-pinned 词推到 cap 之上错峰（这是削平 cap 波的关键），
  但绝不越 90 天硬帽（`min(90, ·)`）。
- **底侧夹 `lo`**：`graduated`（即 `gspGraduationStreak>0 且 correct_streak>=streak`，
  与步骤 4 同一判定）为真时夹至 `gspGraduationFloorDays`——**fuzz 不得把已毕业词拉到 floor 之下**，
  否则破坏 mastered 定义（`next_interval_days >= 30`，G2/mastered 计数依赖此）。非毕业词夹至 1。
- **入口 clamp**：`gspIntervalFuzz` 在 `__init__` 夹至 `[0, 1-1e-9]`（§2）；`==0` 时整段 no-op
  （bit-exact legacy，G5 验证）。

### 7.4 隔离与时间不变

- 仅 `AMASScheduler.next_interval_days` 路径施加；`amas6` 硬置 `_gsp_interval_fuzz=0.0`，
  `fsrs`/`fsrs45`/`fsrs6`/`sm2`/`hlr`/`leitner`/`random` 不经 GSP 包裹，天然隔离。
- 全公式仅读状态量，满足 G1 时间不变红线。

**Rust 移植契约**：mdm.rs `compute_interval`（或等价 next-interval 路径）须在 cap 之后、取整之前
插入 §7.2/§7.3 同序逻辑，`_fuzz_u` 用 f64 同常数同次序，graduated 判定与 floor 复用既有分支。
G6 镜像 parity（Python↔Rust max err ≤ 1e-9）须覆盖 fuzz on/off 两态。

---

## 8. SSP 后端（Cost-ADR；状态相关 DR）— T1.4

SSP 是 GSP 的**可选 base 求解后端**：以「达目标稳定性的期望总复习代价最小」的 3D Bellman DP
（`src/amas/memory/ssp.rs::precompute`）替代单一 `baseDesiredRetention` 求 base 区间。DP 在
(difficulty 1..10) × (stability bins) 上以目标保持率 R 为 action，产出 **(S,D)→最优 R 曲面**
（`optimal_r`，即状态相关 DR），随 stability×difficulty 连续变。默认 `sspEnabled=false`（休眠，
走 §3 MDM banded base）。

### 8.1 base 区间运算次序（SSP 后端，与 mastery.rs 同序）

GSP 激活且 SSP 后端时，§3 的「base 求解」被替换为：

```
base_days_int = max(1, ceil( min( optimal_interval(stability, difficulty), maxIntervalDays ) ))
```

其中 `optimal_interval(S,D)` = 从 `optimal_r[d_index][s_index]` 反演的最优区间天数
（`d_index = clamp(round(difficulty),1,10)-1`；`s_index` 经双网格 `partition_point` 或均匀
`ln(S)/ln(base)-min_index` 量化）。**其后 §3 的 GSP head（interval_scale → 毕业 floor → cap →
fuzz → 取整）原样施加**，与底层求解器无关（MDM/SSP 两后端统一收口）。banded retention 仅在
MDM 后端替换目标保持率口径；SSP 后端 base 由 DP 决定、与 banded 无关。

### 8.2 parity 接口

`maimemo_mdm_adapter` 请求/响应（camelCase）新增：

- 请求 `sspConfig`（可选 `SspConfig`）：Some → 预计算 SSP DP 后端（同 engine.rs 构造，dual_grid
  二选一 + 保留 optimal_r）；None → MDM 路径（bit-exact legacy）。
- 响应 `optimalRetention`（可选 f64）：该 (stability, difficulty) 的状态相关最优 R（DR 曲面值），
  仅 SSP 后端为 Some，供 Python↔Rust 对拍 DR 曲面。
- 响应 `scheduledIntervalDays`：SSP 后端时由 §8.1 base + §3 head 产出。

**Rust↔Python parity 契约**：Python `SSPMMCScheduler` 的 optimal_interval 与本仓 `ssp.rs`
`SspPolicy::optimal_interval`、optimalRetention 与 `SspPolicy::optimal_retention` 须逐位一致
（max err ≤ 1e-9），覆盖 dual_grid on/off。该后端此前在对拍中**零覆盖**，复活/灰度前须先建此 parity。

### 8.3 成本曲线与干净指标（`cost_curve.py`）

- `cost_curve.py` 沿 `desiredRetention` 网格各跑一次 forward-sim，产 reviewsPerDay-vs-实测保持率
  凸曲线，并插值出 **`reviewsPerDayAtMatchedRetention`**（在同一目标保持率下比复习量，floor-无关、
  可证伪；不外推）——这才是 Cost-ADR「达目标保持率的总复习次数最小」的对照口径。
- `simulate.py` 的 `mastered_count` 已与 `gspGraduationFloorDays` **解耦**：改用
  `oracle_halflife >= MASTERED_HALFLIFE_DAYS(180)` 衡量真实长期保持，不再读调度区间
  （旧 `next_interval>=30` 与默认 floor=30 同阈值，自我实现、不可证伪）。
- ⚠️ 墨墨「离线≠真实留存」：下调 DR=拿留存换 workload，离线 reviewsPerDay 降不等于线上留存不掉，
  SSP/Cost-ADR 任何采纳须经 T1.3 真实 A/B 验证。

## §9 退役（retire）—— 2026-07-08 真半衰期口径战役

### 9.1 语义与判据

毕业后「过学」词直接冻结到长间隔，跳过 cap/fuzz（可越区间帽）。在 §3 head 的
毕业 floor（步骤 4）与 cap（步骤 5）之间插入步骤 4.5：

```
if retire_after_reviews > 0 AND graduated
   AND review_count  >= gspRetireAfterReviews
   AND stability     >= gspRetireMinStability
   AND correct_streak >= gspRetireMinStreak:
    return max(1, round_half_to_even(gspRetireIntervalDays))
```

四旋钮（camelCase / Rust snake_case 同名对应；serde 默认全关 = bit-exact legacy）：
`gspRetireAfterReviews`（0=关）、`gspRetireIntervalDays`（默认 365）、
`gspRetireMinStability`（默认 0）、`gspRetireMinStreak`（默认 0）。

### 9.2 设计约束（离线 oracle 校准，2026-07-08）

- `review_count` 含**全部历史**（warm 词首评即可达标）→ 次数门槛不构成保护，
  必须搭配 `gspRetireMinStreak`。连击是「近期无失败」的直接证据：GRU oracle 校准下，
  脏历史词 streak≥4（间隔递增结构）估计半衰期稳越 30 天；短间隔密集节奏下连击的
  置信度降低，需更高门槛或避免与激进密集化（低 floor + 高 young retention）组合。
- stability 门槛**无法**分辨「S 高但真实半衰期低」的脏历史词（FSRS 信念 ≠ oracle 真值），
  只作为辅助下界。
- 退役词停止复习即停止半衰期增长：门槛过低 = 把未固化词提前冻结（mastered 崩塌）。

### 9.3 三处实现同源（漂移即 parity 报错）

- Rust：`mdm.rs::gsp_schedule_days` 步骤 4.5。
- Python 闭环 sim：`schedulers.py::AMASScheduler.next_interval_days` retire 块。
- Python parity 参考：`tests/test_mirror_parity.py::_gsp_schedule_days_ref` 步骤 4.5，
  判别族 `test_gsp_parity_g_retire`（定向 5 例 + 40 随机变体，S/D ≤1e-9 + 区间整数恒等）。
