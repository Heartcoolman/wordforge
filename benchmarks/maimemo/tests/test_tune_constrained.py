"""tune() DHP 约束漏斗的单测与冒烟（spec D13）。

覆盖：
1. _dhp_constraints 边界语义（可行/不可行/精确边界/NaN/零基线）
2. 真 optuna 约束研究：同种子确定性 + best_trial 偏向可行最优
3. TuneFunnelSmokeTest：全 fake 跑真 tune()（winner / conclusive_negative 两景）
4. 14 个种子落在 SEARCH_WINDOWS 内（enqueue VERBATIM 陷阱守护；合成窗口外值 pin）
5. iterative_tune 冒烟（schema 无 KeyError）
6. ExperimentalWarning 已过滤 + 版本守卫告警
"""
from __future__ import annotations

import json
import math
import warnings

import optuna
import pytest

from benchmarks.maimemo import pipeline
from benchmarks.maimemo.config import (
    DEFAULT_MEMORY_MODEL_CONFIG,
    DEFAULT_RANDOM_SEED,
    BenchPaths,
)
from benchmarks.maimemo.pipeline import (
    DHP_ANCHOR_COMPONENTS,
    DHP_CONSTRAINT_KEYS,
    DHP_GUARDRAIL_FLOORS,
    SEARCH_WINDOWS,
    _dhp_constraints,
    _make_constrained_sampler,
    _mastered_objective_term,
    _mutate_config,
    _seed_trial_params,
    iterative_tune,
    tune,
)


BASE_DHP = {
    "expectedMemory": 100.0,
    "nextDayMemory": 50.0,
    "targetCount": 10.0,
    "masteredProxy": 20.0,
}


# ---------------------------------------------------------------------------
# 1. _dhp_constraints
# ---------------------------------------------------------------------------

def test_constraints_feasible_equal_to_baseline():
    feasible, comps = _dhp_constraints(dict(BASE_DHP), BASE_DHP)
    assert feasible is True
    # per-leg 下限：分量 = floor - 1.0（0.90/0.90/0.95/1.00 ⇒ -0.1/-0.1/-0.05/0.0）
    assert comps == pytest.approx(DHP_ANCHOR_COMPONENTS)
    assert comps == pytest.approx((-0.1, -0.1, -0.05, 0.0))
    assert all(c <= 0.0 for c in comps)


def test_constraints_infeasible_violation_magnitude():
    dhp = dict(BASE_DHP, expectedMemory=80.0)
    feasible, comps = _dhp_constraints(dhp, BASE_DHP)
    assert feasible is False
    assert comps[0] >= 1e-12 and comps[0] == pytest.approx(0.1)
    assert all(c <= 0.0 for c in comps[1:])


def test_constraints_mastered_regression_is_infeasible():
    # masteredProxy 下限是 1.0x：低于基线即不可行（冲榜指标不许回退）
    dhp = dict(BASE_DHP, masteredProxy=19.0)
    feasible, comps = _dhp_constraints(dhp, BASE_DHP)
    assert feasible is False
    assert comps[3] == pytest.approx(0.05)


def test_constraints_exact_boundary_is_feasible():
    dhp = {
        key: value * DHP_GUARDRAIL_FLOORS[key] for key, value in BASE_DHP.items()
    }
    feasible, comps = _dhp_constraints(dhp, BASE_DHP)
    assert feasible is True
    assert all(c <= 0.0 for c in comps)


def test_constraints_nan_guard_never_emits_nan():
    dhp = dict(BASE_DHP, nextDayMemory=float("nan"))
    feasible, comps = _dhp_constraints(dhp, BASE_DHP)
    assert feasible is False
    assert comps[1] == 1.0
    assert all(math.isfinite(c) for c in comps)


def test_constraints_zero_baseline_is_infeasible():
    base = dict(BASE_DHP, targetCount=0.0)
    feasible, comps = _dhp_constraints(dict(BASE_DHP), base)
    assert feasible is False
    assert comps[2] == 1.0


# ---------------------------------------------------------------------------
# 2. 约束研究语义（真 optuna）
# ---------------------------------------------------------------------------

def _run_toy_constrained_study(n_trials: int = 30):
    study = optuna.create_study(
        direction="maximize", sampler=_make_constrained_sampler(7, 8)
    )
    xs = []
    for _ in range(n_trials):
        trial = study.ask()
        x = trial.suggest_float("x", 0.0, 1.0)
        trial.set_user_attr("constraint", (x - 0.5,))
        study.tell(trial, x)
        xs.append(x)
    return study, xs


