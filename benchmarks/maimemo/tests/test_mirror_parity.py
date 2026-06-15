"""镜像 × Rust adapter 数值对等测试（spec: mirror-alignment round 2, 2026-06-10）。

200 个种子随机用例：每例独立 21 维 w（DEFAULT × U(0.5,1.5)，w[20] 钳 [0.05,2.0]）、
历史长度 1–20、逐步 delta_t ∈ {0..60}（含 0 → 覆盖同日分支）、二元 r、三档 retention。
另有 60 个 19 维 FSRS-5 旧权重用例（Rust 侧走 w19=0/w20=0.5 迁移路径）+ 1 个
定向回归例，覆盖同日 G≥3 下限在 19 维分支的对等。
Python 侧 WordforgeMirrorState 与 Rust 侧 maimemo_mdm_adapter（mdm.rs 真值，
quality 0.7/0.0、alpha 由 config+history 派生的连击动态 alpha —— 不再固定 0.3）
各自重放同一历史：

- stability / difficulty：abs ≤ 1e-9 + 1e-9·|rust|
- interval（retention 0.85 / 0.90）：复刻 mdm.rs compute_interval 的秒级管线
  （×86400 → `as i64` 向零截断 → max(60s)）后相对误差 ≤ 1e-6；
  仅在 adapter interval ≥ 1 天时断言（60s 底侧地板 vs 镜像 1 天地板的语义差）。

注意：parity 驱动传 RAW delta（含 0），不得套用闭环模拟器的 max(delta,1) 地板。
"""
from __future__ import annotations

import json
import math
import random

from benchmarks.maimemo.adapter import AdapterServer
from benchmarks.maimemo.config import DEFAULT_MEMORY_MODEL_CONFIG, FSRS_BASELINE_CONFIG
from benchmarks.maimemo.dhp_reference import (
    _gsp_band_kwargs_from_config,
    _mirror_from_config,
)

CASE_COUNT = 200
LEGACY_CASE_COUNT = 60
RETENTION_CHOICES = (0.85, 0.90, 0.92)
INTERVAL_RETENTIONS = (0.85, 0.90)
STATE_TOL = 1e-9
INTERVAL_REL_TOL = 1e-6

# FSRS-6 公版 21 维 w（GSP_SPEC §6 / config.py DEFAULT 同源）—— GSP 族基础态。
_FSRS6_W = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001,
    1.8722, 0.1666, 0.796, 1.4835, 0.0614, 0.2629, 1.6483, 0.6014,
    1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
]

# F1 船值（CONTEXT / config.py DEFAULT / amas_config.toml v5 三处同源）。
# 这是进入 TEST one-shot 的精确配置：GSP 调度头全激活（cap40/streak2/floor30/band14/grade4）。
# 以 DEFAULT 为底叠加显式船值键——既保证 Rust MemoryModelConfig 反序列化所需的全字段齐备
# （shortTermLearningRate 等无 serde default），又精确钉死所有 v5 船值键。下方 assert 守口径
# 与 config.py DEFAULT 完全一致（任一处漂移即本模块加载即报错）。
_F1_SHIP_KEYS = {
    "alphaScale": 1.0, "alphaMin": 1.0, "alphaMax": 1.0,
    "alphaRampTau": 0.0, "alphaLapseRampTau": 0.0,
    "gspSuccessGrade": 4,
    "gspIntervalCapDays": 40.0,
    "gspGraduationStreak": 2,
    "gspGraduationFloorDays": 30.0,
    "gspYoungRetention": 0.86,
    "gspMatureRetention": 0.92,
    "gspMaturityBandDays": 14.0,
    "gspIntervalFuzz": 0.0,
    "difficultyLogitWeight": 0.1,
    "difficultyLogitRef": 5.0,
    "w": list(_FSRS6_W),
    "baseDesiredRetention": 0.85,
    "forgettingCurveDecay": -0.1542,
    "forgettingCurveFloor": 0.0,
    "forgettingCurveFactor": 0.9 ** (-1.0 / 0.1542) - 1.0,
}
F1_SHIP_CONFIG = {**json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG)), **_F1_SHIP_KEYS}

# 口径守卫：船值键须与 config.py DEFAULT_MEMORY_MODEL_CONFIG 逐键一致（三处同源）。
for _k, _v in _F1_SHIP_KEYS.items():
    if _k == "w":
        assert [float(x) for x in DEFAULT_MEMORY_MODEL_CONFIG[_k]] == [float(x) for x in _v], (
            f"F1 ship w 漂移：DEFAULT vs test_mirror_parity"
        )
    else:
        assert float(DEFAULT_MEMORY_MODEL_CONFIG[_k]) == float(_v), (
            f"F1 ship key {_k} 漂移：DEFAULT={DEFAULT_MEMORY_MODEL_CONFIG[_k]} vs ship={_v}"
        )


# ===========================================================================
# GSP 调度策略头 Python 参考（逐位镜像 mdm.rs::gsp_schedule_days，契约 GSP_SPEC §3 + §7）
#
# 注意：本参考对齐的是 **benchmark_adapter.rs::evaluate_batch 的 scheduled_interval_days 路径**
# （interval_scale 由 item 透传、非 ensemble 动态），而非 schedulers.py::AMASScheduler.next_interval_days
# （后者注入 ensemble 动态 scale，不可对拍）。adapter 是 Rust↔Python parity 的唯一桥。
#
# base 来源：_mirror_from_config 不透传 banded 三参数（它是 state-update 镜像，banded 仅影响
# interval 求解），故此处显式把 band kwargs 注入 mirror 后再读 interval_days()——与 adapter 侧
# gsp_banded_retention + compute_interval_base_days 同口径（band>0 时替换 desired_retention）。
# ===========================================================================


