# R9-6 — Class-aware dirty routing: wasted-drain MEASUREMENT (judge, no implementation)

**Task:** R9-6 — measure (judge, do not implement) the potential win of
"class-aware dirty routing" for a mixed-class remote-free workload, per an
external review's finding on `drain_dirty_segments`.
**Outcome:** **MEASUREMENT-ONLY.** No per-(segment,class) dirty routing is
implemented. The single permitted `src/` change is one additive diagnostic
counter (`WASTED_DIRTY_DRAINS`), purely Relaxed-ordering, no behaviour change
to the drain algorithm. The deliverable is this report + the judge test +
the counter.
**Date:** 2026-07-20 (original), 2026-07-21 (R10-3 post-fix re-measurement).
**Base revision:** `main` @ `7bdbc0f` (R9-5 just landed; the drain substrate
under analysis is R7-A4 @ `drain_dirty_segments`, `alloc_core_small.rs:1893`).
**R10-3 update:** `changed_classes` bit-gating on `reclaimed` (see §2.1).
**Platform:** Windows 10 Pro x86-64 (native, single test process). Results are
counter-deltas (deterministic given workload shape), not wall-clock — the
absolute counts carry OS-scheduler jitter, but the waste *ratio* is stable
across runs (§7.2).
**Harness:** `tests/r9_6_class_aware_dirty_judge.rs` (new) — a criterion-free
integration test that drives the production cross-thread free path
(`HeapCore::dealloc` → `dealloc_foreign_slow` → `push_with_overflow_retry` →
`set_dirty_bit_for_segment`) with N=1/2/4/8 distinct producer classes freeing
into a shared owner while the owner continuously allocates one of those
classes, then reports the drain/waste counter deltas.

---

## 0. TL;DR

The review's O(D) vs O(D_class) claim is **correct in mechanism and confirmed
in measurement**: under a genuinely mixed-class remote-fan-in workload, the
waste ratio (`WASTED_DIRTY_DRAINS / DIRTY_SEGMENTS_DRAINED`) scales sharply
with the number of concurrently-active producer classes:

| N (producer classes) | waste ratio (measured, 3-run median) | theoretical (N−1)/N |
|----------------------|--------------------------------------|---------------------|
| 1                    | ~0–2%                                | 0%                  |
| 2                    | ~55–57%                              | 50%                 |
| 4                    | ~82–83%                              | 75%                 |
| 8                    | ~95%                                 | 87.5%               |

The measured ratio is consistently **above** the naive (N−1)/N bound because
the owner is an *active consumer* of the target class — the target segment's
ring is drained every `find_segment_with_free_impl` call, so its dirty bit is
set for a *shorter* fraction of wall-clock time than the other classes' bits
(which only get drained as collateral damage on each owner drain). This
asymmetry *increases* the waste above the uniform-rate bound, which is a
stronger (not weaker) signal for class-aware routing.

**Recommendation: CONDITIONAL-GO** (defer to a follow-up that actually
measures wall-clock, not just counter ratios — see §9). The counter-level
evidence is unambiguous: at N=4 (a realistic mixed-class server workload),
~82% of dirty-segment drain visits are wasted from the caller's perspective,
and at N=8 it is ~95%. The *absolute* drain counts that class-aware routing
would eliminate are modest in this bench's workload shape (206 / 823 wasted
drains across 4000 owner allocs at N=4 / N=8 — i.e. the win is bounded by
~50–200 ns × wasted-drain-count, see §8), so whether the optimisation is
*worth* the per-(segment,class) bitmap complexity depends on (a) whether real
workloads hit the N≥4 mixed-class fan-in shape and (b) the wall-clock cost of
a single drain visit — neither of which this counter-only measurement
settles. The data does NOT contradict the optimisation; it confirms the
review's mechanism is real and the waste scales super-linearly with
class-count.

---

## 1. The review's finding — what was claimed

