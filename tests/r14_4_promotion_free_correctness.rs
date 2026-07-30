//! R14-4 (task #289) test (b) — correct free after growth past the promotion
//! threshold: allocate a Small/medium block, grow it past
//! `MEDIUM_REALLOC_PROMOTION_THRESHOLD`, verify the copy survived, free it,
//! and confirm (1) no accounting over-release occurred and (2) no
//! crash/corruption on a subsequent, unrelated allocation. A genuine per-base
//! LEAK proof (open item 4/5, `docs/CORRECTNESS_OPEN_ITEMS.md`) is a SEPARATE
//! test below, `canary_survives_promotion_and_free_leaves_no_leak_per_base`,
//! compiled only where the diagnostic surface it needs actually exists — see
//! that test's own doc comment and the R30-11 module note further down for
//! why the two are not the same assertion (R30-11, task #460).
//!
//! When `try_promote_to_large` is compiled in (see `HAS_PROMOTION` below),
//! this exercises the exact "how does dealloc know to free a promoted block
//! as Large" question the design doc's §4.2 argues is already answered by
//! `SegmentHeader::kind_at`-based routing (no new bookkeeping) — this test
//! confirms it structurally, not just by argument. When `HAS_PROMOTION` is
//! `false` (R15-3, task #305's zero-headroom exclusion), the grow instead
//! stays on the ordinary medium ladder — the SAME correctness properties
//! (canary survival, no over-release, no corruption) still apply and are
//! still a meaningful, non-vacuous check in that configuration; only the
//! identity of the code path exercised (promotion vs. plain medium-ladder
//! move-leg) differs, which is why this file, unlike
//! `tests/r14_4_promotion_move_leg_reduction.rs`, does not need a
//! `HAS_PROMOTION`-gated early return — neither assertion here depends on
//! WHICH path was taken, only that the result is correct.
//!
//! ## R30-11 (task #460) — why "no leak" moved out of the main test's name
//!
//! R29-1 (task #432) correctly replaced this file's original WINDOWED
//! `released_delta <= reserved_delta` guard (which had a real ~0.3%
//! window-crossing false-positive rate — see that commit and
//! `docs/CORRECTNESS_OPEN_ITEMS.md`'s R29-1 correction entry) with the
//! LIFETIME-CUMULATIVE `segments_released_total <= segments_reserved_total`
//! invariant. That fix is correct and this task does not touch its logic.
//! But what the cumulative inequality actually PROVES is narrower than the
//! old test name (`..._leaves_no_leak`, still used below for the per-base
//! test only) implies: it proves no impossible double/over-release occurred
//! (an over-release would make the inequality false). It does NOT prove
//! every freed block was actually released — a MISSING release just makes
//! the inequality more comfortably true, so it has zero leak-detection power
//! on its own (see the review-flagged "near-unfalsifiable" characterization,
//! `docs/CORRECTNESS_OPEN_ITEMS.md` item 5's `[P3]` sub-entry). The REAL leak
//! proof is the per-base before/after `live_count`/`dbg_contains_base` check,
//! which needs `alloc-decommit + alloc-xthread` and therefore does not exist
//! under every combination that compiles this file — notably the CI-tested
//! `hardened medium-classes` row (`hardened = ["fastbin"]` = `alloc-global +
//! alloc-xthread`, WITHOUT `alloc-decommit`).
//!
//! So the file now has two `#[test]`s instead of one, each named for exactly
//! what it proves in every configuration it compiles under:
//!
//!   - `canary_survives_promotion_and_free_no_double_release` (below, always
//!     compiled under this file's top-level gate) — canary survival + the
//!     cumulative no-over-release invariant + no corruption. Never claims
//!     "no leak" in its name or its assertion messages.
//!   - `canary_survives_promotion_and_free_leaves_no_leak_per_base` (below,
//!     gated `alloc-decommit + alloc-xthread`) — the genuine per-base leak
//!     proof. Its name is accurate exactly where it compiles.
//!
//! Under `hardened medium-classes` (no `alloc-decommit`), only the first test
//! exists in the binary — confirmed no stronger diagnostic than the
//! cumulative invariant is reachable there: `dbg_contains_base` alone
//! (`alloc-global + alloc-xthread`, available under `hardened`) cannot
//! distinguish "still legitimately hosts another live block" from "leaked",
//! because without `alloc-decommit` there is no `live_count` accessor
//! (`dbg_live_count_for` is `alloc-decommit`-gated) AND the small-segment
//! release/pool machinery itself (`dec_live_and_maybe_decommit` /
//! `dec_live_batch_and_maybe_decommit`,
//! `src/alloc_core/alloc_core_small_pool.rs`) is entirely
//! `#[cfg(feature = "alloc-decommit")]` — without it, small/medium segments
//! are never released or live-count-tracked in the first place, so
//! `dbg_contains_base` would just read `true` forever regardless of whether
//! `grown`'s block leaked. This is an honest, documented coverage gap for
//! that one feature combination, not an oversight: `hardened` trades away
//! `alloc-decommit`'s release/retention bookkeeping entirely, so there is no
//! segment-release event for a per-base check to observe there.
//!
//! Whole file is a no-op without `medium-classes` (see `#![cfg(...)]` below)
//! — run with:
//!   cargo test --release --features "production medium-classes" --test r14_4_promotion_free_correctness
//!   cargo test --release --features "production medium-classes exact-span-large" --test r14_4_promotion_free_correctness

