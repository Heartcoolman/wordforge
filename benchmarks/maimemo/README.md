# MaiMemo Benchmark

基于墨墨公开数据集的离线评估与调参工具，面向 `WordForge` 的 review core。

## 能做什么

- `prepare`：下载 Harvard Dataverse 数据、解压、Join `raw + difficulty + offset` 并产出：
  - `prefix_events.parquet`
  - `sequence_groups.parquet`
- `fit_oracle`：训练一个 `GRU-HLR` oracle，并同时训练 `HLR` baseline。
- `evaluate`：对一份 `memoryModel` 配置跑三层评估，输出 `benchmark_metrics.json`。
- `tune`：用分阶段 `约束 TPE + successive halving` 搜索候选配置——每 trial 跑确定性 DHP 模拟，三条 0.9× 守门腿编码为 optuna `constraints_func`（不可行 trial 短路跳过预测评估），stage2 在不相交十分位桶上晋级，最终守门（uncapped prediction gain ≥0.5% + DHP 三腿 ≥0.9×基线）保持不变。

## 环境

建议单独创建 benchmark 虚拟环境：

```bash
cd /Users/liji/english/wordforge
uv venv .bench-venv --python 3.12
source .bench-venv/bin/activate
python -m pip install -r benchmarks/maimemo/requirements.txt
```

## 快速开始

```bash
cd /Users/liji/english/wordforge
source .bench-venv/bin/activate

python -m benchmarks.maimemo.cli prepare --root "$WORD_FORGE_BENCH_DATA/maimemo"
python -m benchmarks.maimemo.cli fit_oracle --root "$WORD_FORGE_BENCH_DATA/maimemo"
python -m benchmarks.maimemo.cli evaluate --root "$WORD_FORGE_BENCH_DATA/maimemo"
python -m benchmarks.maimemo.cli tune --root "$WORD_FORGE_BENCH_DATA/maimemo"
```

## 说明

- 主判分来自真实数据层。
- DHP 模拟只作为参考与回归护栏，不参与主目标函数。
- 当前 v1 只覆盖单词级 review memory core，不覆盖候选集排序、新词引入和疲劳/动机驱动策略。