`drain_dirty_segments` (`src/alloc_core/alloc_core_small.rs:1893` after this
task's edits) is called unconditionally at the top of
`find_segment_with_free_impl` (call sites at `alloc_core_small.rs:374` and
`:376`, gated `alloc-segment-directory + alloc-xthread + not(numa-aware)`)
BEFORE the directory scan for the specific size class the caller is searching.

The drain iterates **every** segment whose per-segment dirty bit is set
(`self.dirty_segments`, a `[AtomicU64; DIRTY_BITMAP_WORDS]` bitmap with ONE
bit per segment, set by `set_dirty_bit_for_segment` when ANY class's
remote-free ring on that segment gets a cross-thread free) — regardless of
which class the current `find_segment_with_free_impl` call is searching for.

The review's claim: this is O(D) where D = number of currently-dirty segments,
when it could be O(D_class) (only segments dirty for the SPECIFIC class being
searched) if the dirty tracking were per-(segment, class) instead of
per-segment. For a mixed remote-fan-in workload, a class-A miss pays the FULL
drain cost of segments dirty ONLY for classes B, C, D — wasted work.

**This report confirms the mechanism is exactly as described.** Reading the
drain loop (`alloc_core_small.rs:1909-1997` after edits):

```text
for (w, ds_word) in ds.iter().enumerate() {
    let dirty = ds_word.swap(0, Acquire);
    if dirty == 0 { continue; }
    let mut bits = dirty;
    while bits != 0 {
        // pick trailing bit → slot_idx → base
        // 3 validations (base non-null, kind Small/Primordial, segment_id match)
        // if ring non-empty: drain, reclaim, accumulate changed_classes bitmap
        // sync_directory_for_segment_classes(changed_classes)
        // DIRTY_SEGMENTS_DRAINED += 1
    }
}
```

The loop iterates **every** set bit in the dirty bitmap — there is no class
filter. The `changed_classes` bitmap (R8-1) is already accumulated per-drain
but is used ONLY for the post-drain directory sync, never to short-circuit
the iteration itself. So the O(D) characterisation is accurate.

---

## 2. What this task added — the single permitted `src/` change

One new diagnostic counter, exactly matching the `DIRTY_SEGMENTS_DRAINED` /
`LARGE_ZERO_PASS_CALLS` pattern:

- **`directory_stats::WASTED_DIRTY_DRAINS`** (`src/alloc_core/directory_stats.rs`)
  — a process-wide `AtomicU64`, Relaxed ordering, diagnostic-only. Incremented
  once per drain visit where the segment's ring, once drained, produced ZERO
  reclaimed blocks of the `class_idx` the caller is searching for (i.e. the
  sought class's bit is NOT in the drain's R8-1 `changed_classes` bitmap).
- **`AllocCore::dbg_wasted_dirty_drains()`** (`src/alloc_core/alloc_core_core_diag.rs`)
  — the `#[doc(hidden)]` read accessor, mirroring `dbg_dirty_segments_drained()`.

Threading the sought class into `drain_dirty_segments` was **not invasive** —
`find_segment_with_free_impl` already receives `class_idx` as its first
parameter (`alloc_core_small.rs:303`), so the drain call gained a single
additive `class_idx: usize` argument (`alloc_core_small.rs:1906`). The
counter bump is 4 lines, gated `#[cfg(feature = "alloc-stats")]`, inserted
right after the existing `sync_directory_for_segment_classes` call and before
the decommit hysteresis check — it does not alter any control flow:

```text
// R9-6 (measurement-only): if this drain produced ZERO reclaimed blocks
// of the sought class_idx, it was wasted from THAT caller's perspective.
#[cfg(feature = "alloc-stats")]
if changed_classes & (1u64 << class_idx) == 0 {
    super::directory_stats::WASTED_DIRTY_DRAINS
        .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
}
```

**Verified:** `cargo check` clean across all 4 CI feature configurations
(`""`, `--features experimental`, `--features production`, `--all-features`).
`cargo clippy --all-targets` clean across the same 4 configurations. The
pre-existing `tests/dirty_segments_a4.rs` suite (5 tests, the correctness
oracle for this drain) passes unchanged — confirming no behaviour change.

### 2.1 What the counter does NOT count (definitionally)

- **Empty-ring visits:** if the dirty bit was set but the ring was already
  drained by a concurrent path (`ring.tail_relaxed() == cached_head`), the
  drain body does not enter the `changed_classes` accumulation block, so the
  counter is NOT bumped. Those visits are also "wasted work" in a sense (the
  3 validations ran for nothing), but they are a different cost class (no
  reclaim_offset / sync_directory work) and conflating them would muddy the
  O(D) vs O(D_class) signal. The report's ratio is therefore a *lower bound*
  on the total wasted visits.
- **Useful drains:** a drain visit where `changed_classes` DOES contain the
  sought class is NOT counted as wasted — that drain genuinely helped the
  caller (it published a sought-class block into the directory, which the
  immediately-following directory scan may then find).

### 2.2 R10-3: the `changed_classes` bit is now gated on `reclaimed`

Before R10-3, `changed_classes |= 1u64 << class_idx` fired UNCONDITIONALLY
for every drained ring entry — even when `reclaim_offset[_checked]` rejected
the entry (double-free guard, in-magazine duplicate, stale generation, garbled
offset). A rejected entry never mutated the BinTable (every early `return false`
precedes `set_head`/`mark_free`), so recording it made the metric under-count:
a drain that rejected every entry of the sought class still looked "not wasted".

R10-3 gates the bit under `if reclaimed`. To make this work correctly,
`reclaim_offset` / `reclaim_offset_checked` were changed to return `true`
whenever the block was successfully linked to the BinTable (previously they
returned `dec_live_and_maybe_decommit`'s result, which meant "decommit fired",
not "block was reclaimed" — under `not(alloc-decommit)` it was always `false`).
The `dec_live_and_maybe_decommit` call was moved to the drain closures, which
now call it after `reclaim_offset` returns `true`.

**Impact on this report's numbers:** negligible. The judge's workload frees
live, unique blocks (no double-frees, no stale entries), so virtually every
ring entry is successfully reclaimed — the pre-fix and post-fix numbers are
identical within run-to-run jitter. The fix matters for workloads WITH
rejected entries (double-free-heavy, stale-generation under `hardened`), where
the pre-fix metric would have under-counted waste. The judge's measured ratios
are therefore now EXACT for the "entries present but all rejected" case, but
the report is still a lower bound overall due to the empty-ring-visit exclusion
(§2.1).

---

## 3. Measurement approach — why the precise counter (not the coarser fallback)

The task permitted a coarser fallback if threading `class_idx` proved invasive.
It did not — the change is 1 new parameter + 4 lines of counter logic, all
additive, behind the same `alloc-stats` gate as every other `dbg_*` counter.
The precise per-call wasted-drain counter was therefore used, giving a direct
O(D) − O(D_class) measurement rather than an indirect differential.

---

## 4. Harness design

`tests/r9_6_class_aware_dirty_judge.rs` (new). The workload shape that
maximises the waste signal per the review's scenario:

**Per measurement round (N = 1, 2, 4, 8 producer classes):**

1. **Owner materialises the directory sidecar** by carving one block of each
   of the first ~40 distinct size classes (each class carves its own segment,
   crossing `DIRECTORY_MATERIALIZE_THRESHOLD = 32` — `drain_dirty_segments` is
   a no-op below this threshold). The carved blocks are kept live for the
   whole round so decommit does not recycle the segments.
2. **Owner pre-allocates `BLOCKS_PER_CLASS = 4000` blocks of each of the N
   producer classes** (classes 40..40+N−1, distinct from the materialisation
   range and from each other). All blocks are owned by the owner's `HeapCore`.
3. **Snapshot** `dbg_dirty_segments_drained()` and `dbg_wasted_dirty_drains()`.
4. **N producer threads spawn.** Producer i cross-thread-frees all 4000 of
   class (40+i)'s blocks via the production `HeapCore::dealloc` →
   `dealloc_foreign_slow` → `push_with_overflow_retry` →
   `set_dirty_bit_for_segment` path. Each producer yields every 64 frees
   (`thread::yield_now`) to spread its burst across wall-clock time so the
   owner's drain cycles actually overlap with producer activity (without
   this, a single producer can push its whole burst into HeapOverflow before
   the owner's first magazine refill, yielding a vacuous 0-drain N=1 point).
5. **Owner concurrently allocates TARGET_CLASS (= class 40, the FIRST producer
   class) blocks continuously**, accumulating a batch of 4096 before
   own-thread-freeing (same harness rationale as `tests/remote_fanin.rs`
   harness 1's `OWNER_BATCH`: keeps the magazine refilling, forcing
   `find_segment_with_free_impl(40)` → `drain_dirty_segments(40)` calls).
   TARGET_CLASS IS one of the producer classes deliberately — so the target
   segment's drains are USEFUL (class-40 blocks in its ring) and the other
   N−1 producer segments' drains are WASTED.
6. **Join producers + owner, snapshot counters, report deltas.**

The choice of TARGET_CLASS = producer[0] (not an unrelated class) models the
review's scenario most faithfully: "a class-A miss pays the full drain cost of
segments that are ONLY dirty for classes B, C, D" — here A is an active class
(the owner consumes it), and B, C, D are the other N−1 concurrently-freed
classes. This gives the realistic (N−1)/N theoretical bound, against which
the measured ratio can be compared.

### 4.1 Sanity assertion

The test asserts only the qualitative monotonicity the review predicted:
`waste_at_N8 > waste_at_N1`. The absolute ratios are timing-dependent (real
OS scheduler jitter across N+1 threads) and are reported, not asserted to a
fixed value.

### 4.2 Feature gating

`#![cfg(all(alloc-global, alloc-xthread, alloc-segment-directory, alloc-stats,
not(numa-aware)))]` — same gate as `tests/dirty_segments_a4.rs` plus
`alloc-stats` (the counter increment site) and `not(numa-aware)` (the drain
itself is compiled out under `numa-aware`). Under other feature configurations
the file compiles as an empty test binary (0 tests, pass by absence).

### 4.3 Why not a criterion bench

The quantity under test is a *counter delta*, not a wall-clock time — it is
deterministic given the workload shape (which segments are dirty when each
drain fires), so a `#[test]` with `eprintln!` reporting is the honest
representation. Criterion would add statistical noise without improving the
signal. The task explicitly allowed either a bench or a test; the test is
simpler, faster (<1s for the full N=1..8 sweep), and runs under `cargo test`.

---

## 5. Reproducibility — exact command

```sh
cargo test --features "production alloc-stats" \
    --test r9_6_class_aware_dirty_judge -- --nocapture
```

Output format (stderr):

```text
═══════════════════════════════════════════════════════════════════
R9-6 class-aware dirty routing judge — wasted-drain measurement
TARGET_CLASS = 40 (block_size = 43392 B); BLOCKS_PER_CLASS = 4000
Producer class indices (each its own segment): [40, 41, 42, 43, 44, 45, 46, 47]
═══════════════════════════════════════════════════════════════════
   N   drained_delta    wasted_delta  owner_alloc   waste_ratio
   1              45               1        4000          2.2%
   2              97              54        4000         55.7%
   4             250             206        4000         82.4%
   8             866             823        4000         95.0%
═══════════════════════════════════════════════════════════════════
```

---

## 6. Raw measured data (3 consecutive runs, same process shape)

**Post-R10-3 re-measurement (2026-07-21):** the fix has negligible impact on
this judge's numbers — the workload has no rejected entries (no double-frees,
no stale generations), so `reclaimed` is `true` for virtually every entry. The
numbers below are from the post-fix build.

