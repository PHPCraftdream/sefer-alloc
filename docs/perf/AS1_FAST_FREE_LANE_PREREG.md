# AS-1 — own-thread small free accept-only fast lane: pre-registration

Written and committed before the confirmatory data.

**Change (this branch).** `HeapCore::dealloc` first runs `dealloc_fast`
(`src/registry/heap_core/free/dealloc_fast.rs`, design A-S of the @ox memo).
The lane accepts a free only on a tier-1 memo hit, for a Small or Primordial
segment, with the same checks as `small_free_guard`: payload start, bump,
16-byte alignment and `state == encode(c, LIVE)`, class from the Layout. It
then pushes to the magazine (`cnt < FREE_PARK_CAP[c]`) or calls the
full-magazine tail. Every other case tail-calls the unchanged chain
(`dealloc_routing`), which re-derives everything, so rejects and their
counters keep the original paths. Compiled out under `hardened`.

**Mechanism (screening).** WSL disassembly of `b4_pmu_probe::sefer_dealloc`
drops from about 69 instructions with three pushes to 59 with one. In the
probe `inter` ABBA (10 repeats), the in-process sefer/mi ratio improves by
4.5–5.5 % in every scenario (pair, single-class churn, cross-class).

**Judge.** `node scripts/bench-table-paired.mjs --features production`
(3 processes × 200 rounds), aligned code
(`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`).
Two ABBA sets: base, as1, as1, base, then as1, base, base, as1.

**Rule (magnitude rule, derived by `node scripts/g1_magnitude_verdict.mjs`, applied to each set).**
Decided on the sefer time between the builds. GO if all of the following hold:

1. the geometric mean over the 12 cells is ≤ −0.5 %;
2. the agreed wins are ≥ 2 × the agreed losses;
3. the largest agreed loss is ≤ 3 %;
4. no cell turns into the LOSS class against mimalloc (sefer/mi > 1.03).

The variant is GO if both sets are GO. Before a merge the MT controls are
reported as well: larson/mstress T = 1, 2, 4 (`examples/malloc_macro.rs`).
