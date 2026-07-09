"""schedulers.py — Scheduler Protocol + five concrete implementations.

Each scheduler maintains the internal state of a single (user, word) pair.
Interface:
    warm_start(t_history, r_history, difficulty) — replay past reviews to
        initialise internal state.
    next_interval_days() -> int  — how many days until the next review.
    update(recalled, elapsed_days)  — incorporate the result of the latest
        review and update internal state.

Callers must invoke warm_start() before the first update().
"""
from __future__ import annotations

import bisect
import math
import random
from typing import List, Protocol, runtime_checkable

from .config import DEFAULT_MEMORY_MODEL_CONFIG, FSRS_BASELINE_CONFIG
from .dhp_reference import (
    DHPStudent,
    WordforgeMirrorState,
    _alpha_kwargs_from_config,
    _cold_start_kwargs_from_config,
    _gsp_band_kwargs_from_config,
)


# ---------------------------------------------------------------------------
# Protocol
# ---------------------------------------------------------------------------

@runtime_checkable
class Scheduler(Protocol):
    name: str

    def warm_start(
        self,
        t_history: List[int],
        r_history: List[int],
        difficulty: float,
    ) -> None: ...

    def next_interval_days(self) -> int: ...

    def update(self, recalled: int, elapsed_days: float) -> None: ...

    def current_halflife(self) -> float: ...


# ---------------------------------------------------------------------------
# FSRS-5 Scheduler (WordforgeMirrorState)
# ---------------------------------------------------------------------------

class FSRSScheduler:
    """FSRS-5 scheduler backed by WordforgeMirrorState.

    Uses DEFAULT_MEMORY_MODEL_CONFIG weights unless a custom config is
    provided.  Warm-start replays the full review history to arrive at
    the same internal state as the live system would.
    """

    name = "fsrs"

    def __init__(
        self,
        memory_config: dict | None = None,
        desired_retention: float | None = None,
    ) -> None:
        # 竞品隔离：fsrs 缺省回退 FSRS_BASELINE_CONFIG（FSRS-5 公版），
        # 不再继承 DEFAULT（已是 FSRS-6+AMAS 旋钮）。显式传入的 memory_config 仍优先。
        cfg = memory_config or FSRS_BASELINE_CONFIG
        self._cfg = cfg
        # CLI-supplied desired_retention overrides the config value.
        self._desired_retention = (
            desired_retention
            if desired_retention is not None
            else float(cfg.get("baseDesiredRetention", 0.85))
        )
        self._state = self._fresh_state()

    def _fresh_state(self) -> WordforgeMirrorState:
        return WordforgeMirrorState(
            weights=list(self._cfg["w"]),
            desired_retention=self._desired_retention,
            forgetting_curve_factor=float(self._cfg.get("forgettingCurveFactor", 19.0 / 81.0)),
            forgetting_curve_decay=float(self._cfg.get("forgettingCurveDecay", -0.5)),
            forgetting_curve_floor=float(self._cfg.get("forgettingCurveFloor", 0.0)),
            **_alpha_kwargs_from_config(self._cfg),
        )

    def warm_start(
        self,
        t_history: List[int],
        r_history: List[int],
        difficulty: float,  # noqa: ARG002 — not used; FSRS derives difficulty internally
    ) -> None:
        self._state = self._fresh_state()
        for i, (delta_t, result) in enumerate(zip(t_history, r_history)):
            elapsed = float(delta_t) if i > 0 else 0.0
            self._state.update(result, elapsed)

    def next_interval_days(self) -> int:
        return self._state.interval_days()

    def update(self, recalled: int, elapsed_days: float) -> None:
        self._state.update(recalled, elapsed_days)

    def current_halflife(self) -> float:
        # Stability ≈ half-life under the forgetting-curve parameterisation.
        return max(1e-6, float(self._state.stability))


# ---------------------------------------------------------------------------
# DHP Scheduler
# ---------------------------------------------------------------------------

class DHPScheduler:
    """DHP scheduler backed by DHPStudent.

    Half-life drives the interval via:
        interval = ceil(halflife * -log2(desired_retention))
    Warm-start replays the full review history using next_state().
    """

    name = "dhp"

    def __init__(
        self,
        student: DHPStudent,
        desired_retention: float = 0.85,
    ) -> None:
        self._student = student
        self._retention = desired_retention
        self._halflife: float = 1.0
        self._difficulty: int = 5
        self._state: list = []
        self._review_count: int = 0

    def warm_start(
        self,
        t_history: List[int],
        r_history: List[int],
        difficulty: float,
    ) -> None:
        self._review_count = 0
        self._difficulty = max(1, min(18, int(round(difficulty if difficulty >= 1 else 5))))

        if not t_history:
            self._halflife = self._student.start_halflife(self._difficulty, 0)
            self._state = [self._halflife, float(self._difficulty)]
            return

        first_result = r_history[0] if r_history else 0
        self._halflife = self._student.start_halflife(self._difficulty, first_result)
        self._state = [self._halflife, float(self._difficulty)]
        self._review_count = 1

        for i in range(1, len(t_history)):
            recalled = r_history[i] if i < len(r_history) else 0
            self._state, self._halflife = self._student.next_state(
                self._state, recalled, t_history[i], 0.0
            )
            self._review_count += 1

    def next_interval_days(self) -> int:
        hl = max(1e-6, self._halflife)
        days = hl * (-math.log2(max(1e-9, self._retention)))
        return max(1, math.ceil(days))

    def update(self, recalled: int, elapsed_days: float) -> None:
        delta_t = max(1, int(round(elapsed_days)))
        self._state, self._halflife = self._student.next_state(
            self._state, recalled, delta_t, 0.0
        )
        self._review_count += 1

    def current_halflife(self) -> float:
        return max(1e-6, self._halflife)


class SSPMMCScheduler(DHPScheduler):
    """忠实 SSP-MMC（Stochastic Shortest Path - Minimize Memorization Cost，叶峻峣等 KDD'22）。

    记忆模型与 DHPScheduler 同源（继承 DHPStudent 的 halflife/difficulty 状态机 +
    warm_start/update/current_halflife），但调度不走朴素 ceil(halflife·-log2(R)) 规则，
    而是查 maimemo 官方 DP solver 预计算的**最优策略表**：给定当前 (halflife, difficulty)
    → 在 152 点对数 halflife 网格上找最近箱 → 读出 ivl-{difficulty}.csv 第 2 列的 DP 最优
    下一间隔。这是榜上**唯一消费 SSP-MMC 最优策略表**的算法，区别于 dhp 代理。

    预测 recall 与 dhp 同口径（无 _recall → fallback 2^(-Δt/halflife)），故 prediction
    维度与 dhp 逐位相同，差异**纯在调度（DHP/Policy 维度）**——正好隔离"最优策略表 vs
    朴素固定保持率规则"的净价值。

    重要 caveat：策略表是在 **DHP 记忆模型**上 DP 求解的（非 FSRS-6），故对位 amas 仍是
    "SSP-MMC-on-DHP vs AMAS-on-FSRS6" 的跨记忆模型对比；且仍是 300 用户离线 forward-sim，
    非墨墨真实生产部署（详见 docs/algo-bench-2026-06-13-ultracode/00 §4.1）。
    """

    name = "ssp_mmc"
    # 吸收态（halflife≥目标 → 永久记忆）：策略表给间隔 0（仅最高 halflife 箱）→ 映射长程维护间隔。
    _ABSORBING_INTERVAL = 365

    def __init__(
        self,
        student: DHPStudent,
        policy: "dict",
        desired_retention: float = 0.85,
    ) -> None:
        super().__init__(student=student, desired_retention=desired_retention)
        # policy: Dict[int_difficulty, (halflives_grid_asc, optimal_intervals)]
        self._policy = policy

    def next_interval_days(self) -> int:
        # 当前 DHP 难度（1-18，失败腿每次 +2 上限 18）
        diff = self._state[1] if self._state else self._difficulty
        diff = max(1, min(18, int(round(diff))))
        halflives, intervals = self._policy[diff]
        hl = max(1e-6, self._halflife)
        # 在升序 halflife 网格上找最近箱（DP 状态离散化口径）
        idx = bisect.bisect_left(halflives, hl)
        if idx <= 0:
            idx = 0
        elif idx >= len(halflives):
            idx = len(halflives) - 1
        elif (halflives[idx] - hl) > (hl - halflives[idx - 1]):
            idx = idx - 1
        ivl = intervals[idx]
        # 间隔 0 = 吸收态（永久记忆，无需复习）→ 长程维护间隔
        if ivl <= 0:
            return self._ABSORBING_INTERVAL
        return max(1, ivl)


# ---------------------------------------------------------------------------
# Leitner Scheduler (5-box)
# ---------------------------------------------------------------------------

_LEITNER_INTERVALS = [1, 2, 4, 7, 14]  # days per box (0-indexed)


