"""镜像 × Rust adapter 数值对等测试（spec: mirror-alignment round 2, 2026-06-10）。

200 个种子随机用例：每例独立 21 维 w（DEFAULT × U(0.5,1.5)，w[20] 钳 [0.05,2.0]）、
历史长度 1–20、逐步 delta_t ∈ {0..60}（含 0 → 覆盖同日分支）、二元 r、三档 retention。
另有 60 个 19 维 FSRS-5 旧权重用例（Rust 侧走 w19=0/w20=0.5 迁移路径）+ 1 个
定向回归例，覆盖同日 G≥3 下限在 19 维分支的对等。
Python 侧 WordforgeMirrorState 与 Rust 侧 maimemo_mdm_adapter（mdm.rs 真值，
quality 0.7/0.0、alpha=0.3）各自重放同一历史：

- stability / difficulty：abs ≤ 1e-9 + 1e-9·|rust|
- interval（retention 0.85 / 0.90）：复刻 mdm.rs compute_interval 的秒级管线
  （×86400 → `as i64` 向零截断 → max(60s)）后相对误差 ≤ 1e-6；
  仅在 adapter interval ≥ 1 天时断言（60s 底侧地板 vs 镜像 1 天地板的语义差）。

注意：parity 驱动传 RAW delta（含 0），不得套用闭环模拟器的 max(delta,1) 地板。
"""
from __future__ import annotations

import json
import random

from benchmarks.maimemo.adapter import AdapterServer
from benchmarks.maimemo.config import DEFAULT_MEMORY_MODEL_CONFIG, FSRS_BASELINE_CONFIG
from benchmarks.maimemo.dhp_reference import _mirror_from_config

CASE_COUNT = 200
LEGACY_CASE_COUNT = 60
RETENTION_CHOICES = (0.85, 0.90, 0.92)
INTERVAL_RETENTIONS = (0.85, 0.90)
STATE_TOL = 1e-9
INTERVAL_REL_TOL = 1e-6


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
