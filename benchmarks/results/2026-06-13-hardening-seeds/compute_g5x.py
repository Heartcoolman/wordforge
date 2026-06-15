"""G5X 种子稳定性 10 种子双 tier 统计（预注册公式：00-protocol.md §3，db05fd8 冻结）。

Tier 1: amas-only + what-if 注入固定 seed-42 val 榜（沿用既有方法），读 tier1_seed_runs.jsonl。
Tier 2: 全量 10 算法 val 榜逐种子重模拟，读 val-board-seed<N>/。两 tier 永不混池。
"""
from __future__ import annotations

import json
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
from benchmarks.maimemo.whatif_board import load_board, score_board  # noqa: E402

HERE = Path(__file__).resolve().parent
S10 = [42, 7, 2026, 0, 1, 2, 3, 4, 5, 6]
DATASETS = ("maimemo", "duolingo_hlr", "synthetic")


def tier1() -> dict:
    rows = [json.loads(l) for l in (HERE / "tier1_seed_runs.jsonl").read_text().splitlines()]
    by_seed = {r["seed"]: r for r in rows if r["label"].startswith(("G5_", "G5X_"))}
    assert sorted(by_seed) == sorted(S10), sorted(by_seed)
    out = {}
    for ds in DATASETS:
        ranks = [by_seed[s]["per_dataset_rank"][ds] for s in S10]
        margins = [by_seed[s]["margins"][ds]["amas_to_next"] for s in S10]
        out[ds] = _verdict(ranks, margins)
    out["borda_strict_first_count"] = sum(1 for s in S10 if by_seed[s]["borda"] == 30)
    return out


def tier2() -> dict:
    per_seed = {}
    for s in S10:
        d = HERE / f"val-board-seed{s}"
        files = list(d.glob("*.json"))
        assert len(files) == 30, f"seed {s}: {len(files)}/30 JSONs"
        df = score_board(load_board(d))
        per_seed[s] = {}
        for ds, g in df.groupby("dataset"):
            g = g.sort_values("final_score", ascending=False).reset_index(drop=True)
            amas_rank = int(g.index[g.algo == "amas"][0]) + 1
            if amas_rank == 1:
                margin = float(g.iloc[0].final_score - g.iloc[1].final_score)
            else:  # 失利种子记负缺口
                above = g.iloc[amas_rank - 2].final_score
                mine = float(g[g.algo == "amas"].iloc[0].final_score)
                margin = mine - float(above)
            n = len(g)
            borda_leg = {row.algo: n - int(i) for i, row in g.iterrows()}
            per_seed[s][ds] = {"rank": amas_rank, "margin": margin, "borda_leg": borda_leg}
    out = {"per_seed_raw": {str(s): {ds: {k: v for k, v in per_seed[s][ds].items() if k != "borda_leg"}
                                     for ds in DATASETS} for s in S10}}
    for ds in DATASETS:
        ranks = [per_seed[s][ds]["rank"] for s in S10]
        margins = [per_seed[s][ds]["margin"] for s in S10]
        out[ds] = _verdict(ranks, margins)
    bordas, strict = [], 0
    for s in S10:
        total = {}
        for ds in DATASETS:
            for a, b in per_seed[s][ds]["borda_leg"].items():
                total[a] = total.get(a, 0) + b
        amas = total.pop("amas")
        bordas.append(amas)
        if amas > max(total.values()):
            strict += 1
    out["amas_borda_per_seed"] = bordas
    out["borda_strict_first_count"] = strict
    return out


def _verdict(ranks: list, margins: list) -> dict:
    mean = statistics.mean(margins)
    std = statistics.stdev(margins)  # ddof=1，与 3 种子既有口径一致
    return {
        "ranks": ranks,
        "rank1_count": sum(1 for r in ranks if r == 1),
        "rank_pass": all(r == 1 for r in ranks),
        "margins": [round(m, 6) for m in margins],
        "margin_mean": round(mean, 6),
        "margin_std_ddof1": round(std, 6),
        "margin_pass": mean >= 2 * std,
    }


if __name__ == "__main__":
    result = {
        "protocol": "docs/amas-tuning-2026-06-13-hardening/00-protocol.md (commit db05fd8, frozen pre-run)",
        "seeds": S10,
        "tier1_amas_only_whatif": tier1(),
        "tier2_full_board": tier2(),
        "note": "tiers are different estimands (amas-only noise vs fully coupled board noise); never pooled",
    }
    out = HERE / "seed_stability_10.json"
    out.write_text(json.dumps(result, indent=1, ensure_ascii=False) + "\n")
    print(json.dumps({t: {ds: {k: result[t][ds][k] for k in ("rank1_count", "margin_mean", "margin_std_ddof1", "margin_pass")}
                          for ds in DATASETS} | {"borda_strict_first": result[t]["borda_strict_first_count"]}
                      for t in ("tier1_amas_only_whatif", "tier2_full_board")}, indent=1))
    print("written:", out)
