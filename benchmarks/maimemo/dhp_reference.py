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


_POLICY_GRIDS_CACHE: Dict[str, Dict[int, "tuple[List[float], List[int]]"]] = {}


def load_policy_grids(policy_dir: Path) -> Dict[int, "tuple[List[float], List[int]]"]:
    """加载 SSP-MMC DP 最优策略表，保留 halflife 网格用于状态查表。

    与 load_policy_tables（只读最优间隔列）不同：本函数返回 难度 →
    (halflife 网格升序, 对应最优下一间隔)，供 SSPMMCScheduler 按当前 halflife
    在 152 点对数网格上找最近箱、读出 maimemo 官方 DP solver 解出的最优间隔。
    模块级缓存按 policy_dir 复用（per-trial/per-user 高频构造，避免重复解析 18×152 行）。
    """
    key = str(policy_dir)
    if key not in _POLICY_GRIDS_CACHE:
        grids: Dict[int, "tuple[List[float], List[int]]"] = {}
        for difficulty in range(1, 19):
            halflives: List[float] = []
            intervals: List[int] = []
            with (policy_dir / f"ivl-{difficulty}.csv").open("r", encoding="utf-8") as handle:
                for row in csv.reader(handle):
                    halflives.append(float(row[0]))
                    intervals.append(int(float(row[1])))
            grids[difficulty] = (halflives, intervals)
        _POLICY_GRIDS_CACHE[key] = grids
    return _POLICY_GRIDS_CACHE[key]


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
    # 双腿信任调度（mdm.rs，语义替换旧 count 挂靠；0.0=关闭即冻结语义）：
    # 成功腿 alphaRampTau=τ_s 挂靠 correct_streak（失败清零→阻尼重启）；
    # 失败腿 alphaLapseRampTau=τ_f 挂靠累计 lapse 数（首错 f=1 no-op，leech 加速压 S）
    alpha_ramp_tau: float = 0.0
    alpha_lapse_ramp_tau: float = 0.0
    # GSP 成熟度分带保持率（生产新旋钮，默认 0.0=关闭=冻结旧语义）：
    # gsp_maturity_band_days>0 时，interval 求解的目标保持率改为
    #   stability < band → gsp_young_retention（年轻卡高保持率，密集巩固）
    #   stability >= band → gsp_mature_retention（成熟卡低保持率，拉长间隔省复习量）
    # 仅替换 interval 求解口径；recall()/预测路径完全不受影响。
    gsp_young_retention: float = 0.0
    gsp_mature_retention: float = 0.0
    gsp_maturity_band_days: float = 0.0
    # GSP 成功成绩带（生产新旋钮，默认 3=Good=bit-exact legacy=benchmark_adapter SUCCESS_QUALITY=0.7）：
    # 二元成功复习映射到的 FSRS grade。3=Good（旧默认，首评 S0=w[2]）；4=Easy（首评 S0=w[3]，
    # 且成功复习 S 增长带 w[16] easy_bonus）。FSRS-6 公版 w 与 grade=4 共拟合 → 候选用 4 恢复校准
    # （amas6 = FSRS6MirrorState 即 grade=4）。失败恒映射 1=Again（FAIL_GRADE）。
    # 时间不变（仅依赖成绩）；仅影响 update() 的 grade 选择，recall()/区间求解口径不变。
    success_grade: int = SUCCESS_GRADE
    stability: float = 0.4
    difficulty: float = 5.0
    review_count: int = 0
    correct_streak: int = 0
    lapse_count: int = 0
    # per-word difficulty 先验（amas 专属预测层特征；FSRS6MirrorState 无此字段 → 天然 de-tie）。
    # 仅作为「外部每词难度」的载体由 warm_start 写入；预测期在 AMASScheduler._recall 以
    # logit 加性项消费（见该处）。不参与 S/D 更新（保持 FSRS 动力学纯净）。
    external_difficulty: float = 5.0
    # 冷启动难度先验（Phase 1a；仅首评 review_count==0 消费，逐位镜像 mdm.rs ColdStartPriors）。
    # cs_* 为每词特征（warm_start/测试注入；与 benchmark_adapter item 的 csLenZ 等一一对应）；
    # cold_start_* 系数来自 config（_mirror_from_config 透传）。全缺省 → deltas=(0,0) → bit-exact legacy。
    cs_len_z: float | None = None
    cs_morph_transparency: float | None = None
    cs_ext_difficulty: float | None = None
    cold_start_d_len_weight: float = 0.0
    cold_start_d_morph_weight: float = 0.0
    cold_start_d_extd_weight: float = 0.0
    cold_start_s_len_weight: float = 0.0
    cold_start_s_morph_weight: float = 0.0
    cold_start_s_extd_weight: float = 0.0
    cold_start_extd_ref: float = 5.0

    def recall(self, elapsed_days: float) -> float:
        power = math.pow(
            1.0 + self.forgetting_curve_factor * elapsed_days / max(self.stability, 0.01),
            self.forgetting_curve_decay,
        )
        return max(
            0.0,
            min(1.0, self.forgetting_curve_floor + (1.0 - self.forgetting_curve_floor) * power),
        )

    def _cold_start_deltas(self) -> tuple[float, float]:
        """返回 (δ_S_log, δ_D)：逐位镜像 mdm.rs::ColdStartPriors::deltas（运算结合序一致）。
        cold_start 视为"激活"当且仅当任一 cs_* 特征非 None（与 Rust Option<ColdStartPriors> 同义）；
        未激活 → (0,0)。len_z 缺失按 0.0 处理（对齐 Rust item.cs_len_z.unwrap_or(0.0)）。"""
        active = (
            self.cs_len_z is not None
            or self.cs_morph_transparency is not None
            or self.cs_ext_difficulty is not None
        )
        if not active:
            return (0.0, 0.0)
        len_z = self.cs_len_z if self.cs_len_z is not None else 0.0
        delta_d = self.cold_start_d_len_weight * len_z
        delta_s = -self.cold_start_s_len_weight * len_z
        if self.cs_morph_transparency is not None:
            t = self.cs_morph_transparency
            delta_d -= self.cold_start_d_morph_weight * t
            delta_s += self.cold_start_s_morph_weight * t
        if self.cs_ext_difficulty is not None:
            z = (self.cs_ext_difficulty - self.cold_start_extd_ref) / 9.0
            delta_d += self.cold_start_d_extd_weight * z
            delta_s -= self.cold_start_s_extd_weight * z
        return (delta_s, delta_d)

    def update(self, recalled: int, elapsed_days: float) -> None:
        """逐句镜像 mdm.rs::update_strength（quality 0.7/0.0 + 连击动态 alpha 的 adapter 重放形态）。

        alpha 镜像 mastery.rs:69-87 @ interval_scale=1.0：成功先推进连击（首评 gap 恒过、
        同日 delta_t=0 不加不清）、失败清零，再 base 夹 → ×(1+0.1·min(streak,5)) → 整体夹。
        """
        grade = self.success_grade if recalled == 1 else FAIL_GRADE
        w = self.weights
        if recalled == 1:
            gap_ok = self.review_count == 0 or elapsed_days >= self.streak_min_gap_days
            if gap_ok:
                self.correct_streak += 1
        else:
            self.correct_streak = 0
            # 累计 lapse 在 alpha 计算前自增（生产 f = total_attempts - total_correct 含本次失败）
            self.lapse_count += 1
        base_alpha = max(
            self.alpha_min, min(self.alpha_max, 1.0 * self.alpha_scale)
        )
        streak_bonus = 1.0 + min(self.correct_streak, 5) * 0.1
        alpha = max(self.alpha_min, min(self.alpha_max, base_alpha * streak_bonus))
        # mdm.rs:92 入口 alpha.clamp(0.0,1.0) 的镜像；alphaMax ≤ 1 的合法域内为 no-op，
        # 但 bench adapter 不调 validate()，alphaMax > 1 时缺此夹会与 Rust 发散
        alpha = max(0.0, min(1.0, alpha))
        if self.review_count == 0:
            # 首评（mdm.rs review_count==0 分支）：S/D 直接赋值，无 alpha 平滑。
            s0_base = w[grade - 1]
            d0_base = w[4] - math.exp(w[5] * (grade - 1.0)) + 1.0
            # 冷启动先验（Phase 1a；逐位镜像 mdm.rs）：deltas 全 0 → 走 legacy 路径（S₀ 无 clamp）。
            delta_s, delta_d = self._cold_start_deltas()
            if delta_s == 0.0 and delta_d == 0.0:
                self.stability = s0_base
                self.difficulty = max(1.0, min(10.0, d0_base))
            else:
                self.stability = max(
                    0.01, min(STABILITY_CAP_DAYS, s0_base * math.exp(delta_s))
                )
                self.difficulty = max(1.0, min(10.0, d0_base + delta_d))
        elif self.success_grade == 4:
            # —— FSRS-6 faithful 模式（success_grade==4）——
            # v5 架构：未平滑 FSRS-6 核心，逐位等价 FSRS6MirrorState（amas6 公版参考）。
            # 与旧 mdm.rs 镜像的差异：① D 先更新、S 公式用更新后的 self.difficulty（FSRS-6 标准次序）；
            # ② 无同日特化分支（FSRS-6 参考对 elapsed<1 仍走常规成功公式）。
            # 此模式仅在显式 gspSuccessGrade=4 时启用；grade==3 默认路径逐位 legacy 不变。
            # 已实证：grade=4 + 本次序 → 三数据集 pred 腿与 amas6 完全一致（G3 满余量过门）。
            r = self.recall(elapsed_days)
            # D 先更新（mean-reversion 目标锚 D0(Easy=4)）
            delta_d = -w[6] * (grade - 3.0)
            d_prime = self.difficulty + delta_d * (10.0 - self.difficulty) / 9.0
            d_target = w[4] - math.exp(w[5] * (4.0 - 1.0)) + 1.0
            self.difficulty = max(1.0, min(10.0, w[7] * d_target + (1.0 - w[7]) * d_prime))
            if grade >= 2:
                hard_penalty = w[15] if grade == 2 else 1.0
                easy_bonus = w[16] if grade == 4 else 1.0
                s_inc = max(
                    math.exp(w[8])
                    * (11.0 - self.difficulty)
                    * math.pow(max(self.stability, 0.01), -w[9])
                    * (math.exp(w[10] * (1.0 - r)) - 1.0)
                    * hard_penalty
                    * easy_bonus,
                    0.0,
                )
                self.stability = max(0.01, min(STABILITY_CAP_DAYS, self.stability * (s_inc + 1.0)))
            else:
                self.stability = max(
                    0.01,
                    min(
                        self.stability,
                        w[11]
                        * math.pow(self.difficulty, -w[12])
                        * (math.pow(self.stability + 1.0, w[13]) - 1.0)
                        * math.exp(w[14] * (1.0 - r)),
                    ),
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

            # 双腿信任调度（mastery.rs is_correct 闸门的镜像；二元数据下与 grade≥2 恒等）：
            # 成功腿挂靠连击（失败清零→全阻尼重启），失败腿挂靠累计 lapse（首错保护）。
            # 运算结合序与 Rust 表达式同序（1e-9 对拍纪律）
            if grade >= 2:
                if self.alpha_ramp_tau > 0.0:
                    # 同日成功 gap_ok=false streak 冻结；lapse 后同日成功 streak=0 → k=1 no-op
                    k = float(max(self.correct_streak, 1))
                    alpha = 1.0 - (1.0 - alpha) * math.exp(-(k - 1.0) / self.alpha_ramp_tau)
            elif self.alpha_lapse_ramp_tau > 0.0:
                f = float(max(self.lapse_count, 1))
                alpha = 1.0 - (1.0 - alpha) * math.exp(-(f - 1.0) / self.alpha_lapse_ramp_tau)

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

    def _gsp_banded_retention(self) -> float | None:
        """GSP 成熟度分带：返回替换 desired_retention 的目标保持率；关闭时返回 None。

        band>0 时按当前 stability 与 band 的关系选 young/mature 保持率（< band 用 young，
        >= band 用 mature）。两个保持率分别 clamp 到曲线合法域 (1e-6, 1.0)。
        """
        if self.gsp_maturity_band_days <= 0.0:
            return None
        target = (
            self.gsp_young_retention
            if self.stability < self.gsp_maturity_band_days
            else self.gsp_mature_retention
        )
        return max(1e-6, min(1.0, target))

    def interval_days(self) -> int:
        # 底侧 min_interval_secs=60s 在天粒度下与 max(1, ceil) 等价（mdm.rs:246-247）
        return max(1, math.ceil(self._interval_days_raw(self._gsp_banded_retention())))


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
        # 缺省 0.0（关闭）= Rust serde default，未声明旋钮的配置保持冻结语义
        "alpha_ramp_tau": float(memory_config.get("alphaRampTau", 0.0)),
        "alpha_lapse_ramp_tau": float(memory_config.get("alphaLapseRampTau", 0.0)),
    }


