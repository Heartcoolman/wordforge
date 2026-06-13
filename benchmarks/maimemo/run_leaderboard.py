"""run_leaderboard.py — 一键生成 leaderboard 输入并计算排名。

历史背景：v0.5~v0.9 的 per-(algo,dataset) 结果 JSON 由临时脚本生成（未入仓），
本模块把 runner 固化下来。指标口径与 2026-05-29 v0.9 对齐（经 results JSON 反推验证）：

  prediction: evaluate_scheduler_prediction(n_users=300, max_words_per_user=200, seed=42)
  dhp:        simulate_strategies(n_users=300, sim_days=90, daily_budget=200,
              max_words_per_user=500, desired_retention=0.85, seed=42) 的
              {expected_memory_final, mastered_count, total_reviews, efficiency}
  policy:
    finalRecallRate    = daily[-1].recall_rate（最后一天复习时召回率）
    reviewsPerDay      = total_reviews / sim_days
    retentionStability = 1 − stdev(全 sim_days 天 recall_rate，样本标准差)
                          （v0.9 对照误差 < 0.0007，源于 daily JSON round(4)）

用法（每个数据集 root 单独跑）::

    .bench-venv/bin/python -m benchmarks.maimemo.run_leaderboard \
        --root ~/.wordforge-bench/maimemo --dataset maimemo \
        --out benchmarks/results/<date>/
    # 三数据集都生成后：
    .bench-venv/bin/python -m benchmarks.maimemo leaderboard \
        --results benchmarks/results/<date> --out docs/algo-bench-<date>/
"""
from __future__ import annotations

import argparse
import json
import statistics
import time
from pathlib import Path
from typing import Any, Dict, List

from .config import BenchPaths
from .evaluate_scheduler import evaluate_scheduler_prediction
from .schedulers import AVAILABLE_SCHEDULERS
from .simulate import simulate_strategies


def run_dataset(
    root: Path,
    dataset: str,
    out_dir: Path,
    algos: List[str],
    notes: str = "",
    exclude_users: list[str] | None = None,
) -> List[Path]:
    paths = BenchPaths.from_root(root)
    out_dir.mkdir(parents=True, exist_ok=True)

    # 1) forward simulation：一次跑全部算法（共享 oracle / 用户加载）
    t0 = time.time()
    sim = simulate_strategies(
        paths,
        strategies=algos,
        n_users=300,
        sim_days=90,
        max_words_per_user=500,
        daily_budget=200,
        desired_retention=0.85,
        seed=42,
        exclude_users=exclude_users,
    )
    sim_secs = time.time() - t0
    sim_days = int(sim["meta"]["sim_days"])

    written: List[Path] = []
    for algo in algos:
        # 2) prediction 维度（每算法独立，test split 采样）
        t1 = time.time()
        pred = evaluate_scheduler_prediction(
            paths, algo, n_users=300, max_words_per_user=200, seed=42,
            exclude_users=exclude_users,
        )
        strat = sim["strategies"][algo]
        daily = strat["daily"]
        recalls_all = [float(d["recall_rate"]) for d in daily]
        result: Dict[str, Any] = {
            "scheduler": algo,
            "dataset": dataset,
            "prediction": {
                "logLoss": pred["logLoss"],
                "ici": pred["ici"],
                "auc": pred["auc"],
                "maeP": pred["maeP"],
            },
            "dhp": {
                "expectedMemoryFinal": strat["expected_memory_final"],
                "masteredCount": strat["mastered_count"],
                "totalReviews": strat["total_reviews"],
                "efficiency": strat["efficiency"],
            },
            "policy": {
                "finalRecallRate": recalls_all[-1] if recalls_all else 0.0,
                "reviewsPerDay": strat["total_reviews"] / max(sim_days, 1),
                "retentionStability": (
                    1.0 - statistics.stdev(recalls_all) if len(recalls_all) > 1 else 1.0
                ),
            },
            "runtime_seconds": round(time.time() - t1 + sim_secs / max(len(algos), 1), 2),
            "notes": notes,
        }
        fp = out_dir / f"{algo}__{dataset}.json"
        fp.write_text(json.dumps(result, ensure_ascii=False, indent=1), encoding="utf-8")
        written.append(fp)
        print(f"[{dataset}] {algo}: logLoss={pred['logLoss']:.4f} "
              f"mastered={strat['mastered_count']} → {fp}")
    return written


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate leaderboard input JSONs")
    parser.add_argument("--root", required=True, help="Benchmark data root for the dataset")
    parser.add_argument("--dataset", required=True, help="Dataset label (maimemo/duolingo_hlr/synthetic)")
    parser.add_argument("--out", required=True, help="Output dir for per-(algo,dataset) JSONs")
    parser.add_argument(
        "--algos",
        default=",".join(AVAILABLE_SCHEDULERS),
        help=f"Comma-separated algos (default: {','.join(AVAILABLE_SCHEDULERS)})",
    )
    parser.add_argument("--notes", default="", help="Notes embedded in each result JSON")
    parser.add_argument(
        "--exclude-users",
        default=None,
        help="newline-delimited user ids excluded from sampling (algorithm-neutral)",
    )
    args = parser.parse_args()

    algos = [a.strip() for a in args.algos.split(",") if a.strip()]
    exclude_users: list[str] | None = None
    if args.exclude_users:
        exclude_users = [
            line.strip()
            for line in Path(args.exclude_users).read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
    run_dataset(
        Path(args.root).expanduser(), args.dataset, Path(args.out), algos, args.notes,
        exclude_users=exclude_users,
    )


if __name__ == "__main__":
    main()
