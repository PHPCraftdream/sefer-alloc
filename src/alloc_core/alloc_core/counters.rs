//! Bench/diagnostic static counter bank of [`AllocCore`] (mechanical split of
//! the former flat `alloc_core.rs`; pure code movement, no behavior changed).
//!
//! Every static here is ALWAYS compiled (so the `dbg_*` accessors have a
//! stable definition regardless of feature set); the per-event INCREMENTS
//! live at their call sites and are feature-gated there. The path-parity
//! re-exports for cross-module consumers live in the parent `mod.rs`.

/// TEST-ONLY (Phase 35): process-wide M6-decommit invocation counter. Bumped in
/// `decommit_empty_segment_impl` (the shared decommit body); read by the soak
/// test via [`AllocCore::dbg_decommit_count`]. Diagnostic only (relaxed).
#[cfg(feature = "alloc-decommit")]
pub(in crate::alloc_core) static DECOMMIT_CALLS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// R32-8 (task #499, F9): process-wide path-activation oracle for
/// `AllocCore::maybe_decay_large_cache`'s fast-path guard — counts calls that
/// passed the `large_cache_used_bytes <= headroom_bytes` early-exit and
/// therefore reached the `std::time::Instant::now()` read. Bumped only when
/// `bench-internals` is on (a plain `alloc-decommit` production build never
/// touches this counter, matching the R30-8 CLAUDE.md convention: this is a
/// measurement-only instrument, not a production code-path change). Read via
/// [`AllocCore::dbg_maybe_decay_guard_passed_count`]. Diagnostic only
/// (Relaxed, like `DECOMMIT_CALLS`).
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
pub(in crate::alloc_core) static MAYBE_DECAY_GUARD_PASSED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// R32-8 (task #499, F9): process-wide override switch used ONLY by the F9
/// clock-read-cost A/B probe (`examples/r32_8_large_cache_decay_clock_read_ab_gate.rs`)
/// to isolate `maybe_decay_large_cache`'s `Instant::now()` cost from the
/// headroom/hit-rate effect. When `true`, `maybe_decay_large_cache` skips its
/// `large_cache_used_bytes <= headroom_bytes` fast-path early-exit and always
/// proceeds to the clock read, REGARDLESS of whether the cache is above or
/// below headroom — i.e. it forces the "guard fails" cost shape onto a
/// workload that would otherwise take the "guard passes" fast exit. This lets
/// two runs at the IDENTICAL `headroom_bytes` (so hit rate is structurally
/// unchanged — R31-1's headroom-changes-hit-rate confound is impossible by
/// construction, since headroom is not touched) differ ONLY in whether the
/// clock read executes. `bench-internals`-gated: never reachable in a plain
/// `production` build, so this cannot affect real allocator behavior.
/// Default `false` (the real guarded behavior). Diagnostic/test-only
/// (Relaxed) — no ordering obligation, matches `DECOMMIT_CALLS`.
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
pub(in crate::alloc_core) static FORCE_DECAY_CLOCK_READ: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// R32-10 (task #501, F2): process-wide path-activation oracle for
/// `SegmentTable::contains_base`'s Tier-1 direct-mapped `own_cache` — counts
/// calls that HIT the 4 (now `OWN_CACHE_SIZE`)-entry cache without falling
/// through to the Tier-2 `hash_contains` open-addressing probe. Paired with
/// [`CONTAINS_BASE_TIER1_MISSES`] (which counts the complementary fall-
/// through case), this is the counter
/// `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` finding F2 and
/// `docs/perf/OPEN_ITEMS.md` item 1's own last-open clause ("Tier-2-hash-
/// probe-heavy workloads might show `contains_base` > 8.8% (open, not a
/// proven floor)") both name as never having been built: R23-3 §1.3 could
/// only measure Tier-2's cost in isolation via a bypass hook
/// (`SegmentTable::dbg_hash_contains_only`), never observe HOW OFTEN real
/// production traffic actually falls through to it. Bumped only when
/// `bench-internals` is on (matching [`MAYBE_DECAY_GUARD_PASSED`]'s
/// convention: a plain production build never touches this counter, so it
/// cannot affect real allocator behavior or add overhead to a release
/// build). Read via
/// [`AllocCore::dbg_contains_base_tier1_hits`](alloc_core_core_diag).
/// Diagnostic only (Relaxed, like `DECOMMIT_CALLS`).
#[cfg(feature = "bench-internals")]
pub(in crate::alloc_core) static CONTAINS_BASE_TIER1_HITS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// R32-10 (task #501, F2): the Tier-2-fallback complement of
/// [`CONTAINS_BASE_TIER1_HITS`] — counts `contains_base` calls whose Tier-1
/// `own_cache` probe MISSED and therefore fell through to the Tier-2
/// `hash_contains` open-addressing probe (regardless of whether that Tier-2
/// probe itself found `base` or not — this counts ROUTING, i.e. which tier
/// did the work, not membership). `tier1_hit_rate = hits / (hits + misses)`
/// is the quantity item 1's open clause and F2's own text both ask for.
/// Same gating/ordering/read-accessor convention as
/// [`CONTAINS_BASE_TIER1_HITS`].
#[cfg(feature = "bench-internals")]
pub(in crate::alloc_core) static CONTAINS_BASE_TIER1_MISSES: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// TEST-ONLY (R9-1, task #221 follow-up): process-wide count of EXPLICIT
/// `Node::zero` passes on the Large-classified `alloc_zeroed` path — bumped
/// at both consumers of `alloc_large`'s freshness signal
/// ([`AllocCore::alloc_zeroed`] and `HeapCore::alloc_zeroed`) each time they
/// actually zero (i.e. the fresh-reservation skip did NOT fire). Read via
/// [`AllocCore::dbg_large_zero_pass_count`]. This is the seam that makes
/// `tests/alloc_zeroed_fresh_large_skip.rs` sensitive to the OPTIMIZATION
/// itself, not just the safety contract: with an unconditional memset
/// reintroduced, the fresh-path tests observe a nonzero delta and go red
/// (byte-content assertions alone cannot distinguish "skipped because
/// OS-zeroed" from "zeroed redundantly"). Diagnostic only (Relaxed, like
/// `DECOMMIT_CALLS`); the increment sits on a path already doing multi-KiB
/// zeroing or a fresh OS reservation, so its cost is noise.
///
/// Reads 0 unless `alloc-stats` is on — the per-event increments (in
/// [`AllocCore::alloc_zeroed`] and `HeapCore::alloc_zeroed`) are gated behind
/// `alloc-stats`, matching the established convention for diagnostic counters
/// (`WASTED_DIRTY_DRAINS`, `FOREIGN_OR_UNROUTABLE_FREES`); the static itself is
/// always compiled so [`AllocCore::dbg_large_zero_pass_count`] has a stable
/// definition regardless of the feature set.
pub(crate) static LARGE_ZERO_PASS_CALLS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// TEST-ONLY (R12-10, task #261, `virgin-zero-skip`): process-wide count of
/// EXPLICIT `Node::zero` passes on the Small-classified `alloc_zeroed` path —
/// bumped at both consumers of [`AllocCore::alloc_small_with_virgin`]'s
/// freshness signal ([`AllocCore::alloc_zeroed`] and `HeapCore::alloc_zeroed`)
/// each time they actually zero (i.e. the virgin-carve skip did NOT fire).
/// Read via [`AllocCore::dbg_small_zero_pass_count`]. Mirrors
/// [`LARGE_ZERO_PASS_CALLS`] exactly — this is the seam that makes
/// `tests/alloc_zeroed_virgin_small_skip.rs` sensitive to the OPTIMIZATION
/// itself, not just the safety contract: with an unconditional memset
/// reintroduced (the virgin-skip silently disabled), the fresh-carve tests
/// would observe a nonzero delta and go red — byte-content assertions alone
/// cannot distinguish "skipped because OS-zeroed" from "zeroed redundantly"
/// (the same reasoning `LARGE_ZERO_PASS_CALLS`'s doc states).
///
/// Reads 0 unless BOTH `virgin-zero-skip` AND `alloc-stats` are on — the
/// per-event increments are gated behind `alloc-stats` (matching
/// `LARGE_ZERO_PASS_CALLS`'s convention); the static itself is always
/// compiled (regardless of `virgin-zero-skip`) so
/// [`AllocCore::dbg_small_zero_pass_count`] has a stable definition across
/// every feature set — a build without `virgin-zero-skip` simply never
/// increments it (the Small `alloc_zeroed` arm always zeroes explicitly
/// there, so the counter would read as "every alloc_zeroed counted", which
/// is not the diagnostic's intent) — the increment call sites are
/// additionally gated on `virgin-zero-skip` so the counter reads exactly 0 in
/// a build without the feature, keeping its behaviour indistinguishable from
/// "the feature doesn't exist" absent explicit opt-in.
pub(crate) static SMALL_ZERO_PASS_CALLS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// STAGE-1 DIAGNOSTIC ONLY (R21-2, task #351,
/// `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §6.1/§8 step 1):
/// process-wide count of times [`AllocCore::realloc_inplace_fast_path_known_base`]
/// reaches a cross-class Small/Primordial grow attempt that OPT-H (a
/// **proposed, NOT YET IMPLEMENTED** in-place tail-of-segment grow mechanism —
/// see the design doc) would need to evaluate: `old_class`/`new_class` both
/// resolve to Some, `new_class != old_class`, and
/// `block_size(new_class) > block_size(old_class)` (design §2.1 precondition
/// 1). This is the Stage-1 hit-rate DENOMINATOR. Paired with
/// [`OPT_H_HITS`] (the numerator: how often ALL SIX preconditions hold).
///
/// **This counter has ZERO effect on allocator behavior.** No grow action is
/// taken here — the call site that increments this counter still falls
/// through to `None` exactly as before this counter existed, letting the
/// caller's existing promotion/move-leg path run unchanged. Only observation.
///
/// Read via [`AllocCore::dbg_opt_h_attempts`]. Reads 0 unless `alloc-stats` is
/// on — the per-event increment is gated behind `alloc-stats`, matching
/// [`LARGE_ZERO_PASS_CALLS`]'s convention; the static itself is always
/// compiled so the accessor has a stable definition regardless of the rest of
/// the feature set. Relaxed ordering — a diagnostic count, not a
/// synchronization primitive.
pub(crate) static OPT_H_ATTEMPTS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// STAGE-1 DIAGNOSTIC ONLY (R21-2, task #351): process-wide count of times
/// the cross-class Small/Primordial grow attempts counted by
/// [`OPT_H_ATTEMPTS`] additionally satisfy ALL SIX of OPT-H's preconditions
/// (`docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §2.1): growing cross-class
/// (already implied, since this is only ever incremented alongside
/// `OPT_H_ATTEMPTS`), Small/Primordial segment kind (already implied by the
/// call site), tail-adjacency (`off + block_size(old_class) ==
/// meta.bump_of()`), new-class alignment (`off % block_size(new_class) ==
/// 0`), segment capacity (`off + block_size(new_class) <= SEGMENT`), and the
/// lazy-commit frontier (trivially satisfied when
/// `primordial-lazy-commit`/`small-segment-lazy-commit` are both off; see the
/// call site's own comment for why a tail-adjacent block's frontier is always
/// already sufficient when those features are on). This is the Stage-1
/// hit-rate NUMERATOR — the fraction `OPT_H_HITS / OPT_H_ATTEMPTS` is the hit
/// rate the design's CONDITIONAL-GO trigger (§9) is measured against.
///
/// **This counter has ZERO effect on allocator behavior**, exactly like
/// [`OPT_H_ATTEMPTS`] — incrementing it does NOT cause an in-place grow; the
/// call site still falls through to `None` unconditionally. By construction,
/// `OPT_H_HITS` is only ever incremented in the same call where
/// `OPT_H_ATTEMPTS` was also incremented (the six-precondition check is
/// nested inside the precondition-1 check that bumps `OPT_H_ATTEMPTS`), so
/// `OPT_H_HITS <= OPT_H_ATTEMPTS` always holds.
///
/// Read via [`AllocCore::dbg_opt_h_hits`]. Reads 0 unless `alloc-stats` is on
/// (same gating convention as [`OPT_H_ATTEMPTS`]). Relaxed ordering.
pub(crate) static OPT_H_HITS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