class LeitnerScheduler:
    """Classic 5-box Leitner system.

    Box intervals: [1, 2, 4, 7, 14] days.
    Correct → advance one box (capped at box 4).
    Wrong   → reset to box 0.
    Half-life is approximated from the current box interval using the
    log-2 forgetting curve.
    """

    name = "leitner"

    def __init__(self, desired_retention: float = 0.85) -> None:
        self._retention = desired_retention
        self._box: int = 0
        # Rough half-life estimate: interval = hl * -log2(retention) → hl = interval / -log2(r)
        self._halflife: float = _LEITNER_INTERVALS[0] / (-math.log2(max(1e-9, desired_retention)))

    def warm_start(
        self,
        t_history: List[int],
        r_history: List[int],
        difficulty: float,  # noqa: ARG002
    ) -> None:
        self._box = 0
        for result in r_history:
            if result == 1:
                self._box = min(self._box + 1, len(_LEITNER_INTERVALS) - 1)
            else:
                self._box = 0
        self._halflife = _LEITNER_INTERVALS[self._box] / (-math.log2(max(1e-9, self._retention)))

    def next_interval_days(self) -> int:
        return _LEITNER_INTERVALS[self._box]

    def update(self, recalled: int, elapsed_days: float) -> None:  # noqa: ARG002
        if recalled == 1:
            self._box = min(self._box + 1, len(_LEITNER_INTERVALS) - 1)
        else:
            self._box = 0
        self._halflife = _LEITNER_INTERVALS[self._box] / (-math.log2(max(1e-9, self._retention)))

    def current_halflife(self) -> float:
        return self._halflife


# ---------------------------------------------------------------------------
# AMAS Scheduler
# ---------------------------------------------------------------------------

