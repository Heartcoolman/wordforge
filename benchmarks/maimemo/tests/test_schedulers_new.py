"""新增 scheduler 单元测试：SM2 / HLR / FSRS-4.5。

每个 scheduler 至少 3 个测试用例：
  1. test_<name>_warm_start_fresh — 空历史 warm_start 后 interval ≥ 1
  2. test_<name>_recall_increases_interval — 连续召回 interval 单调非降
  3. test_<name>_lapse_resets — 失败后 interval 显著下降（< 之前 / 2）
"""
from __future__ import annotations

import pytest

from benchmarks.maimemo.schedulers import (
    SM2Scheduler,
    HLRScheduler,
    FSRS45Scheduler,
    FSRS6Scheduler,
    AMAS6Scheduler,
)


# ---------------------------------------------------------------------------
# SM2Scheduler
# ---------------------------------------------------------------------------

def test_sm2_warm_start_fresh() -> None:
    """空历史 warm_start 后，next_interval_days ≥ 1。"""
    sch = SM2Scheduler()
    sch.warm_start([], [], 0.5)
    assert sch.next_interval_days() >= 1


def test_sm2_recall_increases_interval() -> None:
    """5 次连续 recalled=1 后，interval 序列单调非降。"""
    sch = SM2Scheduler()
    sch.warm_start([], [], 0.5)
    intervals: list[int] = [sch.next_interval_days()]
    for _ in range(5):
        sch.update(recalled=1, elapsed_days=float(intervals[-1]))
        intervals.append(sch.next_interval_days())
    for prev, curr in zip(intervals, intervals[1:]):
        assert curr >= prev, f"interval 非单调非降: {intervals}"


def test_sm2_lapse_resets() -> None:
    """5 次召回后 1 次失败，interval 应显著下降（< 之前 / 2）。"""
    sch = SM2Scheduler()
    sch.warm_start([], [], 0.5)
    for _ in range(5):
        sch.update(recalled=1, elapsed_days=float(sch.next_interval_days()))
    before_lapse = sch.next_interval_days()
    sch.update(recalled=0, elapsed_days=float(before_lapse))
    after_lapse = sch.next_interval_days()
    assert after_lapse < before_lapse / 2, (
        f"失败后 interval 未明显下降: {before_lapse} → {after_lapse}"
    )


# ---------------------------------------------------------------------------
# HLRScheduler — Half-Life Regression (Duolingo 2016)
# ---------------------------------------------------------------------------

def test_hlr_warm_start_fresh() -> None:
    """空历史 warm_start 后，next_interval_days ≥ 1。"""
    sch = HLRScheduler()
    sch.warm_start([], [], 0.5)
    assert sch.next_interval_days() >= 1


def test_hlr_recall_increases_interval() -> None:
    """5 次连续 recalled=1 后，interval 序列单调非降。"""
    sch = HLRScheduler()
    sch.warm_start([], [], 0.5)
    intervals: list[int] = [sch.next_interval_days()]
    for _ in range(5):
        sch.update(recalled=1, elapsed_days=float(intervals[-1]))
        intervals.append(sch.next_interval_days())
    for prev, curr in zip(intervals, intervals[1:]):
        assert curr >= prev, f"interval 非单调非降: {intervals}"


def test_hlr_lapse_resets() -> None:
    """5 次召回后 1 次失败，halflife 应下降（验证内部 h，不依赖 round 后的 interval）。

    paper 原值 θ=(0.5,-1.0,-0.3) 在 5 review 小样本上 h 仅在 0.6~1.1 天波动，
    round(h) 全部 = 1，所以断言用 current_halflife() 而非 next_interval_days()。
    """
    sch = HLRScheduler()
    sch.warm_start([], [], 0.5)
    for _ in range(5):
        sch.update(recalled=1, elapsed_days=float(sch.next_interval_days()))
    h_before = sch.current_halflife()
    sch.update(recalled=0, elapsed_days=h_before)
    h_after = sch.current_halflife()
    assert h_after < h_before, (
        f"失败后 halflife 未下降: {h_before:.3f} → {h_after:.3f}"
    )