// ===========================================================================
// R34-23 (task #542) — realloc in-place vs move-leg path-activation counters
// ===========================================================================
//
// Three process-wide counters that let a realloc gate report (R34-23,
// `docs/perf/R34_23_REALLOC_AND_VEC_GATE.md`) PROVE which code path each
// realloc took — the single shared detection function
// [`AllocCore::realloc_inplace_fast_path_known_base`] is the ONLY place these
// are bumped, so `inplace_large + inplace_small + decline` == the number of
// reallocs that reached the fast-path detection (i.e. every non-null realloc
// on a registered segment). This is the path-activation oracle CLAUDE.md's
// R30-8 rule requires: a gate judging the in-place Large grow MUST report per-
// arm evidence that the in-place path actually fired, not just that the config
// resolved correctly.
//
// Same convention as [`OPT_H_ATTEMPTS`]: the statics are ALWAYS compiled (so
// the `dbg_reloc_*` accessors have a stable definition), the per-event
// INCREMENT is gated behind `alloc-stats` (zero cost in plain `production`),
// and reads are Relaxed (diagnostic only). Reads 0 unless `alloc-stats` is on.

/// R34-23 path-activation oracle: process-wide count of Large→Large in-place
/// realloc grows that succeeded via OPT-G (committed-span path OR the
/// `large-reserved-capacity` reserved-VA path). Bumped at BOTH `return
/// Some(ptr)` sites in [`realloc_inplace_fast_path_known_base`]'s Large branch.
/// Read via [`AllocCore::dbg_reloc_inplace_large_count`].
pub(crate) static RELOC_INPLACE_LARGE_CALLS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// R34-23 path-activation oracle: process-wide count of Small/Primordial
/// same-class in-place reallocs that succeeded via OPT-F (the block stayed in
/// its own size class, no copy). Bumped at the `return Some(ptr)` site in the
/// Small branch. Read via [`AllocCore::dbg_reloc_inplace_small_count`].
pub(crate) static RELOC_INPLACE_SMALL_CALLS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// R34-23 path-activation oracle: process-wide count of reallocs where the
/// in-place fast paths DECLINED (returned `None`), forcing the caller's
/// move-leg (alloc-new + copy + dealloc-old). Bumped at every `None` return in
/// [`realloc_inplace_fast_path_known_base`]: Large-grow-doesn't-fit,
/// Small-cross-class, and other-kind. Read via
/// [`AllocCore::dbg_reloc_fastpath_decline_count`]. By construction
/// `inplace_large + inplace_small + decline == total_fast_path_calls`.
pub(crate) static RELOC_FASTPATH_DECLINE_CALLS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

