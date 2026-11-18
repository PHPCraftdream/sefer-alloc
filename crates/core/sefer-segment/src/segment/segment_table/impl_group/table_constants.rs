//! Fixed SegmentTable storage geometry and diagnostic counters.
//!
//! Several items here are consumed only by the root `sefer-alloc` crate
//! (cross-crate re-exports), so the in-crate dead-code lint is a false
//! positive.
#![allow(dead_code)]

use core::mem::size_of;

/// Maximum number of simultaneously live segments the registry can hold WITHOUT
/// recycling. Each live large/huge allocation consumes one segment slot; each
/// small segment can serve thousands of small allocations. Under `alloc-decommit`
/// the effective limit is unbounded: empty segments (including empty small
/// segments) are recycled (their slot is NULLed) and reused by future
/// `register` calls. Without `alloc-decommit`, only the EMPTY small-segment
/// recycle path is disabled; the free-list still recycles any released segment
/// slot (large allocations still recycle their slot on free via
/// `unregister`/`recycle`, unconditionally). Long-running processes with many
/// small-segment carve/decay cycles will pin small-segment slots and eventually
/// hit this cap.
pub const MAX_SEGMENTS: usize = 4096;

/// The footprint of the registry (slots) array in the primordial segment. Fixed
/// and known at compile time so the bootstrap can carve it deterministically.
pub const REGISTRY_FOOTPRINT: usize = MAX_SEGMENTS * size_of::<*mut u8>();

/// Capacity of the open-addressing hash table (OPT-B). Load factor ≤ 50%
/// so `HASH_CAPACITY = 2 * MAX_SEGMENTS`. Power of two for cheap modulo via
/// bitmasking.
pub const HASH_CAPACITY: usize = 2 * MAX_SEGMENTS; // 8192

/// Footprint of the hash table in the primordial segment.
/// `HASH_CAPACITY` entries × `sizeof(*mut u8)` = 64 KiB.
pub const HASH_FOOTPRINT: usize = HASH_CAPACITY * size_of::<*mut u8>();

/// Capacity of the free-list stack of recyclable slot indices (task #135,
/// Part 1). One `u32` per possible slot — the free-list can never hold more
/// entries than there are slots (`MAX_SEGMENTS`), so this bound is exact (no
/// separate overflow path is needed).
pub const FREE_LIST_CAPACITY: usize = MAX_SEGMENTS;

/// Footprint of the free-list stack (the index array only; the top-of-stack
/// counter is a separate `u32` field carved right after it — see
/// `Layout::primordial_free_list_off` / `primordial_free_top_off`).
pub const FREE_LIST_FOOTPRINT: usize = FREE_LIST_CAPACITY * size_of::<u32>();

/// PERF-P2 (eureka Э3) — number of slots in the direct-mapped own-segment
/// cache. A power of two (indexed by masking) kept deliberately tiny (start
/// small, measure before growing). The cache holds ONLY stored pointers
/// returned by a won `hash_find` probe (it *remembers proven*, never *asserts*),
/// and every table-mutation path that can remove a base
/// (`unregister`/`recycle`) clears the matching slot IN THE SAME FUNCTION that
/// mutates the hash — so complete invalidation is structural, not a
/// remember-to-invalidate discipline scattered across call sites.
///
/// R32-10 (task #501, F2): raised 4 -> 16 after MEASURING (not assuming) the
/// original value's real-world cost. `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`
/// §F2 identified that a Large-heavy workload with N concurrently-live
/// Large objects (N > 4) thrashes a 4-entry direct-mapped cache by
/// construction, but `docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE.md` §1.3/§6.2
/// had found no PORTABLE way to force this Tier-2 fallback to build a judge
/// (the cache index depends on OS-assigned addresses). This round built the
/// missing instrument instead of relying on prediction: a process-wide
/// Tier-1 hit/miss counter (`CONTAINS_BASE_TIER1_HITS`/`_MISSES`,
/// `bench-internals`-gated, this same file's `contains_base`) plus an
/// OBSERVATIONAL workload (`examples/r32_10_own_cache_tier1_thrash_gate.rs`)
/// that repeatedly `realloc`s (in-place, same size — no free, no unregister)
/// K concurrently-live Large objects and reads the REAL resulting hit rate
/// back from the allocator's own counters — sidestepping the address-
/// prediction problem entirely.
///
/// **Measured (see `docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE.md` for
/// the full sweep, raw logs, and derivation script):** at `OWN_CACHE_SIZE=4`,
/// EVERY tested K (4/8/16/24/32/48/64) thrashed completely — 0.00% Tier-1
/// hit rate, even at K == OWN_CACHE_SIZE (confirming the survey's own point:
/// the cache is direct-mapped, not associative, so even N == cache-size only
/// avoids collision if the OS happens to hand out bases whose bits 22+ don't
/// collide — which it did not on this measured run). At `OWN_CACHE_SIZE=16`:
/// K=4 and K=8 jump to 99.99% hit rate (32764-65528 hits out of
/// 32768-65536 calls, both cache sizes' runs 7-repetition median), K=16+
/// still thrashes (0.00%, one K=32 outlier run showing partial ~9.4% from a
/// favorable single-launch address collision pattern the rest of that arm's
/// repetitions did not reproduce) — a genuine, large, reproducible hit-rate
/// win at the K range this cache size can actually hold. The LATENCY delta
/// on the SAME workload/arms was NOT clearly separable from this harness's
/// own run-to-run noise (~24-29 ns/op across every arm regardless of hit
/// rate) — an honest null on the wall-clock axis, consistent with OPEN_ITEMS
/// item 1's own component pricing (a Tier-1 hit vs Tier-2 miss differs by
/// only ~4 Ir, a few ns at most, small against the whole `realloc` call's
/// cost). The standing ±10 raw-Ir churn kill-gate (four small-object
/// benches) is untouched by this change — it only affects `SegmentTable`'s
/// per-heap struct size and the Large-object ownership check. Must stay a
/// power of two — `cache_index` masks with
/// `OWN_CACHE_SIZE - 1`.
pub(crate) const OWN_CACHE_SIZE: usize = 16;

