# R-Item 56 (task #1091) — the ~10-13% Ir regression attributed: one-time `#[repr(C)]` init cost (5df56d3) + a bench-internals-gated counter artifact (5289c66); nothing to fix in production

Date: 2026-09-01.

**Task:** #1091, closing item 56 of `docs/perf/OPEN_ITEMS.md` (filed 2026-08-18, task #1090).

**Measurement-only verdict (R30-12).** This investigation changed NO runtime code.
Every artifact here is `bench`-family work: derivation scripts, a report, and
doc updates. The two "counterfactual" measurements were produced by reverting
committed file contents in throwaway worktrees; nothing was landed.

**Host / toolchain identity.** Local WSL Callgrind via `scripts/iai.mjs`:
`valgrind-3.22.0`, `iai-callgrind-runner 0.14.2`, local
`rustc 1.98.0-nightly (bd08c9e71 2026-06-25)`, Windows dev host driving WSL.
CI (the runs that found the regression) used `rustc 1.97.1 (8bab26f4f
2026-07-14)`.

**Feature set.** Every per-commit measurement used `scripts/iai.mjs`'s
per-commit default `DEFAULT_FEATURES = 'production bench-internals internals'`.
This matters: §5's per-call residual exists **only because** the judge builds
with `bench-internals` (see §5, §7, §9-ii).

**Determinism statement.** Callgrind `Ir` counts are contention-independent;
every number below is read from exactly one committed raw log per measurement
(cited per row in the summary CSV). Run-to-run `Ir` jitter on this setup is
observed at ±1-11 Ir for the small churn/probe arms (§4b), a few×10 Ir on
larger helper arms, and up to ±262 Ir on the red-only large-cache arms.

**Immutable identity (R29-6 note, stated honestly).** The endpoints and every
bisect step were measured at COMMITTED clean trees — identity = the commit SHA
itself (`git rev-parse`-reproducible). The two counterfactual trees are
base-SHA + exact blob-swap recipes whose content identities are the cited
blob SHAs (section `identity` rows of
`docs/perf/R56_ITEM56_IR_REGRESSION_ATTRIBUTION_summary.csv`, e.g.
`5df56d3^:src/registry/tcache.rs` = `99648c0cf0e41c152f0fa87d094fc0a080446ada`).
These identities were derived AFTER the fact from the committed git objects via
`git rev-parse <commit>:<path>` — exactly reproducible, but NOT captured as
pre-measurement write-tree hashes at measurement time. That deviation from
R29-6's preferred capture-order is noted here rather than papered over; the
blob SHAs above nonetheless pin the exact bytes that were compiled in both
counterfactual runs.

## 0. Executive verdict

**Item 56 is RESOLVED.** The regression decomposes exactly into two named
commits and nothing else, and NEITHER half is a production defect:

