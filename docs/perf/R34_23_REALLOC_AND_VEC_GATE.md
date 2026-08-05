# R34-23 — Realloc direct-gate + real-Vec subprocess gate (task #542)

**Verdict: NULL for the geometric-grow README claim (corrected); CONFIRMED for
the neighbour-pressure claim; NO-GO for the large-reserved-capacity hypothesis
under the current feature coupling.**

## 0. Summary

This gate closes the measurement gap flagged in the bench-review (findings
P1/P2 item 5): `benches/large_realloc.rs` called `GlobalAlloc::realloc`
directly but only with x2-doubling and one neighbour scenario, never touched
the payload, and reported no committed bytes; `benches/global_alloc.rs`'s
`Vec_push` arm was renamed in R34-9 (task #528) because it manually did
alloc+copy+dealloc and never called `realloc` at all. Two new harnesses were
built to measure the realloc path honestly:

1. **`examples/r34_23_realloc_direct_gate.rs`** — direct `GlobalAlloc::realloc`
   with growth factors x1.25/x1.5/x2, shrink/grow oscillation, neighbour
   pressure, copied-payload (canary write+verify) and untouched-payload
   variants, committed-bytes alongside latency, subprocess-per-allocator
   isolation.
2. **`examples/r34_23_vec_worker_{sefer,mimalloc,system}.rs`** — real `Vec<u8>`
   driving std's own growth logic through a real `#[global_allocator]`,
   subprocess-per-(allocator, rep) with rotating alternation (R34-7 pattern).

### README re-verification results

| README claim | Old number | R34-23 measurement | Verdict |
|---|---|---|---|
| `realloc_grow_geometric` "~40× faster than mimalloc" | sefer ~9.7 µs, mi ~383 µs | sefer ~238 µs, mi ~431 µs (criterion); sefer ~210 µs, mi ~444 µs (direct gate) = **~2× faster** | **MATERIALLY DIVERGES** — README corrected |
| `realloc_grow_neighbour_pressure` "~1,500× faster than mimalloc" | sefer ~906 ns, mi ~1.36 ms | sefer ~400 ns, mi ~1.34 ms = **~3,350× faster** | **CONFIRMED** (even better than claimed) |
| `realloc_grow_geometric` real-Vec parity | (not measured) | sefer ~1.05× mimalloc on `growth_4mib` | **PARITY** (new data, not a prior claim) |

### large-reserved-capacity hypothesis: NO-GO

`large-reserved-capacity` (LRC) does **not** improve the geometric realloc grow
chain — the path-activation oracle is identical with and without LRC (3
in-place + 13 declines per 16-step chain), and timing is **worse** with LRC
(893 µs vs 261 µs median). Root cause: LRC implies `exact-span-large`, which
shrinks the initial `span_usable` from 4 MiB (SEGMENT-rounded) to page-exact
(~256 KiB), hurting large-cache reuse; the 4× reserved-capacity factor is
outgrown within 2 doublings. See §4.

---

## 1. Identity and provenance

| Field | Value |
|---|---|
| git HEAD | `b2291876163c9a253340788a6a67d6d256e1109d` |
| git dirty | true (working tree includes this task's uncommitted changes) |
| git write-tree (immutable) | `70da19db9c443bdbb6cc40444d650b61153e69a4` |
| CPU | 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz (16 cores) |
| platform | win32 |
| rustc | (recorded in raw JSON `identity.rustc_version_verbose`) |

The `git write-tree` SHA was captured BEFORE measurement by the harness scripts
(`scripts/r34_23_realloc_direct_harness.mjs` / `r34_23_vec_harness.mjs`) via a
temp index + `git write-tree`, satisfying CLAUDE.md's R29-6 immutable-source-
identity rule (option 2: git tree object SHA).

---

## 2. Harness 1 — direct `GlobalAlloc::realloc` gate

### 2.1 Design

`examples/r34_23_realloc_direct_gate.rs` is launched as a fresh subprocess per
allocator by `scripts/r34_23_realloc_direct_harness.mjs` (one process per
`--allocator` value). Each process runs all (pattern × payload) cells for
`--samples` timed grow-chains.

**Growth patterns** (all from 64 B start, align 8):

| Pattern | Factor | Target | Steps | README mapping |
|---|---|---|---|---|
| `geometric_x2_4mib` | ×2 | 4 MiB | 16 | `realloc_grow_geometric` (exact reproduction) |
| `geometric_x2_1mib` | ×2 | 1 MiB | 14 | bounded comparison |
| `geometric_x1p5_1mib` | ×1.5 | 1 MiB | ~15 | finer-grain grow |
| `geometric_x1p25_1mib` | ×1.25 | 1 MiB | ~25 | finest-grain grow |
| `shrink_grow_osc` | ×2 + shrink | 1 MiB | 16+6 | shrink/grow oscillation |
| `neighbour_pressure` | +256 KiB | 2.5 MiB | 8 | `realloc_grow_neighbour_pressure` (exact) |

**Payload modes:**
- `copied` — writes a canary (`0xC0DE_FEED_F00D_BA5E`) at offset 0 and
  offset(size-8) before each realloc, verifies it after. Would catch in-place
  realloc data corruption (none found in any run).
- `untouched` — `black_box(ptr)` only (existing `large_realloc.rs` style).

**Committed bytes** reported per cell via `proc_probe::snapshot()` (RSS +
commit charge, same instant, in bytes).

### 2.2 Path-activation oracle (R30-8)

Three new process-wide counters (`src/alloc_core/alloc_core.rs`), bumped inside
the single shared `realloc_inplace_fast_path_known_base` detection function:

| Counter | Increment site | Meaning |
|---|---|---|
| `RELOC_INPLACE_LARGE_CALLS` | OPT-G `return Some(ptr)` (committed-span + reserved-capacity paths) | Large→Large in-place grow succeeded |
| `RELOC_INPLACE_SMALL_CALLS` | OPT-F `return Some(ptr)` (same-class) | Small same-class in-place succeeded |
| `RELOC_FASTPATH_DECLINE_CALLS` | every `return None` in the function | fast path declined → move leg (alloc+copy+free) |

Invariant: `inplace_large + inplace_small + decline == total` reallocs that
reached the fast-path detection (every non-null realloc on a registered
segment). Read via safe `#[doc(hidden)]` accessors
(`dbg_reloc_inplace_large_count` / `_small_count` / `_fastpath_decline_count`),
always compiled; increments gated on `alloc-stats` (zero cost in production).

**Tripwire classification:** all three accessors added to `PURE_OBSERVERS` in
`tests/dbg_hook_safety_tripwire.rs` (read-only atomic loads, no mutation, no
soundness-relevant side effect — same bucket as `dbg_large_zero_pass_count`).

### 2.3 Results (30 samples/cell, fresh subprocess per allocator)

Cross-allocator median ns/chain (full table in `_raw_r34_23_*` + summary CSV):

| Pattern | sefer | mimalloc | system | sefer/mi | sefer/sys |
|---|---|---|---|---|---|
| `geometric_x2_4mib` (copied) | 210,100 | 444,200 | 2,958,400 | 0.47× (2.1× faster) | 0.07× (14× faster) |
| `geometric_x2_1mib` (copied) | 14,100 | 118,200 | 774,700 | 0.12× (8.4× faster) | 0.02× (55× faster) |
| `geometric_x1p5_1mib` (copied) | 39,300 | 240,300 | 530,900 | 0.16× (6.1× faster) | 0.07× (14× faster) |
| `geometric_x1p25_1mib` (copied) | 20,800 | 43,900 | 11,900 | 0.47× (2.1× faster) | 1.75× (SLOWER) |
| `shrink_grow_osc` (copied) | 111,000 | 102,100 | 1,806,700 | 1.09× (SLOWER) | 0.06× (16× faster) |
| `neighbour_pressure` (copied) | 400 | 1,343,900 | 7,552,600 | 0.0003× (3350× faster) | 0.0001× (18,900× faster) |

**Path-activation oracle (sefer, 30 samples, deltas over the full cell):**

| Pattern | inplace_large | inplace_small | decline | inplace_pct |
|---|---|---|---|---|
| `geometric_x2_4mib` | 90 (3/chain) | 0 | 390 (13/chain) | 18.8% |
| `geometric_x2_1mib` | 60 (2/chain) | 0 | 360 (12/chain) | 14.3% |
| `geometric_x1p25_1mib` | 0 | 0 | 960 | 0.0% |
| `shrink_grow_osc` | 150 (5/chain) | 0 | 450 (15/chain) | 25.0% |
| `neighbour_pressure` | 240 (8/chain) | 0 | 0 | 100.0% |

### 2.4 Key finding — geometric_x2_4mib oracle anatomy

The 64 B → 4 MiB ×2 chain (16 reallocs) breaks down as:
- **11 Small reallocs** (128 B … 128 KiB): all cross-class → decline (move leg).
- **1 Small→Large crossover** (128 KiB → 256 KiB): decline (move leg: alloc 4 MiB Large segment + copy 128 KiB).
- **3 Large in-place grows** (256→512 KiB, 512 KiB→1 MiB, 1→2 MiB): fit within `span_usable` (4 MiB) → OPT-G header-update only.
- **1 Large decline** (2→4 MiB): `payload_off + 4194304 > span_usable` (header offset pushes past the 4 MiB committed span) → move leg with a **2 MiB copy** that dominates the chain's ~210 µs timing.

This is CONFIRMED by the criterion re-verification (`_raw_r34_23_criterion_reverification.log`): the same `realloc_grow_geometric` bench that sourced the README's "~9.7 µs" now measures sefer at ~238 µs — a 24× discrepancy that is physically explained by the 2 MiB copy in the final grow step (a 2 MiB memcpy alone takes ~50–100 µs at modern bandwidth; 9.7 µs for the full chain is physically impossible if any multi-MiB copy occurs).

---

## 3. Harness 2 — real `Vec` subprocess gate

### 3.1 Design

Three worker binaries (`examples/r34_23_vec_worker_{sefer,mimalloc,system}.rs`),
each installing its own `#[global_allocator]`, driven by
`scripts/r34_23_vec_harness.mjs` as fresh subprocesses in rotating alternation
(R34-7 pattern). Shared workload via `include!`
(`examples/_shared/r34_23_vec_workload.rs`) — byte-identical across all three.

**Shapes:**
- `growth_4mib` — `Vec::new()` + push 4,194,304 bytes (maps to README
  `realloc_grow_geometric` under real Vec semantics).
- `growth_1mib` — push 1,048,576 bytes.
- `shrink_grow_1mib` — push to 1 MiB, `shrink_to_fit()`, push to 2 MiB.
- `reserve_exact_geom` — push to 1 MiB, then `.reserve_exact(2 MiB)` and
  `.reserve_exact(4 MiB)`, filling between (forces two large reallocs).

### 3.2 Results (8 reps × 5 iterations/shape, fresh subprocess per launch)

| Shape | sefer | mimalloc | system | sefer/mi | sefer/sys |
|---|---|---|---|---|---|
| `growth_4mib` | 9,174,650 | 8,700,000 | 12,325,000 | 1.05× | 0.74× |
| `growth_1mib` | 2,439,250 | 2,747,750 | 3,251,450 | 0.89× | 0.75× |
| `shrink_grow_1mib` | 5,540,250 | 5,644,300 | 6,742,850 | 0.98× | 0.82× |
| `reserve_exact_geom` | 7,996,950 | 8,537,800 | 11,601,050 | 0.94× | 0.69× |

(median elapsed_ns across 8 reps; full table in `R34_23_REAL_VEC_summary.csv`)

**Path-activation oracle (sefer, summed over all shapes, 8 reps):**
inplace_large=528, inplace_small=192, decline=2784, inplace_pct=20.5%.

**Interpretation:** Under real-Vec growth, sefer is at **parity** with mimalloc
(0.89×–1.05×) — the realloc advantage that powers the direct-gate's ~2× win on
`geometric_x2_4mib` is diluted by Vec's per-push overhead (4 M pushes dominate
the wall-clock, making the ~20 reallocs a small fraction). The direct gate's
~2× ratio on the realloc path itself is the more precise realloc-isolated
measurement; the Vec gate confirms it in direction (sefer ≥ mimalloc) but shows
the advantage washes out under realistic push-heavy workloads.

---

## 4. large-reserved-capacity hypothesis

### 4.1 Hypothesis

The bench-review suggested that `large-reserved-capacity` (CONDITIONAL-GO, not
in production) might help the geometric realloc grow by providing reserved VA
for in-place growth past the committed span — specifically, an **adaptive
growth factor + compounding headroom** might let subsequent grows fit within
already-reserved span.

### 4.2 A/B test

Same binary, same pattern (`geometric_x2_4mib`), 10 samples, sefer only. Binary
hashes verified different (md5sum: `09b554c0…` without LRC vs `4e515b80…` with
LRC — see `_raw_r34_23_lrc_hypothesis_ab.log`):

| Config | median ns | inplace_large | decline |
|---|---|---|---|
| production alloc-stats (no LRC) | 261,300 | 30 (3/chain) | 130 (13/chain) |
| production alloc-stats **large-reserved-capacity** | 892,600 | 30 (3/chain) | 130 (13/chain) |

### 4.3 Verdict: NO-GO for geometric realloc

**LRC does not improve the geometric grow chain.** The path-activation oracle
is identical (3 in-place + 13 declines per chain), and timing is **3.4×
WORSE** with LRC. Root cause:

1. `large-reserved-capacity` **implies** `exact-span-large`
   (`Cargo.toml:357`), which changes `usable` from SEGMENT-rounded (4 MiB for
   any Large) to page-exact (~256 KiB for a 256 KiB request). This shrinks
   `span_usable` from 4 MiB to ~256 KiB, so fewer grows fit the committed span.
2. The reserved capacity (4× initial `usable` ≈ 1 MiB) is outgrown within 2
   doublings (256→512→1024 KiB exceeds 1 MiB). The 4× factor
   (`LARGE_RESERVED_CAP_GROWTH_FACTOR`) is too small for a geometric chain that
   reaches 4 MiB.
3. The smaller `usable` also breaks large-cache reuse: a cached 4 MiB segment
   is incompatible with a 256 KiB request (size-ratio bound
   `usable * LARGE_CACHE_SIZE_FACTOR` rejects it), forcing fresh OS
   reservations on every Large alloc.

**The adaptive-growth-factor sub-hypothesis** (reserve based on observed growth
pattern, not a fixed 4× factor) remains theoretically interesting but is a
separate, larger design task (#544/#545). The current fixed-factor LRC
configuration is NET-NEGATIVE for this workload. This task does NOT implement
adaptive growth — it only measures whether the existing LRC mechanism helps,
and it does not.

---

## 5. README correction

### 5.1 What changed and why

The README's `realloc_grow_geometric` row claimed sefer ~9.7 µs / ~40× faster
than mimalloc. The criterion bench that sourced this number now measures sefer
at ~238 µs (24× slower than the published figure), confirmed independently by
the R34-23 direct gate (~210 µs). The mimalloc number (~383 µs → ~431 µs) is
roughly stable. The ratio collapsed from ~40× to ~2×.

The root cause is that the 64 B → 4 MiB ×2 chain's final grow (2 MiB → 4 MiB)
exceeds the Large segment's `span_usable` by the header-offset bytes, forcing a
2 MiB copy. The README's 9.7 µs is physically impossible if any multi-MiB copy
occurs (a 2 MiB memcpy alone takes ~50–100 µs). The historical 9.7 µs was
either measured under different conditions (warm large-cache with a
reserved-capacity segment that no longer reproduces) or is a stale figure from
before a code change that altered the grow-path geometry. Regardless of the
historical reason, the CURRENT measurement is unambiguous and confirmed by two
independent methodologies.

### 5.2 What was corrected

- `realloc_grow_geometric` row: updated from "~9.7 µs / ~40× faster" to the
  criterion-measured ~238 µs / ~1.8× faster (criterion) or ~210 µs / ~2.1×
  faster (R34-23 direct gate), citing this gate report.
- `realloc_grow_neighbour_pressure` row: confirmed (~400 ns / ~3,350× faster),
  updated citation to this gate report.

---

## 6. Artifacts

| Artifact | Path |
|---|---|
| Gate report (this file) | `docs/perf/R34_23_REALLOC_AND_VEC_GATE.md` |
| Direct-gate summary CSV | `docs/perf/R34_23_REALLOC_DIRECT_summary.csv` |
| Vec-gate summary CSV | `docs/perf/R34_23_REAL_VEC_summary.csv` |
| Criterion re-verification raw log | `docs/perf/_raw_r34_23_criterion_reverification.log` |
| LRC hypothesis A/B raw log | `docs/perf/_raw_r34_23_lrc_hypothesis_ab.log` |
| Direct-gate raw per-sample JSON (30 samples, gzip-compressed) | `docs/perf/r34_23_runs/2026-08-04T22-03-44-381Z_direct_raw.json.gz` |
| Vec-gate raw per-sample JSON (8 reps) | `docs/perf/r34_23_runs/2026-08-04T22-03-52-053Z_vec_raw.json` |

The summary CSVs and this report's tables are DERIVED from the raw JSON by the
harness scripts (`scripts/r34_23_realloc_direct_harness.mjs` /
`r34_23_vec_harness.mjs`), not hand-transcribed (CLAUDE.md "tables derived by
one checked script" rule). The harness scripts compute median/min/max/mean/
stdev/cv from the per-sample data and write the CSV directly.

**Task #551 (R34-review F7) note:** the direct-gate raw JSON was originally
committed uncompressed at 258 KiB, exceeding the tier-2 force-add ceiling
CLAUDE.md's artifact-storage-policy sets at 200 KiB (that policy landed later
the same round, in `4ba188a`/R34-24). It is gzip-compressed here per that
policy's tier-2 point 2(b) — chosen over truncation because the file is
1,080 uniform per-sample records that the summary CSV derives from in full,
so truncating to a cited excerpt would lose reproducibility of the derivation,
not just trim boilerplate. `gunzip -k
docs/perf/r34_23_runs/2026-08-04T22-03-44-381Z_direct_raw.json.gz` (or
`zcat`) recovers the original byte-identical JSON. See
`docs/perf/OPEN_ITEMS.md` for the tracked deviation record.