/// Bit-shift of the segment size (log₂(SEGMENT = 4 MiB) = 22). Used by the
/// hash function to convert a segment base into a table index. Exposed as
/// `pub` so the bootstrap can seed the primordial hash entry before
/// the `SegmentTable` struct exists (the bootstrap must not call any
/// `SegmentTable` methods before `from_primordial`).
pub const SEGMENT_SHIFT: usize = 22;

/// R23-6 (task #375) DIAGNOSTIC ONLY: process-wide count of hash-slot probe
/// steps performed by a SINGLE `SegmentTable::hash_remove` call's
/// backward-shift scan (the `j = (j+1) & mask` walk documented on
/// `hash_remove` — one increment per slot visited while scanning the cluster
/// for a shift candidate). This is the exact quantity
/// `tests/regression_segment_table_tombstone_rebuild.rs`'s
/// `backshift_no_latency_spike_at_threshold_boundary` wall-clock check was
/// trying to approximate: "no single delete does more than a small,
/// cluster-bounded amount of work — never `O(HASH_CAPACITY)`". A wall-clock
/// nanosecond measurement is a noisy PROXY for this; this counter is the
/// exact thing.
///
/// Deliberately a MAX, not a running sum: the correctness claim under test is
/// a per-call BOUND ("no single delete is a dramatic outlier"), not a total
/// amount of work across many deletes — a sum would conflate "many small
/// deletes" with "one large delete" the same way the wall-clock test's own
/// max/median ratio does NOT (it isolates the single worst call). Reset via
/// [`reset_hash_remove_max_scan_steps`] at the start of a measurement window
/// so each test/bench run reads a clean high-water mark.
///
/// **This counter has ZERO effect on allocator behavior** — `hash_remove`'s
/// control flow is unchanged; this only counts steps the walk was already
/// going to take. Read via
/// `AllocCore::dbg_hash_remove_max_scan_steps`.
/// Reads 0 unless `alloc-stats` is on — the per-step increment is gated
/// behind `alloc-stats`, matching
/// `OPT_H_ATTEMPTS`'s diagnostic convention; the
/// static itself is always compiled so the accessor has a stable definition
/// regardless of the rest of the feature set. Relaxed ordering — a
/// diagnostic count, not a synchronization primitive.
// R6-T16: written by the alloc-stats-gated `hash_remove` probe and read by
// the internals diag accessor; suppressed exactly where both are absent.
// Suppressed via the module-level `#![allow(dead_code)]` above instead of the
// original `#[cfg_attr(...)]` (P2.1 split into `table_constants.rs`).
pub static HASH_REMOVE_MAX_SCAN_STEPS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// R23-6 (task #375): reset [`HASH_REMOVE_MAX_SCAN_STEPS`] to 0. Test/bench
/// hook — lets a measurement window (e.g. one wave of a threshold-boundary
/// test) start from a clean high-water mark instead of accumulating across
/// the whole process lifetime.
#[doc(hidden)]
#[cfg(feature = "internals")]
#[inline]
pub fn reset_hash_remove_max_scan_steps() {
    HASH_REMOVE_MAX_SCAN_STEPS.store(0, core::sync::atomic::Ordering::Relaxed);
}

/// oxx R2-06 (independent src review round 2): process-wide count of
/// `SegmentTable::recycle` calls whose `base` argument was NOT found as a
/// live member of the hash table at all. In that case `recycle` releases
/// nothing (see its doc comment) — a leak, never a double-free/UB — because
/// the segment's header at an unverified `base` must not be trusted (it may
/// be unmapped, foreign, or garbage). Reachable only via caller corruption
/// (a stale/garbage `base`) or a genuine double-recycle bug; never through a
/// normal decommit → recycle call. Always compiled (not feature-gated) so
/// the diagnostic accessor has a stable definition; stays at 0 unless this
/// branch is ever actually taken. Relaxed ordering — a diagnostic count, not
/// a synchronization primitive.
// R6-T16: bumped by the alloc-decommit-gated `recycle` anomaly branch and
// read by the internals diag accessor; suppressed where both are absent.
// Suppressed via the module-level `#![allow(dead_code)]` above instead of the
// original `#[cfg_attr(...)]` (P2.1 split into `table_constants.rs`).
pub static SEGMENT_RECYCLE_UNVERIFIED_BASE_TOTAL: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