#![cfg(all(feature = "alloc-global", feature = "medium-classes"))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::Mutex;

#[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
use sefer_alloc::global::tls_heap;
use sefer_alloc::{AllocStats, SeferAlloc};

const ALIGN: usize = 8;
const PROMOTION_THRESHOLD: usize = 256 * 1024;

// Both tests in this file read `a.stats()` — which reads the PROCESS-WIDE
// `segments_reserved_total`/`segments_released_total` atomics (see
// `src/alloc_core/os.rs`) — and compute a delta across their own
// snapshot-before/snapshot-after window, asserting that delta stays
// leak-free. `cargo test` runs test functions concurrently across multiple
// OS threads within the SAME process by default; any OTHER test in this
// binary (or the sibling test function in THIS file) reserving/releasing a
// segment between one test's snapshots pollutes its delta with unrelated
// activity, producing a spurious "released_delta > reserved_delta" failure.
// This mirrors the established pattern in
// `tests/directory_authoritative_miss.rs`'s `TEST_LOCK`: a file-scoped
// `Mutex<()>` held for a whole test body serialises every test in this file
// against each other so each one observes a quiescent counter window.
// (This does not protect against unrelated activity from OTHER test
// *binaries* running as separate processes — cargo test isolates process
// memory per binary, so that is a non-issue — only against races between
// test *functions* inside this one binary, which is exactly what the
// documented flake was.)
static TEST_LOCK: Mutex<()> = Mutex::new(());
fn serial() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

/// Mirrors `tests/r14_4_promotion_move_leg_reduction.rs`'s constant of the
/// same name byte-for-byte. Not used to gate any assertion in this file (see
/// the module doc) — kept only as a documented cross-reference for readers
/// checking which path ("promoted to Large" vs. "ordinary medium-ladder
/// move-leg") a given build actually takes.
#[allow(dead_code)]
const HAS_PROMOTION: bool = !cfg!(feature = "exact-span-large")
    || (cfg!(feature = "large-reserved-capacity") && !cfg!(feature = "numa-aware"));

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, ALIGN).unwrap()
}

