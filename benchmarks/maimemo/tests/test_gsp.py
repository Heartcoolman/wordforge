"""GSP（Graduated Scheduling Policy）烟雾测试。

覆盖（契约见 benchmarks/maimemo/GSP_SPEC.md）：
  1. knobs-off bit-exactness —— 数百条随机 update 序列，开/不开新代码路径（GSP=off）
     的 next_interval_days 序列逐位相同（证明默认 = 冻结旧语义）。
  2. graduation floor 在连击恰好达到 k 时触发，k-1 时不触发。
  3. cap 与既有 90 天硬帽复合（min(90, cap)）。
  4. banded retention 切换点（stability 跨过 band 时目标保持率切换）。
  5. 隔离：amas6 / fsrs 不吃 GSP。
"""
from __future__ import annotations

import copy
import math
import random

from benchmarks.maimemo.config import DEFAULT_MEMORY_MODEL_CONFIG, FSRS_BASELINE_CONFIG
from benchmarks.maimemo.dhp_reference import WordforgeMirrorState
from benchmarks.maimemo.schedulers import AMAS6Scheduler, AMASScheduler, FSRSScheduler


# 候选基础态（未平滑 FSRS-6 核心 + 公版 w；与 GSP_SPEC.md §6 一致）
_FSRS6_W = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001,
    1.8722, 0.1666, 0.796, 1.4835, 0.0614, 0.2629, 1.6483, 0.6014,
    1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
]


def _candidate_base() -> dict:
    cfg = copy.deepcopy(DEFAULT_MEMORY_MODEL_CONFIG)
    cfg.update({
        "alphaScale": 1.0, "alphaMin": 1.0, "alphaMax": 1.0,
        "alphaRampTau": 0.0, "alphaLapseRampTau": 0.0,
        "w": list(_FSRS6_W),
        "forgettingCurveFactor": 0.9 ** (-1.0 / 0.1542) - 1.0,
        "forgettingCurveDecay": -0.1542,
        "forgettingCurveFloor": 0.0,
        "baseDesiredRetention": 0.85,
        "gspIntervalCapDays": 0.0,
        "gspGraduationStreak": 0,
        "gspGraduationFloorDays": 30.0,
        "gspYoungRetention": 0.0,
        "gspMatureRetention": 0.0,
        "gspMaturityBandDays": 0.0,
        "gspIntervalFuzz": 0.0,
    })
    return cfg


# ---------------------------------------------------------------------------
# 1. knobs-off bit-exactness
# ---------------------------------------------------------------------------

def _run_sequence(cfg: dict, seq: list[tuple[int, float]]) -> list[int]:
    """对一条 (recalled, elapsed) 序列逐步 update，记录每步 next_interval_days。"""
    sched = AMASScheduler(memory_config=cfg, desired_retention=0.85)
    sched.warm_start([], [], 5.0)
    out: list[int] = []
    for recalled, elapsed in seq:
        sched.update(recalled, elapsed)
        out.append(sched.next_interval_days())
    return out


def test_gsp_off_bit_exact_vs_no_gsp_keys() -> None:
    """带 gsp*=0 显式键的 config 与完全不带 gsp 键的 config，区间序列逐位相同。

    这是「默认 OFF = bit-exact legacy」的核心证据：新代码路径（GSP clamp/floor/cap/band）
    在全关时对 DEFAULT_MEMORY_MODEL_CONFIG（不含 gsp 键）零影响。
    """
    rng = random.Random(20260612)
    cfg_with_keys = _candidate_base()              # 含 gsp*=0
    cfg_no_keys = copy.deepcopy(cfg_with_keys)
    for k in ("gspIntervalCapDays", "gspGraduationStreak", "gspGraduationFloorDays",
              "gspYoungRetention", "gspMatureRetention", "gspMaturityBandDays"):
        cfg_no_keys.pop(k, None)

    n_mismatch = 0
    for _ in range(300):
        length = rng.randint(1, 25)
        seq = [(rng.randint(0, 1), float(rng.randint(0, 60))) for _ in range(length)]
        a = _run_sequence(cfg_with_keys, seq)
        b = _run_sequence(cfg_no_keys, seq)
        if a != b:
            n_mismatch += 1
    assert n_mismatch == 0, f"GSP-off 非 bit-exact：{n_mismatch}/300 条序列不一致"


