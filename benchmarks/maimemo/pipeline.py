from __future__ import annotations

import json
import math
import os
import random
import warnings
from typing import Any, Dict, List, Sequence, Tuple

import duckdb
import numpy as np
import optuna
import pandas as pd
import torch
from tqdm import tqdm

from .adapter import AdapterServer

from .config import (
    DEFAULT_MEMORY_MODEL_CONFIG,
    DEFAULT_RANDOM_SEED,
    RETENTION_TARGETS,
    BenchPaths,
)
from .dhp_reference import DHPStudent, ensure_reference_assets, load_dhp_params, run_wordforge_reference
from .models import (
    DHPActiveModel,
    DHPBaseline,
    FSRS_BASELINE_CONFIG,
    GRUHLROracle,
    HLRBaseline,
    WordForgeAdapterModel,
    infer_half_life,
    metric_summary,
    probability_metric_summary,
    prediction_composite,
)


def _resolved_thread_count(requested: int | None) -> int:
    cpu_count = os.cpu_count() or 1
    return max(1, int(requested or cpu_count))


def _resolved_fit_device(requested: str | None) -> str:
    if requested:
        return requested
    # The current GRU oracle is tiny; CPU training with wider batches is
    # faster and makes thread utilization more obvious on this machine.
    return "cpu"


def _resolved_train_batch_size(device: str, requested: int | None) -> int:
    if requested is not None:
        return max(32, int(requested))
    return 4096 if device == "cpu" else 1024


def _resolved_calibration_batch_size(device: str, requested: int | None) -> int:
    if requested is not None:
        return max(32, int(requested))
    return 8192 if device == "cpu" else 2048


def _duckdb_connect(threads: int | None = None) -> duckdb.DuckDBPyConnection:
    conn = duckdb.connect()
    conn.execute(f"PRAGMA threads={_resolved_thread_count(threads)}")
    # 确定性断言：4M 行头部截断依赖插入序保持
    ordered = conn.execute("SELECT current_setting('preserve_insertion_order')").fetchone()[0]
    assert ordered is True or str(ordered).lower() == "true", (
        "duckdb preserve_insertion_order is off; deterministic head truncation broken"
    )
    return conn


def _configure_torch_threads(threads: int | None = None) -> int:
    resolved = _resolved_thread_count(threads)
    torch.set_num_threads(resolved)
    try:
        torch.set_num_interop_threads(max(1, min(4, resolved)))
    except RuntimeError:
        pass
    return resolved


def load_prefix_frame(paths: BenchPaths, split: str, user_fraction: float = 1.0) -> pd.DataFrame:
    # Within each split, user_bucket_100 has exactly 10 values (e.g., val: {8,18,28,...,98}).
    # To sub-sample, we take the first ceil(fraction*10) of these 10 sub-buckets
    # by filtering on (user_bucket_100 / 10) % 10 < sub_bucket_count.
    sub_bucket_count = max(1, min(10, int(math.ceil(user_fraction * 10))))
    if sub_bucket_count >= 10:
        bucket_filter = ""
    else:
        bucket_filter = f"AND (user_bucket_100 / 10) % 10 < {sub_bucket_count}"
    # 数据卫生过滤同 iter_prefix_batches（负 delta 行会杀死 adapter server）
    query = f"""
        SELECT *
        FROM read_parquet('{(paths.parquet / "prefix_events.parquet").as_posix()}')
        WHERE split = ?
          AND t_history NOT LIKE '%-%'
          {bucket_filter}
    """
    conn = _duckdb_connect()
    return conn.execute(query, [split]).fetch_df()


def iter_prefix_batches(
    paths: BenchPaths,
    split: str,
    user_fraction: float = 1.0,
    batch_rows: int = 100_000,
    max_rows: int | None = None,
    duckdb_threads: int | None = None,
    user_buckets: Sequence[int] | None = None,
):
    if user_buckets is not None:
        # 显式十分位桶选择（stage2 用不相交桶取得独立证据）；None 时与旧行为逐字节一致
        bucket_list = ", ".join(str(int(bucket)) for bucket in user_buckets)
        bucket_filter = f"AND CAST(FLOOR((user_bucket_100 / 10) % 10) AS INTEGER) IN ({bucket_list})"
    else:
        sub_bucket_count = max(1, min(10, int(math.ceil(user_fraction * 10))))
        if sub_bucket_count >= 10:
            bucket_filter = ""
        else:
            bucket_filter = f"AND (user_bucket_100 / 10) % 10 < {sub_bucket_count}"
    limit_sql = f"LIMIT {int(max_rows)}" if max_rows else ""
    # 数据卫生：时间戳乱序产生的负 delta 行（全库仅 test split 1 行）物理无意义，
    # 且会触发 adapter server 的 delta_t>=0 校验并杀死进程（单条坏请求 = 整轮评分失败）。
    query = f"""
        SELECT *
        FROM read_parquet('{(paths.parquet / "prefix_events.parquet").as_posix()}')
        WHERE split = ?
          AND t_history NOT LIKE '%-%'
          {bucket_filter}
        {limit_sql}
    """
    conn = _duckdb_connect(duckdb_threads)
    reader = conn.execute(query, [split]).to_arrow_reader(batch_size=batch_rows)
    yielded = 0
    for batch in reader:
        frame = batch.to_pandas()
        if max_rows is not None and yielded + len(frame) > max_rows:
            frame = frame.head(max_rows - yielded)
        if not frame.empty:
            yielded += len(frame)
            yield frame
        if max_rows is not None and yielded >= max_rows:
            break


class ReservoirSample:
    def __init__(self, limit: int = 200_000, seed: int = DEFAULT_RANDOM_SEED) -> None:
        self.limit = limit
        self.rng = random.Random(seed)
        self.values: List[tuple[float, float]] = []
        self.seen = 0

    def update(self, labels: np.ndarray, probs: np.ndarray) -> None:
        for label, prob in zip(labels.tolist(), probs.tolist()):
            self.seen += 1
            item = (float(label), float(prob))
            if len(self.values) < self.limit:
                self.values.append(item)
            else:
                index = self.rng.randint(0, self.seen - 1)
                if index < self.limit:
                    self.values[index] = item

    def arrays(self) -> tuple[np.ndarray, np.ndarray]:
        if not self.values:
            return np.empty(0), np.empty(0)
        labels = np.asarray([item[0] for item in self.values], dtype=float)
        probs = np.asarray([item[1] for item in self.values], dtype=float)
        return labels, probs


class StreamingPredictionAccumulator:
    def __init__(self) -> None:
        self.count = 0
        self.log_loss_sum = 0.0
        self.mae_sum = 0.0
        self.smape_sum = 0.0
        self.bin_counts = np.zeros(20, dtype=np.int64)
        self.bin_prob_sums = np.zeros(20, dtype=np.float64)
        self.bin_true_sums = np.zeros(20, dtype=np.float64)
        self.auc_sample = ReservoirSample(limit=200_000)

    def update(
        self,
        y_true: np.ndarray,
        probabilities: np.ndarray,
        half_lives: np.ndarray,
        true_half_lives: np.ndarray,
    ) -> None:
        if len(y_true) == 0:
            return
        probs = np.nan_to_num(probabilities.astype(float), nan=0.5, posinf=1.0, neginf=0.0)
        probs = np.clip(probs, 1e-6, 1 - 1e-6)
        truth = y_true.astype(float)
        pred_half_lives = np.nan_to_num(half_lives.astype(float), nan=1.0, posinf=3650.0, neginf=1.0)
        gt_half_lives = np.nan_to_num(true_half_lives.astype(float), nan=1.0, posinf=3650.0, neginf=1.0)
        self.count += len(truth)
        self.log_loss_sum += float(
            -np.sum(truth * np.log(probs) + (1.0 - truth) * np.log(1.0 - probs))
        )
        self.mae_sum += float(np.sum(np.abs(probs - truth)))
        self.smape_sum += float(
            np.sum(
                2.0
                * np.abs(pred_half_lives - gt_half_lives)
                / np.maximum(np.abs(pred_half_lives) + np.abs(gt_half_lives), 1e-9)
            )
        )
        bin_indices = np.minimum((probs * 20).astype(int), 19)
        for index in range(20):
            mask = bin_indices == index
            if not np.any(mask):
                continue
            self.bin_counts[index] += int(mask.sum())
            self.bin_prob_sums[index] += float(probs[mask].sum())
            self.bin_true_sums[index] += float(truth[mask].sum())
        self.auc_sample.update(truth, probs)

    def finalize(self) -> Dict[str, float]:
        if self.count == 0:
            return {"logLoss": 0.0, "ici": 0.0, "auc": 0.5, "maeP": 0.0, "smapeH": 0.0}
        ici = 0.0
        total = 0
        for count, prob_sum, true_sum in zip(self.bin_counts, self.bin_prob_sums, self.bin_true_sums):
            if count == 0:
                continue
            ici += count * abs((true_sum / count) - (prob_sum / count))
            total += count
        labels, probs = self.auc_sample.arrays()
        auc = 0.5
        if len(labels) > 0 and len(np.unique(labels)) > 1:
            from sklearn.metrics import roc_auc_score

            auc = float(roc_auc_score(labels, probs))
        return {
            "logLoss": float(self.log_loss_sum / self.count),
            "ici": float(ici / max(total, 1)),
            "auc": auc,
            "maeP": float(self.mae_sum / self.count),
            "smapeH": float(self.smape_sum / self.count),
        }