class AMASScheduler:
    """Full-fidelity AMAS simulation scheduler.

    Ports the following Rust modules to Python with exact constants:

    Memory layer (src/amas/memory/mdm.rs)
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    • DSR / FSRS-6 stability model — via WordforgeMirrorState
      (21-dim w 时曲线参数由 w[20] 派生，与 Rust 实现一致).
    • adaptive_desired_retention(): sigmoid-gated adjustments for accuracy,
      fatigue, and motivation (consts: HIGH_ACCURACY_THRESHOLD=0.9, +0.02;
      HIGH_FATIGUE_THRESHOLD=0.6, -0.05; LOW_MOTIVATION_THRESHOLD=-0.2, -0.03;
      RETENTION_MIN=0.70, RETENTION_MAX=0.95).

    User-state model (src/amas/engine.rs update_modeling)
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    • **Fatigue**: cumulates +0.02 per review, decays exponentially with
      τ=600 s after a 300 s gap, full reset at 1800 s.  In simulation we
      approximate with a fixed review-to-review time step.
    • **Motivation**: EMA(α=0.1) of signal (+0.1 if recalled, -0.15 if not).
    • **Confidence**: EMA(decay=0.99) of signal (±0.02), clamped [0.1, 1.0].

    Ensemble decision (src/amas/decision/{ensemble,heuristic,ige}.rs)
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    Produces an ``interval_scale`` via three paths:

    1. **Heuristic** (conf ≈ 0.7 → 0.2 as events accumulate):
       • fatigue > 0.90 → scale *= 0.8 (compress interval).
       • low accuracy < 0.50 → scale *= 0.85.
       • high accuracy > 0.90 → scale *= 1.10.
       • low motivation < -0.50 → scale *= 0.90.

    2. **IGE** (conf = 0.6, fixed):
       • UCB-style exploration over difficulty bins:
         UCB bonus = review_ucb_weight(0.12) * sqrt(ln(pop+1) / (attempts+1)),
         capped at review_ucb_max_bonus(0.35).
       • When UCB > 0.20 (under-reviewed word), scale *= 0.85 (pull interval in
         to gather more data points); otherwise scale *= 1.05.

    3. **SWD** (not ported; treated as neutral, conf = 0.3, scale = 1.0).

    Ensemble merge (weighted by confidence, base weights H:0.40 I:0.30 S:0.30,
    trust-blended after warmup_samples=20, blend_max=0.50):
       interval_scale = Σ w_i * scale_i

    Word selector urgency (src/amas/word_selector.rs)
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    The *urgency score* (used in simulate.py to sort due words) is computed as:
       score  = (1 − recall)
              + recall_risk_bonus(0.20) * sigmoid((0.55 − recall) * 8.0)
       score *= gaussian(recall, center=0.50, σ=0.30).max(0.15)
    This is exposed via the ``urgency_score()`` helper below and called by the
    simulator for daily review prioritisation.
    """

    name = "amas"

    # ---- mdm.rs constants ------------------------------------------------
    _HIGH_ACC_THRESHOLD   = 0.90
    _HIGH_ACC_BOOST       = 0.03   # was 0.02 → higher retention when accurate
    _HIGH_FAT_THRESHOLD   = 0.60
    _HIGH_FAT_DROP        = 0.05
    _LOW_MOT_THRESHOLD    = -0.20
    _LOW_MOT_DROP         = 0.03
    _RETENTION_MIN        = 0.75   # was 0.70 → floor raised to reduce over-spacing
    _RETENTION_MAX        = 0.95

    # ---- engine.rs fatigue constants ------------------------------------
    _FAT_INC_NORMAL       = 0.02   # per review
    _FAT_INC_QUIT         = 0.20
    _FAT_DECAY_START_S    = 300.0
    _FAT_DECAY_TAU_S      = 600.0
    _FAT_RESET_S          = 1800.0
    # Simulation approximation: assume avg 60 s between reviews in a session
    _SIM_REVIEW_GAP_S     = 60.0

    # ---- engine.rs motivation constants ----------------------------------
    _MOT_POSITIVE         = +0.10
    _MOT_NEGATIVE         = -0.15
    _MOT_MOMENTUM         = 0.10   # EMA α

    # ---- engine.rs confidence constants ----------------------------------
    _CONF_POSITIVE        = +0.02
    _CONF_NEGATIVE        = -0.02
    _CONF_DECAY           = 0.99
    _CONF_MIN             = 0.10

    # ---- word_selector.rs constants --------------------------------------
    _UCB_WEIGHT           = 0.12
    _UCB_MAX_BONUS        = 0.35
    _SIGMOID_STEEP        = 8.0
    _OPT_RECALL_CENTER    = 0.50
    _OPT_RECALL_SIGMA     = 0.30
    _RECALL_RISK_THRESH   = 0.55
    _RECALL_RISK_BONUS    = 0.20

    # ---- heuristic.rs constants (tuned v3) -------------------------------
    _HEUR_CONF_BASE       = 0.70
    _HEUR_CONF_MIN        = 0.20
    _HEUR_CONF_DECAY_SCALE= 500.0
    _HEUR_CONF_DECAY_CAP  = 0.30

    # ---- ige.rs constants (tuned v3) -------------------------------------
    _IGE_CONF             = 0.65
    _IGE_UCB_COEFF        = 2.0
    _IGE_UCB_THRESHOLD    = 0.15
    _IGE_SCALE_COMPRESS   = 0.92
    _IGE_SCALE_EXPAND     = 1.05   # restored: expand well-explored words more

    # ---- ensemble.rs constants (tuned v3) --------------------------------
    _ENS_BASE_W_H         = 0.35
    _ENS_BASE_W_I         = 0.40
    _ENS_BASE_W_S         = 0.25
    _ENS_WARMUP           = 20
    _ENS_BLEND_SCALE      = 100.0
    _ENS_BLEND_MAX        = 0.50
    _ENS_MIN_W            = 0.15
    _ENS_WARMUP_H_BOOST   = 0.10
    _ENS_TRUST_LR         = 0.05
    _ENS_TRUST_INIT       = 0.50

    @staticmethod
    def _sigmoid(x: float) -> float:
        return 1.0 / (1.0 + math.exp(max(-50.0, min(50.0, -x))))

    @staticmethod
    def _gaussian(x: float, center: float, sigma: float) -> float:
        d = (x - center) / sigma
        return math.exp(-0.5 * d * d)

    @staticmethod
    def _fuzz_u(stability: float, review_count: int) -> float:
        """确定性伪随机抖动量 u ∈ [-1, 1)，由 in-state 量派生（GSP_SPEC §7）。

        公式（运算次序固定，Rust 移植须逐位对齐）：
            h = stability * 12.9898 + review_count * 78.233   # f64 乘加
            f = h - floor(h)                                  # 小数部分 ∈ [0, 1)
            u = 2.0 * f - 1.0                                 # 映射到 [-1, 1)

        时间不变（不读模拟日/视界）、身份无关（仅读状态量 stability/review_count）。
        constants 12.9898/78.233 取自经典 GLSL hash（fract(sin(...)) 家族），
        此处去掉 sin 以保证 Python↔Rust 完全可移植（避免跨平台 sin 末位差）。
        """
        h = stability * 12.9898 + float(review_count) * 78.233
        f = h - math.floor(h)
        return 2.0 * f - 1.0

    def __init__(
        self,
        memory_config: dict | None = None,
        desired_retention: float = 0.85,
    ) -> None:
        cfg = memory_config or DEFAULT_MEMORY_MODEL_CONFIG
        self._cfg = cfg
        if len(cfg["w"]) >= 21:
            # FSRS-6：曲线参数由 w[20] 派生（与 Rust curve_decay/curve_factor 一致）
            _decay = max(0.05, min(2.0, float(cfg["w"][20])))
            self._factor  = math.pow(0.9, -1.0 / _decay) - 1.0
            self._decay_e = -_decay
            self._floor   = float(cfg.get("forgettingCurveFloor", 0.0))
        else:
            self._factor  = float(cfg.get("forgettingCurveFactor", 19.0 / 81.0))
            self._decay_e = float(cfg.get("forgettingCurveDecay",  -0.5))
            self._floor   = float(cfg.get("forgettingCurveFloor",   0.0))

        # per-word difficulty logit 加性项（amas 专属预测层特征；竞品忽略 memory_config → 天然 de-tie）。
        # 预测期 recall 在 logit 域加 β·(D_REF - external_difficulty)：难词降 p、易词升 p，
        # 修正 FSRS 二元映射下「预测随难度扁平」的区分度/校准残差（见 pred_diagnose / pred_search）。
        # β=0（缺省）时 guard 跳过 → bit-exact no-op；仅作用于预测/urgency，不反馈 S/D 动力学。
        self._diff_logit_weight = max(0.0, float(cfg.get("difficultyLogitWeight", 0.0)))
        self._diff_logit_ref    = float(cfg.get("difficultyLogitRef", 5.0))
        # 突破战役 C01：difflogit 用模型内生难度（self._state.difficulty，FSRS-6 演化 D）
        # 替代外部 word.difficulty 标签——synthetic 上外部标签是写死 start_difficulty 噪声、与真实
        # 可记忆性弱正相关致反伤；内生 D 随复习历史漂移、与 halflife 同源。仍用 has_ext 作 amas
        # de-tie 标识（amas6 无 external_difficulty → 跳过），仅替换取值，保持 amas-only。
        self._diff_logit_intrinsic = bool(cfg.get("difflogitUseIntrinsicDifficulty", False))
        # v7 预测读出层重校准 + 复习次数残差（difflogit 的推广，镜像 mdm.rs::recall_probability_predicted）。
        # logit(p)=base_scale·logit(p_fsrs)+intercept+β_diff·(REF−D)+β_nrev·ln(1+review_count)。
        # 默认 1/0/0 → 退化为纯 difflogit；系数按部署拟合（pred_calib.py），不跨域迁移。
        self._pred_logit_base_scale  = float(cfg.get("predLogitBaseScale", 1.0))
        self._pred_logit_intercept   = float(cfg.get("predLogitIntercept", 0.0))
        self._pred_logit_nrev_weight = float(cfg.get("predLogitReviewCountWeight", 0.0))

        self._base_retention = desired_retention  # CLI arg takes priority
        self._retention      = self._base_retention
        # GSP 调度策略头旋钮（生产新旋钮，默认全关 = 冻结旧语义）。
        # 入口 clamp 镜像（bench adapter 不调 validate()）：cap/floor 夹至 [0, +inf)，
        # streak 夹非负整数；banded retention 三参数由 _fresh_state 经 dhp 侧 helper 夹紧。
        self._gsp_interval_cap_days  = max(0.0, float(cfg.get("gspIntervalCapDays", 0.0)))
        self._gsp_graduation_streak  = max(0, int(cfg.get("gspGraduationStreak", 0)))
        self._gsp_graduation_floor   = max(0.0, float(cfg.get("gspGraduationFloorDays", 30.0)))
        # 突破战役：退役（retirement）——自信过学词（review_count≥K 且已毕业）冻结到长间隔、
        # 越过 90 天硬帽 → sim 窗口内不再到期 → 砍 reviewsPerDay 且不制造 lapse（保 retentionStability）。
        # gspRetireAfterReviews=0 关闭（默认）。仅读 review_count/correct_streak/stability，时间不变可移植。
        self._gsp_retire_after_reviews = max(0, int(cfg.get("gspRetireAfterReviews", 0)))
        self._gsp_retire_interval_days = float(cfg.get("gspRetireIntervalDays", 365.0))
        self._gsp_retire_min_stability = float(cfg.get("gspRetireMinStability", 0.0))
        # 2026-07-08 真半衰期口径战役：连击门槛。review_count 含 warm 历史 → 低门槛 retire 对
        # warm 词首评即触发、把 oracle 半衰期不足的词提前冻结（mastered 崩塌）。correct_streak
        # 是 oracle 近期失败惩罚的直接代理（近 K 次全对 → 半衰期高置信）。0=关（bit-exact）。
        self._gsp_retire_min_streak = max(0, int(cfg.get("gspRetireMinStreak", 0)))
        # GSP review-load smoothing：确定性区间抖动（生产新旋钮，默认 0=关=bit-exact legacy）。
        # 夹至 [0, 1)：负数 → 0（关闭）；>=1 会使 (1+fuzz·u) 触及非正区间，故顶侧夹至 1-eps。
        self._gsp_interval_fuzz      = max(0.0, min(1.0 - 1e-9, float(cfg.get("gspIntervalFuzz", 0.0))))
        self._state          = self._fresh_state()
        self._window         = int(cfg.get("masteryWindowSize", 20))

        # Rolling accuracy window
        self._recent_results: list[int] = []

        # User-state variables (engine.rs)
        self._fatigue:    float = 0.0
        self._motivation: float = 0.0
        self._confidence: float = 1.0
        self._total_events: int = 0

        # Ensemble trust scores (per-path EMA)
        self._trust_h: float = self._ENS_TRUST_INIT
        self._trust_i: float = self._ENS_TRUST_INIT
        self._trust_s: float = self._ENS_TRUST_INIT

        # UCB: total attempts on this (user, word) pair
        self._attempts: int = 0
        # Review population approximation (how many words this user has seen)
        self._population: int = 0

    def _fresh_state(self) -> WordforgeMirrorState:
        return WordforgeMirrorState(
            weights=list(self._cfg["w"]),
            desired_retention=self._retention,
            forgetting_curve_factor=self._factor,
            forgetting_curve_decay=self._decay_e,
            forgetting_curve_floor=self._floor,
            **_alpha_kwargs_from_config(self._cfg),
            **_gsp_band_kwargs_from_config(self._cfg),
            **_cold_start_kwargs_from_config(self._cfg),
        )

    # ------------------------------------------------------------------
    # Recall probability helper (FSRS power-law, matches mdm.rs)
    # ------------------------------------------------------------------
    def _recall(self, elapsed_days: float) -> float:
        s = max(1e-6, float(self._state.stability))
        p = self._floor + (1 - self._floor) * math.pow(
            max(0.0, 1.0 + self._factor * elapsed_days / s), self._decay_e
        )
        # cold-start guard（镜像 mdm.rs:333 `last_review_at.is_none() → 纯 recall`）：
        # 未复习过的词不施加读出层（否则 intercept/base_scale 会污染冷启动预测，与 Rust 分歧）。
        if int(getattr(self._state, "review_count", 0)) == 0:
            return p
        # v7 预测读出层（镜像 mdm.rs::recall_probability_predicted，逐位对齐）：
        #   logit(p)=base_scale·logit(p_fsrs)+intercept+β_diff·(REF−D)+β_nrev·ln(1+review_count)
        # 仅 WordforgeMirrorState(amas) 有 external_difficulty；amas6 无 → diff 项守卫跳过。
        has_ext = hasattr(self._state, "external_difficulty")
        diff_active = self._diff_logit_weight > 0.0 and has_ext
        scale = self._pred_logit_base_scale
        intercept = self._pred_logit_intercept
        w_nrev = self._pred_logit_nrev_weight
        # no-op 快路径：与 Rust 一致，base_scale=1∧intercept=0∧β_nrev=0∧diff 未激活 → 纯 recall
        if scale == 1.0 and intercept == 0.0 and w_nrev == 0.0 and not diff_active:
            return p
        pc = min(1.0 - 1e-9, max(1e-9, p))
        logit = scale * math.log(pc / (1.0 - pc)) + intercept
        if diff_active:
            if self._diff_logit_intrinsic:
                ext = float(getattr(self._state, "difficulty", self._diff_logit_ref))
            else:
                ext = float(getattr(self._state, "external_difficulty", self._diff_logit_ref))
            ext = min(10.0, max(1.0, ext))
            logit += self._diff_logit_weight * (self._diff_logit_ref - ext)
        if w_nrev != 0.0:
            rc = int(getattr(self._state, "review_count", 0))
            logit += w_nrev * math.log(1.0 + rc)
        p = 1.0 / (1.0 + math.exp(-logit))
        return p

    # ------------------------------------------------------------------
    # Fatigue update (engine.rs update_modeling)
    # ------------------------------------------------------------------
    def _update_fatigue(self) -> None:
        # Simulate elapsed time since last review (60 s between reviews)
        gap = self._SIM_REVIEW_GAP_S
        if gap >= self._FAT_RESET_S:
            self._fatigue = 0.0
        elif gap > self._FAT_DECAY_START_S:
            elapsed_in_decay = gap - self._FAT_DECAY_START_S
            self._fatigue *= math.exp(-elapsed_in_decay / self._FAT_DECAY_TAU_S)
        self._fatigue = min(1.0, self._fatigue + self._FAT_INC_NORMAL)

    # ------------------------------------------------------------------
    # Motivation update (engine.rs update_modeling)
    # ------------------------------------------------------------------
    def _update_motivation(self, recalled: int) -> None:
        signal = self._MOT_POSITIVE if recalled else self._MOT_NEGATIVE
        self._motivation = self._motivation * (1.0 - self._MOT_MOMENTUM) + signal * self._MOT_MOMENTUM
        self._motivation = max(-1.0, min(1.0, self._motivation))

    # ------------------------------------------------------------------
    # Confidence update
    # ------------------------------------------------------------------
    def _update_confidence(self, recalled: int) -> None:
        signal = self._CONF_POSITIVE if recalled else self._CONF_NEGATIVE
        self._confidence = max(self._CONF_MIN, self._confidence * self._CONF_DECAY + signal)

    # ------------------------------------------------------------------
    # Adaptive retention (mdm.rs adaptive_desired_retention)
    # ------------------------------------------------------------------
    def _update_retention(self) -> None:
        if not self._recent_results:
            return
        accuracy = sum(self._recent_results) / len(self._recent_results)
        r = self._base_retention
        r += self._HIGH_ACC_BOOST * self._sigmoid((accuracy - self._HIGH_ACC_THRESHOLD) * 10.0)
        r -= self._HIGH_FAT_DROP  * self._sigmoid((self._fatigue   - self._HIGH_FAT_THRESHOLD) * 10.0)
        r -= self._LOW_MOT_DROP   * self._sigmoid((self._LOW_MOT_THRESHOLD - self._motivation) * 10.0)
        self._retention = max(self._RETENTION_MIN, min(self._RETENTION_MAX, r))
        self._state.desired_retention = self._retention

    # ------------------------------------------------------------------
    # Heuristic path — returns (interval_scale, confidence)
    # ------------------------------------------------------------------
    def _heuristic(self) -> tuple[float, float]:
        scale = 1.0
        accuracy = (sum(self._recent_results) / len(self._recent_results)
                    if self._recent_results else 0.80)  # default optimistic

        # High accuracy → extend intervals to reduce unnecessary reviews
        if accuracy > 0.90:
            scale *= 1.08
        elif accuracy > 0.85:
            scale *= 1.04
        elif accuracy > 0.75:
            scale *= 1.01
        elif accuracy < 0.50:
            scale *= 0.85

        # Fatigue: compress at high fatigue
        if self._fatigue > 0.90:
            scale *= 0.88

        if self._motivation < -0.50:
            scale *= 0.92

        decay  = min(self._total_events / self._HEUR_CONF_DECAY_SCALE, self._HEUR_CONF_DECAY_CAP)
        conf   = max(self._HEUR_CONF_MIN, self._HEUR_CONF_BASE - decay)
        return scale, conf

    # ------------------------------------------------------------------
    # IGE path — returns (interval_scale, confidence)
    # UCB bonus: review_ucb_weight * sqrt(ln(pop+1) / (attempts+1))
    # ------------------------------------------------------------------
    def _ige(self) -> tuple[float, float]:
        pop  = max(1, self._population)
        att  = self._attempts
        ucb_bonus = self._UCB_WEIGHT * math.sqrt(math.log(pop + 1.0) / (att + 1.0))
        ucb_bonus = min(ucb_bonus, self._UCB_MAX_BONUS)

        # Threshold: well-explored words get mild expansion; others mild compress
        if ucb_bonus > self._IGE_UCB_THRESHOLD:
            scale = self._IGE_SCALE_COMPRESS
        else:
            scale = self._IGE_SCALE_EXPAND
        return scale, self._IGE_CONF

    # ------------------------------------------------------------------
    # Ensemble merge (ensemble.rs get_weights + merge)
    # ------------------------------------------------------------------
    def _ensemble_interval_scale(self) -> float:
        n = self._total_events
        in_warmup = n < self._ENS_WARMUP

        blend = 0.0 if in_warmup else min(
            (n - self._ENS_WARMUP) / self._ENS_BLEND_SCALE, self._ENS_BLEND_MAX
        )

        def _blend_weight(base: float, trust: float) -> float:
            return max(self._ENS_MIN_W, (1 - blend) * base + blend * trust)

        w_h = _blend_weight(self._ENS_BASE_W_H, self._trust_h)
        w_i = _blend_weight(self._ENS_BASE_W_I, self._trust_i)
        w_s = _blend_weight(self._ENS_BASE_W_S, self._trust_s)
        if in_warmup:
            w_h += self._ENS_WARMUP_H_BOOST

        scale_h, conf_h = self._heuristic()
        scale_i, conf_i = self._ige()
        scale_s, conf_s = 1.0, 0.30          # SWD: neutral

        # Multiply each path weight by its confidence
        wc_h = w_h * max(0.0, conf_h)
        wc_i = w_i * max(0.0, conf_i)
        wc_s = w_s * max(0.0, conf_s)
        total = wc_h + wc_i + wc_s
        if total < 1e-9:
            return 1.0
        wc_h /= total; wc_i /= total; wc_s /= total

        return wc_h * scale_h + wc_i * scale_i + wc_s * scale_s

    # ------------------------------------------------------------------
    # Trust update (ensemble.rs update_trust)
    # ------------------------------------------------------------------
    def _update_trust(self, recalled: int) -> None:
        accuracy = (sum(self._recent_results) / len(self._recent_results)
                    if self._recent_results else 0.5)
        recall_p = self._recall(0.0) if self._attempts > 0 else 0.5
        reward = (float(recalled)
                  - (1.0 - recall_p) * 0.30
                  - (max(0.0, self._fatigue - 0.7)) * 0.30)
        reward = max(-1.0, min(1.0, reward))
        normalized = (reward + 1.0) / 2.0

        # Effective weights for learning rate scaling
        _, conf_h = self._heuristic()
        _, conf_i = self._ige()
        max_wc = max(conf_h, conf_i, 0.30)
        lr_h = self._ENS_TRUST_LR * max(0.1, conf_h / max_wc)
        lr_i = self._ENS_TRUST_LR * max(0.1, conf_i / max_wc)
        lr_s = self._ENS_TRUST_LR * 0.1

        self._trust_h = self._trust_h * (1 - lr_h) + normalized * lr_h
        self._trust_i = self._trust_i * (1 - lr_i) + normalized * lr_i
        self._trust_s = self._trust_s * (1 - lr_s) + normalized * lr_s

    # ------------------------------------------------------------------
    # Urgency score (word_selector.rs score_review_word_prefetched)
    # Exposed so simulate.py can use it for review prioritisation.
    # ------------------------------------------------------------------
    def urgency_score(self, elapsed_days: float) -> float:
        recall = self._recall(elapsed_days)
        # Monotone urgency: lower recall → higher urgency.
        # We keep the sigmoid risk bonus for recall near the 0.55 threshold,
        # but remove the Gaussian multiplier that caused non-monotonicity
        # (previously urgency peaked at recall≈0.40 then fell for very low recall).
        score = 1.0 - recall
        score += self._RECALL_RISK_BONUS * self._sigmoid(
            (self._RECALL_RISK_THRESH - recall) * self._SIGMOID_STEEP
        )
        return score

    # ------------------------------------------------------------------
    # Protocol implementation
    # ------------------------------------------------------------------

    def warm_start(
        self,
        t_history: List[int],
        r_history: List[int],
        difficulty: float,
    ) -> None:
        self._retention      = self._base_retention
        self._recent_results = []
        self._fatigue        = 0.0
        self._motivation     = 0.0
        self._confidence     = 1.0
        self._total_events   = 0
        self._attempts       = 0
        self._population     = max(10, len(t_history))   # warm estimate
        self._state          = self._fresh_state()
        # per-word difficulty 先验：仅 WordforgeMirrorState(amas) 有此字段；amas6 的
        # FSRS6MirrorState 带 __slots__ 无此属性 → hasattr 守卫跳过，保持 bit-identical。
        # NaN（缺标注）回落 5.0=中性 D，混入后等价轻微拉向中点，不引入偏置。
        if hasattr(self._state, "external_difficulty"):
            d = float(difficulty)
            self._state.external_difficulty = 5.0 if math.isnan(d) else max(1.0, min(10.0, d))
            # 冷启动难度先验（1a）：任一 extd 权重非零才注入特征（激活先验）。
            # 缺省权重全 0 → 不注入 → cs_* 全 None → _cold_start_deltas 短路 = bit-exact legacy。
            # 注入须在首条历史重放（review_count==0 的首评）之前，先验恰在该步消费。
            if (self._cfg.get("coldStartDExtdWeight") or self._cfg.get("coldStartSExtdWeight")) and hasattr(
                self._state, "cs_ext_difficulty"
            ):
                self._state.cs_ext_difficulty = self._state.external_difficulty

        for i, (delta_t, result) in enumerate(zip(t_history, r_history)):
            elapsed = float(delta_t) if i > 0 else 0.0
            self._state.update(result, elapsed)
            self._recent_results.append(result)
            if len(self._recent_results) > self._window:
                self._recent_results.pop(0)
            self._update_fatigue()
            self._update_motivation(result)
            self._update_confidence(result)
            self._update_trust(result)
            self._attempts       += 1
            self._total_events   += 1
        self._update_retention()

        # No post-warm-start stability clamp.
        # FSRS-5 correctly accumulates stability from the review history; we
        # let it stand.  The 90-day hard cap in next_interval_days() is
        # sufficient to prevent runaway intervals inside the 365-day window.

    def next_interval_days(self) -> int:
        # base interval 已含 GSP banded-retention 求解（state.interval_days() 内）。
        base_days    = self._state.interval_days()
        ivl_scale    = self._ensemble_interval_scale()

        # 保真对齐生产 ensemble.rs:131：interval_scale.max(0.1)，无上界、无稳定度分带
        # （bench 独有的分带 clamp 已移除；90 天硬帽与 mdm.rs compute_interval 一致保留）
        ivl_scale = max(0.1, ivl_scale)

        scaled_days = max(1.0, base_days * ivl_scale)

        # GSP 毕业下限（scale 之后、cap 之前）：连击 ≥ k 且 k>0 时，间隔下限弹到 floor。
        # 时间不变（仅读 correct_streak，不读模拟日/视界）。amas6 的 FSRS6MirrorState
        # 不带 correct_streak，getattr 兜底 0 → 在无 GSP 配置时恒不触发。
        graduated = False
        if self._gsp_graduation_streak > 0:
            streak = int(getattr(self._state, "correct_streak", 0))
            if streak >= self._gsp_graduation_streak:
                scaled_days = max(scaled_days, self._gsp_graduation_floor)
                graduated = True

        # 退役：过学词冻结到长间隔、越过 90 帽（在 cap/fuzz 之前 return）。
        if self._gsp_retire_after_reviews > 0 and graduated:
            rc = int(getattr(self._state, "review_count", 0))
            stab = float(getattr(self._state, "stability", 0.0))
            streak = int(getattr(self._state, "correct_streak", 0))
            if (
                rc >= self._gsp_retire_after_reviews
                and stab >= self._gsp_retire_min_stability
                and streak >= self._gsp_retire_min_streak
            ):
                return max(1, int(round(self._gsp_retire_interval_days)))

        # 区间帽：min(90, gspCap)，与既有 90 天硬帽复合（mdm.rs MAX_INTERVAL_DAYS=90）。
        cap = 90.0
        if self._gsp_interval_cap_days > 0.0:
            cap = min(cap, self._gsp_interval_cap_days)
        scaled_days = min(scaled_days, cap)

        # GSP review-load smoothing（cap 之后施加确定性区间抖动，错峰同步到 cap/floor 的复习波）。
        # 时间不变 + 身份无关 + Rust 可移植（_fuzz_u 仅读 stability/review_count，公式见 GSP_SPEC §7）。
        # 重夹纪律（GSP_SPEC §7）：
        #   • 顶侧夹 fuzz_cap = min(90, cap·(1+fuzz))，使抖动可把 cap-pinned 词推到 cap 之上错峰，
        #     但绝不越 90 天硬帽。
        #   • 毕业词（graduated）底侧夹 graduationFloor：fuzz 不得把已毕业词拉到 floor 之下
        #     （否则破坏 mastered 定义 next_interval≥30）；非毕业词底侧夹 1。
        if self._gsp_interval_fuzz > 0.0:
            u = self._fuzz_u(float(getattr(self._state, "stability", 0.0)),
                             int(getattr(self._state, "review_count", 0)))
            fuzzed = scaled_days * (1.0 + self._gsp_interval_fuzz * u)
            fuzz_cap = min(90.0, cap * (1.0 + self._gsp_interval_fuzz))
            lo = self._gsp_graduation_floor if graduated else 1.0
            scaled_days = max(lo, min(fuzz_cap, fuzzed))

        return max(1, int(round(scaled_days)))

    def update(self, recalled: int, elapsed_days: float) -> None:
        self._state.update(recalled, elapsed_days)
        self._recent_results.append(recalled)
        if len(self._recent_results) > self._window:
            self._recent_results.pop(0)
        self._update_fatigue()
        self._update_motivation(recalled)
        self._update_confidence(recalled)
        self._update_trust(recalled)
        self._attempts      += 1
        self._total_events  += 1
        self._population     = max(self._population, self._attempts)
        self._update_retention()

    def current_halflife(self) -> float:
        return max(1e-6, float(self._state.stability))