def test_gsp_off_matches_default_config() -> None:
    """「缺 gsp 键」与「显式 gsp*=0 键」逐位相同——证明显式关闭 == 缺省关闭（零回归）。

    DEFAULT 已写回 v5 GSP 船值（gsp* 含 F1 激活值），故先剥离所有 gsp 键得到 legacy 缺省态作
    base（不含任何 gsp 键 → success_grade 默认 3、调度 head 不激活），再与「显式全部 gsp*=0」对比。
    """
    rng = random.Random(7)
    _gsp_keys = (
        "gspSuccessGrade", "gspIntervalCapDays", "gspGraduationStreak",
        "gspGraduationFloorDays", "gspYoungRetention", "gspMatureRetention",
        "gspMaturityBandDays", "gspIntervalFuzz",
    )
    base = copy.deepcopy(DEFAULT_MEMORY_MODEL_CONFIG)
    for k in _gsp_keys:
        base.pop(k, None)  # 缺省态：完全不带 gsp 键
    with_gsp_off = copy.deepcopy(base)
    with_gsp_off.update({
        "gspIntervalCapDays": 0.0, "gspGraduationStreak": 0,
        "gspGraduationFloorDays": 30.0, "gspYoungRetention": 0.0,
        "gspMatureRetention": 0.0, "gspMaturityBandDays": 0.0,
        "gspIntervalFuzz": 0.0,
    })
    for _ in range(300):
        length = rng.randint(1, 25)
        seq = [(rng.randint(0, 1), float(rng.randint(0, 60))) for _ in range(length)]
        assert _run_sequence(base, seq) == _run_sequence(with_gsp_off, seq)


# ---------------------------------------------------------------------------
# 2. graduation floor 在 streak == k 精确触发
# ---------------------------------------------------------------------------

def _amas_after_n_correct(cfg: dict, n: int, elapsed: float = 1.0) -> AMASScheduler:
    """warm_start 空历史后连续 n 次 recalled=1（每次 elapsed>gap → streak 递增）。"""
    sched = AMASScheduler(memory_config=cfg, desired_retention=0.85)
    sched.warm_start([], [], 5.0)
    for _ in range(n):
        sched.update(1, elapsed)
    return sched


def test_graduation_floor_triggers_exactly_at_k() -> None:
    """k=3, floor=30：streak<3 时无下限（区间可 <30），streak>=3 时下限弹到 >=30。

    取早期低 stability 词，base 区间天然 < 30，故 floor 触发与否可观测。
    """
    k = 3
    cfg = _candidate_base()
    # 本测试依赖「早期低 S → base 区间 < 30」的判别力；gspSuccessGrade=4（候选基础态默认）下
    # 首评 S0=w[3]（Easy）+ FSRS-6 faithful 增长令 S 过快越 30，故钉 grade=3 保持低 S 判别力。
    cfg.update({"gspGraduationStreak": k, "gspGraduationFloorDays": 30.0, "gspSuccessGrade": 3})

    # streak = k-1 = 2：不触发，区间应 < 30（早期低 S）
    s2 = _amas_after_n_correct(cfg, 2)
    assert s2._state.correct_streak == 2
    ivl2 = s2.next_interval_days()

    # streak = k = 3：触发，区间下限弹到 30
    s3 = _amas_after_n_correct(cfg, 3)
    assert s3._state.correct_streak == 3
    ivl3 = s3.next_interval_days()

    # 对照：同序列但 GSP 关闭，streak=3 处区间仍 < 30（证明 ivl3 的 30 来自 floor）
    cfg_off = _candidate_base()
    cfg_off["gspSuccessGrade"] = 3  # 同上：保持低 S 判别力
    s3_off = _amas_after_n_correct(cfg_off, 3)
    ivl3_off = s3_off.next_interval_days()

    assert ivl2 < 30, f"streak=2(<k) 不应被 floor 抬高：{ivl2}"
    assert ivl3 >= 30, f"streak=3(==k) 应被 floor 抬到 >=30：{ivl3}"
    assert ivl3_off < 30, f"GSP-off 时 streak=3 区间应 <30（否则测试无判别力）：{ivl3_off}"
    assert ivl3 > ivl3_off, "floor 应严格抬高区间"


