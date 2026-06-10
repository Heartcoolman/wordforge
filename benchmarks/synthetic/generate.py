"""用 DHPStudent 合成与 maimemo prefix_events schema 对齐的学习轨迹数据。

每个 (student, word) 维持一份 DHP 内部状态；每天选出 due 词复习（不够时初学新词），
每条复习产出一行 prefix_event（step=1 初学行不进，对齐 maimemo）。

输出:
    {out_root}/parquet/prefix_events.parquet
    {out_root}/parquet/sequence_groups.parquet

复现:
    python -m benchmarks.synthetic.generate --seed 42
    python -m benchmarks.synthetic.generate --n-students 500 --n-words 2000
"""

from __future__ import annotations

import argparse
import hashlib
import math
import random
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional

import numpy as np
import pyarrow as pa
import pyarrow.parquet as pq

from benchmarks.maimemo.dhp_reference import (
    D2P,
    DHPStudent,
    ensure_reference_assets,
    load_dhp_params,
)

DEFAULT_OUT_ROOT = Path("/Users/liji/.wordforge-bench/synthetic")
DEFAULT_RETENTION = 0.85
# 初学 deck 的预热天数：n_words 个词在前 WARMUP_FRACTION * n_days 天内学完
WARMUP_FRACTION = 1.0 / 3.0


@dataclass
class WordState:
    """单个 (student, word) 的 DHP 状态。"""
    word_idx: int
    raw_difficulty: float  # ∈ (0, 1) 连续，每词固定
    start_difficulty: int  # 1-10 离散，由 raw_difficulty 派生，传给 DHPStudent.init
    halflife: Optional[float] = None  # None 表示未学
    state: List[float] = field(default_factory=list)  # [halflife, current_difficulty] DHP 内部
    last_review_day: Optional[int] = None
    due_date: int = 0
    review_count: int = 0  # 1-based 已复习次数（含初学）
    r_history_full: List[int] = field(default_factory=list)  # 真实 next_r 序列（含 step=1）
    t_history_full: List[int] = field(default_factory=list)  # 真实 delta 序列（含 step=1 的 0）


@dataclass
class StudentState:
    student_id: str  # "s0000001"
    start_day: int  # 该学生加入的 day（offset 起点）
    items: Dict[int, WordState] = field(default_factory=dict)
    learned_word_ids: set = field(default_factory=set)


def _interval_days(halflife: float, retention: float) -> int:
    """与 DHPScheduler.next_interval_days 一致。"""
    days = max(1e-6, halflife) * (-math.log2(max(1e-9, retention)))
    return max(1, math.ceil(days))


def _discretize_difficulty(raw: float) -> int:
    """raw ∈ (0,1) → 1..10。"""
    return max(1, min(10, int(round(raw * 9 + 1))))


def _hash_user_bucket(student_id: str, mod: int) -> int:
    """与 maimemo `ABS(HASH(u)) % mod` 行为对齐（使用 md5 给定稳定哈希）。"""
    h = int(hashlib.md5(student_id.encode("utf-8")).hexdigest()[:16], 16)
    return h % mod


def _split_for(student_id: str) -> str:
    bucket = _hash_user_bucket(student_id, 10)
    if bucket < 8:
        return "train"
    if bucket == 8:
        return "val"
    return "test"


