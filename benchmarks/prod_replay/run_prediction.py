"""run_prediction.py — prediction-leg replay: OLD vs NEW (F1) on production events.

Calls the unmodified maimemo harness (`evaluate_scheduler_prediction`) twice with
`memory_config` = OLD / NEW against a production bench root built by
build_parquet.py, then applies the pre-registered statistical-power gating
(spec §5):

  - n >= 1000: report ICI (20-bin calibration needs ~>=50 samples/occupied bin);
  - n >=  500: report logLoss point estimates, always with a user-level paired
               bootstrap 95% CI (1000 resamples; both configs scored on identical
               rows, so the comparison is paired);
  - n <   500: counts and per-row deltas only, labeled
               "insufficient power — directional at best".
  - if one user contributes >50% of pairs, additionally report per-user breakdowns.

Optional --grade-aware sensitivity (spec §3.3): a self-contained mirror of the
production grading path living entirely inside benchmarks/prod_replay/ —
grades derived from (is_correct, response_time_ms), hint_used=False:
  incorrect                     -> Again(1)
  correct, rt <  3750 ms        -> Easy(4)   (quality > 0.85 <=> speed > 0.625)
  correct, rt >= 3750 / missing -> Good(3)
Under NEW/F1 (gspSuccessGrade=4) all success grades collapse to Easy(4)
(mdm.rs:127-130), so grade-aware NEW must equal the binary NEW leg — emitted as a
consistency check. The sensitivity is therefore about the OLD leg (real grade 4:
easy bonus w[16], ΔD = −w6·(4−3)).

Usage:
  .bench-venv/bin/python benchmarks/prod_replay/run_prediction.py \
      --root ~/.wordforge-bench/wordforge_prod [--grade-aware --events prod_events.csv]
"""
from __future__ import annotations

import argparse
import json
import math
import os
import random
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Sequence

import numpy as np

_REPO_ROOT = Path(__file__).resolve().parents[2]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from benchmarks.maimemo.config import (  # noqa: E402
    DEFAULT_BENCH_ROOT,
    DEFAULT_BENCH_ROOT_ENV,
    resolve_bench_root,
)
from benchmarks.maimemo.evaluate_scheduler import (  # noqa: E402
    _build_scheduler,
    _predict_p_recall,
    _sample_test_rows,
    evaluate_scheduler_prediction,
)
from benchmarks.maimemo.models import parse_history  # noqa: E402
from benchmarks.maimemo.pipeline import StreamingPredictionAccumulator  # noqa: E402
from benchmarks.prod_replay.configs import (  # noqa: E402
    NEW_MEMORY_MODEL_CONFIG,
    OLD_MEMORY_MODEL_CONFIG,
)

EPS = 1e-6
GRADE_RT_BOUNDARY_MS = 3750.0
STABILITY_CAP_DAYS = 36_500.0
BOOTSTRAP_RESAMPLES = 1000
ICI_MIN_N = 1000
LOGLOSS_MIN_N = 500
INSUFFICIENT_LABEL = "insufficient power — directional at best"


# ---------------------------------------------------------------------------
# Grade-aware sensitivity path (self-contained; mirrors src/amas/memory/mdm.rs
# update_strength 113-260 semantics — never touches benchmarks/maimemo/)
# ---------------------------------------------------------------------------

def derive_grade(is_correct: int, response_time_ms: float) -> int:
    """Production grading (engine.rs:906-920 quality -> mdm.rs:113-122 bands),
    hint_used=False (not persisted, spec §1.1):
      quality = clamp(1.0*0.6 + speed*0.4, 0, 1), speed = clamp(1 - rt/10000, 0, 1)
      incorrect => quality*0.1 <= 0.04 => Again(1)
      correct   => quality > 0.85 <=> rt < 3750 => Easy(4), else Good(3)
    Missing rt falls back to Good(3) (board-frozen convention)."""
    if int(is_correct) != 1:
        return 1
    if response_time_ms is None or (isinstance(response_time_ms, float) and math.isnan(response_time_ms)):
        return 3
    return 4 if float(response_time_ms) < GRADE_RT_BOUNDARY_MS else 3


