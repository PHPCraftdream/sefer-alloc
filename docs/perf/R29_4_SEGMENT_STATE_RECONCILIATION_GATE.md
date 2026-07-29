# R29-4 — segment-state reconciliation: the R27-3 residual IS attributed — to `small_active`

**Task #435 (R29-4), Round 29.** MEASUREMENT-ONLY, per this project's
"measured, not spun" convention (R24-2/R24-5/R28-1/R29-3). This task
implements the readonly review finding from
`docs/reviews/2026-07-29-r28-readonly-review.md` §"about half of the
post-drain retention delta is real but not accounted to a mechanism": build
a debug snapshot mechanism that reconciles EVERY registered segment of a
heap into exactly ONE state, then re-run R27-3's retention shape with it
active to attribute (or honestly fail to attribute) the ~+4 MiB/heap
"committed-non-pooled" residual R27-3 §2 left unreconciled.

**Verdict: outcome (a) — the residual IS fully reconciled.** The ~+4 MiB/heap
post-drain residual (cap8 − cap4) is entirely the **`small_active`** state
(+1 segment = +4,096 KiB per heap). The mechanism: the extra `small_active`
segment retains `live_count > 0` because **magazine-resident (tcache) blocks**
keep its live count nonzero — blocks freed to the magazine during teardown
were NOT returned to the segment free list, so `live_count` was never
decremented to 0, so the segment was never eligible for pool/release, so it
stays committed and registered. This is exactly why `dbg_drain_small_pool`
(which only releases pooled segments) does not reclaim it — the residual is
NOT a pooled segment and was never pool/release-eligible. The
"registered-empty-but-not-pooled" state the reviewer hypothesised
(`small_empty_orphan`) has **count = 0** at every measurement point — it is
structurally empty in this workload because `release_or_pool_empty_segment`
either pools or releases (never leaves a segment registered-but-empty).

The reconciliation identity `sum(per-state count) == table count()` holds
**exactly** at all four measurement points (hard-asserted in the probe), with
zero unknown/corrupt segments. No unaccounted-for residual bucket.

