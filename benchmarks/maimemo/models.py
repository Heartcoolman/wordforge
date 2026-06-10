from __future__ import annotations

import json
import math
import os
import pickle
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, Iterable, List, Sequence

import numpy as np
import pandas as pd
import torch
from sklearn.isotonic import IsotonicRegression
from sklearn.linear_model import LogisticRegression, SGDRegressor
from sklearn.metrics import roc_auc_score
from sklearn.preprocessing import StandardScaler

from .config import DEFAULT_MEMORY_MODEL_CONFIG, FSRS_BASELINE_CONFIG, RETENTION_TARGETS
from .dhp_reference import DHPStudent


EPS = 1e-9


def _coerce_finite_float(value: Any, default: float = 0.0) -> float:
    try:
        result = float(value)
    except (TypeError, ValueError):
        return default
    return result if math.isfinite(result) else default


def _auto_loader_workers() -> int:
    # This dataset is already materialized in memory per frame, so extra
    # DataLoader workers often add spawn/copy overhead on macOS without
    # improving throughput.
    return 0


def parse_history(value: str) -> List[int]:
    if value is None or value == "" or (isinstance(value, float) and math.isnan(value)):
        return []
    return [int(part) for part in str(value).split(",") if part != ""]


def infer_half_life(next_t: float, p_recall: float) -> float:
    if p_recall is None or (isinstance(p_recall, float) and math.isnan(p_recall)):
        return max(1e-6, float(next_t))
    p = min(max(float(p_recall), 1e-6), 1 - 1e-6)
    return max(1e-6, -float(next_t) / math.log2(p))


def build_history_features(frame: pd.DataFrame) -> np.ndarray:
    rows = []
    for row in frame.itertuples(index=False):
        t_hist = parse_history(row.t_history)
        r_hist = parse_history(row.r_history)
        last_gap = float(t_hist[-1]) if t_hist else 0.0
        mean_gap = float(np.mean(t_hist)) if t_hist else 0.0
        std_gap = float(np.std(t_hist)) if len(t_hist) > 1 else 0.0
        success_count = sum(1 for value in r_hist if value == 1)
        failure_count = len(r_hist) - success_count
        last_result = float(r_hist[-1]) if r_hist else 0.0
        difficulty = 0.0 if pd.isna(row.difficulty) else float(row.difficulty)
        rows.append(
            [
                difficulty,
                float(len(t_hist)),
                float(success_count),
                float(failure_count),
                last_gap,
                mean_gap,
                std_gap,
                float(sum(t_hist)),
                last_result,
            ]
        )
    return np.asarray(rows, dtype=np.float32)


@dataclass
class PredictionOutput:
    probabilities: np.ndarray
    half_lives: np.ndarray
    interval_days: Dict[float, np.ndarray]


class ProbabilityCalibration:
    name = "unknown"

    def fit(self, probabilities: np.ndarray, labels: np.ndarray) -> "ProbabilityCalibration":
        return self

    def predict(self, probabilities: np.ndarray) -> np.ndarray:
        raise NotImplementedError


class IdentityCalibration(ProbabilityCalibration):
    name = "identity"

    def predict(self, probabilities: np.ndarray) -> np.ndarray:
        return np.asarray(probabilities, dtype=np.float64)


def _probabilities_to_logits(probabilities: np.ndarray) -> np.ndarray:
    probs = np.clip(np.asarray(probabilities, dtype=np.float64), 1e-6, 1 - 1e-6)
    return np.log(probs / (1.0 - probs)).reshape(-1, 1)


class LogisticCalibration(ProbabilityCalibration):
    name = "logistic"

    def __init__(self) -> None:
        self.model = LogisticRegression(max_iter=1000)

    def fit(self, probabilities: np.ndarray, labels: np.ndarray) -> "LogisticCalibration":
        self.model.fit(_probabilities_to_logits(probabilities), labels)
        return self

    def predict(self, probabilities: np.ndarray) -> np.ndarray:
        return self.model.predict_proba(_probabilities_to_logits(probabilities))[:, 1]


class IsotonicCalibration(ProbabilityCalibration):
    name = "isotonic"

    def __init__(self) -> None:
        self.model = IsotonicRegression(out_of_bounds="clip")

    def fit(self, probabilities: np.ndarray, labels: np.ndarray) -> "IsotonicCalibration":
        self.model.fit(np.asarray(probabilities, dtype=np.float64), labels)
        return self

    def predict(self, probabilities: np.ndarray) -> np.ndarray:
        return self.model.predict(np.asarray(probabilities, dtype=np.float64))


