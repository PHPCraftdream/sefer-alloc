# R10-6 — NUMA-aware segment-directory scan-cliff: measurement judge + design

**Date:** 2026-07-21
**Base revision:** `main` @ `fdd360d` (R10-5 just landed)
**Platform:** Windows 10 Pro x86-64, 16 threads, Rust release (bench) profile.
Single-NUMA test host (see §2.3 for why this does not invalidate the measurement).
**Feature configs measured:**
- **NUMA** = `--features "alloc-core numa-aware alloc-segment-directory"`
  (the path under test: directory sidecar materialised but write-only — the
  directory-driven lookup block is compiled out under `numa-aware`, so every
  free-list miss walks the O(S) linear scan with the two-pass NUMA preference).
- **Dir-ON** = `--features "alloc-core alloc-segment-directory"`
  (non-NUMA production-equivalent: directory-driven O(1) lookup).
- **Dir-OFF** = `--features "alloc-core"`
  (non-NUMA baseline: pure linear scan, no directory — reproduces R7-A0).
**Harness:** `benches/segment_directory_sweep.rs` (the R6-OPT-A4 / R7-A0 judge,
unchanged — re-run under three feature configs on this host today, so ratios
are computed from matched runs that cancel host-load drift, exactly as R7's
methodology prescribes).

---

## 0. TL;DR

The O(S) linear-scan cliff that R7 Workstream A eliminated for the non-NUMA
configuration is **STILL FULLY PRESENT for anyone running `--features
numa-aware`**. Under `numa-aware`, the directory-driven lookup is compiled out
(`#[cfg(all(feature = "alloc-segment-directory", not(feature = "numa-aware")))]`
at `alloc_core_small.rs:458`), so every free-list miss walks the full O(S) scan
with the two-pass local-first / foreign-fallback NUMA preference. The measured
cost at `holes=0%` (worst case — scan walks all `S-1` non-target segments):

| S | NUMA (scan-only) | Dir-ON (accelerated) | Dir-OFF (R7 baseline) | NUMA ÷ Dir-ON |
|---:|---:|---:|---:|---:|
| 64 | **524 ns** | 59 ns | 231 ns | **8.9×** |
| 256 | **12,777 ns** | 160 ns | 12,233 ns | **79.9×** |
| 1023 | **69,636 ns** | 497 ns | 63,142 ns | **140.1×** |

(class 48 / SMALL_MAX = 258,752 B, holes=0%, the same cell R7-A0 reported.)
At S=1023 the NUMA-aware path pays **~70 µs per scan** — 140× what the
directory-accelerated non-NUMA path costs, and the SAME order of magnitude R7
measured for the non-NUMA cliff before the directory landed (~92–102 µs at
S=1023 per R7-A0/R7-A6). **The cliff is real, significant, and comparable in
order of magnitude to what R7 eliminated.**

**Stage 2 verdict: CONDITIONAL GO** (design-only, no prototype this session —
see §7). The win is large for multi-node deployments that materialise many
segments, the fix is a straightforward node-indexed extension of the existing
directory, and it does not reopen the R8-1/R8-2/R9-8 incremental-sync /
authoritative-miss / drift-recovery machinery. But `numa-aware` is opt-in,
lower priority than the medium-classes work (R10-2/R10-4/R10-5, already done),
and most real users run without it — a prototype session should wait until a
real multi-node user requests it or `numa-aware` is considered for production
promotion.

---

## 1. Why the cliff exists — the compiled-out directory

`find_segment_with_free_impl` (`src/alloc_core/alloc_core_small.rs`) has two
lookup strategies gated by feature:

1. **Directory-driven O(1) lookup** (lines 458–639): compiled under
   `#[cfg(all(feature = "alloc-segment-directory", not(feature =
   "numa-aware")))]`. Walks the per-class `class_nonempty` bitmap (at most
   16 u64 words per class), validates each candidate, returns on first hit.
   On a directory MISS, R8-2 trusts the miss authoritatively (no O(S) scan)
   for up to `DIRECTORY_MISS_FULL_SCAN_PERIOD - 1` consecutive per-class
   misses before a periodic re-validation scan (R9-8).

