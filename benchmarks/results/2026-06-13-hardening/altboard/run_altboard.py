"""Hardening re-run of the unofficial oracle-weighted alt-board for the F1 TEST results.

Reuses docs/algo-bench-2026-06-12-d5-ship/appendix/alt_board.py verbatim (imported as a
module — zero reimplementation) against benchmarks/results/2026-06-13-gsp-ship/.
The `official` variant is the zero-deviation anchor: it must reproduce the official
2026-06-13 board (amas Borda 30 / fsrs45 26 / amas6 23 / fsrs6 19 / fsrs 17).

Outputs (this directory):
  altboard-2026-06-13-gsp-ship.md   — three boards (official anchor / alt-1 / alt-2)
  altboard-2026-06-13-gsp-ship.json — full per-(algo,dataset) rows for all variants
"""
from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]
sys.path.insert(0, str(REPO_ROOT))

spec = importlib.util.spec_from_file_location(
    "alt_board",
    REPO_ROOT / "docs/algo-bench-2026-06-12-d5-ship/appendix/alt_board.py",
)
alt_board = importlib.util.module_from_spec(spec)
spec.loader.exec_module(alt_board)

RESULTS = REPO_ROOT / "benchmarks/results/2026-06-13-gsp-ship"

VARIANTS = [
    ("official", "Official formula (v0.9: 0.7*mastered + 0.3*efficiency*10000) [anchor]"),
    ("alt", "Alt-1 oracle-weighted [unofficial] (0.5*expectedMemoryFinal + 0.3*efficiency*10000 + 0.2*mastered)"),
    ("pure_em", "Alt-2 pure expectedMemoryFinal [unofficial]"),
]

md_lines = [
    "# Oracle-truth alt-board recompute for the F1 TEST results (2026-06-13-gsp-ship)",
    "",
    "Unofficial disclosure appendix, hardening campaign. Formulae and pipeline are",
    "byte-identical to docs/algo-bench-2026-06-12-d5-ship/appendix/alt_board.py;",
    "only --results points at benchmarks/results/2026-06-13-gsp-ship/.",
    "",
]
dump: dict[str, object] = {"results_dir": str(RESULTS.relative_to(REPO_ROOT)), "variants": {}}

for key, title in VARIANTS:
    df, agg = alt_board.compute_board(RESULTS, key)
    md_lines.append(alt_board.board_md(agg, df, title))
    cols = [
        "algo", "dataset", "expectedMemoryFinal", "masteredCount", "efficiency",
        "prediction_raw", "dhp_raw", "policy_raw",
        "prediction_score", "dhp_score", "policy_score", "final_score",
    ]
    rank_col = [c for c in df.columns if c.startswith("rank")]
    dump["variants"][key] = {
        "aggregate": json.loads(agg.to_json(orient="records")),
        "per_dataset": json.loads(df[cols + rank_col].to_json(orient="records")),
    }

(HERE / "altboard-2026-06-13-gsp-ship.md").write_text("\n".join(md_lines))
(HERE / "altboard-2026-06-13-gsp-ship.json").write_text(json.dumps(dump, indent=2))
print("\n".join(md_lines))
