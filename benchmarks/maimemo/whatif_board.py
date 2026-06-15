"""What-if leaderboard 计算器：覆盖某算法的 raw legs 后重算整张 Borda 榜。

用途：评估「amas 条目要拿到综合 Borda 第 1，每条腿至少要到多少」的反事实分析。

设计要点：
- 评分数学**全部从** `benchmarks.maimemo.leaderboard` 导入，本文件不重实现，
  保证与官方 leaderboard 完全一致（含 per-dataset min-max normalize 的耦合效应）。
- override 改的是 *primitive* legs（masteredCount / totalReviews / expectedMemoryFinal /
  retentionStability + prediction 三腿 + finalRecallRate），派生量
  efficiency = expectedMemoryFinal / totalReviews 与 reviewsPerDay = totalReviews / 90
  由本文件重新派生，确保场景自洽（不会出现 mastered 改了但 efficiency 没动的矛盾）。
- 关键：当被改算法移动了某 dataset 的 raw min/max，归一化基准会平移，
  **竞争者的 normalized score 也会变**。这正是本工具要捕捉的耦合动态。
"""
from __future__ import annotations

import copy
import json
from pathlib import Path
from typing import Dict, Optional

import pandas as pd

from benchmarks.maimemo.leaderboard import (
    _load_results,
    _compute_raw_scores,
    _normalize_per_dataset,
    _compute_final_score,
    _compute_dataset_rank_and_borda,
    aggregate_borda,
)

# 派生常量（与 simulate.py / 官方协议一致）
SIM_DAYS = 90

# primitive legs 允许 override 的列名
PRIMITIVE_LEGS = (
    "logLoss", "ici", "auc",                 # prediction
    "masteredCount", "totalReviews", "expectedMemoryFinal",  # DHP primitives
    "retentionStability", "finalRecallRate",  # policy
    "efficiency", "reviewsPerDay",            # 允许直接覆盖派生量（但一般由派生重算）
)


def load_board(results_dir: Path) -> pd.DataFrame:
    """加载原始 results 目录为 raw legs DataFrame（未算分）。"""
    return _load_results(results_dir)


def _derive(df: pd.DataFrame) -> pd.DataFrame:
    """从 primitives 重新派生 efficiency / reviewsPerDay，保证自洽。"""
    df = df.copy()
    df["efficiency"] = df["expectedMemoryFinal"] / df["totalReviews"]
    df["reviewsPerDay"] = df["totalReviews"] / SIM_DAYS
    return df


def score_board(raw_df: pd.DataFrame) -> pd.DataFrame:
    """raw legs → 全套评分（prediction/dhp/policy raw+score → final → rank → borda）。"""
    df = _compute_raw_scores(raw_df)
    df = _normalize_per_dataset(df)
    df = _compute_final_score(df)
    df = _compute_dataset_rank_and_borda(df)
    return df


def compute_borda(raw_df: pd.DataFrame) -> pd.DataFrame:
    """raw legs → aggregate_borda 表（含 overall_rank / per-dataset rank）。"""
    return aggregate_borda(score_board(raw_df))


def whatif(
    raw_df: pd.DataFrame,
    algo: str,
    overrides: Dict[str, Dict[str, float]],
    rederive: bool = True,
) -> pd.DataFrame:
    """覆盖 `algo` 在指定 dataset 上的 raw legs，重算整张榜。

    Args:
        raw_df: 原始 raw legs（load_board 产物）。
        algo: 要改的算法名（如 'amas'）。
        overrides: {dataset: {leg: value}}，leg ∈ PRIMITIVE_LEGS。
                   覆盖 primitives 后若 rederive=True，从中重新派生 efficiency/rpd
                   （除非该派生量被显式 override）。
        rederive: 是否从 primitives 重新派生 efficiency/reviewsPerDay。

    Returns:
        aggregate_borda 表（基于覆盖后的全榜重算）。
    """
    df = raw_df.copy()
    # 把会被 override / 派生写入的列统一升为 float，避免 pandas 3.x int64 列
    # 拒绝写入小数（LossySetitemError）。
    for col in set(PRIMITIVE_LEGS):
        if col in df.columns:
            df[col] = df[col].astype(float)
    for ds, legs in overrides.items():
        mask = (df["algo"] == algo) & (df["dataset"] == ds)
        if not mask.any():
            raise ValueError(f"no row for algo={algo} dataset={ds}")
        for leg, val in legs.items():
            if leg not in PRIMITIVE_LEGS:
                raise ValueError(f"unknown leg {leg}")
            df.loc[mask, leg] = float(val)
    if rederive:
        # 仅对 primitives 改动派生；若 override 显式给了 efficiency/rpd，派生后再覆盖一次
        explicit_deriv = {
            ds: {k: v for k, v in legs.items() if k in ("efficiency", "reviewsPerDay")}
            for ds, legs in overrides.items()
        }
        df = _derive(df)
        for ds, legs in explicit_deriv.items():
            if not legs:
                continue
            mask = (df["algo"] == algo) & (df["dataset"] == ds)
            for leg, val in legs.items():
                df.loc[mask, leg] = val
    return compute_borda(df)