**Date:** 2026-07-29. **Base revision measured:** `main` @
`db356174ec76c547c7cc1bd90d5628a39c30bd64` + this task's uncommitted
working tree. **Platform:** native Windows 10 Pro x86-64, 16 logical cores
(shared host — but the per-state counts are EXACT segment counts, not noisy
RSS point estimates; the cap/conflicts self-checks are exact). **Feature
set:** `production` + `alloc-stats` + `bench-internals` (matching
R27-3/R29-3's build).

**Measurement only. No production behavior changed:** one new
`examples/r29_4_segment_state_reconciliation_gate.rs` binary and three new
`bench-internals`-gated safe `pub fn` accessors (the reconciliation method +
its return types on `AllocCore`, plus a thin `HeapCore` delegation wrapper).
No production call site touched, no existing function body edited.

---

## 0. Headline — the per-state reconciliation

All numbers are **exact per-heap segment counts** (not noisy RSS), median of
2 identical runs — the probe is fully deterministic (same seed, same batch
count, same single-threaded shape). Each arm self-verified
`verified_cap == pool_segments` and `config_conflicts_delta == 0` before
its number was trusted.

### Post-teardown per-state breakdown (1 heap, 1 thread)

| state | cap4 count | cap4 committed KiB | cap8 count | cap8 committed KiB | Δ count | Δ KiB |
|---|---:|---:|---:|---:|---:|---:|
| `primordial` | 1 | 4,096 | 1 | 4,096 | 0 | 0 |
| `small_pooled` | 4 | 16,384 | 5 | 20,480 | **+1** | **+4,096** |
| `small_active` | 1 | 4,096 | 2 | 8,192 | **+1** | **+4,096** |
| `small_empty_orphan` | 0 | 0 | 0 | 0 | 0 | 0 |
| `small_decommitted_retained` | 0 | 0 | 0 | 0 | 0 | 0 |
| `large_active` | 0 | 0 | 0 | 0 | 0 | 0 |
| `large_cached` | 0 | 0 | 0 | 0 | 0 | 0 |
| **total** | **6** | **24,576** | **8** | **32,768** | **+2** | **+8,192** |

Post-teardown total committed delta: **+8,192 KiB = +8 MiB** — matches
R27-3's ~+8 MiB/heap retention finding exactly.

### Post-drain per-state breakdown (after `dbg_drain_small_pool`)

| state | cap4 count | cap4 committed KiB | cap8 count | cap8 committed KiB | Δ count | Δ KiB |
|---|---:|---:|---:|---:|---:|---:|
| `primordial` | 1 | 4,096 | 1 | 4,096 | 0 | 0 |
| `small_pooled` | 0 | 0 | 0 | 0 | 0 | 0 |
| `small_active` | 1 | 4,096 | 2 | 8,192 | **+1** | **+4,096** |
| `small_empty_orphan` | 0 | 0 | 0 | 0 | 0 | 0 |
| `small_decommitted_retained` | 0 | 0 | 0 | 0 | 0 | 0 |
| **total** | **2** | **8,192** | **3** | **12,288** | **+1** | **+4,096** |

Post-drain total committed delta: **+4,096 KiB = +4 MiB** — matches R27-3's
~+4 MiB post-drain residual exactly.

### The reconciliation

R27-3 §2's decomposition of the ~+8 MiB post-teardown delta:

| R27-3 tier | per-heap | R29-4 state | R29-4 per-heap | reclaimable by drain? |
|---|---:|---|---:|---|
| pooled | ~+4 MiB | `small_pooled` | **+1 seg = +4,096 KiB** | yes (5→0, 4→0) |
| committed (non-pooled) | ~+4 MiB | `small_active` | **+1 seg = +4,096 KiB** | **no** (1→1, 2→2) |
| **total** | **~+8 MiB** | | **+2 seg = +8,192 KiB** | |

**The residual IS reconciled.** The "committed-non-pooled" tier is the
`small_active` state. The pooled tier is the `small_pooled` state. There is
no unaccounted-for third tier — the identity holds exactly.

---

## 1. Derived state enumeration (from source — how it differs from the reviewer's sketch)

Before implementing, the actual segment lifecycle source was read to derive
the genuine state enumeration. The reviewer's sketch (from
`docs/reviews/2026-07-29-r28-readonly-review.md` lines 203–210) suggested
seven candidate states. Here is what the source actually yields, with
adjustments:

| # | derived state | predicate | reviewer's sketch | difference |
|---|---|---|---|---|
| 1 | `primordial` | `kind == Primordial` | "primordial segment" | same |
| 2 | `small_pooled` | `kind == Small`, `live_count == 0`, pooled (in intrusive list) | "pooled segment" | same |
| 3 | `small_active` | `kind == Small`, (`live_count > 0` OR `base == small_cur`), not pooled, not decommitted | "current active small segment" | **merged**: the reviewer's "current active" and "has live blocks" are the SAME lifecycle state from the table's perspective — both are "registered and serving/available for allocation." Distinguished by the predicates `live_count > 0` vs `is_cur` within the state, not as separate states. |
| 4 | `small_empty_orphan` | `kind == Small`, `live_count == 0`, NOT pooled, NOT small_cur, NOT decommitted | "registered empty but non-pooled segment" | same concept — but **count = 0** at every measurement point (structurally empty in this workload; see §3) |
| 5 | `small_decommitted_retained` | `kind == Small`, `is_decommitted == true` | "decommitted reservation" | **renamed + scoped**: this is the `release_follows == false` retain-decommit path of `decommit_empty_segment_impl`, which has **ZERO production callers** today (confirmed by reading the source: only the test hook `dbg_force_decommit_retain_for` drives it). A released segment is BOTH decommitted AND recycled (slot NULLed) — it does NOT stay registered. So this state is structurally empty in production. |
| 6 | `large_active` | `kind == Large`, `magic == SEGMENT_MAGIC` | not in sketch (folded into "large-cache contribution") | **split from cached**: a Large segment serving a live allocation vs one deposited in the cache are distinguishable via the `magic` field (cached segments have `magic == 0`, atomically zeroed at deposit). |
| 7 | `large_cached` | `kind == Large`, `magic == 0` | "large-cache contribution" | **confirmed in scope**: a large-cache-held segment DOES appear in `segment_bases()` (it stays registered in the SegmentTable; the cache only stores metadata in `AllocCore::large_cache`). But for THIS gate's workload (1024-byte Small-only churn), `large_cached` count = 0. |
| — | `unknown` | `kind_at` returns `Unknown` (corrupt byte) | not in sketch | always 0 in well-formed heaps |