def test_graduation_floor_off_when_streak_zero() -> None:
    """k=0（关闭）：任意连击都不施加 floor。"""
    cfg = _candidate_base()
    cfg.update({"gspGraduationStreak": 0, "gspGraduationFloorDays": 30.0})
    s = _amas_after_n_correct(cfg, 5)
    cfg_off = _candidate_base()
    s_off = _amas_after_n_correct(cfg_off, 5)
    assert s.next_interval_days() == s_off.next_interval_days()


# ---------------------------------------------------------------------------
# 3. cap 与 90 天硬帽复合
# ---------------------------------------------------------------------------

def test_cap_composes_with_90() -> None:
    """长历史推高 S → 区间撞 90 帽；设 cap=45 应进一步压到 <=45。"""
    # 构造一个会撞 90 帽的高 S 词：长串成功 + 大 elapsed
    long_seq = [(1, 30.0)] * 12
    cfg_off = _candidate_base()
    s_off = AMASScheduler(memory_config=cfg_off, desired_retention=0.85)
    s_off.warm_start([], [], 5.0)
    for r, e in long_seq:
        s_off.update(r, e)
    ivl_90 = s_off.next_interval_days()
    assert ivl_90 == 90, f"高 S 词应撞 90 帽，实际 {ivl_90}"

    cfg_cap = _candidate_base()
    cfg_cap.update({"gspIntervalCapDays": 45.0})
    s_cap = AMASScheduler(memory_config=cfg_cap, desired_retention=0.85)
    s_cap.warm_start([], [], 5.0)
    for r, e in long_seq:
        s_cap.update(r, e)
    ivl_cap = s_cap.next_interval_days()
    assert ivl_cap == 45, f"cap=45 应压到 45，实际 {ivl_cap}"


def test_cap_above_90_is_noop() -> None:
    """cap=120 > 90：min(90,120)=90，与既有硬帽等价（不放大）。"""
    long_seq = [(1, 30.0)] * 12
    cfg = _candidate_base()
    cfg.update({"gspIntervalCapDays": 120.0})
    s = AMASScheduler(memory_config=cfg, desired_retention=0.85)
    s.warm_start([], [], 5.0)
    for r, e in long_seq:
        s.update(r, e)
    assert s.next_interval_days() == 90


# ---------------------------------------------------------------------------
# 4. banded retention 切换点
# ---------------------------------------------------------------------------