@dataclass
class GradeAwareMirror:
    """mdm.rs update_strength mirror parameterised by the actual FSRS grade.

    Same state recursion as dhp_reference.WordforgeMirrorState (board mirror,
    parity-tested against Rust) but `update()` takes the per-event grade instead
    of hard-coding the binary success grade. success_grade==4 selects the
    FSRS-6 faithful order (D first, no same-day branch) and collapses every
    success grade to Easy(4), exactly like production F1 (mdm.rs:127-130)."""

    weights: Sequence[float]
    forgetting_curve_factor: float
    forgetting_curve_decay: float
    forgetting_curve_floor: float
    alpha_scale: float
    alpha_min: float
    alpha_max: float
    streak_min_gap_days: float
    alpha_ramp_tau: float
    alpha_lapse_ramp_tau: float
    success_grade: int
    stability: float = 0.4
    difficulty: float = 5.0
    review_count: int = 0
    correct_streak: int = 0
    lapse_count: int = 0

    def recall(self, elapsed_days: float) -> float:
        power = math.pow(
            1.0 + self.forgetting_curve_factor * elapsed_days / max(self.stability, 0.01),
            self.forgetting_curve_decay,
        )
        return max(0.0, min(1.0, self.forgetting_curve_floor
                            + (1.0 - self.forgetting_curve_floor) * power))

    def update(self, recalled: int, grade: int, elapsed_days: float) -> None:
        w = self.weights
        if recalled == 1:
            if self.success_grade == 4:
                grade = 4  # production collapse: every success grade -> Easy(4)
            gap_ok = self.review_count == 0 or elapsed_days >= self.streak_min_gap_days
            if gap_ok:
                self.correct_streak += 1
        else:
            grade = 1
            self.correct_streak = 0
            self.lapse_count += 1
        base_alpha = max(self.alpha_min, min(self.alpha_max, 1.0 * self.alpha_scale))
        streak_bonus = 1.0 + min(self.correct_streak, 5) * 0.1
        alpha = max(self.alpha_min, min(self.alpha_max, base_alpha * streak_bonus))
        alpha = max(0.0, min(1.0, alpha))

        if self.review_count == 0:
            # first review: direct assignment, no alpha smoothing, no S cap
            self.stability = w[grade - 1]
            self.difficulty = max(1.0, min(10.0, w[4] - math.exp(w[5] * (grade - 1.0)) + 1.0))
        elif self.success_grade == 4:
            # FSRS-6 faithful order: D first, S uses updated D, no same-day branch
            r = self.recall(elapsed_days)
            delta_d = -w[6] * (grade - 3.0)
            d_prime = self.difficulty + delta_d * (10.0 - self.difficulty) / 9.0
            d_target = w[4] - math.exp(w[5] * (4.0 - 1.0)) + 1.0
            self.difficulty = max(1.0, min(10.0, w[7] * d_target + (1.0 - w[7]) * d_prime))
            if grade >= 2:
                hard_penalty = w[15] if grade == 2 else 1.0
                easy_bonus = w[16] if grade == 4 else 1.0
                s_inc = max(
                    math.exp(w[8]) * (11.0 - self.difficulty)
                    * math.pow(max(self.stability, 0.01), -w[9])
                    * (math.exp(w[10] * (1.0 - r)) - 1.0) * hard_penalty * easy_bonus,
                    0.0,
                )
                self.stability = max(0.01, min(STABILITY_CAP_DAYS, self.stability * (s_inc + 1.0)))
            else:
                self.stability = max(0.01, min(
                    self.stability,
                    w[11] * math.pow(self.difficulty, -w[12])
                    * (math.pow(self.stability + 1.0, w[13]) - 1.0)
                    * math.exp(w[14] * (1.0 - r)),
                ))
        else:
            # legacy mdm.rs order: targets from prev D/S, then joint alpha smoothing
            r = self.recall(elapsed_days)
            prev_stability = max(self.stability, 0.01)
            prev_difficulty = self.difficulty

            delta_d = -w[6] * (grade - 3.0)
            d_prime = prev_difficulty + delta_d * (10.0 - prev_difficulty) / 9.0
            d0_4 = max(1.0, min(10.0, w[4] - math.exp(w[5] * 3.0) + 1.0))
            target_difficulty = max(1.0, min(10.0, w[7] * d0_4 + (1.0 - w[7]) * d_prime))

            if elapsed_days < 1.0:
                exponent = max(-20.0, min(20.0, w[17] * (float(grade) - 3.0 + w[18])))
                s_short = prev_stability * math.exp(exponent)
                if len(w) >= 21:
                    s_short *= math.pow(prev_stability, -w[19])
                target_stability = max(s_short, 0.01)
                if grade >= 3:
                    target_stability = max(target_stability, prev_stability)
            elif grade >= 2:
                if grade == 2:
                    bonus = w[15]
                elif grade == 4:
                    bonus = w[16]
                else:
                    bonus = 1.0
                s_inc = max(
                    math.exp(w[8]) * (11.0 - prev_difficulty)
                    * math.pow(prev_stability, -w[9])
                    * (math.exp(w[10] * (1.0 - r)) - 1.0) * bonus,
                    0.0,
                )
                target_stability = max(prev_stability * (s_inc + 1.0), 0.01)
            else:
                target_stability = max(0.01, min(
                    prev_stability,
                    w[11] * math.pow(prev_difficulty, -w[12])
                    * (math.pow(prev_stability + 1.0, w[13]) - 1.0)
                    * math.exp(w[14] * (1.0 - r)),
                ))

            if grade >= 2:
                if self.alpha_ramp_tau > 0.0:
                    k = float(max(self.correct_streak, 1))
                    alpha = 1.0 - (1.0 - alpha) * math.exp(-(k - 1.0) / self.alpha_ramp_tau)
            elif self.alpha_lapse_ramp_tau > 0.0:
                f = float(max(self.lapse_count, 1))
                alpha = 1.0 - (1.0 - alpha) * math.exp(-(f - 1.0) / self.alpha_lapse_ramp_tau)

            self.difficulty = max(1.0, min(
                10.0, prev_difficulty + (target_difficulty - prev_difficulty) * alpha))
            self.stability = max(0.01, min(
                STABILITY_CAP_DAYS,
                prev_stability + (target_stability - prev_stability) * alpha))
        self.review_count += 1


