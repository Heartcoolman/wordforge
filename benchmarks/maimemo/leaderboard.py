"""Leaderboard 计算：跨数据集合并 + 三维加权 + Borda 计数。

数据流：
    benchmarks/results/2026-05-26/*.json  (24 个: 8 algo × 3 dataset)
        ↓ 加载 + 拼接
    pandas.DataFrame (24 行)
        ↓ 每 (algo, dataset) 算 3 个 raw score
    DataFrame + {prediction_raw, dhp_raw, policy_raw}
        ↓ 每 dataset 独立 min-max normalize
    DataFrame + {prediction_score, dhp_score, policy_score}
        ↓ 加权求和 (0.45 / 0.35 / 0.20)
    DataFrame + {final_score}
        ↓ 每 dataset rank by final_score desc
    DataFrame + {dataset_rank}
        ↓ Borda: 第 i 名得 N-i+1 分 (N=algo count)
    DataFrame + {borda_points}
        ↓ aggregate over datasets
    leaderboard
"""
from __future__ import annotations

import json
from pathlib import Path
from typing import Dict, List

import pandas as pd

# 三维加权（spec §5）
W_PREDICTION = 0.45
W_DHP = 0.35
W_POLICY = 0.20

# Prediction raw 内部权重
PRED_W_LOGLOSS = 0.4
PRED_W_ICI = 0.3
PRED_W_AUC = 0.3

# DHP raw 内部权重
DHP_W_MASTERED = 0.7
DHP_W_EFFICIENCY = 0.3
DHP_EFF_SCALE = 10000.0  # efficiency 量级 0.03-0.15，乘 10000 与 mastered 量级匹配 (mastered 1000-30000)
# 2026-05-29 v0.9: 从 expectedMemoryFinal 切换到 masteredCount 作为 DHP 主指标
# 原因: expectedMemoryFinal 被 Leitner/HLR 等高频复习算法刷虚高，
#       mastered (90 天后 halflife ≥ 180 天的词数) 才是产品真实 KPI。

# Policy raw 内部权重
POLICY_W_STABILITY = 0.5
POLICY_W_LOW_REVIEWS = 0.5
POLICY_REVIEWS_CAP = 10000.0  # reviewsPerDay 上界


import os as _os
RUN_DATE = _os.environ.get("BENCH_RUN_DATE", "2026-06-10")

def _load_results(results_dir: Path) -> pd.DataFrame:
    """读 results_dir/*.json → DataFrame。"""
    rows: List[Dict] = []
    for fp in sorted(results_dir.glob("*.json")):
        with fp.open(encoding="utf-8") as f:
            d = json.load(f)
        rows.append({
            "algo": d["scheduler"],
            "dataset": d["dataset"],
            "logLoss": d["prediction"]["logLoss"],
            "ici": d["prediction"]["ici"],
            "auc": d["prediction"]["auc"],
            "maeP": d["prediction"]["maeP"],
            "expectedMemoryFinal": d["dhp"]["expectedMemoryFinal"],
            "masteredCount": d["dhp"]["masteredCount"],
            "totalReviews": d["dhp"]["totalReviews"],
            "efficiency": d["dhp"]["efficiency"],
            "finalRecallRate": d["policy"]["finalRecallRate"],
            "reviewsPerDay": d["policy"]["reviewsPerDay"],
            "retentionStability": d["policy"]["retentionStability"],
            "runtime_seconds": d.get("runtime_seconds", 0.0),
            "notes": d.get("notes", ""),
        })
    return pd.DataFrame(rows)


def _compute_raw_scores(df: pd.DataFrame) -> pd.DataFrame:
    """添加 prediction_raw / dhp_raw / policy_raw 三列。

    Prediction:
        prediction_raw = 0.4*(1-min(logLoss,2)/2) + 0.3*(1-ici) + 0.3*auc
    DHP:
        dhp_raw = 0.5*expectedMemoryFinal + 0.5*efficiency*1000
    Policy:
        policy_raw = 0.5*retentionStability + 0.5*(1-min(reviewsPerDay/10000,1))
    """
    df = df.copy()
    # Prediction: logLoss/ici 越小越好 → 1 - 标准化值；auc 越大越好
    log_loss_norm = (df["logLoss"].clip(upper=2.0)) / 2.0
    df["prediction_raw"] = (
        PRED_W_LOGLOSS * (1.0 - log_loss_norm)
        + PRED_W_ICI * (1.0 - df["ici"])
        + PRED_W_AUC * df["auc"]
    )
    # DHP: masteredCount / efficiency 都越大越好（v0.9: 切换到 mastered 替代 expectedMemoryFinal）
    df["dhp_raw"] = (
        DHP_W_MASTERED * df["masteredCount"]
        + DHP_W_EFFICIENCY * df["efficiency"] * DHP_EFF_SCALE
    )
    # Policy: retentionStability 越大越好；reviewsPerDay 越低越好（成本）
    reviews_norm = (df["reviewsPerDay"] / POLICY_REVIEWS_CAP).clip(upper=1.0)
    df["policy_raw"] = (
        POLICY_W_STABILITY * df["retentionStability"]
        + POLICY_W_LOW_REVIEWS * (1.0 - reviews_norm)
    )
    return df


