# R24-8 — `dealloc_batch` internals gate: ownership cache NO-GO, STAGE_CAP reduction GO

**Task #386 (R24-8), Round 24.** Two independent investigations into
`HeapCore::dealloc_batch`'s internal overhead (`src/registry/
heap_core_dealloc_batch.rs`): (1) a `last_base`/`last_is_owned` ownership cache
to skip redundant `contains_base` probes in same-segment batches, and (2)
whether LLVM elides the 4 KiB staging-array zero-init, and if not, whether a
smaller `STAGE_CAP` removes the cost. **Inv 1 is a measured NO-GO (negligible,
inconsistent sign — the same Heisenberg class as R24-3/R24-4). Inv 2 is a GO:
LLVM does NOT elide the zero-init, the constant cost is −4,065 Ir/call (−47.7%
of a 16-block same-segment batch-free), and reducing `STAGE_CAP` 512→64
eliminates it.**

**Date:** 2026-07-28. **Base revision measured:** `main` @ `7378160`. **Platform
measured:** WSL2 (Ubuntu 24.04, kernel `6.x-microsoft-standard-WSL2`) under
Windows 10 Pro x86-64, `valgrind 3.22.0`, `iai-callgrind-runner 0.14.2`, WSL
rustc `1.98.0-nightly`.

**P2 framing (per task brief):** the batch API has no downstream consumer
(R23-7 decision record), so this is preparation, not a user-visible gain.
Time-boxed accordingly.

---

## 0. Headline

| investigation | verdict | measured delta | action |
|---|---|---|---|
| **1: ownership cache** (`last_base`/`last_is_owned`) | **NO-GO** | +3 / −44 Ir (inconsistent sign — noise) | not implemented |
| **2: STAGE_CAP reduction** (512→64) | **GO** | **−4,065 Ir/call (−47.7% / −24.2%)** | **implemented + tested** |

---

## 1. Investigation 1 — ownership cache: NO-GO (negligible)

### 1.1 Hypothesis

`dealloc_batch_small` calls `self.core.contains_base(base)` for every block in
the batch. In an allocation-order batch, long runs of consecutive blocks share
one segment base. A `last_base`/`last_is_owned` cache would skip the redundant
`contains_base` call when `base == last_base`. R23-1 measured `contains_base`
(Tier-1 `own_cache` hit) at ~8.2 Ir/call; for a 64-block same-segment batch,
the arithmetic ceiling said ~63 × 8.2 ≈ 517 Ir savings.

### 1.2 What was measured

Two new iai arms (`dealloc_batch_fresh_16_16b`, `dealloc_batch_fresh_64_16b`)
allocate N fresh-carve 16 B blocks (all in one segment) then free them all in
one `dealloc_batch` call — the ideal workload for the cache (N−1 same-base
hits). The cache was applied, and the SAME arms were measured BEFORE vs AFTER
under `production batch-api bench-internals`:

| arm | BASELINE (no cache) | AFTER (cache) | delta |
|---|---:|---:|---:|
| `dealloc_batch_fresh_16_16b` | 8,514 | 8,517 | **+3** |
| `dealloc_batch_fresh_64_16b` | 16,757 | 16,713 | **−44** |
| `small_churn_16b` (reference) | 8,051 | 8,051 | 0 (byte-identical) |

### 1.3 Root cause: optimizing an already-cheap operation

The proposed cache replaces one compare (`own_cache[idx] == base` — the Tier-1
hit path) with another compare (`base == last_base`) plus 2 locals (register
pressure). The Tier-1 `own_cache` hit is already a single load+compare+branch
(~3–4 Ir); the `last_base` cache adds its own load+compare+branch, netting
roughly the same cost. The inconsistent sign (+3 at N=16, −44 at N=64 — opposite
directions) is the signature of codegen rearrangement, not a real signal: a
genuine optimization would show a consistent monotonic improvement scaling with
the cached-lookup count.

This is the **exact same Heisenberg class** as R24-3 (bitmap-clear merge, +37
Ir/overflow-event) and R24-4 (bulk-mask primitives, +14 Ir/block): an
arithmetic ceiling predicted a win, but the operation being optimized was
already cheap, and the replacement's own overhead cancelled the savings.

### 1.4 Decision: NO-GO — not implemented

The cache code was reverted. The tree at `src/registry/heap_core_dealloc_batch.rs`
is byte-identical to HEAD for this investigation.

---

## 2. Investigation 2 — STAGE_CAP reduction: GO (−4,065 Ir/call)

### 2.1 LLVM-IR proof: the zero-init is NOT elided

`const STAGE_CAP: usize = 512; let mut stage: [*mut u8; STAGE_CAP] =
[core::ptr::null_mut(); STAGE_CAP];` syntactically zero-initializes 512×8 = 4096
bytes of stack on every call. Only the written prefix (`stage[..staged]`) is
ever read. The question: does LLVM dead-store-eliminate the unused tail?

**Proof via LLVM-IR** (`cargo rustc --lib --features 'production batch-api'
--release -- --emit=llvm-ir -C codegen-units=1`):

```text
; Inside HeapCore::dealloc_batch (IR line ~20732):
call void @llvm.memset.p0.i64(ptr noundef nonnull align 8
    dereferenceable(4096) %stage.i, i8 0, i64 4096, i1 false)
