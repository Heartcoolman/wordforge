from __future__ import annotations

import csv
import math
import random
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Sequence


DAY_LIMIT = 200
LEARN_DAYS = 90
DECK_SIZE = 5000
TARGET_HALFLIFE = 360.0
BASE = 1.05
MIN_INDEX = -30
DEFAULT_RETENTION = 0.85
DEFAULT_FORGETTING_CURVE_FACTOR = 19.0 / 81.0
DEFAULT_FORGETTING_CURVE_DECAY = -0.5
DEFAULT_FORGETTING_CURVE_FLOOR = 0.10

PARAMETERS_URL = "https://raw.githubusercontent.com/maimemo/MaiMemoSimulator/main/parameters.csv"
POLICY_URL = "https://raw.githubusercontent.com/maimemo/MaiMemoSimulator/main/policy/ivl-{difficulty}.csv"

D2P = [0.86, 0.78, 0.72, 0.66, 0.61, 0.55, 0.49, 0.44, 0.39, 0.34]
DIFFICULTY_WEIGHTS = (5, 9, 10, 14, 15, 13, 11, 10, 8, 5)


def ensure_reference_assets(root: Path) -> Dict[str, Path]:
    root.mkdir(parents=True, exist_ok=True)
    parameters_path = root / "parameters.csv"
    policy_dir = root / "policy"
    policy_dir.mkdir(exist_ok=True)
    if not parameters_path.exists():
        urllib.request.urlretrieve(PARAMETERS_URL, parameters_path)
    for difficulty in range(1, 19):
        path = policy_dir / f"ivl-{difficulty}.csv"
        if not path.exists():
            urllib.request.urlretrieve(POLICY_URL.format(difficulty=difficulty), path)
    return {"parameters": parameters_path, "policy_dir": policy_dir}


_DHP_PARAMS_CACHE: Dict[str, Dict[str, float]] = {}


def load_dhp_params(path: Path) -> Dict[str, float]:
    # 模块级缓存（按 str(path)）：per-trial 约束会高频调用，避免重复解析 CSV
    key = str(path)
    if key not in _DHP_PARAMS_CACHE:
        with path.open("r", encoding="utf-8") as handle:
            _DHP_PARAMS_CACHE[key] = {
                name: float(value) for name, value in next(csv.DictReader(handle)).items()
            }
    return dict(_DHP_PARAMS_CACHE[key])


def load_policy_tables(policy_dir: Path) -> Dict[int, List[int]]:
    tables: Dict[int, List[int]] = {}
    for difficulty in range(1, 19):
        with (policy_dir / f"ivl-{difficulty}.csv").open("r", encoding="utf-8") as handle:
            tables[difficulty] = [int(float(row[1])) for row in csv.reader(handle)]
    return tables


@dataclass
class DHPStudent:
    ra: float
    rb: float
    rc: float
    rd: float
    fa: float
    fb: float
    fc: float
    fd: float

    def init(self, difficulty: int, rng: random.Random):
        p = D2P[difficulty - 1]
        result = 1 if rng.random() < p else 0
        halflife = self.start_halflife(difficulty, result)
        return result, 0, p, [halflife, difficulty], halflife

    def start_halflife(self, difficulty: int, result: int) -> float:
        initial = -1.0 / math.log2(max(0.925 - 0.05 * difficulty, 0.025))
        return initial * 10.0 if result == 1 else initial

    def next_state(self, state, recalled: int, delta_t: int, _p_recall: float):
        halflife, difficulty = state
        p_recall = math.pow(2.0, -delta_t / halflife)
        if recalled == 1:
            new_halflife = halflife * (
                1.0
                + math.exp(self.ra)
                * math.pow(difficulty, self.rb)
                * math.pow(halflife, self.rc)
                * math.pow(1 - p_recall, self.rd)
            )
            new_difficulty = difficulty
        else:
            new_halflife = (
                math.exp(self.fa)
                * math.pow(difficulty, self.fb)
                * math.pow(halflife, self.fc)
                * math.pow(1 - p_recall, self.fd)
            )
            new_difficulty = min(difficulty + 2, 18)
        return [new_halflife, new_difficulty], new_halflife