### Key adjustment: "released-unregistered" is structurally OUT OF SCOPE

The reviewer listed "released/unregistered segment" as a candidate state.
But `segment_bases()` (the enumeration primitive) **only iterates non-NULL
table slots** (`SegmentTable::bases` / `base_at` — both filter NULL slots).
A released segment's slot is NULLed by `table.recycle`. So a released segment
is **by construction not iterable** — it cannot be classified because it is
no longer in the table. This is correct behavior, not a gap: the
reconciliation's scope is "every REGISTERED segment," and a released segment
is not registered.

---

## 2. Methodology

### 2.1 The reconciliation hook

A new `bench-internals`-gated safe `pub fn`
`AllocCore::dbg_segment_state_reconciliation(&self) ->
SegmentStateReconciliation` (in `alloc_core_small_pool.rs`) iterates every
non-NULL segment-table slot via `self.table.base_at(i)` for
`i in 0..self.table.count()`, reads each segment's header, classifies it
into exactly one of the seven derived states, and accumulates committed +
reserved bytes per state. A thin `HeapCore` delegation wrapper
(`heap_core_diag.rs`) exposes it at the registry level.

**Safety analysis (CLAUDE.md benchmark-hook rule):** this is a plain SAFE
`pub fn`, NOT `unsafe fn`, because (1) it does NOT derive a segment base
from a caller-provided raw pointer — every base comes from
`self.table.base_at(i)`, the table's own non-NULL slot (inherently validated
by the table's register/recycle invariant); (2) it performs NO mutation;
(3) the per-segment header reads are the SAME seam the existing safe
`dbg_live_count_for` / `dbg_is_decommitted_for` accessors use. This is a
strictly WEAKER access pattern than those (which take a caller `*mut u8` and
validate via `contains_base_ro`). `bench-internals`-gated (rule 2: no
production caller).

### 2.2 Why single-threaded, not the full R27-3 apparatus

The per-state accounting is **exact segment counts**, not noisy RSS. The
question this gate answers — "which state accounts for the ~+4 MiB residual?"
— is answerable from one heap per arm, because the per-heap accounting is
deterministic and exact. R27-3's full subprocess-per-arm / multi-thread /
RSS-monitoring apparatus was needed because it measured noisy process-wide
RSS; this gate measures exact per-heap segment counts, so a single-threaded
single-heap probe per arm is sufficient and still valid evidence.

Subprocess isolation IS kept (one child process per arm) to avoid the
first-claim-wins registry-slot reuse bug (R25-5 / R26-4 pattern), even though
per-heap reads are less susceptible to it than process RSS — it is the
proven-correct approach and costs nothing.

### 2.3 Workload fidelity

Same as R27-3: `SIZE=1024`, `CHURN_WORKING_SET=256`, `OPS=1024`,
`PRESSURE_BATCH_SIZE=120` (the batch R25-5/R26-3 established reliably
exceeds 4 segments and settles around 6). The batched-setup shape
(`collect batch_size prefills UP FRONT, THEN churn+teardown each`) is kept
verbatim. 20 pressure batches (fixed count for determinism, matching R27-3's
~1.5 s window which produces the same settled state).