@dataclass
class SequenceFrameBatch:
    sequences: torch.Tensor
    lengths: torch.Tensor
    difficulty: torch.Tensor
    next_t: torch.Tensor
    targets: torch.Tensor

    def iter_batches(self, batch_size: int, shuffle: bool = False):
        size = int(self.lengths.shape[0])
        if size == 0:
            return
        if shuffle:
            indices = torch.randperm(size)
            for start in range(0, size, batch_size):
                batch_indices = indices[start:start + batch_size]
                yield {
                    "sequences": self.sequences.index_select(0, batch_indices),
                    "lengths": self.lengths.index_select(0, batch_indices),
                    "difficulty": self.difficulty.index_select(0, batch_indices),
                    "next_t": self.next_t.index_select(0, batch_indices),
                    "targets": self.targets.index_select(0, batch_indices),
                }
            return
        for start in range(0, size, batch_size):
            end = min(size, start + batch_size)
            yield {
                "sequences": self.sequences[start:end],
                "lengths": self.lengths[start:end],
                "difficulty": self.difficulty[start:end],
                "next_t": self.next_t[start:end],
                "targets": self.targets[start:end],
            }


def _frame_to_sequence_batch(frame: pd.DataFrame) -> SequenceFrameBatch:
    row_count = len(frame)
    if row_count == 0:
        return SequenceFrameBatch(
            sequences=torch.zeros((0, 1, 3), dtype=torch.float32),
            lengths=torch.zeros(0, dtype=torch.long),
            difficulty=torch.zeros(0, dtype=torch.float32),
            next_t=torch.zeros(0, dtype=torch.float32),
            targets=torch.zeros(0, dtype=torch.float32),
        )

    parsed_steps: List[List[tuple[int, int]]] = []
    normalized_difficulties: List[float] = []
    max_len = 1

    for row in frame.itertuples(index=False):
        t_hist = parse_history(row.t_history)
        r_hist = parse_history(row.r_history)
        steps = list(zip(t_hist or [0], r_hist or [0]))
        if not steps:
            steps = [(0, 0)]
        parsed_steps.append(steps)
        normalized_difficulties.append(_coerce_finite_float(row.difficulty, default=0.0) / 10.0)
        max_len = max(max_len, len(steps))

    sequences = np.zeros((row_count, max_len, 3), dtype=np.float32)
    lengths = np.empty(row_count, dtype=np.int64)
    difficulties = np.asarray(normalized_difficulties, dtype=np.float32)

    for row_index, steps in enumerate(parsed_steps):
        lengths[row_index] = len(steps)
        sequences[row_index, : len(steps), 0] = [math.log1p(gap) for gap, _ in steps]
        sequences[row_index, : len(steps), 1] = [float(result) for _, result in steps]
        sequences[row_index, : len(steps), 2] = difficulties[row_index]

    return SequenceFrameBatch(
        sequences=torch.from_numpy(sequences),
        lengths=torch.from_numpy(lengths),
        difficulty=torch.from_numpy(difficulties),
        next_t=torch.from_numpy(frame["next_t"].to_numpy(dtype=np.float32, copy=True)),
        targets=torch.from_numpy(frame["next_r"].to_numpy(dtype=np.float32, copy=True)),
    )


class DHPBaseline:
    def predict(self, frame: pd.DataFrame) -> PredictionOutput:
        probabilities = frame["dhp_p_recall"].to_numpy(dtype=float)
        half_lives = np.asarray(
            [infer_half_life(t, p) for t, p in zip(frame["next_t"], frame["dhp_p_recall"])],
            dtype=np.float64,
        )
        intervals = {
            retention: np.asarray(
                [max(1.0, math.ceil(h * -math.log2(retention))) for h in half_lives],
                dtype=np.float64,
            )
            for retention in RETENTION_TARGETS
        }
        return PredictionOutput(probabilities, half_lives, intervals)


class DHPActiveModel:
    """Actively replays review history using DHPStudent state transitions."""

    def __init__(self, dhp_student: DHPStudent):
        self.student = dhp_student

    def predict(self, frame: pd.DataFrame) -> PredictionOutput:
        prob_list: List[float] = []
        hl_list: List[float] = []

        for row in frame.itertuples(index=False):
            t_hist = parse_history(row.t_history)
            r_hist = parse_history(row.r_history)
            next_t = float(row.next_t)
            diff = max(1, min(18, int(round(_coerce_finite_float(row.difficulty, 5.0)))))

            if not t_hist:
                hl = self.student.start_halflife(diff, 0)
            else:
                first_result = r_hist[0] if r_hist else 0
                hl = self.student.start_halflife(diff, first_result)
                state = [hl, float(diff)]
                for i in range(1, len(t_hist)):
                    recalled = r_hist[i] if i < len(r_hist) else 0
                    state, hl = self.student.next_state(state, recalled, t_hist[i], 0.0)

            hl = max(1e-6, hl)
            p = max(0.0, min(1.0, math.pow(2.0, -next_t / hl)))
            prob_list.append(p)
            hl_list.append(hl)

        probabilities = np.asarray(prob_list, dtype=np.float64)
        half_lives = np.asarray(hl_list, dtype=np.float64)
        intervals = {
            ret: np.asarray(
                [max(1.0, math.ceil(h * -math.log2(ret))) for h in hl_list],
                dtype=np.float64,
            )
            for ret in RETENTION_TARGETS
        }
        return PredictionOutput(probabilities, half_lives, intervals)


