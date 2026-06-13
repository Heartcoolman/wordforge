"""interval_shift.py — scheduling blast radius: OLD vs NEW(F1) day intervals.

Consumes prod_states.csv (export.sql Q2: live `engine_algo_states` mastery rows)
and computes, for every reviewed (user, word) state, the day-level interval that
the OLD pre-campaign config vs the NEW frozen F1 config would serve (spec §4):

  OLD: legacy mdm.rs compute_interval at day granularity
       (curve from w[20]=0.26325..., desired_retention=0.9, no GSP) ->
       old_days = max(1, min(ceil(raw_days * max(scale, 0.1)), 90))
  NEW: WordforgeMirrorState.interval_days() with DEFAULT_MEMORY_MODEL_CONFIG
       (banded retention: S<14d -> 0.86, else 0.92; capped 90, ceil'd) followed by
       the local GSP head (contract benchmarks/maimemo/GSP_SPEC.md §3/§7):
       scale -> graduation floor (streak>=2 -> max(.,30)) -> cap min(90,40)
       -> fuzz (F1 fuzz=0 -> no-op) -> banker's rounding -> max(1)

States with review_count==0 (never reviewed -> no production interval) are
excluded and counted separately.

Caveats (stated in output): production interval_scale is a dynamic ensemble
output — tool default 1.0 with optional --scale-sweep {0.8, 1.0, 1.2} sensitivity;
OLD leg uses base retention 0.9, not the adaptive ±0.03/0.05 adjustments.

Usage:
  .bench-venv/bin/python benchmarks/prod_replay/interval_shift.py \
      --states prod_states.csv [--scale 1.0] [--scale-sweep] [--out report.json]
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Dict, List

import numpy as np
import pandas as pd

_REPO_ROOT = Path(__file__).resolve().parents[2]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from benchmarks.maimemo.dhp_reference import (  # noqa: E402
    WordforgeMirrorState,
    _gsp_band_kwargs_from_config,
    _mirror_from_config,
)
from benchmarks.prod_replay.configs import (  # noqa: E402
    NEW_MEMORY_MODEL_CONFIG,
    OLD_MEMORY_MODEL_CONFIG,
)

MAX_INTERVAL_DAYS = 90.0
# log2-spaced ratio histogram edges: 2^(k/2) for k in -8..8 (1/16x .. 16x)
RATIO_EDGES = [2.0 ** (k / 2.0) for k in range(-8, 9)]


def gsp_head(
    base_days_int: int,
    interval_scale: float,
    correct_streak: int,
    stability: float,
    review_count: int,
    cap_days: float,
    grad_streak: int,
    grad_floor: float,
    fuzz: float,
) -> tuple[int, bool, bool, bool]:
    """GSP scheduling head, local mirror of GSP_SPEC §3 steps 3-6 + §7
    (= schedulers.py AMASScheduler.next_interval_days / mdm.rs gsp_schedule_days).
    Returns (days, graduated, floor_binding, cap_pinned)."""
    scaled = max(1.0, float(base_days_int) * max(interval_scale, 0.1))     # step 3
    graduated = grad_streak > 0 and correct_streak >= grad_streak          # step 4
    floor_binding = graduated and scaled < grad_floor
    if graduated:
        scaled = max(scaled, grad_floor)
    cap = MAX_INTERVAL_DAYS                                                # step 5
    if cap_days > 0.0:
        cap = min(cap, cap_days)
    cap_pinned = scaled > cap
    scaled = min(scaled, cap)
    if fuzz > 0.0:                                                         # step 5.5
        h = stability * 12.9898 + float(review_count) * 78.233
        u = 2.0 * (h - math.floor(h)) - 1.0
        fuzz_cap = min(MAX_INTERVAL_DAYS, cap * (1.0 + fuzz))
        lo = grad_floor if graduated else 1.0
        scaled = max(lo, min(fuzz_cap, scaled * (1.0 + fuzz * u)))
    return max(1, int(round(scaled))), graduated, floor_binding, cap_pinned  # step 6


def _old_mirror() -> WordforgeMirrorState:
    mirror = _mirror_from_config(OLD_MEMORY_MODEL_CONFIG)
    # OLD leg mirrors mdm.rs compute_interval order: cap applies AFTER interval_scale,
    # so lift the mirror-internal pre-scale cap and apply min(., 90) ourselves.
    mirror.max_interval_days = float("inf")
    return mirror


def _new_mirror() -> WordforgeMirrorState:
    mirror = _mirror_from_config(NEW_MEMORY_MODEL_CONFIG)
    # _mirror_from_config transfers success_grade only; banded-retention knobs are
    # applied by AMASScheduler._fresh_state in the harness — replicate here.
    band = _gsp_band_kwargs_from_config(NEW_MEMORY_MODEL_CONFIG)
    mirror.gsp_maturity_band_days = band["gsp_maturity_band_days"]
    mirror.gsp_young_retention = band["gsp_young_retention"]
    mirror.gsp_mature_retention = band["gsp_mature_retention"]
    return mirror


def _gsp_knobs() -> Dict[str, float]:
    """Entrance clamps mirror AMASScheduler.__init__ (GSP_SPEC §2)."""
    cfg = NEW_MEMORY_MODEL_CONFIG
    return {
        "cap_days": max(0.0, float(cfg.get("gspIntervalCapDays", 0.0))),
        "grad_streak": max(0, int(cfg.get("gspGraduationStreak", 0))),
        "grad_floor": max(0.0, float(cfg.get("gspGraduationFloorDays", 30.0))),
        "fuzz": max(0.0, min(1.0 - 1e-9, float(cfg.get("gspIntervalFuzz", 0.0)))),
    }


def compute_rows(states: pd.DataFrame, interval_scale: float) -> List[dict]:
    mirror_old = _old_mirror()
    mirror_new = _new_mirror()
    knobs = _gsp_knobs()
    band_days = mirror_new.gsp_maturity_band_days
    rows: List[dict] = []
    for st in states.itertuples(index=False):
        stability = max(0.01, float(st.stability))  # mdm.rs s = stability.max(0.01)
        difficulty = float(st.difficulty)
        streak = int(st.correct_streak)
        review_count = int(st.review_count)

        mirror_old.stability = stability
        mirror_old.difficulty = difficulty
        raw_old = mirror_old._interval_days_raw()  # uncapped (max_interval=inf)
        old_days = max(1, min(math.ceil(raw_old * max(interval_scale, 0.1)), int(MAX_INTERVAL_DAYS)))

        mirror_new.stability = stability
        mirror_new.difficulty = difficulty
        base_new = mirror_new.interval_days()  # banded retention + min(.,90) + ceil + max(1)
        new_days, graduated, floor_binding, cap_pinned = gsp_head(
            base_new, interval_scale, streak, stability, review_count, **knobs
        )
        rows.append({
            "user_id": str(st.user_id),
            "word_id": str(st.word_id),
            "stability": stability,
            "correct_streak": streak,
            "review_count": review_count,
            "raw_old_days": raw_old,
            "old_days": old_days,
            "base_new_days": base_new,
            "new_days": new_days,
            "graduated": graduated,
            "floor_binding": floor_binding,
            "cap_pinned": cap_pinned,
            "young_band": stability < band_days,
        })
    return rows


def _quantiles(values: List[int]) -> Dict[str, float]:
    arr = np.asarray(values, dtype=float)
    return {f"p{q}": float(np.percentile(arr, q)) for q in (5, 25, 50, 75, 95)}


def summarize(rows: List[dict], interval_scale: float) -> Dict[str, object]:
    old = [r["old_days"] for r in rows]
    new = [r["new_days"] for r in rows]
    delta = [n - o for n, o in zip(new, old)]
    ratio = [n / o for n, o in zip(new, old)]

    delta_hist: Dict[str, int] = {}
    for d in sorted(delta):
        delta_hist[str(d)] = delta_hist.get(str(d), 0) + 1

    ratio_hist: Dict[str, int] = {}
    for x in ratio:
        if x < RATIO_EDGES[0]:
            key = f"<{RATIO_EDGES[0]:.4f}"
        elif x >= RATIO_EDGES[-1]:
            key = f">={RATIO_EDGES[-1]:.4f}"
        else:
            i = max(0, np.searchsorted(RATIO_EDGES, x, side="right") - 1)
            key = f"[{RATIO_EDGES[i]:.4f},{RATIO_EDGES[i + 1]:.4f})"
        ratio_hist[key] = ratio_hist.get(key, 0) + 1

    per_user: Dict[str, Dict[str, object]] = {}
    for r in rows:
        agg = per_user.setdefault(r["user_id"], {"n": 0, "sum_old": 0, "sum_new": 0,
                                                 "n_shrunk": 0, "n_grew": 0})
        agg["n"] += 1
        agg["sum_old"] += r["old_days"]
        agg["sum_new"] += r["new_days"]
        d = r["new_days"] - r["old_days"]
        agg["n_shrunk"] += int(d < 0)
        agg["n_grew"] += int(d > 0)
    for agg in per_user.values():
        agg["mean_old_days"] = round(agg.pop("sum_old") / agg["n"], 2)
        agg["mean_new_days"] = round(agg.pop("sum_new") / agg["n"], 2)

    return {
        "interval_scale": interval_scale,
        "n_scored": len(rows),
        "n_old_at_90_now_le_40": sum(1 for r in rows if r["old_days"] == 90 and r["new_days"] <= 40),
        "n_new_pinned_at_cap40": sum(1 for r in rows if r["cap_pinned"]),
        "n_graduated": sum(1 for r in rows if r["graduated"]),
        "n_floor_binding": sum(1 for r in rows if r["floor_binding"]),
        "band_split": {
            "young_stability_lt_band": sum(1 for r in rows if r["young_band"]),
            "mature_stability_ge_band": sum(1 for r in rows if not r["young_band"]),
        },
        "n_shrunk": sum(1 for d in delta if d < 0),
        "n_grew": sum(1 for d in delta if d > 0),
        "n_unchanged": sum(1 for d in delta if d == 0),
        "old_days_quantiles": _quantiles(old),
        "new_days_quantiles": _quantiles(new),
        "delta_days_histogram_new_minus_old": delta_hist,
        "ratio_new_over_old_histogram_log2_spaced": ratio_hist,
        "per_user": per_user,
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--states", required=True, help="prod_states.csv (export.sql Q2)")
    ap.add_argument("--scale", type=float, default=1.0,
                    help="static interval_scale stand-in for the dynamic ensemble (default 1.0)")
    ap.add_argument("--scale-sweep", action="store_true",
                    help="additionally report {0.8, 1.0, 1.2} sensitivity summaries")
    ap.add_argument("--dump-rows", default=None, help="write per-row CSV here (spot checks)")
    ap.add_argument("--out", default=None, help="write JSON report here (default stdout only)")
    args = ap.parse_args()

    states = pd.read_csv(Path(args.states).expanduser(), dtype={"user_id": str, "word_id": str})
    required = {"user_id", "word_id", "stability", "difficulty", "review_count", "correct_streak"}
    missing = required - set(states.columns)
    if missing:
        raise SystemExit(f"states csv missing columns: {sorted(missing)}")

    n_total = len(states)
    never = states[states["review_count"].astype(int) == 0]
    scored_states = states[states["review_count"].astype(int) > 0]

    rows = compute_rows(scored_states, args.scale)
    report: Dict[str, object] = {
        "n_total_states": n_total,
        "n_never_reviewed_excluded": len(never),
        **summarize(rows, args.scale),
        "caveats": [
            "interval_scale is a static stand-in (default 1.0); production value is the "
            "dynamic ensemble output (engine.rs:1247-1344) — see scale_sensitivity",
            "OLD leg uses base retention 0.9, not the adaptive ±0.03/0.05 "
            "adjustments (mdm.rs adaptive_desired_retention)",
            "day granularity: OLD ceil-of-days mirrors mdm.rs compute_interval "
            "second-level truncation to within <1 day",
        ],
    }
    if args.scale_sweep:
        sweep = {}
        for s in (0.8, 1.0, 1.2):
            full = summarize(compute_rows(scored_states, s), s)
            sweep[str(s)] = {k: full[k] for k in (
                "n_old_at_90_now_le_40", "n_new_pinned_at_cap40", "n_graduated",
                "n_floor_binding", "n_shrunk", "n_grew", "n_unchanged",
                "old_days_quantiles", "new_days_quantiles",
            )}
        report["scale_sensitivity"] = sweep

    if args.dump_rows:
        pd.DataFrame(rows).to_csv(Path(args.dump_rows).expanduser(), index=False)
        print(f"[interval_shift] wrote per-row dump -> {args.dump_rows}", file=sys.stderr)

    blob = json.dumps(report, indent=2)
    print(blob)
    if args.out:
        Path(args.out).expanduser().write_text(blob + "\n", encoding="utf-8")
        print(f"[interval_shift] wrote {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