# —— WordforgeMirrorState 镜像常量（对齐 benchmark_adapter.rs replay_history 语义）——
# 二元成绩按 FSRS 拟合惯例映射：成功 quality=0.7 → Good(3)，失败 0.0 → Again(1)；
# 勿映射 Easy(4)：首评 stability 会取 w[3] 造成约 5x 膨胀，扭曲调参闸门
SUCCESS_GRADE = 3
FAIL_GRADE = 1
# S 上限对齐 mdm.rs（FSRS-6 参考实现 ≈100 年，防极端 w 组合幂运算溢出）
STABILITY_CAP_DAYS = 36_500.0
# 连击最小间隔（mastery.rs streak_min_gap_ms=1800000 的天级形态，≈30 分钟）
DEFAULT_STREAK_MIN_GAP_DAYS = 1_800_000.0 / 86_400_000.0


@dataclass
class WordforgeMirrorState:
    weights: Sequence[float]
    desired_retention: float
    forgetting_curve_factor: float
    forgetting_curve_decay: float
    forgetting_curve_floor: float
    # 间隔顶侧夹紧（mdm.rs compute_interval max_interval_days，Rust 默认 90.0）
    max_interval_days: float = 90.0
    # 连击动态 alpha（mastery.rs:82-85 / benchmark_adapter.rs 重放，interval_scale 钉 1.0）
    alpha_scale: float = 0.3
    alpha_min: float = 0.1
    alpha_max: float = 0.5
    streak_min_gap_days: float = DEFAULT_STREAK_MIN_GAP_DAYS
    stability: float = 0.4
    difficulty: float = 5.0
    review_count: int = 0
    correct_streak: int = 0

    def recall(self, elapsed_days: float) -> float:
        power = math.pow(
            1.0 + self.forgetting_curve_factor * elapsed_days / max(self.stability, 0.01),
            self.forgetting_curve_decay,
        )
        return max(
            0.0,
            min(1.0, self.forgetting_curve_floor + (1.0 - self.forgetting_curve_floor) * power),
        )

    def update(self, recalled: int, elapsed_days: float) -> None:
        """逐句镜像 mdm.rs::update_strength（quality 0.7/0.0 + 连击动态 alpha 的 adapter 重放形态）。

        alpha 镜像 mastery.rs:69-87 @ interval_scale=1.0：成功先推进连击（首评 gap 恒过、
        同日 delta_t=0 不加不清）、失败清零，再 base 夹 → ×(1+0.1·min(streak,5)) → 整体夹。
        """
        grade = SUCCESS_GRADE if recalled == 1 else FAIL_GRADE
        w = self.weights
        if recalled == 1:
            gap_ok = self.review_count == 0 or elapsed_days >= self.streak_min_gap_days
            if gap_ok:
                self.correct_streak += 1
        else:
            self.correct_streak = 0
        base_alpha = max(
            self.alpha_min, min(self.alpha_max, 1.0 * self.alpha_scale)
        )
        streak_bonus = 1.0 + min(self.correct_streak, 5) * 0.1
        alpha = max(self.alpha_min, min(self.alpha_max, base_alpha * streak_bonus))
        if self.review_count == 0:
            # 首评（mdm.rs review_count==0 分支）：S/D 直接赋值，无 alpha 平滑、无 S 上限
            self.stability = w[grade - 1]
            self.difficulty = max(
                1.0,
                min(10.0, w[4] - math.exp(w[5] * (grade - 1.0)) + 1.0),
            )
        else:
            # 顺序语义（mdm.rs:111-175）：先算 D/S 的 target，再对两者同步 alpha 平滑
            r = self.recall(elapsed_days)
            # mdm.rs 对 prev_S 单次读取 .max(0.01)，本分支所有 prev_S 引用（含平滑基点）共用
            prev_stability = max(self.stability, 0.01)
            prev_difficulty = self.difficulty

            # 难度 target：ΔD 线性项 + 均值回归；锚点 D0(4) 在 mdm.rs:120 硬编码 w[5]*3.0（勿改成 G=3）
            delta_d = -w[6] * (grade - 3.0)
            d_prime = prev_difficulty + delta_d * (10.0 - prev_difficulty) / 9.0
            d0_4 = max(1.0, min(10.0, w[4] - math.exp(w[5] * 3.0) + 1.0))
            target_difficulty = max(
                1.0, min(10.0, w[7] * d0_4 + (1.0 - w[7]) * d_prime)
            )

            if elapsed_days < 1.0:
                # 同日复习：S' = S·e^{w17·(G-3+w18)}·S^{-w19}（w19 饱和项；G≥3 强制 S'≥S）
                exponent = max(-20.0, min(20.0, w[17] * (float(grade) - 3.0 + w[18])))
                s_short = prev_stability * math.exp(exponent)
                if len(w) >= 21:
                    # 19 维旧权重在 Rust 侧迁移为 w19=0（memory.rs de_w_legacy_or_fsrs6），
                    # 饱和项 S^0=1 无操作，故 len<21 时跳过此乘等价
                    s_short *= math.pow(prev_stability, -w[19])
                target_stability = max(s_short, 0.01)
                if grade >= 3:
                    # G≥3 下限对 19/21 维一致适用（mdm.rs:136-140 对迁移后配置无条件执行）
                    target_stability = max(target_stability, prev_stability)
            elif grade >= 2:
                # 成功召回：S'_r = S·(e^w8·(11-D)·S^{-w9}·(e^{w10(1-R)}-1)·bonus + 1)
                if grade == 2:
                    bonus = w[15]  # Hard
                elif grade == 4:
                    bonus = w[16]  # Easy
                else:
                    bonus = 1.0  # Good —— 二元映射下恒走此分支；结构保留与 mdm.rs 逐句对应
                s_inc = max(
                    math.exp(w[8])
                    * (11.0 - prev_difficulty)
                    * math.pow(prev_stability, -w[9])
                    * (math.exp(w[10] * (1.0 - r)) - 1.0)
                    * bonus,
                    0.0,
                )
                target_stability = max(prev_stability * (s_inc + 1.0), 0.01)
            else:
                # 遗忘（Again）：S'_f = w11·D^{-w12}·((S+1)^w13−1)·e^{w14(1-R)}；
                # clamp 到 prev_S 发生在 TARGET 层（平滑之前），对应 mdm.rs .clamp(0.01, prev_stability)
                target_stability = max(
                    0.01,
                    min(
                        prev_stability,
                        w[11]
                        * math.pow(prev_difficulty, -w[12])
                        * (math.pow(prev_stability + 1.0, w[13]) - 1.0)
                        * math.exp(w[14] * (1.0 - r)),
                    ),
                )

            # alpha 平滑（mdm.rs:170-174）：D 夹 [1,10]；S 以 prev_S_safe 为基点，夹 [0.01, 36500]
            self.difficulty = max(
                1.0,
                min(10.0, prev_difficulty + (target_difficulty - prev_difficulty) * alpha),
            )
            self.stability = max(
                0.01,
                min(
                    STABILITY_CAP_DAYS,
                    prev_stability + (target_stability - prev_stability) * alpha,
                ),
            )
        self.review_count += 1

    def _interval_days_raw(self, target_retention: float | None = None) -> float:
        """mdm.rs compute_interval 的天级形态：解曲线 + maxIntervalDays 顶侧夹紧（pre-ceil）。"""
        retention = self.desired_retention if target_retention is None else target_retention
        adjusted_target = max(
            1e-6,
            min(
                1.0,
                (retention - self.forgetting_curve_floor)
                / (1.0 - self.forgetting_curve_floor),
            ),
        )
        days = self.stability / self.forgetting_curve_factor * (
            math.pow(adjusted_target, 1.0 / self.forgetting_curve_decay) - 1.0
        )
        return min(days, self.max_interval_days)

    def interval_days(self) -> int:
        # 底侧 min_interval_secs=60s 在天粒度下与 max(1, ceil) 等价（mdm.rs:246-247）
        return max(1, math.ceil(self._interval_days_raw()))