class HLRBaseline:
    def __init__(self) -> None:
        self.scaler = StandardScaler()
        self.model = SGDRegressor(
            loss="squared_error",
            penalty="l2",
            alpha=1e-4,
            random_state=42,
        )
        self._fitted = False

    def fit(self, frame: pd.DataFrame) -> None:
        self.partial_fit(frame)

    def partial_fit(self, frame: pd.DataFrame) -> None:
        if frame.empty:
            return
        features = build_history_features(frame)
        targets = np.log1p(
            np.asarray(
                [infer_half_life(t, p) for t, p in zip(frame["next_t"], frame["dhp_p_recall"])],
                dtype=np.float64,
            )
        )
        self.scaler.partial_fit(features)
        transformed = self.scaler.transform(features)
        self.model.partial_fit(transformed, targets)
        self._fitted = True

    def predict(self, frame: pd.DataFrame) -> PredictionOutput:
        if frame.empty:
            return PredictionOutput(np.empty(0), np.empty(0), {ret: np.empty(0) for ret in RETENTION_TARGETS})
        if not self._fitted:
            raise RuntimeError("HLRBaseline must be fitted before predict")
        features = build_history_features(frame)
        transformed = self.scaler.transform(features)
        half_lives = np.expm1(self.model.predict(transformed)).clip(min=1e-6)
        next_t = frame["next_t"].to_numpy(dtype=float)
        probabilities = np.power(2.0, -next_t / np.maximum(half_lives, 1e-6))
        intervals = {
            retention: np.asarray(
                [max(1.0, math.ceil(h * -math.log2(retention))) for h in half_lives],
                dtype=np.float64,
            )
            for retention in RETENTION_TARGETS
        }
        return PredictionOutput(probabilities, half_lives, intervals)

    def save(self, path: Path) -> None:
        with path.open("wb") as handle:
            pickle.dump(
                {"model": self.model, "scaler": self.scaler, "fitted": self._fitted},
                handle,
            )

    @classmethod
    def load(cls, path: Path) -> "HLRBaseline":
        instance = cls()
        with path.open("rb") as handle:
            payload = pickle.load(handle)
        instance.model = payload["model"]
        instance.scaler = payload["scaler"]
        instance._fitted = payload.get("fitted", True)
        return instance


DAY_MS = 86_400_000
SUCCESS_QUALITY = 0.95
FAILURE_QUALITY = 0.0
MAX_INTERVAL_DAYS = 90.0
MIN_INTERVAL_SECS = 60


def _fsrs_recall_probability(
    stability: float,
    elapsed_days: float,
    factor: float,
    decay: float,
    floor: float,
) -> float:
    """R(t,S) = floor + (1-floor) * (1 + factor * t/S)^decay"""
    s = max(stability, 0.01)
    power_law = (1.0 + factor * elapsed_days / s) ** decay
    return max(0.0, min(1.0, floor + (1.0 - floor) * power_law))


def _fsrs_compute_interval_days(
    stability: float,
    target_recall: float,
    factor: float,
    decay: float,
    floor: float,
) -> float:
    """Solve R = target_recall for t (in days)."""
    s = max(stability, 0.01)
    adjusted = max(1e-6, min(1.0, (target_recall - floor) / max(1.0 - floor, 1e-9)))
    interval_days = s / factor * (adjusted ** (1.0 / decay) - 1.0)
    return max(MIN_INTERVAL_SECS / 86400.0, min(MAX_INTERVAL_DAYS, interval_days))


def _replay_history_python(
    t_history: List[int],
    r_history: List[int],
    w: List[float],
    factor: float,
    decay: float,
    floor: float,
) -> tuple[float, float, int]:
    """Replay a review history using FSRS-5 formulas. Returns (stability, difficulty, review_count)."""
    stability = 0.4
    difficulty = 5.0
    last_review_ms: int | None = None
    review_count = 0

    for delta_t, result in zip(t_history, r_history):
        quality = SUCCESS_QUALITY if result != 0 else FAILURE_QUALITY
        grade = (
            1 if quality <= 0.15
            else 2 if quality <= 0.5
            else 3 if quality <= 0.85
            else 4
        )

        now_ms = (last_review_ms or 0) + delta_t * DAY_MS

        if review_count == 0:
            # First review: initial stability from w0-w3
            g = max(0, min(3, grade - 1))
            stability = w[g]
            # Initial difficulty: D0(G) = w4 - e^{w5*(G-1)} + 1
            difficulty = max(1.0, min(10.0, w[4] - math.exp(w[5] * (grade - 1.0)) + 1.0))
        else:
            # Compute current R
            if last_review_ms is not None:
                elapsed_days = max(0.0, (now_ms - last_review_ms) / 86_400_000.0)
            else:
                elapsed_days = 0.0
            r = _fsrs_recall_probability(stability, elapsed_days, factor, decay, floor)

            # Update difficulty
            delta_d = -w[6] * (grade - 3.0)
            d_prime = difficulty + delta_d * (10.0 - difficulty) / 9.0
            d0_4 = max(1.0, min(10.0, w[4] - math.exp(w[5] * 3.0) + 1.0))
            difficulty = max(1.0, min(10.0, w[7] * d0_4 + (1.0 - w[7]) * d_prime))

            if elapsed_days < 1.0:
                # Same-day review: FSRS-5 short-term stability formula
                grade_f = float(grade)
                exponent = max(-20.0, min(20.0, w[17] * (grade_f - 3.0 + w[18])))
                stability = max(0.01, stability * math.exp(exponent))
            elif grade >= 2:
                # Successful recall
                bonus = w[15] if grade == 2 else (w[16] if grade == 4 else 1.0)
                s_inc = max(
                    0.0,
                    math.exp(w[8])
                    * (11.0 - difficulty)
                    * max(stability, 0.01) ** (-w[9])
                    * (math.exp(w[10] * (1.0 - r)) - 1.0)
                    * bonus,
                )
                stability = max(0.01, stability * (s_inc + 1.0))
            else:
                # Forgetting
                stability = max(
                    0.01,
                    min(
                        stability,
                        w[11]
                        * difficulty ** (-w[12])
                        * ((stability + 1.0) ** w[13] - 1.0)
                        * math.exp(w[14] * (1.0 - r)),
                    ),
                )

        review_count += 1
        last_review_ms = now_ms

    return stability, difficulty, review_count


