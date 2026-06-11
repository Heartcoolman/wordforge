"""evaluate_scheduler.py — Scheduler 维度的 prediction 评估。

对每个 scheduler 跑 prediction 评估，返回 {logLoss, ici, auc, maeP}。

步骤:
1. 从 prefix_events.parquet test split 采样 n_users
2. 对每条 (user, word) 序列, 用 scheduler.warm_start(t_history, r_history, difficulty)
3. 预测在 elapsed=next_t 天后的 p_recall:
     - amas/fsrs/fsrs45: 用 mirror state recall(elapsed) 或 _recall(elapsed)
     - dhp/sm2/leitner/random/hlr: 用 2^(-elapsed/current_halflife())
     - hlr 也走 fallback (内部 halflife 已经是 2^(theta·feat))
4. ground truth = next_r
5. 累计 logLoss / ICI / AUC / MAE
"""
from __future__ import annotations

import math
import random
import time
from pathlib import Path
from typing import Dict, List

import duckdb
import numpy as np
import pandas as pd

from .config import BenchPaths
from .dhp_reference import DHPStudent, ensure_reference_assets, load_dhp_params
from .models import parse_history
from .pipeline import StreamingPredictionAccumulator
from .schedulers import (
    AMAS6Scheduler,
    AMASScheduler,
    DHPScheduler,
    FSRS6Scheduler,
    FSRS45Scheduler,
    FSRSScheduler,
    HLRScheduler,
    LeitnerScheduler,
    RandomScheduler,
    SM2Scheduler,
)


def _build_scheduler(name: str, dhp_student: DHPStudent | None, seed: int):
    """工厂: 支持全部 8 个 scheduler (build_scheduler 只支持前 5 个)。"""
    if name == "fsrs":
        return FSRSScheduler(desired_retention=0.85)
    if name == "amas":
        return AMASScheduler(desired_retention=0.85)
    if name == "dhp":
        if dhp_student is None:
            raise ValueError("dhp_student is required for DHPScheduler")
        return DHPScheduler(student=dhp_student, desired_retention=0.85)
    if name == "leitner":
        return LeitnerScheduler(desired_retention=0.85)
    if name == "random":
        return RandomScheduler(seed=seed)
    if name == "sm2":
        return SM2Scheduler()
    if name == "hlr":
        return HLRScheduler()
    if name == "fsrs45":
        return FSRS45Scheduler(desired_retention=0.9)
    if name == "fsrs6":
        return FSRS6Scheduler(desired_retention=0.9)
    if name == "amas6":
        return AMAS6Scheduler(desired_retention=0.85)
    raise ValueError(f"unknown scheduler: {name!r}")


def _sample_test_rows(
    paths: BenchPaths,
    n_users: int,
    max_words_per_user: int = 500,
    seed: int = 42,
    duckdb_threads: int = 4,
) -> pd.DataFrame:
    """从 test split 采 n_users 的 (user, word, history, next_t, next_r) 行。

    每个 (user, word) 取最长 history (按 LENGTH(t_history) ARG_MAX)。
    注：DuckDB 1.5.1 在某些 parquet 上 ROW_NUMBER 触发 InternalException,
    所以这里用 ARG_MAX(field, LENGTH(t_history)) 实现等价的 rn=1 语义。

    确定性：裸 LENGTH(t_history) 在并列时 tie-break 依赖扫描顺序（多线程下
    每个 ARG_MAX 还可能取自不同行），重复运行有 ~5e-5 logLoss 抖动。改用
    内容序复合键（长度 → 历史串 → 结果串 → next_t/next_r），全部 ARG_MAX
    共用同一键，保证重复运行返回逐行相同的结果；纯算法中立（不依赖任何
    scheduler 的得分）。
    """
    conn = duckdb.connect()
    conn.execute(f"PRAGMA threads={duckdb_threads}")
    parquet_path = (paths.parquet / "prefix_events.parquet").as_posix()

    # 先估 bucket
    bucket_counts = conn.execute(f"""
        SELECT user_bucket_100, COUNT(DISTINCT u) AS cnt
        FROM read_parquet('{parquet_path}')
        WHERE split = 'test'
        GROUP BY user_bucket_100
        ORDER BY user_bucket_100
    """).fetch_df()

    cumulative = 0
    needed: list[int] = []
    for _, row in bucket_counts.iterrows():
        needed.append(int(row["user_bucket_100"]))
        cumulative += int(row["cnt"])
        if cumulative >= n_users * 1.2:  # 稍微多采一些以便用户级 shuffle
            break

    bucket_list = ", ".join(str(b) for b in needed)
    # 确定性复合 tie-break 键：主序仍是 LENGTH(t_history)（最长 history 语义不变）
    tie_key = "(LENGTH(t_history), t_history, r_history, next_t, next_r, difficulty)"
    query = f"""
        SELECT
            u AS user_id,
            w AS word_id,
            ARG_MAX(t_history, {tie_key}) AS t_history,
            ARG_MAX(r_history, {tie_key}) AS r_history,
            ARG_MAX(difficulty, {tie_key}) AS difficulty,
            ARG_MAX(next_t, {tie_key}) AS next_t,
            ARG_MAX(next_r, {tie_key}) AS next_r
        FROM read_parquet('{parquet_path}')
        WHERE split = 'test'
          AND user_bucket_100 IN ({bucket_list})
          AND next_r IS NOT NULL
        GROUP BY u, w
    """
    full = conn.execute(query).fetch_df()
    conn.close()

    if full.empty:
        return full

    all_users = sorted(full["user_id"].unique().tolist())
    rng = random.Random(seed)
    rng.shuffle(all_users)
    selected = all_users[:n_users]
    df = full[full["user_id"].isin(selected)].copy()

    # 每用户限词数
    if max_words_per_user > 0:
        df = df.sort_values(["user_id", "word_id"]).reset_index(drop=True)
        df = (
            df.groupby("user_id", as_index=False, sort=False, group_keys=False)
            .head(max_words_per_user)
            .reset_index(drop=True)
        )
    return df


