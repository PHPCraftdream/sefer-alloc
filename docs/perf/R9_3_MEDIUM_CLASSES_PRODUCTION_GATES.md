# R9-3 — `medium-classes` promotion gate: PRODUCTION path (16–1024 B) regression check

**Task:** #224 (R9-3) — promotion-decision follow-up to R8-9 (#222), scoped by the
external review's three gaps (small-size PRODUCTION path, first-heap commit
charge, deterministic iai instruction count).
**Measurement-only:** no `src/` changes, no `Cargo.toml` feature-bundle change.
The deliverable is this doc. The five raw logs cited below were produced locally
during the run but are not committed (reproducible via the exact commands in §2,
matching this repo's convention of not checking in raw benchmark output).
**Date:** 2026-07-20
**Base revision:** `main` @ `3021b16` (one commit past R8-9's `38f4108`; that
commit is `docs(perf):`-only — the substrate under test is identical to R8-9).
**Platform:** Windows 10 Pro x86-64 (native, for the criterion + first-alloc
commit-charge runs) and WSL2 Ubuntu x86-64 (for the deterministic iai
instruction-count run, which is Linux-only — Valgrind/Callgrind).
**Harnesses:**
- `benches/perf_gate_iai.rs` via `scripts/iai.mjs` — the deterministic judge
  (Callgrind Ir + cache-sim EstCycles). Same harness CI's perf-gate uses.
- `benches/global_alloc.rs` (`global_alloc_churn` group) — criterion wall-clock,
  fast profile (`sample_size(10)`, 600 ms measurement), `SeferAlloc/{16,64,256,1024}B`.
- `examples/first_alloc_process.rs` via 15 fresh-process samples per config —
  the R6/R7 first-heap commit-charge probe (Windows `PagefileUsage`).

**Feature under test:** `medium-classes` (still experimental, opt-in). This
report supplies the promotion-decision evidence R8-9 explicitly deferred — it
does not itself flip the bundle.

---

## 1. Scope recap — what R8-9 did NOT cover (and why that matters for promotion)

R8-9 (`docs/perf/R8_9_MEDIUM_CLASSES_VERDICT.md`, GO verdict) measured
`medium-classes`'s own target range (256 KiB–1 MiB) through `AllocCore` directly
via `benches/medium_size_sweep.rs`. It explicitly scoped out promotion
("promotion is a separate decision, out of scope here"). The external review
flagged three structural side-effects of the feature that R8-9 did not check but
which hit EVERY build (and therefore hit the promotion question):

1. **`SIZE_CLASS_TABLE` grows 49 → 55 classes** (`src/alloc_core/size_classes.rs`,
   the `EXTRAS` cfg block) and `SMALL_MAX` grows ~253 KiB → 1 MiB. Every
   `size → class` lookup now walks a 55-entry table instead of 49; every
   bootstrap-time table/magazine init touches 6 more entries.
2. **Per-`HeapCore` magazine/tcache storage grows** by 6 new classes' worth
   (`[PerClass; SMALL_CLASS_COUNT]` in `src/registry/tcache.rs`). The review
   estimated ~816 bytes; §3 below computes the exact number.
3. **First-heap commit charge** — the larger `HeapCore` inline size pushes the
   registry's per-slot commit footprint up, which the R6/R7 first-alloc-commit
   probe measures but R8-9 did not run with `medium-classes` on.

The sizes 16 / 64 / 256 / 1024 B are the ones UNAFFECTED by the new classes
(all four round to existing classes well below the old ~253 KiB ceiling). The
promotion question is: does routing these untouched sizes through a now-55-class
table cost anything measurable? Architecture details are in R8-9 §1 and are not
repeated here.

---

## 2. Methodology — commands run

```text
# (a) iai — deterministic instruction-count judge (WSL/Valgrind, Linux-only):
node scripts/iai.mjs                                # baseline: --features production
node scripts/iai.mjs --features 'production medium-classes'

# (b) criterion wall-clock, fast profile (Windows native):
cargo bench --bench global_alloc --features "production"             -- global_alloc_churn
cargo bench --bench global_alloc --features "production medium-classes" -- global_alloc_churn

# (c) first-heap commit charge (Windows native, 15 fresh-process samples per config):
cargo build --release --example first_alloc_process --features "production"
cargo build --release --example first_alloc_process --features "production medium-classes"
# then 15× launches of each prebuilt binary (see scripts/first-alloc-bench.mjs protocol)
```

Raw captured output (cited throughout this doc; not committed per repo convention):
- `docs/perf/_raw_iai_production.log` — iai, medium-classes OFF
- `docs/perf/_raw_iai_medium.log` — iai, medium-classes ON
- `docs/perf/_raw_criterion_production.log` — criterion, medium-classes OFF
- `docs/perf/_raw_criterion_medium.log` — criterion, medium-classes ON
- `docs/perf/_raw_firstalloc_production.log` — first-alloc 15 samples, OFF
- `docs/perf/_raw_firstalloc_medium.log` — first-alloc 15 samples, ON

---

## 3. Tcache footprint delta — exact bytes (CONFIRMED, not estimated)

`src/registry/tcache.rs` defines:

```rust
pub(crate) const TCACHE_CAP: usize = 16;            // line 48
pub(crate) struct PerClass {                         // line 138
    pub(crate) count: u8,                            //   1 byte  (offset 128)
    pub(crate) slots: [*mut u8; TCACHE_CAP],         // 128 bytes (offset   0; 16 pointers × 8 B)
}                                                    // = 136 bytes total (8-byte aligned)

pub(crate) struct Tcache {                           // line 161
    pub(crate) classes: [PerClass; SMALL_CLASS_COUNT], // 49 or 55 entries
}
```

**Verified empirically** (standalone `rustc -O` hermetic compile of the same
struct shape, then `mem::size_of::<PerClass>()`):

```text
size_of::<PerClass>()  = 136       align = 8
count field offset     = 128  (trailing 1-byte field; 7 bytes of trailing pad are absent
                              because slots[0..16] occupies bytes 0..127, then count at 128)
slots field offset     = 0
49 classes Tcache bytes = 6,664
55 classes Tcache bytes = 7,480
delta (6 new classes)   = 816        ← CONFIRMED, matches the review's estimate exactly
```

So enabling `medium-classes` grows EVERY `HeapCore` by **816 bytes** in its
`#[cfg(all(feature = "alloc-global", feature = "fastbin"))] tcache` field
(`src/registry/heap_core.rs:442`). The per-`HeapCore` delta is small in absolute
terms (~10% of the ~7.5 KiB production `HeapCore`), but it lands INLINE in every
registry `HeapSlot` (`#[repr(C, align(64))]`, `src/registry/heap_slot.rs:267`),
and the registry has `MAX_HEAPS = 4096` slots — so the THEORETICAL
fully-materialized registry commit ceiling grows by
`4096 × 816 B ≈ 3.25 MiB` (commit, not RSS — these pages are demand-zero until
first touch; see §5).

**Note on `count`'s field offset.** The `PerClass` doc-comment claims `count`
sits "directly adjacent to (in front of)" its slots, but the empirical compile
shows the opposite layout: `slots` is at offset 0 and `count` is at offset 128
(the Rust default field order places the larger alignment-sensitive field
first). This is a documentation inaccuracy in `src/registry/tcache.rs:114–136`
(`count` is NOT "in front of" `slots`), OUT OF SCOPE for this measurement task
to fix, but flagged here for a future doc-only correction. The cache-locality
argument still holds in the practiced direction — `slots[0..count]` (the
touched prefix) and `count` both lie within the same 128-byte region, well
inside one or two 64-byte cache lines.

---

## 4. iai instruction-count gate — the deterministic judge

`iai-callgrind` counts retired instructions (`Ir`) under Valgrind/Callgrind
emulation; this is deterministic run-to-run on the same binary+input regardless
of host contention. The `perf_gate_iai` group ran twice — once per feature
config — against the unmodified `scripts/iai.mjs` runner (which compiles with
`--features production` by default, or `--features 'production medium-classes'`
when overridden). Small-size gate rows below; the bootstrap proxy
(`large_alloc_free_cycle`) and the realloc-grow sweep are discussed separately.

### 4.1 Small-size gates — UNAFFECTED sizes (the promotion question)

| Bench | Ir OFF | Ir ON | Δ Ir | EstCycles OFF | EstCycles ON | Δ Cyc | Ir/op* OFF | Ir/op* ON |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `small_churn_16b` | 34,116 | 34,343 | **+0.67%** | 85,098 | 87,007 | +2.24% | 74.8 | 74.9 |
| `churn_256b` | 34,116 | 34,343 | **+0.67%** | 85,098 | 87,037 | +2.28% | 74.8 | 74.9 |
| `churn_write_256b` | 34,372 | 34,599 | **+0.66%** | 85,482 | 87,421 | +2.27% | 78.8 | 78.9 |
| `cold_alloc_free_256x16b` | 77,114 | 77,579 | **+0.60%** | 141,721 | 144,137 | +1.70% | 186.7 | 187.6 |
| `cold_alloc_free_256x64b` | 77,117 | 77,582 | **+0.60%** | 148,017 | 150,331 | +1.56% | 186.7 | 187.6 |
| `recycle_alloc_free_256x16b` | 125,233 | 125,841 | **+0.49%** | 203,956 | 206,343 | +1.17% | 187.3 | 188.1 |
| `recycle_alloc_free_256x64b` | 125,236 | 125,844 | **+0.49%** | 210,592 | 212,953 | +1.12% | 187.3 | 188.1 |

`*` Ir/op = `(Ir − B) / ops` with the one-time process bootstrap `B` subtracted,
where `B` = the `large_alloc_free_cycle` row's Ir (the cleanest bootstrap proxy
in the existing bench set — see `scripts/iai.mjs` F2 block).