class WordForgeAdapterModel:
    """Uses Rust JSONL server for FSRS computation. Falls back to pure Python."""

    def __init__(self, memory_config: Dict[str, Any], server=None) -> None:
        self.config = json.loads(json.dumps(memory_config))
        self.w: List[float] = list(self.config["w"])
        self.factor: float = float(self.config.get("forgettingCurveFactor", 0.30))
        self.decay: float = float(self.config.get("forgettingCurveDecay", -0.5))
        self.floor: float = float(self.config.get("forgettingCurveFloor", 0.0))
        self.server = server

    def predict(self, frame: pd.DataFrame) -> PredictionOutput:
        if self.server is not None:
            return self._predict_rust(frame)
        return self._predict_python(frame)

    def _predict_rust(self, frame: pd.DataFrame) -> PredictionOutput:
        """Use Rust JSONL server — send raw CSV strings, Rust does parsing."""
        t_csv = frame["t_history"].tolist()
        r_csv = frame["r_history"].tolist()
        next_t = frame["next_t"].tolist()
        retention_list = list(RETENTION_TARGETS)

        items = [
            {
                "tHistoryCsv": t,
                "rHistoryCsv": r,
                "nextTDays": float(nt),
                "targetRetentions": retention_list,
                "intervalScale": 1.0,
            }
            for t, r, nt in zip(t_csv, r_csv, next_t)
        ]

        # Process in chunks of 8192 via persistent server
        scored = []
        chunk_size = 8192
        for start in range(0, len(items), chunk_size):
            scored.extend(self.server.score_batch(self.config, items[start:start + chunk_size]))
        probabilities = np.asarray(
            [item.get("predictedRecall") or 0.0 for item in scored],
            dtype=np.float64,
        )
        half_lives = np.asarray([item["stability"] for item in scored], dtype=np.float64)
        intervals = {
            retention: np.asarray(
                [
                    next(
                        entry["intervalDays"]
                        for entry in item["intervals"]
                        if abs(entry["retention"] - retention) < 1e-9
                    )
                    for item in scored
                ],
                dtype=np.float64,
            )
            for retention in RETENTION_TARGETS
        }
        return PredictionOutput(probabilities, half_lives, intervals)

    def _predict_python(self, frame: pd.DataFrame) -> PredictionOutput:
        """Pure Python fallback — no Rust binary needed."""
        stabilities = []
        predicted_recalls = []
        interval_arrays: Dict[float, List[float]] = {ret: [] for ret in RETENTION_TARGETS}

        for row in frame.itertuples(index=False):
            t_hist = parse_history(row.t_history)
            r_hist = parse_history(row.r_history)
            next_t = float(row.next_t)

            stability, difficulty, review_count = _replay_history_python(
                t_hist, r_hist, self.w, self.factor, self.decay, self.floor,
            )

            p_recall = _fsrs_recall_probability(stability, next_t, self.factor, self.decay, self.floor) if review_count > 0 else 0.0
            stabilities.append(stability)
            predicted_recalls.append(p_recall)

            for ret in RETENTION_TARGETS:
                ivl = _fsrs_compute_interval_days(stability, ret, self.factor, self.decay, self.floor)
                interval_arrays[ret].append(ivl)

        probabilities = np.asarray(predicted_recalls, dtype=np.float64)
        half_lives = np.asarray(stabilities, dtype=np.float64)
        intervals = {
            ret: np.asarray(interval_arrays[ret], dtype=np.float64)
            for ret in RETENTION_TARGETS
        }
        return PredictionOutput(probabilities, half_lives, intervals)