def test_banded_retention_switch_at_band() -> None:
    """直接在 WordforgeMirrorState 上验证：stability 跨过 band 时目标保持率切换。

    young=0.95（高保持率→短间隔），mature=0.80（低保持率→长间隔）。
    band 两侧用相同 S 求间隔，验证 < band 用 young、>= band 用 mature。
    """
    band = 20.0
    common = dict(
        weights=list(_FSRS6_W),
        forgetting_curve_factor=0.9 ** (-1.0 / 0.1542) - 1.0,
        forgetting_curve_decay=-0.1542,
        forgetting_curve_floor=0.0,
        gsp_maturity_band_days=band,
        gsp_young_retention=0.95,
        gsp_mature_retention=0.80,
    )
    # S 恰在 band 下方（young 口径）
    st_young = WordforgeMirrorState(desired_retention=0.85, stability=band - 0.001, **common)
    # S 恰在 band 上方（mature 口径）—— 但 S 不同会污染 interval，故改为固定 S 比较口径函数
    st_mature = WordforgeMirrorState(desired_retention=0.85, stability=band + 0.001, **common)

    assert st_young._gsp_banded_retention() == 0.95
    assert st_mature._gsp_banded_retention() == 0.80

    # 同一 S 下，high retention(young) 间隔 < low retention(mature) 间隔。
    # 用纯函数 _interval_days_raw 在固定 S 下对比两口径（绕开 S 跨带差异）。
    st_fixed = WordforgeMirrorState(desired_retention=0.85, stability=50.0, **common)
    ivl_young = st_fixed._interval_days_raw(0.95)
    ivl_mature = st_fixed._interval_days_raw(0.80)
    assert ivl_young < ivl_mature, (
        f"高保持率应给更短间隔：young={ivl_young:.3f} mature={ivl_mature:.3f}"
    )


def test_banded_retention_off_uses_desired() -> None:
    """band=0（关闭）：_gsp_banded_retention 返回 None → 用 desired_retention。"""
    st = WordforgeMirrorState(
        weights=list(_FSRS6_W),
        desired_retention=0.85,
        forgetting_curve_factor=0.9 ** (-1.0 / 0.1542) - 1.0,
        forgetting_curve_decay=-0.1542,
        forgetting_curve_floor=0.0,
        gsp_maturity_band_days=0.0,
        gsp_young_retention=0.95,
        gsp_mature_retention=0.80,
        stability=50.0,
    )
    assert st._gsp_banded_retention() is None
    # interval_days 应等于不带任何 GSP 字段的等价 state
    st_plain = WordforgeMirrorState(
        weights=list(_FSRS6_W),
        desired_retention=0.85,
        forgetting_curve_factor=0.9 ** (-1.0 / 0.1542) - 1.0,
        forgetting_curve_decay=-0.1542,
        forgetting_curve_floor=0.0,
        stability=50.0,
    )
    assert st.interval_days() == st_plain.interval_days()


# ---------------------------------------------------------------------------
# 5. 隔离：amas6 / fsrs 不吃 GSP
# ---------------------------------------------------------------------------

def test_amas6_ignores_gsp_keys() -> None:
    """即便 config 误带激进 GSP 键，amas6 也不应被影响（硬关 + FSRS6 state 无 band 字段）。"""
    cfg = copy.deepcopy(DEFAULT_MEMORY_MODEL_CONFIG)
    cfg.update({
        "gspIntervalCapDays": 10.0, "gspGraduationStreak": 1,
        "gspGraduationFloorDays": 80.0, "gspMaturityBandDays": 5.0,
        "gspYoungRetention": 0.99, "gspMatureRetention": 0.5,
    })
    s = AMAS6Scheduler(memory_config=cfg, desired_retention=0.85)
    s.warm_start([], [], 5.0)
    for _ in range(5):
        s.update(1, 5.0)
    s_clean = AMAS6Scheduler(memory_config=DEFAULT_MEMORY_MODEL_CONFIG, desired_retention=0.85)
    s_clean.warm_start([], [], 5.0)
    for _ in range(5):
        s_clean.update(1, 5.0)
    assert s.next_interval_days() == s_clean.next_interval_days()
    assert s._gsp_interval_cap_days == 0.0 and s._gsp_graduation_streak == 0
    assert not hasattr(s._state, "gsp_maturity_band_days")