def _gsp_band_kwargs_from_config(memory_config: Dict[str, object]) -> Dict[str, float]:
    """GSP 成熟度分带保持率三参数（缺省 0.0=关闭=冻结旧语义）。

    镜像纪律：bench adapter 不调 validate()，故在入口对越界值夹紧——
    band 夹 [0, +inf)（负数视为关闭）、young/mature 夹 [0, 1]。
    """
    band = float(memory_config.get("gspMaturityBandDays", 0.0))
    band = max(0.0, band)
    young = max(0.0, min(1.0, float(memory_config.get("gspYoungRetention", 0.0))))
    mature = max(0.0, min(1.0, float(memory_config.get("gspMatureRetention", 0.0))))
    # 成功成绩带：缺省 3=Good（bit-exact legacy）；候选可设 4=Easy 恢复 FSRS-6 公版 w 校准。
    # 入口 clamp：非 {3,4} 一律夹回 3（合法成功成绩域；2=Hard 非二元成功语义，不开放）。
    sg = int(memory_config.get("gspSuccessGrade", SUCCESS_GRADE))
    sg = sg if sg in (3, 4) else SUCCESS_GRADE
    return {
        "gsp_maturity_band_days": band,
        "gsp_young_retention": young,
        "gsp_mature_retention": mature,
        "success_grade": sg,
    }