class SequenceDataset(torch.utils.data.Dataset):
    def __init__(self, frame: pd.DataFrame) -> None:
        self.samples = []
        self.max_len = 1
        for row in frame.itertuples(index=False):
            t_hist = parse_history(row.t_history)
            r_hist = parse_history(row.r_history)
            self.max_len = max(self.max_len, len(t_hist))
            self.samples.append(
                {
                    "t_history": t_hist,
                    "r_history": r_hist,
                    "difficulty": _coerce_finite_float(row.difficulty, default=0.0),
                    "next_t": float(row.next_t),
                    "target": float(row.next_r),
                }
            )

    def __len__(self) -> int:
        return len(self.samples)

    def __getitem__(self, index: int) -> Dict[str, Any]:
        return self.samples[index]


class GRUHalfLife(torch.nn.Module):
    def __init__(self, hidden_size: int = 64) -> None:
        super().__init__()
        self.gru = torch.nn.GRU(input_size=3, hidden_size=hidden_size, batch_first=True)
        self.head = torch.nn.Linear(hidden_size + 1, 1)

    def forward(
        self,
        sequences: torch.Tensor,
        lengths: torch.Tensor,
        difficulty: torch.Tensor,
    ) -> torch.Tensor:
        packed = torch.nn.utils.rnn.pack_padded_sequence(
            sequences,
            lengths.cpu(),
            batch_first=True,
            enforce_sorted=False,
        )
        _, hidden = self.gru(packed)
        hidden = hidden[-1]
        features = torch.cat([hidden, difficulty.unsqueeze(1)], dim=1)
        return torch.nn.functional.softplus(self.head(features)).squeeze(1) + 1e-6


def _sequence_collate(batch: Sequence[Dict[str, Any]]) -> Dict[str, torch.Tensor]:
    lengths = torch.tensor([max(1, len(sample["t_history"])) for sample in batch], dtype=torch.long)
    max_len = int(lengths.max().item())
    sequences = torch.zeros((len(batch), max_len, 3), dtype=torch.float32)
    difficulties = torch.tensor(
        [sample["difficulty"] / 10.0 for sample in batch],
        dtype=torch.float32,
    )
    next_t = torch.tensor([sample["next_t"] for sample in batch], dtype=torch.float32)
    targets = torch.tensor([sample["target"] for sample in batch], dtype=torch.float32)

    for row_index, sample in enumerate(batch):
        t_hist = sample["t_history"] or [0]
        r_hist = sample["r_history"] or [0]
        for step_index, (gap, result) in enumerate(zip(t_hist, r_hist)):
            sequences[row_index, step_index, 0] = math.log1p(gap)
            sequences[row_index, step_index, 1] = float(result)
            sequences[row_index, step_index, 2] = sample["difficulty"] / 10.0

    return {
        "sequences": sequences,
        "lengths": lengths,
        "difficulty": difficulties,
        "next_t": next_t,
        "targets": targets,
    }


