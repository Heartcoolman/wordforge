"""simulate.py — Forward simulation engine for spaced-repetition algorithm comparison.

Usage (Python API)::

    from benchmarks.maimemo.simulate import simulate_strategies
    from benchmarks.maimemo.config import resolve_bench_root

    paths = resolve_bench_root()
    result = simulate_strategies(
        paths=paths,
        strategies=["fsrs", "dhp", "leitner", "random"],
        n_users=200,
        sim_days=90,
        daily_budget=200,
        seed=42,
    )
    # result is a dict serialisable to JSON

Design
------
The simulation is *forward*: we pick a set of real users from
``prefix_events.parquet`` (test split), warm-start each scheduler with the
user's existing review history, then run a day-by-day loop.  On each day we:

1. Collect all (user, word) pairs that are due for review.
2. Sort by urgency (lowest recall probability first) and apply a daily budget cap.
3. Batch-infer half-lives with the GRU Oracle (the "true memory" simulator).
4. Bernoulli-sample actual recall outcomes.
5. Update each scheduler's state and schedule the next review.
6. Collect daily metrics (recall rate, review count, expected memory).

The GRU Oracle acts *only* as the user-memory simulator; it does not influence
scheduling decisions.  This ensures a fair comparison between strategies.
"""
from __future__ import annotations

import copy
import json
import math
import random
from collections import defaultdict
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Collection, Dict, List, Optional, Tuple

import numpy as np
import pandas as pd

from .config import BenchPaths, resolve_bench_root
from .dhp_reference import DHPStudent, ensure_reference_assets, load_dhp_params, load_policy_grids
from .models import GRUHLROracle, parse_history
from .schedulers import AVAILABLE_SCHEDULERS, Scheduler, build_scheduler


# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------

# T1.4: mastered 判定的半衰期阈值（天），与调度 floor(gspGraduationFloorDays) 解耦。
# 用 Oracle 估的记忆半衰期衡量真实长期保持，不读调度区间。
MASTERED_HALFLIFE_DAYS = 180.0


@dataclass
class WordState:
    """Per-(user, word) simulation state."""
    user_id: str
    word_id: str
    t_history: List[int]          # days between reviews (raw from parquet)
    r_history: List[int]          # outcomes (0/1)
    difficulty: float             # 1-10
    due_day: int                  # absolute simulation day when next review is due
    last_day: int                 # absolute simulation day of last review
    scheduler: Scheduler          # live scheduler instance
    oracle_halflife: float = 1.0  # half-life estimated by Oracle after last review


@dataclass
class DailyMetrics:
    day: int
    reviews_due: int      # words that became due on this day
    reviews_done: int     # reviews actually performed (budget-capped)
    recall_rate: float    # mean recall probability at review time
    expected_memory: float  # Σ 2^(-(today-last_day)/halflife) over all seen words


@dataclass
class StrategyResult:
    name: str
    daily: List[DailyMetrics] = field(default_factory=list)
    total_reviews: int = 0
    mastered_count: int = 0          # words with halflife ≥ 180 days at end
    expected_memory_final: float = 0.0
    efficiency: float = 0.0          # expected_memory_final / max(total_reviews, 1)

    def finalize(self) -> None:
        self.total_reviews = sum(d.reviews_done for d in self.daily)
        self.efficiency = self.expected_memory_final / max(self.total_reviews, 1)


# ---------------------------------------------------------------------------
# Sampling helpers
# ---------------------------------------------------------------------------