def _grade_mirror_from_config(cfg: Dict[str, object]) -> GradeAwareMirror:
    w = list(cfg["w"])
    if len(w) >= 21:
        decay = max(0.05, min(2.0, float(w[20])))
        factor = math.pow(0.9, -1.0 / decay) - 1.0
        decay_e = -decay
    else:
        factor = float(cfg.get("forgettingCurveFactor", 19.0 / 81.0))
        decay_e = float(cfg.get("forgettingCurveDecay", -0.5))
    sg = int(cfg.get("gspSuccessGrade", 3))
    return GradeAwareMirror(
        weights=w,
        forgetting_curve_factor=factor,
        forgetting_curve_decay=decay_e,
        forgetting_curve_floor=float(cfg.get("forgettingCurveFloor", 0.0)),
        alpha_scale=float(cfg.get("alphaScale", 0.3)),
        alpha_min=float(cfg.get("alphaMin", 0.1)),
        alpha_max=float(cfg.get("alphaMax", 0.5)),
        streak_min_gap_days=float(cfg.get("streakMinGapMs", 1_800_000)) / 86_400_000.0,
        alpha_ramp_tau=float(cfg.get("alphaRampTau", 0.0)),
        alpha_lapse_ramp_tau=float(cfg.get("alphaLapseRampTau", 0.0)),
        success_grade=sg if sg in (3, 4) else 3,
    )


# ---------------------------------------------------------------------------
# Per-row scoring + power gating
# ---------------------------------------------------------------------------

@dataclass
class RowScores:
    users: List[str] = field(default_factory=list)
    truths: List[int] = field(default_factory=list)
    p_old: List[float] = field(default_factory=list)
    p_new: List[float] = field(default_factory=list)