```

The memset is **present and unelided**. The full IR has exactly **one** memset
of length 4096 — the stage array. (Other memsets in the crate are 16, 24, 128,
1024, 25088, 65536 bytes — none is the stage array.) LLVM cannot elide it
because the array's address escapes into `flush_class(&stage[..staged])` (the
compiler cannot prove the callee doesn't read the unwritten tail through the
slice).

Full evidence: `.crush/r248_ir_evidence.txt`.

### 2.2 Measured cost: −4,065 Ir/call (constant, batch-size-independent)

Reducing `STAGE_CAP` from 512 to 64 changes the memset from 4096 bytes to 512
bytes. The iai delta on both arms is **exactly −4,065 Ir** — a constant,
identical in both N=16 and N=64, proving it is the memset cost alone (not
batch-size-dependent work):

| arm | STAGE_CAP=512 (BASELINE) | STAGE_CAP=64 (AFTER) | delta |
|---|---:|---:|---:|
| `dealloc_batch_fresh_16_16b` | 8,514 | 4,449 | **−4,065 (−47.7%)** |
| `dealloc_batch_fresh_64_16b` | 16,757 | 12,692 | **−4,065 (−24.2%)** |
| `small_churn_16b` (reference) | 8,051 | 8,051 | 0 (byte-identical) |

At N=16 (no overflow — the stage array is zeroed but never written to or read),
the **entire** 4,065 Ir delta is pure zero-init waste. This is the strongest
possible signal: a ~48% reduction from eliminating dead zero-initialization
that the optimizer failed to remove.

### 2.3 Tradeoff analysis: multi-flush for large batches

With `STAGE_CAP=64`, a batch of N blocks does: first 16 → magazine, remaining
N−16 → staged in chunks of 64 (intermediate `flush_class` flush at each 64-block
boundary). Batches up to 80 blocks (STAGE_CAP + TCACHE_CAP) still fit in one
flush. Larger batches do multiple flushes.

The tradeoff is bounded: `flush_class` groups same-segment runs internally, so
each intermediate flush is one `flush_class` call per segment-run (not per
block). For the experimental API with no downstream consumer (R23-7), realistic
batch sizes are tens to low hundreds — well within 1–3 flushes. The constant
4,065 Ir savings per call dominates any per-flush overhead at these sizes.

The existing `batch_tcache.rs` test suite already exercises N=200 (BATCH_NS)
which triggers the multi-flush path at STAGE_CAP=64, and the full test suite
passed green.

### 2.4 Correctness verification

| gate | result |
|---|---|
| `cargo test --features 'production batch-api bench-internals'` (full suite) | **PASS** |
| New test: `r24_8_dealloc_batch_multi_flush.rs` (N=200 → 3 flushes, all blocks recycled) | **PASS** |
| Mutation counterfactual: remove mid-loop flush → `stage[64]` OOB panic | **RED** (test catches it) |
| `cargo clippy --features 'production batch-api bench-internals' --all-targets -- -D warnings` | **PASS** |
| Bench compiles WITHOUT batch-api (no-op stubs for `library_benchmark_group!`) | **PASS** |
| Bench compiles WITH batch-api (real arms) | **PASS** |

### 2.5 Decision: GO — implemented

`STAGE_CAP` reduced from 512 to 64. Two new iai arms added (with no-op stubs
for compilation without `batch-api`). One new correctness test added with a
confirmed mutation counterfactual.

---

## 3. What this confirms about the R24-3/R24-4 lesson

R24-3 and R24-4 established that **per-segment bitmap-clear loops in this
allocator are already efficiently compiled** — the compiler unrolls them and
CSE's the shared work, so arithmetic-ceiling estimates of savings are not
reliable. This task adds a **different** data point: the staging-array
zero-init is a case where LLVM does NOT optimize — the array's address escapes
into a callee, blocking DSE. The contrast is instructive: R24-3/R24-4's NO-GOs
were "the optimizer already did the work"; Inv 2's GO is "the optimizer
genuinely failed to do the work, and the cost is large." The decisive factor in
each case was the in-context measurement, not the arithmetic estimate.

---

## 4. Evidence

- `docs/perf/_raw_r24_8_baseline.log` — BASELINE iai run (STAGE_CAP=512, no
  cache). Features: `production batch-api bench-internals`. (`git add -f`
  needed — `.gitignore` excludes `docs/perf/_raw_*.log`.)
- `docs/perf/_raw_r24_8_inv1_after.log` — Inv 1 AFTER (cache applied).
- `docs/perf/_raw_r24_8_inv2_stage64.log` — Inv 2 AFTER (STAGE_CAP=64).
- `docs/perf/R24_8_DEALLOC_BATCH_INTERNALS_GATE_summary.csv` — companion
  machine-readable summary.
- `.crush/r248_ir_evidence.txt` — LLVM-IR memset proof (Investigation 2).

---

## 5. Files touched (final state)

- `src/registry/heap_core_dealloc_batch.rs` — `STAGE_CAP` 512→64 + updated
  comment. **(Investigation 1's cache code was reverted — no trace.)**
- `benches/perf_gate_iai.rs` — 2 new iai arms (`dealloc_batch_fresh_16_16b`,
  `dealloc_batch_fresh_64_16b`) + no-op stubs for `not(batch-api)`.
- `tests/r24_8_dealloc_batch_multi_flush.rs` — new correctness test (N=200
  multi-flush) with mutation counterfactual.
- `docs/perf/R24_8_DEALLOC_BATCH_INTERNALS_GATE.md` — this report.
- `docs/perf/R24_8_DEALLOC_BATCH_INTERNALS_GATE_summary.csv` — companion summary.
- `docs/perf/_raw_r24_8_baseline.log` / `_raw_r24_8_inv1_after.log` /
  `_raw_r24_8_inv2_stage64.log` — raw iai evidence (`git add -f`).
- `docs/perf/OPEN_ITEMS.md` — updated.