// ===========================================================================
// R29-5 (task #436) — promotion-frequency / copied-byte distribution counters
// ===========================================================================
//
// Stage-1 diagnostic counters for the medium→Large realloc-promotion path
// (`try_promote_to_large`, `src/registry/heap_core_free.rs`). These answer the
// R22-16 remap-instead-of-copy design's "victim" question (does the promotion
// memcpy actually move enough bytes, often enough, for an OS-level `mremap` to
// have a real victim?) — `docs/perf/R29_5_PROMOTION_FREQUENCY_GATE.md`.
//
// Mirrors the OPT_H_ATTEMPTS/OPT_H_HITS convention exactly: the statics are
// ALWAYS compiled (so the `dbg_promotion_*` accessors have a stable definition
// regardless of feature set), but the per-event INCREMENT is gated behind
// `bench-internals` (NOT `production` — per CLAUDE.md's benchmark-hook rule #2:
// a hook with no production caller defaults to `bench-internals`, not a
// production-composition feature). Note the promotion path itself is only
// compiled under `medium-classes` (also not in `production`), so under plain
// `--features production` neither the path nor any counter increment exists —
// zero behavior change, zero surface. Reads 0 unless `bench-internals` is on.
//
// A promotion event increments exactly ONE bucket of the byte histogram, keyed
// by the number of bytes the promotion copy moved (`old_layout.size()` at the
// `try_promote_to_large` copy site). The buckets are fixed-width powers-of-two
// (see `promotion_byte_bucket`) so the SHAPE of the distribution is preserved,
// not just an aggregate — the design's upside scales with copied bytes, so the
// shape matters for the verdict (R29-5's anti-p-hacking guard), not just the
// total.

