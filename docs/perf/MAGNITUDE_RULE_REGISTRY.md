# Magnitude rule and the registry of rejected fast-path candidates (measurement only)

**Runtime changes: none.**

## The rule (owner, 2026-10-30)

Judge an A/B by the size of its wins and losses, not by their count: a large
win paid for with small losses is a good trade. For paired bench:table runs
(order base, x, x, base) `scripts/g1_magnitude_verdict.mjs` reports per cell
both the ratio to mimalloc and the sefer time between the two builds. The
decision uses the sefer time, because the mimalloc code also moves between two
builds, which makes the ratio noisier. The ratio still decides the class against
mimalloc. A cell's effect counts only if both of its pairs agree in sign; its
size is the smaller |Δ|.

GO requires all four:
1. the geometric mean over the cells is ≤ −0.5 %;
2. the agreed wins are ≥ 2 × the agreed losses;
3. the largest agreed loss is ≤ 3 %;
4. no cell enters the LOSS class against mimalloc (sefer/mi > 1.03).

A run-set verdict is a screen. A candidate ships only on fresh data
pre-registered under this rule; FG-1/B1-R is the first such re-measurement.

## Registry: paired bench:table candidates

Derived by `node scripts/g1_magnitude_registry.mjs`:

| # | Candidate (run set) | sefer-time geomean | agreed wins / losses | largest agreed loss | ratio-to-mimalloc geomean | magnitude rule |
|---:|---|---:|---:|---:|---:|---|
| 1 | FG-1/B1 (screen) | -2.4 % | 2.1 / 0.0 pp | +0.0 % | +0.3 % | GO |
| 2 | FG-1/B1-R set 1 | -0.5 % | 5.4 / 0.8 pp | +0.8 % | +1.4 % | NO-GO |
| 3 | FG-1/B1-R set 2 | -0.2 % | 0.8 / 0.8 pp | +0.8 % | -0.5 % | NO-GO |
| 4 | F-1 | +0.2 % | 4.0 / 4.5 pp | +2.4 % | +1.7 % | NO-GO |
| 5 | T-1 | +1.7 % | 13.5 / 30.2 pp | +8.2 % | +1.2 % | NO-GO |
| 6 | WB-1 A-1 (check3) | +1.9 % | 0.7 / 25.3 pp | +20.9 % | +1.7 % | NO-GO |
| 7 | WB-1 A-1 (check3b) | +2.0 % | 1.6 / 25.0 pp | +20.1 % | +4.1 % | NO-GO |
| 8 | WB-3 C1 (check2) | -0.4 % | 2.5 / 1.5 pp | +0.8 % | -3.6 % | NO-GO |

FG-1/B1 was the only rejected run set that passes the magnitude rule as a
screen. Its pre-registered re-measurement on `main` (two ABBA sets,
`docs/perf/FG1_B1R_RECONFIRM_PREREG.md`) found the effect real but small
(−0.5 % and −0.2 %); the screen's −2.4 % came from one slow base run. NO-GO.

## Registry: candidates judged by other means (quoted from their cards)

| # | Candidate | Wins (size) | Losses (size) | Card |
|---:|---|---|---|---|
| 9 | G1-K2: free class from the state byte | bulk64 −9.6 %, mixed −4.6 %, bulk16k −3.1 % (cycles) | pair16 +14 %, churn16 +5.2 % | 126 |
| 10 | FG-1/A: K2 variant | −4.7 Ir | churn +17…+26 % (one run per arm) | 139 |
| 11 | GR-1: flush without re-checks | pair16, churn1024, larson −1.4 % (J1′) | bulk64 +2.3 %, bulk16k +3.5 % | 121 |
| 12 | U32 v2: bump per leaf | — | +0.6 % (CI through 0) | 122 |
| 13 | G1-K6: flush stores | bulk16k −0.7 % cycles | below the layout noise floor | 128 |
| 14 | A-2: plain LIVE store at issue | 16↔64 sefer/mi −1.3 % | ±1 % elsewhere | 141 |
| 15 | WB-1 B-1a: inline depot chain | −0.16 Ir per pair | — (time not measured) | 134 |
| 16 | PERF-2: `TCACHE_CAP` 32/64/128 (July, before the depot) | — | every bench slower | PERF2 report |

None of these passes the rule: K2 and GR-1 lose in the cells they were meant to
help, or by more than they win, and the others are null.