def _gsp_fuzz_u(stability: float, review_count: int) -> float:
    """GSP_SPEC §7.2 确定性抖动量 u ∈ [-1,1)，逐位镜像 mdm.rs::gsp_fuzz_u（去 sin，f64 同序）。"""
    h = stability * 12.9898 + float(review_count) * 78.233
    return 2.0 * (h - math.floor(h)) - 1.0


def _apply_band_kwargs(mirror, config: dict) -> None:
    """把 GSP banded retention 三参数注入 mirror（_mirror_from_config 不透传）。

    入口 clamp 经 dhp 侧 `_gsp_band_kwargs_from_config` 完成（与 Rust gsp_banded_retention 同纪律）。
    仅影响 interval_days() 的目标保持率求解；update()/recall() 不读这些字段。
    """
    bk = _gsp_band_kwargs_from_config(config)
    mirror.gsp_maturity_band_days = bk["gsp_maturity_band_days"]
    mirror.gsp_young_retention = bk["gsp_young_retention"]
    mirror.gsp_mature_retention = bk["gsp_mature_retention"]


def _gsp_schedule_days_ref(
    mirror, interval_scale: float, correct_streak: int, config: dict
) -> int:
    """逐位镜像 mdm.rs::gsp_schedule_days（GSP_SPEC §3 步骤 3–6 + §7）。

    base_days_int = mirror.interval_days()（已含 banded retention 求解 + ceil + max(1)）。
    运算次序严格不可交换；取整用 Python3 round（banker's = round-half-to-even = Rust 侧
    round_half_to_even），底侧夹 max(1)。
    """
    base_days_int = mirror.interval_days()
    cap_cfg = max(0.0, float(config.get("gspIntervalCapDays", 0.0)))
    streak_k = max(0, int(config.get("gspGraduationStreak", 0)))
    floor_d = max(0.0, float(config.get("gspGraduationFloorDays", 30.0)))
    fuzz = max(0.0, min(1.0 - 1e-9, float(config.get("gspIntervalFuzz", 0.0))))
    max_ivl = float(config.get("maxIntervalDays", 90.0))

    # 步骤 3：interval_scale（adapter 透传值；.max(0.1)，无上界、无分带）
    scaled_days = max(1.0, base_days_int * max(0.1, interval_scale))

    # 步骤 4：毕业下限（scale 之后、cap 之前）
    graduated = streak_k > 0 and correct_streak >= streak_k
    if graduated:
        scaled_days = max(scaled_days, floor_d)

    # 步骤 5：区间帽 min(90, gspCap)
    cap = max_ivl
    if cap_cfg > 0.0:
        cap = min(cap, cap_cfg)
    scaled_days = min(scaled_days, cap)

    # 步骤 5.5：区间抖动（cap 之后、取整之前；fuzz=0 即 no-op）
    if fuzz > 0.0:
        u = _gsp_fuzz_u(mirror.stability, mirror.review_count)
        fuzzed = scaled_days * (1.0 + fuzz * u)
        fuzz_cap = min(max_ivl, cap * (1.0 + fuzz))
        lo = floor_d if graduated else 1.0
        scaled_days = max(lo, min(fuzz_cap, fuzzed))

    # 步骤 6：取整 + 底侧夹（banker's）
    return max(1, int(round(scaled_days)))


def _assert_gsp_parity(server: AdapterServer, config: dict, case: dict, label: str):
    """单条历史双侧重放：S/D 对拍（≤1e-9）+ GSP 调度头区间对拍（整数恒等）。

    返回 (state 最大绝对误差, 区间绝对差)。GSP 激活时 adapter 必返 scheduledIntervalDays，
    Python 参考 _gsp_schedule_days_ref 须与之**整数恒等**（GSP head 全程 op-order 逐位）。
    """
    config = json.loads(json.dumps(config))
    config["baseDesiredRetention"] = float(case.get("retention", 0.85))
    config["forgettingCurveFloor"] = 0.0

    # Python 侧：state 镜像（S/D）+ band 注入后的 interval 镜像
    mirror = _mirror_from_config(config)
    _apply_band_kwargs(mirror, config)
    for recalled, delta in zip(case["r"], case["t"]):
        mirror.update(recalled, float(delta))

    interval_scale = float(case.get("intervalScale", 1.0))
    [scored] = server.score_batch(
        config,
        [
            {
                "tHistory": case["t"],
                "rHistory": case["r"],
                "targetRetentions": list(INTERVAL_RETENTIONS),
                "intervalScale": interval_scale,
            }
        ],
    )

    # 终态 review_count / correct_streak 对拍
    assert mirror.review_count == scored["reviewCount"], label
    assert mirror.correct_streak == scored["correctStreak"], (
        f"{label}: correct_streak {mirror.correct_streak} vs {scored['correctStreak']}"
    )

    # S/D 对拍（≤1e-9 + 相对项）
    s_err = abs(mirror.stability - scored["stability"])
    d_err = abs(mirror.difficulty - scored["difficulty"])
    assert s_err <= STATE_TOL + STATE_TOL * abs(scored["stability"]), (
        f"{label}: stability {mirror.stability!r} vs {scored['stability']!r}"
    )
    assert d_err <= STATE_TOL + STATE_TOL * abs(scored["difficulty"]), (
        f"{label}: difficulty {mirror.difficulty!r} vs {scored['difficulty']!r}"
    )

    # GSP 调度头区间对拍（整数恒等）
    ref_days = _gsp_schedule_days_ref(
        mirror, interval_scale, scored["correctStreak"], config
    )
    rust_days = scored["scheduledIntervalDays"]
    assert rust_days is not None, f"{label}: GSP 激活但 adapter 返回 scheduledIntervalDays=None"
    assert ref_days == rust_days, (
        f"{label}: GSP head interval {ref_days} (py) vs {rust_days} (rust); "
        f"S={mirror.stability!r} rc={mirror.review_count} streak={scored['correctStreak']}"
    )
    return max(s_err, d_err), abs(ref_days - rust_days)


