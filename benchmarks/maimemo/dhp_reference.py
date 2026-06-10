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


@dataclass
class WordforgeMirrorState:
    weights: Sequence[float]
    desired_retention: float
    forgetting_curve_factor: float
    forgetting_curve_decay: float
    forgetting_curve_floor: float
    stability: float = 0.4
    difficulty: float = 5.0
    review_count: int = 0

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
        grade = 4 if recalled == 1 else 1
        if self.review_count == 0:
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
            if elapsed_days < 1.0:
                grade_f = float(grade)
                exponent = max(-20.0, min(20.0, self.weights[17] * (grade_f - 3.0 + self.weights[18])))
                s_short = self.stability * math.exp(exponent)
                if len(self.weights) >= 21:
                    # FSRS-6 同日饱和项 S^(-w19)；G≥3 不降 S
                    s_short *= math.pow(max(self.stability, 0.01), -self.weights[19])
                    if grade >= 3:
                        s_short = max(s_short, self.stability)
                self.stability = max(0.01, s_short)
            elif grade >= 2:
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
        adjusted_target = max(
            1e-6,
            min(
                1.0,
                (self.desired_retention - self.forgetting_curve_floor)
                / (1.0 - self.forgetting_curve_floor),
            ),
        )
        days = self.stability / self.forgetting_curve_factor * (
            math.pow(adjusted_target, 1.0 / self.forgetting_curve_decay) - 1.0
        )
        return max(1, math.ceil(days))


@dataclass
class RefItem:
    difficulty: int
    mirror: WordforgeMirrorState
    halflife: float | None = None
    due_date: float = LEARN_DAYS
    last_date: int | None = None
    state: List[float] | None = None

def _mirror_from_config(memory_config: Dict[str, object]) -> WordforgeMirrorState:
    weights = list(memory_config["w"])
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