def _clip(p: float) -> float:
    return max(EPS, min(1.0 - EPS, p))


def _logloss(truth: int, p: float) -> float:
    return -(math.log(p) if truth == 1 else math.log(1.0 - p))


def _score_rows_binary(df, seed: int) -> RowScores:
    """Per-row OLD/NEW predictions through the unmodified harness pieces
    (_build_scheduler + warm_start + _predict_p_recall), needed for the paired
    user-level bootstrap which the aggregate harness call cannot provide."""
    out = RowScores()
    for row in df.itertuples(index=False):
        t_hist = parse_history(row.t_history)
        r_hist = parse_history(row.r_history)
        diff_raw = row.difficulty
        diff = float(diff_raw) if not (isinstance(diff_raw, float) and math.isnan(diff_raw)) else 5.0
        elapsed = float(row.next_t) if row.next_t is not None else 0.0
        truth = int(row.next_r)
        ps = []
        try:
            for cfg in (OLD_MEMORY_MODEL_CONFIG, NEW_MEMORY_MODEL_CONFIG):
                sched = _build_scheduler("amas", None, seed, memory_config=cfg)
                sched.warm_start(t_hist, r_hist, diff)
                ps.append(_clip(_predict_p_recall(sched, elapsed)))
        except Exception:
            continue
        out.users.append(str(row.user_id))
        out.truths.append(truth)
        out.p_old.append(ps[0])
        out.p_new.append(ps[1])
    return out


def _paired_user_bootstrap(scores: RowScores, n_resamples: int, seed: int) -> Dict[str, float]:
    """User-level paired bootstrap of mean per-row logLoss delta (new - old)."""
    by_user: Dict[str, List[int]] = {}
    for i, u in enumerate(scores.users):
        by_user.setdefault(u, []).append(i)
    users = sorted(by_user)
    ll_old = np.array([_logloss(t, p) for t, p in zip(scores.truths, scores.p_old)])
    ll_new = np.array([_logloss(t, p) for t, p in zip(scores.truths, scores.p_new)])
    rng = random.Random(seed)
    deltas = []
    for _ in range(n_resamples):
        idx: List[int] = []
        for _ in range(len(users)):
            idx.extend(by_user[users[rng.randrange(len(users))]])
        a = np.asarray(idx, dtype=int)
        deltas.append(float(ll_new[a].mean() - ll_old[a].mean()))
    deltas_arr = np.asarray(deltas)
    return {
        "n_resamples": n_resamples,
        "delta_logloss_mean": float(deltas_arr.mean()),
        "ci95_low": float(np.percentile(deltas_arr, 2.5)),
        "ci95_high": float(np.percentile(deltas_arr, 97.5)),
    }


def _grade_aware_aggregates(scores: RowScores, leg: str, stabilities: List[float]) -> Dict[str, float]:
    acc = StreamingPredictionAccumulator()
    ps = scores.p_old if leg == "old" else scores.p_new
    y = np.asarray(scores.truths, dtype=float)
    p = np.asarray(ps, dtype=float)
    hl = np.maximum(1e-6, np.asarray(stabilities, dtype=float))
    elapsed_dummy = hl  # SMAPE placeholder convention, not reported
    acc.update(y, p, hl, elapsed_dummy)
    m = acc.finalize()
    return {k: float(m[k]) for k in ("logLoss", "ici", "auc", "maeP")} | {"n_samples": int(acc.count)}