- **(b) Accepted cost — the bulk.** `5df56d3` (`fix(perf): PerClass gains
  #[repr(C)]`, R32-5, task #496) added a **one-time +759 Ir per `HeapCore`
  creation** (15.49 Ir × 49 size classes) and +772 Ir on the Large cycle. It
  is amortized setup, not per-operation cost — the per-op Ir/op and the 2N
  delta are byte-for-byte unchanged (§5). It is visible at all only because
  every gate bench builds a fresh `SeferAlloc`, so bootstrap dominates tiny
  benches (§8).
- **(c) Measurement artifact — the per-op residual.** `5289c66` (`perf(runtime):
  OWN_CACHE_SIZE 4->16 + Tier-1 hit/miss counter`, R32-10, task #501) added
  **+3.0 Ir per `contains_base` call** — but only inside
  `CONTAINS_BASE_TIER1_HITS`/`_MISSES`, which are `#[cfg(feature =
  "bench-internals")]`-gated. The gate's own judge compiles them IN
  (`production bench-internals internals`); a plain production build compiles
  them out. The +3/call is the gate instrument measuring itself, plus a small
  one-time +35..47 Ir fixed part from the `own_cache [*mut u8; 16]` widening
  (§5).

**No production regression to fix.** Residual recommendations are process
changes only (§9).

## 1. Method + endpoint validation

Same method as every prior Ir gate: `scripts/iai.mjs` (WSL + valgrind/callgrind,
isolated `/tmp/sefer-iai` target dir), run per commit in clean worktrees.
The green endpoint log was cross-checked byte-identically against the repo's
recorded R22-15 baseline section in `docs/perf/IAI_BASELINE.md`:

| arm | green log | R22-15 baseline |
|---|---:|---:|
| `mimalloc_small_churn_16b` | 16,629 | 16,629 |
| `mimalloc_bootstrap_proxy` | 13,050 | 13,050 |
| `large_alloc_free_cycle` | 3,308 | 3,308 |
| `small_churn_16b` Ir/op | 74.1 | 74.1 |
| `small_churn_16b`/mimalloc Ir ratio | 1.326 | 1.326 |

The green endpoint is therefore not a stale-or-drifted artifact: the local
toolchain reproduces the recorded baseline exactly. All headline derivations
are re-derived and asserted by `scripts/item56_report_summary.mjs`
(self-asserting per the derived-not-hand-typed rule); exit 0 = all asserts
held.

## 2. Endpoint table

Range `42d8d223..42d42061`, 153 commits. Comparison per the committed
`docs/perf/R56_ITEM56_ENDPOINTS_summary.csv` (pct numerator AND denominator
named inline). Key arms:

| arm | green | red | delta | pct (delta/green) |
|---|---:|---:|---:|---|
| `large_alloc_free_cycle` | 3,308 | 4,132 | +824 | **+824/3,308 = +24.91%** |
| `small_churn_16b` | 8,051 | 9,035 | +984 | **+984/8,051 = +12.22%** |
| `churn_256b` / `medium_class_dealloc_churn_16b` | 8,051 | 9,035 | +984 | +984/8,051 = +12.22% |
| `aligned_churn_640b_a128` | 7,987 | 8,971 | +984 | +984/7,987 = +12.32% |
| `dealloc_flush_class_only_16b` | 4,338 | 5,832 | +1,494 | +1,494/4,338 = +34.44% |
| `dealloc_free_only_16b` | 12,983 | 14,566 | +1,583 | +1,583/12,983 = +12.19% |
| `small_churn_16b_2n` | 12,467 | 13,643 | +1,176 | +1,176/12,467 = +9.43% |
| every `mimalloc_*` arm | — | — | 0 | **0.00%** (control, §8) |

Whole-set: 79 compared arms; **25 of 79 > +10%**; 6 red-only arms
(`large_cache_*` / `alloc_zeroed_magazine_*`, added in-range — they have no
green baseline and are excluded from the pct census). CI's headline numbers
(`large_alloc_free_cycle` +29.00%, churn arms up to +13.8%) read larger than
local only because CI's `rustc 1.97.1` produced a slightly smaller codegen
denominator — same deltas, different base.

## 3. Bisect 1 — first bad commit: `5df56d3`

Predicate (`scripts/item56_bisect_predicate.mjs`): BAD iff either probe ≥
green+5%. Trace (per-step numbers parsed from the committed per-commit logs;
`_raw_item56_bisect_run.log` is the driver log):

| step | `small_churn_16b` | `large_alloc_free_cycle` | verdict |
|---|---:|---:|---|
| `94e133a` | 9,045 | 4,140 | BAD |
| `e3d01b2` | 9,045 | 4,140 | BAD |
| `2dfeaa3` | 8,810 | 4,080 | BAD |
| `e6bbc6a` | 8,055 | 3,312 | GOOD |
| `5d72bc6` | 8,055 | 3,312 | GOOD |
| `62e217f` | 8,055 | 3,312 | GOOD |
| `03a6c55` | 8,810 | 4,080 | BAD |
| **`5df56d3`** | **8,810** | **4,080** | **BAD — first bad commit** |

`5df56d376735933b3fb6c0097f5984771afab276` — "fix(perf): PerClass gains
`#[repr(C)]`" (R32-5, task #496).

## 4. Bisect 2 — the residual: `5289c66`

Residual predicate over `small_churn_16b` with an unambiguous band (GOOD ≤
8,900, BAD ≥ 8,980; no tested commit landed inside the band), logs
`_raw_item56_bisect2_run.log` + `_raw_item56_bisect2_<sha>.log`:

| step | `small_churn_16b` | verdict |
|---|---:|---|
| `454149e` | 9,045 | BAD |
| `f6c3a61` | 9,045 | BAD |
| `ce3f44d` | 8,810 | GOOD (5df56d3 level) |
| `c9a3570` | 9,037 | BAD |
| `a3e3e18` | 9,037 | BAD |
| **`5289c66`** | **9,037** | **BAD — first bad commit** |
| `e550006` | 9,035 | BAD (= red endpoint level) |

`5289c661877462f3caf6c4e136ad3c163f6fe15b` — "perf(runtime): OWN_CACHE_SIZE
4->16 + Tier-1 hit/miss counter" (R32-10, task #501).

### §4b. Red endpoint ≡ `5289c66` on the regression-signal arms

The equivalence is claimed ONLY for the arms that constitute item 56's
regression signal (all four are present at the green endpoint). Deviation,
red endpoint log vs `_raw_item56_bisect2_5289c66.log`:

| arm | red endpoint | `5289c66` log | dev |
|---|---:|---:|---:|
| `small_churn_16b` | 9,035 | 9,037 | +2 |
| `large_alloc_free_cycle` | 4,132 | 4,121 | +11 |
| `cold_alloc_free_256x16b` | 51,545 | 51,547 | +2 |
| `seg_cycle_decommit_256k` | 64,701 | 64,693 | +8 |

Scoping footnote (honest, not "every arm within ±11"): over the full
green-present arm set the max deviation is **+79 Ir** (`decomp_full_cycle_8x`,
9,409 vs 9,330 — ~0.8% of the arm, run-to-run address-layout jitter class),
with `decomp_os_roundtrip_8x` at +16. These two green-present arms sit outside
the probe signal and are **not fully attributed** — their residuals are small
relative to arm size and jitter-consistent, but no dedicated isolation was run
on them. Four arms deviate more and are RED-ONLY (added in-range, no green
baseline): `large_cache_hit_only_4mib` **+262** (7,206 → 7,468) and
`large_cache_prefill_only_4mib` **+232** (6,857 → 7,089). Those are honestly
attributed to the two in-range **post-`5289c66`** `perf(runtime)` large-cache
commits — `eb2463a449ca3497ce2761ee32f95cdc63bac321` (HIT arm writes 4
`SegmentHeader` fields instead of the whole 144-byte struct, task #498) and
`e88390bc88c863c8861d8bdda26fb49269cf9a89` (occupancy bitmask replaces
free-slot linear scan, task #503) — which reshaped exactly those paths after
the bisect's endpoint. They are outside item 56's regression set and outside
both bisect predicates. The proof that the probe-arm residual is exactly
`5289c66` rests on the bisect-2 GOOD boundary (8,810 at `ce3f44d`) plus
counterfactual A' below, not on this equivalence table.

## 5. Decomposition — fixed vs per-op (2N arithmetic, asserted)

The `_2n` variants run exactly 2N operations of the same shape, so
`2n − 1n` isolates the per-op component: a fixed per-process cost cancels,
a per-op cost doubles.

**Component 1 — `5df56d3` (`repr(C)`): fixed only.**

- churn: 8,051 → 8,810 = **+759 fixed per process**
- `small_churn_16b_2n` delta: 13,226 − 8,810 = **4,416** = green's 12,467 −
  8,051 = **4,416** → **zero per-op cost** (asserted equal by the script)
- 759 / 49 classes = **15.49 Ir/PerClass** — the `PerClass::new()` init inside
  `HeapCore` construction, paid once per `SeferAlloc` creation
- `large_alloc_free_cycle`: 3,308 → 4,080 = **+772** (same one-time signature)

**Component 2 — `5289c66` (own-cache + counters): tiny fixed + per-call counter cost.**

- churn: 9,037; `2n` = 13,645 → 2n-delta **4,608** = green 4,416 + **192**
  over the 64 extra pairs = **+3.0 Ir/pair**
- fixed part beyond `5df56d3`: 9,037 − 8,051 − 759 − 192 = **+35** (churn arm)
  … **+47** (`dealloc_hash_contains_only_probe_16b`: 9,783 − 9,736) — the
  `SegmentTable` `own_cache [*mut u8; 16]` vs `[*mut u8; 4]` zero-init widening
- per-arm isolation: `dealloc_contains_base_probe_only_16b` (Tier-1 reached
  THROUGH `contains_base`) grew **+239** (9,491 → 9,730) while
  `dealloc_hash_contains_only_probe_16b` (Tier-2 hash called DIRECTLY,
  bypassing `contains_base`) grew only **+47 ≈ fixed-only** → the per-call
  cost sits INSIDE `contains_base`: (239 − 47)/64 = **+3.0 Ir/call**. That is
  `5289c66`'s `#[cfg(feature = "bench-internals")]`
  `CONTAINS_BASE_TIER1_HITS`/`_MISSES` `AtomicU64::fetch_add` per call
  (lock `xadd` + address setup ≈ 3 instructions) — and `scripts/iai.mjs`
  builds WITH `bench-internals`, so the "measurement-only" counters are
  compiled into the measured binary.

Round-trip assert (held): 8,051 + 759 + 192 + 35 = 9,037.

## 6. Counterfactuals

**A — `tcache.rs` reverted at `5df56d3`**
(`_raw_item56_counterfactual_5df56d3_reverted.log`; reverted-to blob
`5df56d3^:src/registry/tcache.rs` = `99648c0cf0e41c152f0fa87d094fc0a080446ada`):
`small_churn_16b` **8,055** (+4 vs green), `large_alloc_free_cycle` **3,312**
(+4), `dealloc_flush_class_only_16b` **4,343** (+5). **0 of the compared arms
> +10%**; max abs deviation +5 Ir. The entire bisect-1 jump is the one file.

**A' — the four `5289c66` src files reverted at `5289c66`**
(`_raw_item56_counterfactual_5289c66_reverted.log`; reverted-to blobs
`5289c66^:src/alloc_core/segment_table.rs` = `0b480a7a18f204f0aa64af6c734fd41799c8f70c`,
`5289c66^:src/alloc_core/alloc_core.rs` = `9430055a2c4f008b88c60be3b89d442f3c3d750c`,
`5289c66^:src/alloc_core/alloc_core_core_diag.rs` = `d97faa597bd34002e394c1eb480ae316bba44b5d`,
`5289c66^:src/registry/heap_core_diag.rs` = `0e6d665045527ca24aead2c12ba889272d756256`):
every compared arm is byte-identical to the `5df56d3` level — max abs deviation
**1 Ir** across the whole common-arm set (29 arms sit at exactly +1, the rest
+0), e.g. `dealloc_contains_base_probe_only_16b` 9,491 → 9,492. `5289c66`'s
per-op and fixed residuals are 100% contained in its own four files.

## 7. Mechanisms

**Component 1 — `repr(C)` array-materialization lowering.** Standalone WSL
callgrind probe (`/tmp/probe_reprc/`), `[PerClass; 49]`-shaped array
construction ×100: slots-first (default `repr(Rust)`) layout **415,075 Ir** vs
count-first `#[repr(C)]` **560,560 Ir**. Per-function attribution: the delta
sits in `__memcpy_avx_unaligned_erms` (66,208 vs 213,220) — the array-repeat
construction is memcpy-lowered differently under the two layouts; count-first
emits more libc memcpy traffic. **Caveat, stated honestly:** this probe
amplifies per-element magnitude (~2,969 Ir/element vs the real ~15.5) because
its `black_box` forces a full copy per construction. It establishes the
MECHANISM CLASS (layout → array-materialization lowering), not the magnitude.
Notably, `5df56d3`'s own commit message SAW the same signature — a uniform
+755/+804 across three structurally different churn benches, attributed to
"the one-time HeapCore::new() zero-init codegen shift (PerClass::new() × 49
classes)" — while quoting "0 Ir delta" from isolated magazine-pop probe arms
only (§9-iii).

**Component 2 — own-cache size constant is Ir-neutral; the counters are not.**
Asm A/B probe (`/tmp/probe_cachemask/`, objdumps saved): `OWN_CACHE_SIZE` 4 vs
16 emits an IDENTICAL 7-instruction hit path — only `and $0x3` vs `and $0xf`
differs. Combined with the hash-bypass isolation of §5 (+47 fixed-only vs
+239), the +3/call is the bench-internals-gated Tier-1 hit/miss counters (one
Relaxed `fetch_add` per Tier-1 hit arm; `lock xadd` + address setup ≈ 3
instructions). Plain production builds without `bench-internals` compile them
out — real production Ir is unaffected by the +3/call part. `5289c66`'s own
commit message documents the standing ±10 raw-Ir churn gate NOT run ("argued,
not measured, to stay flat") — the argument was correct for the size constant
and WRONG for the counters it shipped in the same commit (§9-iii).

## 8. Why the gate showed 10-13% / 29%

The gate's churn arms total only **3,308-9,000 Ir** — bootstrap-dominated
denominators. A fixed +759 Ir of one-time `HeapCore` init lands as +23% on a
3,308-Ir arm and +12% on an 8,051-Ir arm, while the +3/call counter cost
scales the probe arms further. The control proves the attribution: every
`mimalloc_*` arm — C-compiled, untouched by both commits — reads **0.00%**
exactly, across all 79 compared arms' census. There is no environment drift;
there are two commits' worth of (mostly one-time / gate-only) Ir.

## 9. Decision + recommendations

**Decision: item 56 RESOLVED — outcome (b) accepted-cost for the bulk +
(c) measurement-artifact for the per-op residual. No production code change.**

1. **Accept +759** as the documented cost of the `repr(C)`
   documentation-correctness fix (it restored an already-decided task #53
   invariant, pinned by compile-time `offset_of!` asserts). Option NOTED for a
   future owner-approved micro-fix, NOT landed here, UNTESTED: const-fold
   `classes: [PerClass::new(); SMALL_CLASS_COUNT]` into a `const` item to move
   the init to rodata. Any future attempt must re-run the full 2N decomposition
   of §5 before claiming a win.
2. **The +3/call counter cost argues the gate's instruments should not live in
   the judge's build**: either gate such counters behind alloc-stats-style
   runtime switches or exclude `bench-internals` from
   `scripts/iai.mjs`'s `DEFAULT_FEATURES` for the judge arms that measure the
   instrumented path. Owner decision, tied to the separate threshold-restore
   decision already flagged in the item 56 card (the nightly trigger stays
   off; the self-lock must be fixed before any re-enable).
3. **Gate-process lessons from both commits:** `5df56d3` quoted "0 Ir delta"
   from isolated magazine-pop arms while its own message recorded the +755
   churn shift (the isolation arms didn't cover `HeapCore` construction);
   `5289c66` skipped the standing ±10 raw-Ir gate explicitly ("argued, not
   measured"). Rule of thumb going forward: a perf-family commit that cannot
   run the raw-Ir gate must say what WOULD falsify its flatness claim, and an
   isolation arm set must cover every constructor the commit touches, not only
   the hot path.

## 10. Raw-log citation list + summary CSV

All under `docs/perf/`, all committed:

- Endpoints: `_raw_item56_endpoint_42d8d223.log`, `_raw_item56_endpoint_42d42061.log`;
  comparison `R56_ITEM56_ENDPOINTS_summary.csv` (derived by
  `scripts/item56_compare_endpoints.mjs`)
- Bisect 1: `_raw_item56_bisect_run.log` + `_raw_item56_bisect_{94e133a,e3d01b2,2dfeaa3,e6bbc6a,5d72bc6,62e217f,03a6c55,5df56d3}.log`;
  predicate `scripts/item56_bisect_predicate.mjs`
- Bisect 2: `_raw_item56_bisect2_run.log` + `_raw_item56_bisect2_{454149e,f6c3a61,ce3f44d,c9a3570,a3e3e18,5289c66,e550006}.log`
- Counterfactuals: `_raw_item56_counterfactual_5df56d3_reverted.log`,
  `_raw_item56_counterfactual_5289c66_reverted.log`
- This report's derive script (all §2-§6 numbers asserted in-script):
  `scripts/item56_report_summary.mjs` →
  **`docs/perf/R56_ITEM56_IR_REGRESSION_ATTRIBUTION_summary.csv`**
  (113 rows: endpoint/validation/bisect/decomposition/counterfactual/equivalence
  facts + the `identity` blob/commit SHA rows cited in the header).