def _generate_cases() -> list[dict]:
    rng = random.Random(42)
    base_w = [float(v) for v in DEFAULT_MEMORY_MODEL_CONFIG["w"]]
    cases = []
    for _ in range(CASE_COUNT):
        w = [value * rng.uniform(0.5, 1.5) for value in base_w]
        w[20] = max(0.05, min(2.0, w[20]))
        length = rng.randint(1, 20)
        cases.append(
            {
                "w": w,
                "t": [rng.randint(0, 60) for _ in range(length)],
                "r": [rng.randint(0, 1) for _ in range(length)],
                "retention": rng.choice(RETENTION_CHOICES),
            }
        )
    return cases


def _case_config(case: dict) -> dict:
    config = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
    config["w"] = case["w"]
    config["baseDesiredRetention"] = case["retention"]
    config["forgettingCurveFloor"] = 0.0
    return config


def _generate_legacy_cases() -> list[dict]:
    """19 维旧权重用例：deltas 取 {0..5} 提高同日密度；奇数例翻转 w[17] 符号，
    制造同日成功 s_short < prev_S，专门覆盖 G≥3 下限（修复前 19 维分支缺失）。"""
    rng = random.Random(20260610)
    base_w = [float(v) for v in FSRS_BASELINE_CONFIG["w"]]
    assert len(base_w) == 19
    cases = []
    for index in range(LEGACY_CASE_COUNT):
        w = [value * rng.uniform(0.5, 1.5) for value in base_w]
        if index % 2 == 1:
            w[17] = -w[17]
        length = rng.randint(2, 20)
        cases.append(
            {
                "w": w,
                "t": [rng.randint(0, 5) for _ in range(length)],
                "r": [rng.randint(0, 1) for _ in range(length)],
                "retention": rng.choice(RETENTION_CHOICES),
            }
        )
    return cases


def _legacy_case_config(case: dict) -> dict:
    # 镜像 19 维分支读显式曲线键（factor 19/81、decay -0.5）；Rust 侧曲线由迁移后
    # w20=0.5 派生（curve_factor = 0.9^-2 − 1 ≈ 19/81），两侧数值一致
    config = json.loads(json.dumps(FSRS_BASELINE_CONFIG))
    config["w"] = case["w"]
    config["baseDesiredRetention"] = case["retention"]
    config["forgettingCurveFloor"] = 0.0
    return config


def _assert_case_parity(
    server: AdapterServer, config: dict, case: dict, label: str
) -> tuple[float, int]:
    """单条历史双侧重放 + S/D/interval 断言；返回 (state 最大绝对误差, interval 比较数)。"""
    # Python 侧：传 RAW delta（含 0），不做 max(delta,1) 地板
    mirror = _mirror_from_config(config)
    for recalled, delta in zip(case["r"], case["t"]):
        mirror.update(recalled, float(delta))

    # Rust 侧：同一历史经 replay_history（quality 0.7/0.0，alpha 0.3）
    [scored] = server.score_batch(
        config,
        [
            {
                "tHistory": case["t"],
                "rHistory": case["r"],
                "targetRetentions": list(INTERVAL_RETENTIONS),
                "intervalScale": 1.0,
            }
        ],
    )

    assert mirror.review_count == scored["reviewCount"], label
    s_err = abs(mirror.stability - scored["stability"])
    d_err = abs(mirror.difficulty - scored["difficulty"])
    assert s_err <= STATE_TOL + STATE_TOL * abs(scored["stability"]), (
        f"{label}: stability {mirror.stability!r} vs {scored['stability']!r}"
    )
    assert d_err <= STATE_TOL + STATE_TOL * abs(scored["difficulty"]), (
        f"{label}: difficulty {mirror.difficulty!r} vs {scored['difficulty']!r}"
    )

    interval_compared = 0
    for entry in scored["intervals"]:
        adapter_days = float(entry["intervalDays"])
        if adapter_days < 1.0:
            # adapter 底侧地板是 60s，镜像是 1 天 —— 仅在 ≥1 天区间断言
            continue
        raw_days = mirror._interval_days_raw(float(entry["retention"]))
        # 复刻 mdm.rs compute_interval 秒级管线：×86400 → as i64 截断 → max(60)
        mirror_secs = max(int(raw_days * 86400.0), 60)
        mirror_days = mirror_secs / 86400.0
        assert abs(mirror_days - adapter_days) <= INTERVAL_REL_TOL * abs(adapter_days), (
            f"{label} retention {entry['retention']}: {mirror_days!r} vs {adapter_days!r}"
        )
        interval_compared += 1
    return max(s_err, d_err), interval_compared


def test_mirror_matches_rust_adapter():
    cases = _generate_cases()
    # 同日分支覆盖自检：首步走 review_count==0 分支，故要求 step>=1 处存在 delta_t==0；
    # 同时要求同日成功与同日失败都被覆盖（seed=42 下确定成立）
    same_day_success = same_day_failure = 0
    for case in cases:
        for step, (delta, recalled) in enumerate(zip(case["t"], case["r"])):
            if step >= 1 and delta == 0:
                if recalled == 1:
                    same_day_success += 1
                else:
                    same_day_failure += 1
    assert same_day_success > 0 and same_day_failure > 0

    max_state_err = 0.0
    interval_compared = 0
    with AdapterServer() as server:
        for index, case in enumerate(cases):
            err, compared = _assert_case_parity(
                server, _case_config(case), case, f"case {index}"
            )
            max_state_err = max(max_state_err, err)
            interval_compared += compared

    # 至少应有大量 ≥1 天的 interval 被实际比较（防御性：避免全 continue 形成空断言）
    assert interval_compared >= CASE_COUNT
    # 诊断输出：-s 模式下可见的最大误差
    print(
        f"\nmirror parity: {CASE_COUNT} cases, max state abs err = {max_state_err:.3e}, "
        f"intervals compared = {interval_compared}"
    )