def _predict_p_recall(scheduler, elapsed_days: float) -> float:
    """给定已 warm_start 的 scheduler, 计算 elapsed 天后的 p_recall。

    优先用 scheduler 内部的 forgetting curve (AMAS / FSRS / FSRS45),
    否则 fallback 到 2^(-elapsed/halflife)。
    """
    # AMAS: 内部 _recall(elapsed) 函数 (mdm.rs 的 FSRS 曲线)
    if hasattr(scheduler, "_recall"):
        try:
            return float(scheduler._recall(float(elapsed_days)))
        except Exception:
            pass
    # FSRS / FSRS45: 内部 _state 有 recall(elapsed) 方法
    state = getattr(scheduler, "_state", None)
    if state is not None and hasattr(state, "recall"):
        try:
            return float(state.recall(float(elapsed_days)))
        except Exception:
            pass
    # 通用 fallback: 用 current_halflife
    hl = max(1e-6, float(scheduler.current_halflife()))
    return math.pow(2.0, -float(elapsed_days) / hl)


def evaluate_scheduler_prediction(
    paths: BenchPaths,
    scheduler_name: str,
    n_users: int = 300,
    max_words_per_user: int = 200,
    seed: int = 42,
) -> Dict[str, float]:
    """对单 scheduler 评估 prediction (logLoss / ICI / AUC / MAE)。"""
    df = _sample_test_rows(paths, n_users, max_words_per_user, seed)
    if df.empty:
        return {"logLoss": float("nan"), "ici": float("nan"), "auc": float("nan"), "maeP": float("nan"), "n_samples": 0}

    # DHP 需要 student
    dhp_student: DHPStudent | None = None
    if scheduler_name == "dhp":
        assets = ensure_reference_assets(paths.cache / "maimemo_reference")
        dhp_student = DHPStudent(**load_dhp_params(assets["parameters"]))

    acc = StreamingPredictionAccumulator()

    BATCH = 5000
    y_true_buf: list[float] = []
    p_buf: list[float] = []
    hl_buf: list[float] = []
    true_hl_buf: list[float] = []

    def _flush() -> None:
        if not y_true_buf:
            return
        acc.update(
            np.asarray(y_true_buf, dtype=float),
            np.asarray(p_buf, dtype=float),
            np.asarray(hl_buf, dtype=float),
            np.asarray(true_hl_buf, dtype=float),
        )
        y_true_buf.clear()
        p_buf.clear()
        hl_buf.clear()
        true_hl_buf.clear()

    for row in df.itertuples(index=False):
        t_hist = parse_history(row.t_history)
        r_hist = parse_history(row.r_history)
        diff_raw = row.difficulty
        diff = float(diff_raw) if not (isinstance(diff_raw, float) and math.isnan(diff_raw)) else 5.0
        elapsed = float(row.next_t) if row.next_t is not None else 0.0
        truth = int(row.next_r)

        sched = _build_scheduler(scheduler_name, dhp_student, seed)
        try:
            sched.warm_start(t_hist, r_hist, diff)
        except Exception:
            continue

        p = _predict_p_recall(sched, elapsed)
        # clip 防 log(0)
        p = max(1e-6, min(1.0 - 1e-6, p))
        hl = max(1e-6, float(sched.current_halflife()))
        # ground truth half-life from observed (next_t, next_r) is undefined for binary outcome;
        # use scheduler hl as a placeholder so SMAPE doesn't blow up (we don't report SMAPE).
        true_hl = max(1e-6, elapsed if truth == 0 else max(elapsed, hl))

        y_true_buf.append(float(truth))
        p_buf.append(float(p))
        hl_buf.append(float(hl))
        true_hl_buf.append(float(true_hl))

        if len(y_true_buf) >= BATCH:
            _flush()
    _flush()

    metrics = acc.finalize()
    return {
        "logLoss": float(metrics["logLoss"]),
        "ici": float(metrics["ici"]),
        "auc": float(metrics["auc"]),
        "maeP": float(metrics["maeP"]),
        "n_samples": int(acc.count),
    }