def _cold_start_kwargs_from_config(memory_config: Dict[str, object]) -> Dict[str, float]:
    """冷启动先验系数（Phase 1a；逐位镜像 Rust MemoryModelConfig 同名字段）。
    缺省全 0 / extd_ref=5.0（bit-exact legacy）。cs_* 每词特征不在此处（由 warm_start/测试按词
    注入 mirror 实例，与 benchmark_adapter item 的 csLenZ 等对应）。"""
    return {
        "cold_start_d_len_weight": float(memory_config.get("coldStartDLenWeight", 0.0)),
        "cold_start_d_morph_weight": float(memory_config.get("coldStartDMorphWeight", 0.0)),
        "cold_start_d_extd_weight": float(memory_config.get("coldStartDExtdWeight", 0.0)),
        "cold_start_s_len_weight": float(memory_config.get("coldStartSLenWeight", 0.0)),
        "cold_start_s_morph_weight": float(memory_config.get("coldStartSMorphWeight", 0.0)),
        "cold_start_s_extd_weight": float(memory_config.get("coldStartSExtdWeight", 0.0)),
        "cold_start_extd_ref": float(memory_config.get("coldStartExtdRef", 5.0)),
    }


def _mirror_from_config(memory_config: Dict[str, object]) -> WordforgeMirrorState:
    weights = list(memory_config["w"])
    # 顶侧间隔夹紧与 Rust default_max_interval_days() 同默认（90 天）
    max_interval_days = float(memory_config.get("maxIntervalDays", 90.0))
    alpha_kwargs = _alpha_kwargs_from_config(memory_config)
    # success_grade（成功成绩带，缺省 3=bit-exact legacy）随 GSP band 入口 helper 夹紧后透传，
    # 保证 _mirror_from_config 路径与 AMASScheduler._fresh_state 路径口径一致。
    grade_kwargs = {"success_grade": _gsp_band_kwargs_from_config(memory_config)["success_grade"]}
    cold_start_kwargs = _cold_start_kwargs_from_config(memory_config)
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
            **grade_kwargs,
            **cold_start_kwargs,
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
        **grade_kwargs,
        **cold_start_kwargs,
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
    mastered_proxy = 0
    for item in items:
        if item.halflife is None or item.last_date is None:
            continue
        next_day_memory += math.pow(2.0, -(LEARN_DAYS - item.last_date) / item.halflife)
        if item.halflife >= TARGET_HALFLIFE:
            target_count += 1
        # 镜像 simulate.py mastered_count 语义：调度器自报终态 next-interval >= 30 天
        # （闭环内引入即复习，last_date is not None == 至少复习过一次）
        if item.mirror.interval_days() >= 30:
            mastered_proxy += 1
    return {
        "expectedMemory": expected_memory,
        "nextDayMemory": next_day_memory,
        "targetCount": target_count,
        "masteredProxy": mastered_proxy,
        "avgDueRecall": sum(due_recall_series) / len(due_recall_series),
    }
