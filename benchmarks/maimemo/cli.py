from __future__ import annotations

import argparse
import json
from pathlib import Path

from .config import load_memory_config, resolve_bench_root
from .data import prepare_dataset, prepare_from_local_tsvs
from .pipeline import evaluate, fit_oracle, tune, iterative_tune
from .simulate import simulate_strategies
from .schedulers import AVAILABLE_SCHEDULERS


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="MaiMemo-backed benchmark pipeline")
    parser.add_argument("--root", help="Benchmark working root")
    subparsers = parser.add_subparsers(dest="command", required=True)

    prepare_parser = subparsers.add_parser("prepare", help="Download and prepare parquet datasets")
    prepare_parser.add_argument("--root", help="Benchmark working root")
    prepare_parser.add_argument("--max-rows", type=int, default=None)
    prepare_parser.add_argument(
        "--skip-sequence-groups",
        action="store_true",
        help="Only build prefix_events.parquet for faster end-to-end benchmarking.",
    )
    prepare_parser.add_argument("--raw-tsv")
    prepare_parser.add_argument("--difficulty-tsv")
    prepare_parser.add_argument("--offset-tsv")
    prepare_parser.add_argument("--forgetting-curve-tsv")

    fit_parser = subparsers.add_parser("fit_oracle", help="Fit GRU-HLR oracle and HLR baseline")
    fit_parser.add_argument("--root", help="Benchmark working root")
    fit_parser.add_argument("--max-train-rows", type=int, default=None)
    fit_parser.add_argument("--epochs", type=int, default=5)
    fit_parser.add_argument("--device", choices=("cpu", "mps"), default=None)
    fit_parser.add_argument("--torch-threads", type=int, default=None)
    fit_parser.add_argument("--loader-workers", type=int, default=None)
    fit_parser.add_argument("--duckdb-threads", type=int, default=None)
    fit_parser.add_argument("--train-batch-size", type=int, default=None)
    fit_parser.add_argument("--calibration-batch-size", type=int, default=None)
    fit_parser.add_argument("--hidden-size", type=int, default=64)

    eval_parser = subparsers.add_parser("evaluate", help="Evaluate one memory config")
    eval_parser.add_argument("--root", help="Benchmark working root")
    eval_parser.add_argument("--config")
    eval_parser.add_argument("--split", default="val")
    eval_parser.add_argument("--user-fraction", type=float, default=1.0)
    eval_parser.add_argument("--output", default="benchmark_metrics.json")

    tune_parser = subparsers.add_parser("tune", help="Tune memory config against the benchmark")
    tune_parser.add_argument("--root", help="Benchmark working root")
    tune_parser.add_argument("--output", default="benchmark_metrics.json")
    tune_parser.add_argument(
        "--iterative",
        action="store_true",
        help="Run iterative tuning with convergence checking (default: single pass)",
    )
    tune_parser.add_argument(
        "--max-iterations",
        type=int,
        default=10,
        help="Maximum iterations for iterative tuning (default: 10)",
    )
    tune_parser.add_argument(
        "--convergence-threshold",
        type=float,
        default=0.01,
        help="Relative improvement threshold for convergence (default: 0.01 = 1%%)",
    )
    tune_parser.add_argument(
        "--convergence-patience",
        type=int,
        default=2,
        help="Iterations without improvement before stopping (default: 2)",
    )
    tune_parser.add_argument(
        "--stage1-trials",
        type=int,
        default=128,
        help="Number of Stage 1 trials (default: 128, increase to 256 for later iterations)",
    )

    sim_parser = subparsers.add_parser(
        "simulate",
        help="Forward simulation: compare scheduling algorithms on real user data",
    )
    sim_parser.add_argument("--root", help="Benchmark working root")
    sim_parser.add_argument(
        "--strategies",
        default=",".join(AVAILABLE_SCHEDULERS),
        help=f"Comma-separated list of strategies to compare. "
             f"Available: {', '.join(AVAILABLE_SCHEDULERS)}. "
             f"Default: all four.",
    )
    sim_parser.add_argument(
        "--n-users",
        type=int,
        default=200,
        help="Number of users to sample from the test split (-1 = all). Default: 200.",
    )
    sim_parser.add_argument(
        "--sim-days",
        type=int,
        default=90,
        help="Number of days to simulate. Default: 90.",
    )
    sim_parser.add_argument(
        "--max-words-per-user",
        type=int,
        default=500,
        help="Maximum number of words per user. Default: 500.",
    )
    sim_parser.add_argument(
        "--daily-budget",
        type=int,
        default=200,
        help="Maximum reviews per user per day. Default: 200.",
    )
    sim_parser.add_argument(
        "--desired-retention",
        type=float,
        default=0.85,
        help="Target recall probability used by schedulers. Default: 0.85.",
    )
    sim_parser.add_argument(
        "--config",
        default=None,
        help="Optional path to a JSON memory-model config for the FSRS strategy.",
    )
    sim_parser.add_argument(
        "--oracle-batch-size",
        type=int,
        default=4096,
        help="Batch size for GRU Oracle inference. Default: 4096.",
    )
    sim_parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Global random seed. Default: 42.",
    )
    sim_parser.add_argument(
        "--duckdb-threads",
        type=int,
        default=4,
        help="DuckDB thread count. Default: 4.",
    )
    sim_parser.add_argument(
        "--output",
        default="simulation_summary.json",
        help="Output filename (relative to reports dir). Default: simulation_summary.json.",
    )

    leaderboard_parser = subparsers.add_parser(
        "leaderboard",
        help="Compute leaderboard from benchmark results and emit Markdown reports.",
    )
    leaderboard_parser.add_argument(
        "--results",
        default="benchmarks/results/2026-05-26",
        type=Path,
        help="Directory containing per-(algo,dataset) JSON results.",
    )
    leaderboard_parser.add_argument(
        "--out",
        default="docs/algo-bench-2026-05-26",
        type=Path,
        help="Output directory for 01-leaderboard.md / 03-detailed-results.md / 04-methodology.md.",
    )

    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()

    # leaderboard 不依赖 bench root（只读 results JSON + 写 docs）
    if args.command == "leaderboard":
        from .leaderboard import compute_leaderboard, generate_reports
        results_dir = Path(args.results).expanduser().resolve()
        out_dir = Path(args.out).expanduser().resolve()
        df = compute_leaderboard(results_dir)
        sizes = generate_reports(df, out_dir)
        summary = {
            "results_dir": str(results_dir),
            "out_dir": str(out_dir),
            "rows": len(df),
            "files": sizes,
        }
        print(json.dumps(summary, indent=2, ensure_ascii=False))
        return

    paths = resolve_bench_root(args.root)

    if args.command == "prepare":
        if args.raw_tsv and args.difficulty_tsv and args.offset_tsv:
            summary = prepare_from_local_tsvs(
                paths=paths,
                raw_tsv=Path(args.raw_tsv).expanduser().resolve(),
                difficulty_tsv=Path(args.difficulty_tsv).expanduser().resolve(),
                offset_tsv=Path(args.offset_tsv).expanduser().resolve(),
                forgetting_curve_tsv=Path(args.forgetting_curve_tsv).expanduser().resolve()
                if args.forgetting_curve_tsv
                else None,
                max_rows=args.max_rows,
                build_sequence_groups=not args.skip_sequence_groups,
            )
        else:
            summary = prepare_dataset(
                paths,
                max_rows=args.max_rows,
                build_sequence_groups=not args.skip_sequence_groups,
            )
        print(json.dumps(summary, indent=2, ensure_ascii=False))
        return

    if args.command == "fit_oracle":
        summary = fit_oracle(
            paths,
            max_train_rows=args.max_train_rows,
            epochs=args.epochs,
            device=args.device,
            torch_threads=args.torch_threads,
            loader_workers=args.loader_workers,
            duckdb_threads=args.duckdb_threads,
            train_batch_size=args.train_batch_size,
            calibration_batch_size=args.calibration_batch_size,
            hidden_size=args.hidden_size,
        )
        print(json.dumps(summary, indent=2, ensure_ascii=False))
        return

    if args.command == "evaluate":
        payload = evaluate(
            paths,
            memory_config=load_memory_config(args.config),
            split=args.split,
            user_fraction=args.user_fraction,
        )
        output_path = paths.reports / args.output
        output_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
        print(json.dumps(payload, indent=2, ensure_ascii=False))
        return

    if args.command == "tune":
        if args.iterative:
            payload = iterative_tune(
                paths,
                max_iterations=args.max_iterations,
                convergence_threshold=args.convergence_threshold,
                convergence_patience=args.convergence_patience,
                stage1_trials=args.stage1_trials,
                verbose=True,
            )
        else:
            payload = tune(paths, stage1_trials=args.stage1_trials)
        output_path = paths.reports / args.output
        output_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
        print(json.dumps(payload, indent=2, ensure_ascii=False))
        return

    if args.command == "simulate":
        strategies = [s.strip() for s in args.strategies.split(",") if s.strip()]
        memory_config = load_memory_config(args.config) if args.config else None
        payload = simulate_strategies(
            paths=paths,
            strategies=strategies,
            n_users=args.n_users,
            sim_days=args.sim_days,
            max_words_per_user=args.max_words_per_user,
            daily_budget=args.daily_budget,
            desired_retention=args.desired_retention,
            memory_config=memory_config,
            oracle_batch_size=args.oracle_batch_size,
            seed=args.seed,
            duckdb_threads=args.duckdb_threads,
        )
        output_path = paths.reports / args.output
        output_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
        print(json.dumps(payload, indent=2, ensure_ascii=False))
        return

if __name__ == "__main__":
    main()
