"""T1.4 Cost-ADR：reviewsPerDay-vs-保持率 凸曲线 + 「总复习次数@匹配保持率」干净指标。

动机（路线图 T1.4-①）：现有榜单把保持率与复习量加权混合（leaderboard policy_raw），
无法回答「在同一保持率下谁的复习量更小」。本脚本沿 desired_retention 网格各跑一次 forward-sim，
收集 (实测保持率 finalRecallRate, reviewsPerDay) 凸曲线，并把各算法插值到同一目标保持率后比
reviewsPerDay —— 这才是 Cost-ADR「达目标保持率的总复习次数最小」的可证伪口径。

⚠️ 受墨墨「离线≠真实留存」铁证：离线 reviewsPerDay 降不等于线上留存不掉，结论须经 T1.3 真实 A/B。

用法：
    python -m benchmarks.maimemo.cost_curve --dataset synthetic --strategy amas \\
        --dr-grid 0.70,0.75,0.80,0.85,0.90,0.95 --target-recall 0.85 --n-users 50
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any, Dict, List, Optional

from .bench_candidate import DATASET_ROOTS
from .config import BenchPaths
from .simulate import simulate_strategies

DEFAULT_DR_GRID = [0.70, 0.75, 0.80, 0.85, 0.90, 0.95]


def sweep_cost_curve(
    dataset: str,
    strategy: str,
    dr_grid: List[float],
    n_users: int,
    sim_days: int,
    seed: int,
    split: str,
    memory_config: Optional[dict] = None,
) -> List[Dict[str, float]]:
    """沿 desired_retention 网格各跑一次 sim，返回每点 (DR, 实测保持率, reviewsPerDay, ...)。"""
    paths = BenchPaths.from_root(Path(DATASET_ROOTS[dataset]).expanduser())
    points: List[Dict[str, float]] = []
    for dr in dr_grid:
        sim = simulate_strategies(
            paths,
            strategies=[strategy],
            n_users=n_users,
            sim_days=sim_days,
            max_words_per_user=500,
            daily_budget=200,
            desired_retention=dr,
            memory_config=memory_config,
            seed=seed,
            split=split,
        )
        strat = sim["strategies"][strategy]
        daily = strat["daily"]
        recalls = [float(d["recall_rate"]) for d in daily]
        final_recall = recalls[-1] if recalls else 0.0
        active = [d["recall_rate"] for d in daily if d["reviews_done"] > 0]
        avg_recall = float(sum(active) / len(active)) if active else 0.0
        points.append(
            {
                "desiredRetention": dr,
                "finalRecallRate": round(final_recall, 4),
                "avgRecallAtReview": round(avg_recall, 4),
                "reviewsPerDay": round(strat["total_reviews"] / max(sim_days, 1), 2),
                "totalReviews": strat["total_reviews"],
                "masteredCount": strat["mastered_count"],
                "efficiency": round(strat["efficiency"], 6),
            }
        )
    # 按实测保持率升序，便于插值与画凸曲线
    points.sort(key=lambda p: p["finalRecallRate"])
    return points


def reviews_at_matched_retention(
    points: List[Dict[str, float]],
    target_recall: float,
    recall_key: str = "finalRecallRate",
) -> Optional[float]:
    """线性插值：在 target_recall 处读 reviewsPerDay。target 超出曲线 recall 范围 → None（不外推）。

    这是 Cost-ADR 的干净指标：把各算法/各 DR 的保持率对齐到同一锚点后比复习量，floor-无关、可证伪。
    """
    if len(points) < 2:
        return None
    xs = [p[recall_key] for p in points]
    ys = [p["reviewsPerDay"] for p in points]
    lo, hi = min(xs), max(xs)
    if target_recall < lo or target_recall > hi:
        return None
    # 找包夹区间线性插值（xs 已随 finalRecallRate 升序；recall_key 同序则单调）
    for i in range(len(points) - 1):
        x0, x1 = xs[i], xs[i + 1]
        if x0 == x1:
            continue
        if (x0 <= target_recall <= x1) or (x1 <= target_recall <= x0):
            t = (target_recall - x0) / (x1 - x0)
            return round(ys[i] + t * (ys[i + 1] - ys[i]), 2)
    return None


def main() -> None:
    ap = argparse.ArgumentParser(description="T1.4 Cost-ADR cost curve + reviews@matched-retention")
    ap.add_argument("--dataset", default="synthetic", choices=list(DATASET_ROOTS.keys()))
    ap.add_argument("--strategy", default="amas")
    ap.add_argument("--dr-grid", default=",".join(str(x) for x in DEFAULT_DR_GRID),
                    help="逗号分隔的 desired_retention 网格")
    ap.add_argument("--target-recall", type=float, default=0.85,
                    help="干净指标的匹配保持率锚点（实测 finalRecallRate）")
    ap.add_argument("--n-users", type=int, default=50)
    ap.add_argument("--sim-days", type=int, default=90)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--split", default="test")
    ap.add_argument("--out", default="", help="可选：曲线 JSON 输出路径")
    args = ap.parse_args()

    dr_grid = [float(x) for x in args.dr_grid.split(",") if x.strip()]
    points = sweep_cost_curve(
        args.dataset, args.strategy, dr_grid,
        n_users=args.n_users, sim_days=args.sim_days, seed=args.seed, split=args.split,
    )
    matched = reviews_at_matched_retention(points, args.target_recall)

    print(f"\n=== Cost curve [{args.dataset}/{args.strategy}] ===")
    print(f"{'DR':>5} {'finalRecall':>12} {'avgRecall':>10} {'reviews/day':>12} {'mastered':>9}")
    for p in points:
        print(f"{p['desiredRetention']:>5.2f} {p['finalRecallRate']:>12.4f} "
              f"{p['avgRecallAtReview']:>10.4f} {p['reviewsPerDay']:>12.2f} {p['masteredCount']:>9}")
    if matched is not None:
        print(f"\n→ reviews/day @ matched retention {args.target_recall:.3f} = {matched:.2f} "
              f"(越低越好；Cost-ADR 干净指标)")
    else:
        print(f"\n→ target retention {args.target_recall:.3f} 超出曲线范围，无法插值（扩大 --dr-grid）")

    result: Dict[str, Any] = {
        "dataset": args.dataset,
        "strategy": args.strategy,
        "targetRecall": args.target_recall,
        "reviewsPerDayAtMatchedRetention": matched,
        "curve": points,
    }
    if args.out:
        Path(args.out).write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
        print(f"[cost_curve] written to {args.out}")


if __name__ == "__main__":
    main()