def test_dynamic_alpha_streak_semantics():
    """纯 Python 侧连击语义自检：首评恒过、同日不加不清、失败清零。"""
    mirror = _mirror_from_config(DEFAULT_MEMORY_MODEL_CONFIG)
    mirror.update(1, 0.0)  # 首评：gap_ok 恒真 → streak 1
    assert mirror.correct_streak == 1
    mirror.update(1, 0.0)  # 同日成功：gap 0 < 30min → 不加不清
    assert mirror.correct_streak == 1
    mirror.update(1, 1.0)  # 跨日成功 → 2
    assert mirror.correct_streak == 2
    mirror.update(0, 1.0)  # 失败清零
    assert mirror.correct_streak == 0


def _dynamic_alpha_cases() -> list[tuple[str, dict, dict]]:
    """连击动态 alpha 定向族 (a)-(d)：返回 (label, config, case)。

    (a) ≥7 连续成功 —— streak 饱和 min(streak,5)；
    (b) 失败穿插 —— 清零后重爬坡；
    (c) 混入 delta_t=0 同日条目 —— gap 规则不加不清；
    (d) alphaScale=0.45 —— base×bonus 撞 alphaMax 夹紧。
    """
    base = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
    boosted = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
    boosted["alphaScale"] = 0.45
    families: list[tuple[str, dict, dict]] = [
        ("a_saturation", base, {"t": [1] * 10, "r": [1] * 10}),
        ("b_reset_reramp", base, {"t": [1] * 12, "r": [1, 1, 1, 0, 1, 1, 0, 1, 1, 1, 1, 1]}),
        ("c_same_day_mix", base, {"t": [0, 0, 1, 0, 1, 1, 0, 2, 0, 1], "r": [1, 1, 1, 1, 0, 1, 1, 1, 1, 1]}),
        ("d_alpha_max_clamp", boosted, {"t": [1] * 8, "r": [1] * 8}),
    ]
    # 每族追加 5 个种子随机变体（含随机 w 扰动，保持族特征）
    rng = random.Random(20260611)
    base_w = [float(v) for v in DEFAULT_MEMORY_MODEL_CONFIG["w"]]
    out: list[tuple[str, dict, dict]] = []
    for label, config, case in families:
        out.append((label, config, dict(case, retention=0.90)))
        for k in range(5):
            w = [value * rng.uniform(0.5, 1.5) for value in base_w]
            w[20] = max(0.05, min(2.0, w[20]))
            cfg = json.loads(json.dumps(config))
            cfg["w"] = w
            if label == "a_saturation":
                t = [rng.randint(1, 4) for _ in range(rng.randint(7, 12))]
                r = [1] * len(t)
            elif label == "b_reset_reramp":
                t = [rng.randint(1, 4) for _ in range(12)]
                r = [1] * 12
                for pos in rng.sample(range(1, 11), 2):
                    r[pos] = 0
            elif label == "c_same_day_mix":
                t = [rng.choice([0, 0, 1, 2]) for _ in range(rng.randint(8, 14))]
                r = [rng.randint(0, 1) for _ in t]
            else:  # d_alpha_max_clamp
                t = [rng.randint(1, 3) for _ in range(rng.randint(6, 10))]
                r = [1] * len(t)
            out.append((f"{label}_rnd{k}", cfg, {"t": t, "r": r, "retention": 0.90}))

    # (e) alphaScale < alphaMin —— 内层 base clamp 判别配置：无内夹时 alpha 恒 0.1
    # （0.05×bonus ≤ 0.075 → 外夹拉回 0.1），有内夹则 0.11..0.15
    floored = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
    floored["alphaScale"] = 0.05
    out.append(("e_base_below_min", floored, {"t": [1] * 8, "r": [1] * 8, "retention": 0.90}))
    out.append((
        "e_base_below_min_mix",
        floored,
        {"t": [1, 0, 1, 2, 1, 1, 0, 1, 1, 1], "r": [1, 1, 1, 0, 1, 1, 1, 1, 0, 1], "retention": 0.90},
    ))
    # (g) alphaMax > 1 —— mdm.rs:92 入口 [0,1] clamp 判别配置（base×bonus 可达 1.35）；
    # bench adapter 不调 validate()，缺镜像入口夹时此族会发散
    over_unit = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
    over_unit["alphaScale"] = 0.9
    over_unit["alphaMax"] = 1.5
    out.append(("g_entry_unit_clamp", over_unit, {"t": [1] * 8, "r": [1] * 8, "retention": 0.90}))
    out.append((
        "g_entry_unit_clamp_mix",
        over_unit,
        {"t": [1, 1, 0, 1, 2, 1, 1, 0, 1, 1], "r": [1, 1, 1, 1, 0, 1, 1, 1, 1, 1], "retention": 0.90},
    ))
    return out


def test_mirror_dynamic_alpha_families():
    max_state_err = 0.0
    with AdapterServer() as server:
        for label, config, case in _dynamic_alpha_cases():
            config = json.loads(json.dumps(config))
            config["baseDesiredRetention"] = float(case.get("retention", 0.90))
            config["forgettingCurveFloor"] = 0.0
            err, _ = _assert_case_parity(server, config, case, f"dyn-alpha {label}")
            max_state_err = max(max_state_err, err)
    print(f"\ndynamic-alpha parity: max state abs err = {max_state_err:.3e}")