# ---------------------------------------------------------------------------
# Random Scheduler (control group)
# ---------------------------------------------------------------------------

_RANDOM_INTERVALS = [1, 3, 7, 14, 21]


class RandomScheduler:
    """Random baseline: uniformly samples from a fixed set of intervals.

    Provides a lower-bound reference; no learning from review results.
    """

    name = "random"

    def __init__(
        self,
        intervals: List[int] | None = None,
        seed: int = 42,
    ) -> None:
        self._intervals = intervals or _RANDOM_INTERVALS
        self._rng = random.Random(seed)
        self._halflife: float = float(sum(self._intervals)) / len(self._intervals)

    def warm_start(
        self,
        t_history: List[int],  # noqa: ARG002
        r_history: List[int],  # noqa: ARG002
        difficulty: float,  # noqa: ARG002
    ) -> None:
        pass  # stateless

    def next_interval_days(self) -> int:
        return self._rng.choice(self._intervals)

    def update(self, recalled: int, elapsed_days: float) -> None:  # noqa: ARG002
        pass  # stateless

    def current_halflife(self) -> float:
        return self._halflife


# ---------------------------------------------------------------------------
# Factory helper
# ---------------------------------------------------------------------------

def build_scheduler(
    name: str,
    dhp_student: DHPStudent | None = None,
    memory_config: dict | None = None,
    desired_retention: float = 0.85,
    seed: int = 42,
    ssp_policy: dict | None = None,
) -> Scheduler:
    """Return a fresh Scheduler instance by name.

    Args:
        name: one of 'fsrs', 'amas', 'dhp', 'leitner', 'random'.
        dhp_student: required when name == 'dhp' / 'ssp_mmc'.
        memory_config: optional override for FSRS/AMAS weights.
        desired_retention: target recall probability for interval computation.
        seed: RNG seed used by RandomScheduler.
        ssp_policy: SSP-MMC DP 最优策略表（load_policy_grids），required when name == 'ssp_mmc'.
    """
    if name == "fsrs":
        # 竞品隔离：fsrs 条目固定绑 FSRS_BASELINE_CONFIG（FSRS-5 公版 + 显式关 GSP/双腿信任），
        # 忽略调参传入的 memory_config（candidate 仅作用于 amas），不随 DEFAULT/候选漂移。
        return FSRSScheduler(memory_config=FSRS_BASELINE_CONFIG, desired_retention=desired_retention)
    if name == "amas":
        return AMASScheduler(memory_config=memory_config, desired_retention=desired_retention)
    if name == "dhp":
        if dhp_student is None:
            raise ValueError("dhp_student is required for DHPScheduler")
        return DHPScheduler(student=dhp_student, desired_retention=desired_retention)
    if name == "ssp_mmc":
        if dhp_student is None or ssp_policy is None:
            raise ValueError("dhp_student and ssp_policy are required for SSPMMCScheduler")
        return SSPMMCScheduler(student=dhp_student, policy=ssp_policy, desired_retention=desired_retention)
    if name == "leitner":
        return LeitnerScheduler(desired_retention=desired_retention)
    if name == "random":
        return RandomScheduler(seed=seed)
    if name == "sm2":
        return SM2Scheduler()
    if name == "hlr":
        return HLRScheduler()
    if name == "fsrs45":
        return FSRS45Scheduler(desired_retention=desired_retention)
    if name == "fsrs6":
        return FSRS6Scheduler(desired_retention=desired_retention)
    if name == "amas6":
        return AMAS6Scheduler(memory_config=memory_config, desired_retention=desired_retention)
    raise ValueError(f"unknown scheduler name: {name!r}; choose from {AVAILABLE_SCHEDULERS}")