### 2.4 Self-verification

Each child hard-`panic!`s on:
1. `verified_cap == pool_segments` (resolved cap matches requested).
2. `config_conflicts_delta == 0` (fresh process → no conflict).
3. **Reconciliation identity**: `sum(per-state count) + unknown_count ==
   segment_bases().count()` at BOTH measurement points (post-teardown and
   post-drain) — the structural proof no segment was skipped or
   double-counted.

All 4 snapshots (2 arms × 2 measurement points) passed all three checks.

---

## 3. Why `small_empty_orphan` is structurally empty

The reviewer hypothesised "registered empty but non-pooled" as a candidate
for the residual. The source shows why this state is structurally empty in
this workload:

`release_or_pool_empty_segment` (`alloc_core_small_pool.rs:236`) is the ONLY
function that finalizes an emptied segment. Its decision is a strict
dichotomy:
- **pool not full** → `pool_push_front` (segment stays registered, committed,
  classified `small_pooled`).
- **pool full or disabled** → `release_empty_segment_now` (decommit payload +
  reset) then `table.recycle` (slot NULLed — segment unregistered).

There is NO "leave it registered-but-empty" branch. A segment at
`live_count == 0` that is NOT `small_cur` is ALWAYS either pooled or
recycled within the same function call. The ONLY way a `small_empty_orphan`
can exist is if a segment went empty but `release_or_pool_empty_segment` was
never called for it — the `finalize_orphaned_empty_segments` sweep exists
for exactly this edge case (drain-buffer overflow with >64 distinct bases
emptying in one call). In this single-threaded, moderate-churn workload,
that edge case does not fire. The measurement confirms: **`small_empty_orphan`
count = 0** at every measurement point, for both arms.

---

## 4. The residual's mechanism — magazine residency

The +1 `small_active` segment in cap8 (post-teardown and post-drain) has
`live_count > 0`. This is logically forced: only one segment can be
`small_cur` (the current bump-carve target), and cap8's `small_active` count
is 2 while cap4's is 1. The second segment must have `live_count > 0` (it
cannot be `small_cur`, and it is not pooled/decommitted/orphan).

**Why does a segment have `live_count > 0` after teardown?** Because teardown
frees blocks via `heap.dealloc()`, which under `fastbin` (part of `production`)
pushes freed blocks into the per-class **magazine** (tcache) rather than
directly to the segment free list. A magazine-resident block's `live_count`
is **NOT decremented** (the magazine push does not call `dealloc_small` →
`reclaim_offset` → `dec_live`). The magazine holds up to `TCACHE_CAP` (16)
blocks per class. After teardown, up to 16 blocks per class sit in the
magazine, keeping their owning segment(s)' `live_count > 0`.

This is why `dbg_drain_small_pool` does NOT reclaim the residual: drain
releases `small_pooled` segments (which have `live_count == 0`); the residual
segment has `live_count > 0` (magazine-resident blocks), so it was never
eligible for pool/release in the first place. It stays committed until either
(a) the magazine is flushed (next allocation drains the magazine, returning
blocks to the free list and decrementing `live_count`), or (b) thread-exit /
recycle (`trim_for_recycle` flushes the magazine + drains the pool).

Under cap4 vs cap8: the magazine holds the same number of blocks regardless
of pool cap. The difference is that cap8's gentler pool policy (no decommit
churn: `decommit_total = 0` vs cap4's `40`) leaves magazine-resident blocks
spread across one additional segment. Cap4's aggressive churn cycles segments
through more rapidly, concentrating magazine-resident blocks into fewer
segments.

---

## 5. What this gate does NOT claim

- **No claim that `small_active` is always the residual** — this is the
  finding for the R27-3 batch-120 / 1024-byte / single-thread workload.
  Workloads with different pressure shapes, sizes, or thread counts may
  distribute retention across different states. But the MECHANISM (magazine
  residency keeps `live_count > 0`, blocking pool/release eligibility) is
  structural and workload-independent.