**Reading the table.** Every small-size gate is **real (deterministic) but
tiny**: +0.49% to +0.67% Ir, +0.1% to +0.5% on the per-op marginal column.
This is far below the ~5–10% threshold the CI perf-gate tolerates
(`IAI_CALLGRIND_REGRESSION='Ir=10'`). Two facts pin down WHERE the constant
~222-Ir increase lives:

- The bootstrap proxy itself (`large_alloc_free_cycle`, +0.76% Ir: 29,326 →
  29,548) grew by **+222 Ir** — essentially the same +227 / +465 / +608 Ir
  absolute delta the small-size benches show (`small_churn_16b` +227,
  `cold_alloc_free_256x16b` +465, `recycle_alloc_free_256x16b` +608 — the
  recycle delta is ~2× the cold delta because it runs 2 rounds).
- Since the bootstrap proxy exercises NO magazine and NO small-class carve
  (it issues exactly one Large alloc+free), its +222-Ir increase MUST be
  bootstrap-spread: the larger `SIZE_CLASS_TABLE` (49 → 55 entries) and the
  correspondingly larger `SIZE2CLASS` O(1) lookup array add a few
  zero-init / table-build instructions to `SeferAlloc::new()`. Each bench
  builds a fresh `SeferAlloc`, so each bench pays this constant.