def _sample_users(
    paths: BenchPaths,
    n_users: int,
    max_words_per_user: int,
    seed: int,
    duckdb_threads: int = 4,
    split: str = "test",
    exclude_users: Collection[str] | None = None,
) -> pd.DataFrame:
    """Return the most-recent prefix row per (user, word) from the given split.

    Sampling is pushed entirely into DuckDB to avoid loading the full dataset.
    If n_users == -1, all users are returned.

    exclude_users（算法中性，默认 None ⇒ 过滤串为空，结果与现状恒等）：
    从候选池中排除指定用户 id；注入 bucket 计数查询与主查询两处，
    使累计计数反映排除后的真实可用量（不足时自动拉入更多 bucket）。
    """
    assert split in ("train", "val", "test"), f"invalid split: {split!r}"
    import duckdb
    import math as _math

    conn = duckdb.connect()
    conn.execute(f"PRAGMA threads={duckdb_threads}")

    if exclude_users:
        conn.register(
            "_excluded_users",
            pd.DataFrame({"u": sorted(set(map(str, exclude_users)))}),
        )
        exclude_filter = "AND u NOT IN (SELECT u FROM _excluded_users)"
    else:
        exclude_filter = ""

    parquet_path = (paths.parquet / "prefix_events.parquet").as_posix()

    if n_users == -1:
        user_filter = ""
    else:
        # user_bucket_100 partitions users into 100 roughly equal buckets (0-99).
        # First, count distinct users per bucket to estimate how many buckets we need.
        bucket_counts = conn.execute(f"""
            SELECT user_bucket_100, COUNT(DISTINCT u) AS cnt
            FROM read_parquet('{parquet_path}')
            WHERE split = '{split}'
              {exclude_filter}
            GROUP BY user_bucket_100
            ORDER BY user_bucket_100
        """).fetch_df()

        cumulative = 0
        needed_buckets: list[int] = []
        for _, row in bucket_counts.iterrows():
            needed_buckets.append(int(row["user_bucket_100"]))
            cumulative += int(row["cnt"])
            if cumulative >= n_users:
                break

        bucket_list = ", ".join(str(b) for b in needed_buckets)
        user_filter = f"AND user_bucket_100 IN ({bucket_list})"

    query = f"""
        WITH ranked AS (
            SELECT
                u  AS user_id,
                w  AS word_id,
                t_history,
                r_history,
                difficulty,
                ROW_NUMBER() OVER (PARTITION BY u, w ORDER BY LENGTH(t_history) DESC) AS rn
            FROM read_parquet('{parquet_path}')
            WHERE split = '{split}'
              {user_filter}
              {exclude_filter}
        )
        SELECT user_id, word_id, t_history, r_history, difficulty
        FROM ranked
        WHERE rn = 1
        ORDER BY user_id, word_id
    """
    full = conn.execute(query).fetch_df()
    conn.close()

    if full.empty:
        return full

    # Fine-grained user subsample (the bucket filter may have fetched slightly
    # more than n_users distinct users).
    all_users = sorted(full["user_id"].unique().tolist())
    rng = random.Random(seed)
    rng.shuffle(all_users)
    selected_users = all_users if n_users == -1 else all_users[:n_users]
    assert not (set(selected_users) & set(exclude_users or ())), \
        "exclude_users leaked into the selected cohort"

    df = full[full["user_id"].isin(selected_users)].copy()

    # Cap words per user (stable head — preserves user_id column)
    if max_words_per_user > 0:
        df = df.sort_values(["user_id", "word_id"]).reset_index(drop=True)
        df = df.groupby("user_id", as_index=False, sort=False, group_keys=False).head(max_words_per_user).reset_index(drop=True)

    return df


# ---------------------------------------------------------------------------
# Warm-start helpers
# ---------------------------------------------------------------------------

def _build_word_states(
    df: pd.DataFrame,
    strategy_name: str,
    dhp_student: Optional[DHPStudent],
    memory_config: Optional[dict],
    desired_retention: float,
    seed: int,
    ssp_policy: Optional[dict] = None,
) -> List[WordState]:
    """Instantiate WordState for every (user, word) row, warm-starting the scheduler."""
    states: List[WordState] = []

    for row in df.itertuples(index=False):
        t_hist = parse_history(row.t_history)
        r_hist = parse_history(row.r_history)
        diff = float(row.difficulty) if not (isinstance(row.difficulty, float) and math.isnan(row.difficulty)) else 5.0

        sched = build_scheduler(
            strategy_name,
            dhp_student=dhp_student,
            memory_config=memory_config,
            desired_retention=desired_retention,
            seed=seed,
            ssp_policy=ssp_policy,
        )
        sched.warm_start(t_hist, r_hist, diff)

        # The word is due for review starting on simulation day 0
        next_ivl = sched.next_interval_days()

        states.append(WordState(
            user_id=str(row.user_id),
            word_id=str(row.word_id),
            t_history=t_hist,
            r_history=r_hist,
            difficulty=diff,
            due_day=next_ivl,  # already has a history, so first review is in the future
            last_day=0,
            scheduler=sched,
            oracle_halflife=sched.current_halflife(),
        ))

    return states


# ---------------------------------------------------------------------------
# Oracle batch inference helper
# ---------------------------------------------------------------------------