@dataclass
class RefItem:
    difficulty: int
    mirror: WordforgeMirrorState
    halflife: float | None = None
    due_date: float = LEARN_DAYS
    last_date: int | None = None
    state: List[float] | None = None

def _alpha_kwargs_from_config(memory_config: Dict[str, object]) -> Dict[str, float]:
    """连击动态 alpha 四参数（缺省值与 Rust serde default 一致）。"""
    return {
        "alpha_scale": float(memory_config.get("alphaScale", 0.3)),
        "alpha_min": float(memory_config.get("alphaMin", 0.1)),
        "alpha_max": float(memory_config.get("alphaMax", 0.5)),
        "streak_min_gap_days": float(memory_config.get("streakMinGapMs", 1_800_000))
        / 86_400_000.0,
    }


def _mirror_from_config(memory_config: Dict[str, object]) -> WordforgeMirrorState:
    weights = list(memory_config["w"])
    # 顶侧间隔夹紧与 Rust default_max_interval_days() 同默认（90 天）
    max_interval_days = float(memory_config.get("maxIntervalDays", 90.0))
    alpha_kwargs = _alpha_kwargs_from_config(memory_config)
    if len(weights) >= 21:
        # FSRS-6：曲线参数由 w[20] 派生（与 Rust MemoryModelConfig::curve_* 一致）
        decay = max(0.05, min(2.0, float(weights[20])))
        return WordforgeMirrorState(
            weights=weights,
            desired_retention=float(memory_config.get("baseDesiredRetention", DEFAULT_RETENTION)),
            forgetting_curve_factor=math.pow(0.9, -1.0 / decay) - 1.0,
            forgetting_curve_decay=-decay,
            forgetting_curve_floor=float(
                memory_config.get("forgettingCurveFloor", 0.0)
            ),
            max_interval_days=max_interval_days,
            **alpha_kwargs,
        )
    return WordforgeMirrorState(
        weights=weights,
        desired_retention=float(memory_config.get("baseDesiredRetention", DEFAULT_RETENTION)),
        forgetting_curve_factor=float(
            memory_config.get("forgettingCurveFactor", DEFAULT_FORGETTING_CURVE_FACTOR)
        ),
        forgetting_curve_decay=float(
            memory_config.get("forgettingCurveDecay", DEFAULT_FORGETTING_CURVE_DECAY)
        ),
        forgetting_curve_floor=float(
            memory_config.get("forgettingCurveFloor", DEFAULT_FORGETTING_CURVE_FLOOR)
        ),
        max_interval_days=max_interval_days,
        **alpha_kwargs,
    )