def test_constrained_study_deterministic_and_prefers_feasible_best():
    study_a, xs_a = _run_toy_constrained_study()
    _, xs_b = _run_toy_constrained_study()
    assert xs_a == xs_b  # 同种子 ⇒ 完全一致的建议序列
    feasible = [x for x in xs_a if x <= 0.5]
    assert feasible
    assert max(xs_a) > 0.5  # 裸最优不可行
    assert study_a.best_trial.params["x"] == max(feasible)


def test_constraint_tuple_survives_user_attrs():
    study = optuna.create_study(
        direction="maximize", sampler=_make_constrained_sampler(11, 4)
    )
    trial = study.ask()
    trial.suggest_float("x", 0.0, 1.0)
    trial.set_user_attr("constraint", (-0.1, -0.1, -0.1))
    study.tell(trial, 0.5)
    stored = study.trials[0].user_attrs["constraint"]
    assert tuple(stored) == (-0.1, -0.1, -0.1)  # InMemoryStorage 序列语义比较


# ---------------------------------------------------------------------------
# 3. TuneFunnelSmokeTest（全 fake，跑真 tune()）
# ---------------------------------------------------------------------------

BASELINE_PREDICTION = {"logLoss": 0.5, "ici": 0.1, "auc": 0.7, "maeP": 0.2, "smapeH": 0.4}
GOOD_PREDICTION = {"logLoss": 0.45, "ici": 0.09, "auc": 0.72, "maeP": 0.18, "smapeH": 0.38}
BAD_PREDICTION = {"logLoss": 0.52, "ici": 0.104, "auc": 0.69, "maeP": 0.21, "smapeH": 0.42}


def _interval_policy_stub():
    return {
        "targets": {
            f"{ret:.2f}": {
                "safety": 0.5,
                "efficiency": 0.5,
                "intervalScore": 0.25,
                "oracleMeanDelta": 1.0,
                "candidateMeanDelta": 1.0,
            }
            for ret in (0.80, 0.85, 0.90)
        },
        "policyScore": 0.25,
    }


class _FakeAdapterServer:
    def __enter__(self):
        return self

    def __exit__(self, *args):
        return None


def _is_default_config(memory_config) -> bool:
    return memory_config["w"] == DEFAULT_MEMORY_MODEL_CONFIG["w"]


def _install_funnel_fakes(monkeypatch, candidate_prediction):
    calls = {"single_pass": [], "canary": 0}

    def fake_single_pass(
        paths, memory_config, split, user_fraction,
        hlr_model=None, oracle=None, server=None,
        include_interval=True, include_baselines=True,
        skip_slow_baselines=False, max_rows=None, dhp_student=None,
        user_buckets=None,
    ):
        calls["single_pass"].append({
            "userFraction": user_fraction,
            "userBuckets": user_buckets,
            "includeInterval": include_interval,
            "maxRows": max_rows,
        })
        prediction = dict(
            BASELINE_PREDICTION if _is_default_config(memory_config) else candidate_prediction
        )
        result = {"wordforge": prediction, "rows": 1000, "batches": 1}
        if include_interval:
            result["interval_policy"] = _interval_policy_stub()
        return result

    def fake_dhp_reference(memory_config, paths):
        return {
            "wordforge": {
                "expectedMemory": 4000.0,
                "nextDayMemory": 3000.0,
                "targetCount": 600.0,
                "masteredProxy": 800.0,
                "avgDueRecall": 0.8,
            },
            "configEcho": memory_config,
        }

    def fake_canary():
        calls["canary"] += 1

    monkeypatch.setattr(pipeline, "_ensure_oracle_guard", lambda paths: None)
    monkeypatch.setattr(pipeline, "_load_oracle", lambda paths: object())
    monkeypatch.setattr(pipeline, "_load_hlr", lambda paths: object())
    monkeypatch.setattr(pipeline, "AdapterServer", _FakeAdapterServer)
    monkeypatch.setattr(pipeline, "_single_pass_evaluate", fake_single_pass)
    monkeypatch.setattr(pipeline, "_dhp_reference", fake_dhp_reference)
    monkeypatch.setattr(pipeline, "_verify_constrained_tpe", fake_canary)
    return calls