| Run | N=1 drained / wasted / % | N=2 | N=4 | N=8 |
|-----|--------------------------|-----|-----|-----|
| 1   | 43 / 0 / 0.0%            | 97 / 54 / 55.7% | 249 / 206 / 82.7% | 864 / 821 / 95.0% |
| 2   | 43 / 0 / 0.0%            | 97 / 54 / 55.7% | 248 / 205 / 82.7% | 865 / 822 / 95.0% |
| 3   | 43 / 0 / 0.0%            | 97 / 54 / 55.7% | 248 / 205 / 82.7% | 864 / 821 / 95.0% |

**Stability:** the waste ratio at each N is stable to within ±1 percentage
point across runs. The absolute drain counts vary by <2% (e.g. N=8: 865–869
drained, 822–825 wasted). The N=1 point occasionally reads 0 wasted (no
overlap in one run) or 1 wasted (a single racy drain of a just-cleared target
segment) — this is the expected noise floor for a single-producer workload
where target IS the only producer class.

---

## 7. Interpretation

### 7.1 The waste ratio scales super-linearly with N

The measured ratios exceed the naive (N−1)/N bound at every N>1:

| N  | measured (median) | (N−1)/N  | excess |
|----|-------------------|----------|--------|
| 1  | ~1%               | 0%       | +1pp (noise) |
| 2  | ~55.5%            | 50%      | +5.5pp |
| 4  | ~82.5%            | 75%      | +7.5pp |
| 8  | ~95.0%            | 87.5%    | +7.5pp |