/// DIAGNOSTIC (R29-5, task #436): process-wide count of successful
/// medium→Large realloc promotions (`try_promote_to_large` returning `Some`)
/// since process start. Read via [`AllocCore::dbg_promotion_count`]. Relaxed
/// ordering — a diagnostic count, not a synchronization primitive. Reads 0
/// unless `bench-internals` is on.
pub(crate) static PROMOTION_COUNT: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// DIAGNOSTIC (R29-5): cumulative bytes copied by all promotions counted by
/// [`PROMOTION_COUNT`] (sum of `old_layout.size()` per event). `sum / count`
/// is the mean copied bytes per promotion. Read via
/// [`AllocCore::dbg_promotion_bytes_sum`]. Relaxed.
pub(crate) static PROMOTION_BYTES_SUM: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// DIAGNOSTIC (R29-5): smallest `old_layout.size()` ever copied by a single
/// promotion. Updated via `fetch_min` per event. Read via
/// [`AllocCore::dbg_promotion_bytes_min`]. Relaxed. Initial `u64::MAX` reads
/// as "no promotion has occurred yet" — the accessor maps that to 0.
pub(crate) static PROMOTION_BYTES_MIN: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(u64::MAX);

/// DIAGNOSTIC (R29-5): largest `old_layout.size()` ever copied by a single
/// promotion. Updated via `fetch_max` per event. Read via
/// [`AllocCore::dbg_promotion_bytes_max`]. Relaxed.
pub(crate) static PROMOTION_BYTES_MAX: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// DIAGNOSTIC (R29-5): per-bucket histogram of bytes copied per promotion
/// (one increment per event in exactly one bucket, per `promotion_byte_bucket`).
/// Read via [`AllocCore::dbg_promotion_bytes_hist`]. The buckets are:
/// `[0,4KiB) [4KiB,16KiB) [16KiB,64KiB) [64KiB,128KiB) [128KiB,256KiB)
/// [256KiB,512KiB) [512KiB,1MiB) [1MiB,∞)`. Relaxed.
pub(crate) static PROMOTION_BYTES_HIST: [core::sync::atomic::AtomicU64; 8] = [
    core::sync::atomic::AtomicU64::new(0),
    core::sync::atomic::AtomicU64::new(0),
    core::sync::atomic::AtomicU64::new(0),
    core::sync::atomic::AtomicU64::new(0),
    core::sync::atomic::AtomicU64::new(0),
    core::sync::atomic::AtomicU64::new(0),
    core::sync::atomic::AtomicU64::new(0),
    core::sync::atomic::AtomicU64::new(0),
];