def run_wordforge_reference(
    policy_dir: Path,
    parameters_path: Path,
    memory_config: Dict[str, object],
    seed: int = 42,
) -> Dict[str, float]:
    student = DHPStudent(**load_dhp_params(parameters_path))
    rng = random.Random(seed)
    items = [
        RefItem(
            difficulty=rng.choices(range(1, 11), weights=DIFFICULTY_WEIGHTS, k=1)[0],
            mirror=_mirror_from_config(memory_config),
        )
        for _ in range(DECK_SIZE)
    ]

    due_recall_series: List[float] = []
    expected_memory = 0.0

    for day in range(LEARN_DAYS):
        due_probs: List[float] = []
        for item in items:
            if item.halflife is None or item.last_date is None:
                continue
            delta_t = day - item.last_date
            p = math.pow(2.0, -delta_t / item.halflife)
            if item.due_date <= day:
                due_probs.append(p)
        due_recall_series.append(sum(due_probs) / len(due_probs) if due_probs else 0.0)

        budget = DAY_LIMIT
        due_items = [item for item in items if item.halflife is not None and item.due_date <= day]
        due_items.sort(key=lambda entry: entry.mirror.recall(max(0, day - (entry.last_date or day))))
        for item in due_items:
            if budget <= 0:
                break
            delta_t = day - (item.last_date or day)
            p = math.pow(2.0, -delta_t / item.halflife)
            recalled = 1 if rng.random() < p else 0
            item.last_date = day
            item.state, item.halflife = student.next_state(item.state, recalled, delta_t, p)
            item.mirror.update(recalled, max(float(delta_t), 1.0))
            item.due_date = day + item.mirror.interval_days()
            budget -= 1

        for item in items:
            if budget <= 0:
                break
            if item.halflife is not None:
                continue
            recalled, _, p, state, halflife = student.init(item.difficulty, rng)
            item.last_date = day
            item.state = state
            item.halflife = halflife
            item.mirror.update(recalled, 0.0)
            item.due_date = day + item.mirror.interval_days()
            budget -= 1

        expected_memory = 0.0
        for item in items:
            if item.halflife is None or item.last_date is None:
                continue
            expected_memory += math.pow(2.0, -(day - item.last_date) / item.halflife)

    next_day_memory = 0.0
    target_count = 0
    for item in items:
        if item.halflife is None or item.last_date is None:
            continue
        next_day_memory += math.pow(2.0, -(LEARN_DAYS - item.last_date) / item.halflife)
        if item.halflife >= TARGET_HALFLIFE:
            target_count += 1
    return {
        "expectedMemory": expected_memory,
        "nextDayMemory": next_day_memory,
        "targetCount": target_count,
        "avgDueRecall": sum(due_recall_series) / len(due_recall_series),
    }