class StreamingIntervalAccumulator:
    def __init__(self) -> None:
        self.counts = {ret: 0 for ret in RETENTION_TARGETS}
        self.safety_sums = {ret: 0.0 for ret in RETENTION_TARGETS}
        self.efficiency_sums = {ret: 0.0 for ret in RETENTION_TARGETS}
        self.oracle_sums = {ret: 0.0 for ret in RETENTION_TARGETS}
        self.candidate_sums = {ret: 0.0 for ret in RETENTION_TARGETS}

    def update(
        self,
        oracle: GRUHLROracle,
        oracle_half_lives: np.ndarray,
        candidate_intervals: Dict[float, np.ndarray],
    ) -> None:
        half_lives = oracle_half_lives.astype(np.float64)

        for retention in RETENTION_TARGETS:
            intervals = candidate_intervals[retention]
            n = len(half_lives)

            # Vectorized binary search for optimal intervals
            optimal_days = _vectorized_oracle_deltas(oracle, half_lives, retention)

            # Vectorized calibrated probability for scheduled intervals
            raw_probs = np.power(2.0, -intervals / np.maximum(half_lives, 1e-6))
            calibrated = oracle.calibration.predict(raw_probs)

            self.counts[retention] += n
            self.safety_sums[retention] += float(np.sum(calibrated >= retention))
            self.efficiency_sums[retention] += float(
                np.sum(np.minimum(1.0, intervals / np.maximum(optimal_days, 1e-6)))
            )
            self.oracle_sums[retention] += float(np.sum(optimal_days))
            self.candidate_sums[retention] += float(np.sum(intervals))

    def finalize(self) -> Dict[str, Any]:
        scores: Dict[str, Dict[str, float]] = {}
        interval_scores = []
        for retention in RETENTION_TARGETS:
            count = max(self.counts[retention], 1)
            safety = self.safety_sums[retention] / count
            efficiency = self.efficiency_sums[retention] / count
            interval_score = safety * efficiency
            interval_scores.append(interval_score)
            scores[f"{retention:.2f}"] = {
                "safety": float(safety),
                "efficiency": float(efficiency),
                "intervalScore": float(interval_score),
                "oracleMeanDelta": float(self.oracle_sums[retention] / count),
                "candidateMeanDelta": float(self.candidate_sums[retention] / count),
            }
        return {
            "targets": scores,
            "policyScore": float(np.mean(interval_scores)) if interval_scores else 0.0,
        }


# ---------------------------------------------------------------------------
# Oracle fit (unchanged logic, slightly streamlined)
# ---------------------------------------------------------------------------