def _oracle_halflives_batch(
    oracle: GRUHLROracle,
    states: List[WordState],
    elapsed_days_list: List[float],
    batch_size: int = 4096,
) -> np.ndarray:
    """Batch-infer Oracle half-lives for a list of WordStates at given elapsed days.

    Returns an array of half-lives (same order as states).
    """
    rows = []
    for ws, elapsed in zip(states, elapsed_days_list):
        rows.append({
            "t_history": ",".join(str(t) for t in ws.t_history),
            "r_history": ",".join(str(r) for r in ws.r_history),
            "difficulty": ws.difficulty,
            "next_t": float(elapsed),
            "next_r": 0,
            "dhp_p_recall": math.pow(2.0, -float(elapsed) / max(ws.oracle_halflife, 1e-6)),
        })

    if not rows:
        return np.empty(0)

    frame = pd.DataFrame(rows)
    pred = oracle.predict_raw(frame, batch_size=batch_size)
    return pred.half_lives


# ---------------------------------------------------------------------------
# Single-day simulation step
# ---------------------------------------------------------------------------

def _run_day(
    day: int,
    all_states: List[WordState],
    oracle: GRUHLROracle,
    rng: np.random.Generator,
    daily_budget: int,
    oracle_batch_size: int = 4096,
) -> DailyMetrics:
    """Simulate one day.  Mutates WordState objects in place."""

    # Collect due reviews
    due: List[Tuple[int, WordState]] = [
        (idx, ws) for idx, ws in enumerate(all_states) if ws.due_day <= day
    ]

    reviews_due = len(due)
    if not due:
        # Still compute expected memory across all seen words
        expected_mem = _expected_memory(all_states, day)
        return DailyMetrics(
            day=day,
            reviews_due=0,
            reviews_done=0,
            recall_rate=0.0,
            expected_memory=expected_mem,
        )

    # Sort by urgency (descending score = most urgent first).
    # AMAS schedulers expose urgency_score() which mirrors word_selector.rs
    # score_review_word_prefetched(); other schedulers fall back to (1 - recall).
    def _urgency(item: Tuple[int, WordState]) -> float:
        _, ws = item
        elapsed = float(max(0, day - ws.last_day))
        if hasattr(ws.scheduler, "urgency_score"):
            return ws.scheduler.urgency_score(elapsed)
        recall = math.pow(2.0, -elapsed / max(ws.oracle_halflife, 1e-6))
        return 1.0 - recall   # higher = more urgent

    due.sort(key=_urgency, reverse=True)

    # Apply budget cap
    due_capped = due[:daily_budget]

    # Build elapsed days for batch Oracle inference
    elapsed_list = [float(max(0, day - ws.last_day)) for _, ws in due_capped]

    # Batch Oracle inference → true half-lives
    halflives = _oracle_halflives_batch(oracle, [ws for _, ws in due_capped], elapsed_list, oracle_batch_size)

    # Vectorised Bernoulli sampling
    p_recall = np.power(2.0, -np.asarray(elapsed_list) / np.maximum(halflives, 1e-6))
    recalled_arr = (rng.random(len(p_recall)) < p_recall).astype(int)

    # Update states
    for i, (_, ws) in enumerate(due_capped):
        recalled = int(recalled_arr[i])
        elapsed_days = elapsed_list[i]

        # Update Oracle half-life from batch inference result
        ws.oracle_halflife = float(halflives[i])

        # Update scheduler state
        ws.scheduler.update(recalled, max(elapsed_days, 1.0))

        # Schedule next review
        next_ivl = ws.scheduler.next_interval_days()
        ws.due_day = day + next_ivl
        ws.last_day = day

        # Append to history (for Oracle warm-state on next batch call)
        ws.t_history = ws.t_history + [max(1, int(round(elapsed_days)))]
        ws.r_history = ws.r_history + [recalled]

    recall_rate = float(np.mean(p_recall)) if len(p_recall) > 0 else 0.0
    expected_mem = _expected_memory(all_states, day)

    return DailyMetrics(
        day=day,
        reviews_due=reviews_due,
        reviews_done=len(due_capped),
        recall_rate=recall_rate,
        expected_memory=expected_mem,
    )


def _expected_memory(states: List[WordState], day: int) -> float:
    """Σ 2^(-(day - last_day) / oracle_halflife) for all seen words."""
    total = 0.0
    for ws in states:
        elapsed = max(0, day - ws.last_day)
        total += math.pow(2.0, -elapsed / max(ws.oracle_halflife, 1e-6))
    return total


