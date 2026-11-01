# WILD-2 — preregistration (written before any measurement of the change)

Status: registered on the branch before the first run of the changed build. Rules below are fixed; a
result that does not meet them is reported as it is.

## 1. Question and change

WILD-1 (`docs/perf/WILD1_GAP_DECOMPOSITION.md`) found that in `wild` the own-thread small-free lane misses its
one-entry segment memo on 88.8 % of the frees (3 804 795 / 4 285 881, Windows, median of 5) and that the
unchanged chain behind a miss executes 91.7 instructions per call (Linux callgrind, scale 0.3).

**Change A (the only code change of this task):** the memo-mismatch exit of `dealloc_fast` calls a new cold
function, `lane_second_chance`. It probes the direct-mapped `own_cache` (`SegmentTable::probe_arm`): only an
exact-base hit (`cached_key == key` and `cached_root == key`) arms the memo and continues with the lane's own
acceptance steps (kind and payload start, granule bound, state byte == tag, magazine push or the existing
full-magazine tail), copied in the same order with the same two refinement `debug_assert!`s. Every other case
and every reject returns to the unchanged chain (`lane_miss`). The memo-HIT path and the first two lane exits
are not edited; `OWN_CACHE_SIZE` stays 16.

**Not in this task (B):** raising `OWN_CACHE_SIZE`. Under `--all-features` `HeapCore` is 9208 B
(`tests/r34_18_heap_core_stack_pressure_pin.rs`), +768 B for 64 entries exceeds the 9216 B assert; B needs a
stack-budget decision or an out-of-line cache and is a separate, later factorial arm.