LEGACY_TOP_KEYS = ("selected", "baseline", "nearMisses")
CANDIDATE_KEYS = (
    "memoryModel", "metrics", "dhpReference",
    "predictionGainPercent", "interval85GainPercent", "passes",
)
DIAG_KEYS = (
    "verdict", "feasibleCount", "infeasibleCount", "shortCircuitedCount",
    "feasibleShareByQuarter", "bestFeasibleTrajectory", "paramDispersion",
    "nullReplicates", "boundaryAutopsy", "anchorFeasible",
    "retentionSensitivity", "avgDueRecall", "predictionDeltas", "refinement",
)


def test_tune_funnel_winner(monkeypatch, tmp_path):
    paths = BenchPaths.from_root(tmp_path).ensure()
    calls = _install_funnel_fakes(monkeypatch, GOOD_PREDICTION)

    result = tune(paths, stage1_trials=32)

    summary = json.loads(
        (paths.reports / "tuning_summary.json").read_text(encoding="utf-8")
    )
    for key in LEGACY_TOP_KEYS + ("searchDiagnostics",):
        assert key in summary
    for key in ("memoryModel", "prediction", "dhpReference"):
        assert key in summary["baseline"]
    diag = summary["searchDiagnostics"]
    for key in DIAG_KEYS:
        assert key in diag
    assert diag["verdict"] == "winner"
    assert diag["anchorFeasible"] is True  # 锚点自检确已执行且通过
    assert diag["feasibleCount"] == 32
    assert diag["shortCircuitedCount"] == 0
    assert len(diag["feasibleShareByQuarter"]) == 4
    assert len(diag["bestFeasibleTrajectory"]) == 32
    assert len(diag["nullReplicates"]["objectives"]) == 10

    selected = summary["selected"]
    for key in CANDIDATE_KEYS:
        assert key in selected
    assert selected["passes"] is True
    for key in (
        "objective", "prediction", "predictionScore", "intervalPolicy",
        "avgEfficiency", "rows", "masteredProxy",
    ):
        assert key in selected["metrics"]
    assert len(summary["nearMisses"]) <= 3

    # stage-2 必须在不相交桶 (1, 2) 上复评
    assert any(c["userBuckets"] == (1, 2) for c in calls["single_pass"])
    assert calls["canary"] == 1
    assert result["selected"]["passes"] is True


def test_tune_funnel_conclusive_negative(monkeypatch, tmp_path):
    paths = BenchPaths.from_root(tmp_path).ensure()
    _install_funnel_fakes(monkeypatch, BAD_PREDICTION)

    tune(paths, stage1_trials=32)

    summary = json.loads(
        (paths.reports / "tuning_summary.json").read_text(encoding="utf-8")
    )
    diag = summary["searchDiagnostics"]
    assert diag["verdict"] == "conclusive_negative"  # 无人过闸且 feasibleCount >= 30
    assert diag["anchorFeasible"] is True
    assert diag["refinement"] == {
        "triggered": False, "trials": 0, "bestGain": None, "passed": False,
    }
    selected = summary["selected"]
    assert selected["keptBaseline"] is True
    assert selected["passes"] is False
    assert selected["metrics"]["objective"] == 1.0


# ---------------------------------------------------------------------------
# 4. 种子键完整 + 窗口内（enqueue 按 VERBATIM 透传，不做越界裁剪）
#    2026-06-13 v5 GSP 写回后 DEFAULT tau=0.0（未平滑核心，alpha 钉死），锚点 tau 落 D5 窗口
#    (1.5,4.0) 外 —— 由 optuna VERBATIM 透传（test_anchor_seed_out_of_window_passes_verbatim）。
#    GSP 为独立 campaign 不经此 D5 harness；w 维 + 性能/阶梯教师 tau 仍全在窗口内。
# ---------------------------------------------------------------------------

def test_seed_trials_inside_windows():
    seeds = _seed_trial_params()
    assert len(seeds) == 14
    for seed in seeds:
        assert set(seed) == set(SEARCH_WINDOWS)
        for name, value in seed.items():
            low, high = SEARCH_WINDOWS[name]
            if name == "alphaRampTau":
                # 锚点 tau=DEFAULT=0.0 落窗口外（v5 GSP 写回后）；其余 tau（性能/阶梯教师）在窗口内
                continue
            assert low <= value <= high, f"seed {name}={value} outside [{low}, {high}]"
    # stock 锚点必须程序化等于 DEFAULT（防字面漂移），tau=0.0 == v5 GSP 写回船值（未平滑核心）
    stock = seeds[0]
    for name, idx in pipeline._W_INDEX.items():
        assert stock[name] == DEFAULT_MEMORY_MODEL_CONFIG["w"][idx]
    assert stock["alphaRampTau"] == DEFAULT_MEMORY_MODEL_CONFIG["alphaRampTau"] == 0.0
    # 锚点 tau=0.0（出窗，VERBATIM 透传）；性能教师统一 tau=3.0；tau 阶梯教师 {2.5, 3.5}
    taus = sorted({seed["alphaRampTau"] for seed in seeds})
    assert taus == [0.0, 2.5, 3.0, 3.5]


