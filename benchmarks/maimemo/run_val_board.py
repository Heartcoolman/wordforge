"""run_val_board.py — VAL split 版 leaderboard 输入生成器（战役选型仪器）。

与 run_leaderboard.run_dataset 同口径，唯一差异：闭环 sim 与重放 prediction
**双腿都传 split='val'**（既有 run_leaderboard 写死 test，本文件不动它，避免污染
官方 test 榜的复现路径）。指标装配完全一致：

  prediction: evaluate_scheduler_prediction(n_users=300, max_words_per_user=200,
              seed=42, split='val')
  dhp/policy: simulate_strategies(n_users=300, sim_days=90, daily_budget=200,
              max_words_per_user=500, desired_retention=0.85, seed=42, split='val')
    retentionStability = 1 − stdev(全 sim_days 天 recall_rate，样本标准差)
    reviewsPerDay      = total_reviews / sim_days

用法（每个数据集 root 单独跑）::

    .bench-venv/bin/python -m benchmarks.maimemo.run_val_board \
        --root ~/.wordforge-bench/maimemo --dataset maimemo \
        --out benchmarks/results/2026-06-12-rank1-val-board/
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


def run_dataset_val(
    root: Path,
    dataset: str,
    out_dir: Path,
    algos: List[str],
    notes: str = "",
    split: str = "val",
) -> List[Path]:
    paths = BenchPaths.from_root(root)
    out_dir.mkdir(parents=True, exist_ok=True)

    # 1) forward simulation：一次跑全部算法（共享 oracle / 用户加载），val split
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
        split=split,
    )
    sim_secs = time.time() - t0
    sim_days = int(sim["meta"]["sim_days"])

    written: List[Path] = []
    for algo in algos:
        # 2) prediction 维度（每算法独立，val split 采样）
        t1 = time.time()
        pred = evaluate_scheduler_prediction(
            paths, algo, n_users=300, max_words_per_user=200, seed=42, split=split
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
            "split": split,
            "runtime_seconds": round(time.time() - t1 + sim_secs / max(len(algos), 1), 2),
            "notes": notes,
        }
        fp = out_dir / f"{algo}__{dataset}.json"
        fp.write_text(json.dumps(result, ensure_ascii=False, indent=1), encoding="utf-8")
        written.append(fp)
        print(f"[{dataset}/{split}] {algo}: logLoss={pred['logLoss']:.4f} "
              f"mastered={strat['mastered_count']} → {fp}")
    return written


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate VAL-split leaderboard input JSONs")
    parser.add_argument("--root", required=True, help="Benchmark data root for the dataset")
    parser.add_argument("--dataset", required=True, help="Dataset label (maimemo/duolingo_hlr/synthetic)")
    parser.add_argument("--out", required=True, help="Output dir for per-(algo,dataset) JSONs")
    parser.add_argument("--split", default="val", choices=["train", "val", "test"], help="Data split (default: val)")
    parser.add_argument(
        "--algos",
        default=",".join(AVAILABLE_SCHEDULERS),
        help=f"Comma-separated algos (default: {','.join(AVAILABLE_SCHEDULERS)})",
    )
    parser.add_argument("--notes", default="", help="Notes embedded in each result JSON")
    args = parser.parse_args()

    algos = [a.strip() for a in args.algos.split(",") if a.strip()]
    run_dataset_val(Path(args.root).expanduser(), args.dataset, Path(args.out), algos, args.notes, args.split)


if __name__ == "__main__":
    main()