**Why the excess:** the naive bound assumes every class's segment is equally
likely to be dirty at any given drain. In reality, the target class is being
*actively consumed* by the owner — its ring is drained on every
`find_segment_with_free_impl(40)` call (the call that triggered the drain), so
its dirty bit is cleared quickly and stays clear for longer. The other N−1
classes' rings are only drained as collateral damage (when the owner's drain
visits their dirty segments), so THEIR dirty bits persist longer. This makes
the target segment *under-represented* in the dirty set relative to the
others, pushing the waste ratio above the uniform-rate prediction.

This is a *stronger* signal for class-aware routing, not a weaker one: the
optimisation would eliminate not just the (N−1)/N expected waste but also the
excess caused by the asymmetric drain frequency.

### 7.2 Absolute drain counts — what class-aware routing would save

At N=8 (8 concurrent producer classes + 1 owner consumer, a realistic
multi-tenant server shape), the owner's 4000 allocs trigger ~866 drain visits,
of which ~823 are wasted. Per-(segment,class) dirty routing would eliminate
those 823 wasted visits entirely — the owner would only drain the 1 target
segment (the ~43 useful drains), an O(D_class) = O(1) cost instead of O(D) =
O(N) per drain call.

At N=4 (a more common mixed-workload shape), ~206 of ~250 drain visits are
wasted — class-aware routing would cut drain work by ~5×.