/// Shared setup for both tests below: allocate a Small/medium block, stamp a
/// distinctive canary, snapshot process-wide stats, grow it past
/// `PROMOTION_THRESHOLD` (promoting to Large under `HAS_PROMOTION`, or moving
/// up the medium ladder otherwise), and verify the canary survived the growth
/// copy across the full old span. Stops BEFORE freeing `grown` — each caller
/// owns its own free/assert sequencing (the per-base leak proof needs a
/// `live_count` baseline taken between this point and the free; the
/// no-double-release test does not), so factoring the free in here would
/// force an ordering neither caller actually wants.
///
/// Returns `(grown, grown_layout, stats_before, stats_after_promote)`.
fn alloc_grow_and_verify_canary(a: &SeferAlloc) -> (*mut u8, Layout, AllocStats, AllocStats) {
    let old_size = 96 * 1024;
    let old_layout = layout(old_size);
    // SAFETY: valid, non-zero-size layout.
    let p = unsafe { a.alloc(old_layout) };
    assert!(
        !p.is_null(),
        "initial alloc of {old_size} bytes failed (old_layout={old_layout:?})"
    );

    // Write a distinctive, position-dependent canary (not a flat byte) so a
    // partial/misaligned copy is detectable, not just a gross zeroing bug.
    // SAFETY: p valid for old_size bytes.
    unsafe {
        for i in 0..old_size {
            p.add(i).write((i % 251) as u8);
        }
    }

    let stats_before = a.stats();

    let new_size = PROMOTION_THRESHOLD + 8192; // crosses the threshold -> promotes to Large (HAS_PROMOTION) or moves up the medium ladder otherwise
                                               // SAFETY: p live, old_layout matches, freed at most once on success.
    let grown = unsafe { a.realloc(p, old_layout, new_size) };
    assert!(
        !grown.is_null(),
        "growing realloc failed: {old_size} -> {new_size} bytes (old_layout={old_layout:?})"
    );

    // Canary must have survived the growth copy across the FULL old span.
    // SAFETY: grown valid for new_size >= old_size bytes.
    unsafe {
        for i in 0..old_size {
            assert_eq!(
                grown.add(i).read(),
                (i % 251) as u8,
                "canary byte {i} corrupted or lost during the growth copy"
            );
        }
    }

    let stats_after_promote = a.stats();
    // Growing this block reserves at most one fresh segment (Large under
    // `HAS_PROMOTION`, or a medium-class segment otherwise — or reuses a
    // cached one, reserving zero) — either way `segments_reserved_total`
    // does not go backwards and the delta is small/bounded, never a wild
    // runaway (a sanity bound, not an exact-count assertion, since the
    // large_cache's admission policy is not this test's concern).
    let monotonic =
        stats_after_promote.segments_reserved_total >= stats_before.segments_reserved_total;
    if !monotonic {
        eprintln!(
            "[r14_4 diag] reserved_total went backwards: before={} after_promote={}",
            stats_before.segments_reserved_total, stats_after_promote.segments_reserved_total
        );
    }
    assert!(monotonic, "segments_reserved_total must be monotonic");

    let grown_layout = layout(new_size);
    (grown, grown_layout, stats_before, stats_after_promote)
}

