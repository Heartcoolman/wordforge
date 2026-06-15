"""Unit tests for leaderboard 计算。

测试覆盖：
1. per-dataset min-max normalize 是 dataset-scoped（不跨数据集污染）
2. Borda 计数公式正确（第 1 名得 N 分）
3. final_score 三维加权 0.45 / 0.35 / 0.20
"""
from __future__ import annotations

import json
from pathlib import Path

import pandas as pd
import pytest

from benchmarks.maimemo.leaderboard import (
    W_PREDICTION,
    W_DHP,
    W_POLICY,
    _compute_dataset_rank_and_borda,
    _compute_final_score,
    _compute_raw_scores,
    _load_results,
    _normalize_per_dataset,
    aggregate_borda,
    compute_leaderboard,
    generate_reports,
)


# ---------------------------------------------------------------------------
# Fixture: 手工 JSON
# ---------------------------------------------------------------------------

def _write_json(dir_path: Path, algo: str, dataset: str, *, ll=0.5, ici=0.1, auc=0.7, em=10000.0, eff=0.1, rs=0.9, rpd=1000.0):
    """生成一个 result JSON 到 dir_path/{algo}__{dataset}.json。"""
    fp = dir_path / f"{algo}__{dataset}.json"
    fp.write_text(json.dumps({
        "scheduler": algo,
        "dataset": dataset,
        "prediction": {"logLoss": ll, "ici": ici, "auc": auc, "maeP": 0.2},
        "dhp": {
            "expectedMemoryFinal": em,
            "masteredCount": 1000,
            "totalReviews": 5000,
            "efficiency": eff,
        },
        "policy": {
            "finalRecallRate": 0.85,
            "reviewsPerDay": rpd,
            "retentionStability": rs,
        },
        "runtime_seconds": 10.0,
        "notes": "",
    }), encoding="utf-8")


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

def test_leaderboard_per_dataset_normalization(tmp_path: Path) -> None:
    """min-max normalize 必须 per-dataset 独立计算，不跨数据集污染。

    构造 2 dataset × 3 algo，每数据集内 prediction_raw 的 min/max 不同：
    - dataset A: 三 algo prediction_raw 大致 [0.2, 0.5, 0.8] → normalize [0, 0.5, 1]
    - dataset B: 三 algo prediction_raw 大致 [0.6, 0.65, 0.7] → normalize [0, 0.5, 1]

    若误用全局 normalize，B 数据集 algo3 的 score 会接近 1（与 A 同），
    但 per-dataset 下 B-algo3 score 必须独立成为 1（局部最优）。
    """
    results = tmp_path / "results"
    results.mkdir()
    # dataset A：logLoss 差异大
    _write_json(results, "a1", "dsA", ll=1.5, ici=0.5, auc=0.4)  # 差
    _write_json(results, "a2", "dsA", ll=0.5, ici=0.3, auc=0.6)  # 中
    _write_json(results, "a3", "dsA", ll=0.1, ici=0.1, auc=0.8)  # 优
    # dataset B：三 algo 都比较好，但相对差异小
    _write_json(results, "a1", "dsB", ll=0.4, ici=0.15, auc=0.55)
    _write_json(results, "a2", "dsB", ll=0.3, ici=0.1, auc=0.65)
    _write_json(results, "a3", "dsB", ll=0.2, ici=0.05, auc=0.75)

    df = _load_results(results)
    df = _compute_raw_scores(df)
    df = _normalize_per_dataset(df)

    # 检查每 dataset 内 prediction_score 都覆盖 [0, 1] 区间
    for ds in ["dsA", "dsB"]:
        sub = df[df["dataset"] == ds]
        assert sub["prediction_score"].min() == pytest.approx(0.0)
        assert sub["prediction_score"].max() == pytest.approx(1.0)

    # 验证 dsB 内 a3 即便绝对 prediction_raw 不如 dsA-a3 仍 normalize 到 1.0
    b3 = df[(df["dataset"] == "dsB") & (df["algo"] == "a3")]["prediction_score"].iloc[0]
    a3 = df[(df["dataset"] == "dsA") & (df["algo"] == "a3")]["prediction_score"].iloc[0]
    assert b3 == pytest.approx(1.0)
    assert a3 == pytest.approx(1.0)

    # 同样验证 worst：a1 在两数据集内都是 0.0
    b1 = df[(df["dataset"] == "dsB") & (df["algo"] == "a1")]["prediction_score"].iloc[0]
    a1 = df[(df["dataset"] == "dsA") & (df["algo"] == "a1")]["prediction_score"].iloc[0]
    assert b1 == pytest.approx(0.0)
    assert a1 == pytest.approx(0.0)