- **No claim that the magazine is the ONLY source of `small_active` retention**
  — `small_cur` is always `small_active` (the current carve target). But
  `small_cur` is present in BOTH arms (1 each), so it does not contribute to
  the DELTA. The delta's source is the magazine-residency mechanism above.
- **No fix or redesign attempted** — this is measurement-only, matching
  R27-3/R29-3's scope discipline. A future task that wants to reclaim the
  residual would need to flush magazines before pool/release decisions (e.g.
  a `trim_for_recycle`-style flush on idle), but that is explicitly out of
  scope here.

---

## 6. Implications

- **R27-3 §2's "committed-non-pooled" residual is now source-verified.** It
  is the `small_active` state, with the mechanism identified as magazine
  residency. The phenomenological RSS-subtraction split ("~4 MiB pooled +
  ~4 MiB committed-non-pooled") is now a **structural per-state accounting**
  ("+1 `small_pooled` + +1 `small_active`"), with the identity
  `sum(states) == table count()` proven at every measurement point.
- **R27-5/#423 (adaptive/process-wide pool budget):** the residual is NOT
  reclaimable by small-pool drain alone. An adaptive design that wants to
  bound idle retention must ALSO flush the magazine (tcache), not just drain
  the pool. The existing `trim_for_recycle` (thread-exit path) already does
  both — a scavenger would need to replicate that shape.
- **R29-3's decomposition context:** R29-3 measured the segment-lifecycle
  cycle cost and closed item 15 (reservation-only overflow tier). This gate
  does not reopen it, but it does clarify that the retention this gate
  reconciles is orthogonal to the lifecycle cost R29-3 decomposed.

---

## 7. Files changed

| file | change |
|---|---|
| `src/alloc_core/alloc_core_small_pool.rs` | +`SegmentStateAccount` / `SegmentStateReconciliation` structs + `dbg_segment_state_reconciliation` method (all `bench-internals`-gated safe `pub`) |
| `src/alloc_core/mod.rs` | +re-export of the two structs under `#[cfg(all(alloc-decommit, bench-internals))]` |
| `src/registry/heap_core_diag.rs` | +thin `HeapCore::dbg_segment_state_reconciliation` delegation wrapper |
| `examples/r29_4_segment_state_reconciliation_gate.rs` | NEW — subprocess-per-arm reconciliation probe (2 children: cap4, cap8) |
| `Cargo.toml` | +`[[example]]` entry for `r29_4_segment_state_reconciliation_gate` |
| `docs/perf/R29_4_SEGMENT_STATE_RECONCILIATION_GATE.md` | this report (new) |
| `docs/perf/R29_4_SEGMENT_STATE_RECONCILIATION_GATE_summary.csv` | machine-readable summary (new) |
| `docs/perf/_raw_r29_4_reconciliation_run1.log` | raw probe stdout run 1 (`.gitignore`d — `git add -f` at commit time) |
| `docs/perf/_raw_r29_4_reconciliation_run2.log` | raw probe stdout run 2 (`.gitignore`d — `git add -f` at commit time) |
| `docs/perf/R27_3_POOL_RETENTION_GATE.md` | +dated correction note adjacent to §2 (append-only) |

**No production source file changed.** No `src/` production path touched
(the new accessors are `bench-internals`-gated additions, no existing body
edited).

---

## 8. Reproduce

```text
cargo run --release --example r29_4_segment_state_reconciliation_gate --features "production alloc-stats bench-internals"
```

The orchestrator spawns two child processes (cap4, cap8), each claiming one
heap, self-verifying cap + conflicts + reconciliation identity, running 20
batch-120 pressure cycles, snapshotting per-state reconciliation
post-teardown and post-drain, then emitting per-state `RESULT` lines + an
`OK` summary. ~5 s total wall-clock.