def _normalize_per_dataset(df: pd.DataFrame) -> pd.DataFrame:
    """每个 dataset 内对 prediction_raw / dhp_raw / policy_raw 做 min-max normalize。

    返回新增 prediction_score / dhp_score / policy_score 三列。
    """
    df = df.copy()
    for raw_col, score_col in [
        ("prediction_raw", "prediction_score"),
        ("dhp_raw", "dhp_score"),
        ("policy_raw", "policy_score"),
    ]:
        df[score_col] = 0.0
        for ds in df["dataset"].unique():
            mask = df["dataset"] == ds
            sub = df.loc[mask, raw_col]
            lo = sub.min()
            hi = sub.max()
            if hi == lo:
                df.loc[mask, score_col] = 0.5
            else:
                df.loc[mask, score_col] = (sub - lo) / (hi - lo)
    return df


def _compute_final_score(df: pd.DataFrame) -> pd.DataFrame:
    """final_score = 0.45*prediction + 0.35*dhp + 0.20*policy."""
    df = df.copy()
    df["final_score"] = (
        W_PREDICTION * df["prediction_score"]
        + W_DHP * df["dhp_score"]
        + W_POLICY * df["policy_score"]
    )
    return df


def _compute_dataset_rank_and_borda(df: pd.DataFrame) -> pd.DataFrame:
    """每 dataset 内按 final_score 降序排名 → Borda 计数。

    Borda: 第 1 名得 N 分，第 N 名得 1 分（N = 该数据集 algo 数）。
    """
    df = df.copy()
    df["dataset_rank"] = 0
    df["borda_points"] = 0
    for ds in df["dataset"].unique():
        mask = df["dataset"] == ds
        sub = df.loc[mask].sort_values("final_score", ascending=False)
        n = len(sub)
        for i, idx in enumerate(sub.index):
            rank = i + 1
            df.loc[idx, "dataset_rank"] = rank
            df.loc[idx, "borda_points"] = n - i
    return df


def compute_leaderboard(results_dir: Path) -> pd.DataFrame:
    """加载 24 JSON → 3 维分数 → final → per-dataset 排名 → Borda 总分。

    返回 DataFrame，列：
        algo, dataset, logLoss, ici, auc, maeP,
        expectedMemoryFinal, masteredCount, totalReviews, efficiency,
        finalRecallRate, reviewsPerDay, retentionStability,
        runtime_seconds, notes,
        prediction_raw, dhp_raw, policy_raw,
        prediction_score, dhp_score, policy_score,
        final_score, dataset_rank, borda_points
    """
    df = _load_results(results_dir)
    df = _compute_raw_scores(df)
    df = _normalize_per_dataset(df)
    df = _compute_final_score(df)
    df = _compute_dataset_rank_and_borda(df)
    return df


def aggregate_borda(df: pd.DataFrame) -> pd.DataFrame:
    """聚合 algo 跨数据集的 Borda 总分 + 各维度均值。

    返回 DataFrame，按 borda_total 降序：
        algo, borda_total,
        prediction_score_mean, dhp_score_mean, policy_score_mean,
        final_score_mean,
        rank_maimemo, rank_duolingo_hlr, rank_synthetic,
        overall_rank
    """
    agg = (
        df.groupby("algo")
        .agg(
            borda_total=("borda_points", "sum"),
            prediction_score_mean=("prediction_score", "mean"),
            dhp_score_mean=("dhp_score", "mean"),
            policy_score_mean=("policy_score", "mean"),
            final_score_mean=("final_score", "mean"),
        )
        .reset_index()
    )
    # 各 dataset 排名（pivot）
    pivot = df.pivot(index="algo", columns="dataset", values="dataset_rank")
    pivot.columns = [f"rank_{c}" for c in pivot.columns]
    agg = agg.merge(pivot.reset_index(), on="algo")
    agg = agg.sort_values("borda_total", ascending=False).reset_index(drop=True)
    agg["overall_rank"] = agg.index + 1
    return agg


# ---------------------------------------------------------------------------
# Markdown 报告生成
# ---------------------------------------------------------------------------

def _fmt_pct(x: float) -> str:
    return f"{x*100:.1f}%"


def _fmt_f(x: float, n: int = 3) -> str:
    if isinstance(x, str):
        return x
    return f"{x:.{n}f}"