def fit_oracle(
    paths: BenchPaths,
    max_train_rows: int | None = None,
    epochs: int = 5,
    device: str | None = None,
    torch_threads: int | None = None,
    loader_workers: int | None = None,
    duckdb_threads: int | None = None,
    train_batch_size: int | None = None,
    calibration_batch_size: int | None = None,
    hidden_size: int = 64,
    train_passes: int = 3,
) -> Dict[str, Any]:
    resolved_epochs = max(1, int(epochs))
    resolved_passes = max(1, int(train_passes))
    resolved_torch_threads = _configure_torch_threads(torch_threads)
    resolved_duckdb_threads = _resolved_thread_count(duckdb_threads)
    resolved_device = _resolved_fit_device(device)
    resolved_train_batch_size = _resolved_train_batch_size(resolved_device, train_batch_size)
    resolved_calibration_batch_size = _resolved_calibration_batch_size(
        resolved_device, calibration_batch_size
    )
    oracle = GRUHLROracle(
        device=resolved_device,
        loader_workers=loader_workers,
        hidden_size=hidden_size,
    )
    oracle_path = paths.artifacts / "gru_hlr_oracle"

    # Materialize training batches for multi-pass iteration
    train_frames = list(iter_prefix_batches(
        paths, "train", 1.0, max_rows=max_train_rows, duckdb_threads=resolved_duckdb_threads
    ))

    hlr = HLRBaseline()
    for train_batch in train_frames:
        hlr.partial_fit(train_batch)

    # Count total mini-batch steps for LR schedule
    steps_per_pass = sum(
        max(1, math.ceil(len(frame) / resolved_train_batch_size))
        for frame in train_frames
    ) * resolved_epochs
    total_steps = resolved_passes * steps_per_pass

    # Cosine LR with warmup
    peak_lr = 3e-3
    min_lr = 1e-5
    warmup_steps = int(total_steps * 0.05)

    optimizer = torch.optim.Adam(oracle.model.parameters(), lr=peak_lr, weight_decay=1e-5)

    def _lr_lambda(step):
        if step < warmup_steps:
            return (min_lr + (peak_lr - min_lr) * step / max(warmup_steps, 1)) / peak_lr
        progress = (step - warmup_steps) / max(total_steps - warmup_steps, 1)
        return (min_lr + 0.5 * (peak_lr - min_lr) * (1.0 + math.cos(math.pi * progress))) / peak_lr

    scheduler = torch.optim.lr_scheduler.LambdaLR(optimizer, _lr_lambda)

    # Multi-pass training with best checkpoint selection
    calibration_limit = max_train_rows // 4 if max_train_rows and max_train_rows > 4 else max_train_rows
    best_val_loss = float("inf")
    best_state_dict = None

    for pass_idx in range(resolved_passes):
        for train_batch in train_frames:
            oracle._train_on_frame(
                train_batch,
                optimizer=optimizer,
                batch_size=resolved_train_batch_size,
                epochs=resolved_epochs,
                scheduler=scheduler,
            )

        # Evaluate val logLoss after each pass
        val_acc = StreamingPredictionAccumulator()
        for val_batch in iter_prefix_batches(
            paths, "val", 1.0, max_rows=calibration_limit, duckdb_threads=resolved_duckdb_threads
        ):
            y_true = val_batch["next_r"].to_numpy(dtype=float)
            true_hl = np.asarray(
                [infer_half_life(t, p) for t, p in zip(val_batch["next_t"], val_batch["dhp_p_recall"])],
                dtype=np.float64,
            )
            pred = oracle.predict_raw(val_batch, batch_size=resolved_calibration_batch_size)
            val_acc.update(y_true, pred.probabilities, pred.half_lives, true_hl)
        val_metrics = val_acc.finalize()
        if val_metrics["logLoss"] < best_val_loss:
            best_val_loss = val_metrics["logLoss"]
            best_state_dict = {k: v.clone() for k, v in oracle.model.state_dict().items()}

    # Restore best checkpoint
    if best_state_dict is not None:
        oracle.model.load_state_dict(best_state_dict)

    probs, labels = oracle._collect_calibration_points(
        iter_prefix_batches(
            paths, "val", 1.0, max_rows=calibration_limit, duckdb_threads=resolved_duckdb_threads
        ),
        batch_size=resolved_calibration_batch_size,
    )
    mask = np.isfinite(probs) & np.isfinite(labels)
    probs = probs[mask]
    labels = labels[mask]
    if probs.size == 0:
        raise RuntimeError("no validation rows available to calibrate oracle")
    raw_calibration_metrics = probability_metric_summary(labels, probs)
    calibration_selection = oracle.fit_calibration(probs, labels)
    oracle.save(oracle_path)
    hlr.save(paths.artifacts / "hlr_baseline.pkl")

    oracle_acc = StreamingPredictionAccumulator()
    raw_oracle_acc = StreamingPredictionAccumulator()
    hlr_acc = StreamingPredictionAccumulator()
    for val_batch in iter_prefix_batches(
        paths, "val", 1.0, max_rows=calibration_limit, duckdb_threads=resolved_duckdb_threads
    ):
        y_true = val_batch["next_r"].to_numpy(dtype=float)
        true_half_lives = np.asarray(
            [infer_half_life(t, p) for t, p in zip(val_batch["next_t"], val_batch["dhp_p_recall"])],
            dtype=np.float64,
        )
        oracle_raw_prediction = oracle.predict_raw(val_batch, batch_size=resolved_calibration_batch_size)
        oracle_prediction = oracle.predict(val_batch)
        hlr_prediction = hlr.predict(val_batch)
        raw_oracle_acc.update(
            y_true,
            oracle_raw_prediction.probabilities,
            oracle_raw_prediction.half_lives,
            true_half_lives,
        )
        oracle_acc.update(y_true, oracle_prediction.probabilities, oracle_prediction.half_lives, true_half_lives)
        hlr_acc.update(y_true, hlr_prediction.probabilities, hlr_prediction.half_lives, true_half_lives)
    raw_oracle_metrics = raw_oracle_acc.finalize()
    oracle_metrics = oracle_acc.finalize()
    hlr_metrics = hlr_acc.finalize()
    def _finite(v):
        return isinstance(v, (int, float)) and math.isfinite(v)

    oracle_beats_hlr = (
        (_finite(oracle_metrics["logLoss"]) and _finite(hlr_metrics["logLoss"])
         and oracle_metrics["logLoss"] < hlr_metrics["logLoss"])
        or (_finite(oracle_metrics["maeP"]) and _finite(hlr_metrics["maeP"])
            and _finite(oracle_metrics["auc"]) and _finite(hlr_metrics["auc"])
            and oracle_metrics["maeP"] < hlr_metrics["maeP"]
            and oracle_metrics["auc"] > hlr_metrics["auc"])
    )

    summary = {
        "runtime": {
            "device": oracle.device,
            "hiddenSize": oracle.hidden_size,
            "epochs": resolved_epochs,
            "trainPasses": resolved_passes,
            "torchThreads": resolved_torch_threads,
            "loaderWorkers": oracle.loader_workers,
            "duckdbThreads": resolved_duckdb_threads,
            "trainBatchSize": resolved_train_batch_size,
            "calibrationBatchSize": resolved_calibration_batch_size,
        },
        "rawOracle": raw_oracle_metrics,
        "rawCalibration": raw_calibration_metrics,
        "calibrationSelection": calibration_selection,
        "oracle": oracle_metrics,
        "hlrBaseline": hlr_metrics,
        "oracleBeatsHlr": oracle_beats_hlr,
    }
    (paths.reports / "fit_oracle_summary.json").write_text(
        json.dumps(summary, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    return summary


def _load_oracle(paths: BenchPaths) -> GRUHLROracle:
    return GRUHLROracle.load(paths.artifacts / "gru_hlr_oracle")


def _load_hlr(paths: BenchPaths) -> HLRBaseline:
    return HLRBaseline.load(paths.artifacts / "hlr_baseline.pkl")


def _ensure_oracle_guard(paths: BenchPaths) -> None:
    summary_path = paths.reports / "fit_oracle_summary.json"
    if not summary_path.exists():
        raise RuntimeError("missing fit_oracle_summary.json; run fit_oracle first")
    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    if not summary.get("oracleBeatsHlr", False):
        raise RuntimeError(
            "oracleBeatsHlr=false; refusing to tune until oracle beats HLR on val "
            "for logLoss OR (maeP AND auc)"
        )


def _vectorized_oracle_deltas(
    oracle: GRUHLROracle,
    half_lives: np.ndarray,
    retention: float,
) -> np.ndarray:
    """Vectorized binary search for optimal intervals across all rows."""
    n = len(half_lives)
    hl = np.maximum(half_lives, 1e-6)

    low = np.zeros(n, dtype=np.float64)
    high = np.maximum(1.0, hl * 12.0)

    # Expand high where calibrated prob > retention
    for _ in range(20):
        raw = np.power(2.0, -high / hl)
        cal = oracle.calibration.predict(raw)
        mask = cal > retention
        if not np.any(mask):
            break
        high[mask] *= 2.0

    # Vectorized binary search
    for _ in range(40):
        mid = (low + high) / 2.0
        raw = np.power(2.0, -mid / hl)
        cal = oracle.calibration.predict(raw)
        go_high = cal >= retention
        low = np.where(go_high, mid, low)
        high = np.where(go_high, high, mid)

    return np.maximum(1.0, low)


def _oracle_delta(oracle: GRUHLROracle, half_life: float, retention: float) -> float:
    """Single-row version kept for backward compatibility."""
    result = _vectorized_oracle_deltas(oracle, np.asarray([half_life]), retention)
    return float(result[0])


def _dhp_reference(memory_config: Dict[str, Any], paths: BenchPaths) -> Dict[str, Any]:
    assets = ensure_reference_assets(paths.cache / "maimemo_reference")
    current = run_wordforge_reference(
        assets["policy_dir"],
        assets["parameters"],
        memory_config=memory_config,
    )
    return {"wordforge": current, "configEcho": memory_config}


# 预注册 per-leg 守门下限（基线 = (NM0 w, tau=0.0) = 当前生产冠军；2026-06-12 tau 网格实测
# 校准，详见 docs/amas-tuning-2026-06-12/00-pre-registered-gates.md）：
#   - expectedMemory / nextDayMemory ≥ 0.90x：沿用历史纪律；tau 网格实测 tau∈{2.5,3.0,3.5}
#     反而抬升这两腿（1.16-1.20x），0.90x 防的是 w 侧记忆质量塌方
#   - targetCount ≥ 0.95x（0.90→0.95 收紧）：tau 对该腿近中性（0.99-1.03x），更大跌幅只能
#     来自 w 侧用真实半衰期换 mastered 的套利；tau3-stock 实测 0.9899x 留 4% 余量
#   - masteredProxy ≥ 1.00x（新腿）：冲榜指标本体——胜者不得低于现冠军自报掌握量；
#     tau3-stock 实测 1.954x，余量巨大
DHP_GUARDRAIL_FLOORS: Dict[str, float] = {
    "expectedMemory": 0.90,
    "nextDayMemory": 0.90,
    "targetCount": 0.95,
    "masteredProxy": 1.00,
}
DHP_CONSTRAINT_KEYS = tuple(DHP_GUARDRAIL_FLOORS)
# 锚点自检靶：stock 配置 == baseline 配置 ⇒ 每腿比率恰为 1.0 ⇒ 分量恰为 floor-1.0
DHP_ANCHOR_COMPONENTS = tuple(floor - 1.0 for floor in DHP_GUARDRAIL_FLOORS.values())

# Stage-1/2 mastered 感知目标（2026-06-12 预注册，公式不得事后修改）：
#   objective = prediction_composite(ratio_cap=1.5)
#             + MASTERED_OBJECTIVE_WEIGHT * min(masteredProxy/baseline - 1.0, MASTERED_OBJECTIVE_RATIO_CAP)
# Stage-3 保持纯 prediction composite（uncapped），masteredProxy 仅记录不进目标。
MASTERED_OBJECTIVE_WEIGHT = 0.5
MASTERED_OBJECTIVE_RATIO_CAP = 1.5


def _mastered_objective_term(dhp: Dict[str, Any], baseline_dhp: Dict[str, Any]) -> float:
    base = float(baseline_dhp["masteredProxy"])
    ratio = float(dhp["masteredProxy"]) / max(base, 1.0)
    return MASTERED_OBJECTIVE_WEIGHT * min(ratio - 1.0, MASTERED_OBJECTIVE_RATIO_CAP)


def _dhp_constraints(
    dhp: Dict[str, Any], baseline_dhp: Dict[str, Any]
) -> Tuple[bool, Tuple[float, ...]]:
    """Returns (feasible: bool, components: tuple[float,...] aligned with DHP_CONSTRAINT_KEYS).

    feasible uses the GATE'S LITERAL expression dhp[k] >= floor_k*base[k] (boolean authority);
    components are normalized magnitudes floor_k - dhp[k]/base[k] for TPE's least-violation ranking.
    The boolean and the magnitudes are DECOUPLED on purpose (1-ulp boundary hardening):
    if feasible, components are clamped to <= 0.0 (min(c, 0.0)); if infeasible, the violated
    components are forced >= +1e-12. Non-finite dhp value or base <= 0 → feasible=False, component=1.0
    (never NaN — optuna raises ValueError on NaN constraints inside tell(), stranding a poisoned trial).
    """
    feasible = True
    components: List[float] = []
    for key, floor in DHP_GUARDRAIL_FLOORS.items():
        value = dhp.get(key)
        base = baseline_dhp.get(key)
        if (
            not isinstance(value, (int, float))
            or not isinstance(base, (int, float))
            or not math.isfinite(value)
            or not math.isfinite(base)
            or base <= 0
        ):
            feasible = False
            components.append(1.0)
            continue
        leg_ok = value >= floor * base
        component = floor - value / base
        components.append(min(component, 0.0) if leg_ok else max(component, 1e-12))
        feasible = feasible and leg_ok
    return feasible, tuple(components)


def _make_constrained_sampler(seed: int, n_startup_trials: int) -> optuna.samplers.TPESampler:
    # constraints_func 与 multivariate 在构造时各发一条 ExperimentalWarning，就地过滤；
    # 约束绑定协议（optuna 4.8.0 源码核验）：tell() 内以 state=RUNNING 调 constraints_func，
    # 因此每次 tell 前必须无条件 set_user_attr("constraint", ...)
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", optuna.exceptions.ExperimentalWarning)
        return optuna.samplers.TPESampler(
            seed=seed,
            multivariate=True,
            n_startup_trials=n_startup_trials,
            constraints_func=lambda t: t.user_attrs["constraint"],
        )


def _warn_if_unverified_optuna() -> None:
    if optuna.__version__ != "4.8.0":
        warnings.warn(
            "constrained tune() relies on optuna 4.8.0 internals (tell-time constraint "
            f"binding, infeasible values ignored by TPE); got {optuna.__version__}",
            RuntimeWarning,
        )


def _verify_constrained_tpe() -> None:
    """启动金丝雀：toy 约束研究断言 best_trial 是可行最优（x≈0.5 区），而非裸最优（x→1）。"""
    study = optuna.create_study(
        direction="maximize", sampler=_make_constrained_sampler(DEFAULT_RANDOM_SEED, 4)
    )
    xs: List[float] = []
    for _ in range(15):
        trial = study.ask()
        x = trial.suggest_float("x", 0.0, 1.0)
        trial.set_user_attr("constraint", (x - 0.5,))
        study.tell(trial, x)
        xs.append(x)
    feasible = [x for x in xs if x <= 0.5]
    if not feasible or study.best_trial.params["x"] != max(feasible):
        raise RuntimeError(
            "constrained TPE canary failed: best_trial is not the feasible best"
        )


# ---------------------------------------------------------------------------
# SINGLE-PASS evaluate: all models in one data traversal
# ---------------------------------------------------------------------------

def _single_pass_evaluate(
    paths: BenchPaths,
    memory_config: Dict[str, Any],
    split: str,
    user_fraction: float,
    hlr_model: HLRBaseline,
    oracle: GRUHLROracle,
    server=None,
    include_interval: bool = True,
    include_baselines: bool = True,
    skip_slow_baselines: bool = False,
    max_rows: int | None = None,
    dhp_student: DHPStudent | None = None,
    user_buckets: Sequence[int] | None = None,
) -> Dict[str, Any]:
    """Evaluate models in a SINGLE pass over the data.

    When skip_slow_baselines=True, only fast models (WF, FSRS, DHP) run.
    Oracle GRU and HLR sklearn are skipped to save time.
    """

    # Accumulators for prediction metrics
    acc_wf = StreamingPredictionAccumulator()
    acc_fsrs = StreamingPredictionAccumulator() if include_baselines else None
    acc_dhp = StreamingPredictionAccumulator() if (include_baselines and not skip_slow_baselines) else None
    acc_hlr = StreamingPredictionAccumulator() if (include_baselines and not skip_slow_baselines) else None

    # Accumulator for interval policy (requires Oracle — only when not skipping)
    interval_acc = StreamingIntervalAccumulator() if (include_interval and not skip_slow_baselines) else None

    row_count = 0
    batch_count = 0

    wf_model = WordForgeAdapterModel(memory_config, server=server)
    fsrs_model = WordForgeAdapterModel(FSRS_BASELINE_CONFIG, server=server) if include_baselines else None
    dhp_model = DHPActiveModel(dhp_student) if (include_baselines and not skip_slow_baselines and dhp_student is not None) else None

    for frame in tqdm(
        iter_prefix_batches(
            paths, split=split, user_fraction=user_fraction, max_rows=max_rows,
            user_buckets=user_buckets,
        ),
        desc=f"eval({split} {user_fraction:.0%})",
        unit="batch",
    ):
        batch_count += 1
        row_count += len(frame)

        y_true = frame["next_r"].to_numpy(dtype=float)
        true_half_lives = np.asarray(
            [infer_half_life(t, p) for t, p in zip(frame["next_t"], frame["dhp_p_recall"])],
            dtype=np.float64,
        )

        # WordForge candidate
        wf_pred = wf_model.predict(frame)
        acc_wf.update(y_true, wf_pred.probabilities, wf_pred.half_lives, true_half_lives)

        # FSRS baseline (fast — uses same Rust server)
        if acc_fsrs is not None:
            fsrs_pred = fsrs_model.predict(frame)
            acc_fsrs.update(y_true, fsrs_pred.probabilities, fsrs_pred.half_lives, true_half_lives)

        # Interval policy (wordforge vs oracle) — only when not skipping slow models
        if interval_acc is not None:
            oracle_pred = oracle.predict(frame)
            interval_acc.update(oracle, oracle_pred.half_lives, wf_pred.interval_days)

        # Slow baselines (DHP, HLR) — skipped in full-data pass
        if acc_dhp is not None:
            dhp_pred = dhp_model.predict(frame)
            acc_dhp.update(y_true, dhp_pred.probabilities, dhp_pred.half_lives, true_half_lives)

        if acc_hlr is not None:
            hlr_pred = hlr_model.predict(frame)
            acc_hlr.update(y_true, hlr_pred.probabilities, hlr_pred.half_lives, true_half_lives)

    if row_count == 0:
        print(f"WARNING: 0 rows processed for split={split}, user_fraction={user_fraction}")

    result: Dict[str, Any] = {
        "wordforge": acc_wf.finalize(),
        "rows": row_count,
        "batches": batch_count,
    }
    if acc_fsrs is not None:
        result["fsrsBaseline"] = acc_fsrs.finalize()
    if acc_dhp is not None:
        result["dhp"] = acc_dhp.finalize()
    if acc_hlr is not None:
        result["hlr"] = acc_hlr.finalize()
    if interval_acc is not None:
        result["interval_policy"] = interval_acc.finalize()

    return result


def evaluate(paths: BenchPaths, memory_config: Dict[str, Any], split: str = "val", user_fraction: float = 1.0) -> Dict[str, Any]:
    _ensure_oracle_guard(paths)
    hlr_model = _load_hlr(paths)
    oracle = _load_oracle(paths)

    # Load DHP student for active model
    assets = ensure_reference_assets(paths.cache / "maimemo_reference")
    dhp_student = DHPStudent(**load_dhp_params(assets["parameters"]))

    # Determine sample fraction for the expensive baseline+Oracle pass
    # 1% (230k rows) is still statistically sufficient for baseline comparisons
    baseline_fraction = min(user_fraction, 0.01)

    with AdapterServer() as server:
        # Pass 1: All models + Oracle on small sample — for baseline metrics + interval policy
        metrics_results = _single_pass_evaluate(
            paths, memory_config, split, baseline_fraction,
            hlr_model=hlr_model, oracle=oracle, server=server,
            include_interval=True, include_baselines=True,
            max_rows=250_000,
            dhp_student=dhp_student,
        )

        # Pass 2: WordForge-only on full data (baselines from sample pass)
        if user_fraction > baseline_fraction:
            full_results = _single_pass_evaluate(
                paths, memory_config, split, user_fraction,
                hlr_model=hlr_model, oracle=oracle, server=server,
                include_interval=False, include_baselines=False,
            )
            # Use full-data WordForge prediction as the primary
            wordforge_prediction = full_results["wordforge"]
            total_rows = full_results["rows"]
        else:
            wordforge_prediction = metrics_results["wordforge"]
            total_rows = metrics_results["rows"]

        # Baseline for composite score (default config on same sample)
        baseline_results = _single_pass_evaluate(
            paths, DEFAULT_MEMORY_MODEL_CONFIG, split, baseline_fraction,
            hlr_model=hlr_model, oracle=oracle, server=server,
            include_interval=False, include_baselines=False,
            max_rows=250_000,
        )

    baseline_prediction = baseline_results["wordforge"]
    baseline_score = prediction_composite(wordforge_prediction, baseline_prediction)

    dhp_reference = _dhp_reference(memory_config, paths)

    return {
        "prediction": {
            "wordforge": wordforge_prediction,
            "fsrsBaseline": metrics_results.get("fsrsBaseline", {}),
            "dhp": metrics_results.get("dhp", {}),
            "hlr": metrics_results.get("hlr", {}),
        },
        "interval_policy": metrics_results.get("interval_policy", {}),
        "dhp_reference": dhp_reference,
        "selection_summary": {
            "split": split,
            "userFraction": user_fraction,
            "baselineSampleFraction": baseline_fraction,
            "rows": total_rows,
            "predictionScore": baseline_score,
        },
        "candidate_config": {"memoryModel": memory_config},
    }


# ---------------------------------------------------------------------------
# Tune (optimized: fewer trials, shared server, single-pass scoring)
# ---------------------------------------------------------------------------

# 14 个双活（objective-live ∧ gate-live）w 维 + 1 个非 w 结构旋钮（alphaRampTau），
# 其余维度钉死在 DEFAULT 值。2026-06-11 镜像对齐后 per-dim 探针重测：对齐解锁难度链路
# w4-w7（mean reversion 锚 d0(4) 持续作用，w4 守门峰值 40.7%）与遗忘分支 w11-w14
# （G=1 路径双侧可达）。
# 2026-06-12 增量：
#   - alphaRampTau (1.5, 4.0)：val tau 网格在 stock w 下呈内点最优 tau=3.0（tau=4.0 全腿
#     回落；tau∈{1.5,2.0} 触 duo expMemFinal 塌方线被拒）——峰值可能随 w 移动，进搜索；
#     tau=0（关闭）仅作为锚点种子在窗口外注入，TPE 分布不含 0
#   - w_2 5.0→8.5 / w_8 3.0→3.5 / w_10 1.5→2.0 拓宽：旧"公版即最优"结论是在 alpha=0.3
#     冻结语义下得出的，D1（连击动态 alpha）+ D2（alphaRampTau）落地后该结论失效；
#     duo/syn mastered 残差（x1.68/x1.57）需要增长侧 headroom；记忆质量由 per-leg
#     守门下限（DHP_GUARDRAIL_FLOORS）与胜者级二元闸门兜底。其余窗口不变。
SEARCH_WINDOWS: Dict[str, Tuple[float, float]] = {
    "w_0": (0.05, 0.60),
    "w_2": (1.00, 8.50),
    "w_4": (3.50, 9.50),
    "w_5": (0.30, 2.00),
    "w_6": (1.00, 4.00),
    "w_7": (0.001, 0.15),
    "w_8": (1.00, 3.50),
    "w_9": (0.05, 0.40),
    "w_10": (0.30, 2.00),
    "w_11": (0.50, 3.00),
    "w_12": (0.01, 0.25),
    "w_13": (0.05, 0.70),
    "w_14": (0.50, 3.50),
    "w_20": (0.10, 0.80),
    "alphaRampTau": (1.5, 4.0),
}
_W_INDEX = {
    name: int(name.split("_")[1]) for name in SEARCH_WINDOWS if name.startswith("w_")
}


def _mutate_config(
    trial: optuna.Trial, windows: Dict[str, Tuple[float, float]] | None = None
) -> Dict[str, Any]:
    """Search space = 14 双活 w dims + alphaRampTau（2026-06-12 升 15 维）。

    双活 = objective-live（adapter replay 路径结构可达）∧ gate-live（对齐镜像闭环探针
    腿变化 ≥0.1%）。继承 6 维：w_0/w_2/w_8/w_9/w_10/w_20（2026-06-12 拓宽
    w_2/w_8/w_10，理由见 SEARCH_WINDOWS 注释）；对齐解锁 8 维：
      - w_4/w_5/w_6/w_7：难度动力学 — 旧镜像 D 冻结使其守门盲；对齐后 mean reversion
        锚 d0(4)=f(w4,w5) 持续作用，w_4 守门峰值 40.7% 为全空间最强杠杆
      - w_11/w_12/w_13/w_14：遗忘分支 — G=1 双侧可达，守门 2.8-3.8%

    维持钉死，逐维理由：
      - w_1, w_15：三路全死 — 二值映射使 Hard(2) 不可达（grade 对齐后依旧）
      - w_3, w_16：三路全死 — 成功现映射 Good(3)，Easy(4) 双侧不可达（对齐前为
        gate-only 失真源，正是本次镜像修复的主病灶）
      - w_17/w_18/w_19：objective-live 但 gate-blind — 闭环模拟无同日复习（due≥day+1），
        探针三腿 0.00%；进空间有守门外滑移风险
      - base_desired_retention：objective-blind 且 gate-critical（2026-06-10 全员被拒根因）；
        生产 retention 敏感性由报告行 retentionSensitivity 覆盖（D9）
      - max_interval_days：gate 不可见（mirror 与 Rust 同享 90d 默认顶帽，且 90 天闭环内
        due=day+顶帽 恒越仿真窗、复习计划不受影响），仅剩可被套利的弱效率信号

    windows 仅供 D10 局部精炼传缩窄盒；None = 原始窗口。
    非 w 键（alphaRampTau）直接写 config 顶层标量；DEFAULT 自带 0.0（关闭）。
    """
    config = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
    weights = list(config["w"])
    for name, (low, high) in (windows or SEARCH_WINDOWS).items():
        value = trial.suggest_float(name, low, high)
        if name in _W_INDEX:
            weights[_W_INDEX[name]] = value
        else:
            config[name] = value
    config["w"] = weights
    return config


_STOCK_ANCHOR_PARAMS: Dict[str, float] = {
    **{name: float(DEFAULT_MEMORY_MODEL_CONFIG["w"][idx]) for name, idx in _W_INDEX.items()},
    # 锚点 tau=0.0 在搜索窗 (1.5,4.0) 之外：optuna 4.8.0 对窗口外 fixed param 仅发
    # UserWarning 并 VERBATIM 透传（已实测 30-trial TPE 不崩），保证 trial 0 配置
    # 与守门基线逐位相等，约束自检 (floor-1.0) 接线成立
    "alphaRampTau": float(DEFAULT_MEMORY_MODEL_CONFIG["alphaRampTau"]),
}
# 2026-06-10 run 三个 near-miss 在 6 个存活维上的投影（钉死维回退 stock）——直接检验
# +29-40% 校准角在移除 gate-killer 后能否幸存。来源（冻结历史记录，全精度照抄）：
# ~/.wordforge-bench/maimemo/reports/tuning_summary.json nearMisses[0..2]
_REPAIRED_NEAR_MISSES: Tuple[Dict[str, float], ...] = (
    {
        "w_0": 0.07978223679403902,
        "w_2": 3.02498794640543,
        "w_8": 1.6259347829476551,
        "w_9": 0.2533678250394599,
        "w_10": 1.1124862163489901,
        "w_20": 0.2632510019126722,
    },
    {
        "w_0": 0.08041269355978323,
        "w_2": 2.9305596369135642,
        "w_8": 1.6241486350160141,
        "w_9": 0.2520068375839821,
        "w_10": 1.1287184743061447,
        "w_20": 0.27197432266552485,
    },
    {
        "w_0": 0.09198069520607176,
        "w_2": 2.5731565730123,
        "w_8": 1.6065116959233467,
        "w_9": 0.2455277063490539,
        "w_10": 1.1281524448213665,
        "w_20": 0.2710885682127271,
    },
)


def _seed_trial_params() -> List[Dict[str, float]]:
    """D5 教师种子，14 个（FIFO 入队即弹出顺序；键名与 suggest 名严格一致）。

    enqueue 按 VERBATIM 透传不裁剪越界值；除锚点 alphaRampTau=0.0（刻意窗口外，
    见 _STOCK_ANCHOR_PARAMS 注释）外所有取值在 SEARCH_WINDOWS 内（由
    test_seed_trials_inside_windows 守护）。15 维空间下种子必须全维显式
    （部分指定会让 sampler 随机补维，破坏教学确定性）。
    2026-06-12：性能教师统一带 tau=3.0（val tau 网格 stock-w 内点最优）；
    另设 tau 阶梯教师 {2.5, 3.5}（stock w）教 TPE tau 维曲率。
    """
    stock = dict(_STOCK_ANCHOR_PARAMS)          # tau=0.0（锚点专用，窗口外）
    perf = {**stock, "alphaRampTau": 3.0}       # 性能教师基座
    half_step = {
        name: (perf[name] + _REPAIRED_NEAR_MISSES[0].get(name, perf[name])) / 2.0
        for name in SEARCH_WINDOWS
    }
    return [
        stock,                                  # 1. stock 锚点（约束接线自检靶；tau=0.0）
        {**perf, **_REPAIRED_NEAR_MISSES[0]},   # 2-4. 修复后的 near-miss（新维回 stock）
        {**perf, **_REPAIRED_NEAR_MISSES[1]},
        {**perf, **_REPAIRED_NEAR_MISSES[2]},
        half_step,                              # 5. perf↔NM0 每维中点（tau=3.0）
        {**perf, "w_20": 0.22},                 # 6-7. w_20 阶梯
        {**perf, "w_20": 0.30},
        {**perf, "w_0": 0.10},                  # 8. w_0 探针
        # 9-12. 对齐解锁维的边界教师（2026-06-11）：教 TPE 新维梯度方向
        {**perf, "w_4": 4.5},                   # D0 锚下探（probe ×0.75 守门 5%）
        {**perf, "w_4": 8.0},                   # D0 锚上探
        {**perf, "w_11": 1.0, "w_12": 0.10, "w_13": 0.40, "w_14": 2.30},  # 遗忘组向 FSRS-5 官方风格
        {**perf, "w_6": 2.0, "w_7": 0.05},      # 难度增量减半 + 真 mean reversion
        # 13-14. tau 阶梯教师（stock w）：内点最优两翼
        {**stock, "alphaRampTau": 2.5},
        {**stock, "alphaRampTau": 3.5},
    ]


def _candidate_score(
    paths: BenchPaths,
    split: str,
    user_fraction: float,
    memory_config: Dict[str, Any],
    baseline_prediction: Dict[str, float],
    oracle: GRUHLROracle,
    hlr_model: HLRBaseline,
    server=None,
    max_rows: int | None = None,
    include_interval: bool = True,
    ratio_cap: float | None = None,
    user_buckets: Sequence[int] | None = None,
) -> Dict[str, Any]:
    results = _single_pass_evaluate(
        paths, memory_config, split, user_fraction,
        hlr_model=hlr_model, oracle=oracle, server=server,
        include_interval=include_interval, include_baselines=False,
        max_rows=max_rows, user_buckets=user_buckets,
    )
    prediction = results["wordforge"]
    # D2 语义变更：objective == 该阶段使用的 prediction composite（s1/s2 capped@1.5
    # 防单指标劫持，s3/selected uncapped 与旧闸门标量一致）。0.15*avgEfficiency 项已删——
    # 其唯一搜索消费方 maxIntervalDays/retention 均已钉死；iterative_tune 读
    # selected.metrics.objective 仍内部自洽。
    prediction_score = prediction_composite(prediction, baseline_prediction, ratio_cap=ratio_cap)
    metrics: Dict[str, Any] = {
        "objective": prediction_score,
        "prediction": prediction,
        "predictionScore": prediction_score,
        "rows": results["rows"],
    }
    if include_interval:
        interval_policy = results["interval_policy"]
        # policyScore = safety * efficiency is structurally ≈0 on MaiMemo data:
        # the GRU oracle infers very short half-lives (optimalMeanDelta=1.0 day),
        # making safety=0 regardless of model quality. avgEfficiency 仅保留为
        # 报告字段（tuning_summary 向后兼容），不再参与 objective。
        avg_efficiency = float(
            sum(interval_policy["targets"][f"{ret:.2f}"]["efficiency"] for ret in RETENTION_TARGETS)
            / max(len(RETENTION_TARGETS), 1)
        )
        metrics["intervalPolicy"] = interval_policy
        metrics["avgEfficiency"] = avg_efficiency
    return metrics


def iterative_tune(
    paths: BenchPaths,
    max_iterations: int = 10,
    convergence_threshold: float = 0.01,
    convergence_patience: int = 2,
    stage1_trials: int = 128,
    verbose: bool = True,
) -> Dict[str, Any]:
    """Iterative tuning with convergence checking.

    Runs tune() repeatedly until convergence or max_iterations reached.

    Parameters:
    -----------
    paths: BenchPaths
        Benchmark data paths
    max_iterations: int
        Maximum number of tune() iterations (default 10)
    convergence_threshold: float
        Relative improvement threshold for convergence (default 0.01 = 1%)
    convergence_patience: int
        Number of iterations without improvement before stopping (default 2)
    stage1_trials: int
        Number of Stage 1 trials per iteration (default 128, can increase to 256)
    verbose: bool
        Print convergence log (default True)

    Returns:
    --------
    Dict[str, Any]
        Final tuning result with convergence metadata
    """
    iterations: List[Dict[str, Any]] = []
    best_objective = 0.0
    patience_counter = 0

    for iteration in range(max_iterations):
        if verbose:
            print(f"\n{'='*70}")
            print(f"Iteration {iteration + 1}/{max_iterations}")
            print(f"{'='*70}")

        # Keep stage1 trial count constant; TPE has converged with smaller dim space
        result = tune(paths, stage1_trials=stage1_trials)

        selected = result["selected"]
        objective = selected["metrics"]["objective"]
        iterations.append({
            "iteration": iteration + 1,
            "objective": objective,
            "prediction_gain": selected["predictionGainPercent"],
            "interval_gain": selected["interval85GainPercent"],
            "passes": selected["passes"],
            "config_hash": _config_hash_short(selected["memoryModel"]),
        })

        if verbose:
            print(f"Iteration {iteration + 1} Results:")
            print(f"  Objective: {objective:.6f}")
            print(f"  Prediction Gain: {selected['predictionGainPercent']:.2f}%")
            print(f"  Interval Gain: {selected['interval85GainPercent']:.2f}%")
            print(f"  Passes: {selected['passes']}")

        # Convergence check
        if iteration == 0:
            best_objective = objective
            patience_counter = 0
        else:
            relative_improvement = (objective - best_objective) / best_objective
            if verbose:
                print(f"  Relative Improvement: {relative_improvement * 100:.3f}%")
                print(f"  Patience: {patience_counter}/{convergence_patience}")

            if relative_improvement >= convergence_threshold:
                best_objective = objective
                patience_counter = 0
                if verbose:
                    print(f"  → Improvement found, reset patience counter")
            else:
                patience_counter += 1
                if verbose:
                    print(f"  → No significant improvement, patience+1")

            if patience_counter >= convergence_patience:
                if verbose:
                    print(f"\n{'='*70}")
                    print(f"✅ Convergence reached after {iteration + 1} iterations")
                    print(f"   Threshold: {convergence_threshold * 100:.2f}%")
                    print(f"   Final Objective: {best_objective:.6f}")
                    print(f"{'='*70}\n")
                break

    # Add convergence metadata
    result["convergence_log"] = iterations
    result["converged"] = patience_counter >= convergence_patience or iteration == max_iterations - 1
    result["iterations_completed"] = iteration + 1
    result["convergence_threshold"] = convergence_threshold

    return result


def _config_hash_short(config: Dict[str, Any], length: int = 8) -> str:
    """Generate short hash of config for logging."""
    import hashlib
    config_json = json.dumps(config, sort_keys=True, separators=(',', ':'))
    hash_obj = hashlib.md5(config_json.encode())
    return hash_obj.hexdigest()[:length]


def tune(paths: BenchPaths, stage1_trials: int = 128) -> Dict[str, Any]:
    _ensure_oracle_guard(paths)
    # D8 启动守卫：软版本告警 + 约束语义金丝雀（毫秒级）
    _warn_if_unverified_optuna()
    _verify_constrained_tpe()
    oracle = _load_oracle(paths)
    hlr_model = _load_hlr(paths)

    # D7 备忘缓存：per-trial 约束、stage-3、闸门、baseline、精炼共用同一 DHP 入口，
    # 使"约束 vs 闸门"分歧在结构上不可能出现
    dhp_memo: Dict[str, Dict[str, Any]] = {}

    def _dhp_cached(config: Dict[str, Any]) -> Dict[str, Any]:
        key = json.dumps(config, sort_keys=True, separators=(",", ":"))
        if key not in dhp_memo:
            dhp_memo[key] = _dhp_reference(config, paths)["wordforge"]
        return dhp_memo[key]

    baseline_dhp = _dhp_cached(DEFAULT_MEMORY_MODEL_CONFIG)

    with AdapterServer() as server:
        # Precompute baseline once（max_rows 预算：全量 val 23M 行 oracle 推理不可行，
        # 4M 行确定性头部截断已远超统计需要；candidate 同水位对比保持公平）
        baseline_results = _single_pass_evaluate(
            paths, DEFAULT_MEMORY_MODEL_CONFIG, "val", 1.0,
            hlr_model=hlr_model, oracle=oracle, server=server,
            include_interval=True, include_baselines=False,
            max_rows=4_000_000,
        )
        baseline_prediction = baseline_results["wordforge"]
        baseline_interval_policy = baseline_results["interval_policy"]
        baseline_rows = baseline_results["rows"]

        def _gate_candidate(
            config: Dict[str, Any], metrics: Dict[str, Any], dhp: Dict[str, Any]
        ) -> Dict[str, Any]:
            # D11：闸门算术逐字节保留（uncapped predictionScore + 三条 DHP 腿）
            prediction_gain = (metrics["predictionScore"] - 1.0) * 100.0
            interval_gain = (
                metrics["intervalPolicy"]["targets"]["0.85"]["intervalScore"]
                - baseline_interval_policy["targets"]["0.85"]["intervalScore"]
            ) * 100.0
            # interval_gain is structurally ≈0 on MaiMemo (see _candidate_score comment),
            # so the passes gate uses prediction_gain + DHP guardrails only.
            # 闸门与 per-trial 约束共用 DHP_GUARDRAIL_FLOORS（预注册 per-leg 下限）
            passes = prediction_gain >= 0.5 and all(
                dhp[key] >= baseline_dhp[key] * floor
                for key, floor in DHP_GUARDRAIL_FLOORS.items()
            )
            return {
                "memoryModel": config,
                "metrics": metrics,
                "dhpReference": dhp,
                "predictionGainPercent": prediction_gain,
                "interval85GainPercent": interval_gain,
                "passes": passes,
            }

        study = optuna.create_study(
            direction="maximize",
            # 14 维空间 startup 16→24：multivariate TPE 建模前需更多样的随机底料
            # （不可行随机 trial 经 DHP 短路仅花 ~0.07s，代价可忽略）
            sampler=_make_constrained_sampler(DEFAULT_RANDOM_SEED, 24),
        )
        # D5 种子：FIFO 弹出，trial 0 必为 stock 锚点
        for params in _seed_trial_params():
            study.enqueue_trial(params)

        # Stage 1: DHP-first 短路 + 约束感知 TPE（2% 用户、capped 目标、不跑 interval）
        stage1_records: List[Dict[str, Any]] = []
        anchor_feasible = False
        for index in tqdm(range(stage1_trials), desc=f"tune stage1 ({stage1_trials} trials)"):
            trial = study.ask()
            config = _mutate_config(trial)
            dhp = _dhp_cached(config)
            feasible, comps = _dhp_constraints(dhp, baseline_dhp)
            # 必须在 tell 前无条件绑定：4.8.0 在 tell() 内读 user_attrs["constraint"]，
            # 缺失会 KeyError 且 trial 仍以 constraints=None 的 COMPLETE 态毒化后续采样
            trial.set_user_attr("constraint", comps)
            if not feasible:
                # 哨兵 tell：不可行 trial 仅按违反量之和排序，telled 值不进 TPE 分裂
                study.tell(trial, 0.0)
                record: Dict[str, Any] = {
                    "objective": 0.0,
                    "dhpFeasible": False,
                    "dhp": dhp,
                    "dhpConstraint": comps,
                    "shortCircuited": True,
                }
            else:
                metrics = _candidate_score(
                    paths, "val", 0.02, config,
                    baseline_prediction=baseline_prediction,
                    oracle=oracle, hlr_model=hlr_model, server=server,
                    include_interval=False, ratio_cap=1.5,
                )
                # mastered 感知目标（stage-1，预注册公式见 MASTERED_OBJECTIVE_WEIGHT 注释）
                mastered_term = _mastered_objective_term(dhp, baseline_dhp)
                metrics["masteredProxy"] = dhp["masteredProxy"]
                metrics["masteredObjectiveTerm"] = mastered_term
                metrics["objective"] = float(metrics["objective"] + mastered_term)
                study.tell(trial, metrics["objective"])
                record = {
                    **metrics,
                    "dhpFeasible": True,
                    "dhp": dhp,
                    "dhpConstraint": comps,
                    "shortCircuited": False,
                }
            record["params"] = dict(trial.params)
            record["config"] = config
            stage1_records.append(record)
            if index == 0:
                # D5 锚点自检：stock 配置 == baseline 配置 ⇒ 比率恰为 1.0，
                # 分量恰为 floor-1.0（per-leg 下限下的 DHP_ANCHOR_COMPONENTS）
                if not feasible or any(
                    abs(c - e) > 1e-9 for c, e in zip(comps, DHP_ANCHOR_COMPONENTS)
                ):
                    raise RuntimeError("constraint wiring broken")
                anchor_feasible = True

        feasible_sorted = sorted(
            (r for r in stage1_records if r["dhpFeasible"]),
            key=lambda r: r["objective"],
            reverse=True,
        )
        feasible_count = len(feasible_sorted)

        # Stage 2: 不相交用户桶 (1, 2) 上复评 top-16 可行 trial（stage1 固定在桶 0），
        # 晋升从此基于统计独立证据（修复 ceil(0.02*10)==ceil(0.10*10)==1 同桶缺陷）
        stage2 = []
        for record in tqdm(feasible_sorted[: min(16, feasible_count)], desc="tune stage2"):
            metrics = _candidate_score(
                paths, "val", 0.10, record["config"],
                baseline_prediction=baseline_prediction,
                oracle=oracle, hlr_model=hlr_model, server=server,
                max_rows=2_000_000, include_interval=False, ratio_cap=1.5,
                user_buckets=(1, 2),
            )
            # stage-2 与 stage-1 同公式（DHP 备忘缓存命中，零额外闭环开销）
            stage2_dhp = _dhp_cached(record["config"])
            mastered_term = _mastered_objective_term(stage2_dhp, baseline_dhp)
            metrics["masteredProxy"] = stage2_dhp["masteredProxy"]
            metrics["masteredObjectiveTerm"] = mastered_term
            metrics["objective"] = float(metrics["objective"] + mastered_term)
            stage2.append((record["config"], metrics))
        stage2 = sorted(stage2, key=lambda pair: pair[1]["objective"], reverse=True)[
            : min(4, len(stage2))
        ]

        # Stage 3: top 4 on 100% — uncapped + interval，目标语义与旧版一致
        # （纯 prediction composite，不混 mastered 项）；masteredProxy 仅记录
        finalists = []
        for config, _ in tqdm(stage2, desc="tune stage3"):
            metrics = _candidate_score(
                paths, "val", 1.0, config,
                baseline_prediction=baseline_prediction,
                oracle=oracle, hlr_model=hlr_model, server=server,
                max_rows=4_000_000,
            )
            stage3_dhp = _dhp_cached(config)
            metrics["masteredProxy"] = stage3_dhp["masteredProxy"]
            finalists.append((config, metrics, stage3_dhp))

        gated: List[Dict[str, Any]] = []
        for config, metrics, dhp in sorted(
            finalists, key=lambda entry: entry[1]["objective"], reverse=True
        ):
            # D6 回归检查：备忘缓存的 per-trial DHP 必须与闸门时刻新算结果逐位一致
            if _dhp_reference(config, paths)["wordforge"] != dhp:
                raise RuntimeError("DHP memo cache diverged from gate-time reference")
            gated.append(_gate_candidate(config, metrics, dhp))

        selected = None
        near_misses = []
        for candidate in gated:
            if candidate["passes"] and selected is None:
                selected = candidate
            else:
                near_misses.append(candidate)

        # D10 条件局部精炼（确定性救援）：无人过闸 且 最佳可行 finalist 增益 >= 0.25%
        refinement: Dict[str, Any] = {
            "triggered": False, "trials": 0, "bestGain": None, "passed": False,
        }
        if selected is None and gated and gated[0]["predictionGainPercent"] >= 0.25:
            refinement["triggered"] = True
            center = gated[0]["memoryModel"]
            # 每维 ±15% 原窗宽的盒子，居中于最佳可行 finalist，裁剪回原窗口；
            # 非 w 维读 config 顶层标量。中心值先夹回窗口（锚点 tau=0.0 在窗口外，
            # 若它成为中心，不夹会产生 low>high 的非法盒）
            boxes: Dict[str, Tuple[float, float]] = {}
            for name, (low, high) in SEARCH_WINDOWS.items():
                width = high - low
                if name in _W_INDEX:
                    value = float(center["w"][_W_INDEX[name]])
                else:
                    value = float(center.get(name, DEFAULT_MEMORY_MODEL_CONFIG[name]))
                value = min(max(value, low), high)
                boxes[name] = (max(low, value - 0.15 * width), min(high, value + 0.15 * width))
            refine_study = optuna.create_study(
                direction="maximize",
                sampler=_make_constrained_sampler(DEFAULT_RANDOM_SEED + 1, 8),
            )
            refined: List[Tuple[Dict[str, Any], Dict[str, Any]]] = []
            for _ in tqdm(range(24), desc="tune refine"):
                trial = refine_study.ask()
                config = _mutate_config(trial, windows=boxes)
                dhp = _dhp_cached(config)
                feasible, comps = _dhp_constraints(dhp, baseline_dhp)
                trial.set_user_attr("constraint", comps)
                if not feasible:
                    refine_study.tell(trial, 0.0)
                    continue
                metrics = _candidate_score(
                    paths, "val", 0.02, config,
                    baseline_prediction=baseline_prediction,
                    oracle=oracle, hlr_model=hlr_model, server=server,
                    include_interval=False, ratio_cap=1.5,
                )
                # 精炼是 stage-1 同级筛选，沿用 mastered 感知目标
                mastered_term = _mastered_objective_term(dhp, baseline_dhp)
                metrics["masteredProxy"] = dhp["masteredProxy"]
                metrics["masteredObjectiveTerm"] = mastered_term
                metrics["objective"] = float(metrics["objective"] + mastered_term)
                refine_study.tell(trial, metrics["objective"])
                refined.append((config, metrics))
            refinement["trials"] = 24
            promoted = []
            for config, _ in sorted(
                refined, key=lambda pair: pair[1]["objective"], reverse=True
            )[:2]:
                metrics = _candidate_score(
                    paths, "val", 1.0, config,
                    baseline_prediction=baseline_prediction,
                    oracle=oracle, hlr_model=hlr_model, server=server,
                    max_rows=4_000_000,
                )
                promoted_dhp = _dhp_cached(config)
                metrics["masteredProxy"] = promoted_dhp["masteredProxy"]
                promoted.append((config, metrics, promoted_dhp))
            for config, metrics, dhp in sorted(
                promoted, key=lambda entry: entry[1]["objective"], reverse=True
            ):
                candidate = _gate_candidate(config, metrics, dhp)
                gain = candidate["predictionGainPercent"]
                if refinement["bestGain"] is None or gain > refinement["bestGain"]:
                    refinement["bestGain"] = gain
                if candidate["passes"] and selected is None:
                    selected = candidate
                    refinement["passed"] = True

        # D9 nullReplicates：stock 配置在 10 个十分位桶上各评一次，量化桶间噪声
        null_objectives: List[float] = []
        for bucket in range(10):
            metrics = _candidate_score(
                paths, "val", 0.02, DEFAULT_MEMORY_MODEL_CONFIG,
                baseline_prediction=baseline_prediction,
                oracle=oracle, hlr_model=hlr_model, server=server,
                include_interval=False, ratio_cap=1.5, user_buckets=(bucket,),
            )
            null_objectives.append(float(metrics["objective"]))

        # D9 boundaryAutopsy：8 个最小违反（正分量和最小）的不可行 trial，stage1 之后补测
        infeasible_records = [r for r in stage1_records if not r["dhpFeasible"]]
        boundary_autopsy: List[Dict[str, Any]] = []
        for record in sorted(
            infeasible_records,
            key=lambda r: sum(c for c in r["dhpConstraint"] if c > 0),
        )[:8]:
            metrics = _candidate_score(
                paths, "val", 0.02, record["config"],
                baseline_prediction=baseline_prediction,
                oracle=oracle, hlr_model=hlr_model, server=server,
                include_interval=False, ratio_cap=1.5,
            )
            comps = record["dhpConstraint"]
            boundary_autopsy.append({
                "params": record["params"],
                "constraint": list(comps),
                # 与 stage-1 目标同口径（含 mastered 项），便于跨界对照
                "objective": float(
                    metrics["objective"]
                    + _mastered_objective_term(record["dhp"], baseline_dhp)
                ),
                "whichGuardrailBinds": DHP_CONSTRAINT_KEYS[
                    max(range(len(comps)), key=lambda i: comps[i])
                ],
            })

    # ---- D9 诊断与负面协议（对 tuning_summary 纯 ADDITIVE）----
    verdict = (
        "winner"
        if selected is not None
        else ("conclusive_negative" if feasible_count >= 30 else "inconclusive_explore")
    )
    best_candidate = selected if selected is not None else (gated[0] if gated else None)

    quarters: List[float] = []
    total = len(stage1_records)
    for quarter in range(4):
        chunk = stage1_records[quarter * total // 4 : (quarter + 1) * total // 4]
        quarters.append(
            float(sum(1 for r in chunk if r["dhpFeasible"]) / len(chunk)) if chunk else 0.0
        )

    trajectory: List[float | None] = []
    best_so_far = None
    for record in stage1_records:
        if record["dhpFeasible"]:
            best_so_far = (
                record["objective"] if best_so_far is None
                else max(best_so_far, record["objective"])
            )
        trajectory.append(best_so_far)

    top16 = feasible_sorted[: min(16, feasible_count)]
    param_dispersion: Dict[str, Dict[str, float]] = {}
    if top16:
        for name in SEARCH_WINDOWS:
            values = np.asarray([r["params"][name] for r in top16], dtype=float)
            param_dispersion[name] = {
                "min": float(values.min()),
                "p25": float(np.percentile(values, 25)),
                "p50": float(np.percentile(values, 50)),
                "p75": float(np.percentile(values, 75)),
                "max": float(values.max()),
            }

    retention_sensitivity = None
    avg_due_recall = None
    if best_candidate is not None:
        production_config = json.loads(json.dumps(best_candidate["memoryModel"]))
        production_config["baseDesiredRetention"] = 0.90  # 生产默认值；report-only，不动闸门
        production_dhp = _dhp_cached(production_config)
        retention_sensitivity = {
            "dhp": production_dhp,
            "wouldStillPassDhpLegs": all(
                production_dhp[key] >= baseline_dhp[key] * floor
                for key, floor in DHP_GUARDRAIL_FLOORS.items()
            ),
        }
        candidate_recall = float(best_candidate["dhpReference"]["avgDueRecall"])
        baseline_recall = float(baseline_dhp["avgDueRecall"])
        recall_ratio = candidate_recall / max(baseline_recall, 1e-12)
        avg_due_recall = {
            "baseline": baseline_recall,
            "selectedCandidate": candidate_recall,
            "ratio": recall_ratio,
            "warn": recall_ratio < 0.95,
        }

    if selected is None:
        selected = {
            "memoryModel": DEFAULT_MEMORY_MODEL_CONFIG,
            "metrics": {
                "objective": 1.0,
                "predictionScore": 1.0,
                "intervalPolicy": baseline_interval_policy,
                "prediction": baseline_prediction,
                "rows": baseline_rows,
            },
            "dhpReference": baseline_dhp,
            "predictionGainPercent": 0.0,
            "interval85GainPercent": 0.0,
            "passes": False,
            "keptBaseline": True,
        }

    # predictionDeltas（WARN 级报告，非闸门条款）：比率沿用 composite 约定
    # （smaller-better 用 base/cur，auc 用 cur/base），ratio < 1.0 即该指标恶化
    selected_prediction = selected["metrics"]["prediction"]

    def _delta(key: str, smaller_better: bool) -> Dict[str, float]:
        base = float(baseline_prediction[key])
        current = float(selected_prediction[key])
        ratio = (base / max(current, 1e-12)) if smaller_better else (current / max(base, 1e-12))
        return {"val_baseline": base, "val_candidate": current, "ratio": ratio}

    prediction_deltas: Dict[str, Any] = {
        "logLoss": _delta("logLoss", True),
        "ici": _delta("ici", True),
        "auc": _delta("auc", False),
        "maeP": _delta("maeP", True),
    }
    prediction_deltas["anyMetricWorse"] = any(
        prediction_deltas[key]["ratio"] < 1.0 for key in ("logLoss", "ici", "auc", "maeP")
    )

    search_diagnostics = {
        "verdict": verdict,
        "feasibleCount": feasible_count,
        "infeasibleCount": len(infeasible_records),
        "shortCircuitedCount": sum(1 for r in stage1_records if r["shortCircuited"]),
        "feasibleShareByQuarter": quarters,
        "bestFeasibleTrajectory": trajectory,
        "paramDispersion": param_dispersion,
        "nullReplicates": {
            "objectives": null_objectives,
            "mean": float(np.mean(null_objectives)),
            "std": float(np.std(null_objectives)),
        },
        "boundaryAutopsy": boundary_autopsy,
        "anchorFeasible": anchor_feasible,
        "retentionSensitivity": retention_sensitivity,
        "avgDueRecall": avg_due_recall,
        "predictionDeltas": prediction_deltas,
        "refinement": refinement,
    }

    result = {
        "selected": selected,
        "baseline": {
            "memoryModel": DEFAULT_MEMORY_MODEL_CONFIG,
            "prediction": baseline_prediction,
            "dhpReference": baseline_dhp,
        },
        "nearMisses": near_misses[:3],
        "searchDiagnostics": search_diagnostics,
    }
    (paths.reports / "tuning_summary.json").write_text(
        json.dumps(result, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    return result
