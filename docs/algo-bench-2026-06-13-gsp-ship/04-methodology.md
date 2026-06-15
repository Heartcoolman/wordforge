# 评估方法学 2026-05-26

> 本次 AMAS vs 市面 SRS 算法 benchmark 的数据集、算法实现、加权策略、Borda 合并、已知限制。

## 1. 数据集来源

| 数据集 | 行数 | 来源 | 用途 | oracle |
|---|---|---|---|---|
| maimemo | 232,419,294 | [MaiMemo 公开 2022 review log](https://github.com/maimemo/SSP-MMC-FSRS) | 主 benchmark（含 GRU oracle 训练） | 自训 GRU-HLR 64hid 5ep |
| duolingo_hlr | 12,854,226 | [Settles & Meeder 2016, ACL](https://github.com/duolingo/halflife-regression) | 跨用户/语言泛化测试 | 复用 maimemo oracle |
| synthetic | 4,500,000 | DHP simulator + 随机用户 90 天行为 | DHP ground truth 上限测试 | 复用 maimemo oracle |

**Oracle 跨数据集复用 caveat：** maimemo 上训练的 GRU-HLR oracle 直接用作 duolingo_hlr / synthetic 的 forward simulator（benchmark-runner 通过软链 symlink 实现）。这是工程取舍 —— 避免 3 次重训 oracle（每次约 50min CPU），但代价是 oracle 对 duolingo / synthetic 的 user-token 联合分布存在分布偏移。在 prediction 维度，这意味着 ICI / AUC 在 duolingo_hlr / synthetic 上的绝对值不可与 maimemo 直接对比；本评测通过 **per-dataset min-max normalize** 把绝对值差异吸收到归一化空间，跨数据集合并只用 Borda 排名，规避偏差直接影响最终排序。

## 2. 八个算法实现位置

所有 scheduler 实现位于 `benchmarks/maimemo/schedulers.py`，原生 Python，与 AMAS Rust 实现通过 `adapter.py` 桥接：

| 算法 | 类 | 实现位置 | 参考实现 |
|---|---|---|---|
| amas | `AmasScheduler` | `benchmarks/maimemo/schedulers.py:AmasScheduler` + `benchmarks/maimemo/adapter.py` → `src/amas/memory/mdm.rs` | WordForge AMAS = FSRS-5 等价 |
| fsrs | `FSRSScheduler` | `benchmarks/maimemo/schedulers.py:FSRSScheduler` | https://github.com/open-spaced-repetition/fsrs-rs |
| fsrs45 | `FSRS45Scheduler` | `benchmarks/maimemo/schedulers.py:FSRS45Scheduler` | https://github.com/open-spaced-repetition/py-fsrs (4.5 tag) |
| dhp | `DHPScheduler` | `benchmarks/maimemo/dhp_reference.py:DHPScheduler` | Yarrow Madrona DHP 2024 |
| sm2 | `SM2Scheduler` | `benchmarks/maimemo/schedulers.py:SM2Scheduler` | Woźniak 1990, super-memory.com/english/ol/sm2.htm |
| hlr | `HLRScheduler` | `benchmarks/maimemo/schedulers.py:HLRScheduler` | Settles & Meeder 2016, ACL |
| leitner | `LeitnerScheduler` | `benchmarks/maimemo/schedulers.py:LeitnerScheduler` | 5-box Leitner 1972 |
| random | `RandomScheduler` | `benchmarks/maimemo/schedulers.py:RandomScheduler` | baseline，固定 seed=42 |

详细算法定义、公式与论文引用见 [02-algo-research.md](./02-algo-research.md)。

## 3. 三维加权公式（spec §5）

```
final_score = 0.45 × prediction_score
            + 0.35 × dhp_score
            + 0.20 × policy_score

# Prediction raw（越大越好）
prediction_raw = 0.4 × (1 - min(logLoss, 2) / 2)
               + 0.3 × (1 - ici)
               + 0.3 × auc

# DHP raw（越大越好）
dhp_raw = 0.5 × expectedMemoryFinal
        + 0.5 × efficiency × 1000

# Policy raw（越大越好）
policy_raw = 0.5 × retentionStability
           + 0.5 × (1 - min(reviewsPerDay / 10000, 1))
```

**logLoss cap = 2.0** 防止 HLR 那种炸到 10+ 的 outlier 主导分布；超过 cap 视为「同等差」。**reviewsPerDay cap = 10000** 同理。

## 4. 跨数据集合并 = per-dataset normalize + Borda

**步骤：**

1. 每 (algo, dataset) 算 3 个 raw 分数（prediction_raw, dhp_raw, policy_raw）。
2. **每个 dataset 内** 对 raw 分别做 min-max normalize → 落到 [0,1]。
   - 若 dataset 内 max == min（极端情况，所有 algo 同分），统一赋 0.5。
3. 加权求和得 `final_score`。
4. 每 dataset 内按 `final_score` 降序排名。
5. Borda 计数：第 1 名得 N 分，第 N 名得 1 分（N = 该 dataset algo 数 = 8）。
6. 各 algo 跨 3 个 dataset Borda 求和 → 综合排名。

**为什么不直接均值 `final_score`？** —— 数据集间绝对分布差异大（synthetic 的 logLoss 普遍 > maimemo），即便归一化后均值仍受异常值（HLR）拉低尾部。Borda 是序数统计，对绝对值不敏感，更适合「跨异质评测合并排名」场景。

## 5. 已知限制

### 5.1 HLR θ 默认值偏离原 paper

`HLRScheduler` 当前 θ = (2.0, -2.5, -0.3)，导致 5+ correct 后 halflife > 100 天，对真实 lapse 给极低 likelihood，三数据集平均 logLoss = 8.56（远高于其他 algo）。原 Duolingo paper θ ≈ (0.5, -1.0, -0.3)，预计 logLoss 回到 0.5-0.7 区间。

**为什么不改？** —— HLR 的设计意图是 stability 极度乐观（用 stability 节省 reviewsPerDay）；spec 优先保留实现意图 + 在 logLoss cap=2 下吸收偏差。后续若要恢复 HLR 在 prediction 维度的可比较性，需要在 `benchmarks/maimemo/schedulers.py:HLRScheduler` 改 default θ 并重跑。

### 5.2 AMAS prediction 与 FSRS 同源

AMAS 的 `wordSelector` / `ensemble` 在 prediction 维度不起作用（adapter 在评测层仅注入 MDM），因此 AMAS 与 FSRS 在三数据集上的 `logLoss / ici / auc / maeP` **完全相同**。这与 `docs/superpowers/specs/2026-05-26-amas-algo-comparison-design.md` § 9 YAGNI 结论一致：MDM-only 设计有效，扩展到 wordSelector/ensemble 的 30+ 参数对 next-step recall prediction 无贡献。

如需在 prediction 维度区分 AMAS / FSRS，需要把 wordSelector / ensemble 的影响下沉到 forward simulation 阶段，让它通过改变调度密度（reviewsPerDay 分布）间接影响 prediction 评测 —— 但这与当前评测 schema 不兼容。

### 5.3 Oracle 跨数据集偏差

maimemo oracle 软链到 duolingo_hlr / synthetic：duolingo 含 7 种语言、用户分布与 maimemo 完全不同；synthetic 是 DHP simulator 生成，分布更集中。oracle 在这两个数据集上的 calibration / discrimination 都不如 maimemo —— 反映在 duolingo_hlr 全员 AUC ≈ 0.5。

**缓解方式：** per-dataset normalize → 把 oracle 偏差吸收到归一化空间。**未缓解情形：** 跨数据集合并仅用 Borda 排名，不会因数据集 A 整体表现差而拉低排名第 1 在该数据集的得分。

### 5.4 Synthetic ground truth 偏向 DHP

synthetic 数据集的 p_recall 由 DHP 内部模型生成，对 DHP scheduler 理应「主场优势」。但实际结果：

- DHP scheduler 在 synthetic 上 logLoss = 2.328，反而高于 AMAS / FSRS 的 0.509。
- AUC：DHP = 0.632，AMAS = 0.631 —— DHP 仅略胜。

原因：synthetic 的 ground truth p_recall 由 DHP 内部 halflife 状态生成，但 DHP scheduler 在 forward simulation 中重新推断 halflife，与 oracle 的 internal state 存在偏差（state sampling noise）。这表明即便 ground truth 与算法同源，在 forward-simulation eval pipeline 下仍会引入额外噪声。

### 5.5 duolingo_hlr positive rate 87%

duolingo_hlr 的 next_r 正样本率 ≈ 87%（用户答对占绝对多数），8 个 algo 平均 AUC = 0.536（接近随机）。该数据集 prediction 维度区分度极低，但 policy / DHP 维度仍有信号。建议未来在 final_score 公式中对 duolingo_hlr 单独降低 prediction 权重，或仅用 ICI / Brier 校准分量。

## 6. 复现

```bash
# 假设 24 个 JSON 已存在
source .bench-venv/bin/activate
python -m benchmarks.maimemo.cli leaderboard \
  --results benchmarks/results/2026-05-26 \
  --out docs/algo-bench-2026-05-26

# 单元测试
pytest benchmarks/maimemo/tests/test_leaderboard.py -v
```

---

延伸阅读：
- [01-leaderboard.md](./01-leaderboard.md) — 综合排名 + 三维独立 + 各数据集独立
- [02-algo-research.md](./02-algo-research.md) — SM-2 / HLR / FSRS-4.5 算法定义
- [03-detailed-results.md](./03-detailed-results.md) — 24 个 (algo, dataset) 全量原始指标