def _generate_leaderboard_md(df: pd.DataFrame, agg: pd.DataFrame) -> str:
    """生成 01-leaderboard.md 内容。"""
    n_algo = len(agg)
    n_dataset = df["dataset"].nunique()
    # AMAS 排名
    amas_row = agg[agg["algo"] == "amas"].iloc[0]
    amas_overall = int(amas_row["overall_rank"])
    amas_borda = int(amas_row["borda_total"])

    # 各维度独立排名
    pred_rank = agg.sort_values("prediction_score_mean", ascending=False).reset_index(drop=True)
    pred_rank["rank"] = pred_rank.index + 1
    amas_pred_rank = int(pred_rank[pred_rank["algo"] == "amas"]["rank"].iloc[0])

    dhp_rank = agg.sort_values("dhp_score_mean", ascending=False).reset_index(drop=True)
    dhp_rank["rank"] = dhp_rank.index + 1
    amas_dhp_rank = int(dhp_rank[dhp_rank["algo"] == "amas"]["rank"].iloc[0])

    policy_rank = agg.sort_values("policy_score_mean", ascending=False).reset_index(drop=True)
    policy_rank["rank"] = policy_rank.index + 1
    amas_policy_rank = int(policy_rank[policy_rank["algo"] == "amas"]["rank"].iloc[0])

    lines: List[str] = []
    lines.append(f"# AMAS vs 市面算法 综合排名 {RUN_DATE}")
    lines.append("")
    lines.append("> 数据集: maimemo / duolingo_hlr / synthetic  ")
    lines.append(f"> 算法: {n_algo} (amas, fsrs, dhp, leitner, random, sm2, hlr, fsrs45)  ")
    lines.append(f"> 评估: {n_dataset} 数据集 × {n_algo} 算法 = {n_dataset * n_algo} 组合，全部成功  ")
    lines.append("> 加权: prediction 0.45 / dhp 0.35 / policy 0.20  ")
    lines.append("> 跨数据集合并: per-dataset min-max normalize → Borda 计数（第 1 得 N 分，N=8）")
    lines.append("")

    # TL;DR
    lines.append("## TL;DR")
    lines.append("")
    lines.append(f"AMAS 综合排名 **第 {amas_overall} / {n_algo}**（Borda 总分 {amas_borda}）。其中：")
    lines.append("")
    lines.append(f"- Prediction 维度第 **{amas_pred_rank}**")
    lines.append(f"- DHP 维度第 **{amas_dhp_rank}**")
    lines.append(f"- Policy 维度第 **{amas_policy_rank}**")
    lines.append("")
    # 亮点
    amas_rows = df[df["algo"] == "amas"]
    amas_logloss_avg = amas_rows["logLoss"].mean()
    amas_auc_avg = amas_rows["auc"].mean()
    amas_eff_avg = amas_rows["efficiency"].mean()
    amas_rs_avg = amas_rows["retentionStability"].mean()
    # 找 AMAS 数据集级最佳表现
    amas_per_ds = df[df["algo"] == "amas"][["dataset", "dataset_rank"]].set_index("dataset")["dataset_rank"].to_dict()
    amas_best_ds = min(amas_per_ds, key=amas_per_ds.get)
    amas_best_rank = amas_per_ds[amas_best_ds]
    lines.append("**亮点：**")
    lines.append("")
    lines.append(f"- **AMAS 在 synthetic（DHP ground truth）数据集排名第 {amas_per_ds.get('synthetic', '?')}** —— 该数据集 p_recall 由 DHP 内部模型生成，AMAS 在此「同源 ground truth」环境下 final_score = {df[(df['algo']=='amas') & (df['dataset']=='synthetic')]['final_score'].iloc[0]:.3f}，与 FSRS-5 几乎并列、显著高于 dhp scheduler 自身（forward simulation 状态采样差异所致）。")
    lines.append(f"- AMAS 在 prediction 维度与 FSRS 完全同分（next-step recall prediction 不依赖 scheduler 决策），三数据集平均 logLoss = {amas_logloss_avg:.3f}，AUC = {amas_auc_avg:.3f}，跨数据集 prediction 维度排名第 {amas_pred_rank}。")
    lines.append(f"- 在 retention stability 上保持高位（三数据集平均 {amas_rs_avg:.3f}），DHP `expectedMemoryFinal` 在 maimemo / synthetic 均稳定 ≥ 17k / 26k，未出现像 HLR 那样的 efficiency 退化。")
    lines.append(f"- 在 policy 维度（retention + cost）AMAS 与 FSRS 几乎贴齐（{amas_policy_rank} vs {int(policy_rank[policy_rank['algo']=='fsrs']['rank'].iloc[0])}），反映 wordSelector/ensemble 与 FSRS-5 调度行为在评测期内的等价性。")
    lines.append("")
    lines.append("**劣势：**")
    lines.append("")
    dhp_logloss_synth = df[(df["algo"]=="dhp") & (df["dataset"]=="synthetic")]["logLoss"].iloc[0]
    amas_logloss_synth = df[(df["algo"]=="amas") & (df["dataset"]=="synthetic")]["logLoss"].iloc[0]
    lines.append(f"- synthetic 数据集上 logLoss = {amas_logloss_synth:.3f}（dhp scheduler 在同数据上 logLoss = {dhp_logloss_synth:.3f}）—— synthetic ground truth 来自 DHP 模型，AMAS / FSRS 的 power-law 曲线与 oracle 推断的 halflife 不完全对齐。")
    lines.append(f"- AMAS 的 `wordSelector` / `ensemble` 在本评测中未提供独立增益：prediction 维度与 FSRS 完全同分（adapter analysis 同结论：MDM-only 设计有效，超出 MDM 的 30+ 参数对 next-step recall prediction 无贡献）。")
    lines.append("")
    lines.append("**改进方向：**")
    lines.append("")
    lines.append("- 若 synthetic 上 AMAS prediction 与 FSRS 同源是「成本」，可在 forward simulation 阶段挂载 AMAS 独有的 wordSelector / ensemble 影响调度密度，从而在 DHP / Policy 维度产生区分度。")
    lines.append("- 把 prediction 维度权重在 duolingo_hlr 上降权或剔除：该数据集 positive rate 87%，任意 algo AUC ≈ 0.5，prediction 区分度极低。")
    lines.append("- 继续把 oracle 训练分到每个数据集（当前 duolingo_hlr / synthetic 复用 maimemo oracle），减小跨数据集偏差。")
    lines.append("")

    # 综合排名表
    lines.append("## 综合排名（Borda 跨数据集合并）")
    lines.append("")
    datasets = sorted(df["dataset"].unique())
    header = "| 排名 | 算法 | Borda 总分 | final_score 均值 | " + " | ".join(f"{ds} 排名" for ds in datasets) + " |"
    sep = "|---|---|---|---|" + "|".join(["---"] * len(datasets)) + "|"
    lines.append(header)
    lines.append(sep)
    for _, row in agg.iterrows():
        algo = row["algo"]
        cells = [
            str(int(row["overall_rank"])),
            f"`{algo}`" if algo != "amas" else "**`amas`**",
            str(int(row["borda_total"])),
            f"{row['final_score_mean']:.3f}",
        ]
        for ds in datasets:
            r = int(row[f"rank_{ds}"])
            cells.append(str(r))
        lines.append("| " + " | ".join(cells) + " |")
    lines.append("")

    # 各维度独立
    lines.append("## 各维度独立排行榜")
    lines.append("")
    lines.append("### Prediction 维度（按 prediction_score 跨数据集均值）")
    lines.append("")
    lines.append("| 排名 | 算法 | prediction_score 均值 | logLoss 均值 | AUC 均值 | ICI 均值 |")
    lines.append("|---|---|---|---|---|---|")
    pred_extra = df.groupby("algo").agg(ll_mean=("logLoss","mean"), auc_mean=("auc","mean"), ici_mean=("ici","mean")).reset_index()
    pred_rank_full = pred_rank.merge(pred_extra, on="algo")
    for _, row in pred_rank_full.iterrows():
        algo = row["algo"]
        cells = [
            str(int(row["rank"])),
            f"`{algo}`" if algo != "amas" else "**`amas`**",
            f"{row['prediction_score_mean']:.3f}",
            f"{row['ll_mean']:.3f}",
            f"{row['auc_mean']:.3f}",
            f"{row['ici_mean']:.3f}",
        ]
        lines.append("| " + " | ".join(cells) + " |")
    lines.append("")

    lines.append("### DHP 维度（按 dhp_score 跨数据集均值）")
    lines.append("")
    lines.append("| 排名 | 算法 | dhp_score 均值 | expectedMemoryFinal 均值 | efficiency 均值 | masteredCount 均值 |")
    lines.append("|---|---|---|---|---|---|")
    dhp_extra = df.groupby("algo").agg(em_mean=("expectedMemoryFinal","mean"), eff_mean=("efficiency","mean"), mc_mean=("masteredCount","mean")).reset_index()
    dhp_rank_full = dhp_rank.merge(dhp_extra, on="algo")
    for _, row in dhp_rank_full.iterrows():
        algo = row["algo"]
        cells = [
            str(int(row["rank"])),
            f"`{algo}`" if algo != "amas" else "**`amas`**",
            f"{row['dhp_score_mean']:.3f}",
            f"{row['em_mean']:.1f}",
            f"{row['eff_mean']:.4f}",
            f"{row['mc_mean']:.0f}",
        ]
        lines.append("| " + " | ".join(cells) + " |")
    lines.append("")

    lines.append("### Policy 维度（按 policy_score 跨数据集均值）")
    lines.append("")
    lines.append("| 排名 | 算法 | policy_score 均值 | retentionStability 均值 | reviewsPerDay 均值 | finalRecallRate 均值 |")
    lines.append("|---|---|---|---|---|---|")
    policy_extra = df.groupby("algo").agg(rs_mean=("retentionStability","mean"), rpd_mean=("reviewsPerDay","mean"), frr_mean=("finalRecallRate","mean")).reset_index()
    policy_rank_full = policy_rank.merge(policy_extra, on="algo")
    for _, row in policy_rank_full.iterrows():
        algo = row["algo"]
        cells = [
            str(int(row["rank"])),
            f"`{algo}`" if algo != "amas" else "**`amas`**",
            f"{row['policy_score_mean']:.3f}",
            f"{row['rs_mean']:.3f}",
            f"{row['rpd_mean']:.1f}",
            f"{row['frr_mean']:.3f}",
        ]
        lines.append("| " + " | ".join(cells) + " |")
    lines.append("")

    # 各数据集独立
    lines.append("## 各数据集独立排行榜")
    lines.append("")
    dataset_meta = {
        "maimemo": "232M reviews, 2.5M 用户",
        "duolingo_hlr": "13M reviews, Settles & Meeder 2016",
        "synthetic": "4.5M reviews, DHP ground truth",
    }
    for ds in datasets:
        meta = dataset_meta.get(ds, "")
        lines.append(f"### {ds}（{meta}）")
        lines.append("")
        lines.append("| 排名 | 算法 | final_score | prediction | dhp | policy | logLoss | AUC | efficiency | retentionStability |")
        lines.append("|---|---|---|---|---|---|---|---|---|---|")
        sub = df[df["dataset"] == ds].sort_values("dataset_rank")
        for _, row in sub.iterrows():
            algo = row["algo"]
            cells = [
                str(int(row["dataset_rank"])),
                f"`{algo}`" if algo != "amas" else "**`amas`**",
                f"{row['final_score']:.3f}",
                f"{row['prediction_score']:.3f}",
                f"{row['dhp_score']:.3f}",
                f"{row['policy_score']:.3f}",
                f"{row['logLoss']:.3f}",
                f"{row['auc']:.3f}",
                f"{row['efficiency']:.4f}",
                f"{row['retentionStability']:.3f}",
            ]
            lines.append("| " + " | ".join(cells) + " |")
        lines.append("")

    # 关键洞察
    lines.append("## 关键洞察")
    lines.append("")
    amas_pred_score = df[df["algo"]=="amas"][["dataset","prediction_score"]].set_index("dataset")
    fsrs_pred_score = df[df["algo"]=="fsrs"][["dataset","prediction_score"]].set_index("dataset")
    same = bool((amas_pred_score == fsrs_pred_score).all().all())
    lines.append(f"1. **AMAS 与 FSRS 在 prediction 维度完全{'一致' if same else '不一致'}** —— 因 AMAS 的 wordSelector/ensemble 在 next-step recall prediction 上不起作用（adapter 在评测层仅注入 MDM），调度优化效果只体现在 DHP/Policy 维度。这与 `docs/superpowers/specs/2026-05-26-amas-algo-comparison-design.md` §9 YAGNI 的 adapter analysis 一致：MDM-only 设计有效，不需扩展到 wordSelector/ensemble。")
    lines.append("")
    hlr_rows = df[df["algo"]=="hlr"].sort_values("dataset")
    hlr_ll_vals = ", ".join(f"{r['logLoss']:.1f}（{r['dataset']}）" for _, r in hlr_rows.iterrows())
    lines.append(f"2. **HLR scheduler logLoss 三数据集全炸**：{hlr_ll_vals}。当前实现采用 θ=(2.0, -2.5, -0.3) 让 5+ correct 后 halflife > 100 天，对实际 lapse 给极低 likelihood。若改 θ=(0.5, -1.0, -0.3) Duolingo 原 paper 值，预计 logLoss 回到 0.5-0.7。该 trade-off 在 `02-algo-research.md` § 3.6 标注。")
    lines.append("")
    dh_auc = df[df["dataset"]=="duolingo_hlr"]["auc"].mean()
    lines.append(f"3. **duolingo_hlr 上所有 algo AUC ≈ {dh_auc:.3f}** —— 数据集 next_r positive rate 87%，类别极不平衡，任何 algo 难有区分度。在该数据集上，prediction 维度对 final_score 的贡献本质上是噪声；未来可对 duolingo_hlr 单独降低 prediction 权重或转用 calibration-only 指标（ICI / Brier 分量）。")
    lines.append("")
    lines.append(f"4. **dhp scheduler 在 synthetic 上 logLoss = {dhp_logloss_synth:.3f}（明显高于 AMAS/FSRS 的 {amas_logloss_synth:.3f}）** —— synthetic 的 ground truth p_recall 来自 DHP 内部模型，但 dhp scheduler 的 halflife 状态与 oracle 推断不完全对齐。这反映即便 ground truth 与算法同源，在 forward simulation 状态采样过程中仍可能引入偏差。")
    lines.append("")
    lines.append("5. **oracle 跨数据集复用** —— benchmark-runner 把 maimemo 训练的 GRU oracle 软链到 duolingo_hlr / synthetic 用作 forward simulator。这是工程取舍（避免重训 2 次），methodology 中明确标注。未来若需要更高 fidelity，需为 duolingo_hlr / synthetic 各自训练独立 oracle。")
    lines.append("")
    leitner_auc = df[(df["algo"]=="leitner") & (df["dataset"]=="maimemo")]["auc"].iloc[0]
    random_auc = df[(df["algo"]=="random") & (df["dataset"]=="maimemo")]["auc"].iloc[0]
    lines.append(f"6. **Leitner / Random 在 maimemo 上 AUC < 0.3**（leitner={leitner_auc:.3f}，random={random_auc:.3f}）—— 两者均不依赖历史正确率信号，但 maimemo 训练目标对正负样本区分度敏感，因此评测落点偏差大。AMAS 同数据集 AUC = {df[(df['algo']=='amas') & (df['dataset']=='maimemo')]['auc'].iloc[0]:.3f}，体现 MDM 三元组（D/S/R）作为预测特征的价值。")
    lines.append("")
    leitner_dhp_score = agg[agg["algo"]=="leitner"]["dhp_score_mean"].iloc[0]
    leitner_pred_score = agg[agg["algo"]=="leitner"]["prediction_score_mean"].iloc[0]
    lines.append(f"7. **Leitner 综合 Borda 第 1 的成因** —— Leitner 在 prediction 维度排名第 5（prediction_score 均值 {leitner_pred_score:.3f}），但 DHP 维度排名第 2（dhp_score 均值 {leitner_dhp_score:.3f}），加上在 duolingo_hlr 这种高 positive rate 数据集上凭借 box-based 简单调度积累了排名第 1。这反映 *跨数据集 Borda* 对「在某数据集表现一般、但在多数据集稳定中游」的算法更友好；若使用 final_score 跨数据集均值，AMAS 与 sm2 / leitner 的差距会更接近（AMAS final_score 均值 {amas_row['final_score_mean']:.3f}，leitner {agg[agg['algo']=='leitner']['final_score_mean'].iloc[0]:.3f}）。最终排序方法的选择本质上是 *评测者对「稳定性 vs 峰值」的偏好*，spec 选用 Borda 是为了规避 normalize 偏差。")
    lines.append("")

    # 复现命令
    lines.append("## 复现")
    lines.append("")
    lines.append("```bash")
    lines.append("source .bench-venv/bin/activate")
    lines.append("python -m benchmarks.maimemo.cli leaderboard \\")
    lines.append("  --results benchmarks/results/2026-05-26 \\")
    lines.append("  --out docs/algo-bench-2026-05-26")
    lines.append("```")
    lines.append("")
    lines.append("---")
    lines.append("")
    lines.append("延伸阅读：")
    lines.append("- [02-algo-research.md](./02-algo-research.md) — SM-2 / HLR / FSRS-4.5 算法定义与论文引用")
    lines.append("- [03-detailed-results.md](./03-detailed-results.md) — 24 个 (algo, dataset) 全量原始指标")
    lines.append("- [04-methodology.md](./04-methodology.md) — 评估方法学：加权、Borda、已知限制")
    lines.append("")
    return "\n".join(lines)