def test_leaderboard_borda_count(tmp_path: Path) -> None:
    """Borda 计数公式：第 i 名得 N-i+1 分。

    手工构造 3 algo × 2 dataset：
    - dataset A: a1 final=0.9, a2=0.6, a3=0.3  → 排名 1,2,3 → Borda 3,2,1
    - dataset B: a1 final=0.2, a2=0.5, a3=0.8  → 排名 3,2,1 → Borda 1,2,3

    Borda 总分：a1 = 3+1 = 4, a2 = 2+2 = 4, a3 = 1+3 = 4 → 全 tie

    验证：每 dataset 排名 1 的 algo 得 N=3 分，最差得 1 分。
    """
    # 直接构造已含 final_score 的 DF（绕过 raw 计算）
    df = pd.DataFrame([
        {"algo": "a1", "dataset": "dsA", "final_score": 0.9},
        {"algo": "a2", "dataset": "dsA", "final_score": 0.6},
        {"algo": "a3", "dataset": "dsA", "final_score": 0.3},
        {"algo": "a1", "dataset": "dsB", "final_score": 0.2},
        {"algo": "a2", "dataset": "dsB", "final_score": 0.5},
        {"algo": "a3", "dataset": "dsB", "final_score": 0.8},
    ])

    df = _compute_dataset_rank_and_borda(df)

    # 检查 dsA 排名
    dsA_a1 = df[(df["dataset"] == "dsA") & (df["algo"] == "a1")].iloc[0]
    dsA_a2 = df[(df["dataset"] == "dsA") & (df["algo"] == "a2")].iloc[0]
    dsA_a3 = df[(df["dataset"] == "dsA") & (df["algo"] == "a3")].iloc[0]
    assert dsA_a1["dataset_rank"] == 1 and dsA_a1["borda_points"] == 3
    assert dsA_a2["dataset_rank"] == 2 and dsA_a2["borda_points"] == 2
    assert dsA_a3["dataset_rank"] == 3 and dsA_a3["borda_points"] == 1

    # 检查 dsB 排名（反序）
    dsB_a1 = df[(df["dataset"] == "dsB") & (df["algo"] == "a1")].iloc[0]
    dsB_a2 = df[(df["dataset"] == "dsB") & (df["algo"] == "a2")].iloc[0]
    dsB_a3 = df[(df["dataset"] == "dsB") & (df["algo"] == "a3")].iloc[0]
    assert dsB_a1["dataset_rank"] == 3 and dsB_a1["borda_points"] == 1
    assert dsB_a2["dataset_rank"] == 2 and dsB_a2["borda_points"] == 2
    assert dsB_a3["dataset_rank"] == 1 and dsB_a3["borda_points"] == 3

    # 总分 a1=4, a2=4, a3=4 (全 tie)
    totals = df.groupby("algo")["borda_points"].sum().to_dict()
    assert totals == {"a1": 4, "a2": 4, "a3": 4}


def test_leaderboard_final_score_weight() -> None:
    """final_score = 0.45 prediction + 0.35 dhp + 0.20 policy."""
    df = pd.DataFrame([
        {"algo": "a", "dataset": "d", "prediction_score": 1.0, "dhp_score": 0.5, "policy_score": 0.0},
    ])
    out = _compute_final_score(df)
    expected = 0.45 * 1.0 + 0.35 * 0.5 + 0.20 * 0.0
    assert out["final_score"].iloc[0] == pytest.approx(expected)
    # 权重之和 = 1
    assert W_PREDICTION + W_DHP + W_POLICY == pytest.approx(1.0)


def test_leaderboard_normalize_max_eq_min(tmp_path: Path) -> None:
    """当 dataset 内 max == min（极端 tie），所有 algo 应得 0.5。"""
    results = tmp_path / "results"
    results.mkdir()
    # 同 dataset 三 algo 完全一致
    _write_json(results, "a1", "dsA", ll=0.5, ici=0.1, auc=0.7)
    _write_json(results, "a2", "dsA", ll=0.5, ici=0.1, auc=0.7)
    _write_json(results, "a3", "dsA", ll=0.5, ici=0.1, auc=0.7)

    df = _load_results(results)
    df = _compute_raw_scores(df)
    df = _normalize_per_dataset(df)

    assert (df["prediction_score"] == 0.5).all()
    assert (df["dhp_score"] == 0.5).all()
    assert (df["policy_score"] == 0.5).all()