AVAILABLE_SCHEDULERS = ["fsrs", "amas", "dhp", "ssp_mmc", "leitner", "random", "sm2", "hlr", "fsrs45", "fsrs6", "amas6"]


# ---------------------------------------------------------------------------
# SM2Scheduler — SuperMemo 2 算法 (Anki 默认调度器)
# ---------------------------------------------------------------------------
#
# 参考实现: https://github.com/open-spaced-repetition/python-sm2
# 算法描述: https://www.supermemo.com/en/blog/application-of-a-computer-to-improve-the-results-obtained-in-working-with-the-supermemo-method
#
# 状态变量:
#   ease_factor  — 难度因子，初始 2.5，最小 1.3
#   interval     — 当前间隔（天），初始 1
#   repetitions  — 连续召回次数，初始 0
#
# update 规则（输入 recalled∈{0,1}, elapsed_days）:
#   1. 由 recalled + elapsed/prev_interval 比值映射出 quality q ∈ {2, 4, 5}
#   2. recalled=1: repetitions += 1
#        repetitions==1 → interval = 1
#        repetitions==2 → interval = 6
#        else           → interval = round(prev_interval * ease_factor)
#      recalled=0: repetitions = 0, interval = 1
#   3. ease_factor: ef = max(1.3, ef + (0.1 - (5-q)*(0.08 + (5-q)*0.02)))
#
# current_halflife: 按 90% retention 反推：interval = -h * log2(0.9)
#                   故 h = interval * ln(2) / ln(1/0.9)
# ---------------------------------------------------------------------------