def test_dual_trust_lapse_semantics():
    """纯 Python 侧双腿语义自检：首错 no-op、lapse 累计不清零、elif 互斥。

    位级断言用 dyadic alphaScale=0.25：失败步 streak=0 → alpha=0.25 精确，
    1-(1-α) round-trip 无 1 ULP 误差（0.3 不满足，IEEE 固有）。
    """
    frozen_cfg = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
    frozen_cfg["alphaScale"] = 0.25
    # DEFAULT 已写回 v5 GSP 船值（gspSuccessGrade=4 + alpha 钉死 1.0）：grade=4 走 FSRS-6
    # faithful 路径（无 alpha 平滑），双腿信任调度仅作用于 grade=3 legacy 平滑路径。
    # 本测试自检 legacy 双腿语义，故显式钉 gspSuccessGrade=3 + 恢复连击 alpha 区间 + 归零双腿基线。
    frozen_cfg["gspSuccessGrade"] = 3
    frozen_cfg["alphaMin"] = 0.1
    frozen_cfg["alphaMax"] = 0.5
    frozen_cfg["alphaRampTau"] = 0.0
    frozen_cfg["alphaLapseRampTau"] = 0.0
    config = json.loads(json.dumps(frozen_cfg))
    config["alphaLapseRampTau"] = 6.0

    # 首错 f=1 ⇒ e^0=1 ⇒ 与冻结语义逐位一致（偶发失误保护）
    frozen = _mirror_from_config(frozen_cfg)
    ramped = _mirror_from_config(config)
    for recalled, delta in [(1, 0.0), (1, 1.0), (0, 2.0)]:
        frozen.update(recalled, delta)
        ramped.update(recalled, delta)
    assert ramped.lapse_count == 1
    assert ramped.stability == frozen.stability
    assert ramped.difficulty == frozen.difficulty

    # 第二次失败 f=2 实化；后续成功不清 lapse 累计（与 streak 不同），
    # 且成功腿关闭（tau_s=0）时成功步永不吃失败腿 ramp（elif 互斥）
    frozen.update(0, 1.0)
    ramped.update(0, 1.0)
    assert ramped.lapse_count == 2
    assert ramped.stability != frozen.stability
    state_after_fail = (ramped.stability, ramped.difficulty)
    ramped.update(1, 1.0)
    assert ramped.lapse_count == 2  # 成功不清 lapse
    # 成功步 alpha 未被失败腿污染：把同状态用冻结配置重放一步对照
    probe = _mirror_from_config(frozen_cfg)
    probe.stability, probe.difficulty = state_after_fail
    probe.review_count = 4
    probe.correct_streak = 0
    probe.lapse_count = 2
    probe.update(1, 1.0)
    assert probe.stability == ramped.stability
    assert probe.difficulty == ramped.difficulty


def _dual_trust_case(rng: random.Random, fail_heavy: bool) -> dict:
    """长混合历史；fail_heavy=失败占优（lapse 腿密集触发），否则成功占优。"""
    length = rng.randint(12, 20)
    weights = [0, 0, 0, 1] if fail_heavy else [0, 1, 1, 1]
    return {
        "t": [rng.choice([0, 1, 1, 2, 3, 7]) for _ in range(length)],
        "r": [rng.choice(weights) for _ in range(length)],
    }


def test_mirror_dual_trust_ramp_families():
    """(f) 双腿信任调度族 —— Rust↔Python 1e-9 对拍。

    - tau=(0,0)：冻结语义逐位回归（默认关闭）
    - 成功腿单开 tau_s ∈ {2.5, 5.0}：混合历史（streak 挂靠 + 失败清零重启）
    - 失败腿单开 tau_f ∈ {2.5, 6.0}：失败占优历史 n∈[12,20]（首错 no-op + leech 实化）
    - 双开 (3.0, 6.0) / (3.0, 5.0)：候选 ship 值（elif 互斥路径全覆盖）
    - 19 维 legacy × 双开：legacy 迁移路径（w19=0/w20=0.5）与双腿的正交性
    """
    rng = random.Random(20260612)
    base_w = [float(v) for v in DEFAULT_MEMORY_MODEL_CONFIG["w"]]
    families: list[tuple[str, float, float, bool]] = [
        ("frozen", 0.0, 0.0, False),
        ("success_2.5", 2.5, 0.0, False),
        ("success_5.0", 5.0, 0.0, False),
        ("lapse_2.5", 0.0, 2.5, True),
        ("lapse_6.0", 0.0, 6.0, True),
        ("dual_3_6", 3.0, 6.0, False),
        ("dual_3_6_failheavy", 3.0, 6.0, True),
        ("dual_3_5", 3.0, 5.0, True),
    ]
    max_state_err = 0.0
    with AdapterServer() as server:
        for label, tau_s, tau_f, fail_heavy in families:
            for k in range(6):
                w = [value * rng.uniform(0.5, 1.5) for value in base_w]
                w[20] = max(0.05, min(2.0, w[20]))
                config = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
                config["w"] = w
                config["alphaRampTau"] = tau_s
                config["alphaLapseRampTau"] = tau_f
                config["baseDesiredRetention"] = 0.90
                config["forgettingCurveFloor"] = 0.0
                case = _dual_trust_case(rng, fail_heavy)
                err, _ = _assert_case_parity(
                    server, config, case, f"dual-trust {label} case {k}"
                )
                max_state_err = max(max_state_err, err)

        # 19 维 legacy × 双开：legacy 迁移路径与双腿 ramp 的正交性对拍
        legacy_base_w = [float(v) for v in FSRS_BASELINE_CONFIG["w"]]
        for k in range(4):
            config = json.loads(json.dumps(FSRS_BASELINE_CONFIG))
            config["w"] = [value * rng.uniform(0.5, 1.5) for value in legacy_base_w]
            config["alphaRampTau"] = 3.0
            config["alphaLapseRampTau"] = 6.0
            config["baseDesiredRetention"] = 0.90
            config["forgettingCurveFloor"] = 0.0
            case = _dual_trust_case(rng, fail_heavy=(k % 2 == 1))
            err, _ = _assert_case_parity(
                server, config, case, f"dual-trust legacy case {k}"
            )
            max_state_err = max(max_state_err, err)
    print(f"\ndual-trust parity: max state abs err = {max_state_err:.3e}")