Consultations: @oh and the rush reviewer (read-only), both before the code was written; their conditions are
met in the code: exact-root gate, memo armed before the guard and left armed on a reject, Tier-1 accounting once
per free, `cfg(all(alloc-global, fastbin, not(hardened)))` on every new accessor, the acceptance steps duplicated
in the cold function instead of a shared inline helper (to keep the hit path's inlining unchanged), a
`bench-internals` activation counter (`tier1_bump`).

## 2. Arms and identity

- `base` = `origin/main` at `c0ee7dfb`; `base2` = a byte-identical copy of the `base` executable (A/A control);
  `x` = the commit of this branch that is fixed before the first measurement (recorded in `ident.txt` together
  with the tree hash, the executable hashes, features and rustc).
- Judge builds: `--release --example g1_wild_judge --features "production internals"`; activation build:
  the same plus `bench-internals`, used for counts only, never for timing.
- Windows (primary) and Linux/WSL2 (confirmation). One machine; shared; nothing else started by this work.

## 3. Measurements and decision rules

All timings: per block and arm the minimum of 3 back-to-back repeats, 12 blocks, scenario order shuffled per
block with a fixed seed, arm order rotated over the six permutations of (base, base2, x); per block
d = ln(x / base) and the A/A control a = ln(base2 / base); cell = exp(mean) with a two-sided 95 % t-interval
(df = 11). The A/A interval must include 1 in a scenario for that scenario to be read at all.

| id | what | rule |
|---|---|---|
| T1 | Windows judge `wild`, x / base | **GO needs mean <= 0.97 and upper bound < 1.0** |
| T2 | Linux judge `wild`, x / base (scale 1) | mean <= 0.98 and upper bound < 1.0 |
| T3 | activation (bench-internals build, Windows, `wild`) | frees accepted by the second chance >= 40 % of the memo exits; otherwise the mechanism is not what moved T1 and the result is reported as unattributed |
| C1 | judge controls `phase`, `large`, `burst`, `grow64`, `grow1k`, `regrow`, `growmix`, Windows | **kill** if a mean is above 1.03; **HOLD (owner decides)** if the mean is at most 1.0 but the upper bound is above 1.03 |
| C2 | the same controls on Linux | same as C1 |
| C3 | `bench:table` paired, two sets (order base, x, x, base; aligned code), `scripts/g1_magnitude_verdict.mjs` | no new LOSS cell against mimalloc and largest loss <= 3 %. The plain magnitude rule (gm <= -0.5 %) is **not** required: the memo-hit path that the cells exercise is not edited, and the cells never miss the memo (WILD-1: 2 of 3 021 541 on a clean heap). This exception is stated here, before the data. |
| C4 | MT `malloc_macro` T = 1, 2, 4, ABBA x 3 | no cell with x worse than base by more than 3 % (median) |
| C5 | callgrind (Linux), Ir | `burst`: change within +-0.3 %; `phase`: <= +0.5 %; `wild`: negative |
| C6 | `.text` of the inlined memo-hit path (disassembly of the judge shim `a_dealloc`, base against x, call targets masked) | reported either way; any difference other than the target of the memo-mismatch call is listed and discussed before a GO |
| C7 | CI | all required jobs green, including `npm run iai` (+-10 Ir gate) and the new differential test |

Accept rule: T1 and T2 pass, T3 holds, no kill in C1..C6, C7 green. A HOLD in C1/C2 goes to the owner with
both campaigns' data. At most one confirmation campaign with the same rules; no further campaign after a HOLD
or a miss. Reported but not gated: `growmix` and `regrow` (probable beneficiaries), `wild` ratios to mimalloc.

## 4. Expectations (information, not thresholds)

From the WILD-1 probe: of the lane frees in `wild`, 88.8 % leave on a memo miss; 2 554 708 of 3 804 795 of those
hit `own_cache` (67 %). The change moves those frees from the chain (91.7 Ir) to the lane's own steps; the
remaining 1 256 610 (29.3 % of the lane frees) still reach the hash probe through the chain and pay one extra
failed `own_cache` probe. The hit path of the memo and the `burst` and `phase` scenarios (memo exits 0.0 % and
3.0 %) should not move.

## 5. Tests

`tests/wild2_lane_second_chance.rs` (differential, runs on CI): two Small segments, frees alternating between
them force memo miss + `own_cache` hit; `dbg_lane_second_chance_hits` proves the exit ran; the magazine-LIFO
oracle of `as1_fast_free_lane.rs` is replayed through that exit for accept, double free, interior and
misaligned pointers, wrong layout, metadata and uncarved offsets; over-aligned and Large frees must not be
counted.

## 6. Amendment 1 (written after campaign 1 and before campaign 2; post hoc by construction)

What campaign 1 showed (data committed under `docs/perf/wild2/`): on Windows the A/A control (a byte-identical
copy of `base`) has an interval that excludes 1 in `grow1k` and `wild`, so both are unreadable by the rule of
section 3; two controls (`phase` 1.083, `burst` 1.030) read KILL by the letter of C1 although their intervals
include 1 and their own A/A intervals are as wide; the two `bench:table` sets disagree about whole cells by tens of
percent (the same build, repeat against repeat); the MT samples contain values of 2-7 Mops/s against about 50 for
the same arm. At the time several other processes were using the shared machine (the process list is not recorded).
The 3 % gates of section 3 therefore cannot be evaluated from campaign 1; this is **not** a pass and not evidence of
a regression.

Changes for the one confirmation campaign that section 3 allows (decided before it runs; the gates themselves are
unchanged):

1. every judge process of campaign 2 is launched through `wnice -p high --` (the process gets the HIGH priority
   class; no CPU, thread or memory limit), so other work on the machine competes less with the measured process;
2. 16 blocks x minimum of 5 repeats (campaign 1: 12 x 3); the t quantile for df = 15 is 2.131;
3. deterministic evidence is reported next to the gates and **does not replace them**: callgrind Ir (rule C5, Linux)
   and `perf stat` user-mode cycles, instructions and L1D load misses, median of 5 runs per arm (Linux, supporting);
4. for C3 and C4, which are not repeated, the report states the noise floor of the data it already has: a
   `bench:table` cell whose two runs of the **same** build differ in their sefer / mi ratio by more than 10 % is
   marked UNREADABLE and left out of the loss test (the number of such cells is printed), and an MT cell whose base
   arm spreads by more than 1.25 (max / min of its 6 samples) is marked UNREADABLE. These two noise rules are post
   hoc and are disclosed as such in the report; a gate marked UNREADABLE is neither passed nor failed.

No third campaign. If campaign 2 ends in HOLD or UNREADABLE for a gate, the decision goes to the owner with the
data of both campaigns and the deterministic evidence.

## 7. Amendment 2 (written after the pilot of Amendment 1 and before any further campaign)

Pilot of Amendment 1 point 1 (Windows, `wild` and `phase`, 12 blocks x minimum of 3, every judge process launched
through `wnice -p high --`; `docs/perf/wild2/win_pilot_high_raw.jsonl.gz`): the A/A interval of `wild` was
0.916 (0.790-1.062) and of `phase` 0.961 (0.878-1.053), no narrower than at normal priority. A raised priority does
not reduce the noise here, so the Windows confirmation campaign of Amendment 1 is **not run**: it would be
unreadable by the same rule. The pilot is reported with the other data.

The one confirmation campaign of section 3 moves to Linux/WSL2, the platform where campaign 1 was readable
(A/A interval of `wild` 0.988 (0.953-1.024)): 24 blocks x minimum of 5 repeats, scenarios `wild`, `phase`, `burst`,
`regrow`, `growmix`, `large` (the scenarios in which the second chance fires; `grow64` and `grow1k` free in
allocation order, never miss the memo and keep their campaign-1 reading), scale 1 for `wild`, `phase`, `burst`,
`large` and 0.5 for `regrow` and `growmix`; t quantile for df = 23 is 2.069. Rules T2 and C2 are unchanged and apply
to this campaign. `regrow` is reported as a secondary result (it is not a preregistered target; campaign 1 read it as
a possible gain on both platforms). Also added (deterministic, supporting): callgrind Ir of `regrow` (Linux).
Still no third campaign.

## 8. Amendment 3 (written after the Linux confirmation campaign and its `perf stat`, before the layout control runs)

Observation: in the supporting `perf stat` of the default-layout Linux builds, `burst` has identical instruction
counts in base and x (985.4 M) but user-mode cycles of 297.8-303.0 M (base, five runs) against 305.3-340.1 M (x,
five runs), median +6.5 %; `phase` +2.6 % at equal instructions (samples overlap). The memo-hit path is
instruction-identical (C6) but sits at a different address in x, so a code-layout effect is the first suspect (the
repository's earlier campaigns used aligned-code builds for `bench:table` for exactly this reason). The judge's
`burst` reading of the same builds is 1.012 (0.994-1.030).

Layout control (a measurement added to the report, not a replacement of any gate; T2 stays defined on the default
builds): rebuild base and x (same commits) with `RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C
llvm-args=-align-all-nofallthru-blocks=5"` under Linux, rerun the Linux confirmation design (24 blocks x minimum of 5
repeats; `wild`, `phase`, `burst`, `regrow`, `growmix`, `large`) and the `perf stat` set (5 runs per arm; `wild`,
`phase`, `burst`). Reading rule: if the `burst` and `phase` cycle excess disappears in the aligned build while
`wild` keeps its gain, the excess is layout; if it stays, it is a cost of the change and is reported as such. No
other analysis is added after seeing these data.
