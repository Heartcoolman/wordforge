from __future__ import annotations

import hashlib
import json
import shutil
import urllib.request
from pathlib import Path
from typing import Dict, Optional

import duckdb
import py7zr

from .config import (
    DATASET_API_URL,
    DATASET_LABELS,
    DEFAULT_RANDOM_SEED,
    EXTRACTED_FILENAMES,
    STATIC_FILE_MANIFEST,
    BenchPaths,
)


def fetch_dataset_metadata() -> Dict:
    request = urllib.request.Request(
        DATASET_API_URL,
        headers={
            "User-Agent": "WordForgeBenchmark/1.0 (+https://github.com/Heartcoolman/wordforge)",
            "Accept": "application/json",
        },
    )
    with urllib.request.urlopen(request) as response:
        return json.load(response)


def build_file_manifest() -> Dict[str, Dict]:
    try:
        metadata = fetch_dataset_metadata()
        files = metadata["data"]["latestVersion"]["files"]
        manifest: Dict[str, Dict] = {}
        for logical_name, label in DATASET_LABELS.items():
            matched = next((entry for entry in files if entry["label"] == label), None)
            if matched is None:
                raise RuntimeError(f"missing dataset file {label}")
            manifest[logical_name] = {
                "label": label,
                "id": matched["dataFile"]["id"],
                "md5": matched["dataFile"]["md5"],
                "filesize": matched["dataFile"]["filesize"],
                "download_url": f"https://dataverse.harvard.edu/api/access/datafile/{matched['dataFile']['id']}",
            }
        return manifest
    except Exception:
        return json.loads(json.dumps(STATIC_FILE_MANIFEST))


def _md5sum(path: Path) -> str:
    digest = hashlib.md5()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def download_file(url: str, path: Path, expected_md5: str) -> Path:
    if path.exists() and _md5sum(path) == expected_md5:
        return path
    request = urllib.request.Request(
        url,
        headers={
            "User-Agent": "WordForgeBenchmark/1.0 (+https://github.com/Heartcoolman/wordforge)",
        },
    )
    with urllib.request.urlopen(request) as response, path.open("wb") as output:
        while True:
            chunk = response.read(1024 * 1024)
            if not chunk:
                break
            output.write(chunk)
    actual_md5 = _md5sum(path)
    if actual_md5 != expected_md5:
        raise RuntimeError(f"checksum mismatch for {path.name}: {actual_md5} != {expected_md5}")
    return path


def extract_archive(archive_path: Path, output_path: Path) -> Path:
    if output_path.exists():
        return output_path
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with py7zr.SevenZipFile(archive_path, mode="r") as archive:
        archive.extractall(path=output_path.parent / f".{output_path.stem}_tmp")
    tmp_dir = output_path.parent / f".{output_path.stem}_tmp"
    extracted = list(tmp_dir.glob("*.tsv"))
    if len(extracted) != 1:
        raise RuntimeError(f"expected exactly one tsv in {archive_path}, got {extracted}")
    extracted[0].rename(output_path)
    shutil.rmtree(tmp_dir, ignore_errors=True)
    return output_path