# ---------------------------------------------------------------------------
# FSRS45Scheduler — FSRS-4.5
# ---------------------------------------------------------------------------

def test_fsrs45_warm_start_fresh() -> None:
    """空历史 warm_start 后，next_interval_days ≥ 1。"""
    sch = FSRS45Scheduler()
    sch.warm_start([], [], 0.5)
    assert sch.next_interval_days() >= 1


def test_fsrs45_recall_increases_interval() -> None:
    """5 次连续 recalled=1 后，interval 序列单调非降。"""
    sch = FSRS45Scheduler()
    sch.warm_start([], [], 0.5)
    intervals: list[int] = [sch.next_interval_days()]
    for _ in range(5):
        sch.update(recalled=1, elapsed_days=float(intervals[-1]))
        intervals.append(sch.next_interval_days())
    for prev, curr in zip(intervals, intervals[1:]):
        assert curr >= prev, f"interval 非单调非降: {intervals}"


def test_fsrs45_lapse_resets() -> None:
    """5 次召回后 1 次失败，interval 应显著下降（< 之前 / 2）。"""
    sch = FSRS45Scheduler()
    sch.warm_start([], [], 0.5)
    for _ in range(5):
        sch.update(recalled=1, elapsed_days=float(sch.next_interval_days()))
    before_lapse = sch.next_interval_days()
    sch.update(recalled=0, elapsed_days=float(before_lapse))
    after_lapse = sch.next_interval_days()
    assert after_lapse < before_lapse / 2, (
        f"失败后 interval 未明显下降: {before_lapse} → {after_lapse}"
    )


# ---------------------------------------------------------------------------
# FSRS6Scheduler (21 维 w，trainable decay)
# ---------------------------------------------------------------------------

def test_fsrs6_warm_start_fresh() -> None:
    sch = FSRS6Scheduler()
    sch.warm_start([], [], 0.5)
    assert sch.next_interval_days() >= 1


def test_fsrs6_recall_increases_interval() -> None:
    sch = FSRS6Scheduler()
    sch.warm_start([], [], 0.5)
    intervals = []
    for _ in range(5):
        sch.update(recalled=1, elapsed_days=float(sch.next_interval_days()))
        intervals.append(sch.next_interval_days())
    for prev, curr in zip(intervals, intervals[1:]):
        assert curr >= prev, f"interval 非单调非降: {intervals}"


def test_fsrs6_lapse_resets() -> None:
    sch = FSRS6Scheduler()
    sch.warm_start([], [], 0.5)
    for _ in range(5):
        sch.update(recalled=1, elapsed_days=float(sch.next_interval_days()))
    before_lapse = sch.next_interval_days()
    sch.update(recalled=0, elapsed_days=float(before_lapse))
    after_lapse = sch.next_interval_days()
    assert after_lapse < before_lapse / 2, (
        f"失败后 interval 未明显下降: {before_lapse} → {after_lapse}"
    )


# ---------------------------------------------------------------------------
# AMAS6Scheduler (AMAS 全栈 + FSRS-6 mirror state)
# ---------------------------------------------------------------------------

def test_amas6_warm_start_fresh() -> None:
    sch = AMAS6Scheduler()
    sch.warm_start([], [], 0.5)
    assert sch.next_interval_days() >= 1


def test_amas6_recall_uses_fsrs6_curve() -> None:
    """AMAS6._recall 应 delegate 到 FSRS6MirrorState.recall（21 维 w）。"""
    sch = AMAS6Scheduler()
    sch.warm_start([], [], 0.5)
    sch.update(recalled=1, elapsed_days=1.0)
    # _recall 在 elapsed=1 时应该 ~1.0（刚复习完）
    r0 = sch._recall(0.0)
    r1 = sch._recall(sch._state.stability)
    # forgetting curve 性质：r(S,S) ≈ 0.9 (FSRS-6 约束)
    assert 0.85 < r1 < 0.95, f"R(S,S) 偏离 0.9: {r1:.3f}"
    assert r0 > r1, f"R(0) 应 > R(S): {r0:.3f} → {r1:.3f}"
