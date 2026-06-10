from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import duckdb

from benchmarks.maimemo.config import resolve_bench_root
from benchmarks.maimemo.data import prepare_from_local_tsvs


FIXTURES = Path(__file__).resolve().parent / "fixtures"


class PrepareDatasetTest(unittest.TestCase):
    def test_prepare_creates_parquet_and_fixed_user_split(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            paths = resolve_bench_root(tmpdir)
            summary = prepare_from_local_tsvs(
                paths=paths,
                raw_tsv=FIXTURES / "raw.tsv",
                difficulty_tsv=FIXTURES / "difficulty.tsv",
                offset_tsv=FIXTURES / "offset.tsv",
            )

            self.assertEqual(summary["rows"], 4)
            prefix_path = paths.parquet / "prefix_events.parquet"
            sequence_path = paths.parquet / "sequence_groups.parquet"
            self.assertTrue(prefix_path.exists())
            self.assertTrue(sequence_path.exists())

            conn = duckdb.connect()
            rows = conn.execute(
                'SELECT u, w, difficulty, "offset", split FROM read_parquet(?) ORDER BY u, w, step',
                [str(prefix_path)],
            ).fetchall()
            self.assertEqual(len(rows), 4)
            self.assertTrue(all(row[2] is not None for row in rows))
            self.assertTrue(all(row[3] >= 0 for row in rows))

            split_rows = conn.execute(
                "SELECT u, COUNT(DISTINCT split) FROM read_parquet(?) GROUP BY u",
                [str(prefix_path)],
            ).fetchall()
            self.assertTrue(all(count == 1 for _, count in split_rows))

            prepare_summary = json.loads((paths.metadata / "prepare_summary.json").read_text("utf-8"))
            self.assertEqual(prepare_summary["missingDifficulty"], 0)
            self.assertEqual(prepare_summary["missingOffset"], 0)


if __name__ == "__main__":
    unittest.main()