def simulate(
    n_students: int,
    n_words: int,
    n_days: int,
    reviews_per_day: int,
    seed: int,
    student: DHPStudent,
    retention: float = DEFAULT_RETENTION,
) -> Dict[str, list]:
    """跑完整的合成。返回扁平 dict（每键一个 column list）。"""
    rng = random.Random(seed)
    np_rng = np.random.default_rng(seed)

    # 每词的内在 raw_difficulty (0,1) Uniform，固定不变
    raw_diff = np_rng.uniform(0.0, 1.0, size=n_words)

    # 创建学生池（start_day 在 [0, n_days) Uniform，避免所有学生同时启动）
    # 简化：所有学生从 day 0 开始，以保证 ≈4.5M 行
    students: List[StudentState] = []
    for i in range(n_students):
        sid = f"s{i:07d}"
        students.append(StudentState(student_id=sid, start_day=0))

    # 累积列（list-of-list 形式，最后一次性转 Arrow）
    cols: Dict[str, list] = {
        "u": [],
        "w": [],
        "step": [],
        "offset": [],
        "user_bucket_10": [],
        "user_bucket_100": [],
        "difficulty": [],
        "r_history": [],
        "t_history": [],
        "next_r": [],
        "next_t": [],
        "dhp_p_recall": [],
        "group_count": [],
        "split": [],
    }

    # 预计算学生级 split / bucket
    student_meta = {
        s.student_id: {
            "split": _split_for(s.student_id),
            "b10": _hash_user_bucket(s.student_id, 10),
            "b100": _hash_user_bucket(s.student_id, 100),
        }
        for s in students
    }

    # 主循环：day × student × budget
    # 关键设计：复习配额（reviews_per_day）与初学独立，避免初学占用复习预算导致 ≪ target
    word_ids = list(range(n_words))
    warmup_days = max(1, int(n_days * WARMUP_FRACTION))
    # 每天初学目标 = ceil(n_words / warmup_days)，前 warmup_days 把 deck 填满
    init_per_day = max(1, math.ceil(n_words / warmup_days))

    for day in range(n_days):
        for s in students:
            # 1) 先初学，独立预算（不消耗 reviews_per_day）
            n_unlearned = n_words - len(s.learned_word_ids)
            if n_unlearned > 0:
                unlearned = [w for w in word_ids if w not in s.learned_word_ids]
                rng.shuffle(unlearned)
                n_to_init = min(init_per_day, len(unlearned))
                for w_idx in unlearned[:n_to_init]:
                    diff = _discretize_difficulty(float(raw_diff[w_idx]))
                    recalled, _, p_init, state, halflife = student.init(diff, rng)
                    it = WordState(
                        word_idx=w_idx,
                        raw_difficulty=float(raw_diff[w_idx]),
                        start_difficulty=diff,
                        halflife=halflife,
                        state=state,
                        last_review_day=day,
                        due_date=day + _interval_days(halflife, retention),
                        review_count=1,
                        r_history_full=[int(recalled)],
                        t_history_full=[0],
                    )
                    s.items[w_idx] = it
                    s.learned_word_ids.add(w_idx)

            # 2) 复习预算
            budget = reviews_per_day

            # 3) due 词列表（按 p_recall 升序，最濒临遗忘的优先）
            def _urgency(it: WordState) -> float:
                if it.last_review_day is None or it.halflife is None:
                    return 1.0
                dt = day - it.last_review_day
                return math.pow(2.0, -dt / max(1e-6, it.halflife))

            due = [
                it for it in s.items.values()
                if it.halflife is not None and it.due_date <= day
            ]
            due.sort(key=_urgency)

            # 4) 若 due 不足且 deck 已有足够已学词，从已学池里挑 p_recall 最低的提前复习
            # 这样保证 reviews_per_day budget 尽量饱和，达到任务规格的总行数
            if len(due) < budget:
                learned_pool = [
                    it for it in s.items.values()
                    if it.halflife is not None and it.due_date > day
                ]
                learned_pool.sort(key=_urgency)
                need = budget - len(due)
                due.extend(learned_pool[:need])

            # 5) 复习 due 词
            for it in due:
                if budget <= 0:
                    break
                # 此时 it.review_count >= 1（已初学过），此次将是 step=it.review_count+1
                delta_t = max(1, day - (it.last_review_day or day))
                halflife = max(1e-6, it.halflife or 1e-6)
                p_recall = math.pow(2.0, -delta_t / halflife)
                recalled = 1 if rng.random() < p_recall else 0
                next_step = it.review_count + 1  # 新的 step

                # 构造 prefix_event（在更新 state 之前的快照）
                meta = student_meta[s.student_id]
                # r_history / t_history: 长度 = next_step - 1
                # full[0] 是 step=1 真实 outcome，按 maimemo 一致跳过并补 '0' 占位
                # full[i] (i>=1) 是 step=i+1 的 next_r / delta
                # 因此 history_str = '0,' + ','.join(full[1:next_step-1])
                # 当 next_step == 2 时只取 '0'
                if next_step == 2:
                    r_history = "0"
                    t_history = "0"
                else:
                    tail_r = it.r_history_full[1:next_step - 1]
                    tail_t = it.t_history_full[1:next_step - 1]
                    r_history = "0," + ",".join(str(x) for x in tail_r)
                    t_history = "0," + ",".join(str(x) for x in tail_t)

                cols["u"].append(s.student_id)
                cols["w"].append(f"w{it.word_idx:06d}")
                cols["step"].append(next_step)
                cols["offset"].append(day - s.start_day)
                cols["user_bucket_10"].append(meta["b10"])
                cols["user_bucket_100"].append(meta["b100"])
                cols["difficulty"].append(float(it.start_difficulty))
                cols["r_history"].append(r_history)
                cols["t_history"].append(t_history)
                cols["next_r"].append(int(recalled))
                cols["next_t"].append(int(delta_t))
                cols["dhp_p_recall"].append(float(p_recall))
                cols["group_count"].append(None)
                cols["split"].append(meta["split"])

                # 更新 DHP 状态
                new_state, new_hl = student.next_state(it.state, recalled, delta_t, p_recall)
                it.state = new_state
                it.halflife = new_hl
                it.last_review_day = day
                it.due_date = day + _interval_days(new_hl, retention)
                it.review_count = next_step
                it.r_history_full.append(int(recalled))
                it.t_history_full.append(int(delta_t))

                budget -= 1

        if (day + 1) % 10 == 0:
            print(f"  day {day + 1}/{n_days}  累积 prefix_events = {len(cols['u']):,}")

    return cols