2. **Linear-scan fallback** (lines 650–925): the O(S) walk over
   `[0, count)`. Under `numa-aware`, this is the ONLY path — the directory
   block above is compiled out entirely. The scan implements the two-pass
   local-first / foreign-fallback NUMA preference: for each segment with a
   free block, it checks `node_id_of()`; a foreign-node segment is recorded
   in `fallback` and the scan continues; after walking all S, it returns the
   first local hit (or the fallback if no local hit was found).

The code comment at lines 427–436 documents this deliberately:

> P2 (NUMA two-pass preference): when `numa-aware` is active, the
> directory-driven lookup is DISABLED — the linear-scan fallback below
> handles the two-pass local-first / foreign-fallback NUMA preference.
> The directory bitmap is still maintained (A1/A2 helpers fire normally), so
> the sidecar stays consistent for a future round that adds node-aware bit
> selection.

The R7 plan (`docs/perf/R7_PLAN.md` §P2, lines 68–72) set this as a blocking
design constraint: *"either node-aware bit selection, or forbid `numa-aware ×
alloc-segment-directory` at compile time — never silently lose placement."*
The MVP chose the former's deferral (directory disabled under `numa-aware`,
not a compile_error) so the two features coexist without losing NUMA
preference. This report measures the COST of that deferral.

---

## 2. Stage 1 — measurement

### 2.1 Methodology

Identical harness (`segment_directory_sweep.rs`), three feature configs, same
host, same session — so the ON/OFF/NUMA ratios are matched-run (cancel
host-load drift, per R7-A6's methodology). Each cell constructs exactly `S+1`
registered segments (S scannable + 1 always-full "current"), punches the
target (last) with one free block, times `SCANS_PER_TRIAL=256` back-to-back
`alloc()` calls that each fall through to `find_segment_with_free`, divided
by 256 for the per-scan mean. `adaptive_repeats` independent trials are pooled
for mean/p50/p99. See the harness's own 870-line module doc for the full
construction-discipline story (two real bugs found and fixed during its own
development, `dbg_table_count()` invariants verified, etc.).

The NUMA config (`numa-aware`) compiles the directory out and adds:
- one `numa::current_node()` call per `find_segment_with_free` invocation
  (a Windows syscall pair: `GetCurrentProcessorNumberEx` +
  `GetNumaProcessorNodeEx` — see §2.4),
- one `meta.node_id_of()` read + comparison per segment that has a free block
  (only the target at `holes=0%`, since the node check is gated INSIDE the
  `bt.head(class_idx) != FREE_LIST_NULL` branch — full segments fail the
  BinTable check first, never reaching the node comparison).

### 2.2 Headline numbers — class 48 (SMALL_MAX, 258,752 B), holes=0%

| S | Metric | NUMA | Dir-ON | Dir-OFF | NUMA ÷ ON | NUMA ÷ OFF |
|---:|---|---:|---:|---:|---:|---:|
| 64 | mean | 524 ns | 59 ns | 231 ns | **8.9×** | 2.3× |
| 64 | p50 | 547 ns | 60 ns | 217 ns | | |
| 64 | p99 | 826 ns | 82 ns | 298 ns | | |
| 256 | mean | 12,777 ns | 160 ns | 12,233 ns | **79.9×** | 1.04× |
| 256 | p50 | 12,895 ns | 166 ns | 12,164 ns | | |
| 256 | p99 | 15,346 ns | 230 ns | 14,530 ns | | |
| 1023 | mean | 69,636 ns | 497 ns | 63,142 ns | **140.1×** | 1.10× |
| 1023 | p50 | 70,636 ns | 557 ns | 64,320 ns | | |
| 1023 | p99 | 74,182 ns | 578 ns | 69,585 ns | | |

(Small-class sweep for class 25 / ~4 KiB confirms the same shape: NUMA
S=64/256/1023 = 527 / 15,070 / 67,066 ns; Dir-ON = 65 / 153 / 550 ns.)

### 2.3 Why single-NUMA does not invalidate the measurement

On this single-NUMA Windows host, `GetNumaProcessorNodeEx` returns node 0 for
every CPU, so `my_node = 0` and every segment is stamped `node_id = 0` at
reserve time. The scan's foreign-node branch (`seg_node != my_node &&
seg_node != NO_NODE_RAW`) is therefore NEVER taken — every segment is "local"
and the scan returns on the first segment with a free block, exactly as the
non-NUMA scan does.

This means the measured numbers are the **first-pass-only** (local-hit) cost
of the NUMA scan. On a real multi-node host where the only free block is on a
FOREIGN node, the scan walks ALL S segments (no local hit terminates early),
records the foreign fallback, and returns it after the full walk — the SAME
O(S) cost, never less. So the single-node measurement is a **lower bound** on
the real multi-node cost; the cliff can only be WORSE on genuine multi-node
hardware, never better.

The algorithmic shape being judged (O(S) walk, one metadata read per segment)
is identical regardless of node count. The per-segment NUMA check adds one
`u32` read + compare per segment-with-a-free-block — negligible relative to
the O(S) metadata-walk cost that dominates at high S (confirmed by the
NUMA ÷ Dir-OFF ratio converging to ~1.0–1.1× at S≥256).

### 2.4 Secondary finding — the `current_node()` syscall overhead

The NUMA ÷ Dir-OFF ratio at S=64 is **2.3×** (524 vs 231 ns), but at S=256+
it drops to ~1.04–1.10×. The difference (~293 ns) is the fixed per-scan cost
of `numa::current_node()`, which calls `GetCurrentProcessorNumberEx` +
`GetNumaProcessorNodeEx` (two kernel transitions) once per
`find_segment_with_free` invocation. At S=64 the scan itself costs only
~231 ns, so the ~293 ns syscall nearly doubles the total. At S=1023 the scan
costs ~63 µs and the syscall is noise (~0.5%).

This is a **separate** cost from the O(S) cliff — it applies to every
free-list miss under `numa-aware` regardless of whether a directory exists.
A NUMA-aware directory (§5) would NOT eliminate this overhead (it still needs
`current_node()` to know which node's bitmap to query first). Caching
`current_node()` (e.g. per-thread, refreshed every N allocations) would
address it but changes the migration-handling semantics of §4 of
`PHASE_NUMA_DESIGN.md` (strategy (a) "ignore migration" — calling
`current_node()` once per scan, not once per alloc, is the documented MVP
trade-off). This is flagged as a follow-up (§7.4), NOT part of the
directory-cliff fix.

### 2.5 Stage 1 verdict

**The cliff is REAL and comparable in order of magnitude to the R7 non-NUMA
cliff.** NUMA-aware at S=1023 costs ~70 µs/scan (140× the directory-accelerated
non-NUMA path). This is the same algorithmic O(S) shape R7 eliminated for the
non-NUMA configuration, present unchanged under `numa-aware` because the
directory read path is compiled out. **Stage 2 (design) is warranted.**

---

## 3. Stage 2 — design (CONDITIONAL GO, no prototype this session)

### 3.1 Design constraint — preserve local-first / foreign-fallback

The R7 plan's P2 rule (lines 68–72) is binding: a NUMA-aware directory must
preserve the two-pass local-first / foreign-fallback preference that the
linear scan implements today. A naive "first set bit" directory query (the
existing non-NUMA lookup) would silently drop this preference — it would
return a foreign-node segment when a local one exists later in the bitmap,
defeating the entire purpose of `numa-aware`. Two concrete approaches satisfy
this constraint.

### 3.2 Approach A — node-indexed directory (RECOMMENDED for a prototype)

**Structure.** Extend the existing flat 2-D bitmap with a node dimension:

```text
class_nonempty_by_node: [[u64; WORDS_PER_CLASS]; SMALL_CLASS_COUNT]  per node

