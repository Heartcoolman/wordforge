from __future__ import annotations

import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict


DATASET_DOI = "10.7910/DVN/VAGUL0"
DATASET_API_URL = (
    "https://dataverse.harvard.edu/api/datasets/:persistentId"
    f"?persistentId=doi:{DATASET_DOI}"
)
DEFAULT_BENCH_ROOT_ENV = "WORD_FORGE_BENCH_DATA"
DEFAULT_BENCH_ROOT = Path.home() / ".wordforge-bench" / "maimemo"
DEFAULT_RANDOM_SEED = 42
RETENTION_TARGETS = (0.80, 0.85, 0.90)

DATASET_LABELS = {
    "raw": "opensource_dataset_raw.tsv.7z",
    "difficulty": "opensource_dataset_difficulty.tsv.7z",
    "offset": "opensource_dataset_offset.tsv.7z",
    "forgetting_curve": "opensource_dataset_forgetting_curve.tsv.7z",
}

STATIC_FILE_MANIFEST = {
    "raw": {
        "label": "opensource_dataset_raw.tsv.7z",
        "id": 5857523,
        "md5": "fc1104daa1337e26e8199fd2ef3efb26",
        "filesize": 1708611032,
        "download_url": "https://dataverse.harvard.edu/api/access/datafile/5857523",
    },
    "difficulty": {
        "label": "opensource_dataset_difficulty.tsv.7z",
        "id": 5857524,
        "md5": "1f596e62064934760e06ba040c3d1756",
        "filesize": 126353,
        "download_url": "https://dataverse.harvard.edu/api/access/datafile/5857524",
    },
    "forgetting_curve": {
        "label": "opensource_dataset_forgetting_curve.tsv.7z",
        "id": 5857525,
        "md5": "d61fda6efe351cdd9d240d3adbb00438",
        "filesize": 593244,
        "download_url": "https://dataverse.harvard.edu/api/access/datafile/5857525",
    },
    "offset": {
        "label": "opensource_dataset_offset.tsv.7z",
        "id": 12025698,
        "md5": "78272644b479ac12aca5d41ca1142cc8",
        "filesize": 836192393,
        "download_url": "https://dataverse.harvard.edu/api/access/datafile/12025698",
    },
}

EXTRACTED_FILENAMES = {
    "raw": "raw.tsv",
    "difficulty": "difficulty.tsv",
    "offset": "offset.tsv",
    "forgetting_curve": "forgetting_curve.tsv",
}

DEFAULT_MEMORY_MODEL_CONFIG: Dict[str, Any] = {
    "shortTermLearningRate": 0.85,
    "mediumTermLearningRate": 0.30,
    "longTermLearningRate": 0.12,
    "compositeWeightShort": 0.20,
    "compositeWeightMedium": 0.30,
    "compositeWeightLong": 0.50,
    "consolidationRateScale": 0.25,
    "consolidationBonus": 1.5,
    "masteryCompositeThreshold": 0.30,
    "masteryAccuracyThreshold": 0.65,
    "masteryStreakThreshold": 1,
    "reviewingThreshold": 0.4,
    "halfLifeBaseEpsilon": 0.3,
    "halfLifeTimeUnitSecs": 1296000.0,
    "halfLifePower": 1.5,
    "recallRiskBonus": 0.2,
    "recallRiskThreshold": 0.55,
    "baseDesiredRetention": 0.92,
    "passiveDecayHalfLifeDays": 30.0,
    "passiveDecayPower": 0.30,
    "masteryWindowSize": 20,
    "stabilityBaseDays": 20.0,
    # FSRS-6：曲线参数由 w[20] 派生（这里写派生值，兜底未升级的消费方）
    "forgettingCurveFactor": 0.9 ** (-1.0 / 0.2710885682127271) - 1.0,
    "forgettingCurveDecay": -0.2710885682127271,
    "forgettingCurveFloor": 0.0,
    # FSRS-6 21 维（w[19]=同日饱和, w[20]=曲线 decay）。
    # 2026-06-10 DHP 约束调参胜者：w[0,2,8,9,10,20] 调优（三条 0.9x 守门腿全过，
    # test split +27.8%），其余维 FSRS-6 公版 —— 与生产 amas_config.toml 同步。
    "w": [
        0.09198069520607176,
        1.2931,
        2.5731565730123,
        8.2956,
        6.4133,
        0.8334,
        3.0194,
        0.001,
        1.6065116959233467,
        0.2455277063490539,
        1.1281524448213665,
        1.4835,
        0.0614,
        0.2629,
        1.6483,
        0.6014,
        1.8729,
        0.5425,
        0.0912,
        0.0658,
        0.2710885682127271,
    ],
}

# fsrs 基线条目专用：保持 FSRS-5 语义（19 维官方 w + 固定曲线），不随 AMAS 升级漂移
FSRS_BASELINE_CONFIG: Dict[str, Any] = {
    **DEFAULT_MEMORY_MODEL_CONFIG,
    "baseDesiredRetention": 0.90,
    "forgettingCurveFactor": 19.0 / 81.0,
    "forgettingCurveDecay": -0.5,
    "forgettingCurveFloor": 0.0,
    "w": [
        0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046,
        1.54575, 0.1192, 1.01925, 1.9395, 0.11, 0.29605, 2.2698, 0.2315,
        2.9898, 0.51655, 0.6621,
    ],
}


@dataclass(frozen=True)
class BenchPaths:
    root: Path
    archives: Path
    extracted: Path
    parquet: Path
    metadata: Path
    artifacts: Path
    reports: Path
    cache: Path

    @classmethod
    def from_root(cls, root: Path) -> "BenchPaths":
        return cls(
            root=root,
            archives=root / "archives",
            extracted=root / "extracted",
            parquet=root / "parquet",
            metadata=root / "metadata",
            artifacts=root / "artifacts",
            reports=root / "reports",
            cache=root / "cache",
        )

    def ensure(self) -> "BenchPaths":
        for path in (
            self.root,
            self.archives,
            self.extracted,
            self.parquet,
            self.metadata,
            self.artifacts,
            self.reports,
            self.cache,
        ):
            path.mkdir(parents=True, exist_ok=True)
        return self


def resolve_bench_root(root: str | None = None) -> BenchPaths:
    if root:
        return BenchPaths.from_root(Path(root).expanduser().resolve()).ensure()
    env_root = os.environ.get(DEFAULT_BENCH_ROOT_ENV)
    if env_root:
        return BenchPaths.from_root(Path(env_root).expanduser().resolve()).ensure()
    return BenchPaths.from_root(DEFAULT_BENCH_ROOT).ensure()


def load_memory_config(config_path: str | None = None) -> Dict[str, Any]:
    if not config_path:
        return json.loads(json.dumps(DEFAULT_MEMORY_MODEL_CONFIG))
    with Path(config_path).expanduser().open("r", encoding="utf-8") as handle:
        loaded = json.load(handle)
    return loaded.get("memoryModel", loaded)