def evaluate_all_schedulers(
    paths: BenchPaths,
    scheduler_names: List[str],
    n_users: int = 300,
    max_words_per_user: int = 200,
    seed: int = 42,
) -> Dict[str, Dict[str, float]]:
    """对一组 scheduler 跑 prediction 评估,共享同一份采样。"""
    df = _sample_test_rows(paths, n_users, max_words_per_user, seed)
    if df.empty:
        return {name: {"logLoss": float("nan"), "ici": float("nan"), "auc": float("nan"), "maeP": float("nan"), "n_samples": 0} for name in scheduler_names}

    dhp_student: DHPStudent | None = None
    if "dhp" in scheduler_names:
        assets = ensure_reference_assets(paths.cache / "maimemo_reference")
        dhp_student = DHPStudent(**load_dhp_params(assets["parameters"]))

    # 预解析 history (避免重复 parse)
    parsed_rows = []
    for row in df.itertuples(index=False):
        t_hist = parse_history(row.t_history)
        r_hist = parse_history(row.r_history)
        diff_raw = row.difficulty
        diff = float(diff_raw) if not (isinstance(diff_raw, float) and math.isnan(diff_raw)) else 5.0
        elapsed = float(row.next_t) if row.next_t is not None else 0.0
        truth = int(row.next_r)
        parsed_rows.append((t_hist, r_hist, diff, elapsed, truth))

    results: Dict[str, Dict[str, float]] = {}
    for name in scheduler_names:
        t0 = time.time()
        acc = StreamingPredictionAccumulator()
        BATCH = 5000
        y_true_buf: list[float] = []
        p_buf: list[float] = []
        hl_buf: list[float] = []
        true_hl_buf: list[float] = []

        def _flush() -> None:
            if not y_true_buf:
                return
            acc.update(
                np.asarray(y_true_buf, dtype=float),
                np.asarray(p_buf, dtype=float),
                np.asarray(hl_buf, dtype=float),
                np.asarray(true_hl_buf, dtype=float),
            )
            y_true_buf.clear()
            p_buf.clear()
            hl_buf.clear()
            true_hl_buf.clear()

        for t_hist, r_hist, diff, elapsed, truth in parsed_rows:
            sched = _build_scheduler(name, dhp_student, seed)
            try:
                sched.warm_start(t_hist, r_hist, diff)
            except Exception:
                continue
            p = _predict_p_recall(sched, elapsed)
            p = max(1e-6, min(1.0 - 1e-6, p))
            hl = max(1e-6, float(sched.current_halflife()))
            true_hl = max(1e-6, elapsed if truth == 0 else max(elapsed, hl))

            y_true_buf.append(float(truth))
            p_buf.append(float(p))
            hl_buf.append(float(hl))
            true_hl_buf.append(float(true_hl))
            if len(y_true_buf) >= BATCH:
                _flush()
        _flush()
        m = acc.finalize()
        results[name] = {
            "logLoss": float(m["logLoss"]),
            "ici": float(m["ici"]),
            "auc": float(m["auc"]),
            "maeP": float(m["maeP"]),
            "n_samples": int(acc.count),
            "eval_seconds": round(time.time() - t0, 2),
        }
        print(f"[pred] {name:>8s} samples={acc.count} logLoss={m['logLoss']:.4f} auc={m['auc']:.4f} ({results[name]['eval_seconds']}s)")
    return results