def test_mirror_matches_rust_adapter_legacy_19dim():
    cases = _generate_legacy_cases()
    # 自检：负 w17 例中存在 step>=1 的同日成功（G≥3 下限实际触发点；seed 下确定成立）
    floor_hits = sum(
        1
        for idx, case in enumerate(cases)
        if idx % 2 == 1
        for step, (delta, recalled) in enumerate(zip(case["t"], case["r"]))
        if step >= 1 and delta == 0 and recalled == 1
    )
    assert floor_hits > 0

    max_state_err = 0.0
    interval_compared = 0
    with AdapterServer() as server:
        # 定向回归（review 实测发散例）：w17=-0.5、t=[0,0,0]、r=[1,1,1] —— 修复前
        # 镜像 S≈2.659 vs Rust S=3.173（19 维分支漏掉同日 G≥3 下限）
        targeted = json.loads(json.dumps(FSRS_BASELINE_CONFIG))
        targeted_w = list(targeted["w"])
        targeted_w[17] = -0.5
        targeted["w"] = targeted_w
        targeted["forgettingCurveFloor"] = 0.0
        err, compared = _assert_case_parity(
            server, targeted, {"t": [0, 0, 0], "r": [1, 1, 1]}, "legacy targeted"
        )
        max_state_err = max(max_state_err, err)
        interval_compared += compared

        for index, case in enumerate(cases):
            err, compared = _assert_case_parity(
                server, _legacy_case_config(case), case, f"legacy case {index}"
            )
            max_state_err = max(max_state_err, err)
            interval_compared += compared

    assert interval_compared >= LEGACY_CASE_COUNT // 2
    print(
        f"\nlegacy 19-dim parity: {LEGACY_CASE_COUNT}+1 cases, "
        f"max state abs err = {max_state_err:.3e}, intervals compared = {interval_compared}"
    )


# ===========================================================================
# GSP 调度策略头 Rust↔Python 对拍判别族（contract lock，campaign 2026-06-13 v5）
#
# 六族对齐 TASK 1(a)-(f)：
#   (a) frozen-default       —— GSP 全关 / grade=3 → bit-exact legacy（回归网）
#   (b) grade-4 regime       —— gspSuccessGrade=4 随机序列，FSRS-6 faithful S/D 对拍
#   (c) graduation           —— 跨 correct_streak k 边界，区间含 floor 对拍
#   (d) cap + banded         —— S 轨迹跨 band 切换 + 撞 cap，区间对拍
#   (e) fuzz                 —— gspIntervalFuzz=0.15，§7.2 区间对拍（整数恒等）
#   (f) F1-ship              —— 精确船值 × 长混合序列（守 TEST one-shot 的族）
#
# 全部经 _assert_gsp_parity：S/D ≤1e-9 + GSP head 整数恒等。
# ===========================================================================


def _rand_w(rng: random.Random, base_w: list[float]) -> list[float]:
    """DEFAULT × U(0.5,1.5) 扰动，w[20]（曲线 decay）钳 [0.05,2.0] 防域错误。"""
    w = [value * rng.uniform(0.5, 1.5) for value in base_w]
    w[20] = max(0.05, min(2.0, w[20]))
    return w


def test_gsp_parity_a_frozen_default():
    """(a) frozen-default：GSP 全关 / gspSuccessGrade=3 → adapter 不产 head（None），
    且 S/D 与 grade=3 legacy 路径逐位一致（Python↔Rust ≤1e-9）。回归网：证明 GSP 代码
    路径在全关时对状态/区间零影响。"""
    rng = random.Random(20260613)
    base_w = [float(v) for v in _FSRS6_W]
    max_state_err = 0.0
    checked = 0
    with AdapterServer() as server:
        for _ in range(80):
            config = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
            config["w"] = _rand_w(rng, base_w)
            config["baseDesiredRetention"] = rng.choice(RETENTION_CHOICES)
            config["forgettingCurveFloor"] = 0.0
            # GSP 全关 + grade=3（legacy）
            config.update({
                "gspSuccessGrade": 3, "gspIntervalCapDays": 0.0,
                "gspGraduationStreak": 0, "gspMaturityBandDays": 0.0,
                "gspIntervalFuzz": 0.0,
            })
            length = rng.randint(1, 20)
            case = {
                "t": [rng.randint(0, 60) for _ in range(length)],
                "r": [rng.randint(0, 1) for _ in range(length)],
            }
            # 复用 legacy S/D 对拍管线（grade=3 = SUCCESS_QUALITY=0.7 = bit-exact legacy）
            err, _ = _assert_case_parity(server, config, case, "gsp-a-frozen")
            max_state_err = max(max_state_err, err)
            # adapter 在 GSP 全关时不产 head
            [scored] = server.score_batch(
                config,
                [{"tHistory": case["t"], "rHistory": case["r"],
                  "targetRetentions": [0.85], "intervalScale": 1.0}],
            )
            assert scored["scheduledIntervalDays"] is None, (
                f"GSP 全关应不产 head，实得 {scored['scheduledIntervalDays']}"
            )
            checked += 1
    assert checked == 80
    print(f"\ngsp-a frozen-default parity: {checked} cases, max state abs err = {max_state_err:.3e}")


