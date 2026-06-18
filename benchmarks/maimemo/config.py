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
    # 2026-06-13 v5 GSP 船值：F1 冠军 desired_retention（val Borda 30，三数据集 rank 1）
    "baseDesiredRetention": 0.85,
    "passiveDecayHalfLifeDays": 30.0,
    "passiveDecayPower": 0.30,
    "masteryWindowSize": 20,
    "streakMinGapMs": 1800000,
    "stabilityBaseDays": 20.0,
    # 2026-06-13 v5 GSP 船值：alpha 平滑钉死 alpha_eff=1（未平滑 FSRS-6 核心），双腿 ramp no-op
    "alphaScale": 1.0,
    "alphaMin": 1.0,
    "alphaMax": 1.0,
    # 双腿信任调度：2026-06-13 v5 GSP 船值关闭（alpha 已钉死 1.0，ramp 本为 no-op）
    "alphaRampTau": 0.0,
    "alphaLapseRampTau": 0.0,
    # FSRS-6：曲线参数由 w[20] 派生（这里写派生值，兜底未升级的消费方）
    "forgettingCurveFactor": 0.9 ** (-1.0 / 0.1542) - 1.0,
    "forgettingCurveDecay": -0.1542,
    "forgettingCurveFloor": 0.0,
    # FSRS-6 21 维（w[19]=同日饱和, w[20]=曲线 decay）。
    # 2026-06-13 v5 GSP 船值：回归 FSRS-6 公版 21 维 w（py-fsrs DEFAULT_PARAMETERS）。
    # 公版 w 与 gspSuccessGrade=4（Easy）共拟合 → 预测腿逐位 = amas6（G3 满余量），
    # 调度增益由 GSP 策略头承载（见 GSP_SPEC §3.5）—— 与生产 amas_config.toml 同步。
    "w": [
        0.212,
        1.2931,
        2.3065,
        8.2956,
        6.4133,
        0.8334,
        3.0194,
        0.001,
        1.8722,
        0.1666,
        0.796,
        1.4835,
        0.0614,
        0.2629,
        1.6483,
        0.6014,
        1.8729,
        0.5425,
        0.0912,
        0.0658,
        0.1542,
    ],
    # === GSP 调度策略头 F1 船值（契约 benchmarks/maimemo/GSP_SPEC.md）===
    # amas board 条目消费 DEFAULT；fsrs 已 rebind FSRS_BASELINE 故不泄漏。
    "gspSuccessGrade": 4,
    "gspIntervalCapDays": 40.0,
    "gspGraduationStreak": 2,
    "gspGraduationFloorDays": 30.0,
    "gspYoungRetention": 0.86,
    "gspMatureRetention": 0.92,
    "gspMaturityBandDays": 14.0,
    "gspIntervalFuzz": 0.0,
    # === per-word difficulty logit 加性项（v6 预测层船值，amas 专属）===
    # 预测期 recall 在 logit 域加 β·(REF - external_difficulty)：难词降 p、易词升 p，
    # 补 FSRS 二元映射下「预测随难度扁平」的区分度/校准残差。β=0.1 由 maimemo+duolingo val
    # 选型、test 多 seed(42/123/777) 验证：vs amas6/fsrs6 logLoss-0.005~-0.012、AUC+0.0005~+0.015，
    # 且在 maimemo AUC 反超 dhp(墨墨)。仅作用于 AMASScheduler._recall（预测+urgency），不入 S/D 动力学。
    # 竞品（fsrs/fsrs6/amas6）忽略此键 → 天然 de-tie。详见 pred_diagnose/pred_search/pred_test_eval。
    "difficultyLogitWeight": 0.1,
    "difficultyLogitRef": 5.0,
    # === 冷启动难度先验（Phase 1a；仅首评 review_count==0 调整 S₀/D₀）===
    # 6 个权重默认 0.0 = 关闭（与 Rust MemoryModelConfig 同名字段 / dhp_reference 镜像逐位一致）。
    # 调高任一权重即可在 benchmark 评测冷启动先验；启用前须 prod_replay + 真实 A/B。
    "coldStartLenRef": 7.0,
    "coldStartLenScale": 4.0,
    "coldStartExtdRef": 5.0,
    "coldStartDLenWeight": 0.0,
    "coldStartDMorphWeight": 0.0,
    "coldStartDExtdWeight": 0.0,
    "coldStartSLenWeight": 0.0,
    "coldStartSMorphWeight": 0.0,
    "coldStartSExtdWeight": 0.0,
}

# fsrs 基线条目专用：保持 FSRS-5 语义（19 维官方 w + 固定曲线），不随 AMAS 升级漂移
FSRS_BASELINE_CONFIG: Dict[str, Any] = {
    **DEFAULT_MEMORY_MODEL_CONFIG,
    "baseDesiredRetention": 0.90,
    "forgettingCurveFactor": 19.0 / 81.0,
    "forgettingCurveDecay": -0.5,
    "forgettingCurveFloor": 0.0,
    # 竞品隔离：显式关闭双腿信任调度，DEFAULT 即便日后调参写回也不随动
    "alphaRampTau": 0.0,
    "alphaLapseRampTau": 0.0,
    # 竞品隔离：显式关闭 GSP 调度策略头（成功成绩带/区间帽/毕业下限/成熟度分带保持率/抖动），
    # 即便 amas 候选写回 DEFAULT（含 gspSuccessGrade=4、cap40 等 F1 船值），fsrs 基线也固定为
    # FSRS-5 公版调度语义不随动（gspSuccessGrade=3=Good 旧默认；其余 gsp*=0=关闭）。
    "gspSuccessGrade": 3,
    "gspIntervalCapDays": 0.0,
    "gspGraduationStreak": 0,
    "gspGraduationFloorDays": 30.0,
    "gspYoungRetention": 0.0,
    "gspMatureRetention": 0.0,
    "gspMaturityBandDays": 0.0,
    "gspIntervalFuzz": 0.0,
    # alphaMin/alphaMax/alphaScale 也须显式恢复旧合法值：DEFAULT 已写回 1.0/1.0/1.0（v5 钉死），
    # 但 FSRS-5 公版调度需 0.3/0.1/0.5（连击动态 alpha 区间）。双腿信任已在上方显式归零。
    "alphaScale": 0.3,
    "alphaMin": 0.1,
    "alphaMax": 0.5,
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
