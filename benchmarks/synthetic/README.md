# Synthetic DHP Benchmark

用 `benchmarks.maimemo.dhp_reference.DHPStudent` 驱动的合成学习轨迹数据集。Schema 与 `benchmarks.maimemo.prefix_events` 完全对齐，可作为 maimemo / duolingo_hlr 之外的第三方"对照组" —— 因为合成参数完全已知，可以用来做模型 oracle 上限/退化测试。

## 设计

每个 `(student, word)` 维持一份 `DHPStudent` 内部状态 (`[halflife, difficulty]`)；每天分两步：

1. **初学** —— 用 `init_per_day = ceil(n_words / (n_days * 1/3))` 的预算给该学生学**新词**，调用 `DHPStudent.init(difficulty)` 拿到起始 `halflife` 与 step=1 的 `recalled` 真实值。**step=1 初学不进 `prefix_events`**，与 maimemo 行为一致。
2. **复习** —— 用 `reviews_per_day` 个 budget 复习已学词。优先选 `due_date <= today` 的词（按当前 `p_recall = 2^(-Δt/halflife)` 升序，最濒临遗忘的先）；若 due 池不足 budget，从已学词里挑 `p_recall` 最低的提前复习，**保证复习配额尽量饱和到 reviews_per_day**。

每次复习按 DHP 给的 `p_recall` Bernoulli 采样 `recalled ∈ {0, 1}`，更新 `state` 与 `halflife`，下次 `due_date = day + ceil(halflife * -log2(0.85))`。

## CLI 参数

```bash
python -m benchmarks.synthetic.generate \
  --n-students 1000 \
  --n-words 1000 \
  --n-days 90 \
  --reviews-per-day 50 \
  --seed 42 \
  --out-root /Users/liji/.wordforge-bench/synthetic \
  --retention 0.85
```

默认值已设为任务规格 (1000, 1000, 90, 50, 42)，直接 `python -m benchmarks.synthetic.generate` 即可。

## Schema 对齐

`prefix_events.parquet` 14 列与 `benchmarks/maimemo` 完全一致：

| 列 | 类型 | 合成时的语义 |
| --- | --- | --- |
| `u` | string | `s{i:07d}` |
| `w` | string | `w{i:06d}` |
| `step` | int64 | (s, w) 第 N 次事件（>= 2，因为 step=1 不进 prefix_events） |
| `offset` | int64 | `day - student_start_day`，所有学生 start=0 故 offset = 当前 day |
| `user_bucket_10` | uint64 | `md5(u).hex[:16]` 转 int → `% 10`（稳定哈希，每次重跑一致） |
| `user_bucket_100` | uint64 | 同上 `% 100` |
| `difficulty` | double | `clip(round(uniform(0,1) * 9 + 1), 1, 10)`，每词固定 |
| `r_history` | string | `"0,"` + 前 step-2 个 next_r 拼接（首项 '0' 占位 + 跳过真实 step=1 outcome） |
| `t_history` | string | `"0,"` + 前 step-2 个 next_t 拼接 |
| `next_r` | int32 | `1 if rng.random() < p_recall else 0` |
| `next_t` | int64 | `max(1, day - last_review_day)` |
| `dhp_p_recall` | double | `2^(-Δt/halflife)`（合成时 ground-truth 概率，maimemo 是聚合统计估计） |
| `group_count` | double | `null`（合成无聚合统计意义） |
| `split` | string | `train/val/test = 8:1:1` by `md5(u) % 10` |

`sequence_groups.parquet` 同 maimemo `LIST_AGG ORDER BY step` 形态（u/w/split/difficulty/user_bucket_10/100 + 6 个 list 列）。

## 与真实数据集的差异

| 维度 | maimemo | duolingo_hlr | synthetic |
| --- | --- | --- | --- |
| ground-truth `p_recall` | 聚合统计估计（join 来自 forgetting_curve.tsv） | 无（HLR 数据没有 DHP 标定） | **完全已知**（DHP `2^(-Δt/halflife)`） |
| `dhp_p_recall` 列非空率 | ~85% | 0% | 100% |
| `group_count` 列 | 聚合 cohort 大小 | null | null（合成无 cohort） |
| 词难度分布 | DHP 标定 1-10 | log1p(history_seen) 派生 | Uniform(0,1) → 离散 1-10 |
| 调度策略 | MaiMemo 真实生产 | Duolingo 真实生产 | DHP `interval = halflife × -log2(retention)` |
| 用户起点 | 真实 | 真实（用户首次 ts） | 所有 student start_day=0 |
| 学习/复习混合 | 真实 | 真实 | 前 1/3 周期偏初学，后期纯复习 + 提前复习兜底 |

主要用途：
- **Oracle 上限测试** —— 因为 DHP 是真实的 p_recall 生成过程，模型若能精准学到 DHP 参数则 metrics 接近理论上限。
- **退化路径排查** —— 当真实数据训练效果不佳时，先在合成数据上跑一遍，确认模型在已知 DGP 下是否正常学习。

## 验收

- `prefixRows = 4,484,000`（≈ 4.5M ±10% [4,050,000, 4,950,000] ✓）
- `sequenceRows = 916,444`（每 student 平均学到约 916 个词 / 1000 词 deck）
- `criticalNanCount = 0`（u/w/step/offset/next_r/next_t/r_history/t_history/split）
- `seed=42` 重跑确定性：
  - `fingerprint(seed=42) = 85152c19fe773c9b`（前 1000 行指纹）
  - 全 parquet 内容 `sha256[:32] = 15664f1389c11f9dfc30631dfad2dbc0`（三次重跑一致 ✓）

## 复现命令

```bash
cd /Users/liji/english/wordforge
source .bench-venv/bin/activate
python -m benchmarks.synthetic.generate --seed 42
```

约 3-4 分钟（M-series 8 thread, pure-python loop；4.5M 行写 zstd parquet 约 60s）。

## 输出位置

- `/Users/liji/.wordforge-bench/synthetic/cache/maimemo_reference/parameters.csv`（DHP 参考参数，由 `ensure_reference_assets()` 拉取）
- `/Users/liji/.wordforge-bench/synthetic/cache/maimemo_reference/policy/ivl-*.csv`
- `/Users/liji/.wordforge-bench/synthetic/parquet/prefix_events.parquet`
- `/Users/liji/.wordforge-bench/synthetic/parquet/sequence_groups.parquet`