def test_anchor_seed_out_of_window_passes_verbatim():
    """optuna 4.8.0 行为 pin：窗口外 fixed param 仅发 UserWarning 并逐字透传。

    2026-06-13 v5 GSP 写回后锚点 tau=DEFAULT=0.0 落 D5 窗口 (1.5,4.0) 外，VERBATIM 透传
    路径再次实测覆盖（锚点 trial 0 配置 == DEFAULT == 守门基线仍成立，tau 分量恰为 floor-1.0
    的合法 0.0）。第二个合成例显式 tau=0.0 同样出窗，双重 pin optuna 不重采样语义。
    """
    study = optuna.create_study(
        direction="maximize", sampler=_make_constrained_sampler(DEFAULT_RANDOM_SEED, 4)
    )
    anchor = _seed_trial_params()[0]
    study.enqueue_trial(anchor)                            # 锚点（tau 出窗，VERBATIM 透传）
    study.enqueue_trial({**anchor, "alphaRampTau": 0.0})   # 合成窗口外值
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", UserWarning)
        trial0 = study.ask()
        config0 = _mutate_config(trial0)
        trial1 = study.ask()
        config1 = _mutate_config(trial1)
    assert config0 == DEFAULT_MEMORY_MODEL_CONFIG  # 逐键逐位相等 ⇒ 锚点分量恰为 floor-1.0
    assert trial1.params["alphaRampTau"] == 0.0    # 窗口外逐字透传，未被重采样
    expected = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
    expected["alphaRampTau"] = 0.0
    assert config1 == expected


def test_mastered_objective_term_formula():
    base = {"masteredProxy": 800.0}
    # 等于基线 ⇒ 0；+20% ⇒ 0.5*0.2；超 cap（ratio-1 > 1.5）截到 0.5*1.5；回退为负不截
    assert _mastered_objective_term({"masteredProxy": 800.0}, base) == pytest.approx(0.0)
    assert _mastered_objective_term({"masteredProxy": 960.0}, base) == pytest.approx(0.1)
    assert _mastered_objective_term({"masteredProxy": 2400.0}, base) == pytest.approx(0.75)
    assert _mastered_objective_term({"masteredProxy": 400.0}, base) == pytest.approx(-0.25)


# ---------------------------------------------------------------------------
# 5. iterative_tune 冒烟
# ---------------------------------------------------------------------------

def test_iterative_tune_smoke(monkeypatch, tmp_path):
    paths = BenchPaths.from_root(tmp_path).ensure()
    _install_funnel_fakes(monkeypatch, GOOD_PREDICTION)

    result = iterative_tune(
        paths, max_iterations=2, convergence_threshold=0.01,
        convergence_patience=1, stage1_trials=24, verbose=False,
    )

    for key in ("convergence_log", "converged", "iterations_completed", "convergence_threshold"):
        assert key in result
    assert result["iterations_completed"] == 2
    assert result["converged"] is True
    for entry in result["convergence_log"]:
        for key in ("iteration", "objective", "prediction_gain", "interval_gain", "passes", "config_hash"):
            assert key in entry


# ---------------------------------------------------------------------------
# 6. 告警行为
# ---------------------------------------------------------------------------

def test_sampler_construction_filters_experimental_warning():
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        _make_constrained_sampler(DEFAULT_RANDOM_SEED, 16)
    experimental = [
        w for w in caught if issubclass(w.category, optuna.exceptions.ExperimentalWarning)
    ]
    assert experimental == []


def test_version_guard_warns_on_unverified_version(monkeypatch):
    monkeypatch.setattr(optuna, "__version__", "9.9.9")
    with pytest.warns(RuntimeWarning, match="4.8.0"):
        pipeline._warn_if_unverified_optuna()


def test_version_guard_silent_on_pinned_version(recwarn):
    assert optuna.__version__ == "4.8.0"
    pipeline._warn_if_unverified_optuna()
    assert [w for w in recwarn.list if issubclass(w.category, RuntimeWarning)] == []
