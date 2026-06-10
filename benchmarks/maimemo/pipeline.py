from __future__ import annotations

import json
import math
import os
import random
from typing import Any, Dict, List, Tuple

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
    query = f"""
        SELECT *
        FROM read_parquet('{(paths.parquet / "prefix_events.parquet").as_posix()}')
        WHERE split = ?
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
):
    sub_bucket_count = max(1, min(10, int(math.ceil(user_fraction * 10))))
    if sub_bucket_count >= 10:
        bucket_filter = ""
    else:
        bucket_filter = f"AND (user_bucket_100 / 10) % 10 < {sub_bucket_count}"
    limit_sql = f"LIMIT {int(max_rows)}" if max_rows else ""
    query = f"""
        SELECT *
        FROM read_parquet('{(paths.parquet / "prefix_events.parquet").as_posix()}')
        WHERE split = ?
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
        iter_prefix_batches(paths, split=split, user_fraction=user_fraction, max_rows=max_rows),
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

def _mutate_config(trial: optuna.Trial) -> Dict[str, Any]:
    """Search space = 12 high-leverage dims (Tier-A only) starting from FSRS-6 defaults.

    Iter 1 with 25-dim wide search produced 0 winners (top trials -2.8% to -4% predictionGain
    vs baseline). Root cause: TPE 64 trials cannot adequately cover 25-dim space — too many
    irrelevant dims dilute signal. Researcher's Tier-A recommendation: tune only the 9 most
    sensitive FSRS-5 weights, lock everything else to known-good values.

    Tuned dims (12):
      - w[0..3]   (initial stability per rating)
      - w[8..10]  (stability boost on recall)
      - w[15..16] (hard penalty / easy bonus)
      - w[20]     (forgetting-curve decay, FSRS-6 trainable)
      - baseDesiredRetention
      - maxIntervalDays

    Locked dims (defaults from DEFAULT_MEMORY_MODEL_CONFIG = FSRS-6 official):
      - w[4..7]   (difficulty params, FSRS-6 standard)
      - w[11..14] (lapse stability params)
      - w[17..19] (same-day review incl. w[19] saturation)
      - forgettingCurveFloor = 0.0
      - minIntervalSecs = 60
    """
    config = json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))

    # ---- Tier-A w[] (10 dims) — windows around FSRS-6 official defaults ----
    weights = list(config["w"])
    # w0-w3: initial stability — windows around FSRS-6 official [0.212, 1.2931, 2.3065, 8.2956]
    weights[0] = trial.suggest_float("w_0", 0.05, 0.60)
    weights[1] = trial.suggest_float("w_1", 0.40, 2.50)
    weights[2] = trial.suggest_float("w_2", 1.00, 5.00)
    weights[3] = trial.suggest_float("w_3", 4.00, 16.00)
    # w8: S boost (exp) — FSRS-6 default 1.8722
    weights[8] = trial.suggest_float("w_8", 1.00, 3.00)
    # w9: S saturation (S^-w9) — FSRS-6 default 0.1666
    weights[9] = trial.suggest_float("w_9", 0.05, 0.40)
    # w10: R-bonus on recall — FSRS-6 default 0.796
    weights[10] = trial.suggest_float("w_10", 0.30, 1.50)
    # w15: hard-penalty multiplier — FSRS-6 default 0.6014
    weights[15] = trial.suggest_float("w_15", 0.30, 1.00)
    # w16: easy-bonus multiplier — FSRS-6 default 1.8729
    weights[16] = trial.suggest_float("w_16", 1.20, 3.50)
    # w20: forgetting-curve decay (FSRS-6 trainable) — default 0.1542, 多数用户 <0.2
    weights[20] = trial.suggest_float("w_20", 0.10, 0.80)
    config["w"] = weights

    # ---- baseDesiredRetention (1 dim) ----
    # Researcher report §4.1: 0.85 (-25% workload) or 0.90 (Anki default, U-curve sweetspot)
    config["baseDesiredRetention"] = trial.suggest_float("base_desired_retention", 0.80, 0.92)

    # ---- maxIntervalDays (1 dim) ----
    # Current 90 is tight; let TPE explore [60, 180]
    config["maxIntervalDays"] = trial.suggest_float("max_interval_days", 60.0, 180.0)

    return config


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
) -> Dict[str, Any]:
    results = _single_pass_evaluate(
        paths, memory_config, split, user_fraction,
        hlr_model=hlr_model, oracle=oracle, server=server,
        include_interval=True, include_baselines=False,
        max_rows=max_rows,
    )
    prediction = results["wordforge"]
    prediction_score = prediction_composite(prediction, baseline_prediction)
    interval_policy = results["interval_policy"]
    # policyScore = safety * efficiency is structurally ≈0 on MaiMemo data:
    # the GRU oracle infers very short half-lives (optimalMeanDelta=1.0 day),
    # making safety=0 regardless of model quality. Drive optimization with
    # prediction_score; keep mean efficiency as a light guardrail.
    avg_efficiency = float(
        sum(interval_policy["targets"][f"{ret:.2f}"]["efficiency"] for ret in RETENTION_TARGETS)
        / max(len(RETENTION_TARGETS), 1)
    )
    objective = 0.85 * prediction_score + 0.15 * avg_efficiency
    return {
        "objective": objective,
        "prediction": prediction,
        "predictionScore": prediction_score,
        "intervalPolicy": interval_policy,
        "avgEfficiency": avg_efficiency,
        "rows": results["rows"],
    }


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
    oracle = _load_oracle(paths)
    hlr_model = _load_hlr(paths)

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

        study = optuna.create_study(
            direction="maximize",
            sampler=optuna.samplers.TPESampler(seed=DEFAULT_RANDOM_SEED),
        )

        # Stage 1: configurable trials on 2% of users
        scored_trials: List[Tuple[Dict[str, Any], Dict[str, Any]]] = []
        for _ in tqdm(range(stage1_trials), desc=f"tune stage1 ({stage1_trials} trials)"):
            trial = study.ask()
            config = _mutate_config(trial)
            metrics = _candidate_score(
                paths, "val", 0.02, config,
                baseline_prediction=baseline_prediction,
                oracle=oracle, hlr_model=hlr_model, server=server,
            )
            study.tell(trial, metrics["objective"])
            scored_trials.append((config, metrics))

        # Stage 2: top 16 on 10% of users
        stage1 = sorted(scored_trials, key=lambda pair: pair[1]["objective"], reverse=True)[:16]
        stage2 = []
        for config, _ in tqdm(stage1, desc="tune stage2"):
            metrics = _candidate_score(
                paths, "val", 0.10, config,
                baseline_prediction=baseline_prediction,
                oracle=oracle, hlr_model=hlr_model, server=server,
                max_rows=2_000_000,
            )
            stage2.append((config, metrics))
        stage2 = sorted(stage2, key=lambda pair: pair[1]["objective"], reverse=True)[:4]

        # Stage 3: top 4 on 100% of users (was 8)
        finalists = []
        for config, _ in tqdm(stage2, desc="tune stage3"):
            metrics = _candidate_score(
                paths, "val", 1.0, config,
                baseline_prediction=baseline_prediction,
                oracle=oracle, hlr_model=hlr_model, server=server,
                max_rows=4_000_000,
            )
            dhp = _dhp_reference(config, paths)["wordforge"]
            finalists.append((config, metrics, dhp))

    baseline_dhp = _dhp_reference(DEFAULT_MEMORY_MODEL_CONFIG, paths)["wordforge"]
    selected = None
    near_misses = []
    for config, metrics, dhp in sorted(finalists, key=lambda entry: entry[1]["objective"], reverse=True):
        prediction_gain = (metrics["predictionScore"] - 1.0) * 100.0
        interval_gain = (
            metrics["intervalPolicy"]["targets"]["0.85"]["intervalScore"]
            - baseline_interval_policy["targets"]["0.85"]["intervalScore"]
        ) * 100.0
        # interval_gain is structurally ≈0 on MaiMemo (see _candidate_score comment),
        # so the passes gate uses prediction_gain + DHP guardrails only.
        passes = (
            prediction_gain >= 0.5
            and dhp["expectedMemory"] >= baseline_dhp["expectedMemory"] * 0.9
            and dhp["nextDayMemory"] >= baseline_dhp["nextDayMemory"] * 0.9
            and dhp["targetCount"] >= baseline_dhp["targetCount"] * 0.9
        )
        candidate = {
            "memoryModel": config,
            "metrics": metrics,
            "dhpReference": dhp,
            "predictionGainPercent": prediction_gain,
            "interval85GainPercent": interval_gain,
            "passes": passes,
        }
        if passes and selected is None:
            selected = candidate
        else:
            near_misses.append(candidate)

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

    result = {
        "selected": selected,
        "baseline": {
            "memoryModel": DEFAULT_MEMORY_MODEL_CONFIG,
            "prediction": baseline_prediction,
            "dhpReference": baseline_dhp,
        },
        "nearMisses": near_misses[:3],
    }
    (paths.reports / "tuning_summary.json").write_text(
        json.dumps(result, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    return result
