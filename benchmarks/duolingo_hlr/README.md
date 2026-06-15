# Duolingo HLR Benchmark

把 Settles & Meeder (2016) ACL 论文公开的 `learning_traces.13m` Duolingo 学习轨迹数据集，对齐到 wordforge `benchmarks.maimemo` 的 `prefix_events` schema，作为 maimemo 之外的第二个真实数据源用于离线评估。

## 来源

| 项 | 值 |
| --- | --- |
| 数据集 | Duolingo HLR learning traces (13M rows) |
| 论文 | B. Settles, B. Meeder. *A Trainable Spaced Repetition Model for Language Learning.* ACL 2016 |
| 权威源 | Harvard Dataverse DOI [10.7910/DVN/N8XJME](https://doi.org/10.7910/DVN/N8XJME) |
| License | CC BY-NC 4.0 |
| 文件 | `settles.acl16.learning_traces.13m.csv.gz`（361.4 MB gz，1.25 GB CSV） |

`prepare.py` 按以下顺序尝试下载源：

1. **Harvard Dataverse** `api/access/datafile/:persistentId?persistentId=doi:10.7910/DVN/N8XJME/UEPJVH`（实测可用）
2. `github.com/duolingo/half-life-regression/raw/master/...`（已 404，保留作历史 fallback）
3. `raw.githubusercontent.com/duolingo/half-life-regression/...`（已 404）
4. `s3.amazonaws.com/duolingo-papers/publications/...`（已 deprecated 403）

全部失败时 `prepare.py` 退出码 1 并打印手动 fallback 提示：

```text
手动 fallback: 把 learning_traces.13m.csv.gz 放到
/Users/liji/.wordforge-bench/duolingo_hlr/raw/learning_traces.13m.csv.gz
```

## 与 maimemo schema 的对齐

`prefix_events.parquet` 14 列与 `benchmarks/maimemo` 完全一致：

| 列 | 类型 | 来源映射 |
| --- | --- | --- |
| `u` | string | `user_id` 原值保留 |
| `w` | string | `lexeme_id` 原值保留（比 `lexeme_string` 更稳定，不含 POS/修饰符歧义） |
| `step` | int64 | 每 `(u, w)` 按 `timestamp` 升序的 1-based 序号 |
| `offset` | int64 | `round((ts - min(ts | u)) / 86400)`，相对该用户首次出现的天数 |
| `user_bucket_10` | uint64 | `abs(hash(u)) % 10` |
| `user_bucket_100` | uint64 | `abs(hash(u)) % 100` |
| `difficulty` | double | `clip(round(log1p(history_seen)/log1p(1000) * 9 + 1), 1, 10)`，离散 1-10（与 maimemo 范围一致；HLR 数据本身无 DHP 标定） |
| `r_history` | string | `"0,"` + 前 `step-2` 个 `next_r` 拼接（首位 `0` 占位，跳过真实 step=1 outcome，与 maimemo 行为一致） |
| `t_history` | string | `"0,"` + 前 `step-2` 个 `next_t` 拼接，同上 |
| `next_r` | int32 | `1 if p_recall >= 0.5 else 0` |
| `next_t` | int64 | `max(1, round(delta / 86400))`，秒转天 |
| `dhp_p_recall` | double | `null`（HLR 无 DHP 标定） |
| `group_count` | double | `null` |
| `split` | string | `train/val/test` = 8:1:1 by `abs(hash(u)) % 10` |

`sequence_groups.parquet` 同 maimemo `LIST_AGG ORDER BY step` 形态。

**与 maimemo 关键差异**：
1. 单位密度更稀 —— Duolingo session 是"同日 N 次复习"，HLR 的 `delta` 最小 0；我们 `next_t = max(1, round(delta/86400))` 把同日复习强制压成 1 天。
2. `dhp_p_recall` 与 `group_count` 全为 null —— HLR 数据没有 DHP 遗忘曲线统计，下游消费方需对此容错（maimemo 自身也有 ~14.6% 的 dhp_p_recall null）。
3. `step=1` 不进 prefix_events，与 maimemo 对齐（首次学习不参与预测）。

## 统计

| 指标 | 值 |
| --- | --- |
| 原始 CSV 行数 | 12,854,226 |
| `prefix_events` 行数 | 6,992,597 |
| `sequence_groups` 行数 | 2,570,437 |
| 独立用户数 | 64,997 |
| 独立 lexeme 数 | 15,299 |
| 关键字段 NaN | 0 |
| `train/val/test` 比例 | 8 : 1 : 1（by user hash） |

## 复现命令

```bash
cd /Users/liji/english/wordforge
source .bench-venv/bin/activate
python -m benchmarks.duolingo_hlr.prepare
```

输出：

- `/Users/liji/.wordforge-bench/duolingo_hlr/raw/learning_traces.13m.csv.gz`（原始 gz，被复用）
- `/Users/liji/.wordforge-bench/duolingo_hlr/raw/learning_traces.13m.csv`（解压后）
- `/Users/liji/.wordforge-bench/duolingo_hlr/cache/prepare.duckdb`（duckdb 工作区，可删）
- `/Users/liji/.wordforge-bench/duolingo_hlr/parquet/prefix_events.parquet`
- `/Users/liji/.wordforge-bench/duolingo_hlr/parquet/sequence_groups.parquet`

整段大约 1-2 分钟（下载 30s，duckdb 转换 60-90s，机器：M-series 8 thread + 12GB memory_limit）。

## Adapter

`adapter.py` 直接 `from benchmarks.maimemo.adapter import *`。Duolingo HLR 复用同一个 Rust MDM adapter 二进制（`target/release/maimemo_mdm_adapter`），因为 wordforge memory 接口是数据无关的。

## 引用

```bibtex
@inproceedings{settles2016hlr,
  title={A Trainable Spaced Repetition Model for Language Learning},
  author={Settles, Burr and Meeder, Brendan},
  booktitle={Proceedings of ACL},
  pages={1848--1858},
  year={2016}
}
```
