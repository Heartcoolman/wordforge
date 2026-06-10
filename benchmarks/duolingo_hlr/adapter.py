"""Duolingo HLR 数据集复用 maimemo 的 AdapterServer（同 wordforge MDM 接口）。"""
from benchmarks.maimemo.adapter import *  # noqa: F401,F403
from benchmarks.maimemo.adapter import (  # noqa: F401  显式 re-export
    ADAPTER_BINARY,
    REPO_ROOT,
    AdapterServer,
    ensure_adapter_binary,
    score_histories,
)