/// Canary pattern survives the growth copy (promotion to Large under
/// `HAS_PROMOTION`, or an ordinary medium-ladder move-leg otherwise — see
/// `HAS_PROMOTION`'s doc), the grown block frees with no accounting
/// over-release (the process-wide, lifetime-cumulative
/// `segments_released_total <= segments_reserved_total` invariant — see the
/// module doc's R30-11 note for exactly what this does and does not prove),
/// and a later, unrelated allocation is uncorrupted.
///
/// R30-11 (task #460): this test does NOT prove "no leak" — a MISSING
/// release makes the cumulative inequality MORE comfortably true, so it has
/// zero leak-detection power on its own (see the module doc). It proves only
/// that no impossible double/over-release occurred. The genuine leak proof
/// is the separate `canary_survives_promotion_and_free_leaves_no_leak_per_base`
/// test below, compiled only where its diagnostic surface exists.
#[test]
fn canary_survives_promotion_and_free_no_double_release() {
    let _guard = serial();
    let a = SeferAlloc::new();
    let (grown, grown_layout, stats_before, stats_after_promote) = alloc_grow_and_verify_canary(&a);

    // SAFETY: grown live, grown_layout matches, freed exactly once.
    unsafe { a.dealloc(grown, grown_layout) };

    let stats_after_free = a.stats();
    // R29-1 (task #432): the `segments_reserved_total` / `segments_released_total`
    // counters are PROCESS-WIDE and CUMULATIVE. The ORIGINAL guard here compared
    // WINDOWED DELTAS (`released_delta <= reserved_delta` since `stats_before`);
    // R29-1 reproduced that form's failure at ~0.3% under the
    // `production medium-classes` combo and PROVED it unsound: the promotion grow
    // empties `p`'s old segment, and if the allocator releases that segment to
    // the OS during the grow, that release lands INSIDE this test's snapshot
    // window while its matching reserve landed BEFORE the window (heap/TLS init,
    // the primordial segment, or the sibling test in this binary that shares this
    // thread's heap via the persistent TLS binding). So `released_delta`
    // legitimately exceeded `reserved_delta` while `grown`'s OWN segment was
    // provably correctly freed — a window-crossing FALSE POSITIVE, not a leak or
    // double-release.
    //
    // The windowed deltas are retained BELOW as diagnostic context only. The
    // SOUND invariant is GLOBAL and CUMULATIVE, not windowed: process-wide
    // `segments_released_total` can NEVER exceed `segments_reserved_total` (every
    // release corresponds to a prior reserve; only a genuine double-release of
    // the same OS reservation could push released past reserved). This is
    // window-independent and exactly captures the guard's stated intent.
    //
    // R30-11 (task #460): renamed from `no_double_release`'s former framing as
    // part of "no leak" to make explicit what it actually is — an over-release /
    // accounting-invariant guard. Leak detection is NOT this counter's job (a
    // missing release only makes this MORE true, never false) — that is the
    // per-base proof's job, in the separate test below, where it compiles.
    let reserved_delta =
        stats_after_free.segments_reserved_total - stats_before.segments_reserved_total;
    let released_delta =
        stats_after_free.segments_released_total - stats_before.segments_released_total;
    let no_over_release =
        stats_after_free.segments_released_total <= stats_after_free.segments_reserved_total;
    if !no_over_release {
        // R29-1: failure-path-only diagnostics (zero pass-path cost). A
        // GLOBAL released > reserved is a genuine double-release, so print the
        // full process-wide cumulative trajectory to localize it.
        eprintln!(
            "[r14_4 diag] GLOBAL over-release invariant FAILED: \
             segments_released_total={} > segments_reserved_total={}. \
             counter trajectory: reserved before={} after_promote={} after_free={} | \
             released before={} after_promote={} after_free={} | \
             windowed deltas (context): reserved_delta={} released_delta={}",
            stats_after_free.segments_released_total,
            stats_after_free.segments_reserved_total,
            stats_before.segments_reserved_total,
            stats_after_promote.segments_reserved_total,
            stats_after_free.segments_reserved_total,
            stats_before.segments_released_total,
            stats_after_promote.segments_released_total,
            stats_after_free.segments_released_total,
            reserved_delta,
            released_delta,
        );
    }
    assert!(
        no_over_release,
        "segments_released_total ({}) must not exceed segments_reserved_total ({}) — \
         a process-wide released > reserved is a double-release / corruption \
         signal (windowed deltas since stats_before, shown for context only, \
         were reserved_delta={reserved_delta}, released_delta={released_delta}). \
         NOTE: this assertion proves no over-release, NOT no leak — see this \
         file's module doc (R30-11, task #460) for the separate per-base leak \
         proof and which feature combinations compile it.",
        stats_after_free.segments_released_total, stats_after_free.segments_reserved_total
    );

    // No corruption: a subsequent, unrelated allocation must still work and
    // be independently writable/readable (would likely crash or read back
    // wrong bytes if the growth/free path corrupted segment/table state).
    let q_layout = layout(4096);
    // SAFETY: valid, non-zero-size layout.
    let q = unsafe { a.alloc(q_layout) };
    assert!(
        !q.is_null(),
        "unrelated post-free allocation of 4096 bytes failed (q_layout={q_layout:?})"
    );
    // SAFETY: q valid for 4096 bytes.
    unsafe {
        for i in 0..4096usize {
            q.add(i).write((i % 199) as u8);
        }
        for i in 0..4096usize {
            assert_eq!(q.add(i).read(), (i % 199) as u8);
        }
        a.dealloc(q, q_layout);
    }
}