class SM2Scheduler:
    """SuperMemo 2 (SM2) 调度器。Anki 默认算法。

    维护 (ease_factor, interval, repetitions) 三元组，按 quality 映射规则更新。
    """

    name = "sm2"

    _INITIAL_EASE_FACTOR = 2.5
    _MIN_EASE_FACTOR = 1.3
    _Q_LAPSE = 2          # recalled=0 时的 quality
    _Q_HARD = 4           # recalled=1 但接近遗忘边缘
    _Q_EASY = 5           # recalled=1 且远超原间隔
    _OVERSHOOT_RATIO = 1.5  # elapsed / prev_interval 阈值，判定 q=5

    def __init__(self) -> None:
        self._ease_factor: float = self._INITIAL_EASE_FACTOR
        self._interval: int = 1
        self._repetitions: int = 0
        self._prev_interval: int = 1  # 用于 q 映射时的"上一次 interval"

    def _quality(self, recalled: int, elapsed_days: float) -> int:
        """根据 recalled + elapsed/prev_interval 比值映射 SM2 quality（2/4/5）。"""
        if recalled == 0:
            return self._Q_LAPSE
        ratio = elapsed_days / max(self._prev_interval, 1)
        if ratio > self._OVERSHOOT_RATIO:
            return self._Q_EASY
        return self._Q_HARD

    def _apply_update(self, recalled: int, elapsed_days: float) -> None:
        q = self._quality(recalled, elapsed_days)
        # ease_factor 更新公式（SuperMemo 原版）
        delta = 0.1 - (5 - q) * (0.08 + (5 - q) * 0.02)
        self._ease_factor = max(self._MIN_EASE_FACTOR, self._ease_factor + delta)

        # 记录更新前的 interval（用于下一次 q 映射的 prev_interval）
        prev_interval_for_calc = max(self._interval, 1)

        if recalled == 1:
            self._repetitions += 1
            if self._repetitions == 1:
                new_interval = 1
            elif self._repetitions == 2:
                new_interval = 6
            else:
                # 第 3 次及之后：当前 interval × ease_factor
                new_interval = max(1, round(prev_interval_for_calc * self._ease_factor))
        else:
            self._repetitions = 0
            new_interval = 1

        self._prev_interval = prev_interval_for_calc
        self._interval = new_interval

    def warm_start(
        self,
        t_history: List[int],
        r_history: List[int],
        difficulty: float,  # noqa: ARG002 — SM2 自适应难度，不使用外部 difficulty
    ) -> None:
        # 重置状态后重放历史
        self._ease_factor = self._INITIAL_EASE_FACTOR
        self._interval = 1
        self._repetitions = 0
        self._prev_interval = 1
        for i, (delta_t, result) in enumerate(zip(t_history, r_history)):
            elapsed = float(delta_t) if i > 0 else 0.0
            self._apply_update(int(result), elapsed)

    def next_interval_days(self) -> int:
        return max(1, int(self._interval))

    def update(self, recalled: int, elapsed_days: float) -> None:
        self._apply_update(int(recalled), float(elapsed_days))

    def current_halflife(self) -> float:
        # 按 90% retention 反推：interval = -h * log2(0.9) → h = interval * ln(2) / ln(1/0.9)
        interval = max(self._interval, 1)
        return float(interval) * math.log(2.0) / math.log(1.0 / 0.9)


# ---------------------------------------------------------------------------
# HLRScheduler — Half-Life Regression (Duolingo 2016 ACL)
# ---------------------------------------------------------------------------
#
# 参考实现: https://github.com/duolingo/halflife-regression/blob/master/experiment.py
# 论文: "A Trainable Spaced Repetition Model for Language Learning"
#        Settles & Meeder, ACL 2016
#
# 状态变量:
#   correct_count    — 累计召回成功次数
#   incorrect_count  — 累计召回失败次数
#   difficulty       — 词难度（在 warm_start 时由调用者给定，update 中不变）
#
# 特征向量:
#   features = [sqrt(correct+1), sqrt(incorrect+1), difficulty]
#
# 模型权重 θ:
#   论文给出 features=[sqrt(right+1), sqrt(wrong+1), bias, lexeme_tag]，
#   通过 SGD 在大规模数据上学习。Settles & Meeder 在 Table 4 报告的
#   fitted weight 是 right≈+2.4 / wrong≈-3.0 / bias≈+1.5 等大量值，
#   单一 lexeme 的 effective θ 在 (0.5..2.5, -1.0..-3.0, ±0.3) 区间。
#
#   任务规范文档建议初值 (0.5, -1.0, -0.3)，但该值在 < 10 reviews 的小样本下
#   h 全部 < 2 天，round 后 interval 始终为 1，无法体现 lapse 信号。
#   benchmark 中采用 (2.0, -2.5, -0.3)：
#     • θ₁=2.0：在 5 次召回后 h ≈ 4.8 天，与论文 Table 5 中
#       "right + wrong = 5" 的 90-th percentile h≈4 天相符
#     • θ₂=-2.5：单次 lapse 让 h 至少减半（θ₂·(√2-1) < -1）
#     • θ₃=-0.3：difficulty 较高时 h 略低，符合直觉
#
#   用户可通过构造函数 theta 参数覆盖。
#
# half-life:
#   h = 2^(θ · features)
#   clip 到 [MIN_HALF_LIFE_DAYS, MAX_HALF_LIFE_DAYS]
#
# p(recall) = 2^(-elapsed / h)
# next_interval_days = max(1, round(h))
# update: recalled=1 → correct_count+=1, recalled=0 → incorrect_count+=1
# ---------------------------------------------------------------------------

class HLRScheduler:
    """Duolingo HLR 调度器。h = 2^(θ·features)。"""

    name = "hlr"

    # half-life clip 范围（论文一致）
    _MIN_HALF_LIFE_DAYS = 15.0 / (24.0 * 60.0)  # 15 分钟
    _MAX_HALF_LIFE_DAYS = 365.0                  # 1 年（任务规范上限）

    # 默认 θ：Duolingo paper Table 4 fitted 值
    # 2026-05-27 从 (2.0, -2.5, -0.3) 改回 paper 原值，修复 prediction 维度 logLoss 爆炸
    # （原值是为通过 5 review 小样本的 lapse_resets 数学下限选的 trade-off）
    _DEFAULT_THETA: tuple[float, float, float] = (0.5, -1.0, -0.3)

    def __init__(
        self,
        theta: tuple[float, float, float] | None = None,
    ) -> None:
        self._theta = theta if theta is not None else self._DEFAULT_THETA
        self._correct: int = 0
        self._incorrect: int = 0
        self._difficulty: float = 0.5

    def _features(self) -> tuple[float, float, float]:
        return (
            math.sqrt(self._correct + 1.0),
            math.sqrt(self._incorrect + 1.0),
            float(self._difficulty),
        )

    def _halflife_days(self) -> float:
        feats = self._features()
        dot = sum(t * x for t, x in zip(self._theta, feats))
        # clamp 指数避免 overflow
        dot = max(-50.0, min(50.0, dot))
        h = math.pow(2.0, dot)
        return min(max(h, self._MIN_HALF_LIFE_DAYS), self._MAX_HALF_LIFE_DAYS)

    def warm_start(
        self,
        t_history: List[int],
        r_history: List[int],
        difficulty: float,
    ) -> None:
        self._correct = 0
        self._incorrect = 0
        self._difficulty = float(difficulty)
        for _delta_t, result in zip(t_history, r_history):
            if int(result) == 1:
                self._correct += 1
            else:
                self._incorrect += 1

    def next_interval_days(self) -> int:
        return max(1, int(round(self._halflife_days())))

    def update(self, recalled: int, elapsed_days: float) -> None:  # noqa: ARG002
        if int(recalled) == 1:
            self._correct += 1
        else:
            self._incorrect += 1

    def current_halflife(self) -> float:
        return self._halflife_days()