class GRUHLROracle:
    def __init__(
        self,
        device: str | None = None,
        loader_workers: int | None = None,
        hidden_size: int = 64,
    ) -> None:
        self.device = device or ("mps" if torch.backends.mps.is_available() else "cpu")
        self.hidden_size = max(2, int(hidden_size))
        self.model = GRUHalfLife(hidden_size=self.hidden_size).to(self.device)
        self.calibration = IsotonicRegression(out_of_bounds="clip")
        self.loader_workers = _auto_loader_workers() if loader_workers is None else max(0, int(loader_workers))

    def _loader_kwargs(self) -> Dict[str, Any]:
        if self.loader_workers <= 0:
            return {"num_workers": 0}
        return {
            "num_workers": self.loader_workers,
            "persistent_workers": True,
            "prefetch_factor": 2,
        }

    def fit(
        self,
        train_frame: pd.DataFrame,
        val_frame: pd.DataFrame,
        epochs: int = 5,
        batch_size: int = 512,
    ) -> None:
        self.fit_streaming([train_frame], [val_frame], epochs=epochs, batch_size=batch_size)

    def fit_streaming(
        self,
        train_frames: Iterable[pd.DataFrame],
        calibration_frames: Iterable[pd.DataFrame],
        epochs: int = 5,
        batch_size: int = 512,
        max_calibration_points: int = 500_000,
    ) -> None:
        optimizer = torch.optim.Adam(self.model.parameters(), lr=1e-3, weight_decay=1e-5)
        for frame in train_frames:
            if frame.empty:
                continue
            frame_batch = _frame_to_sequence_batch(frame)
            self._train_on_sequence_batch(
                frame_batch,
                optimizer=optimizer,
                batch_size=batch_size,
                epochs=epochs,
            )
        self._assert_finite_parameters()
        calibration_probs, calibration_labels = self._collect_calibration_points(
            calibration_frames,
            batch_size=batch_size,
            max_points=max_calibration_points,
        )
        if calibration_probs.size == 0:
            raise RuntimeError("no calibration points collected for GRUHLROracle")
        self.fit_calibration(calibration_probs, calibration_labels)
        self.validate()

    def fit_calibration(
        self,
        calibration_probs: np.ndarray,
        calibration_labels: np.ndarray,
        seed: int = 42,
    ) -> Dict[str, Any]:
        probs = np.asarray(calibration_probs, dtype=np.float64)
        labels = np.asarray(calibration_labels, dtype=np.float64)
        if probs.size == 0:
            raise RuntimeError("no calibration points collected for GRUHLROracle")

        if probs.size < 1000 or len(np.unique(labels)) < 2:
            self.calibration = IsotonicCalibration().fit(probs, labels)
            return {
                "selected": self.calibration.name,
                "candidates": {
                    self.calibration.name: probability_metric_summary(labels, self.calibration.predict(probs)),
                },
                "selectionRows": int(probs.size),
                "selectionSplit": "full",
            }

        rng = np.random.default_rng(seed)
        indices = rng.permutation(probs.size)
        split_index = max(1, int(probs.size * 0.7))
        fit_indices = indices[:split_index]
        eval_indices = indices[split_index:]
        if len(eval_indices) == 0:
            eval_indices = fit_indices

        fit_probs = probs[fit_indices]
        fit_labels = labels[fit_indices]
        eval_probs = probs[eval_indices]
        eval_labels = labels[eval_indices]

        candidate_builders = [
            IdentityCalibration,
            IsotonicCalibration,
            LogisticCalibration,
        ]
        scored_candidates: List[tuple[tuple[float, float, float], ProbabilityCalibration, Dict[str, float]]] = []
        for builder in candidate_builders:
            candidate = builder().fit(fit_probs, fit_labels)
            metrics = probability_metric_summary(eval_labels, candidate.predict(eval_probs))
            score = (
                metrics["logLoss"],
                metrics["maeP"],
                -metrics["auc"],
            )
            scored_candidates.append((score, candidate, metrics))

        _, best_candidate, _ = min(scored_candidates, key=lambda item: item[0])

        selected_name = best_candidate.name
        if selected_name == IdentityCalibration.name:
            self.calibration = IdentityCalibration()
        elif selected_name == LogisticCalibration.name:
            self.calibration = LogisticCalibration().fit(probs, labels)
        else:
            self.calibration = IsotonicCalibration().fit(probs, labels)

        return {
            "selected": self.calibration.name,
            "candidates": {
                candidate.name: metrics
                for _, candidate, metrics in scored_candidates
            },
            "selectionRows": int(probs.size),
            "selectionSplit": f"{len(fit_indices)}/{len(eval_indices)}",
        }

    def _assert_finite_tensor(self, name: str, tensor: torch.Tensor) -> None:
        invalid = int((~torch.isfinite(tensor)).sum().item())
        if invalid > 0:
            raise RuntimeError(f"GRUHLROracle produced non-finite {name}: {invalid} invalid values")

    def _assert_finite_parameters(self) -> None:
        invalid_params = []
        for name, parameter in self.model.named_parameters():
            invalid = int((~torch.isfinite(parameter)).sum().item())
            if invalid > 0:
                invalid_params.append(f"{name}={invalid}")
        if invalid_params:
            joined = ", ".join(invalid_params)
            raise RuntimeError(f"GRUHLROracle contains non-finite parameters: {joined}")

    def validate(self) -> None:
        self._assert_finite_parameters()
        probe = np.asarray([0.0, 0.5, 1.0], dtype=np.float64)
        try:
            calibrated = np.asarray(self.calibration.predict(probe), dtype=np.float64)
        except Exception as exc:
            raise RuntimeError("GRUHLROracle calibration is not usable") from exc
        if calibrated.shape != probe.shape:
            raise RuntimeError(
                "GRUHLROracle calibration returned an unexpected shape: "
                f"{calibrated.shape} != {probe.shape}"
            )
        if not np.all(np.isfinite(calibrated)):
            raise RuntimeError("GRUHLROracle calibration produced non-finite predictions")

    def _train_on_frame(
        self,
        frame: pd.DataFrame,
        optimizer: torch.optim.Optimizer,
        batch_size: int = 512,
        epochs: int = 1,
        scheduler=None,
    ) -> None:
        if frame.empty:
            return
        frame_batch = _frame_to_sequence_batch(frame)
        self._train_on_sequence_batch(frame_batch, optimizer=optimizer, batch_size=batch_size, epochs=epochs, scheduler=scheduler)

    def _train_on_sequence_batch(
        self,
        frame_batch: SequenceFrameBatch,
        optimizer: torch.optim.Optimizer,
        batch_size: int = 512,
        epochs: int = 1,
        scheduler=None,
    ) -> None:
        if frame_batch.lengths.numel() == 0:
            return
        self.model.train()
        for _ in range(max(1, epochs)):
            for batch in frame_batch.iter_batches(batch_size=batch_size, shuffle=True):
                optimizer.zero_grad()
                self._assert_finite_tensor("batch.sequences", batch["sequences"])
                self._assert_finite_tensor("batch.difficulty", batch["difficulty"])
                self._assert_finite_tensor("batch.next_t", batch["next_t"])
                self._assert_finite_tensor("batch.targets", batch["targets"])
                half_lives = self.model(
                    batch["sequences"].to(self.device),
                    batch["lengths"].to(self.device),
                    batch["difficulty"].to(self.device),
                )
                self._assert_finite_tensor("half_lives", half_lives)
                probabilities = torch.pow(
                    torch.tensor(2.0, device=self.device),
                    -batch["next_t"].to(self.device) / torch.clamp(half_lives, min=1e-6),
                )
                self._assert_finite_tensor("probabilities", probabilities)
                loss = torch.nn.functional.binary_cross_entropy(
                    torch.clamp(probabilities, min=1e-6, max=1 - 1e-6),
                    batch["targets"].to(self.device),
                )
                if not torch.isfinite(loss):
                    raise RuntimeError("GRUHLROracle training loss became non-finite")
                loss.backward()
                for name, parameter in self.model.named_parameters():
                    if parameter.grad is None:
                        continue
                    invalid = int((~torch.isfinite(parameter.grad)).sum().item())
                    if invalid > 0:
                        raise RuntimeError(
                            f"GRUHLROracle produced non-finite gradients in {name}: {invalid} invalid values"
                        )
                optimizer.step()
                if scheduler is not None:
                    scheduler.step()
                self._assert_finite_parameters()

    def _collect_calibration_points(
        self,
        frames: Iterable[pd.DataFrame],
        batch_size: int = 1024,
        max_points: int = 500_000,
    ) -> tuple[np.ndarray, np.ndarray]:
        probabilities: List[np.ndarray] = []
        labels: List[np.ndarray] = []
        collected = 0
        for frame in frames:
            if frame.empty or collected >= max_points:
                continue
            raw = self.predict_raw(frame, batch_size=batch_size)
            y = frame["next_r"].to_numpy(dtype=float)
            remaining = max_points - collected
            if len(y) > remaining:
                probabilities.append(raw.probabilities[:remaining])
                labels.append(y[:remaining])
                collected += remaining
            else:
                probabilities.append(raw.probabilities)
                labels.append(y)
                collected += len(y)
        if not probabilities:
            return np.empty(0), np.empty(0)
        return np.concatenate(probabilities), np.concatenate(labels)

    def predict_raw(self, frame: pd.DataFrame, batch_size: int = 1024) -> PredictionOutput:
        if frame.empty:
            return PredictionOutput(np.empty(0), np.empty(0), {ret: np.empty(0) for ret in RETENTION_TARGETS})
        self._assert_finite_parameters()
        frame_batch = _frame_to_sequence_batch(frame)
        all_probabilities: List[np.ndarray] = []
        all_half_lives: List[np.ndarray] = []
        with torch.no_grad():
            for batch in frame_batch.iter_batches(batch_size=batch_size, shuffle=False):
                half_lives = self.model(
                    batch["sequences"].to(self.device),
                    batch["lengths"].to(self.device),
                    batch["difficulty"].to(self.device),
                )
                probabilities = torch.pow(
                    torch.tensor(2.0, device=self.device),
                    -batch["next_t"].to(self.device) / torch.clamp(half_lives, min=1e-6),
                )
                all_probabilities.append(probabilities.cpu().numpy())
                all_half_lives.append(half_lives.cpu().numpy())
        probabilities = np.concatenate(all_probabilities) if all_probabilities else np.empty(0)
        half_lives = np.concatenate(all_half_lives) if all_half_lives else np.empty(0)
        if not np.all(np.isfinite(probabilities)):
            raise RuntimeError("GRUHLROracle produced non-finite raw probabilities")
        if np.any((probabilities < 0.0) | (probabilities > 1.0)):
            raise RuntimeError("GRUHLROracle produced raw probabilities outside [0, 1]")
        if not np.all(np.isfinite(half_lives)):
            raise RuntimeError("GRUHLROracle produced non-finite half-lives")
        if np.any(half_lives <= 0.0):
            raise RuntimeError("GRUHLROracle produced non-positive half-lives")
        probabilities = probabilities.clip(1e-6, 1 - 1e-6)
        intervals = {
            retention: np.asarray(
                [max(1.0, math.ceil(h * -math.log2(retention))) for h in half_lives],
                dtype=np.float64,
            )
            for retention in RETENTION_TARGETS
        }
        return PredictionOutput(probabilities, half_lives, intervals)

    def predict(self, frame: pd.DataFrame) -> PredictionOutput:
        self.validate()
        raw = self.predict_raw(frame)
        calibrated = np.asarray(self.calibration.predict(raw.probabilities), dtype=np.float64)
        if calibrated.shape != raw.probabilities.shape:
            raise RuntimeError(
                "GRUHLROracle calibration returned an unexpected shape: "
                f"{calibrated.shape} != {raw.probabilities.shape}"
            )
        if not np.all(np.isfinite(calibrated)):
            raise RuntimeError("GRUHLROracle calibration produced non-finite probabilities")
        if np.any((calibrated < 0.0) | (calibrated > 1.0)):
            raise RuntimeError("GRUHLROracle calibration produced probabilities outside [0, 1]")
        return PredictionOutput(calibrated, raw.half_lives, raw.interval_days)

    def calibrated_probability(self, half_life: float, delta_days: float) -> float:
        raw = math.pow(2.0, -delta_days / max(half_life, 1e-6))
        return float(self.calibration.predict(np.asarray([raw]))[0])

    def save(self, base_path: Path) -> None:
        self.validate()
        base_path.parent.mkdir(parents=True, exist_ok=True)
        torch.save(self.model.state_dict(), base_path.with_suffix(".pt"))
        with base_path.with_suffix(".iso.pkl").open("wb") as handle:
            pickle.dump(self.calibration, handle)
        with base_path.with_suffix(".meta.json").open("w", encoding="utf-8") as handle:
            json.dump(
                {
                    "device": self.device,
                    "hiddenSize": self.hidden_size,
                    "loaderWorkers": self.loader_workers,
                },
                handle,
                indent=2,
            )

    @classmethod
    def load(cls, base_path: Path) -> "GRUHLROracle":
        loader_workers = None
        hidden_size = 64
        meta_path = base_path.with_suffix(".meta.json")
        if meta_path.exists():
            meta = json.loads(meta_path.read_text(encoding="utf-8"))
            loader_workers = meta.get("loaderWorkers")
            hidden_size = int(meta.get("hiddenSize", hidden_size))
        # 推理设备默认 cpu（数值可复现）；吞吐密集场景（tune）可用
        # BENCH_ORACLE_DEVICE=mps 切 GPU（GRU f32 推理，logLoss 聚合差异可忽略）
        device = os.environ.get("BENCH_ORACLE_DEVICE", "cpu")
        if device == "mps" and not torch.backends.mps.is_available():
            device = "cpu"
        instance = cls(device=device, loader_workers=loader_workers, hidden_size=hidden_size)
        state = torch.load(base_path.with_suffix(".pt"), map_location="cpu")
        instance.model.load_state_dict(state)
        instance.model.to(instance.device)
        instance.model.eval()
        with base_path.with_suffix(".iso.pkl").open("rb") as handle:
            instance.calibration = pickle.load(handle)
        try:
            instance.validate()
        except RuntimeError as exc:
            raise RuntimeError(f"invalid GRUHLROracle artifact at {base_path}") from exc
        return instance


