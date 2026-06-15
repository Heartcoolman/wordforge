from __future__ import annotations

import pickle
import tempfile
import unittest
from pathlib import Path

import numpy as np
import pandas as pd
import torch

from benchmarks.maimemo.models import GRUHLROracle, SequenceDataset, _sequence_collate
from benchmarks.maimemo.pipeline import _ensure_oracle_guard


def _sample_frame() -> pd.DataFrame:
    return pd.DataFrame(
        [
            {
                "t_history": "1,3,7",
                "r_history": "1,1,0",
                "difficulty": 7.0,
                "next_t": 5.0,
                "next_r": 1.0,
            },
            {
                "t_history": "2,6",
                "r_history": "1,0",
                "difficulty": np.nan,
                "next_t": 9.0,
                "next_r": 0.0,
            },
            {
                "t_history": "",
                "r_history": "",
                "difficulty": float("inf"),
                "next_t": 2.0,
                "next_r": 1.0,
            },
        ]
    )


class SequenceDatasetTest(unittest.TestCase):
    def test_missing_and_non_finite_difficulty_are_sanitized(self) -> None:
        dataset = SequenceDataset(_sample_frame())

        batch = _sequence_collate([dataset[index] for index in range(len(dataset))])

        self.assertTrue(torch.isfinite(batch["difficulty"]).all())
        self.assertTrue(torch.isfinite(batch["sequences"]).all())
        self.assertEqual(batch["difficulty"][1].item(), 0.0)
        self.assertEqual(batch["difficulty"][2].item(), 0.0)


class GRUHLROracleTest(unittest.TestCase):
    def test_train_on_frame_keeps_parameters_finite_with_missing_difficulty(self) -> None:
        oracle = GRUHLROracle(device="cpu")
        optimizer = torch.optim.Adam(oracle.model.parameters(), lr=1e-3, weight_decay=1e-5)

        oracle._train_on_frame(_sample_frame(), optimizer=optimizer, batch_size=2)

        oracle.calibration.fit(np.asarray([0.1, 0.9]), np.asarray([0.0, 1.0]))
        oracle.validate()

    def test_load_rejects_non_finite_artifact(self) -> None:
        oracle = GRUHLROracle(device="cpu")
        oracle.calibration.fit(np.asarray([0.1, 0.9]), np.asarray([0.0, 1.0]))

        with tempfile.TemporaryDirectory() as tmpdir:
            base_path = Path(tmpdir) / "gru_hlr_oracle"
            bad_state = {
                name: torch.full_like(parameter, float("nan"))
                for name, parameter in oracle.model.state_dict().items()
            }
            torch.save(bad_state, base_path.with_suffix(".pt"))
            with base_path.with_suffix(".iso.pkl").open("wb") as handle:
                pickle.dump(oracle.calibration, handle)

            with self.assertRaisesRegex(RuntimeError, "invalid GRUHLROracle artifact"):
                GRUHLROracle.load(base_path)

    def test_fit_calibration_selects_a_usable_candidate(self) -> None:
        oracle = GRUHLROracle(device="cpu")
        probs = np.linspace(0.05, 0.95, 2000)
        labels = (probs > 0.55).astype(float)

        selection = oracle.fit_calibration(probs, labels)
        predicted = oracle.calibration.predict(np.asarray([0.2, 0.5, 0.8]))

        self.assertIn(selection["selected"], {"identity", "isotonic", "logistic"})
        self.assertIn("candidates", selection)
        self.assertEqual(predicted.shape, (3,))
        self.assertTrue(np.all(np.isfinite(predicted)))


class OracleGuardTest(unittest.TestCase):
    def test_guard_rejects_failed_fit_summary(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            reports = root / "reports"
            reports.mkdir(parents=True, exist_ok=True)
            (reports / "fit_oracle_summary.json").write_text(
                '{"oracleBeatsHlr": false}',
                encoding="utf-8",
            )

            class _Paths:
                def __init__(self, reports_path: Path) -> None:
                    self.reports = reports_path

            with self.assertRaisesRegex(RuntimeError, "oracleBeatsHlr=false"):
                _ensure_oracle_guard(_Paths(reports))


if __name__ == "__main__":
    unittest.main()