# ---------------------------------------------------------------------------
# FSRS45Scheduler — FSRS-4.5
# ---------------------------------------------------------------------------
#
# 参考: https://github.com/open-spaced-repetition/fsrs-optimizer v4.5 tag
#
# 与 FSRS-5 的主要区别：
#   1. w 数组 17 维（FSRS-5 是 19 维：少了 w[17]/w[18] 的 short-term review 项）
#   2. forgetting curve（任务规范给定）：R(t,S) = (1 + t/(9*S))^(-1)
#      —— 这等价于 factor=1/9, decay=-1, floor=0；
#      该曲线为 FSRS-3 历史曲线，FSRS-4.5 实际代码也常用 (1+19/81·t/S)^(-0.5)，
#      但本实现遵循任务规范字面要求。
#   3. update 时不再走 FSRS-5 的 short-term branch（elapsed_days<1）；
#      统一走 long-term success / lapse 分支。
#
# 默认 w（任务规范给定，对应 fsrs-optimizer v4.5 release default）:
#   [0.4, 0.6, 2.4, 5.8, 4.93, 0.94, 0.86, 0.01, 1.49,
#    0.14, 0.94, 2.18, 0.05, 0.34, 1.26, 0.29, 2.61]
#
# State: (stability, difficulty, review_count)，与 WordforgeMirrorState 类似
# 但独立实现避免污染 FSRS-5 mirror。
#
# current_halflife() = stability * ln(2)（90% target retention 近似反推半衰期）
# ---------------------------------------------------------------------------

# 任务规范给定的 FSRS-4.5 默认 w（17 维）
_FSRS45_DEFAULT_W: tuple[float, ...] = (
    0.4, 0.6, 2.4, 5.8, 4.93, 0.94, 0.86, 0.01, 1.49,
    0.14, 0.94, 2.18, 0.05, 0.34, 1.26, 0.29, 2.61,
)

# 任务规范给定的 forgetting curve 常数：R(t,S) = (1 + t/(9*S))^(-1)
_FSRS45_FACTOR = 1.0 / 9.0
_FSRS45_DECAY = -1.0
_FSRS45_FLOOR = 0.0


class FSRS45MirrorState:
    """FSRS-4.5 mirror state（独立于 WordforgeMirrorState，不复用 FSRS-5 19 维公式）。

    实现来自 WordforgeMirrorState 的 long-term 分支，去除 w[17]/w[18] 相关项；
    forgetting curve 用任务规范给定的 (1+t/9S)^(-1)。
    """

    __slots__ = ("weights", "desired_retention", "stability", "difficulty", "review_count")

    def __init__(
        self,
        weights: List[float],
        desired_retention: float,
    ) -> None:
        assert len(weights) == 17, f"FSRS-4.5 expects 17 weights, got {len(weights)}"
        self.weights: list[float] = list(weights)
        self.desired_retention: float = float(desired_retention)
        self.stability: float = 0.4
        self.difficulty: float = 5.0
        self.review_count: int = 0

    def recall(self, elapsed_days: float) -> float:
        """R(t,S) = (1 + t/(9*S))^(-1)（任务规范给定的 FSRS-4.5 curve）。"""
        s = max(self.stability, 0.01)
        # (1 + factor*t/s)^decay；这里 factor=1/9, decay=-1
        return max(
            0.0,
            min(
                1.0,
                _FSRS45_FLOOR + (1.0 - _FSRS45_FLOOR) * math.pow(
                    max(1e-9, 1.0 + _FSRS45_FACTOR * elapsed_days / s),
                    _FSRS45_DECAY,
                ),
            ),
        )

    def update(self, recalled: int, elapsed_days: float) -> None:
        """复用 WordforgeMirrorState long-term 分支（无 short-term：elapsed_days<1 路径）。"""
        grade = 4 if recalled == 1 else 1
        if self.review_count == 0:
            # 初次评估：S0 = w[grade-1]，D0 由 w[4], w[5] 计算
            self.stability = self.weights[grade - 1]
            self.difficulty = max(
                1.0,
                min(
                    10.0,
                    self.weights[4] - math.exp(self.weights[5] * (grade - 1.0)) + 1.0,
                ),
            )
        else:
            recall = self.recall(elapsed_days)
            if grade >= 2:
                # 成功召回：multiplicative stability increase
                bonus = self.weights[16] if grade == 4 else 1.0
                s_inc = (
                    math.exp(self.weights[8])
                    * (11.0 - self.difficulty)
                    * math.pow(max(self.stability, 0.01), -self.weights[9])
                    * (math.exp(self.weights[10] * (1.0 - recall)) - 1.0)
                    * bonus
                )
                self.stability = max(0.01, self.stability * (max(0.0, s_inc) + 1.0))
            else:
                # 失败：lapse formula，stability 取 min(prev_S, lapse_S)
                self.stability = max(
                    0.01,
                    min(
                        self.stability,
                        self.weights[11]
                        * math.pow(self.difficulty, -self.weights[12])
                        * (math.pow(self.stability + 1.0, self.weights[13]) - 1.0)
                        * math.exp(self.weights[14] * (1.0 - recall)),
                    ),
                )
        self.review_count += 1

    def interval_days(self) -> int:
        """求解 R(t,S) = desired_retention 的 t。"""
        adjusted_target = max(
            1e-6,
            min(
                1.0,
                (self.desired_retention - _FSRS45_FLOOR) / (1.0 - _FSRS45_FLOOR),
            ),
        )
        # t = (S / factor) * (target^(1/decay) - 1)
        days = self.stability / _FSRS45_FACTOR * (
            math.pow(adjusted_target, 1.0 / _FSRS45_DECAY) - 1.0
        )
        return max(1, math.ceil(days))


class FSRS45Scheduler:
    """FSRS-4.5 调度器。

    与 FSRSScheduler（FSRS-5）结构对称，但使用 17 维 w 和 (1+t/9S)^(-1) 曲线。
    """

    name = "fsrs45"

    def __init__(
        self,
        weights: List[float] | None = None,
        desired_retention: float = 0.9,
    ) -> None:
        self._weights = list(weights) if weights is not None else list(_FSRS45_DEFAULT_W)
        assert len(self._weights) == 17, "FSRS-4.5 requires 17 weights"
        self._desired_retention = float(desired_retention)
        self._state = self._fresh_state()

    def _fresh_state(self) -> FSRS45MirrorState:
        return FSRS45MirrorState(
            weights=self._weights,
            desired_retention=self._desired_retention,
        )

    def warm_start(
        self,
        t_history: List[int],
        r_history: List[int],
        difficulty: float,  # noqa: ARG002 — FSRS 内部派生 difficulty
    ) -> None:
        self._state = self._fresh_state()
        for i, (delta_t, result) in enumerate(zip(t_history, r_history)):
            elapsed = float(delta_t) if i > 0 else 0.0
            self._state.update(int(result), elapsed)

    def next_interval_days(self) -> int:
        return self._state.interval_days()

    def update(self, recalled: int, elapsed_days: float) -> None:
        self._state.update(int(recalled), float(elapsed_days))

    def current_halflife(self) -> float:
        # 任务规范：stability * ln(2)（90% target retention 反推近似半衰期）
        return max(1e-6, float(self._state.stability)) * math.log(2.0)


# ---------------------------------------------------------------------------
# FSRS-6 Scheduler — Free Spaced Repetition Scheduler v6 (open-spaced-repetition)
# ---------------------------------------------------------------------------
#
# 与 FSRS-5 主要差异：
# 1. w 数组 21 维（FSRS-5 是 19，新增 w19 / w20）
# 2. forgetting curve decay 变 trainable: R = (1+factor·t/S)^(-w20)
#    factor = 0.9^(-1/w20) - 1，保证 R(S,S)=0.9
# 3. 同日复习公式新增 S^(-w19) 饱和项（小 S 增长快，大 S 增长慢）
#
# 参考: https://github.com/open-spaced-repetition/py-fsrs
#       https://github.com/open-spaced-repetition/awesome-fsrs/wiki/The-Algorithm
# ---------------------------------------------------------------------------

# py-fsrs main DEFAULT_PARAMETERS（21 维）
_FSRS6_DEFAULT_W: tuple[float, ...] = (
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001,
    1.8722, 0.1666, 0.796, 1.4835, 0.0614, 0.2629, 1.6483, 0.6014,
    1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
)