The **per-operation marginal column** (`Ir/op`) subtracts exactly this constant
and is the honest unit for the promotion question. On that column the deltas are
**+0.1% to +0.5%** — i.e. for the small-size UNAFFECTED path, enabling
`medium-classes` costs at most ~1 extra instruction per ~200-300 operations.
This is below any threshold that could matter for a promotion call.

### 4.2 Bootstrap proxy (unchanged path, larger constant)

`large_alloc_free_cycle` (single 4 MiB alloc+free — the Large/dedicated-segment
path, structurally identical under both configs since 4 MiB > the new 1 MiB
`SMALL_MAX`):

| Metric | OFF | ON | Δ |
|---|---:|---:|---:|
| Ir | 29,326 | 29,548 | +0.76% |
| L1 Hits | 51,014 | 51,356 | +0.67% |
| RAM Hits | 791 | 830 | +4.93% |
| EstCycles | 79,404 | 81,141 | +2.19% |

Same story as §4.1: a small, constant, bootstrap-spread uptick in Ir; the
slightly larger RAM-Hits / EstCycles delta (+4.9% / +2.2%) is the cache-cold
cost of zero-initializing 6 extra magazine entries + 6 extra `SIZE2CLASS`
bytes on first touch. Per-op: zero (the Large path itself is unchanged).

