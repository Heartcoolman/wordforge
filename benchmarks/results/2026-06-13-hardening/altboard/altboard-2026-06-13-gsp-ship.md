# Oracle-truth alt-board recompute for the F1 TEST results (2026-06-13-gsp-ship)

Unofficial disclosure appendix, hardening campaign. Formulae and pipeline are
byte-identical to docs/algo-bench-2026-06-12-d5-ship/appendix/alt_board.py;
only --results points at benchmarks/results/2026-06-13-gsp-ship/.

### Official formula (v0.9: 0.7*mastered + 0.3*efficiency*10000) [anchor]

| 排名 | 算法 | Borda 总分 | final_score 均值 | duolingo_hlr 排名 | maimemo 排名 | synthetic 排名 |
|---|---|---|---|---|---|---|
| 1 | **`amas`** | 30 | 0.965 | 1 | 1 | 1 |
| 2 | `fsrs45` | 26 | 0.907 | 3 | 2 | 2 |
| 3 | `amas6` | 23 | 0.891 | 2 | 5 | 3 |
| 4 | `fsrs6` | 19 | 0.841 | 4 | 6 | 4 |
| 5 | `fsrs` | 17 | 0.671 | 7 | 4 | 5 |
| 6 | `dhp` | 14 | 0.626 | 8 | 3 | 8 |
| 7 | `random` | 13 | 0.474 | 5 | 9 | 6 |
| 8 | `sm2` | 13 | 0.605 | 6 | 7 | 7 |
| 9 | `leitner` | 7 | 0.420 | 9 | 8 | 9 |
| 10 | `hlr` | 3 | 0.001 | 10 | 10 | 10 |

### Alt-1 oracle-weighted [unofficial] (0.5*expectedMemoryFinal + 0.3*efficiency*10000 + 0.2*mastered)

| 排名 | 算法 | Borda 总分 | final_score 均值 | duolingo_hlr 排名 | maimemo 排名 | synthetic 排名 |
|---|---|---|---|---|---|---|
| 1 | **`amas`** | 29 | 0.907 | 2 | 1 | 1 |
| 2 | `fsrs6` | 24 | 0.800 | 1 | 6 | 2 |
| 3 | `amas6` | 20 | 0.787 | 5 | 5 | 3 |
| 4 | `sm2` | 20 | 0.753 | 3 | 4 | 6 |
| 5 | `fsrs` | 18 | 0.773 | 7 | 3 | 5 |
| 6 | `dhp` | 17 | 0.740 | 6 | 2 | 8 |
| 7 | `leitner` | 15 | 0.690 | 4 | 7 | 7 |
| 8 | `fsrs45` | 13 | 0.697 | 8 | 8 | 4 |
| 9 | `random` | 5 | 0.472 | 9 | 10 | 9 |
| 10 | `hlr` | 4 | 0.350 | 10 | 9 | 10 |

### Alt-2 pure expectedMemoryFinal [unofficial]

| 排名 | 算法 | Borda 总分 | final_score 均值 | duolingo_hlr 排名 | maimemo 排名 | synthetic 排名 |
|---|---|---|---|---|---|---|
| 1 | **`amas`** | 24 | 0.771 | 6 | 2 | 1 |
| 2 | `fsrs` | 22 | 0.724 | 4 | 3 | 4 |
| 3 | `sm2` | 22 | 0.713 | 1 | 4 | 6 |
| 4 | `fsrs6` | 21 | 0.759 | 5 | 5 | 2 |
| 5 | `dhp` | 20 | 0.675 | 3 | 1 | 9 |
| 6 | `amas6` | 17 | 0.730 | 7 | 6 | 3 |
| 7 | `leitner` | 17 | 0.703 | 2 | 7 | 7 |
| 8 | `fsrs45` | 12 | 0.639 | 8 | 8 | 5 |
| 9 | `random` | 7 | 0.532 | 9 | 9 | 8 |
| 10 | `hlr` | 3 | 0.350 | 10 | 10 | 10 |