def metric_summary(y_true: np.ndarray, probabilities: np.ndarray, half_lives: np.ndarray, true_half_lives: np.ndarray) -> Dict[str, float]:
    if len(y_true) == 0:
        return {
            "logLoss": 0.0,
            "ici": 0.0,
            "auc": 0.5,
            "maeP": 0.0,
            "smapeH": 0.0,
        }
    probs = np.clip(probabilities.astype(float), 1e-6, 1 - 1e-6)
    truth = y_true.astype(float)
    log_loss = float(
        -np.mean(truth * np.log(probs) + (1.0 - truth) * np.log(1.0 - probs))
    )
    mae_p = float(np.mean(np.abs(probs - truth)))
    order = np.argsort(probs)
    bins = np.array_split(order, 20)
    ici = 0.0
    total = 0
    for bucket in bins:
        if len(bucket) == 0:
            continue
        ici += len(bucket) * abs(float(truth[bucket].mean()) - float(probs[bucket].mean()))
        total += len(bucket)
    ici = float(ici / max(total, 1))
    auc = 0.5
    if len(np.unique(truth)) > 1:
        auc = float(roc_auc_score(truth, probs))
    smape_h = float(
        np.mean(
            2.0
            * np.abs(half_lives - true_half_lives)
            / np.maximum(np.abs(half_lives) + np.abs(true_half_lives), EPS)
        )
    )
    return {
        "logLoss": log_loss,
        "ici": ici,
        "auc": auc,
        "maeP": mae_p,
        "smapeH": smape_h,
    }