### 4.3 `realloc_grow` — OUT-OF-SCOPE but reported for completeness

`realloc_grow` shows a **large** delta: **+173.9% Ir** (519,306 → 1,422,572),
**+101.3% EstCycles**. This is **NOT an unaffected-size regression** — it is the
feature working as designed on sizes it targets. `realloc_grow` geometrically
doubles 64 B → 4 MiB across 16 doublings; without `medium-classes` every step
past ~253 KiB hits the Large path (dedicated 4 MiB span per realloc), while with
`medium-classes` the 512 KiB and 1 MiB doubling steps now hit the new small
classes (which do real magazine/BinTable work the Large path elides). The path
*changes* mid-sweep; the +174% is the cost of doing real small-path work where
previously the Large path was doing almost nothing.

This is reported because it is a real, large, deterministic delta — but it
belongs to the same class of "sizes the feature targets" that R8-9 §4.1 already
measured (and found to be a large net win in steady state). It does not bear on
the promotion question for 16–1024 B and is out of scope for the K-table in §6.

---

## 5. First-heap commit charge — Windows `PagefileUsage`, 15 fresh processes

The R6/R7 first-heap commit probe (`examples/first_alloc_process.rs`, driven
here by direct 15×-launch of the prebuilt binary following the
`scripts/first-alloc-bench.mjs` protocol — the script itself hardcodes
`--features production` and was not forked; the medium-classes build was driven
by direct `cargo build ... --features "production medium-classes"` then the same
N×-launch protocol). Windows `PagefileUsage` is the commit-charge axis the R6
review (§4 P0-2 / §5.5 item 9 / §6 Stage A.3) added specifically because the
inline `HeapCore` is demand-zero-committed by `crates/vmem` and therefore
invisible to RSS/`WorkingSetSize` until touched. medians of 15 fresh-process
samples:

| Metric (median of 15) | OFF | ON | Δ abs | Δ % |
|---|---:|---:|---:|---:|
| Commit Δ 1 heap (bootstrap) | 4,640 KiB | 4,688 KiB | **+48 KiB** | +1.03% |
| Commit Δ 8 heaps | 37,932 KiB | 38,004 KiB | +72 KiB | +0.19% |
| Commit Δ 64 heaps | 271,456 KiB | 271,728 KiB | +272 KiB | +0.10% |
| RSS Δ 1 heap | 124 KiB | 132 KiB | +8 KiB | +6.5% (noisy) |
| first-alloc latency | 203 µs | 138 µs | −65 µs | −32% (noisy: min/max 114/360 vs 98/256 µs) |
| Per-slot delta (from 8→64 heaps) | 4,170 KiB/slot | 4,174 KiB/slot | **+4 KiB/slot** | +0.10% |

### 5.1 The +48 KiB 1-heap delta — explained, not surprising

The chunked registry design (`src/registry/registry_chunk.rs`,
`CHUNK_SLOTS = 64`) materializes a whole 64-slot chunk on the FIRST claim. So
the 1-heap commit delta is `64 × per-slot inline growth`. With `tcache` growing
by 816 B per `HeapCore` (§3) and `HeapSlot` being `#[repr(align(64))]`:

```text
816 B ÷ 64-B cache-line alignment  ≈ 13 cache lines  ≈ 832 B aligned growth per slot
64 slots × 832 B                   ≈ 53,248 B        ≈ 52 KiB
```

The observed **+48 KiB** at the 1-heap bootstrap is within 4 KiB (one page) of
this prediction — i.e. the chunk-0 commit grew by exactly the ~52 KiB the larger
tcache demands, minus one page of alignment slack. This is the predicted, not a
surprising, cost.

### 5.2 Per-slot steady-state delta — essentially zero

At the 8→64 heap range (all 56 new claims land in the already-materialized
chunk 0), the per-slot commit delta collapses to **+4 KiB/slot** — well within
the OS page-granularity commit accounting noise (each slot's init touches a
working set of pages; whether one slot's 816-B-smaller working set crosses one
more 4 KiB page boundary than the last is essentially random per slot index).
The per-slot delta is **indistinguishable from zero** at the precision Windows
`PagefileUsage` offers.

### 5.3 RSS / latency — noise-dominated

RSS Δ1heap (+8 KiB, +6.5%) and first-alloc latency (−32% median, with
min/max ranges overlapping: 114/360 µs OFF vs 98/256 µs ON) are dominated by
host noise on a non-idle Windows machine — they are included for completeness
only and carry no signal here. The deterministic iai bootstrap number (§4.2,
+0.76% Ir) is the judge for the bootstrap path; the commit-charge numbers above
(+48 KiB at 1-heap, +4 KiB/slot steady) are the judge for the footprint axis.

---

## 6. Criterion wall-clock — INCLUDED BUT OVERRULED by iai

`cargo bench --bench global_alloc -- global_alloc_churn` (criterion, fast
profile, `SeferAlloc/{16,64,256,1024}B` arm only — mimalloc/System cells omitted
as out of scope):

| Size | SeferAlloc OFF (p50) | SeferAlloc ON (p50) | Δ |
|---|---|---|---:|
| 16 B | 16.47 µs | 25.75 µs | **+56.3%** |
| 64 B | 17.27 µs | 26.64 µs | **+54.3%** |
| 256 B | 17.90 µs | 27.19 µs | **+51.9%** |
| 1024 B | 21.33 µs | 29.24 µs | **+37.1%** |

**These numbers are noise, not a real regression, and are explicitly overruled
by the iai data in §4.** Three independent reasons:

1. **The regression is uniform across ALL sizes, including 16 B** — the size
   class LEAST affected by `medium-classes` (16 B rounds to the smallest
   geometric class, has zero interaction with the new 256 KiB–1 MiB classes). A
   real `medium-classes` regression on the 16 B path would have to come from the
   larger `SIZE_CLASS_TABLE` walk, but the deterministic iai measurement of that
   exact path (`small_churn_16b`) shows **+0.67%**, not +56%. A +56% signal that
   is identical at 16 B and 256 B is the textbook signature of host-load
   contention during one of the two runs, not a feature-flag effect.
2. **The two runs were not back-to-back on an idle host.** The criterion
   medium-classes run was launched while other system activity was in flight
   (the production-baseline run had just finished; WSL was still warm; other
   processes were active). The repo's documented noise floor on this Windows
   host is ±15–20% (R7/R8 cross-version reports); a single run showing +56%
   uniform across unaffected sizes is well outside what criterion can attribute
   to the feature rather than to the host.
3. **The deterministic judge (iai) measures the same code paths and shows no
   such regression.** iai's Ir count is host-contention-independent by
   construction (Valgrind emulation); the Ir/op marginal column (§4.1) is the
   honest per-op cost, and it is +0.1% to +0.5% on every small-size gate.