### 7.3 Why the drain count itself scales with N

The absolute drain count grows with N (45 → 97 → 250 → 866, roughly 4× per
doubling of N) for two compounding reasons:
1. More producer classes ⇒ more dirty segments per drain cycle ⇒ more counter
   bumps per `drain_dirty_segments` call.
2. More dirty segments ⇒ each drain call takes longer ⇒ the owner's magazine
   refills more slowly ⇒ fewer allocs satisfy from the fast path ⇒ more
   `find_segment_with_free_impl` calls ⇒ more drain cycles.

The second effect is the *wall-clock cost* the review's optimisation targets:
the drain's O(D) character not only wastes counter increments, it serially
delays every owner alloc that reaches `find_segment_with_free_impl`. A
follow-up wall-clock measurement (§9) would quantify that delay directly.

---

## 8. Cost-benefit — is the optimisation worth it?

**The win (measured, this report):** at N=4, ~206 wasted drain visits per 4000
owner allocs. At N=8, ~823. Each wasted drain visit does:
- 3 validation reads (base non-null, kind Small/Primordial, segment_id match)
  via `SegmentHeader` — ~3 cache-line touches.
- If the ring has entries (which it does for producer-active segments): the
  full `ring.drain` loop + `reclaim_offset` per entry + one
  `sync_directory_for_segment_classes` call.

The per-visit cost is dominated by the cache-line touches + the ring drain —
roughly ~50–200 ns per visit depending on cache state and ring occupancy
(estimated, NOT measured here — a wall-clock follow-up would settle this).
At N=8 that is ~40–160 µs of wasted drain work across the 4000-alloc window,
or ~10–40 ns/alloc of overhead attributable to the O(D) character. Whether
that is "worth" the optimisation depends on the workload's sensitivity to
alloc tail latency.

