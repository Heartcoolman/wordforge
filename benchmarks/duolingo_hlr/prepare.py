"""下载 Duolingo HLR (learning_traces.13m) 并对齐到 maimemo prefix_events schema。

原始 schema (Settles & Meeder, 2016, ACL):
    p_recall, timestamp, delta, user_id, learning_language, ui_language,
    lexeme_id, lexeme_string, history_seen, history_correct,
    session_seen, session_correct

目标 schema (与 benchmarks.maimemo.data prefix_events 完全一致):
    u           : string
    w           : string
    step        : int64   (从 2 起算，等同 maimemo)
    offset      : int64   (累计 t_history 之和，相对学习起点的天数)
    user_bucket_10  : uint64
    user_bucket_100 : uint64
    difficulty  : double  (1-10 离散，由 history_seen 派生)
    r_history   : string  ("0,1,1,0" 形式)
    t_history   : string  ("0,1,3,7" 形式，首位 0 表示初学)
    next_r      : int32
    next_t      : int64
    dhp_p_recall: double  (null：HLR 无 DHP 标定)
    group_count : double  (null：HLR 无 DHP 分组)
    split       : string  (train/val/test, 8:1:1 by user hash)

运行:
    python -m benchmarks.duolingo_hlr.prepare
"""

from __future__ import annotations

import gzip
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import List

import duckdb

OUT_ROOT = Path("/Users/liji/.wordforge-bench/duolingo_hlr")
RAW_DIR = OUT_ROOT / "raw"
PARQUET_DIR = OUT_ROOT / "parquet"
CACHE_DIR = OUT_ROOT / "cache"

RAW_CSV = RAW_DIR / "learning_traces.13m.csv"
RAW_GZ = RAW_DIR / "learning_traces.13m.csv.gz"

# 下载尝试列表：按顺序尝试，任一成功即可
# 经实测 2026-05-27：duolingo/half-life-regression GitHub 仓库已无 csv.gz；
# 真实权威源是 Harvard Dataverse（Settles 2017 deposit，DOI 10.7910/DVN/N8XJME）
DOWNLOAD_URLS: List[str] = [
    # 权威源：Harvard Dataverse（361 MB gz，会 303 重定向到 AWS S3 签名 URL）
    "https://dataverse.harvard.edu/api/access/datafile/:persistentId?persistentId=doi:10.7910/DVN/N8XJME/UEPJVH",
    # 历史 fallback（GitHub raw / Duolingo S3，均已 404 但保留作记录）
    "https://github.com/duolingo/half-life-regression/raw/master/learning_traces.13m.csv.gz",
    "https://raw.githubusercontent.com/duolingo/half-life-regression/master/learning_traces.13m.csv.gz",
    "https://s3.amazonaws.com/duolingo-papers/publications/settles.acl16.learning_traces.13m.csv.gz",
]