/// The genuine per-base LEAK proof (open item 4/5, `docs/CORRECTNESS_OPEN_ITEMS.md`;
/// split out by R30-11, task #460, from the combined test this file used to
/// have). Only compiled where its diagnostic surface exists —
/// `dbg_live_count_for` needs `alloc-decommit`, `dbg_contains_base` needs
/// `alloc-global + alloc-xthread` — so its very presence in a given build is
/// itself the "leak coverage exists here" signal; see the module doc for the
/// combinations that do (`production medium-classes[, exact-span-large]`,
/// full `production`) and do not (`hardened medium-classes`, the CI-tested
/// row lacking `alloc-decommit`) compile it.
#[test]
#[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
fn canary_survives_promotion_and_free_leaves_no_leak_per_base() {
    let _guard = serial();
    let a = SeferAlloc::new();
    let (grown, grown_layout, _stats_before, _stats_after_promote) =
        alloc_grow_and_verify_canary(&a);

    // Snapshot this thread's TLS-bound `*mut HeapCore` via the established
    // save/poison/restore hook (`tests/dealloc_only_no_bind_torn.rs` uses the
    // identical pattern) — `SeferAlloc` itself exposes no direct `HeapCore`
    // accessor, but binding is per-THREAD (TLS), not per-`SeferAlloc`-
    // instance (see `SeferAlloc::with_config`'s "Binding semantics" doc), so
    // the pointer this yields for the CURRENT thread is exactly the same
    // `HeapCore` `a.alloc`/`a.dealloc` above already routed through.
    //
    // Per-base observable (open item 4, `docs/CORRECTNESS_OPEN_ITEMS.md`):
    // resolve `grown`'s segment base and take a "genuinely allocated blocks
    // only" live_count BASELINE before freeing it, so the post-free
    // membership check below is anchored to the SPECIFIC segment this
    // test's own grow produced — not a process-wide counter that cannot
    // distinguish "still held by something else" from "genuinely never
    // released".
    //
    // `dbg_trim_current_thread` (the production teardown-trim primitive,
    // normally run on thread exit) is called HERE, BEFORE the baseline
    // snapshot, for the same reason it is called again below after the free:
    // `live_count` only reflects blocks that have been reconciled with the
    // substrate — a block sitting in this thread's per-class magazine
    // (tcache) is NOT yet subtracted from `live_count` (magazine push does
    // NOT call `dec_live`; see `HeapCore::dbg_is_free_for`'s doc comment).
    // Under `medium-classes` (`!HAS_PROMOTION`), every Small/Primordial-kind
    // segment is carved from a single PER-THREAD `small_cur` bump cursor
    // shared across every small/medium size class
    // (`AllocCore::carve_block`, `src/alloc_core/alloc_core_small.rs`), so
    // `grown`'s segment routinely hosts OTHER blocks from this test's own
    // earlier `p` carve or the 31-block cold-carve refill batch — some of
    // which may still be sitting in THEIR OWN class's magazine at this
    // point. Trimming BEFORE the baseline flushes all of that pre-existing
    // magazine residency first, so the baseline counts only blocks that are
    // genuinely, substrate-level allocated right now — the SAME converged
    // regime the post-free reading below is taken in — making the two
    // snapshots a true apples-to-apples comparison (their only possible
    // difference is `grown`'s own departure, not an unrelated co-tenant
    // block happening to also drain out of its magazine in between).
    let (heap, grown_base, live_count_before_free) = {
        // R29-7 (task #438): `dbg_mark_local_torn_for_test`/
        // `dbg_restore_local_for_test` are now `bench-internals`-gated (they
        // install a caller-supplied raw pointer as this thread's live `LOCAL`
        // binding — a real soundness hole outside test code, see their doc
        // comments in `tls_heap.rs`). This test never needed TORN semantics
        // at all — it only ever wanted "this thread's own already-bound
        // `HeapCore` pointer", which the production-path, side-effect-free
        // `current_for_alloc()` already provides directly (this thread is
        // already bound by the alloc/realloc calls above, so it always takes
        // the `Own` fast-path arm here, never the cold bind/fallback arms).
        let heap = match tls_heap::current_for_alloc() {
            tls_heap::CurrentHeap::Own(p) => p,
            tls_heap::CurrentHeap::Fallback => std::ptr::null_mut(),
        };
        assert!(
            !heap.is_null(),
            "this thread has no bound HeapCore — current_for_alloc() resolved \
             to Fallback, which should be impossible after the alloc/realloc \
             calls above already bound one"
        );
        a.dbg_trim_current_thread();
        // SAFETY: `heap` is this thread's own live, bound `HeapCore` (just
        // resolved above); `grown` is a live pointer returned by the `realloc`
        // above.
        let grown_base = unsafe { (*heap).dbg_segment_base_of_ptr(grown) };
        // SAFETY: `heap` is this thread's own live, bound `HeapCore`.
        let live_count_before_free = unsafe { (*heap).dbg_live_count_for(grown_base) };
        (heap, grown_base, live_count_before_free)
    };

    // SAFETY: grown live, grown_layout matches, freed exactly once.
    unsafe { a.dealloc(grown, grown_layout) };

    // The strengthened leak proof itself (open item 4): after freeing
    // `grown` AND trimming this thread's heap (flushing its magazine, pool,
    // and large cache), `grown`'s specific segment base must be in one of
    // exactly two SANCTIONED states — never a third, silent "still there for
    // no accounted reason" state:
    //
    //   (a) UNREGISTERED — `dbg_contains_base(grown_base) == false`. This is
    //       what happens once the trim below has run: a Large segment's
    //       `AllocCore::dealloc` Large branch always calls
    //       `self.table.unregister(base)` (cache-admitted, budget-declined,
    //       and no-`alloc-decommit` eager-release alike — see
    //       `src/alloc_core/alloc_core.rs`'s Large arm) before it returns,
    //       and `dbg_trim_current_thread`'s `evict_all` releases whatever the
    //       cache subsequently held; a Small/Primordial segment that became
    //       fully empty (this test's own block was its last occupant) is
    //       either released directly or, if pool-admitted, released by the
    //       trim's `drain_small_pool` call. A leak that skipped this
    //       bookkeeping (e.g. a grow that reserved a segment and never
    //       released it) would leave this `true` forever, which is exactly
    //       the gap this assertion closes: the GLOBAL cumulative invariant in
    //       the sibling `canary_survives_promotion_and_free_no_double_release`
    //       test is satisfied trivially by a missing release (it only ever
    //       gets MORE comfortably true) and would not catch it, but this
    //       per-base check would.
    //   (b) STILL REGISTERED BUT `grown`'s OWN BLOCK GENUINELY LEFT —
    //       `dbg_contains_base(grown_base) == true` AND
    //       `live_count_after_trim == Some(live_count_before_free - 1)`
    //       (exactly one fewer than before this free, never equal to or
    //       greater than before). Reachable only when the segment still
    //       hosts OTHER live (truly allocated, not just freed-to-magazine)
    //       blocks from the shared-`small_cur` co-tenancy described above —
    //       after the trim, every magazine-buffered/pooled block has been
    //       reconciled, so any REMAINING live_count reflects genuinely
    //       allocated blocks, and `grown`'s own departure must show up as
    //       exactly one fewer of them. A leak that left `grown`'s own
    //       block's slot still counted as live (e.g. a `dec_live` that
    //       silently no-oped) would violate this, and a leak that skipped
    //       the free's bookkeeping entirely would show
    //       `live_count_after_trim == live_count_before_free` (no change at
    //       all) rather than a decrement.
    //
    // `dbg_live_count_for` itself already returns `None` (not `Some(n)`)
    // whenever `contains_base_ro` is `false` or the segment's kind is not
    // Small/Primordial — so for the Large-promoted case, both
    // `live_count_before_free` and `live_count_after_trim` are `None` and
    // the check collapses to (a) alone, exactly as expected.
    //
    // `a.dealloc` above may leave `grown`'s block sitting in this thread's
    // own per-class magazine rather than immediately returning it to the
    // substrate — same magazine-push behavior as above. Trim again so
    // `grown`'s own departure (and only that) is reconciled into
    // `live_count`/`dbg_contains_base` before reading them: this flushes
    // every tcache class back to the substrate (`dec_live` runs for each),
    // drains the empty-small-segment hysteresis pool (releases every pooled
    // segment to the OS), and evicts the entire large_cache (releases every
    // cached Large span) — so after this call there is no remaining
    // ambiguous "buffered/pooled/cached" state left for `grown_base` to hide
    // in; only the two sanctioned end-states above are possible.
    a.dbg_trim_current_thread();
    // SAFETY: `heap` is this thread's own live, bound `HeapCore`.
    let still_registered = unsafe { (*heap).dbg_contains_base(grown_base) };
    // SAFETY: `heap` is this thread's own live, bound `HeapCore`.
    let live_count_after_trim = unsafe { (*heap).dbg_live_count_for(grown_base) };
    let exactly_one_fewer = matches!(
        (live_count_before_free, live_count_after_trim),
        (Some(before), Some(after)) if after + 1 == before
    );
    let leak_ok = !still_registered || exactly_one_fewer;
    if !leak_ok {
        eprintln!(
            "[r14_4 diag] per-base LEAK proof FAILED: grown_base={grown_base:?} \
             still_registered={still_registered} \
             live_count_before_free={live_count_before_free:?} \
             live_count_after_trim={live_count_after_trim:?}"
        );
    }
    assert!(
        leak_ok,
        "LEAK: grown_base ({grown_base:?}) is still registered in the \
         segment table after being freed and this thread's heap trimmed \
         (dbg_contains_base == true), but its live_count went from \
         {live_count_before_free:?} to {live_count_after_trim:?} — not a \
         decrease of exactly one — so `grown`'s own block was neither \
         unregistered (Large-style release/cache) nor validly removed from \
         its still-registered segment's live count (Small-style free); it \
         is unaccounted for, i.e. genuinely leaked"
    );
}