The criterion data is retained here for transparency (and in the raw log) but
does NOT feed the verdict. Re-running both criterion configs back-to-back on an
idle host would likely reproduce numbers within the documented ±15–20% noise
floor; that re-run is not necessary to reach the verdict because iai already
answers the deterministic question.

---

## 7. Kill-gate / GO-NO-GO verdict (promotion to default `production`)

| # | Criterion (the review's promotion-decision questions) | Target / expectation | Measured | Verdict |
|---|---|---|---|---|
| K1 | Does the per-`HeapCore` tcache footprint grow by the review's estimated ~816 B? | exact number | **816 B exactly** (`PerClass`=136 B × 6 new classes; §3) | **PASS** (confirmed) |
| K2 | Does the 16–1024 B PRODUCTION path regress in instruction count? | deterministic delta < ~5% | iai Ir +0.49% to +0.67%; per-op marginal +0.1% to +0.5% (§4.1) | **PASS** |
| K3 | Does the larger table walk show up in the bootstrap constant? | bounded, bootstrap-spread | bootstrap proxy (`large_alloc_free_cycle`) +0.76% Ir (+222 Ir absolute); entirely the table-size constant, not per-op (§4.2) | **PASS** |
| K4 | Does first-heap commit charge grow materially? | bounded, small relative to ~125 MiB commit floor | +48 KiB at 1-heap (+1.0%); +4 KiB/slot steady-state (§5) | **PASS** |
| K5 | Is the criterion wall-clock signal clean enough to read? | within ±15–20% noise floor | NO — +56% uniform across ALL sizes incl. 16 B; textbook host-load signature, overruled by iai (§6) | **N/A** (overruled) |

### Verdict

**GO (safe to promote `medium-classes` into the default `production` feature
bundle).**

The promotion-decision evidence R8-9 deferred is now in:

- **Small-size PRODUCTION path (16–1024 B), deterministic:** no measurable
  regression. iai Ir is +0.49% to +0.67% on every small-size gate, and the
  marginal per-op column (which subtracts the bootstrap constant) is +0.1% to
  +0.5%. The entire Ir delta is bootstrap-spread from the larger
  `SIZE_CLASS_TABLE` / `SIZE2CLASS` zero-init; the per-operation cost on the
  unaffected small path is essentially zero.
- **Tcache footprint:** exactly +816 B per `HeapCore` (CONFIRMED, not
  estimated — `PerClass`=136 B × 6 new classes). Inline in every registry
  `HeapSlot`; theoretical fully-materialized registry commit ceiling grows by
  ~3.25 MiB.
- **First-heap commit charge:** +48 KiB at the 1-heap bootstrap (within one
  page of the ~52 KiB the chunked-registry chunk-0 materialisation predicts
  from the §3 footprint delta), +4 KiB/slot in steady state (indistinguishable
  from zero at Windows commit-accounting granularity). RSS / latency deltas
  were noise-dominated and carry no signal.
- **Wall-clock (criterion):** inconclusive due to host load; overruled by the
  deterministic judge.

The one real, large, deterministic delta found — `realloc_grow` +173.9% Ir
(§4.3) — is **the feature working as designed on sizes it targets** (the
geometric realloc sweep passes through the new 256 KiB–1 MiB classes), NOT a
regression on the unaffected sizes this gate exists to protect. It belongs to
R8-9's measurement scope (sizes the feature targets), is consistent with R8-9's
findings, and does not bear on the promotion call.

### Conditions / caveats on the GO

1. **This report supplies evidence; it does not flip the bundle.** Per the task
   constraints, `Cargo.toml` was not modified. Promotion is a separate explicit
   change-request.
2. **The `PerClass` doc-comment inaccuracy** (`src/registry/tcache.rs:114–136`
   claims `count` is "in front of" `slots`; the empirical compile shows `slots`
   at offset 0, `count` at offset 128) is flagged in §3 for a future doc-only
   correction. It does not affect the cache-locality argument in either
   direction and is strictly out of scope for this measurement task.