def test_gsp_parity_b_grade4_regime():
    """(b) grade-4 regime：gspSuccessGrade=4（FSRS-6 faithful）随机序列，S/D 对拍 ≤1e-9。
    至少开一个 GSP 调度旋钮（cap）使 adapter 产 head，附带 head 对拍。"""
    rng = random.Random(20260614)
    base_w = [float(v) for v in _FSRS6_W]
    max_state_err = 0.0
    with AdapterServer() as server:
        for _ in range(80):
            config = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
            config["w"] = _rand_w(rng, base_w)
            config.update({
                "gspSuccessGrade": 4,
                "gspIntervalCapDays": 40.0,   # 开 cap → head 激活
                "gspGraduationStreak": 0, "gspMaturityBandDays": 0.0,
                "gspIntervalFuzz": 0.0,
            })
            length = rng.randint(1, 20)
            case = {
                "t": [rng.choice([0, 1, 2, 3, 7, 14, 30, 60]) for _ in range(length)],
                "r": [rng.randint(0, 1) for _ in range(length)],
                "retention": rng.choice(RETENTION_CHOICES),
            }
            err, _ = _assert_gsp_parity(server, config, case, "gsp-b-grade4")
            max_state_err = max(max_state_err, err)
    print(f"\ngsp-b grade4-regime parity: max state abs err = {max_state_err:.3e}")


