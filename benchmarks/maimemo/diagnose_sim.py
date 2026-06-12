"""diagnose_sim.py — Instrumented forward sim (split='val') for rank-1 head design.

Replicates simulate_strategies' protocol EXACTLY (per-strategy master-rng seeding,
warm-start, 90-day _run_day loop, per-user budget scaling) on split='val' for a
fixed strategy set, then dumps per-(dataset, algo) diagnostic JSON.

Usage::

    .bench-venv/bin/python -m benchmarks.maimemo.diagnose_sim \
        --root ~/.wordforge-bench/maimemo --dataset maimemo \
        --out benchmarks/results/2026-06-12-rank1-recon

The sim itself is identical to the official one; only post-loop instrumentation
is added. We reuse _sample_users / _build_word_states / _run_day from simulate.
"""
from __future__ import annotations

import argparse
import json
import statistics
from pathlib import Path
from typing import Any, Dict, List

import numpy as np

from .config import BenchPaths
from .models import GRUHLROracle
from .simulate import (
    WordState,
    _build_word_states,
    _run_day,
    _sample_users,
)

STRATEGIES = ["amas", "amas6", "fsrs45", "fsrs6"]

# interval bucket edges (days): <7 / 7-14 / 15-29 / 30-89 / >=90
def _ivl_bucket(d: int) -> str:
    if d < 7:
        return "lt7"
    if d <= 14:
        return "7_14"
    if d <= 29:
        return "15_29"
    if d <= 89:
        return "30_89"
    return "ge90"


# warm-start due_day buckets (days): <=7 / 8-14 / 15-29 / 30-89 / >=90
def _warm_bucket(d: int) -> str:
    if d <= 7:
        return "le7"
    if d <= 14:
        return "8_14"
    if d <= 29:
        return "15_29"
    if d <= 89:
        return "30_89"
    return "ge90"


def _empty_warm() -> Dict[str, int]:
    return {"le7": 0, "8_14": 0, "15_29": 0, "30_89": 0, "ge90": 0}


def _empty_ivl() -> Dict[str, int]:
    return {"lt7": 0, "7_14": 0, "15_29": 0, "30_89": 0, "ge90": 0}


def _quartile_edges(values: List[int]) -> List[float]:
    """Return 3 cut points (q25,q50,q75) for history-length quartiles."""
    if not values:
        return [0.0, 0.0, 0.0]
    arr = np.asarray(values, dtype=float)
    return [float(np.quantile(arr, q)) for q in (0.25, 0.50, 0.75)]


def _quartile_of(v: int, edges: List[float]) -> int:
    q25, q50, q75 = edges
    if v <= q25:
        return 0
    if v <= q50:
        return 1
    if v <= q75:
        return 2
    return 3