def probability_metric_summary(y_true: np.ndarray, probabilities: np.ndarray) -> Dict[str, float]:
    if len(y_true) == 0:
        return {
            "logLoss": 0.0,
            "auc": 0.5,
            "maeP": 0.0,
            "std": 0.0,
        }
    probs = np.clip(np.asarray(probabilities, dtype=float), 1e-6, 1 - 1e-6)
    truth = np.asarray(y_true, dtype=float)
    auc = 0.5
    if len(np.unique(truth)) > 1:
        auc = float(roc_auc_score(truth, probs))
    return {
        "logLoss": float(-np.mean(truth * np.log(probs) + (1.0 - truth) * np.log(1.0 - probs))),
        "auc": auc,
        "maeP": float(np.mean(np.abs(probs - truth))),
        "std": float(np.std(probs)),
    }


def prediction_composite(metrics: Dict[str, float], baseline_metrics: Dict[str, float]) -> float:
    def ratio(smaller_better: bool, key: str) -> float:
        base = baseline_metrics[key]
        current = metrics[key]
        if smaller_better:
            return float(base / max(current, EPS))
        return float(current / max(base, EPS))

    return (
        0.35 * ratio(True, "logLoss")
        + 0.30 * ratio(True, "ici")
        + 0.20 * ratio(False, "auc")
        + 0.15 * ratio(True, "maeP")
    )