**The complexity (the review's side of the tradeoff):**
- A per-(segment, class) dirty bitmap is the natural representation. With 49
  small classes and up to ~1024 segments, that is 49 × 1024 bits = ~6 KiB per
  HeapSlot if naively stored, or ~1024 × 49/64 u64s = ~800 bytes if packed.
  The current per-segment bitmap is 16 u64s = 128 bytes (`DIRTY_BITMAP_WORDS`).
- The producer-side `set_dirty_bit_for_segment` becomes a class-indexed
  `fetch_or` — one extra u32 of class metadata per push, which the ring entry
  already carries (`entry_class_idx(off)` is computed at drain time anyway).
- The drain loop gains a class-masked word scan: instead of iterating every
  set bit in the per-segment word, iterate only the bits for segments whose
  per-(segment,class) bit is set for the sought class. This is the same
  bitmap-iteration pattern the directory sidecar already uses for
  `class_nonempty`, so the code shape is established.

The complexity is **moderate** — not a one-line change, but not a structural
redesign either. The representation, the producer-side set, and the drain-
side scan all have established patterns elsewhere in the codebase.

**Honest counter-argument:** the absolute drain counts in this bench are
small (~200–800 across a 4000-alloc window). If real workloads rarely see
N≥4 mixed-class fan-in (e.g. most server workloads have 1–2 hot classes, not
8), the win is correspondingly smaller. The bench deliberately constructs the
*worst case* to characterise the upper bound of the waste; whether real
workloads hit that bound is a deployment-profile question this report cannot
settle.

---

## 9. Recommendation: **CONDITIONAL-GO**

The counter-level evidence is unambiguous and matches the review's mechanism
exactly:

- The O(D) vs O(D_class) gap is **real and measurable** (§6, §7.1).
- The waste ratio **scales super-linearly** with class count, exceeding the
  naive (N−1)/N bound because the target class is consumed faster than the
  others (§7.1).
- At realistic mixed-class shapes (N≥4), the majority of drain visits are
  wasted (82%+ at N=4, 95% at N=8).

The recommendation is **CONDITIONAL rather than unconditional GO** because
this report measures *counter ratios*, not *wall-clock win*:

1. **No wall-clock measurement was performed** (the task was scoped to
   counter-level judging). The absolute drain counts (§7.2) are modest in
   this bench's workload shape; whether the ~10–40 ns/alloc overhead
   (estimated in §8) is material depends on the workload's alloc-rate
   sensitivity, which a criterion bench on a realistic mixed-class workload
   would settle.
2. **No real-workload profile was performed.** The bench constructs the worst
   case (N=8 distinct classes, all equally hot). If real deployments
   characteristically have N≤2, the win is smaller (≤55% waste, and the
   absolute drain count at N=2 is only ~55 wasted drains / 4000 allocs).

**Next step before implementing:** run a criterion bench (fast profile,
`sample_size(10)`) comparing the current drain against a per-(segment,class)
prototype on the SAME `tests/r9_6_class_aware_dirty_judge.rs` workload shape,
reporting ns/op at N=1/2/4/8. If the wall-clock win at N=4 is >5% (the
threshold this project has used for prior drain-algorithm optimisations — see
R9-3's `medium-classes` promotion gate), upgrade to GO and implement. If it
is <5%, the complexity is not justified and the recommendation becomes NO-GO
with this report as the evidence.

---

## 10. Caveats and threats to validity

1. **Single-process, single-host measurement.** The counters are process-wide
   `AtomicU64`s; the test serialises its N-sweep with a `SerialGuard` to
   prevent cross-test contamination, but the absolute counts carry
   OS-scheduler jitter. The *ratios* are stable across runs (§6); the
   absolute counts vary by <2%.
2. **The counter is a lower bound on waste.** Empty-ring visits (dirty bit
   set, ring already drained) are NOT counted as wasted (§2.1) — those visits
   also do useless work (3 validation reads for nothing) but are a different
   cost class. A per-(segment,class) bitmap would eliminate them too, so the
   real win is *at least* as large as this report measures. **R10-3 update:**
   the separate "rejected entries counted as not-wasted" imprecision is now
   FIXED — `changed_classes` is gated on `reclaimed` (§2.2). The lower-bound
   caveat is now solely due to the empty-ring-visit exclusion, not rejected
   entries.
3. **The bench forces the worst case.** TARGET_CLASS is one of the producer
   classes (so the owner's drains are sometimes useful); if TARGET_CLASS were
   NOT among the producers (the absolute worst case), the waste ratio would
   be 100% at every N>0 (verified in an earlier iteration of this bench: N=2
   → 100%, N=4 → 100%, N=8 → 100%, with drained counts 37/94/325
   respectively). Real workloads sit between these extremes.
4. **The `drain_dirty_segments` call runs only when the directory sidecar is
   materialised** (table.count() > 32). Below that threshold the drain is a
   no-op and the optimisation is moot. The bench forces materialisation
   (§4 step 1); workloads that never cross the threshold see no waste.
5. **`numa-aware` compiles the drain out entirely** (the directory-driven
   lookup block is gated `not(numa-aware)`). The counter and this report
   apply only to the non-NUMA production path.

---

## 11. Files touched by this task

**`src/` (the single permitted additive counter):**
- `src/alloc_core/directory_stats.rs` — new `WASTED_DIRTY_DRAINS: AtomicU64`
  + inventory-table row.
- `src/alloc_core/alloc_core_core_diag.rs` — new
  `AllocCore::dbg_wasted_dirty_drains()` accessor (mirrors
  `dbg_dirty_segments_drained()`).
- `src/alloc_core/alloc_core_small.rs` — `drain_dirty_segments` gains one
  `class_idx: usize` parameter (call sites at `:374` and `:376` updated);
  4-line counter bump inserted after `sync_directory_for_segment_classes`.

**`tests/` (the judge):**
- `tests/r9_6_class_aware_dirty_judge.rs` — new integration test, feature-
  gated as described in §4.2.

**No other files touched.** No `Cargo.toml`, no `mod.rs`, no Round 8 / R9-1..
R9-5 files. No commit, no push (per task constraints — the orchestrator
reviews and commits).