def diagnose_dataset(
    root: Path,
    dataset: str,
    out_dir: Path,
    sim_days: int = 90,
    n_users: int = 300,
    max_words_per_user: int = 500,
    daily_budget: int = 200,
    desired_retention: float = 0.85,
    seed: int = 42,
) -> Path:
    paths = BenchPaths.from_root(root)
    out_dir.mkdir(parents=True, exist_ok=True)

    oracle_path = paths.artifacts / "gru_hlr_oracle"
    oracle = GRUHLROracle.load(oracle_path)

    print(f"[diag:{dataset}] sampling {n_users} users from VAL split …", flush=True)
    df = _sample_users(
        paths, n_users, max_words_per_user, seed, duckdb_threads=4, split="val"
    )
    if df.empty:
        raise RuntimeError(f"[diag:{dataset}] no val-split rows")

    actual_users = int(df["user_id"].nunique())
    actual_words = len(df)
    print(f"[diag:{dataset}] {actual_users} users, {actual_words} pairs", flush=True)

    # Replicate simulate_strategies per-strategy seeding EXACTLY: a single master
    # rng draws one strategy_seed per strategy, in STRATEGIES order. This makes
    # cross-algo numbers comparable (same warm-start word set, deterministic
    # Bernoulli draws derived from the same master stream position).
    rng_master = np.random.default_rng(seed)

    out: Dict[str, Any] = {
        "dataset": dataset,
        "split": "val",
        "meta": {
            "n_users": actual_users,
            "n_pairs": actual_words,
            "sim_days": sim_days,
            "daily_budget_per_user": daily_budget,
            "desired_retention": desired_retention,
            "seed": seed,
            "strategies": STRATEGIES,
        },
        "algos": {},
    }

    for strategy_name in STRATEGIES:
        strategy_seed = int(rng_master.integers(0, 2**31))
        rng = np.random.default_rng(strategy_seed)

        # warm-start: build states + capture warm-start interval BEFORE the loop
        word_states: List[WordState] = _build_word_states(
            df,
            strategy_name=strategy_name,
            dhp_student=None,
            memory_config=None,  # mirrors simulate_strategies(memory_config=None)
            desired_retention=desired_retention,
            seed=strategy_seed,
        )

        # ---- warm-start snapshot (due_day == first next_interval_days) ----
        warm_ivl_all = _empty_warm()  # warm-start interval over ALL pairs
        warm_ivl_list: List[int] = []
        hist_len_list: List[int] = []  # warm-start history length per pair
        for ws in word_states:
            wd = int(ws.due_day)  # == next_ivl at build time, last_day==0
            warm_ivl_all[_warm_bucket(wd)] += 1
            warm_ivl_list.append(wd)
            hist_len_list.append(len(ws.t_history))

        print(
            f"[diag:{dataset}] strategy={strategy_name} seed={strategy_seed} "
            f"running {sim_days}d …",
            flush=True,
        )

        # ---- 90-day loop (identical to simulate_strategies) ----
        daily_due: List[int] = []
        daily_done: List[int] = []
        daily_recall: List[float] = []
        for day in range(sim_days):
            dm = _run_day(
                day=day,
                all_states=word_states,
                oracle=oracle,
                rng=rng,
                daily_budget=daily_budget * actual_users,
            )
            daily_due.append(dm.reviews_due)
            daily_done.append(dm.reviews_done)
            daily_recall.append(dm.recall_rate)

        # ----------------------------------------------------------------
        # Post-loop instrumentation
        # ----------------------------------------------------------------
        n_pairs = len(word_states)
        never_reviewed = 0
        never_warm_buckets = _empty_warm()  # warm due_day dist of never-reviewed
        final_ivl_reviewed = _empty_ivl()
        reviews_per_reviewed: List[int] = []
        mastered = 0
        mastered_hist_len: List[int] = []  # warm hist-len of mastered pairs

        # failure stats accumulators (review outcomes appended to r_history during sim)
        total_reviews_appended = 0
        total_failures = 0
        # oracle p_recall at review: reconstruct from oracle_halflife snapshots is
        # not stored per-review; we approximate the population mean recall at
        # review via the daily recall_rate weighted by reviews_done (this is the
        # SAME quantity simulate uses for policy). Reported separately below.

        # cross-tab: mastered membership vs warm-start history-length quartile
        hist_edges = _quartile_edges(hist_len_list)
        # quartile -> {"total": n_reviewed_in_q, "mastered": n_mastered_in_q}
        xtab = {q: {"total": 0, "mastered": 0, "reviewed": 0} for q in range(4)}

        # amas-only: adaptive desired_retention + ensemble interval_scale samples
        amas_retention_dist: Dict[str, int] = {}
        amas_ivl_scale_sample: List[float] = []

        for idx, ws in enumerate(word_states):
            sched = ws.scheduler
            warm_len = hist_len_list[idx]
            n_appended = len(ws.r_history) - warm_len  # reviews during sim
            reviewed = ws.last_day > 0

            if not reviewed:
                never_reviewed += 1
                never_warm_buckets[_warm_bucket(int(warm_ivl_list[idx]))] += 1
            else:
                reviews_per_reviewed.append(n_appended)
                fin_ivl = int(sched.next_interval_days())
                final_ivl_reviewed[_ivl_bucket(fin_ivl)] += 1

            # failures: outcomes appended during sim (r_history tail beyond warm_len)
            if n_appended > 0:
                sim_outcomes = ws.r_history[warm_len:]
                total_reviews_appended += len(sim_outcomes)
                total_failures += sum(1 for o in sim_outcomes if o == 0)

            # mastered (official def: reviewed AND final next_interval >= 30)
            is_mastered = reviewed and sched.next_interval_days() >= 30
            if is_mastered:
                mastered += 1
                mastered_hist_len.append(warm_len)

            # cross-tab
            q = _quartile_of(warm_len, hist_edges)
            xtab[q]["total"] += 1
            if reviewed:
                xtab[q]["reviewed"] += 1
            if is_mastered:
                xtab[q]["mastered"] += 1

            # amas-only adaptive instrumentation
            if strategy_name == "amas":
                ret = round(float(getattr(sched, "_retention", desired_retention)), 3)
                key = f"{ret:.3f}"
                amas_retention_dist[key] = amas_retention_dist.get(key, 0) + 1

        if strategy_name == "amas":
            # sample 200 pairs' ensemble interval_scale (deterministic stride)
            stride = max(1, n_pairs // 200)
            for ws in word_states[::stride][:200]:
                try:
                    amas_ivl_scale_sample.append(
                        round(float(ws.scheduler._ensemble_interval_scale()), 4)
                    )
                except Exception:
                    pass

        total_reviews = int(sum(daily_done))
        # budget saturation: days where due > done
        saturation_days = sum(1 for du, do in zip(daily_due, daily_done) if du > do)
        # how much over-budget on saturated days (sum of overflow)
        overflow_total = sum(max(0, du - do) for du, do in zip(daily_due, daily_done))

        reviewed_pairs = n_pairs - never_reviewed
        failure_rate = (
            total_failures / total_reviews_appended if total_reviews_appended else 0.0
        )
        # mean recall at review weighted by reviews_done (== oracle p_recall mean)
        weighted_recall_num = sum(r * d for r, d in zip(daily_recall, daily_done))
        weighted_recall = (
            weighted_recall_num / total_reviews if total_reviews else 0.0
        )

        algo_out: Dict[str, Any] = {
            "n_pairs": n_pairs,
            "never_reviewed": never_reviewed,
            "never_reviewed_warm_buckets": never_warm_buckets,
            "warm_start_ivl_all_buckets": warm_ivl_all,
            "reviewed_pairs": reviewed_pairs,  # mastered ceiling for graduation policy
            "mastered_count": mastered,
            "final_ivl_reviewed_buckets": final_ivl_reviewed,
            "reviews_per_reviewed_mean": (
                round(statistics.mean(reviews_per_reviewed), 3)
                if reviews_per_reviewed
                else 0.0
            ),
            "reviews_per_reviewed_median": (
                statistics.median(reviews_per_reviewed) if reviews_per_reviewed else 0
            ),
            "total_reviews": total_reviews,
            "failure_stats": {
                "total_reviews_with_outcome": total_reviews_appended,
                "total_failures": total_failures,
                "failure_rate_at_review": round(failure_rate, 4),
                "mean_oracle_p_recall_at_review": round(weighted_recall, 4),
            },
            "budget_saturation": {
                "saturation_days": saturation_days,
                "overflow_total": int(overflow_total),
                "sim_days": sim_days,
            },
            "xtab_mastered_vs_histlen_quartile": {
                "hist_len_quartile_edges_q25_q50_q75": [
                    round(e, 1) for e in hist_edges
                ],
                "quartiles": {
                    str(q): xtab[q] for q in range(4)
                },
            },
        }
        if strategy_name == "amas":
            algo_out["amas_adaptive_retention_dist"] = dict(
                sorted(amas_retention_dist.items())
            )
            algo_out["amas_ensemble_ivl_scale_sample"] = {
                "n": len(amas_ivl_scale_sample),
                "mean": (
                    round(statistics.mean(amas_ivl_scale_sample), 4)
                    if amas_ivl_scale_sample
                    else 0.0
                ),
                "median": (
                    round(statistics.median(amas_ivl_scale_sample), 4)
                    if amas_ivl_scale_sample
                    else 0.0
                ),
                "min": (round(min(amas_ivl_scale_sample), 4) if amas_ivl_scale_sample else 0.0),
                "max": (round(max(amas_ivl_scale_sample), 4) if amas_ivl_scale_sample else 0.0),
                "values": amas_ivl_scale_sample,
            }

        out["algos"][strategy_name] = algo_out
        print(
            f"[diag:{dataset}] {strategy_name}: reviewed={reviewed_pairs} "
            f"mastered={mastered} never_rev={never_reviewed} "
            f"totRev={total_reviews} sat_days={saturation_days} "
            f"fail_rate={failure_rate:.3f}",
            flush=True,
        )

    out_path = out_dir / f"diag_{dataset}.json"
    out_path.write_text(json.dumps(out, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"[diag:{dataset}] written → {out_path}", flush=True)
    return out_path


def main() -> None:
    parser = argparse.ArgumentParser(description="Instrumented diagnostic sim (val split)")
    parser.add_argument("--root", required=True)
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    diagnose_dataset(
        Path(args.root).expanduser(), args.dataset, Path(args.out)
    )


if __name__ == "__main__":
    main()