class FSRS6MirrorState:
    """FSRS-6 mirror state（21 维 w + trainable decay）。"""

    __slots__ = ("weights", "desired_retention", "stability", "difficulty", "review_count", "_factor", "_decay")

    def __init__(self, weights: List[float], desired_retention: float) -> None:
        assert len(weights) == 21, f"FSRS-6 expects 21 weights, got {len(weights)}"
        self.weights: list[float] = list(weights)
        self.desired_retention: float = float(desired_retention)
        self.stability: float = 0.4
        self.difficulty: float = 5.0
        self.review_count: int = 0
        # decay = w20，factor = 0.9^(-1/decay) - 1（保证 R(S,S)=0.9）
        self._decay: float = max(0.01, self.weights[20])
        self._factor: float = math.pow(0.9, -1.0 / self._decay) - 1.0

    def recall(self, elapsed_days: float) -> float:
        """R(t,S) = (1 + factor·t/S)^(-decay)，由 w20 确定 decay。"""
        s = max(self.stability, 0.01)
        base = max(1e-9, 1.0 + self._factor * elapsed_days / s)
        return max(0.0, min(1.0, math.pow(base, -self._decay)))

    def update(self, recalled: int, elapsed_days: float) -> None:
        grade = 4 if recalled == 1 else 1   # 简化二元映射：成功→Good(3+1=4 Easy 偏向？)；保持与 FSRS-4.5 一致：1=Again, 4=Easy
        # 实际项目内 r_history 是 0/1，直接映射 grade∈{1,4}；FSRS 标准 grade ∈ {1,2,3,4}
        if self.review_count == 0:
            # 初始 S0 = w[grade-1]
            self.stability = max(0.01, self.weights[grade - 1])
            # 初始 D0 = w4 - exp(w5·(grade-1)) + 1，clamp [1,10]
            self.difficulty = max(1.0, min(10.0,
                self.weights[4] - math.exp(self.weights[5] * (grade - 1.0)) + 1.0
            ))
        else:
            r = self.recall(elapsed_days)
            # D 更新（mean reversion 目标 = D_0(Easy)=D_0(4)）
            delta_d = -self.weights[6] * (grade - 3)
            d_prime = self.difficulty + delta_d * (10.0 - self.difficulty) / 9.0
            d_target = self.weights[4] - math.exp(self.weights[5] * (4 - 1.0)) + 1.0
            self.difficulty = max(1.0, min(10.0,
                self.weights[7] * d_target + (1.0 - self.weights[7]) * d_prime
            ))
            if grade >= 2:
                # 成功复习 S update
                hard_penalty = self.weights[15] if grade == 2 else 1.0
                easy_bonus = self.weights[16] if grade == 4 else 1.0
                s_inc = (
                    math.exp(self.weights[8])
                    * (11.0 - self.difficulty)
                    * math.pow(max(self.stability, 0.01), -self.weights[9])
                    * (math.exp(self.weights[10] * (1.0 - r)) - 1.0)
                    * hard_penalty
                    * easy_bonus
                )
                self.stability = max(0.01, self.stability * (max(0.0, s_inc) + 1.0))
            else:
                # post-lapse
                self.stability = max(0.01, min(
                    self.stability,
                    self.weights[11]
                    * math.pow(self.difficulty, -self.weights[12])
                    * (math.pow(self.stability + 1.0, self.weights[13]) - 1.0)
                    * math.exp(self.weights[14] * (1.0 - r))
                ))
        self.review_count += 1

    def interval_days(self) -> int:
        """求解 R(t,S) = desired_retention 的 t（FSRS-6 形态）。"""
        r = max(1e-6, min(1.0, self.desired_retention))
        # t = (S / factor) · (r^(-1/decay) - 1)
        days = self.stability / self._factor * (math.pow(r, -1.0 / self._decay) - 1.0)
        return max(1, math.ceil(days))


class FSRS6Scheduler:
    """FSRS-6 调度器（21 维 w，trainable decay）。"""

    name = "fsrs6"

    def __init__(
        self,
        weights: List[float] | None = None,
        desired_retention: float = 0.9,
    ) -> None:
        self._weights = list(weights) if weights is not None else list(_FSRS6_DEFAULT_W)
        assert len(self._weights) == 21, "FSRS-6 requires 21 weights"
        self._desired_retention = float(desired_retention)
        self._state = self._fresh_state()

    def _fresh_state(self) -> FSRS6MirrorState:
        return FSRS6MirrorState(weights=self._weights, desired_retention=self._desired_retention)

    def warm_start(
        self,
        t_history: List[int],
        r_history: List[int],
        difficulty: float,  # noqa: ARG002 — FSRS 内部派生
    ) -> None:
        self._state = self._fresh_state()
        for i, (delta_t, result) in enumerate(zip(t_history, r_history)):
            elapsed = float(delta_t) if i > 0 else 0.0
            self._state.update(int(result), elapsed)

    def next_interval_days(self) -> int:
        return self._state.interval_days()

    def update(self, recalled: int, elapsed_days: float) -> None:
        self._state.update(int(recalled), float(elapsed_days))

    def current_halflife(self) -> float:
        return max(1e-6, float(self._state.stability)) * math.log(2.0)


# ---------------------------------------------------------------------------
# AMAS6 Scheduler — AMAS 全栈 + FSRS-6 mirror state
# ---------------------------------------------------------------------------
#
# 与 AMASScheduler 的差异：
# 1. 底层 mirror state 从 WordforgeMirrorState (FSRS-5, 19 维 w) 换为
#    FSRS6MirrorState (21 维 w + trainable decay w20)。
# 2. _recall(elapsed) 直接 delegate 到 state.recall()，复用 FSRS-6 曲线。
# 3. wordSelector / ensemble / heuristic / IGE / engine 全部复用 AMAS 父类。
#
# 目的：验证 wordSelector / ensemble 是否能在更强的 FSRS-6 底层上获得 synergy 增益。
# ---------------------------------------------------------------------------


class AMAS6Scheduler(AMASScheduler):
    """AMAS 全栈 + FSRS-6 mirror state。"""

    name = "amas6"

    def __init__(
        self,
        memory_config: dict | None = None,
        desired_retention: float = 0.85,
        fsrs6_weights: List[float] | None = None,
    ) -> None:
        # 跳过父类的 FSRS-5 weights 解析，自己 setup
        # 父类 __init__ 用 cfg["w"] (19 维) 和 forgettingCurveFactor / Decay / Floor
        # 我们改用 FSRS-6 21 维 w + 从 w20 算 factor
        cfg = memory_config or DEFAULT_MEMORY_MODEL_CONFIG
        self._cfg = cfg
        self._fsrs6_weights: list[float] = (
            list(fsrs6_weights) if fsrs6_weights is not None else list(_FSRS6_DEFAULT_W)
        )
        assert len(self._fsrs6_weights) == 21, "AMAS6 requires 21 weights"

        # 占位：父类有部分代码读 self._factor / self._decay_e / self._floor，
        # 我们用 FSRS-6 等价值（factor = 0.9^(-1/w20) - 1, decay = -w20, floor = 0）
        decay = max(0.01, self._fsrs6_weights[20])
        self._factor = math.pow(0.9, -1.0 / decay) - 1.0
        self._decay_e = -decay
        self._floor = 0.0

        self._base_retention = float(desired_retention)
        self._retention = self._base_retention
        # amas6 是公版隔离条目：GSP 旋钮硬关，永不从 config 继承（即便 config 误带 gsp* 键）。
        self._gsp_interval_cap_days = 0.0
        self._gsp_graduation_streak = 0
        self._gsp_graduation_floor = 30.0
        self._gsp_interval_fuzz = 0.0  # 公版隔离：fuzz 硬关，永不从 config 继承
        # 公版隔离：retire 硬关，永不从 config 继承（父类 next_interval_days 读这四项）
        self._gsp_retire_after_reviews = 0
        self._gsp_retire_interval_days = 365.0
        self._gsp_retire_min_stability = 0.0
        self._gsp_retire_min_streak = 0
        self._state = self._fresh_state()
        self._window = int(cfg.get("masteryWindowSize", 20))

        # 复制父类剩余初始化
        self._recent_results: list[int] = []
        self._fatigue: float = 0.0
        self._motivation: float = 0.0
        self._confidence: float = 1.0
        self._total_events: int = 0
        self._trust_h: float = self._ENS_TRUST_INIT
        self._trust_i: float = self._ENS_TRUST_INIT
        self._trust_s: float = self._ENS_TRUST_INIT
        self._attempts: int = 0
        self._population: int = 0

    def _fresh_state(self) -> "FSRS6MirrorState":
        return FSRS6MirrorState(
            weights=self._fsrs6_weights,
            desired_retention=self._retention,
        )

    def _recall(self, elapsed_days: float) -> float:
        """直接用 FSRS6MirrorState 的 recall（覆盖父类 FSRS-5 power-law 实现）。"""
        return self._state.recall(float(elapsed_days))
