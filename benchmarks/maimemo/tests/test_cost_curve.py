"""T1.4 Cost-ADR：reviews@matched-retention 插值的单元测试（纯函数，不依赖 duckdb/数据集）。"""
from __future__ import annotations

from benchmarks.maimemo.cost_curve import reviews_at_matched_retention


def _curve():
    # 凸曲线：保持率升 → reviewsPerDay 升（DR 越高复习越多）。
    return [
        {"finalRecallRate": 0.72, "reviewsPerDay": 40.0},
        {"finalRecallRate": 0.80, "reviewsPerDay": 60.0},
        {"finalRecallRate": 0.90, "reviewsPerDay": 120.0},
    ]


def test_interpolates_between_points():
    # 0.85 在 (0.80,60) 与 (0.90,120) 中点 → 90
    assert abs(reviews_at_matched_retention(_curve(), 0.85) - 90.0) < 1e-6


def test_exact_anchor_returns_sample():
    assert abs(reviews_at_matched_retention(_curve(), 0.80) - 60.0) < 1e-6
    assert abs(reviews_at_matched_retention(_curve(), 0.72) - 40.0) < 1e-6


def test_out_of_range_returns_none_no_extrapolation():
    assert reviews_at_matched_retention(_curve(), 0.95) is None
    assert reviews_at_matched_retention(_curve(), 0.50) is None


def test_too_few_points_returns_none():
    assert reviews_at_matched_retention(_curve()[:1], 0.80) is None
    assert reviews_at_matched_retention([], 0.80) is None