def _power_gated_section(
    scores: RowScores,
    agg_old: Dict[str, float],
    agg_new: Dict[str, float],
    bootstrap_seed: int,
) -> Dict[str, object]:
    n = len(scores.truths)
    ll_old = [_logloss(t, p) for t, p in zip(scores.truths, scores.p_old)]
    ll_new = [_logloss(t, p) for t, p in zip(scores.truths, scores.p_new)]
    delta = np.asarray(ll_new) - np.asarray(ll_old)

    user_counts: Dict[str, int] = {}
    for u in scores.users:
        user_counts[u] = user_counts.get(u, 0) + 1
    max_share = (max(user_counts.values()) / n) if n else 0.0

    tier = "full" if n >= ICI_MIN_N else ("logloss_only" if n >= LOGLOSS_MIN_N else "insufficient")
    section: Dict[str, object] = {
        "n_samples": n,
        "n_users": len(user_counts),
        "max_user_share": round(max_share, 4),
        "power_tier": tier,
    }
    if n:
        section["per_row_delta_logloss_new_minus_old"] = {
            "mean": float(delta.mean()),
            "median": float(np.median(delta)),
            "p5": float(np.percentile(delta, 5)),
            "p95": float(np.percentile(delta, 95)),
            "n_new_better": int((delta < 0).sum()),
            "n_old_better": int((delta > 0).sum()),
            "n_tied": int((delta == 0).sum()),
        }
    if tier == "insufficient":
        section["label"] = INSUFFICIENT_LABEL
        section["note"] = (
            f"n_samples < {LOGLOSS_MIN_N}: logLoss/ICI point estimates withheld per "
            f"pre-registered gating; counts and per-row deltas only"
        )
    else:
        section["logloss"] = {
            "old": agg_old["logLoss"],
            "new": agg_new["logLoss"],
            "delta_new_minus_old": agg_new["logLoss"] - agg_old["logLoss"],
        }
        section["paired_user_bootstrap_95ci"] = _paired_user_bootstrap(
            scores, BOOTSTRAP_RESAMPLES, bootstrap_seed
        )
        if tier == "full":
            section["ici"] = {"old": agg_old["ici"], "new": agg_new["ici"]}
        else:
            section["note"] = f"n_samples < {ICI_MIN_N}: ICI withheld per pre-registered gating"
    if max_share > 0.5 and n:
        per_user = {}
        for u in sorted(user_counts):
            idx = [i for i, uu in enumerate(scores.users) if uu == u]
            per_user[u] = {
                "n": len(idx),
                "mean_logloss_old": float(np.mean([ll_old[i] for i in idx])),
                "mean_logloss_new": float(np.mean([ll_new[i] for i in idx])),
            }
        section["per_user_breakdown"] = per_user
    return section


# ---------------------------------------------------------------------------
# Grade-aware leg
# ---------------------------------------------------------------------------