# ---------------------------------------------------------------------------
# Main entry point
# ---------------------------------------------------------------------------

def simulate_strategies(
    paths: BenchPaths,
    strategies: List[str] | None = None,
    n_users: int = 200,
    sim_days: int = 90,
    max_words_per_user: int = 500,
    daily_budget: int = 200,
    desired_retention: float = 0.85,
    memory_config: dict | None = None,
    oracle_batch_size: int = 4096,
    seed: int = 42,
    duckdb_threads: int = 4,
    split: str = "test",
    exclude_users: Collection[str] | None = None,
) -> Dict[str, Any]:
    """Run forward simulation and return a comparison report.

    Args:
        paths: BenchPaths pointing to benchmark data root.
        strategies: list of strategy names to compare.  Defaults to all four.
        n_users: number of users to sample from the test split (-1 = all).
        sim_days: number of simulation days.
        max_words_per_user: cap on words per user (prevents memory blow-up).
        daily_budget: maximum reviews per user per day.
        desired_retention: target recall for interval scheduling.
        memory_config: FSRS weights override (default = DEFAULT_MEMORY_MODEL_CONFIG).
        oracle_batch_size: batch size for GRU Oracle inference.
        seed: global RNG seed.
        duckdb_threads: DuckDB thread count.

    Returns:
        A dict with keys ``strategies``, ``meta``, ``comparison`` that is
        directly JSON-serialisable.
    """
    if strategies is None:
        strategies = list(AVAILABLE_SCHEDULERS)
    unknown = [s for s in strategies if s not in AVAILABLE_SCHEDULERS]
    if unknown:
        raise ValueError(f"unknown strategies: {unknown}; choose from {AVAILABLE_SCHEDULERS}")

    # -----------------------------------------------------------------------
    # Load oracle
    # -----------------------------------------------------------------------
    oracle_path = paths.artifacts / "gru_hlr_oracle"
    if not oracle_path.with_suffix(".pt").exists():
        raise RuntimeError(
            "GRU Oracle not found; run 'fit_oracle' first:\n"
            "  python -m benchmarks.maimemo fit_oracle"
        )
    oracle = GRUHLROracle.load(oracle_path)

    # -----------------------------------------------------------------------
    # Load DHP student (needed for dhp strategy)
    # -----------------------------------------------------------------------
    dhp_student: Optional[DHPStudent] = None
    ssp_policy: Optional[dict] = None
    if "dhp" in strategies or "ssp_mmc" in strategies:
        assets = ensure_reference_assets(paths.cache / "maimemo_reference")
        dhp_student = DHPStudent(**load_dhp_params(assets["parameters"]))
        if "ssp_mmc" in strategies:
            ssp_policy = load_policy_grids(assets["policy_dir"])

    # -----------------------------------------------------------------------
    # Sample users from test split
    # -----------------------------------------------------------------------
    print(f"[simulate] sampling {n_users} users from {split} split …")
    df = _sample_users(paths, n_users, max_words_per_user, seed, duckdb_threads, split=split, exclude_users=exclude_users)

    if df.empty:
        raise RuntimeError(
            "No test-split rows found in prefix_events.parquet. "
            "Run 'prepare' first to build the dataset."
        )

    actual_users = int(df["user_id"].nunique())
    actual_words = len(df)
    print(f"[simulate] {actual_users} users, {actual_words} (user, word) pairs loaded")

    # -----------------------------------------------------------------------
    # Run each strategy
    # -----------------------------------------------------------------------
    results: Dict[str, StrategyResult] = {}
    rng_master = np.random.default_rng(seed)

    for strategy_name in strategies:
        print(f"[simulate] strategy={strategy_name!r}  days={sim_days}  budget={daily_budget}")

        # Seed per-strategy RNG from master so strategies are independent but
        # deterministic regardless of order.
        strategy_seed = int(rng_master.integers(0, 2**31))
        rng = np.random.default_rng(strategy_seed)

        # Build word states (warm-start)
        word_states = _build_word_states(
            df,
            strategy_name=strategy_name,
            dhp_student=dhp_student,
            memory_config=memory_config,
            desired_retention=desired_retention,
            seed=strategy_seed,
            ssp_policy=ssp_policy,
        )

        result = StrategyResult(name=strategy_name)

        for day in range(sim_days):
            day_metrics = _run_day(
                day=day,
                all_states=word_states,
                oracle=oracle,
                rng=rng,
                daily_budget=daily_budget * actual_users,  # budget is per-user, scale by users
                oracle_batch_size=oracle_batch_size,
            )
            result.daily.append(day_metrics)

        # Final state metrics
        # T1.4: mastered 口径与 gspGraduationFloorDays 解耦。旧口径 next_interval>=30 与默认
        # floor=30 同阈值 → 毕业旋钮直接拉高 mastered（自我实现，不可证伪记忆质量）。改用 Oracle 估的
        # 半衰期 oracle_halflife >= MASTERED_HALFLIFE_DAYS（与 StrategyResult.mastered_count 注释
        # "halflife >= 180 days" 一致、与调度 floor 无关），衡量真实长期保持而非调度尾部。
        result.expected_memory_final = result.daily[-1].expected_memory if result.daily else 0.0
        result.mastered_count = sum(
            1 for ws in word_states
            if ws.last_day > 0 and ws.oracle_halflife >= MASTERED_HALFLIFE_DAYS
        )
        result.finalize()
        results[strategy_name] = result
        print(
            f"  → reviews={result.total_reviews}  "
            f"mastered={result.mastered_count}  "
            f"efficiency={result.efficiency:.4f}"
        )

    # -----------------------------------------------------------------------
    # Comparison summary
    # -----------------------------------------------------------------------
    comparison = _build_comparison(results, sim_days, actual_users)

    # -----------------------------------------------------------------------
    # Serialise
    # -----------------------------------------------------------------------
    output = {
        "meta": {
            "n_users": actual_users,
            "n_word_pairs": actual_words,
            "sim_days": sim_days,
            "daily_budget_per_user": daily_budget,
            "max_words_per_user": max_words_per_user,
            "desired_retention": desired_retention,
            "seed": seed,
            "split": split,
            "strategies": strategies,
        },
        "strategies": {
            name: _serialise_result(res) for name, res in results.items()
        },
        "comparison": comparison,
    }

    report_path = paths.reports / "simulation_summary.json"
    report_path.write_text(json.dumps(output, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"[simulate] report written to {report_path}")

    return output


# ---------------------------------------------------------------------------
# Comparison & serialisation helpers
# ---------------------------------------------------------------------------

def _build_comparison(
    results: Dict[str, StrategyResult],
    sim_days: int,
    n_users: int,
) -> Dict[str, Any]:
    """Build a side-by-side comparison table and ranking."""
    rows = []
    for name, res in results.items():
        avg_daily_reviews = res.total_reviews / max(sim_days * n_users, 1)
        avg_recall = (
            sum(d.recall_rate for d in res.daily if d.reviews_done > 0)
            / max(sum(1 for d in res.daily if d.reviews_done > 0), 1)
        )
        rows.append({
            "strategy": name,
            "total_reviews": res.total_reviews,
            "avg_daily_reviews_per_user": round(avg_daily_reviews, 2),
            "mastered_count": res.mastered_count,
            "expected_memory_final": round(res.expected_memory_final, 2),
            "efficiency": round(res.efficiency, 6),
            "avg_recall_at_review": round(avg_recall, 4),
        })

    # Rank by efficiency (higher is better)
    rows.sort(key=lambda r: r["efficiency"], reverse=True)
    for rank, row in enumerate(rows, 1):
        row["rank_efficiency"] = rank

    # Rank by mastered_count
    for rank, row in enumerate(
        sorted(rows, key=lambda r: r["mastered_count"], reverse=True), 1
    ):
        row["rank_mastered"] = rank

    return {"table": rows}


def _serialise_result(res: StrategyResult) -> Dict[str, Any]:
    """Convert StrategyResult to a JSON-safe dict."""
    return {
        "total_reviews": res.total_reviews,
        "mastered_count": res.mastered_count,
        "expected_memory_final": round(res.expected_memory_final, 4),
        "efficiency": round(res.efficiency, 6),
        "daily": [
            {
                "day": d.day,
                "reviews_due": d.reviews_due,
                "reviews_done": d.reviews_done,
                "recall_rate": round(d.recall_rate, 4),
                "expected_memory": round(d.expected_memory, 2),
            }
            for d in res.daily
        ],
    }