def test_fsrs_isolated_from_default_drift() -> None:
    """fsrs 默认绑 FSRS_BASELINE_CONFIG（FSRS-5 公版 w），不继承 DEFAULT（FSRS-6+AMAS）。"""
    f = FSRSScheduler()
    assert f._cfg is FSRS_BASELINE_CONFIG
    assert abs(f._cfg["w"][0] - 0.40255) < 1e-9      # FSRS-5 公版首权重
    # baseline 显式带 gsp*=0
    assert FSRS_BASELINE_CONFIG["gspIntervalCapDays"] == 0.0
    assert FSRS_BASELINE_CONFIG["gspGraduationStreak"] == 0
    assert FSRS_BASELINE_CONFIG["gspMaturityBandDays"] == 0.0


def test_success_grade_default_is_legacy_good3() -> None:
    """gspSuccessGrade 缺省 = 3（Good）= bit-exact legacy（首评 S0=w[2]）。"""
    cfg = _candidate_base()
    cfg.pop("gspSuccessGrade", None)  # 确保缺省
    s = AMASScheduler(memory_config=cfg, desired_retention=0.85)
    s.warm_start([], [], 5.0)
    s.update(1, 0.0)  # 首评成功
    # grade=3 → S0 = w[2] = 2.3065
    assert abs(s._state.stability - _FSRS6_W[2]) < 1e-9
    assert s._state.success_grade == 3


def test_success_grade4_matches_fsrs6_mirror() -> None:
    """gspSuccessGrade=4 + FSRS-6 faithful 次序 → 逐位等价 FSRS6MirrorState（amas6 公版核心）。

    这是 v5 架构「未平滑 FSRS-6 核心 = amas6 预测腿」的契约：grade=4 下 WordforgeMirrorState
    必须与 FSRS6MirrorState 在任意 (recalled, elapsed) 序列上 stability/difficulty/recall 全等价。
    """
    from benchmarks.maimemo.schedulers import FSRS6MirrorState

    cfg = _candidate_base()
    cfg["gspSuccessGrade"] = 4

    rng = random.Random(20260613)
    for _ in range(300):
        n = rng.randint(1, 25)
        seq = [(rng.randint(0, 1), float(rng.choice([0, 1, 2, 3, 7, 14, 30, 60, 90]))) for _ in range(n)]
        # 首评 elapsed 强制 0（与 warm_start/ FSRS6 一致），后续保持
        if seq:
            seq[0] = (seq[0][0], 0.0)

        s = AMASScheduler(memory_config=cfg, desired_retention=0.85)
        s.warm_start([], [], 5.0)
        f6 = FSRS6MirrorState(weights=list(_FSRS6_W), desired_retention=0.85)

        for recalled, elapsed in seq:
            s._state.update(recalled, elapsed)
            f6.update(recalled, elapsed)
            assert abs(s._state.stability - f6.stability) < 1e-9, (seq, s._state.stability, f6.stability)
            assert abs(s._state.difficulty - f6.difficulty) < 1e-9
            for t in (1.0, 10.0, 50.0):
                assert abs(s._state.recall(t) - f6.recall(t)) < 1e-9


def test_success_grade_entry_clamp() -> None:
    """成功成绩带入口 clamp：非 {3,4} 一律夹回 3（合法成功成绩域）。"""
    from benchmarks.maimemo.dhp_reference import _gsp_band_kwargs_from_config

    assert _gsp_band_kwargs_from_config({"gspSuccessGrade": 4})["success_grade"] == 4
    assert _gsp_band_kwargs_from_config({"gspSuccessGrade": 3})["success_grade"] == 3
    assert _gsp_band_kwargs_from_config({"gspSuccessGrade": 2})["success_grade"] == 3  # Hard 非二元成功语义
    assert _gsp_band_kwargs_from_config({"gspSuccessGrade": 99})["success_grade"] == 3
    assert _gsp_band_kwargs_from_config({})["success_grade"] == 3  # 缺省


# ---------------------------------------------------------------------------
# 6. gspIntervalFuzz —— 复习负载平滑（GSP_SPEC §7）
# ---------------------------------------------------------------------------

