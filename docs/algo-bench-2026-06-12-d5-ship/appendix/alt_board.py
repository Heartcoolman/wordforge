"""非官方口径附录脚本：DHP 维度替代加权下的 10 条目 TEST 榜重算。

官方管线（benchmarks/maimemo/leaderboard.py）唯一被替换的环节是 dhp_raw 公式；
prediction_raw / policy_raw / per-dataset min-max normalize / 0.45-0.35-0.20 加权 /
Borda 计数全部逐字复用官方模块函数，保证除 DHP 公式外零偏离。

三种口径：
  official : dhp_raw = 0.7*masteredCount + 0.3*efficiency*10000        (v0.9 现行)
  alt      : dhp_raw = 0.5*expectedMemoryFinal + 0.3*efficiency*10000 + 0.2*masteredCount
             (oracle 真值加权；0.5*expectedMemoryFinal + 0.5*efficiency*1000 的
              pre-v0.9 公式先例见 leaderboard.py:87 docstring)
  pure_em  : dhp_raw = expectedMemoryFinal                              (纯 oracle 真值)

用法（仓库根目录）：
  source .bench-venv/bin/activate
  python docs/algo-bench-2026-06-12-d5-ship/appendix/alt_board.py \
      --results benchmarks/results/2026-06-12-d5-ship
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO_ROOT))

import pandas as pd  # noqa: E402

from benchmarks.maimemo.leaderboard import (  # noqa: E402
    DHP_EFF_SCALE,
    DHP_W_EFFICIENCY,
    DHP_W_MASTERED,
    PRED_W_AUC,
    PRED_W_ICI,
    PRED_W_LOGLOSS,
    POLICY_REVIEWS_CAP,
    POLICY_W_LOW_REVIEWS,
    POLICY_W_STABILITY,
    _compute_dataset_rank_and_borda,
    _compute_final_score,
    _load_results,
    _normalize_per_dataset,
    aggregate_borda,
)

# 替代 DHP 公式权重（非官方，仅本附录）
ALT_W_EM = 0.5
ALT_W_EFF = 0.3
ALT_W_MASTERED = 0.2
ALT_EFF_SCALE = 10000.0


def _raw_scores(df: pd.DataFrame, dhp_variant: str) -> pd.DataFrame:
    """prediction_raw / policy_raw 与官方逐字一致；dhp_raw 按 variant 替换。"""
    df = df.copy()
    log_loss_norm = (df["logLoss"].clip(upper=2.0)) / 2.0
    df["prediction_raw"] = (
        PRED_W_LOGLOSS * (1.0 - log_loss_norm)
        + PRED_W_ICI * (1.0 - df["ici"])
        + PRED_W_AUC * df["auc"]
    )
    if dhp_variant == "official":
        df["dhp_raw"] = (
            DHP_W_MASTERED * df["masteredCount"]
            + DHP_W_EFFICIENCY * df["efficiency"] * DHP_EFF_SCALE
        )
    elif dhp_variant == "alt":
        df["dhp_raw"] = (
            ALT_W_EM * df["expectedMemoryFinal"]
            + ALT_W_EFF * df["efficiency"] * ALT_EFF_SCALE
            + ALT_W_MASTERED * df["masteredCount"]
        )
    elif dhp_variant == "pure_em":
        df["dhp_raw"] = df["expectedMemoryFinal"]
    else:
        raise ValueError(f"unknown dhp_variant: {dhp_variant}")
    reviews_norm = (df["reviewsPerDay"] / POLICY_REVIEWS_CAP).clip(upper=1.0)
    df["policy_raw"] = (
        POLICY_W_STABILITY * df["retentionStability"]
        + POLICY_W_LOW_REVIEWS * (1.0 - reviews_norm)
    )
    return df


def compute_board(results_dir: Path, dhp_variant: str) -> tuple[pd.DataFrame, pd.DataFrame]:
    df = _load_results(results_dir)
    df = _raw_scores(df, dhp_variant)
    df = _normalize_per_dataset(df)
    df = _compute_final_score(df)
    df = _compute_dataset_rank_and_borda(df)
    return df, aggregate_borda(df)


def board_md(agg: pd.DataFrame, df: pd.DataFrame, title: str) -> str:
    datasets = sorted(df["dataset"].unique())
    lines = [f"### {title}", ""]
    header = "| 排名 | 算法 | Borda 总分 | final_score 均值 | " + " | ".join(
        f"{ds} 排名" for ds in datasets
    ) + " |"
    lines.append(header)
    lines.append("|---|---|---|---|" + "|".join(["---"] * len(datasets)) + "|")
    for _, row in agg.iterrows():
        algo = row["algo"]
        cells = [
            str(int(row["overall_rank"])),
            f"**`{algo}`**" if algo == "amas" else f"`{algo}`",
            str(int(row["borda_total"])),
            f"{row['final_score_mean']:.3f}",
        ]
        for ds in datasets:
            cells.append(str(int(row[f"rank_{ds}"])))
        lines.append("| " + " | ".join(cells) + " |")
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--results", type=Path,
                    default=REPO_ROOT / "benchmarks/results/2026-06-12-d5-ship")
    args = ap.parse_args()

    variants = [
        ("official", "官方口径（v0.9 现行：0.7·mastered + 0.3·efficiency·10000）"),
        ("alt", "替代口径 A（非官方：0.5·expectedMemoryFinal + 0.3·efficiency·10000 + 0.2·mastered）"),
        ("pure_em", "替代口径 B（非官方：纯 expectedMemoryFinal）"),
    ]
    for key, title in variants:
        df, agg = compute_board(args.results, key)
        print(board_md(agg, df, title))


if __name__ == "__main__":
    main()
