"""configs.py — OLD vs NEW memory-model configs for the production-log replay audit.

OLD_MEMORY_MODEL_CONFIG: the pre-campaign ship `[memoryModel]` table, transcribed
verbatim from `git show 4091e9a^:amas_config.toml` (4091e9a = "GSP 毕业制调度头生产化
——F1 船值落地"). No `gsp*` keys exist in that file: serde defaults keep every GSP
knob inert (gspSuccessGrade=3, cap/streak/band/fuzz all off), so the bench mirrors
reproduce the legacy compute_interval path bit-exactly (success_grade defaults to 3
in dhp_reference._gsp_band_kwargs_from_config — do NOT add gsp keys here).

NEW_MEMORY_MODEL_CONFIG: the frozen F1 ship values = DEFAULT_MEMORY_MODEL_CONFIG
(benchmarks/maimemo/config.py). Imported, not copied, so it can never drift.
"""
from __future__ import annotations

import sys
from pathlib import Path
from typing import Any, Dict

_REPO_ROOT = Path(__file__).resolve().parents[2]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from benchmarks.maimemo.config import DEFAULT_MEMORY_MODEL_CONFIG  # noqa: E402

# `git show 4091e9a^:amas_config.toml` [memoryModel], full table, values verbatim.
OLD_MEMORY_MODEL_CONFIG: Dict[str, Any] = {
    "shortTermLearningRate": 0.85,
    "mediumTermLearningRate": 0.3,
    "longTermLearningRate": 0.12,
    "compositeWeightShort": 0.2,
    "compositeWeightMedium": 0.3,
    "compositeWeightLong": 0.5,
    "consolidationRateScale": 0.25,
    "consolidationBonus": 1.5,
    "masteryCompositeThreshold": 0.3,
    "masteryAccuracyThreshold": 0.65,
    "masteryStreakThreshold": 1,
    "reviewingThreshold": 0.4,
    "halfLifeBaseEpsilon": 0.3,
    "halfLifeTimeUnitSecs": 1296000.0,
    "halfLifePower": 1.5,
    "recallRiskBonus": 0.2,
    "recallRiskThreshold": 0.55,
    "baseDesiredRetention": 0.9,
    "passiveDecayHalfLifeDays": 30.0,
    "passiveDecayPower": 0.3,
    "masteryWindowSize": 20,
    "streakMinGapMs": 1800000,
    "stabilityBaseDays": 20.0,
    # FSRS-6: curve factor/decay derived from w[20] (forgettingCurveFactor/Decay
    # deprecated in the old file; only the floor was written)
    "forgettingCurveFloor": 0.0,
    # FSRS-6 21-dim NM0 tuned w (w[19]=same-day saturation, w[20]=curve decay)
    "w": [
        0.07978223679403902,
        1.2931,
        3.02498794640543,
        8.2956,
        6.4133,
        0.8334,
        3.0194,
        0.001,
        1.6259347829476551,
        0.2533678250394599,
        1.1124862163489901,
        1.4835,
        0.0614,
        0.2629,
        1.6483,
        0.6014,
        1.8729,
        0.5425,
        0.0912,
        0.0658,
        0.2632510019126722,
    ],
    "alphaScale": 0.3,
    "alphaMin": 0.1,
    "alphaMax": 0.5,
    # D5 dual-leg trust write-back, 2026-06-12
    "alphaRampTau": 3.0,
    "alphaLapseRampTau": 6.0,
    "forgettingThreshold": 0.2,
    "retentionMin": 0.75,
    "retentionMax": 0.95,
    "maxIntervalDays": 90.0,
    "minIntervalSecs": 60,
    "highAccuracyThreshold": 0.9,
    "highAccuracyRetentionBoost": 0.03,
    "highFatigueThreshold": 0.6,
    "highFatigueRetentionDrop": 0.05,
    "lowMotivationThreshold": -0.2,
    "lowMotivationRetentionDrop": 0.03,
}

NEW_MEMORY_MODEL_CONFIG: Dict[str, Any] = DEFAULT_MEMORY_MODEL_CONFIG