/// Multiple grow+free round-trips in a loop (each crossing
/// `PROMOTION_THRESHOLD` — promoting to Large under `HAS_PROMOTION`, or
/// moving up the medium ladder otherwise) must not accumulate a leak
/// (segments_reserved_total - segments_released_total must not grow
/// unboundedly across iterations, modulo cache retention which is itself
/// bounded).
#[test]
fn repeated_promote_and_free_does_not_leak_unboundedly() {
    let _guard = serial();
    let a = SeferAlloc::new();
    let stats_before = a.stats();

    for round in 0..20 {
        let old_size = 48 * 1024;
        let old_layout = layout(old_size);
        // SAFETY: valid, non-zero-size layout.
        let p = unsafe { a.alloc(old_layout) };
        assert!(!p.is_null(), "round {round}: initial alloc failed");

        let new_size = PROMOTION_THRESHOLD + 1024 * (round + 1);
        // SAFETY: p live, old_layout matches, freed at most once on success.
        let grown = unsafe { a.realloc(p, old_layout, new_size) };
        assert!(!grown.is_null(), "round {round}: growing realloc failed");

        let grown_layout = layout(new_size);
        // SAFETY: grown live, grown_layout matches, freed exactly once.
        unsafe { a.dealloc(grown, grown_layout) };
    }

    let stats_after = a.stats();
    let reserved_delta = stats_after.segments_reserved_total - stats_before.segments_reserved_total;
    // 20 rounds, each doing one small alloc (never freed individually — it
    // is superseded by the growing realloc) plus one grow-across-threshold: a
    // reasonable, generous upper bound on distinct segments reserved is 2x
    // rounds (worst case zero cache reuse AND every round's small alloc also
    // lands in a fresh segment) — this is a leak-detection ceiling (catching
    // UNBOUNDED growth), not a tight performance assertion pinning an exact
    // count.
    assert!(
        reserved_delta <= 40,
        "20 grow+free rounds reserved {reserved_delta} segments — \
         expected at most 40 (<=2 per round), suggesting a leak"
    );
}