def _score_rows_grade_aware(events_csv: Path) -> tuple[RowScores, Dict[str, object], List[float]]:
    from benchmarks.prod_replay.build_parquet import load_event_groups

    groups = load_event_groups(events_csv)
    out = RowScores()
    stabilities_new: List[float] = []
    grade_counts = {1: 0, 3: 0, 4: 0}
    n_missing_rt = 0
    for (user_id, _word_id), events in sorted(groups.items()):
        if len(events) < 2:
            continue
        deltas = [0.0]
        for i in range(1, len(events)):
            deltas.append(float(int((events[i]["ts"] - events[i - 1]["ts"]) // 86_400.0)))
        next_t = max(0.0, deltas[-1])
        truth = int(events[-1]["is_correct"])
        history = events[:-1]
        ps = []
        s_new = 0.0
        for cfg in (OLD_MEMORY_MODEL_CONFIG, NEW_MEMORY_MODEL_CONFIG):
            mirror = _grade_mirror_from_config(cfg)
            for i, ev in enumerate(history):
                rt = ev["response_time_ms"]
                grade = derive_grade(ev["is_correct"], rt)
                if cfg is OLD_MEMORY_MODEL_CONFIG:
                    grade_counts[grade] += 1
                    if ev["is_correct"] == 1 and (rt is None or (isinstance(rt, float) and math.isnan(rt))):
                        n_missing_rt += 1
                elapsed = deltas[i] if i > 0 else 0.0
                mirror.update(int(ev["is_correct"]), grade, elapsed)
            ps.append(_clip(mirror.recall(next_t)))
            s_new = mirror.stability
        out.users.append(str(user_id))
        out.truths.append(truth)
        out.p_old.append(ps[0])
        out.p_new.append(ps[1])
        stabilities_new.append(s_new)
    meta = {"grade_distribution_history_events": {str(k): v for k, v in grade_counts.items()},
            "n_success_events_missing_rt": n_missing_rt}
    return out, meta, stabilities_new


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--root", default=None,
                    help=f"bench root (default: ${DEFAULT_BENCH_ROOT_ENV})")
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--grade-aware", action="store_true",
                    help="also run the §3.3 grade-aware sensitivity leg")
    ap.add_argument("--events", default=None,
                    help="prod_events.csv (required with --grade-aware)")
    ap.add_argument("--out", default=None, help="write JSON report here (default stdout only)")
    args = ap.parse_args()

    if not args.root and not os.environ.get(DEFAULT_BENCH_ROOT_ENV):
        raise SystemExit(
            f"refusing to run without an explicit bench root: pass --root or set "
            f"${DEFAULT_BENCH_ROOT_ENV} (the implicit default {DEFAULT_BENCH_ROOT} "
            f"is the real maimemo benchmark root)"
        )
    if args.grade_aware and not args.events:
        raise SystemExit("--grade-aware requires --events prod_events.csv")
    paths = resolve_bench_root(args.root)

    # Headline aggregates: the unmodified harness, called twice (OLD/NEW).
    common = dict(n_users=10**9, max_words_per_user=0, seed=args.seed, split="test")
    agg_old = evaluate_scheduler_prediction(paths, "amas", memory_config=OLD_MEMORY_MODEL_CONFIG, **common)
    agg_new = evaluate_scheduler_prediction(paths, "amas", memory_config=NEW_MEMORY_MODEL_CONFIG, **common)

    # Per-row paired scoring on the identical sampled rows.
    df = _sample_test_rows(paths, n_users=10**9, max_words_per_user=0, seed=args.seed, split="test")
    scores = _score_rows_binary(df, args.seed)

    report: Dict[str, object] = {
        "root": str(paths.root),
        "configs": {"old": "git show 4091e9a^:amas_config.toml [memoryModel]",
                    "new": "DEFAULT_MEMORY_MODEL_CONFIG (F1 ship, frozen)"},
        "gating": {"ici_min_n": ICI_MIN_N, "logloss_min_n": LOGLOSS_MIN_N,
                   "bootstrap_resamples": BOOTSTRAP_RESAMPLES},
        "binary": _power_gated_section(scores, agg_old, agg_new, args.seed),
        "caveats": [
            "hint_used not persisted: replay assumes hint_used=false (spec §1.1)",
            "mirror pins interval_scale=1.0 inside alpha vs production dynamic ensemble "
            "scale — affects OLD leg only (F1 pins alpha=1); matches board methodology "
            "(commit f9d506d)",
            "integer-day deltas freeze same-day streak advancement vs production "
            "ms-precision gap (spec §3.3)",
        ],
    }

    if args.grade_aware:
        ga_scores, ga_meta, ga_stab = _score_rows_grade_aware(Path(args.events).expanduser())
        ga_old = _grade_aware_aggregates(ga_scores, "old", ga_stab)
        ga_new = _grade_aware_aggregates(ga_scores, "new", ga_stab)
        ga_section = _power_gated_section(ga_scores, ga_old, ga_new, args.seed)
        ga_section.update(ga_meta)
        # F1 collapse invariant: grade-aware NEW must equal binary NEW row-by-row.
        # Both legs emit rows sorted by (user_id, word_id) with identical >=2-event
        # filtering, so equal lengths imply 1:1 row alignment.
        if len(ga_scores.p_new) == len(scores.p_new):
            diff = max(
                (abs(a - b) for a, b in zip(ga_scores.p_new, scores.p_new)),
                default=0.0,
            )
            ga_section["consistency_new_vs_binary_max_abs_p_diff"] = float(diff)
        report["grade_aware"] = ga_section

    blob = json.dumps(report, indent=2)
    print(blob)
    if args.out:
        Path(args.out).expanduser().write_text(blob + "\n", encoding="utf-8")
        print(f"[run_prediction] wrote {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
