"""gsp_search.py — Staged GSP search driver (val split, campaign 2026-06-12-rank1).

目标：候选 amas 配置（未平滑 FSRS-6 核心 + GSP 调度策略头）代入 VAL 全量 10 算法榜
（benchmarks/results/2026-06-12-rank1-val-board/），经 whatif_board 重算 Borda，
要求 amas **严格 #1**，受 G2/G3/G4 闸门约束。

经济学：GSP 旋钮不触状态更新 → 预测腿在 GSP 网格上**恒定**（= base config 的 pred legs，
测一次即可）。每候选只重跑 3 个数据集的单算法 'amas' 闭环 sim（快）。

用法（单候选）::

    .bench-venv/bin/python -m benchmarks.maimemo.gsp_search \
        --config '<json>' --label stageA_cap60 \
        [--pred-legs '<json: {dataset: {logLoss,ici,auc}}>']   # 复用已测 pred（GSP 不变）

不传 --pred-legs 时本驱动会**测一次**候选 pred legs（3 数据集）并写入结果行；
传入则跳过 pred 评估直接复用（grid 加速）。

结果行 append 到 benchmarks/results/2026-06-12-rank1-search/results.jsonl。
"""
from __future__ import annotations

import argparse
import json
import statistics
import time
from pathlib import Path
from typing import Any, Dict, List, Optional

from .config import BenchPaths, DEFAULT_MEMORY_MODEL_CONFIG
from .evaluate_scheduler import evaluate_scheduler_prediction
from .simulate import simulate_strategies
from .whatif_board import (
    load_board,
    compute_borda,
    compact_table,
    pred_legs_of,
)

# 数据集 → bench root
DATASETS = {
    "maimemo": "~/.wordforge-bench/maimemo",
    "duolingo_hlr": "~/.wordforge-bench/duolingo_hlr",
    "synthetic": "~/.wordforge-bench/synthetic",
}

VAL_BOARD_DIR = Path("benchmarks/results/2026-06-12-rank1-search").parent.parent / "results" / "2026-06-12-rank1-val-board"
RESULTS_JSONL = Path("benchmarks/results/2026-06-12-rank1-search/results.jsonl")
SIM_DAYS = 90

# ---- 预注册闸门锚点（从 val board amas6 / baseline 派生，启动前冻结）----
G2_MAI_MASTERED_FLOOR = 17000.0
G3_TOL = 1.02
G4_RETSTAB_FLOOR = {"maimemo": 0.895, "duolingo_hlr": 0.825, "synthetic": 0.885}
# G1 margin 阈值：每个贡献胜负的数据集 rank 的 final_score 余量 ≥ 0.005
G1_MARGIN_FLOOR = 0.005


# ---------------------------------------------------------------------------
# Candidate base config（v5 架构起点：未平滑 FSRS-6 + 公版 21 维 w + GSP 全关）
# ---------------------------------------------------------------------------
_FSRS6_DEFAULT_W = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001,
    1.8722, 0.1666, 0.796, 1.4835, 0.0614, 0.2629, 1.6483, 0.6014,
    1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
]


def candidate_base_config() -> Dict[str, Any]:
    """{**DEFAULT, **v5 架构起点}。GSP 旋钮全关，由 grid 覆盖。"""
    base = {
        "alphaScale": 1.0, "alphaMin": 1.0, "alphaMax": 1.0,
        "alphaRampTau": 0.0, "alphaLapseRampTau": 0.0,
        "w": list(_FSRS6_DEFAULT_W),
        "forgettingCurveFactor": 0.9 ** (-1.0 / 0.1542) - 1.0,
        "forgettingCurveDecay": -0.1542,
        "forgettingCurveFloor": 0.0,
        "baseDesiredRetention": 0.85,
        # 成功成绩带 Easy(4)：FSRS-6 公版 w 与 grade=4 共拟合 → 恢复校准（grade=3 默认令 G3 mai/syn 失守）。
        # 这是 v5 架构的关键修正（recon 已标记此结构性风险），amas6=FSRS6MirrorState 即 grade=4。
        "gspSuccessGrade": 4,
        "gspIntervalCapDays": 0.0,
        "gspGraduationStreak": 0,
        "gspGraduationFloorDays": 30.0,
        "gspYoungRetention": 0.0,
        "gspMatureRetention": 0.0,
        "gspMaturityBandDays": 0.0,
    }
    return {**DEFAULT_MEMORY_MODEL_CONFIG, **base}