def test_fuzz_off_bit_exact() -> None:
    """gspIntervalFuzz=0（默认/关闭）与完全不带该键的 config，区间序列逐位相同。

    证明 fuzz 默认 OFF = bit-exact legacy（新代码路径在全关时零影响）。
    """
    rng = random.Random(20260613)
    cfg_fuzz0 = _candidate_base()
    cfg_fuzz0.update({"gspIntervalCapDays": 40, "gspGraduationStreak": 2,
                      "gspIntervalFuzz": 0.0})
    cfg_nokey = copy.deepcopy(cfg_fuzz0)
    cfg_nokey.pop("gspIntervalFuzz", None)

    n_mismatch = 0
    for _ in range(300):
        length = rng.randint(1, 25)
        seq = [(rng.randint(0, 1), float(rng.randint(0, 60))) for _ in range(length)]
        if _run_sequence(cfg_fuzz0, seq) != _run_sequence(cfg_nokey, seq):
            n_mismatch += 1
    assert n_mismatch == 0, f"fuzz=0 非 bit-exact：{n_mismatch}/300"


def test_fuzz_determinism_same_state_same_value() -> None:
    """确定性：同一 (stability, review_count) → 同一 u → 同一区间（无 RNG/全局态）。"""
    from benchmarks.maimemo.schedulers import AMASScheduler as _A

    # _fuzz_u 纯函数对同输入恒等
    for s, rc in [(1.5, 3), (37.2, 10), (90.0, 25), (0.4, 1), (12.34, 7)]:
        assert _A._fuzz_u(s, rc) == _A._fuzz_u(s, rc)
        u = _A._fuzz_u(s, rc)
        assert -1.0 <= u < 1.0, f"u 越界 [-1,1): {u}"

    # 整条 update 序列在两次独立运行下区间序列逐位相同
    cfg = _candidate_base()
    cfg.update({"gspIntervalCapDays": 40, "gspGraduationStreak": 2, "gspIntervalFuzz": 0.15})
    rng = random.Random(99)
    seq = [(rng.randint(0, 1), float(rng.randint(0, 60))) for _ in range(20)]
    assert _run_sequence(cfg, seq) == _run_sequence(cfg, seq)


def test_fuzz_preserves_graduated_floor() -> None:
    """毕业词 floor 不被 fuzz 向下击穿：streak>=k 的词，fuzz 后区间仍 >= floor（>=30）。

    构造大幅 fuzz（0.20），扫多组 (stability, review_count) 使 u 可能为负，
    断言所有毕业态词的 next_interval_days 恒 >= graduationFloor（mastered 定义不破）。
    """
    cfg = _candidate_base()
    cfg.update({"gspIntervalCapDays": 40, "gspGraduationStreak": 2,
                "gspGraduationFloorDays": 30.0, "gspIntervalFuzz": 0.20})

    rng = random.Random(31337)
    n_checked = 0
    for _ in range(200):
        # 连续成功推高 streak（每次 elapsed>gap），构造毕业态
        n = rng.randint(2, 12)
        sched = AMASScheduler(memory_config=cfg, desired_retention=0.85)
        sched.warm_start([], [], 5.0)
        for _ in range(n):
            sched.update(1, float(rng.choice([2, 5, 10, 20, 30])))
        if sched._state.correct_streak >= 2:  # 已毕业
            ivl = sched.next_interval_days()
            assert ivl >= 30, f"毕业词被 fuzz 拉到 floor 之下：{ivl} < 30 (streak={sched._state.correct_streak})"
            n_checked += 1
    assert n_checked > 50, f"判别力不足：仅 {n_checked} 个毕业态被检查"