/// DIAGNOSTIC (R29-5): the histogram-bucket index for a given copied-byte
/// count, matching the [`PROMOTION_BYTES_HIST`] bucket layout documented
/// above. `pub(crate)` so the single increment site (`try_promote_to_large`)
/// and the accessor agree on bucket boundaries by construction. Gated on
/// `bench-internals` (unlike the statics above, which stay always-compiled
/// so the always-available `dbg_promotion_*` accessors have a stable
/// definition) **AND** on `try_promote_to_large`'s own reachability
/// predicate (`registry::heap_core_free`'s `medium_promotion_reachable!`
/// macro, reproduced here verbatim since a `#[cfg]` cannot take a macro
/// invocation as its argument): this function's ONLY caller is the
/// `bench-internals`-gated increment site inside that function, so under any
/// feature combination where `try_promote_to_large` itself does not compile
/// (plain `production`, which lacks `bench-internals`; but ALSO
/// `--all-features`, where `exact-span-large` + `large-reserved-capacity` +
/// `numa-aware` are simultaneously on and the macro's `any(...)` term
/// evaluates false) this has zero callers and would otherwise be `dead_code`
/// (caught by `cargo clippy --all-features -- -D warnings`, a real CI
/// matrix row, after the narrower `bench-internals`-only gate here missed
/// it — R29-13/task #444 zero-trust review).
#[cfg(all(
    feature = "bench-internals",
    feature = "medium-classes",
    any(
        not(feature = "exact-span-large"),
        all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
    )
))]
#[must_use]
pub(crate) const fn promotion_byte_bucket(bytes: usize) -> usize {
    let b = bytes as u64;
    // [0,4KiB)=0 [4KiB,16KiB)=1 [16KiB,64KiB)=2 [64KiB,128KiB)=3
    // [128KiB,256KiB)=4 [256KiB,512KiB)=5 [512KiB,1MiB)=6 [1MiB,∞)=7
    const KIB: u64 = 1024;
    if b < 4 * KIB {
        0
    } else if b < 16 * KIB {
        1
    } else if b < 64 * KIB {
        2
    } else if b < 128 * KIB {
        3
    } else if b < 256 * KIB {
        4
    } else if b < 512 * KIB {
        5
    } else if b < 1024 * KIB {
        6
    } else {
        7
    }
}