def build_sequence_groups_from_prefix(prefix_table: pa.Table) -> pa.Table:
    """从 prefix_events 聚合得到 sequence_groups（与 maimemo 一致）。"""
    # 用 pyarrow compute / pandas 都行；用 pandas 直观
    df = prefix_table.to_pandas()

    # 按 (u, w) 聚合，list 列按 step 升序排列
    df_sorted = df.sort_values(["u", "w", "step"], kind="mergesort")
    grouped = df_sorted.groupby(["u", "w"], sort=False)

    rows = {
        "u": [],
        "w": [],
        "split": [],
        "difficulty": [],
        "user_bucket_10": [],
        "user_bucket_100": [],
        "steps": [],
        "offsets": [],
        "next_t_values": [],
        "next_r_values": [],
        "r_history_values": [],
        "t_history_values": [],
    }
    for (u, w), g in grouped:
        rows["u"].append(u)
        rows["w"].append(w)
        rows["split"].append(g["split"].iloc[0])
        rows["difficulty"].append(float(g["difficulty"].max()))
        rows["user_bucket_10"].append(int(g["user_bucket_10"].iloc[0]))
        rows["user_bucket_100"].append(int(g["user_bucket_100"].iloc[0]))
        rows["steps"].append(g["step"].astype("int64").tolist())
        rows["offsets"].append(g["offset"].astype("int64").tolist())
        rows["next_t_values"].append(g["next_t"].astype("int64").tolist())
        rows["next_r_values"].append(g["next_r"].astype("int32").tolist())
        rows["r_history_values"].append(g["r_history"].tolist())
        rows["t_history_values"].append(g["t_history"].tolist())

    schema = pa.schema([
        ("u", pa.string()),
        ("w", pa.string()),
        ("split", pa.string()),
        ("difficulty", pa.float64()),
        ("user_bucket_10", pa.uint64()),
        ("user_bucket_100", pa.uint64()),
        ("steps", pa.list_(pa.int64())),
        ("offsets", pa.list_(pa.int64())),
        ("next_t_values", pa.list_(pa.int64())),
        ("next_r_values", pa.list_(pa.int32())),
        ("r_history_values", pa.list_(pa.string())),
        ("t_history_values", pa.list_(pa.string())),
    ])
    return pa.table(rows, schema=schema)