def test_fuzz_respects_90_hard_cap() -> None:
    """fuzz 上推不越 90 天硬帽：高 fuzz + cap=89 的词，区间恒 <= 90。"""
    cfg = _candidate_base()
    # 显式关 retire：本测验证 fuzz 的 90 硬帽语义；V1 船值 retire(streak6) 在 14 连击下
    # 会合法返回 365（越帽是 retire 的契约行为，见 GSP_SPEC §9），与 fuzz 帽无关。
    cfg.update({"gspIntervalCapDays": 89, "gspGraduationStreak": 2, "gspIntervalFuzz": 0.20,
                "gspRetireAfterReviews": 0})
    long_seq = [(1, 30.0)] * 14
    s = AMASScheduler(memory_config=cfg, desired_retention=0.85)
    s.warm_start([], [], 5.0)
    for r, e in long_seq:
        s.update(r, e)
    ivl = s.next_interval_days()
    assert ivl <= 90, f"fuzz 上推越过 90 硬帽：{ivl}"


def test_fuzz_actually_spreads_intervals() -> None:
    """判别力：fuzz>0 时同一批词的区间分布比 fuzz=0 更分散（错峰削波的直接证据）。

    构造多组不同 (stability, review_count) 的毕业态词撞 cap=40，断言 fuzz>0 产生
    >1 个不同的区间值（fuzz=0 时它们全钉在 40）。
    """
    base = _candidate_base()
    # 显式关 retire（同上：本测验证 fuzz 分散判别力，毕业连击词被 retire 钉 365 会消灭分散性）
    base.update({"gspIntervalCapDays": 40, "gspGraduationStreak": 2,
                 "gspGraduationFloorDays": 30.0, "gspRetireAfterReviews": 0})
    cfg0 = copy.deepcopy(base); cfg0["gspIntervalFuzz"] = 0.0
    cfgF = copy.deepcopy(base); cfgF["gspIntervalFuzz"] = 0.15

    def _cap_words_intervals(cfg):
        vals = []
        rng = random.Random(2024)
        for _ in range(60):
            n = rng.randint(6, 16)
            s = AMASScheduler(memory_config=cfg, desired_retention=0.85)
            s.warm_start([], [], 5.0)
            for _ in range(n):
                s.update(1, 30.0)  # 长成功串 → 高 S → 撞 cap=40
            vals.append(s.next_interval_days())
        return vals

    v0 = _cap_words_intervals(cfg0)
    vF = _cap_words_intervals(cfgF)
    # fuzz=0：撞 cap 的词全钉 40（最多 1 个值）
    assert len(set(v0)) == 1 and v0[0] == 40, f"fuzz=0 应全钉 40，实得 {set(v0)}"
    # fuzz>0：散开到多个值
    assert len(set(vF)) > 3, f"fuzz>0 应散开区间，实得 {sorted(set(vF))}"
    # 散开范围在 [floor, fuzz_cap=min(90,40*1.15=46)] 内
    assert min(vF) >= 30 and max(vF) <= 46, f"fuzz 散布越界：{min(vF)}..{max(vF)}"


def test_amas6_ignores_fuzz() -> None:
    """amas6 公版隔离：即便 config 误带激进 fuzz，amas6 区间不变（硬关 _gsp_interval_fuzz）。"""
    cfg = copy.deepcopy(DEFAULT_MEMORY_MODEL_CONFIG)
    cfg.update({"gspIntervalFuzz": 0.20, "gspIntervalCapDays": 40, "gspGraduationStreak": 1})
    s = AMAS6Scheduler(memory_config=cfg, desired_retention=0.85)
    s.warm_start([], [], 5.0)
    for _ in range(6):
        s.update(1, 5.0)
    s_clean = AMAS6Scheduler(memory_config=DEFAULT_MEMORY_MODEL_CONFIG, desired_retention=0.85)
    s_clean.warm_start([], [], 5.0)
    for _ in range(6):
        s_clean.update(1, 5.0)
    assert s.next_interval_days() == s_clean.next_interval_days()
    assert s._gsp_interval_fuzz == 0.0