# ---------------------------------------------------------------------------
# 单数据集 sim leg 抽取
# ---------------------------------------------------------------------------
def run_amas_sim(
    dataset: str,
    memory_config: Dict[str, Any],
    seed: int = 42,
) -> Dict[str, float]:
    """跑单算法 'amas' val 闭环 sim，抽取 5 条 sim legs（+派生 rpd/eff/frr）。"""
    root = Path(DATASETS[dataset]).expanduser()
    paths = BenchPaths.from_root(root)
    sim = simulate_strategies(
        paths,
        strategies=["amas"],
        n_users=300,
        sim_days=SIM_DAYS,
        max_words_per_user=500,
        daily_budget=200,
        desired_retention=0.85,
        memory_config=memory_config,
        seed=seed,
        split="val",
    )
    strat = sim["strategies"]["amas"]
    daily = strat["daily"]
    recalls_all = [float(d["recall_rate"]) for d in daily]
    ret_stab = (1.0 - statistics.stdev(recalls_all)) if len(recalls_all) > 1 else 1.0
    return {
        "masteredCount": float(strat["mastered_count"]),
        "totalReviews": float(strat["total_reviews"]),
        "expectedMemoryFinal": float(strat["expected_memory_final"]),
        "efficiency": float(strat["efficiency"]),
        "retentionStability": float(ret_stab),
        "finalRecallRate": float(recalls_all[-1]) if recalls_all else 0.0,
        "reviewsPerDay": float(strat["total_reviews"]) / SIM_DAYS,
    }


def run_amas_pred(dataset: str, memory_config: Dict[str, Any], seed: int = 42) -> Dict[str, float]:
    """测候选 amas pred legs（注入 memory_config）。GSP 不变 → 测一次复用。"""
    root = Path(DATASETS[dataset]).expanduser()
    paths = BenchPaths.from_root(root)
    pred = evaluate_scheduler_prediction(
        paths, "amas", n_users=300, max_words_per_user=200, seed=seed,
        split="val", memory_config=memory_config,
    )
    return {"logLoss": float(pred["logLoss"]), "ici": float(pred["ici"]), "auc": float(pred["auc"])}