def test_compute_leaderboard_end_to_end(tmp_path: Path) -> None:
    """compute_leaderboard pipeline 全链路。"""
    results = tmp_path / "results"
    results.mkdir()
    # 2 dataset × 2 algo
    _write_json(results, "good", "dsA", ll=0.2, ici=0.05, auc=0.85, em=20000, eff=0.15, rs=0.95, rpd=800)
    _write_json(results, "bad", "dsA", ll=1.5, ici=0.5, auc=0.5, em=5000, eff=0.03, rs=0.7, rpd=8000)
    _write_json(results, "good", "dsB", ll=0.3, ici=0.08, auc=0.8, em=15000, eff=0.12, rs=0.92, rpd=900)
    _write_json(results, "bad", "dsB", ll=1.2, ici=0.4, auc=0.55, em=4000, eff=0.04, rs=0.75, rpd=6000)

    df = compute_leaderboard(results)
    assert len(df) == 4

    # "good" 在两数据集都应该排名第 1
    for ds in ["dsA", "dsB"]:
        rank = df[(df["dataset"] == ds) & (df["algo"] == "good")]["dataset_rank"].iloc[0]
        assert rank == 1

    # aggregate_borda
    agg = aggregate_borda(df)
    assert agg.iloc[0]["algo"] == "good"
    assert agg.iloc[0]["borda_total"] == 4  # 2 dataset × N(=2) = 4
    assert agg.iloc[1]["algo"] == "bad"
    assert agg.iloc[1]["borda_total"] == 2


def test_generate_reports_writes_3_files(tmp_path: Path) -> None:
    """generate_reports 必须写 3 个 md 文件，且都非空。"""
    results = tmp_path / "results"
    results.mkdir()
    # 给 8 个 algo × 3 dataset 全 fixture，模拟真实场景
    for ds in ["maimemo", "duolingo_hlr", "synthetic"]:
        for i, algo in enumerate(["amas", "fsrs", "dhp", "leitner", "random", "sm2", "hlr", "fsrs45"]):
            _write_json(results, algo, ds, ll=0.3 + i*0.1, em=10000 + i*1000, eff=0.1 - i*0.005)

    out = tmp_path / "docs"
    df = compute_leaderboard(results)
    sizes = generate_reports(df, out)

    assert "01-leaderboard.md" in sizes
    assert "03-detailed-results.md" in sizes
    assert "04-methodology.md" in sizes
    for fname, size in sizes.items():
        assert size > 1000, f"{fname} 仅 {size} 字节，过小"

    # 文件存在性
    for fname in ["01-leaderboard.md", "03-detailed-results.md", "04-methodology.md"]:
        assert (out / fname).exists()


def test_leaderboard_logloss_cap() -> None:
    """logLoss > 2.0 应被 cap 到 2.0（影响 prediction_raw 计算）。"""
    df = pd.DataFrame([
        {"algo": "ok", "dataset": "d", "logLoss": 0.5, "ici": 0.1, "auc": 0.7,
         "expectedMemoryFinal": 1000.0, "efficiency": 0.1, "retentionStability": 0.9, "reviewsPerDay": 1000.0,
         "maeP": 0.2, "masteredCount": 100, "totalReviews": 500, "finalRecallRate": 0.8, "runtime_seconds": 1.0, "notes": ""},
        {"algo": "extreme", "dataset": "d", "logLoss": 50.0, "ici": 0.1, "auc": 0.7,
         "expectedMemoryFinal": 1000.0, "efficiency": 0.1, "retentionStability": 0.9, "reviewsPerDay": 1000.0,
         "maeP": 0.2, "masteredCount": 100, "totalReviews": 500, "finalRecallRate": 0.8, "runtime_seconds": 1.0, "notes": ""},
        {"algo": "atcap", "dataset": "d", "logLoss": 2.0, "ici": 0.1, "auc": 0.7,
         "expectedMemoryFinal": 1000.0, "efficiency": 0.1, "retentionStability": 0.9, "reviewsPerDay": 1000.0,
         "maeP": 0.2, "masteredCount": 100, "totalReviews": 500, "finalRecallRate": 0.8, "runtime_seconds": 1.0, "notes": ""},
    ])
    out = _compute_raw_scores(df)
    # extreme 与 atcap 的 prediction_raw 应相同（logLoss 都被 cap 到 2.0）
    extreme = out[out["algo"] == "extreme"]["prediction_raw"].iloc[0]
    atcap = out[out["algo"] == "atcap"]["prediction_raw"].iloc[0]
    assert extreme == pytest.approx(atcap)