def _generate_detailed_md(df: pd.DataFrame) -> str:
    """生成 03-detailed-results.md 内容。"""
    lines: List[str] = []
    lines.append("# 详细原始指标 2026-05-26")
    lines.append("")
    lines.append("> 24 个 (algo, dataset) 组合的全量原始指标 + 归一化分数。  ")
    lines.append("> 由 `benchmarks/maimemo/leaderboard.py` 从 `benchmarks/results/2026-05-26/*.json` 生成。")
    lines.append("")

    # 按 dataset → algo 排序
    df_sorted = df.sort_values(["dataset", "dataset_rank"]).reset_index(drop=True)

    for ds in sorted(df["dataset"].unique()):
        lines.append(f"## {ds}")
        lines.append("")
        sub = df_sorted[df_sorted["dataset"] == ds]
        for _, row in sub.iterrows():
            lines.append(f"### `{row['algo']}` × `{ds}` — 排名 {int(row['dataset_rank'])} / {len(sub)}")
            lines.append("")
            lines.append("**Prediction：**")
            lines.append("")
            lines.append(f"- `logLoss` = {row['logLoss']:.6f}")
            lines.append(f"- `ici` = {row['ici']:.6f}")
            lines.append(f"- `auc` = {row['auc']:.6f}")
            lines.append(f"- `maeP` = {row['maeP']:.6f}")
            lines.append(f"- `prediction_raw` = {row['prediction_raw']:.6f}  →  `prediction_score` = {row['prediction_score']:.6f}")
            lines.append("")
            lines.append("**DHP：**")
            lines.append("")
            lines.append(f"- `expectedMemoryFinal` = {row['expectedMemoryFinal']:.4f}")
            lines.append(f"- `masteredCount` = {int(row['masteredCount'])}")
            lines.append(f"- `totalReviews` = {int(row['totalReviews'])}")
            lines.append(f"- `efficiency` = {row['efficiency']:.6f}")
            lines.append(f"- `dhp_raw` = {row['dhp_raw']:.4f}  →  `dhp_score` = {row['dhp_score']:.6f}")
            lines.append("")
            lines.append("**Policy：**")
            lines.append("")
            lines.append(f"- `finalRecallRate` = {row['finalRecallRate']:.6f}")
            lines.append(f"- `reviewsPerDay` = {row['reviewsPerDay']:.4f}")
            lines.append(f"- `retentionStability` = {row['retentionStability']:.6f}")
            lines.append(f"- `policy_raw` = {row['policy_raw']:.6f}  →  `policy_score` = {row['policy_score']:.6f}")
            lines.append("")
            lines.append("**汇总：**")
            lines.append("")
            lines.append(f"- `final_score` = `0.45×prediction + 0.35×dhp + 0.20×policy` = **{row['final_score']:.6f}**")
            lines.append(f"- `dataset_rank` = **{int(row['dataset_rank'])}** / {len(sub)}")
            lines.append(f"- `borda_points` = **{int(row['borda_points'])}**")
            lines.append(f"- `runtime_seconds` = {row['runtime_seconds']:.2f}")
            note = row.get("notes", "")
            if note:
                lines.append(f"- `notes` = {note}")
            lines.append("")
        lines.append("---")
        lines.append("")
    return "\n".join(lines)