def _try_download(url: str, dest: Path) -> bool:
    """单源下载尝试，成功返回 True。

    用 curl 子进程是因为：
    - Dataverse 会 303 重定向到 AWS S3 签名 URL，urllib 默认 UA 会被 S3 403；
    - curl -L 跟随重定向更稳；
    - 大文件流式下载，避免内存膨胀。
    """
    dest.parent.mkdir(parents=True, exist_ok=True)
    print(f"[duolingo_hlr] 下载尝试: {url}")
    try:
        result = subprocess.run(
            [
                "curl",
                "-sSL",
                "-A", "Mozilla/5.0 (wordforge-bench)",
                "--max-time", "900",
                "-o", str(dest),
                "-w", "%{http_code} %{size_download}",
                url,
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            print(f"  curl 失败 rc={result.returncode}: {result.stderr.strip()[:200]}")
            dest.unlink(missing_ok=True)
            return False
        meta = result.stdout.strip().split()
        http_code = meta[0] if meta else "?"
        size = dest.stat().st_size if dest.exists() else 0
        if http_code != "200":
            print(f"  HTTP {http_code}, 下载 {size} B")
            dest.unlink(missing_ok=True)
            return False
        if size < 1024 * 1024 * 50:  # < 50MB 视为失败（实际 ~360MB gz）
            print(f"  文件过小 ({size} B)，视为失败")
            dest.unlink(missing_ok=True)
            return False
        print(f"  下载成功，{size / 1024 / 1024:.1f} MB")
        return True
    except (FileNotFoundError, subprocess.SubprocessError, OSError) as e:
        print(f"  失败: {type(e).__name__}: {e}")
        dest.unlink(missing_ok=True)
        return False


def download_dataset() -> Path:
    """按顺序尝试下载源，任一成功即返回 CSV 路径。

    若 CSV 已存在直接复用；否则下载 gz 并解压。
    全失败 → SystemExit(1)。
    """
    if RAW_CSV.exists() and RAW_CSV.stat().st_size > 1024 * 1024 * 100:  # >100MB
        print(f"[duolingo_hlr] 复用已存在 CSV: {RAW_CSV} ({RAW_CSV.stat().st_size / 1024 / 1024:.1f} MB)")
        return RAW_CSV

    if not RAW_GZ.exists() or RAW_GZ.stat().st_size < 1024 * 1024:
        for url in DOWNLOAD_URLS:
            if _try_download(url, RAW_GZ):
                break
        else:
            print(
                "[duolingo_hlr] 所有下载源均失败。\n"
                "  手动 fallback: 把 learning_traces.13m.csv.gz 放到 "
                f"{RAW_GZ}\n"
                "  Web 搜索关键词: settles.acl16.learning_traces.13m.csv.gz mirror",
                file=sys.stderr,
            )
            raise SystemExit(1)

    print(f"[duolingo_hlr] 解压 {RAW_GZ} -> {RAW_CSV}")
    with gzip.open(RAW_GZ, "rb") as src, open(RAW_CSV, "wb") as dst:
        shutil.copyfileobj(src, dst, length=1024 * 1024)
    print(f"  解压完成 {RAW_CSV.stat().st_size / 1024 / 1024:.1f} MB")
    return RAW_CSV


def build_parquets(csv_path: Path) -> dict:
    """用 duckdb 流式处理 13M 行 -> prefix_events + sequence_groups。"""
    PARQUET_DIR.mkdir(parents=True, exist_ok=True)
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    prefix_path = PARQUET_DIR / "prefix_events.parquet"
    sequence_path = PARQUET_DIR / "sequence_groups.parquet"

    conn = duckdb.connect(str(CACHE_DIR / "prepare.duckdb"))
    conn.execute("PRAGMA threads=4")
    conn.execute("PRAGMA memory_limit='12GB'")
    conn.execute(f"PRAGMA temp_directory='{(CACHE_DIR / 'duckdb_tmp').as_posix()}'")
    conn.execute("SET preserve_insertion_order=false")

    # 1) 读 CSV，做基础字段转换，并按 (u, w, timestamp) 排序生成 step
    #    Duolingo CSV header (no quotes, comma sep)：
    #      p_recall, timestamp, delta, user_id, learning_language, ui_language,
    #      lexeme_id, lexeme_string, history_seen, history_correct,
    #      session_seen, session_correct
    print("[duolingo_hlr] 加载 CSV 到 duckdb 临时表...")
    conn.execute(
        f"""
        CREATE OR REPLACE TABLE raw_events AS
        SELECT
            CAST(user_id  AS VARCHAR)  AS u,
            CAST(lexeme_id AS VARCHAR) AS w,
            CAST(timestamp AS BIGINT)  AS ts,
            -- next_t: delta 单位是秒，转换到天，min=1（HLR 同日复习给最小值）
            GREATEST(1, CAST(ROUND(CAST(delta AS DOUBLE) / 86400.0) AS BIGINT)) AS next_t,
            -- next_r: 阈值 0.5（HLR p_recall 是该 session 正确率，>0 即视为有正答 → 1）
            CASE WHEN CAST(p_recall AS DOUBLE) >= 0.5 THEN 1 ELSE 0 END AS next_r,
            -- difficulty: log1p(history_seen) / log1p(1000) -> 1-10 离散
            CAST(
                GREATEST(1.0, LEAST(10.0,
                    ROUND(LEAST(LN(1.0 + CAST(history_seen AS DOUBLE)) / LN(1001.0), 1.0) * 9.0 + 1.0)
                ))
                AS DOUBLE
            ) AS difficulty
        FROM read_csv_auto('{csv_path.as_posix()}', header=true, sample_size=-1)
        WHERE user_id IS NOT NULL
          AND lexeme_id IS NOT NULL
          AND timestamp IS NOT NULL
          AND delta IS NOT NULL
        """
    )
    n_raw = conn.execute("SELECT COUNT(*) FROM raw_events").fetchone()[0]
    print(f"  原始行数: {n_raw:,}")

    # 2) 在 (u, w) 内按 ts 升序生成 step；同时算每用户的学习起点 ts
    print("[duolingo_hlr] 生成 (u,w) 序列 step + 用户学习起点...")
    conn.execute(
        """
        CREATE OR REPLACE TABLE events_seq AS
        SELECT
            u, w, ts, next_t, next_r, difficulty,
            ROW_NUMBER() OVER (PARTITION BY u, w ORDER BY ts, next_t) AS step,
            -- 该用户首次出现的 ts（任何 word），作为 offset 的起点
            MIN(ts) OVER (PARTITION BY u) AS user_start_ts
        FROM raw_events
        """
    )

    # 3) 用 STRING_AGG window 累积生成 r_history / t_history（远比 LIST OVER 省内存）
    #    maimemo r_history 长度 = step - 1，首项固定 '0' 占位（不是真实 step=1 outcome）
    #    实现：prev_r_str = step 1..step-1 的 next_r 拼接（含 step=1 的真实 outcome）
    #         然后 r_history = '0' + ',' + prev_r_str[2..]（去掉第一个真实 outcome，补 '0'）
    #         在字符串层面：r_history = '0' + 去掉 prev_r_str 第一个 token 后剩余部分
    print("[duolingo_hlr] 计算 prev_r / prev_t (STRING_AGG 累积)...")
    conn.execute(
        """
        CREATE OR REPLACE TABLE events_prefix AS
        SELECT
            u, w, step, ts, user_start_ts, next_t, next_r, difficulty,
            STRING_AGG(CAST(next_r AS VARCHAR), ',') OVER (
                PARTITION BY u, w ORDER BY step
                ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING
            ) AS prev_r_str,
            STRING_AGG(CAST(next_t AS VARCHAR), ',') OVER (
                PARTITION BY u, w ORDER BY step
                ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING
            ) AS prev_t_str
        FROM events_seq
        """
    )

    # 4) 拼接 r_history / t_history + 计算 offset，输出 prefix_events.parquet
    print("[duolingo_hlr] 拼接历史 + 输出 prefix_events.parquet...")
    # 字符串处理：去掉首 token（直到第一个逗号或末尾）后再补 '0,' 前缀
    # 用 REGEXP_REPLACE 把 "X" 或 "X,..." 首段替换掉
    conn.execute(
        f"""
        COPY (
            SELECT
                u,
                w,
                step,
                -- offset: 用户学习起点到本次复习的天数（与 maimemo review_offset 语义对齐）
                CAST(GREATEST(0, ROUND((ts - user_start_ts) / 86400.0)) AS BIGINT) AS "offset",
                ABS(HASH(u)) % 10  AS user_bucket_10,
                ABS(HASH(u)) % 100 AS user_bucket_100,
                difficulty,
                -- r_history: 首项 '0' 占位 + (prev_r_str 去掉第一个 token)
                -- step=2 时 prev_r_str 长度 1（只有 step=1），去掉后空，故结果就是 '0'
                CASE
                    WHEN step = 2 THEN '0'
                    ELSE '0,' || REGEXP_REPLACE(prev_r_str, '^[^,]*,', '')
                END AS r_history,
                CASE
                    WHEN step = 2 THEN '0'
                    ELSE '0,' || REGEXP_REPLACE(prev_t_str, '^[^,]*,', '')
                END AS t_history,
                CAST(next_r AS INTEGER) AS next_r,
                next_t,
                CAST(NULL AS DOUBLE) AS dhp_p_recall,
                CAST(NULL AS DOUBLE) AS group_count,
                CASE
                    WHEN ABS(HASH(u)) % 10 < 8 THEN 'train'
                    WHEN ABS(HASH(u)) % 10 = 8 THEN 'val'
                    ELSE 'test'
                END AS split
            FROM events_prefix
            WHERE step >= 2  -- step=1 是初学，与 maimemo 对齐不进 prefix_events
        )
        TO '{prefix_path.as_posix()}' (FORMAT PARQUET, COMPRESSION ZSTD)
        """
    )

    # 5) 释放中间表内存
    conn.execute("DROP TABLE IF EXISTS events_prefix")
    conn.execute("DROP TABLE IF EXISTS events_seq")
    conn.execute("DROP TABLE IF EXISTS raw_events")
    conn.execute("CHECKPOINT")

    # 6) sequence_groups: 按 (u, w) GROUP BY，输出 list 列
    print("[duolingo_hlr] 输出 sequence_groups.parquet...")
    conn.execute(
        f"""
        COPY (
            SELECT
                u,
                w,
                ANY_VALUE(split) AS split,
                MAX(difficulty) AS difficulty,
                ANY_VALUE(user_bucket_10)  AS user_bucket_10,
                ANY_VALUE(user_bucket_100) AS user_bucket_100,
                LIST(step      ORDER BY step) AS steps,
                LIST("offset"  ORDER BY step) AS offsets,
                LIST(next_t    ORDER BY step) AS next_t_values,
                LIST(next_r    ORDER BY step) AS next_r_values,
                LIST(r_history ORDER BY step) AS r_history_values,
                LIST(t_history ORDER BY step) AS t_history_values
            FROM read_parquet('{prefix_path.as_posix()}')
            GROUP BY u, w
        )
        TO '{sequence_path.as_posix()}' (FORMAT PARQUET, COMPRESSION ZSTD)
        """
    )

    # 统计
    n_prefix = conn.execute(
        f"SELECT COUNT(*) FROM read_parquet('{prefix_path.as_posix()}')"
    ).fetchone()[0]
    n_seq = conn.execute(
        f"SELECT COUNT(*) FROM read_parquet('{sequence_path.as_posix()}')"
    ).fetchone()[0]
    n_users = conn.execute(
        f"SELECT COUNT(DISTINCT u) FROM read_parquet('{prefix_path.as_posix()}')"
    ).fetchone()[0]
    n_words = conn.execute(
        f"SELECT COUNT(DISTINCT w) FROM read_parquet('{prefix_path.as_posix()}')"
    ).fetchone()[0]
    nan_count = conn.execute(
        f"""
        SELECT
            SUM(CASE WHEN u IS NULL THEN 1 ELSE 0 END) +
            SUM(CASE WHEN w IS NULL THEN 1 ELSE 0 END) +
            SUM(CASE WHEN step IS NULL THEN 1 ELSE 0 END) +
            SUM(CASE WHEN next_t IS NULL THEN 1 ELSE 0 END) +
            SUM(CASE WHEN next_r IS NULL THEN 1 ELSE 0 END) +
            SUM(CASE WHEN r_history IS NULL THEN 1 ELSE 0 END) +
            SUM(CASE WHEN t_history IS NULL THEN 1 ELSE 0 END) +
            SUM(CASE WHEN split IS NULL THEN 1 ELSE 0 END)
        FROM read_parquet('{prefix_path.as_posix()}')
        """
    ).fetchone()[0]

    conn.close()

    summary = {
        "rawRows": n_raw,
        "prefixRows": n_prefix,
        "sequenceRows": n_seq,
        "uniqueUsers": n_users,
        "uniqueWords": n_words,
        "criticalNanCount": nan_count or 0,
        "prefixPath": str(prefix_path),
        "sequencePath": str(sequence_path),
    }
    return summary


def main() -> int:
    csv_path = download_dataset()
    summary = build_parquets(csv_path)
    print("\n[duolingo_hlr] 完成")
    for k, v in summary.items():
        print(f"  {k}: {v}")

    # 验收门禁
    if summary["prefixRows"] < 1_000_000:
        print(f"\n[duolingo_hlr] 验收失败: prefix_events 行数 {summary['prefixRows']} < 1M", file=sys.stderr)
        return 2
    if summary["criticalNanCount"] > 0:
        print(f"\n[duolingo_hlr] 验收失败: 关键字段有 {summary['criticalNanCount']} 个 NaN", file=sys.stderr)
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