def legs_of(raw_df: pd.DataFrame, algo: str, dataset: str) -> Dict[str, float]:
    """提取 algo×dataset 的 primitive legs（用于「采纳竞争者腿」场景）。"""
    mask = (raw_df["algo"] == algo) & (raw_df["dataset"] == dataset)
    row = raw_df.loc[mask].iloc[0]
    return {
        "logLoss": float(row["logLoss"]),
        "ici": float(row["ici"]),
        "auc": float(row["auc"]),
        "masteredCount": float(row["masteredCount"]),
        "totalReviews": float(row["totalReviews"]),
        "expectedMemoryFinal": float(row["expectedMemoryFinal"]),
        "retentionStability": float(row["retentionStability"]),
        "finalRecallRate": float(row["finalRecallRate"]),
    }


def pred_legs_of(raw_df: pd.DataFrame, algo: str, dataset: str) -> Dict[str, float]:
    """只取 prediction 三腿。"""
    full = legs_of(raw_df, algo, dataset)
    return {k: full[k] for k in ("logLoss", "ici", "auc")}


def sim_legs_of(raw_df: pd.DataFrame, algo: str, dataset: str) -> Dict[str, float]:
    """只取 sim（DHP+policy）腿。"""
    full = legs_of(raw_df, algo, dataset)
    return {k: full[k] for k in (
        "masteredCount", "totalReviews", "expectedMemoryFinal",
        "retentionStability", "finalRecallRate",
    )}


def borda_dict(agg: pd.DataFrame) -> Dict[str, int]:
    """{algo: borda_total}。"""
    return {r["algo"]: int(r["borda_total"]) for _, r in agg.iterrows()}


def rank_dict(agg: pd.DataFrame) -> Dict[str, int]:
    """{algo: overall_rank}。"""
    return {r["algo"]: int(r["overall_rank"]) for _, r in agg.iterrows()}


def per_dataset_ranks(agg: pd.DataFrame) -> Dict[str, Dict[str, int]]:
    """{algo: {dataset: rank}}。"""
    out: Dict[str, Dict[str, int]] = {}
    rank_cols = [c for c in agg.columns if c.startswith("rank_")]
    for _, r in agg.iterrows():
        out[r["algo"]] = {c[len("rank_"):]: int(r[c]) for c in rank_cols}
    return out


def compact_table(agg: pd.DataFrame) -> list:
    """紧凑 borda 表：[{algo, borda, overall_rank, ranks:{ds:rank}}], 按 borda 降序。"""
    pds = per_dataset_ranks(agg)
    rows = []
    for _, r in agg.iterrows():
        rows.append({
            "algo": r["algo"],
            "borda": int(r["borda_total"]),
            "overall_rank": int(r["overall_rank"]),
            "final_mean": round(float(r["final_score_mean"]), 4),
            "ranks": pds[r["algo"]],
        })
    return rows


def verify_baseline(raw_df: pd.DataFrame, expected: Dict[str, int]) -> tuple:
    """校验榜单复现官方 Borda。返回 (ok, actual_dict)。"""
    agg = compute_borda(raw_df)
    actual = borda_dict(agg)
    ok = all(actual.get(k) == v for k, v in expected.items())
    return ok, actual