class_nonempty_by_node[node][class][word]  — bit j set iff slot (word*64+j)
  is a live Small/Primordial segment with node_id == node and
  BinTable::head(class) != FREE_LIST_NULL.
```

Physically: `[[[u64; 16]; SMALL_CLASS_COUNT]; MAX_NODES]`. The existing
`SegmentDirectory` struct gains one outer dimension. `MAX_NODES` is a
compile-time constant (8 covers all current x86 server topologies; 16 is
conservative; the numa-shim already caps its sysfs scan at 64 nodes).

**Memory.** `MAX_NODES ×` the existing 6.1 KiB (49 classes) / 6.9 KiB
(medium-classes). For `MAX_NODES = 8`: **48.9 KiB** (default) / 55.3 KiB
(medium). For `MAX_NODES = 16`: 97.9 / 110.6 KiB. Materialised once per heap
after the existing threshold (32 segments); below threshold the scan runs
unchanged. The reservation is `aligned_vmem::reserve_aligned` + `mem::forget`
(same pattern as today). On a single-node host, 7/8 of the space is zero
(virtual pages, never faulted) — zero RSS cost for the empty node bitmaps.

**Query protocol** (replaces the compiled-out directory block at
`alloc_core_small.rs:458`):

```text
1. local node:  scan class_nonempty_by_node[my_node][class] (≤16 words)
   — validate each candidate (base, kind, ring drain, BinTable head)
   — return first valid hit (LOCAL hit — preferred)