def test_gsp_parity_c_graduation():
    """(c) graduation：序列跨 correct_streak k 边界，区间含 floor 对拍。

    定向例（k=2/3，连续成功跨阈值；失败穿插后重爬坡）+ 随机变体。判别力：grade=3 保持
    低 S（避免 grade=4 增长过快令 base 区间天然 >=floor 掩盖 floor 触发）。"""
    rng = random.Random(20260615)
    base_w = [float(v) for v in _FSRS6_W]
    # 定向例：跨 k 边界 + lapse-reset 重爬坡
    directed = [
        ("k2_cross", 2, {"t": [1, 1, 1, 1, 1], "r": [1, 1, 1, 1, 1]}),
        ("k3_cross", 3, {"t": [1, 1, 1, 1, 1, 1], "r": [1, 1, 1, 1, 1, 1]}),
        ("k2_reset_reramp", 2, {"t": [1, 1, 1, 0, 1, 1, 1], "r": [1, 1, 1, 0, 1, 1, 1]}),
        ("k2_boundary_minus1", 2, {"t": [1, 1], "r": [1, 1]}),
        ("k3_same_day_freeze", 3, {"t": [1, 0, 1, 0, 1, 1, 1], "r": [1, 1, 1, 1, 1, 1, 1]}),
    ]
    max_state_err = 0.0
    with AdapterServer() as server:
        for label, k, case in directed:
            config = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
            config["w"] = list(_FSRS6_W)
            config.update({
                "gspSuccessGrade": 3,  # 低 S 判别力
                "gspGraduationStreak": k, "gspGraduationFloorDays": 30.0,
                "gspIntervalCapDays": 40.0, "gspMaturityBandDays": 0.0,
                "gspIntervalFuzz": 0.0,
            })
            err, _ = _assert_gsp_parity(server, config, dict(case, retention=0.85), f"gsp-c-{label}")
            max_state_err = max(max_state_err, err)
        # 随机变体：长成功串跨 k + lapse 穿插
        for k in (2, 3):
            for _ in range(20):
                config = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
                config["w"] = _rand_w(rng, base_w)
                config.update({
                    "gspSuccessGrade": 3,
                    "gspGraduationStreak": k, "gspGraduationFloorDays": rng.choice([30.0, 35.0]),
                    "gspIntervalCapDays": rng.choice([40.0, 60.0]),
                    "gspMaturityBandDays": 0.0, "gspIntervalFuzz": 0.0,
                })
                length = rng.randint(k, k + 8)
                r = [1] * length
                for pos in rng.sample(range(length), rng.randint(0, max(1, length // 4))):
                    r[pos] = 0
                case = {"t": [rng.choice([1, 2, 3, 5]) for _ in range(length)], "r": r,
                        "retention": 0.85}
                err, _ = _assert_gsp_parity(server, config, case, f"gsp-c-rnd-k{k}")
                max_state_err = max(max_state_err, err)
    print(f"\ngsp-c graduation parity: max state abs err = {max_state_err:.3e}")


def test_gsp_parity_d_cap_banded():
    """(d) cap + banded-retention：S 轨迹跨 gspMaturityBandDays 切换 young/mature 口径，
    并撞 cap。在 band 切换点与 cap 边界对拍区间（含 banded base 求解）。"""
    rng = random.Random(20260616)
    base_w = [float(v) for v in _FSRS6_W]
    max_state_err = 0.0
    # 定向：构造跨 band 的 S 轨迹（短历史 S<band，长历史 S>band）+ 撞 cap 的高 S 词
    directed = [
        # young 区（S 低，< band=14）
        ("young_side", {"t": [0, 1, 2], "r": [1, 1, 1]}),
        # mature 区（S 高，>= band），长成功串
        ("mature_side", {"t": [2, 5, 10, 20, 30, 40], "r": [1, 1, 1, 1, 1, 1]}),
        # 撞 cap（极长成功串）
        ("cap_pinned", {"t": [30] * 12, "r": [1] * 12}),
        # band 邻域来回（成功后失败压 S 回 young 区）
        ("band_recross", {"t": [5, 10, 20, 1, 1, 5, 10], "r": [1, 1, 1, 0, 0, 1, 1]}),
    ]
    with AdapterServer() as server:
        for label, case in directed:
            config = json.loads(json.dumps(F1_SHIP_CONFIG))  # band=14, young=0.86, mature=0.92, cap=40
            err, _ = _assert_gsp_parity(server, config, dict(case, retention=0.85), f"gsp-d-{label}")
            max_state_err = max(max_state_err, err)
        # 随机变体：扫多组 band/young/mature/cap，混合历史跨带
        for _ in range(50):
            config = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
            config["w"] = _rand_w(rng, base_w)
            band = rng.choice([7.0, 14.0, 21.0, 30.0])
            young = rng.uniform(0.80, 0.95)
            mature = rng.uniform(0.80, 0.95)
            config.update({
                "gspSuccessGrade": 4,
                "gspMaturityBandDays": band,
                "gspYoungRetention": round(young, 3),
                "gspMatureRetention": round(mature, 3),
                "gspIntervalCapDays": rng.choice([40.0, 60.0, 89.0]),
                "gspGraduationStreak": 0, "gspIntervalFuzz": 0.0,
            })
            length = rng.randint(2, 16)
            case = {
                "t": [rng.choice([0, 1, 2, 5, 10, 20, 30]) for _ in range(length)],
                "r": [rng.choice([0, 1, 1, 1]) for _ in range(length)],
                "retention": 0.85,
            }
            err, _ = _assert_gsp_parity(server, config, case, "gsp-d-rnd")
            max_state_err = max(max_state_err, err)
    print(f"\ngsp-d cap+banded parity: max state abs err = {max_state_err:.3e}")


def test_gsp_parity_e_fuzz():
    """(e) fuzz：gspIntervalFuzz=0.15，§7.2 抖动公式 float-exact → 区间整数恒等对拍。

    扫多组 (stability, review_count) 使 u 取正/负；含毕业词（floor 底夹）、cap-pinned 词
    （fuzz_cap 顶夹）。fuzz 的 §7.2/§7.3 op-order 逐位（去 sin，f64 同序、同 clamp）。"""
    rng = random.Random(20260617)
    base_w = [float(v) for v in _FSRS6_W]
    max_state_err = 0.0
    with AdapterServer() as server:
        for _ in range(100):
            config = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
            config["w"] = _rand_w(rng, base_w)
            config.update({
                "gspSuccessGrade": rng.choice([3, 4]),
                "gspIntervalCapDays": rng.choice([40.0, 60.0, 89.0]),
                "gspGraduationStreak": rng.choice([0, 2, 3]),
                "gspGraduationFloorDays": 30.0,
                "gspMaturityBandDays": rng.choice([0.0, 14.0]),
                "gspYoungRetention": 0.86, "gspMatureRetention": 0.92,
                "gspIntervalFuzz": 0.15,
            })
            length = rng.randint(1, 18)
            case = {
                "t": [rng.choice([0, 1, 2, 3, 7, 14, 30]) for _ in range(length)],
                "r": [rng.choice([0, 1, 1, 1]) for _ in range(length)],
                "retention": rng.choice(RETENTION_CHOICES),
            }
            err, _ = _assert_gsp_parity(server, config, case, "gsp-e-fuzz")
            max_state_err = max(max_state_err, err)
        # 定向：撞 cap 的高 S 词（fuzz_cap 顶夹）+ 毕业词（floor 底夹）
        directed_cfg = json.loads(json.dumps(F1_SHIP_CONFIG))
        directed_cfg["gspIntervalFuzz"] = 0.20
        for label, case in [
            ("cap_top_clamp", {"t": [30] * 14, "r": [1] * 14}),
            ("grad_floor_bottom", {"t": [2, 5, 10, 2, 5], "r": [1, 1, 1, 1, 1]}),
        ]:
            err, _ = _assert_gsp_parity(server, directed_cfg, dict(case, retention=0.85), f"gsp-e-{label}")
            max_state_err = max(max_state_err, err)
    print(f"\ngsp-e fuzz parity: max state abs err = {max_state_err:.3e}")


def test_gsp_parity_f_f1_ship():
    """(f) F1-ship-config：精确船值 × 长混合序列（成功串 / lapse / 同日 / 长 gap）。

    这是守 TEST one-shot 的族——船值精确口径（cap40/streak2/floor30/band14/grade4）下
    S/D 与 GSP head 全程对拍 ≤1e-9 + 整数恒等。船值字典与 config.py DEFAULT / amas_config.toml
    v5 三处同源（任一漂移即本族报错）。"""
    rng = random.Random(20260618)
    max_state_err = 0.0
    interval_compared = 0
    # 定向长混合序列（手工覆盖各分支）
    directed = [
        ("success_run", {"t": [0, 2, 5, 10, 20, 40, 60], "r": [1, 1, 1, 1, 1, 1, 1]}),
        ("lapse_recover", {"t": [0, 3, 7, 5, 1, 2, 10, 30], "r": [1, 1, 1, 0, 0, 1, 1, 1]}),
        ("same_day_burst", {"t": [0, 0, 0, 1, 0, 2, 0, 5], "r": [1, 1, 1, 1, 0, 1, 1, 1]}),
        ("long_gaps", {"t": [0, 30, 60, 90, 30, 60], "r": [1, 1, 1, 1, 0, 1]}),
        ("fail_heavy", {"t": [0, 1, 2, 1, 3, 1, 2, 7], "r": [1, 0, 0, 1, 0, 0, 1, 0]}),
        ("alternating", {"t": [0, 1, 1, 2, 3, 1, 5, 2, 1, 7], "r": [1, 0, 1, 0, 1, 0, 1, 0, 1, 1]}),
    ]
    with AdapterServer() as server:
        for label, case in directed:
            err, _ = _assert_gsp_parity(server, F1_SHIP_CONFIG, dict(case, retention=0.85), f"gsp-f-{label}")
            max_state_err = max(max_state_err, err)
            interval_compared += 1
        # 随机长混合序列（船值精确口径，仅扰历史）
        for _ in range(120):
            length = rng.randint(8, 25)
            case = {
                "t": [rng.choice([0, 0, 1, 1, 2, 3, 7, 14, 30, 60]) for _ in range(length)],
                "r": [rng.choice([0, 1, 1, 1]) for _ in range(length)],
                "retention": 0.85,
            }
            err, _ = _assert_gsp_parity(server, F1_SHIP_CONFIG, case, "gsp-f-rnd")
            max_state_err = max(max_state_err, err)
            interval_compared += 1
    assert interval_compared >= 120
    print(
        f"\ngsp-f F1-ship parity: {interval_compared} cases, "
        f"max state abs err = {max_state_err:.3e}"
    )