# ---------------------------------------------------------------------------
# whatif + gate 评估
# ---------------------------------------------------------------------------
def evaluate_candidate(
    sim_legs: Dict[str, Dict[str, float]],
    pred_legs: Dict[str, Dict[str, float]],
    board_dir: Path = VAL_BOARD_DIR,
) -> Dict[str, Any]:
    """把候选 amas 腿（sim 5 腿 + pred 3 腿）代入 val board，重算 Borda + 闸门判定。"""
    from .whatif_board import whatif  # 局部导入避免循环

    bd = load_board(board_dir)
    # amas6 pred 锚点（G3）
    amas6_pred = {ds: pred_legs_of(bd, "amas6", ds) for ds in DATASETS}

    overrides: Dict[str, Dict[str, float]] = {}
    for ds in DATASETS:
        leg = {}
        leg.update(pred_legs[ds])              # logLoss, ici, auc
        leg.update({k: sim_legs[ds][k] for k in (
            "masteredCount", "totalReviews", "expectedMemoryFinal",
            "retentionStability", "finalRecallRate",
        )})
        # efficiency / rpd 由 whatif 从 expMem/totReviews 重新派生（自洽），无需显式覆盖
        overrides[ds] = leg

    agg = whatif(bd, "amas", overrides, rederive=True)
    table = compact_table(agg)
    # amas 行
    amas_row = next(r for r in table if r["algo"] == "amas")
    amas_borda = amas_row["borda"]
    amas_rank = amas_row["overall_rank"]
    amas_per_ds_rank = amas_row["ranks"]

    # 严格 #1 判定 + 余量
    # 重新计算每个 dataset 内 amas 与紧邻竞争者的 final_score 余量
    scored = _scored_board(bd, overrides)
    margins = _dataset_margins(scored)

    # G1: borda 严格 > 全部 9 竞争者
    other_bordas = [r["borda"] for r in table if r["algo"] != "amas"]
    strict_first = amas_borda > max(other_bordas) if other_bordas else False
    # 每个「贡献胜负」（amas rank==1）的数据集 final_score 余量 ≥ 0.005
    g1_margin_ok = True
    won_datasets = [ds for ds in DATASETS if amas_per_ds_rank[ds] == 1]
    for ds in won_datasets:
        if margins[ds]["amas_to_next"] < G1_MARGIN_FLOOR:
            g1_margin_ok = False
    g1_pass = bool(strict_first and g1_margin_ok and len(won_datasets) >= 1)

    # G2: mai mastered >= 17000
    g2_pass = sim_legs["maimemo"]["masteredCount"] >= G2_MAI_MASTERED_FLOOR

    # G3: pred LL/ICI 每数据集 <= amas6 * 1.02
    g3_bits = {}
    g3_pass = True
    for ds in DATASETS:
        ll_ok = pred_legs[ds]["logLoss"] <= amas6_pred[ds]["logLoss"] * G3_TOL
        ici_ok = pred_legs[ds]["ici"] <= amas6_pred[ds]["ici"] * G3_TOL
        g3_bits[ds] = {"ll_ok": bool(ll_ok), "ici_ok": bool(ici_ok),
                       "ll": pred_legs[ds]["logLoss"], "ll_ceiling": amas6_pred[ds]["logLoss"] * G3_TOL,
                       "ici": pred_legs[ds]["ici"], "ici_ceiling": amas6_pred[ds]["ici"] * G3_TOL}
        if not (ll_ok and ici_ok):
            g3_pass = False

    # G4: retStab >= floor
    g4_bits = {}
    g4_pass = True
    for ds in DATASETS:
        ok = sim_legs[ds]["retentionStability"] >= G4_RETSTAB_FLOOR[ds]
        g4_bits[ds] = {"ok": bool(ok), "retStab": sim_legs[ds]["retentionStability"], "floor": G4_RETSTAB_FLOOR[ds]}
        if not ok:
            g4_pass = False

    return {
        "borda": amas_borda,
        "overall_rank": amas_rank,
        "per_dataset_rank": amas_per_ds_rank,
        "borda_table": table,
        "margins": margins,
        "won_datasets": won_datasets,
        "gates": {
            "G1": {"pass": g1_pass, "strict_first": bool(strict_first),
                   "margin_ok": bool(g1_margin_ok), "max_other_borda": max(other_bordas) if other_bordas else None},
            "G2": {"pass": bool(g2_pass), "mai_mastered": sim_legs["maimemo"]["masteredCount"], "floor": G2_MAI_MASTERED_FLOOR},
            "G3": {"pass": bool(g3_pass), "bits": g3_bits},
            "G4": {"pass": bool(g4_pass), "bits": g4_bits},
        },
        "all_gates_pass": bool(g1_pass and g2_pass and g3_pass and g4_pass),
    }


def _scored_board(bd, overrides):
    """复用 leaderboard 评分，返回带 final_score / dataset_rank 的完整 df（override 后）。"""
    import copy
    from benchmarks.maimemo.leaderboard import (
        _compute_raw_scores, _normalize_per_dataset, _compute_final_score, _compute_dataset_rank_and_borda,
    )
    df = bd.copy()
    for col in ("logLoss", "ici", "auc", "masteredCount", "totalReviews",
                "expectedMemoryFinal", "retentionStability", "finalRecallRate",
                "efficiency", "reviewsPerDay"):
        if col in df.columns:
            df[col] = df[col].astype(float)
    for ds, legs in overrides.items():
        mask = (df["algo"] == "amas") & (df["dataset"] == ds)
        for leg, val in legs.items():
            df.loc[mask, leg] = float(val)
    # 派生 efficiency / rpd
    df["efficiency"] = df["expectedMemoryFinal"] / df["totalReviews"]
    df["reviewsPerDay"] = df["totalReviews"] / SIM_DAYS
    df = _compute_raw_scores(df)
    df = _normalize_per_dataset(df)
    df = _compute_final_score(df)
    df = _compute_dataset_rank_and_borda(df)
    return df