/// DIAGNOSTIC (review finding 2.3): process-wide count of `dealloc` calls that
/// hit the foreign-or-unroutable no-op branch — a `ptr` whose segment base is
/// NOT one of this heap's registered segments, so `dealloc` silently drops it
/// (see [`AllocCore::dealloc`]).
///
/// **Why this counter exists — the `alloc-global`-without-`alloc-xthread`
/// footgun.** In a build WITHOUT `alloc-xthread` there is no cross-thread
/// routing path: a block allocated on thread A and freed on thread B resolves
/// to a base that is not in B's heap's segment table, falls into this no-op,
/// and is **leaked permanently** (see `SeferAlloc`'s "Multi-thread safety"
/// docs). That configuration is a legitimate single-threaded trade-off — so
/// there is no `compile_error!` — but a multi-threaded program built that way
/// by mistake would leak monotonically with NO observable metric. This counter
/// is that metric: a non-zero, growing value under `alloc-global` alone is the
/// signature of a misconfiguration (or a genuine foreign-pointer free).
///
/// Surfaced as [`AllocStats::foreign_or_unroutable_frees`](crate::AllocStats::foreign_or_unroutable_frees)
/// via [`AllocCore::dbg_foreign_or_unroutable_frees`]. Diagnostic only
/// (Relaxed, like `DECOMMIT_CALLS` / `DBG_RING_OVERFLOW`).
///
/// The per-event increment is gated behind `alloc-stats` (default OFF, not in
/// `production`), matching the other per-event stat counters (`tcache_hits`,
/// `large_cache_hits`): the free hot path carries no bookkeeping unless
/// `alloc-stats` is compiled in. The static itself is always present (gated on
/// `alloc-core` — the feature that first defines `AllocCore::dealloc` and its
/// foreign-pointer no-op) so the accessor has a stable definition regardless of
/// the rest of the feature set. `alloc-stats` depends on `alloc-core`, so
/// whenever the increment is compiled in the static is guaranteed to exist.
#[cfg(feature = "alloc-core")]
pub(crate) static FOREIGN_OR_UNROUTABLE_FREES: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

// TEST/DIAGNOSTIC-ONLY (task D1 → 0.4.x task #133): large-cache HIT
// counter. Originally a single process-wide `static AtomicU64`, bumped by
// EVERY heap's `alloc_large` cache-hit path — a contended `lock xadd` on a
// path that is architecturally per-heap (each `AllocCore` lives inside one
// `HeapCore`, which lives on one thread's registry slot). Under MT this
// counter's cache line ping-ponged across cores on every large-cache hit —
// directly on the hot path of the crate's flagship workload (large-object
// churn, e.g. shamir-db), perf regression #133.
//
// Fix: the counter is now a PER-HEAP field (`AllocCore::large_cache_hits`,
// see below), incremented only by its owning `AllocCore`'s (and therefore
// that heap's owning thread's) own calls — never shared with another
// heap's cache line. It stays an `AtomicU64` (Relaxed) rather than a plain
// `u64` because the process-global VIEW
// (`registry::heap_registry::large_cache_hits_total`, aggregated into
// `SeferAlloc::stats()`) reads every live heap's counter from whatever
// thread calls `stats()` — a plain `u64` written by the owner and read by
// a different thread without synchronisation would be a data race (UB);
// `Relaxed` on both sides is sound for a diagnostic counter with no
// ordering requirement (the same pattern as `DBG_LARGE_XTHREAD_RECLAIMED`
// and the new `HeapCore::tcache_hits`), and needs no `unsafe` — safe-Rust
// atomics all the way, consistent with `#![forbid(unsafe_code)]`.
//
// TASK W3 (0.3.0) — the counter STORAGE moved out of `AllocCore` and into the
// owning `HeapSlot` (`HeapSlot::large_cache_hits`), closing a formal aliasing
// gap: the process-wide aggregator (`large_cache_hits_total`) used to
// materialise a shared `&HeapCore`/`&AllocCore` (`(*heap_ptr).core
// .dbg_large_cache_hits()`) over a struct the OWNING thread concurrently holds
// a protected `&mut` into — a foreign-read of a protected `Unique`, UB under
// Stacked Borrows. The counter now lives in the `Sync` slot; the owner reaches
// it through a SAFE `Option<&'static AtomicU64>` handle (a raw pointer would
// be a hard error — this module is `#![forbid(unsafe_code)]`), planted by
// `HeapRegistry::claim` at bind time. See `HeapSlot::large_cache_hits`.
#[cfg(feature = "alloc-decommit")]
pub(in crate::alloc_core) type LargeCacheHitCounter = core::sync::atomic::AtomicU64;