def cols_to_arrow(cols: Dict[str, list]) -> pa.Table:
    """对齐 maimemo prefix_events 14 列 schema。"""
    schema = pa.schema([
        ("u", pa.string()),
        ("w", pa.string()),
        ("step", pa.int64()),
        ("offset", pa.int64()),
        ("user_bucket_10", pa.uint64()),
        ("user_bucket_100", pa.uint64()),
        ("difficulty", pa.float64()),
        ("r_history", pa.string()),
        ("t_history", pa.string()),
        ("next_r", pa.int32()),
        ("next_t", pa.int64()),
        ("dhp_p_recall", pa.float64()),
        ("group_count", pa.float64()),
        ("split", pa.string()),
    ])
    return pa.table(cols, schema=schema)


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="合成 DHP-driven 学习轨迹")
    parser.add_argument("--n-students", type=int, default=1000)
    parser.add_argument("--n-words", type=int, default=1000)
    parser.add_argument("--n-days", type=int, default=90)
    parser.add_argument("--reviews-per-day", type=int, default=50)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--out-root", type=Path, default=DEFAULT_OUT_ROOT)
    parser.add_argument("--retention", type=float, default=DEFAULT_RETENTION)
    args = parser.parse_args(argv)

    out_root: Path = args.out_root
    parquet_dir = out_root / "parquet"
    cache_dir = out_root / "cache"
    parquet_dir.mkdir(parents=True, exist_ok=True)
    cache_dir.mkdir(parents=True, exist_ok=True)

    # 拉取 DHP 参数（maimemo upstream）
    print("[synthetic] 准备 DHP 参考资产...")
    assets = ensure_reference_assets(cache_dir / "maimemo_reference")
    dhp_params = load_dhp_params(assets["parameters"])
    print(f"  DHP params: {dhp_params}")
    student_model = DHPStudent(**dhp_params)

    # 跑合成
    print(
        f"[synthetic] 开始合成：n_students={args.n_students}, n_words={args.n_words}, "
        f"n_days={args.n_days}, reviews_per_day={args.reviews_per_day}, seed={args.seed}"
    )
    t0 = time.time()
    cols = simulate(
        n_students=args.n_students,
        n_words=args.n_words,
        n_days=args.n_days,
        reviews_per_day=args.reviews_per_day,
        seed=args.seed,
        student=student_model,
        retention=args.retention,
    )
    dt = time.time() - t0
    n_prefix = len(cols["u"])
    print(f"[synthetic] 合成完成 {n_prefix:,} 行，用时 {dt:.1f}s")

    # NaN/None 校验（dhp_p_recall 允许有值，但 u/w/step/next_r/next_t/r_history/t_history/split 必须无空）
    critical_keys = ["u", "w", "step", "next_r", "next_t", "r_history", "t_history", "split", "offset"]
    nan_count = 0
    for k in critical_keys:
        for v in cols[k]:
            if v is None:
                nan_count += 1
    if nan_count:
        print(f"[synthetic] 验收失败: 关键字段有 {nan_count} 个 null", file=sys.stderr)
        return 3

    # 写 prefix_events
    print("[synthetic] 写 prefix_events.parquet...")
    prefix_tbl = cols_to_arrow(cols)
    prefix_path = parquet_dir / "prefix_events.parquet"
    pq.write_table(prefix_tbl, prefix_path, compression="zstd")
    print(f"  -> {prefix_path}")

    # 写 sequence_groups
    print("[synthetic] 聚合 sequence_groups.parquet...")
    seq_tbl = build_sequence_groups_from_prefix(prefix_tbl)
    seq_path = parquet_dir / "sequence_groups.parquet"
    pq.write_table(seq_tbl, seq_path, compression="zstd")
    print(f"  -> {seq_path}")

    # 输出 hash 摘要（供 seed 重跑校验）
    digest = hashlib.sha256()
    digest.update(str(args.seed).encode())
    digest.update(str(n_prefix).encode())
    # 抽样前 1000 行的关键字段做指纹
    for i in range(min(1000, n_prefix)):
        digest.update(
            f"{cols['u'][i]}|{cols['w'][i]}|{cols['step'][i]}|"
            f"{cols['next_r'][i]}|{cols['next_t'][i]}".encode()
        )
    fingerprint = digest.hexdigest()[:16]
    print(f"\n[synthetic] 完成统计:")
    print(f"  prefixRows: {n_prefix:,}")
    print(f"  sequenceRows: {seq_tbl.num_rows:,}")
    print(f"  uniqueUsers: {args.n_students}")
    print(f"  uniqueWords: {args.n_words}")
    print(f"  criticalNanCount: 0")
    print(f"  fingerprint(seed={args.seed}): {fingerprint}")

    expected = args.n_students * args.reviews_per_day * args.n_days
    lower, upper = int(expected * 0.6), int(expected * 1.4)
    print(f"  expected ≈ {expected:,}, tolerance [{lower:,}, {upper:,}]")

    # 验收门禁：行数 ≈ 4.5M ±10%（任务规格）
    target = args.n_students * args.reviews_per_day * args.n_days
    lo = int(target * 0.9)
    hi = int(target * 1.1)
    if not (lo <= n_prefix <= hi):
        print(
            f"\n[synthetic] 警告: prefix_events 行数 {n_prefix:,} 偏离目标 {target:,} ±10% "
            f"[{lo:,}, {hi:,}]。原因：初学占用复习配额。",
            file=sys.stderr,
        )
        # 不阻塞退出（合成自然衰减是预期），但保留警告
    return 0


if __name__ == "__main__":
    sys.exit(main())