def _dataset_margins(scored):
    """每数据集：amas 的 final_score、其 rank、与紧上/紧下竞争者的 final_score 余量。"""
    out = {}
    for ds in DATASETS:
        sub = scored[scored["dataset"] == ds].sort_values("final_score", ascending=False).reset_index(drop=True)
        amas_idx = sub.index[sub["algo"] == "amas"][0]
        amas_fs = float(sub.loc[amas_idx, "final_score"])
        rank = int(sub.loc[amas_idx, "dataset_rank"])
        # 与下一名（rank+1）的余量
        next_fs = float(sub.loc[amas_idx + 1, "final_score"]) if amas_idx + 1 < len(sub) else None
        prev_fs = float(sub.loc[amas_idx - 1, "final_score"]) if amas_idx - 1 >= 0 else None
        out[ds] = {
            "amas_final_score": amas_fs,
            "amas_rank": rank,
            "amas_to_next": (amas_fs - next_fs) if next_fs is not None else None,
            "to_beat_above": (prev_fs - amas_fs) if prev_fs is not None else None,
            "above_algo": str(sub.loc[amas_idx - 1, "algo"]) if amas_idx - 1 >= 0 else None,
            "next_algo": str(sub.loc[amas_idx + 1, "algo"]) if amas_idx + 1 < len(sub) else None,
        }
    return out


# ---------------------------------------------------------------------------
# 主流程：单候选评估 + append 结果行
# ---------------------------------------------------------------------------
def run_candidate(
    config: Dict[str, Any],
    label: str,
    pred_legs: Optional[Dict[str, Dict[str, float]]] = None,
    seed: int = 42,
    stage: str = "",
) -> Dict[str, Any]:
    t0 = time.time()
    # 1) 3 数据集闭环 sim
    sim_legs = {ds: run_amas_sim(ds, config, seed=seed) for ds in DATASETS}
    # 2) pred legs：传入则复用，否则测一次
    if pred_legs is None:
        pred_legs = {ds: run_amas_pred(ds, config, seed=seed) for ds in DATASETS}
    # 3) whatif + gates
    ev = evaluate_candidate(sim_legs, pred_legs)

    row = {
        "label": label,
        "stage": stage,
        "seed": seed,
        "config": {k: config[k] for k in (
            "gspIntervalCapDays", "gspGraduationStreak", "gspGraduationFloorDays",
            "gspYoungRetention", "gspMatureRetention", "gspMaturityBandDays",
        )},
        "sim_legs": sim_legs,
        "pred_legs": pred_legs,
        "borda": ev["borda"],
        "overall_rank": ev["overall_rank"],
        "per_dataset_rank": ev["per_dataset_rank"],
        "won_datasets": ev["won_datasets"],
        "margins": ev["margins"],
        "gates": ev["gates"],
        "all_gates_pass": ev["all_gates_pass"],
        "borda_table": ev["borda_table"],
        "runtime_seconds": round(time.time() - t0, 1),
    }
    RESULTS_JSONL.parent.mkdir(parents=True, exist_ok=True)
    with RESULTS_JSONL.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(row, ensure_ascii=False) + "\n")
    return row


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--config", help="候选 GSP 旋钮覆盖 JSON（合并到 candidate_base）", default="{}")
    ap.add_argument("--label", required=True)
    ap.add_argument("--stage", default="")
    ap.add_argument("--pred-legs", default="", help="复用已测 pred legs JSON（GSP 不变时跳过 pred）")
    ap.add_argument("--seed", type=int, default=42)
    args = ap.parse_args()

    overrides = json.loads(args.config)
    config = {**candidate_base_config(), **overrides}
    pred_legs = json.loads(args.pred_legs) if args.pred_legs else None

    row = run_candidate(config, args.label, pred_legs=pred_legs, seed=args.seed, stage=args.stage)
    # 紧凑 stdout 摘要
    g = row["gates"]
    print(json.dumps({
        "label": row["label"], "borda": row["borda"], "rank": row["overall_rank"],
        "per_ds_rank": row["per_dataset_rank"], "won": row["won_datasets"],
        "mai_mastered": row["sim_legs"]["maimemo"]["masteredCount"],
        "gates": {k: g[k]["pass"] for k in ("G1", "G2", "G3", "G4")},
        "all_pass": row["all_gates_pass"],
        "runtime": row["runtime_seconds"],
    }, ensure_ascii=False))


if __name__ == "__main__":
    main()