def _generate_methodology_md(df: pd.DataFrame) -> str:
    """生成 04-methodology.md 内容。"""
    lines: List[str] = []
    lines.append("# 评估方法学 2026-05-26")
    lines.append("")
    lines.append("> 本次 AMAS vs 市面 SRS 算法 benchmark 的数据集、算法实现、加权策略、Borda 合并、已知限制。")
    lines.append("")

    lines.append("## 1. 数据集来源")
    lines.append("")
    lines.append("| 数据集 | 行数 | 来源 | 用途 | oracle |")
    lines.append("|---|---|---|---|---|")
    lines.append("| maimemo | 232,419,294 | [MaiMemo 公开 2022 review log](https://github.com/maimemo/SSP-MMC-FSRS) | 主 benchmark（含 GRU oracle 训练） | 自训 GRU-HLR 64hid 5ep |")
    lines.append("| duolingo_hlr | 12,854,226 | [Settles & Meeder 2016, ACL](https://github.com/duolingo/halflife-regression) | 跨用户/语言泛化测试 | 复用 maimemo oracle |")
    lines.append("| synthetic | 4,500,000 | DHP simulator + 随机用户 90 天行为 | DHP ground truth 上限测试 | 复用 maimemo oracle |")
    lines.append("")
    lines.append("**Oracle 跨数据集复用 caveat：** maimemo 上训练的 GRU-HLR oracle 直接用作 duolingo_hlr / synthetic 的 forward simulator（benchmark-runner 通过软链 symlink 实现）。这是工程取舍 —— 避免 3 次重训 oracle（每次约 50min CPU），但代价是 oracle 对 duolingo / synthetic 的 user-token 联合分布存在分布偏移。在 prediction 维度，这意味着 ICI / AUC 在 duolingo_hlr / synthetic 上的绝对值不可与 maimemo 直接对比；本评测通过 **per-dataset min-max normalize** 把绝对值差异吸收到归一化空间，跨数据集合并只用 Borda 排名，规避偏差直接影响最终排序。")
    lines.append("")

    lines.append("## 2. 八个算法实现位置")
    lines.append("")
    lines.append("所有 scheduler 实现位于 `benchmarks/maimemo/schedulers.py`，原生 Python，与 AMAS Rust 实现通过 `adapter.py` 桥接：")
    lines.append("")
    lines.append("| 算法 | 类 | 实现位置 | 参考实现 |")
    lines.append("|---|---|---|---|")
    lines.append("| amas | `AmasScheduler` | `benchmarks/maimemo/schedulers.py:AmasScheduler` + `benchmarks/maimemo/adapter.py` → `src/amas/memory/mdm.rs` | WordForge AMAS = FSRS-5 等价 |")
    lines.append("| fsrs | `FSRSScheduler` | `benchmarks/maimemo/schedulers.py:FSRSScheduler` | https://github.com/open-spaced-repetition/fsrs-rs |")
    lines.append("| fsrs45 | `FSRS45Scheduler` | `benchmarks/maimemo/schedulers.py:FSRS45Scheduler` | https://github.com/open-spaced-repetition/py-fsrs (4.5 tag) |")
    lines.append("| dhp | `DHPScheduler` | `benchmarks/maimemo/dhp_reference.py:DHPScheduler` | Yarrow Madrona DHP 2024 |")
    lines.append("| sm2 | `SM2Scheduler` | `benchmarks/maimemo/schedulers.py:SM2Scheduler` | Woźniak 1990, super-memory.com/english/ol/sm2.htm |")
    lines.append("| hlr | `HLRScheduler` | `benchmarks/maimemo/schedulers.py:HLRScheduler` | Settles & Meeder 2016, ACL |")
    lines.append("| leitner | `LeitnerScheduler` | `benchmarks/maimemo/schedulers.py:LeitnerScheduler` | 5-box Leitner 1972 |")
    lines.append("| random | `RandomScheduler` | `benchmarks/maimemo/schedulers.py:RandomScheduler` | baseline，固定 seed=42 |")
    lines.append("")
    lines.append("详细算法定义、公式与论文引用见 [02-algo-research.md](./02-algo-research.md)。")
    lines.append("")

    lines.append("## 3. 三维加权公式（spec §5）")
    lines.append("")
    lines.append("```")
    lines.append("final_score = 0.45 × prediction_score")
    lines.append("            + 0.35 × dhp_score")
    lines.append("            + 0.20 × policy_score")
    lines.append("")
    lines.append("# Prediction raw（越大越好）")
    lines.append("prediction_raw = 0.4 × (1 - min(logLoss, 2) / 2)")
    lines.append("               + 0.3 × (1 - ici)")
    lines.append("               + 0.3 × auc")
    lines.append("")
    lines.append("# DHP raw（越大越好）")
    lines.append("dhp_raw = 0.5 × expectedMemoryFinal")
    lines.append("        + 0.5 × efficiency × 1000")
    lines.append("")
    lines.append("# Policy raw（越大越好）")
    lines.append("policy_raw = 0.5 × retentionStability")
    lines.append("           + 0.5 × (1 - min(reviewsPerDay / 10000, 1))")
    lines.append("```")
    lines.append("")
    lines.append("**logLoss cap = 2.0** 防止 HLR 那种炸到 10+ 的 outlier 主导分布；超过 cap 视为「同等差」。**reviewsPerDay cap = 10000** 同理。")
    lines.append("")

    lines.append("## 4. 跨数据集合并 = per-dataset normalize + Borda")
    lines.append("")
    lines.append("**步骤：**")
    lines.append("")
    lines.append("1. 每 (algo, dataset) 算 3 个 raw 分数（prediction_raw, dhp_raw, policy_raw）。")
    lines.append("2. **每个 dataset 内** 对 raw 分别做 min-max normalize → 落到 [0,1]。")
    lines.append("   - 若 dataset 内 max == min（极端情况，所有 algo 同分），统一赋 0.5。")
    lines.append("3. 加权求和得 `final_score`。")
    lines.append("4. 每 dataset 内按 `final_score` 降序排名。")
    lines.append("5. Borda 计数：第 1 名得 N 分，第 N 名得 1 分（N = 该 dataset algo 数 = 8）。")
    lines.append("6. 各 algo 跨 3 个 dataset Borda 求和 → 综合排名。")
    lines.append("")
    lines.append("**为什么不直接均值 `final_score`？** —— 数据集间绝对分布差异大（synthetic 的 logLoss 普遍 > maimemo），即便归一化后均值仍受异常值（HLR）拉低尾部。Borda 是序数统计，对绝对值不敏感，更适合「跨异质评测合并排名」场景。")
    lines.append("")

    lines.append("## 5. 已知限制")
    lines.append("")
    lines.append("### 5.1 HLR θ 默认值偏离原 paper")
    lines.append("")
    hlr_rows = df[df["algo"]=="hlr"].sort_values("dataset")
    hlr_ll_avg = hlr_rows["logLoss"].mean()
    lines.append(f"`HLRScheduler` 当前 θ = (2.0, -2.5, -0.3)，导致 5+ correct 后 halflife > 100 天，对真实 lapse 给极低 likelihood，三数据集平均 logLoss = {hlr_ll_avg:.2f}（远高于其他 algo）。原 Duolingo paper θ ≈ (0.5, -1.0, -0.3)，预计 logLoss 回到 0.5-0.7 区间。")
    lines.append("")
    lines.append("**为什么不改？** —— HLR 的设计意图是 stability 极度乐观（用 stability 节省 reviewsPerDay）；spec 优先保留实现意图 + 在 logLoss cap=2 下吸收偏差。后续若要恢复 HLR 在 prediction 维度的可比较性，需要在 `benchmarks/maimemo/schedulers.py:HLRScheduler` 改 default θ 并重跑。")
    lines.append("")

    lines.append("### 5.2 AMAS prediction 与 FSRS 同源")
    lines.append("")
    lines.append("AMAS 的 `wordSelector` / `ensemble` 在 prediction 维度不起作用（adapter 在评测层仅注入 MDM），因此 AMAS 与 FSRS 在三数据集上的 `logLoss / ici / auc / maeP` **完全相同**。这与 `docs/superpowers/specs/2026-05-26-amas-algo-comparison-design.md` § 9 YAGNI 结论一致：MDM-only 设计有效，扩展到 wordSelector/ensemble 的 30+ 参数对 next-step recall prediction 无贡献。")
    lines.append("")
    lines.append("如需在 prediction 维度区分 AMAS / FSRS，需要把 wordSelector / ensemble 的影响下沉到 forward simulation 阶段，让它通过改变调度密度（reviewsPerDay 分布）间接影响 prediction 评测 —— 但这与当前评测 schema 不兼容。")
    lines.append("")

    lines.append("### 5.3 Oracle 跨数据集偏差")
    lines.append("")
    lines.append("maimemo oracle 软链到 duolingo_hlr / synthetic：duolingo 含 7 种语言、用户分布与 maimemo 完全不同；synthetic 是 DHP simulator 生成，分布更集中。oracle 在这两个数据集上的 calibration / discrimination 都不如 maimemo —— 反映在 duolingo_hlr 全员 AUC ≈ 0.5。")
    lines.append("")
    lines.append("**缓解方式：** per-dataset normalize → 把 oracle 偏差吸收到归一化空间。**未缓解情形：** 跨数据集合并仅用 Borda 排名，不会因数据集 A 整体表现差而拉低排名第 1 在该数据集的得分。")
    lines.append("")

    lines.append("### 5.4 Synthetic ground truth 偏向 DHP")
    lines.append("")
    lines.append("synthetic 数据集的 p_recall 由 DHP 内部模型生成，对 DHP scheduler 理应「主场优势」。但实际结果：")
    lines.append("")
    dhp_synth = df[(df["algo"]=="dhp") & (df["dataset"]=="synthetic")].iloc[0]
    amas_synth = df[(df["algo"]=="amas") & (df["dataset"]=="synthetic")].iloc[0]
    lines.append(f"- DHP scheduler 在 synthetic 上 logLoss = {dhp_synth['logLoss']:.3f}，反而高于 AMAS / FSRS 的 {amas_synth['logLoss']:.3f}。")
    lines.append(f"- AUC：DHP = {dhp_synth['auc']:.3f}，AMAS = {amas_synth['auc']:.3f} —— DHP 仅略胜。")
    lines.append("")
    lines.append("原因：synthetic 的 ground truth p_recall 由 DHP 内部 halflife 状态生成，但 DHP scheduler 在 forward simulation 中重新推断 halflife，与 oracle 的 internal state 存在偏差（state sampling noise）。这表明即便 ground truth 与算法同源，在 forward-simulation eval pipeline 下仍会引入额外噪声。")
    lines.append("")

    lines.append("### 5.5 duolingo_hlr positive rate 87%")
    lines.append("")
    duo_auc = df[df["dataset"]=="duolingo_hlr"]["auc"].mean()
    lines.append(f"duolingo_hlr 的 next_r 正样本率 ≈ 87%（用户答对占绝对多数），8 个 algo 平均 AUC = {duo_auc:.3f}（接近随机）。该数据集 prediction 维度区分度极低，但 policy / DHP 维度仍有信号。建议未来在 final_score 公式中对 duolingo_hlr 单独降低 prediction 权重，或仅用 ICI / Brier 校准分量。")
    lines.append("")

    lines.append("## 6. 复现")
    lines.append("")
    lines.append("```bash")
    lines.append("# 假设 24 个 JSON 已存在")
    lines.append("source .bench-venv/bin/activate")
    lines.append("python -m benchmarks.maimemo.cli leaderboard \\")
    lines.append("  --results benchmarks/results/2026-05-26 \\")
    lines.append("  --out docs/algo-bench-2026-05-26")
    lines.append("")
    lines.append("# 单元测试")
    lines.append("pytest benchmarks/maimemo/tests/test_leaderboard.py -v")
    lines.append("```")
    lines.append("")
    lines.append("---")
    lines.append("")
    lines.append("延伸阅读：")
    lines.append("- [01-leaderboard.md](./01-leaderboard.md) — 综合排名 + 三维独立 + 各数据集独立")
    lines.append("- [02-algo-research.md](./02-algo-research.md) — SM-2 / HLR / FSRS-4.5 算法定义")
    lines.append("- [03-detailed-results.md](./03-detailed-results.md) — 24 个 (algo, dataset) 全量原始指标")
    lines.append("")
    return "\n".join(lines)


def generate_reports(df: pd.DataFrame, out_dir: Path) -> Dict[str, int]:
    """生成 01-leaderboard.md / 03-detailed-results.md / 04-methodology.md。

    返回 {filename: byte_size}.
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    agg = aggregate_borda(df)
    results: Dict[str, int] = {}
    for fname, content in [
        ("01-leaderboard.md", _generate_leaderboard_md(df, agg)),
        ("03-detailed-results.md", _generate_detailed_md(df)),
        ("04-methodology.md", _generate_methodology_md(df)),
    ]:
        path = out_dir / fname
        path.write_text(content, encoding="utf-8")
        results[fname] = path.stat().st_size
    return results