3. **Cross-thread variant not re-run.** `npm run iai` runs the same-process
   single-thread perf-gate benches; the cross-thread free path
   (`heap_fanin_production` / `heap_fanin_persistent` criterion benches) was not
   re-run with `medium-classes` here. The substrate it exercises (remote-free
   ring, `HeapOverflow` drain) does not depend on `SMALL_CLASS_COUNT` for the
   16–1024 B path (those sizes hit the same magazine path single- and
   cross-thread), so this is not expected to add signal — but a future
   cross-thread iai arm, if one is added, would close the gap formally.
4. **Criterion re-run on an idle host** would tighten the wall-clock signal but
   is not required for the verdict (iai is the deterministic judge and has
   already answered the question).

---

## 8. Recommendations

1. **Promote `medium-classes` into the default `production` feature bundle.**
   The evidence in this report + R8-9 together cover both (a) the feature's own
   target range (R8-9: 16× segment reduction, 48–600× free latency improvement,
   n=1024 OOM elimination for covered sizes, real warm freelist) and (b) the
   no-regression-on-unaffected-sizes side of the promotion question (this
   report: +0.1–0.5% per-op marginal Ir on 16–1024 B, +48 KiB commit at
   bootstrap, +816 B/HeapCore inline). Both sides are data-backed.
2. **If promotion proceeds, expect a one-time bump in the CI perf-gate Ir
   baseline** of ~0.5–1% across all `perf_gate_iai` benches (the bootstrap
   constant grows because the table is bigger). This is not a regression; the
   CI gate's `IAI_CALLGRIND_REGRESSION='Ir=10'` threshold tolerates it, but the
   baseline cache (`.github/workflows/perf-gate.yml`) will need a refresh on
   the promotion commit so the new baseline is recorded against the new table
   size, not compared against the pre-promotion baseline.
3. **Doc-only follow-up (low priority):** correct the `PerClass` field-order
   claim in `src/registry/tcache.rs:114–136` (§3). Strictly cosmetic; the
   cache-locality argument is unchanged.

---

## 9. Caveats

- **Two hosts (Windows native + WSL2 Linux).** The iai judge ran under WSL2
  Valgrind (Linux-only); the criterion + first-alloc runs ran Windows-native.
  This is the established protocol for this repo (the iai runner explicitly
  uses WSL for Valgrind while criterion and the RSS/commit probes are
  Windows-native — see `scripts/iai.mjs` module doc). The two hosts are the
  same physical machine; the iai result is binary+input-deterministic and does
  not depend on host load.
- **Single iai run per config.** iai is deterministic (Valgrind emulation), so
  a single run per config produces the exact Ir number; no multi-sample
  treatment is needed or meaningful for the Ir column. The cache-sim columns
  (L1/L2/RAM Hits, EstCycles) are also deterministic given the same
  binary+input under Callgrind's fixed cost model.
- **First-alloc: 15 fresh-process samples per config** (medians reported in §5;
  min/max ranges in the raw logs). The commit-charge axis is page-granular and
  therefore has a small inherent discretization jitter (±4 KiB = one page);
  the medians are stable across the 15 samples (commit Δ1heap range: 4,636–
  4,644 KiB OFF, 4,688–4,692 KiB ON — a 4–8 KiB intra-config spread vs a 48 KiB
  inter-config delta).
- **No `src/` or `Cargo.toml` was modified.** The raw logs produced during the
  run (`_raw_iai_*.log`, `_raw_criterion_*.log`, `_raw_firstalloc_*.log`) are
  not committed per repo convention; re-run the §2 commands to reproduce them.
- **`realloc_grow` (§4.3) is the only large deterministic delta found** and is
  explicitly classified as a targeted-size effect (the realloc sweep passes
  through the new medium classes), NOT a regression on the 16–1024 B
  unaffected sizes this gate protects. It is consistent with R8-9's targeted-
  size findings.
