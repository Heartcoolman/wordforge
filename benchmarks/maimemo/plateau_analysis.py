"""plateau_analysis.py — F1 邻域平台/针尖离线分析（hardening campaign 2026-06-13）。

只读 benchmarks/results/2026-06-12-rank1-search/results.jsonl（+ 可选 seed_stability.json），
纯离线确定性复现：不联网、不跑 sim、不写文件。回答：F1 是否坐在搜索面平台上
（邻居也赢榜 = 稳），还是针尖上（旋钮微动即丢榜 = 过拟合信号）。

用法::

    python -m benchmarks.maimemo.plateau_analysis \
        [--results benchmarks/results/2026-06-12-rank1-search/results.jsonl]
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any, Dict, List

# F1 冻结配置（GSP 搜索维度 + fuzz；其余维度全网格恒定：grade=4 / alpha 全关）
F1_KNOBS: Dict[str, float] = {
    "gspIntervalCapDays": 40,
    "gspGraduationStreak": 2,
    "gspGraduationFloorDays": 30,
    "gspYoungRetention": 0.86,
    "gspMatureRetention": 0.92,
    "gspMaturityBandDays": 14,
}
KNOB_ORDER = list(F1_KNOBS.keys()) + ["gspIntervalFuzz"]
SHORT = {
    "gspIntervalCapDays": "cap", "gspGraduationStreak": "streak",
    "gspGraduationFloorDays": "floor", "gspYoungRetention": "young",
    "gspMatureRetention": "mature", "gspMaturityBandDays": "band",
    "gspIntervalFuzz": "fuzz",
}
DATASETS = ["maimemo", "duolingo_hlr", "synthetic"]
DEFAULT_RESULTS = Path("benchmarks/results/2026-06-12-rank1-search/results.jsonl")


def fuzz_of(label: str) -> float:
    m = re.search(r"fuzz([0-9.]+)", label)
    return float(m.group(1)) if m else 0.0


def load(path: Path) -> List[Dict[str, Any]]:
    rows = [json.loads(l) for l in path.open(encoding="utf-8")]
    for r in rows:
        knobs = {k: float(v) for k, v in r["config"].items()}
        knobs["gspIntervalFuzz"] = fuzz_of(r["label"])
        r["_knobs"] = knobs
        r["_diffs"] = [k for k in KNOB_ORDER
                       if knobs[k] != float(F1_KNOBS.get(k, 0.0))]
        g = r["gates"]
        r["_g123"] = bool(g["G1"]["pass"] and g["G2"]["pass"] and g["G3"]["pass"])
    return rows


def cfg_key(r: Dict[str, Any]) -> str:
    k = r["_knobs"]
    return (f"cap{k['gspIntervalCapDays']:g}/k{k['gspGraduationStreak']:g}"
            f"/fl{k['gspGraduationFloorDays']:g}/y{k['gspYoungRetention']:g}"
            f"/m{k['gspMatureRetention']:g}/b{k['gspMaturityBandDays']:g}"
            f"/fz{k['gspIntervalFuzz']:g}")


def gate_fail_reasons(r: Dict[str, Any]) -> str:
    g, out = r["gates"], []
    if not g["G1"]["pass"]:
        out.append(f"G1(strict={g['G1']['strict_first']},margin_ok={g['G1']['margin_ok']})")
    if not g["G2"]["pass"]:
        out.append(f"G2(mast={g['G2']['mai_mastered']:.0f}<17000)")
    if not g["G3"]["pass"]:
        bad = [f"{ds}.{'ll' if not b['ll_ok'] else ''}{'ici' if not b['ici_ok'] else ''}"
               for ds, b in g["G3"]["bits"].items() if not (b["ll_ok"] and b["ici_ok"])]
        out.append("G3(" + ",".join(bad) + ")")
    return ";".join(out) if out else "-"


def margins_str(r: Dict[str, Any]) -> str:
    parts = []
    for ds in DATASETS:
        m = r["margins"][ds]
        if m["amas_rank"] == 1:
            parts.append(f"{ds[:3]}+{m['amas_to_next']:.4f}")
        else:
            parts.append(f"{ds[:3]}#%d-%.4f" % (m["amas_rank"], m["to_beat_above"]))
    return " ".join(parts)


def row_line(r: Dict[str, Any]) -> str:
    return (f"{r['label']:<30s} seed={r['seed']:<4d} borda={r['borda']:>2d} "
            f"rank={r['overall_rank']} G123={'Y' if r['_g123'] else 'n'} "
            f"fails={gate_fail_reasons(r):<40s} margins[{margins_str(r)}]")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--results", type=Path, default=DEFAULT_RESULTS)
    args = ap.parse_args()
    rows = load(args.results)

    print(f"# F1 plateau analysis — {args.results} ({len(rows)} rows)")
    print(f"# F1 = cap40/k2/fl30/y0.86/m0.92/b14/fz0 (grade=4, alpha off — grid-constant)")

    # ---- Table A: F1 复现行（dist=0，含多 seed）----
    print("\n== [A] F1 exact-config rows (replicates, all seeds) ==")
    for r in rows:
        if not r["_diffs"]:
            print("  " + row_line(r))

    # ---- Table B: 单旋钮邻居（dist=1）----
    print("\n== [B] Single-knob neighbors of F1 (dist=1) ==")
    for r in rows:
        if len(r["_diffs"]) == 1:
            k = r["_diffs"][0]
            print(f"  {SHORT[k]}={r['_knobs'][k]:g}".ljust(14) + row_line(r))

    # ---- Table B2: 锚点 cap45（已验证 Borda-30 邻居）的单旋钮邻居，补 F1 处缺测轴 ----
    anchor = dict(F1_KNOBS, gspIntervalCapDays=45)
    print("\n== [B2] Single-knob neighbors of anchor cap45 (proxy for axes untested at F1) ==")
    for r in rows:
        diffs = [k for k in KNOB_ORDER
                 if r["_knobs"][k] != float(anchor.get(k, 0.0))]
        if len(diffs) == 1 and diffs[0] != "gspIntervalCapDays":
            k = diffs[0]
            print(f"  {SHORT[k]}={r['_knobs'][k]:g}".ljust(14) + row_line(r))

    # ---- Table C: 各旋钮轴向证据（band-off 上下文 cap*/k*/fl*/y0/m0/b0）----
    print("\n== [C] Band-off context scans (y=m=b=0): cap / streak / floor axes ==")
    for r in sorted((r for r in rows if r["seed"] == 42
                     and r["_knobs"]["gspYoungRetention"] == 0.0
                     and r["_knobs"]["gspMatureRetention"] == 0.0),
                    key=lambda r: (r["_knobs"]["gspGraduationStreak"],
                                   r["_knobs"]["gspIntervalCapDays"],
                                   r["_knobs"]["gspGraduationFloorDays"])):
        print(f"  {cfg_key(r):<40s}" + row_line(r))

    # ---- Table D: G1+G2+G3 passers 的旋钮覆盖谱 ----
    passers, seen = [], set()
    for r in rows:
        if r["seed"] == 42 and r["_g123"] and cfg_key(r) not in seen:
            seen.add(cfg_key(r))
            passers.append(r)
    print(f"\n== [D] Distinct seed-42 configs passing G1+G2+G3: {len(passers)} ==")
    for k in KNOB_ORDER:
        vals_all = sorted({r["_knobs"][k] for r in passers})
        vals_b30 = sorted({r["_knobs"][k] for r in passers if r["borda"] == 30})
        print(f"  {SHORT[k]:<7s} pass-values={[f'{v:g}' for v in vals_all]}"
              f"  borda30-values={[f'{v:g}' for v in vals_b30]}")
    n30 = sum(1 for r in passers if r["borda"] == 30)
    print(f"  ({n30}/{len(passers)} passers are Borda-30)")

    # ---- Table E: F1 处缺失的单旋钮邻居（诚实缺口）----
    print("\n== [E] Single-knob axes UNTESTED at F1 (gaps; no new sims run) ==")
    covered = {r["_diffs"][0] for r in rows if len(r["_diffs"]) == 1}
    for k in KNOB_ORDER:
        if k not in covered:
            print(f"  {SHORT[k]}: no dist-1 row at F1 (nearest evidence = dist>=2 contexts, see [B2]/[C])")
    print("  gspSuccessGrade: grade=3 only at GSP-off base "
          "(results_grade3_diagnostic.jsonl borda=17 vs grade4 base 21/24); never at F1")

    # ---- seed stability 摘要 ----
    ss_path = args.results.parent / "seed_stability.json"
    if ss_path.exists():
        ss = json.loads(ss_path.read_text(encoding="utf-8"))
        f1 = ss["finalists"]["F1"]
        print("\n== [F] F1 seed stability (G5, seeds 42/7/2026) ==")
        print(f"  borda_per_seed={f1['borda_per_seed']} strict_gap={f1['borda_strict_gap_per_seed']}"
              f" g1_all_seeds={f1['g1_pass_all_seeds']}")
        for ds in DATASETS:
            d = f1["per_dataset"][ds]
            print(f"  {ds:<13s} win_margins={[round(m,5) for m in d['win_margins']]}"
                  f" mean={d['margin_mean']:.5f} 2std={d['two_std']:.5f}"
                  f" mean>=2std={d['margin_mean_ge_2std']}")
        print(f"  g5_verdicts={f1['g5_dataset_verdicts']} (duolingo margin thin — known caveat)")


if __name__ == "__main__":
    main()
