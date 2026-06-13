"""build_parquet.py — prod_events.csv → <bench_root>/parquet/prefix_events.parquet.

Transforms exported `learning_records` rows (export.sql Q1) into the maimemo-harness
prefix-event layout (spec §3.2) so `evaluate_scheduler_prediction` can be reused
verbatim with zero changes to benchmarks/maimemo/*:

  - group by (user_id, word_id), keep export order within groups (Q1 already
    ORDER BY created_at, id; ties on created_at preserve file order);
  - drop groups with <2 events (need ≥1 history event + 1 held-out target);
  - t_history = comma-joined integer day deltas: first element 0, then
    floor((ts_i − ts_{i−1})/86400 s) for events 1..n−2;
  - r_history = comma-joined is_correct for events 0..n−2;
  - held-out target = LAST event: next_t = floor(Δdays) clamped ≥0, next_r = is_correct;
  - difficulty = 5.0 (AMAS warm_start ignores it; only DHP consumes it);
  - split = 'test'; user_bucket_100 = int(md5(u).hexdigest(), 16) % 100.

Output root resolved via --root or WORD_FORGE_BENCH_DATA (resolve_bench_root).
Safety: refuses to run without an explicit root, and refuses to write into the
default maimemo bench root (would clobber the real benchmark parquet).

Usage:
  .bench-venv/bin/python benchmarks/prod_replay/build_parquet.py \
      --events prod_events.csv --root ~/.wordforge-bench/wordforge_prod
"""
from __future__ import annotations

import argparse
import hashlib
import os
import sys
from pathlib import Path
from typing import Dict, List, Tuple

import duckdb
import pandas as pd

_REPO_ROOT = Path(__file__).resolve().parents[2]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from benchmarks.maimemo.config import (  # noqa: E402
    DEFAULT_BENCH_ROOT,
    DEFAULT_BENCH_ROOT_ENV,
    resolve_bench_root,
)

SECS_PER_DAY = 86_400.0


def _bucket_100(user_id: str) -> int:
    return int(hashlib.md5(str(user_id).encode("utf-8")).hexdigest(), 16) % 100


def load_event_groups(events_csv: Path) -> Dict[Tuple[str, str], List[dict]]:
    """CSV → {(user_id, word_id): [event dicts in chronological order]}.

    Events carry epoch-second timestamps (`ts`), `is_correct` (int 0/1) and
    `response_time_ms` (float, NaN when absent). Order: stable sort by parsed
    created_at with original file order as tiebreak (mirrors Q1's
    ORDER BY created_at, id — `id` is not exported, the file order encodes it).
    Shared with run_prediction.py --grade-aware so both legs see identical groups.
    """
    df = pd.read_csv(events_csv, dtype={"user_id": str, "word_id": str})
    required = {"user_id", "word_id", "created_at", "is_correct"}
    missing = required - set(df.columns)
    if missing:
        raise SystemExit(f"events csv missing columns: {sorted(missing)}")
    ts = pd.to_datetime(df["created_at"], utc=True, format="ISO8601")
    # epoch seconds, resolution-safe (pandas may parse to ns or us depending on version)
    epoch = (ts - pd.Timestamp(0, tz="UTC")).dt.total_seconds()
    df = df.assign(
        ts_epoch=epoch,
        file_order=range(len(df)),
    )
    df = df.sort_values(["user_id", "word_id", "ts_epoch", "file_order"], kind="stable")

    groups: Dict[Tuple[str, str], List[dict]] = {}
    has_rt = "response_time_ms" in df.columns
    for row in df.itertuples(index=False):
        key = (row.user_id, row.word_id)
        rt = float(getattr(row, "response_time_ms")) if has_rt else float("nan")
        groups.setdefault(key, []).append(
            {
                "ts": float(row.ts_epoch),
                "is_correct": int(row.is_correct),
                "response_time_ms": rt,
            }
        )
    return groups


def build_rows(groups: Dict[Tuple[str, str], List[dict]]) -> pd.DataFrame:
    rows = []
    n_dropped = 0
    for (user_id, word_id), events in sorted(groups.items()):
        if len(events) < 2:
            n_dropped += 1
            continue
        deltas = [0]
        for i in range(1, len(events)):
            deltas.append(int((events[i]["ts"] - events[i - 1]["ts"]) // SECS_PER_DAY))
        # history = events 0..n-2; held-out target = last event
        t_history = ",".join(str(d) for d in deltas[:-1])
        r_history = ",".join(str(e["is_correct"]) for e in events[:-1])
        next_t = max(0, deltas[-1])
        next_r = events[-1]["is_correct"]
        rows.append(
            {
                "u": user_id,
                "w": word_id,
                "t_history": t_history,
                "r_history": r_history,
                "difficulty": 5.0,
                "next_t": next_t,
                "next_r": next_r,
                "split": "test",
                "user_bucket_100": _bucket_100(user_id),
            }
        )
    out = pd.DataFrame(
        rows,
        columns=[
            "u", "w", "t_history", "r_history", "difficulty",
            "next_t", "next_r", "split", "user_bucket_100",
        ],
    )
    print(f"[build_parquet] groups={len(groups)} kept={len(out)} dropped_lt2={n_dropped}")
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--events", required=True, help="prod_events.csv (export.sql Q1)")
    ap.add_argument("--root", default=None,
                    help=f"bench root (default: ${DEFAULT_BENCH_ROOT_ENV})")
    args = ap.parse_args()

    if not args.root and not os.environ.get(DEFAULT_BENCH_ROOT_ENV):
        raise SystemExit(
            f"refusing to run without an explicit bench root: pass --root or set "
            f"${DEFAULT_BENCH_ROOT_ENV} (the implicit default {DEFAULT_BENCH_ROOT} "
            f"is the real maimemo benchmark root)"
        )
    paths = resolve_bench_root(args.root)
    if paths.root == DEFAULT_BENCH_ROOT.expanduser().resolve():
        raise SystemExit(
            f"refusing to write into the default maimemo bench root {paths.root}; "
            f"use a dedicated root (e.g. ~/.wordforge-bench/wordforge_prod)"
        )

    df = build_rows(load_event_groups(Path(args.events).expanduser()))
    if df.empty:
        raise SystemExit("no (user, word) groups with >=2 events — nothing to write")

    target = paths.parquet / "prefix_events.parquet"
    conn = duckdb.connect()
    conn.register("df", df)
    conn.execute(
        f"COPY (SELECT * FROM df) TO '{target.as_posix()}' (FORMAT PARQUET)"
    )
    conn.close()
    print(f"[build_parquet] wrote {len(df)} rows -> {target}")


if __name__ == "__main__":
    main()