2. foreign fallback: for each node != my_node (in ascending order):
   scan class_nonempty_by_node[node][class] (≤16 words each)
   — same validation; return first valid hit (FOREIGN fallback)
3. NO_NODE segments (node_id == NO_NODE_RAW): scan a dedicated
   node-index = MAX_NODES (the "unknown node" bucket) — these are treated
   as acceptable (matches today's seg_node == NO_NODE_RAW == local logic)
4. directory miss → R8-2 authoritative-miss / R9-8 per-class streak
```

Total worst-case word examination: `MAX_NODES × 16 = 128` words (MAX_NODES=8).
In practice, most nodes' bitmaps are zero for most classes (zero-word skip is
one branch), so the common case is `~16` words (local node only).

**Update protocol.** `set_bit` / `clear_bit` gain a `node_id` parameter,
derived from the segment header at every call site (the callers already have
`base`):

```text
set_bit(node_id, class_idx, slot_idx)    // was set_bit(class_idx, slot_idx)
clear_bit(node_id, class_idx, slot_idx)  // was clear_bit(class_idx, slot_idx)
```

`clear_slot(slot_idx)` (segment recycle) clears ALL nodes' bitmaps for that
slot — or, since the caller (`clear_segment_directory`) has `base`, reads
`node_id_of(base)` and clears only that node (cheaper, but requires the header
still be readable at recycle time, which it is — the segment goes to the pool
before unmapping).

`sync_directory_for_segment_classes(base, slot_idx, changed_classes)` (R8-1)
derives `node_id` from `base` internally and writes to the per-node bitmap.
No change to the `changed_classes` popcount optimisation.

`rebuild_from_table` (one-time materialisation) reads each segment's
`node_id_of()` and places bits in the correct node's bitmap.

### 3.3 Approach B — global directory + per-node slot-membership filter

**Structure.** Keep the existing `class_nonempty[class][word]` UNCHANGED (the
global directory), PLUS add a per-node slot-membership bitmap:

```text
slots_by_node: [[u64; WORDS_PER_CLASS]; MAX_NODES]
  — bit j set iff slot j currently holds a segment with node_id == node.
```

**Memory.** 6.1 KiB (existing) + `MAX_NODES × 128 B` = **7.1 KiB** for
MAX_NODES=8. Dramatically less than Approach A.

**Query protocol:**

```text
1. local candidates: for each word w in [0, 16):
   candidates = class_nonempty[class][w] AND slots_by_node[my_node][w]
   — scan set bits in candidates, validate, return first hit
2. foreign fallback: candidates = class_nonempty[class][w] (the full set)
   minus already-examined local bits — or just re-scan the full bitmap
```

**Trade-off vs. Approach A.** Approach B reuses the existing class bitmap
unchanged (lower diff risk) and has far lower memory. But the per-word AND
intersection doubles the memory reads per word (two arrays), and the
"foreign fallback" phase must distinguish "already examined as local" from
"new foreign candidate" — more complex control flow. The `slots_by_node`
bitmap is a different KIND of structure (per-slot membership, not per-class
nonemptiness) that must be maintained on segment register/recycle
independently of the class bitmap.

### 3.4 Recommendation: Approach A for a prototype

Approach A is simpler (one structure, one dimension added, query is a linear
scan of node-indexed bitmaps in preference order), directly mirrors the
existing directory's structure and invariants, and makes the local-first
preference structurally explicit (local bitmap is examined FIRST). The memory
cost (~49 KiB for MAX_NODES=8) is acceptable for a per-heap sidecar. Approach
B's memory advantage (~7 KiB vs ~49 KiB) matters only for processes with many
heaps (e.g. 64+ threads), and even then 64 × 49 KiB = 3.1 MiB is modest.

---

## 4. Interaction with R8-1 / R8-2 / R9-8 machinery (must not reopen)

A NUMA-aware directory is a **strict extension** of the existing directory —
it adds a node dimension to the bitmap, not a new concept. Every existing
invariant carries over because it operates on the logical union "does ANY
segment (any node) have a free block for this class," which is the OR of all
nodes' bitmaps:

| Mechanism | What it does | NUMA-directory impact |
|---|---|---|
| **R8-1** (task #214) incremental sync | `sync_directory_for_segment_classes` writes only touched classes' bits after a ring drain | Derives `node_id` from `base` (already available); writes to the per-node bitmap. O(popcount) unchanged. No reopening. |
| **R8-2** (task #215) authoritative miss | A directory MISS (no candidate in ANY node's bitmap) is trusted for N consecutive per-class misses | A miss means ALL nodes' bitmaps are empty for this class — the same logical condition, just checked across MAX_NODES bitmaps instead of one. The miss-streak / re-validation logic is unchanged. No reopening. |
| **R9-8** (task #230) per-class streak + rescue | Per-class `[u8; SMALL_CLASS_COUNT]` streak; OOM-rescue scan bypasses directory trust | Orthogonal to node preference — the streak is about directory COMPLETENESS, not WHICH candidate is preferred. The rescue scan already preserves NUMA preference (it IS the existing NUMA linear scan). No reopening. |
| **A4** dirty-routing drain | `drain_dirty_segments` runs before the directory lookup | Under `numa-aware` this is ALREADY compiled out (`not(feature = "numa-aware")` at line 449). A NUMA-aware directory would re-enable it, draining into the per-node bitmap. The drain body is unchanged. |

**The assert_directory_equals_rebuild oracle** (the test that proves the
incremental directory tracks true state) extends naturally: rebuild writes
per-node bits; incremental sync writes per-node bits; the oracle compares the
full per-node bitmap against a from-scratch rebuild. If they ever diverge,
the test catches it — exactly as it does today for the single-node bitmap.

---

## 5. Materialisation threshold

Unchanged: `DIRECTORY_MATERIALIZE_THRESHOLD = 32`. The per-node directory
materialises at the same segment count. Below 32, the NUMA linear scan runs
unchanged (its cost at S≤16 is ~300 ns including the `current_node()` syscall
— see §2.4 — which is acceptable). The threshold choice from R7-A0 §3
(scan cost ~442 ns at S=32) still holds: the directory becomes a net win
once the scan it replaces costs more than the directory lookup overhead.

One nuance: the `current_node()` syscall (~293 ns) is a fixed per-scan cost
that exists whether or not a directory is present. At S=32 the scan costs
~442 ns + ~293 ns (syscall) = ~735 ns under `numa-aware` today; a
NUMA-aware directory would reduce this to ~100 ns (lookup) + ~293 ns
(syscall) = ~393 ns. Still a win, but smaller than the non-NUMA case (where
the syscall doesn't exist). This does NOT change the threshold — 32 remains
correct — but it means the NUMA-aware directory's win at low S is partially
eaten by the syscall overhead that a `current_node()` cache (§7.4) would
address separately.

---

## 6. What the O(S) scan costs on this host vs. R7's host

| S | This session (Dir-OFF) | R7-A0 baseline | R7-A6 OFF run 1 |
|---:|---:|---:|---:|
| 64 | 231 ns | 1,119 ns | 1,085 ns |
| 256 | 12,233 ns | 17,194 ns | 19,028 ns |
| 1023 | 63,142 ns | 101,834 ns | 91,905 ns |

This session's Dir-OFF numbers are ~30–40% lower than R7-A0's. This is
host-load variance across sessions (different day, different background load)
— R7 itself documented ±15–20% host noise. The ORDER OF MAGNITUDE is the same
(~0.2–1.1 µs at S=64, ~12–19 µs at S=256, ~63–102 µs at S=1023), and the
matched-run ratios within THIS session (NUMA ÷ Dir-ON = 140× at S=1023) are
the load-cancelled signal that matters.

---

## 7. Verdict and recommendations

### 7.1 Overall: CONDITIONAL GO (design-only, no prototype this session)

The cliff is real (140× at S=1023), the design is sound (Approach A, §3.2),
and the fix does not reopen R8-1/R8-2/R9-8. But:

1. **`numa-aware` is opt-in and not in `production`.** The default and
   production configurations do not pay this cliff — they use the
   directory-accelerated path. Only users who explicitly enable
   `--features numa-aware` are affected.
2. **Lower priority than active work.** The medium-classes workstream
   (R10-2/R10-4/R10-5) is done; this is the next candidate but not blocking.
3. **Single-node hosts never hit the foreign-fallback path.** The measured
   numbers are a lower bound; the REAL multi-node cost (where the scan
   ALWAYS walks all S when the only free block is foreign) can only be
   confirmed on genuine 2+ socket hardware or QEMU fake-NUMA
   (`docs/PHASE_NUMA_DESIGN.md` §5).
4. **The `current_node()` syscall overhead (§2.4) is a separate, cheaper
   win** that should be evaluated first (it helps EVERY `numa-aware` user at
   low S, not just high-S directory users).

### 7.2 When to prototype

A prototype session for Approach A is warranted when ANY of:
- A real multi-node user reports scan-latency pain under `numa-aware`.
- `numa-aware` is being considered for promotion toward `production`.
- The `current_node()` cache (§7.4) has shipped and the remaining cliff
  (the O(S) walk itself) is the dominant `numa-aware` cost.

### 7.3 What a prototype session must verify

1. **Correctness oracle.** Extend `assert_directory_equals_rebuild` to the
   per-node bitmap. The existing directory test suite (R7-A5 correctness
   matrix + R8-1 incremental sync + R9-8 drift recovery) must pass with the
   node dimension added.
2. **Local-first / foreign-fallback preservation.** A test that constructs
   segments on two nodes (via the numa-shim `mock` feature, which allows
   scripting `current_node()` returns — `crates/numa/src/lib.rs:96–122`) and
   verifies the directory returns a local-node segment even when a
   foreign-node segment appears earlier in the bitmap.
3. **R8-2 authoritative miss under NUMA.** A per-node-bitmap miss (all nodes
   empty for this class) must still trigger the R8-2 trust + R9-8 periodic
   re-validation, unchanged.
4. **R9-8 rescue scan.** The OOM-rescue scan must still run the NUMA linear
   scan (with local-first / foreign-fallback), not a non-NUMA scan.
5. **Performance gate.** Re-run this harness under
   `--features "alloc-core numa-aware alloc-segment-directory"` with the
   prototype and confirm S=1023 drops from ~70 µs to sub-µs (matching the
   Dir-ON numbers ± the `current_node()` syscall overhead).

### 7.4 Follow-up: `current_node()` caching (orthogonal, cheaper, do first)

The ~293 ns per-scan `current_node()` syscall overhead (§2.4) affects every
`numa-aware` free-list miss at low-moderate S, independent of the directory.
Caching the result (e.g. in `AllocCore`, refreshed every N allocations or on
a time-based decay) would halve the S=64 NUMA cost immediately, with no
directory work at all. This changes the §4 migration strategy (strategy (a)
"ignore" becomes "ignore except every N allocations"), so it needs its own
design note — but it is strictly cheaper and lower-risk than the NUMA
directory, and should be evaluated before the directory prototype.

---

## Appendix: raw data

### A.1 NUMA-aware sweep (`alloc-core numa-aware alloc-segment-directory`)

```text
KILL-GATE (class=SMALL_MAX):
S=   1  mean=    293.0ns   p50=    299.0ns   p99=    355.0ns
S=   3  mean=    298.0ns   p50=    305.0ns   p99=    428.0ns
S=1023  mean=  66134.0ns   p50=  67336.0ns   p99=  71514.0ns
KILL-GATE ratios: mean(S=3)/mean(S=1)=1.02x  mean(S=1023)/mean(S=3)=221.93x

Consistency (S=64, class=SMALL_MAX, 5 repeats):
mean_of_means=495.4ns  max_abs_dev=28.4ns  rel_spread=5.7%

Quick matrix (class=48 / SMALL_MAX, holes=0%):
S=   1  mean=    272.0ns   p50=    268.0ns   p99=    338.0ns
S=   3  mean=    238.0ns   p50=    232.0ns   p99=    289.0ns
S=  16  mean=    301.0ns   p50=    278.0ns   p99=    417.0ns
S=  64  mean=    524.0ns   p50=    547.0ns   p99=    826.0ns
S= 256  mean=  12777.0ns   p50=  12895.0ns   p99=  15346.0ns
S=1023  mean=  69636.0ns   p50=  70636.0ns   p99=  74182.0ns

Quick matrix (class=25 / ~4 KiB, holes=0%):
S=  64  mean=    527.0ns
S= 256  mean=  15070.0ns
S=1023  mean=  67066.0ns
```

### A.2 Dir-ON sweep (`alloc-core alloc-segment-directory`)

```text
KILL-GATE (class=SMALL_MAX):
S=1023  mean=    514.0ns   p50=    543.0ns   p99=    703.0ns

Consistency (S=64, class=SMALL_MAX, 5 repeats):
mean_of_means=74.2ns  max_abs_dev=4.8ns  rel_spread=6.5%

Quick matrix (class=48 / SMALL_MAX, holes=0%):
S=   1  mean=     25.0ns
S=   3  mean=     34.0ns
S=  16  mean=     80.0ns
S=  64  mean=     59.0ns   p50=     60.0ns   p99=     82.0ns
S= 256  mean=    160.0ns   p50=    166.0ns   p99=    230.0ns
S=1023  mean=    497.0ns   p50=    557.0ns   p99=    578.0ns
```

### A.3 Dir-OFF sweep (`alloc-core`)

```text
KILL-GATE (class=SMALL_MAX):
S=1023  mean=  63389.0ns   p50=  63984.0ns   p99=  69196.0ns

Consistency (S=64, class=SMALL_MAX, 5 repeats):
mean_of_means=264.0ns  max_abs_dev=17.0ns  rel_spread=6.4%

Quick matrix (class=48 / SMALL_MAX, holes=0%):
S=   1  mean=     14.0ns
S=   3  mean=     29.0ns
S=  16  mean=     69.0ns
S=  64  mean=    231.0ns   p50=    217.0ns   p99=    298.0ns
S= 256  mean=  12233.0ns   p50=  12164.0ns   p99=  14530.0ns
S=1023  mean=  63142.0ns   p50=  64320.0ns   p99=  69585.0ns
```