def ensure_dataset_files(paths: BenchPaths) -> Dict[str, Path]:
    manifest = build_file_manifest()
    (paths.metadata / "dataset_manifest.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    extracted: Dict[str, Path] = {}
    for logical_name, file_info in manifest.items():
        archive_path = paths.archives / file_info["label"]
        download_file(file_info["download_url"], archive_path, file_info["md5"])
        extracted_path = paths.extracted / EXTRACTED_FILENAMES[logical_name]
        extracted[logical_name] = extract_archive(archive_path, extracted_path)
    return extracted


def _copy_query_to_parquet(
    conn: duckdb.DuckDBPyConnection,
    query: str,
    output_path: Path,
) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    conn.execute(
        f"COPY ({query}) TO '{output_path.as_posix()}' (FORMAT PARQUET, COMPRESSION ZSTD)"
    )


def prepare_from_local_tsvs(
    paths: BenchPaths,
    raw_tsv: Path,
    difficulty_tsv: Path,
    offset_tsv: Path,
    forgetting_curve_tsv: Optional[Path] = None,
    max_rows: Optional[int] = None,
    build_sequence_groups: bool = True,
) -> Dict[str, object]:
    conn = duckdb.connect(str(paths.cache / "prepare.duckdb"))
    conn.execute(f"PRAGMA threads={max(1, 4)}")

    raw_query = f"""
        SELECT
            CAST(u AS VARCHAR) AS u,
            CAST(w AS VARCHAR) AS w,
            CAST(i AS BIGINT) AS step,
            CAST(t_history AS VARCHAR) AS t_history,
            CAST(r_history AS VARCHAR) AS r_history,
            CAST(delta_t AS BIGINT) AS next_t,
            CAST(r AS INTEGER) AS next_r
        FROM read_csv_auto('{raw_tsv.as_posix()}', delim='\\t', header=true)
        {f'LIMIT {int(max_rows)}' if max_rows else ''}
    """
    difficulty_query = f"""
        SELECT
            CAST(w AS VARCHAR) AS w,
            CAST(d AS DOUBLE) AS difficulty
        FROM read_csv_auto('{difficulty_tsv.as_posix()}', delim='\\t', header=true)
    """
    offset_query = f"""
        SELECT
            CAST(u AS VARCHAR) AS u,
            CAST(w AS VARCHAR) AS w,
            CAST(i AS BIGINT) AS step,
            CAST("offset" AS BIGINT) AS review_offset
        FROM read_csv_auto('{offset_tsv.as_posix()}', delim='\\t', header=true)
    """
    curve_query = None
    if forgetting_curve_tsv:
        curve_query = f"""
            SELECT
                CAST(i AS BIGINT) AS step,
                CAST(t_history AS VARCHAR) AS t_history,
                CAST(r_history AS VARCHAR) AS r_history,
                CAST(delta_t AS BIGINT) AS next_t,
                SUM(CAST(p_recall AS DOUBLE) * CAST(total_cnt AS DOUBLE))
                    / NULLIF(SUM(CAST(total_cnt AS DOUBLE)), 0.0) AS dhp_p_recall,
                SUM(CAST(total_cnt AS BIGINT)) AS group_count,
                CAST(d AS BIGINT) AS difficulty
            FROM read_csv_auto('{forgetting_curve_tsv.as_posix()}', delim='\\t', header=true)
            GROUP BY 1, 2, 3, 4, 7
        """

    if curve_query:
        prefix_query = f"""
        WITH raw AS ({raw_query}),
        difficulty AS ({difficulty_query}),
        offsets AS ({offset_query}),
        curves AS ({curve_query})
        SELECT
            raw.u,
            raw.w,
            raw.step,
            COALESCE(offsets.review_offset, -1) AS "offset",
            ABS(HASH(raw.u)) % 10 AS user_bucket_10,
            ABS(HASH(raw.u)) % 100 AS user_bucket_100,
            difficulty.difficulty,
            raw.r_history,
            raw.t_history,
            raw.next_r,
            raw.next_t,
            curves.dhp_p_recall,
            curves.group_count,
            CASE
                WHEN ABS(HASH(raw.u)) % 10 < 8 THEN 'train'
                WHEN ABS(HASH(raw.u)) % 10 = 8 THEN 'val'
                ELSE 'test'
            END AS split
        FROM raw
        LEFT JOIN difficulty
            ON raw.w = difficulty.w
        LEFT JOIN offsets
            ON raw.u = offsets.u
           AND raw.w = offsets.w
           AND raw.step = offsets.step
        LEFT JOIN curves
            ON CAST(difficulty.difficulty AS BIGINT) = curves.difficulty
           AND raw.step = curves.step
           AND raw.t_history = curves.t_history
           AND raw.r_history = curves.r_history
           AND raw.next_t = curves.next_t
    """
    else:
        prefix_query = f"""
        WITH raw AS ({raw_query}),
        difficulty AS (
            SELECT
                CAST(w AS VARCHAR) AS w,
                CAST(d AS DOUBLE) AS difficulty,
                CAST(t_history AS VARCHAR) AS t_history,
                CAST(r_history AS VARCHAR) AS r_history,
                CAST(delta_t AS BIGINT) AS next_t,
                CAST(p_recall AS DOUBLE) AS dhp_p_recall,
                CAST(total_cnt AS BIGINT) AS group_count
            FROM read_csv_auto('{difficulty_tsv.as_posix()}', delim='\\t', header=true)
        ),
        offsets AS ({offset_query})
        SELECT
            raw.u,
            raw.w,
            raw.step,
            COALESCE(offsets.review_offset, -1) AS "offset",
            ABS(HASH(raw.u)) % 10 AS user_bucket_10,
            ABS(HASH(raw.u)) % 100 AS user_bucket_100,
            difficulty.difficulty,
            raw.r_history,
            raw.t_history,
            raw.next_r,
            raw.next_t,
            difficulty.dhp_p_recall,
            difficulty.group_count,
            CASE
                WHEN ABS(HASH(raw.u)) % 10 < 8 THEN 'train'
                WHEN ABS(HASH(raw.u)) % 10 = 8 THEN 'val'
                ELSE 'test'
            END AS split
        FROM raw
        LEFT JOIN difficulty
            ON raw.w = difficulty.w
           AND raw.t_history = difficulty.t_history
           AND raw.r_history = difficulty.r_history
           AND raw.next_t = difficulty.next_t
        LEFT JOIN offsets
            ON raw.u = offsets.u
           AND raw.w = offsets.w
           AND raw.step = offsets.step
        """
    _copy_query_to_parquet(conn, prefix_query, paths.parquet / "prefix_events.parquet")

    if build_sequence_groups:
        sequence_query = f"""
            SELECT
                u,
                w,
                ANY_VALUE(split) AS split,
                MAX(difficulty) AS difficulty,
                ANY_VALUE(user_bucket_10) AS user_bucket_10,
                ANY_VALUE(user_bucket_100) AS user_bucket_100,
                LIST(step ORDER BY step) AS steps,
                LIST("offset" ORDER BY step) AS offsets,
                LIST(next_t ORDER BY step) AS next_t_values,
                LIST(next_r ORDER BY step) AS next_r_values,
                LIST(r_history ORDER BY step) AS r_history_values,
                LIST(t_history ORDER BY step) AS t_history_values
            FROM read_parquet('{(paths.parquet / "prefix_events.parquet").as_posix()}')
            GROUP BY u, w
        """
        _copy_query_to_parquet(conn, sequence_query, paths.parquet / "sequence_groups.parquet")

    summary = conn.execute(
        """
        SELECT
            COUNT(*) AS rows,
            SUM(CASE WHEN difficulty IS NULL THEN 1 ELSE 0 END) AS missing_difficulty,
            SUM(CASE WHEN "offset" < 0 THEN 1 ELSE 0 END) AS missing_offset
        FROM read_parquet(?)
        """,
        [str(paths.parquet / "prefix_events.parquet")],
    ).fetchone()

    metadata = {
        "rows": int(summary[0]),
        "missingDifficulty": int(summary[1]),
        "missingOffset": int(summary[2]),
        "maxRows": max_rows,
        "seed": DEFAULT_RANDOM_SEED,
        "rawTsv": str(raw_tsv),
        "difficultyTsv": str(difficulty_tsv),
        "offsetTsv": str(offset_tsv),
        "forgettingCurveTsv": str(forgetting_curve_tsv) if forgetting_curve_tsv else None,
        "buildSequenceGroups": build_sequence_groups,
    }
    (paths.metadata / "prepare_summary.json").write_text(
        json.dumps(metadata, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    return metadata


def prepare_dataset(
    paths: BenchPaths,
    max_rows: Optional[int] = None,
    build_sequence_groups: bool = True,
) -> Dict[str, object]:
    extracted = ensure_dataset_files(paths)
    return prepare_from_local_tsvs(
        paths=paths,
        raw_tsv=extracted["raw"],
        difficulty_tsv=extracted["difficulty"],
        offset_tsv=extracted["offset"],
        forgetting_curve_tsv=extracted.get("forgetting_curve"),
        max_rows=max_rows,
        build_sequence_groups=build_sequence_groups,
    )
