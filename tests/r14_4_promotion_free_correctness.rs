//! R14-4 (task #289) test (b) — correct free after growth past the promotion
//! threshold: allocate a Small/medium block, grow it past
//! `MEDIUM_REALLOC_PROMOTION_THRESHOLD`, verify the copy survived, free it,
//! and confirm there is no leak (via the process-wide, always-available
//! `segments_reserved_total`/`segments_released_total` counters) and no
//! crash/corruption on a subsequent, unrelated allocation.
//!
//! When `try_promote_to_large` is compiled in (see `HAS_PROMOTION` below),
//! this exercises the exact "how does dealloc know to free a promoted block
//! as Large" question the design doc's §4.2 argues is already answered by
//! `SegmentHeader::kind_at`-based routing (no new bookkeeping) — this test
//! confirms it structurally, not just by argument. When `HAS_PROMOTION` is
//! `false` (R15-3, task #305's zero-headroom exclusion), the grow instead
//! stays on the ordinary medium ladder — the SAME correctness properties
//! (canary survival, no leak, no corruption) still apply and are still a
//! meaningful, non-vacuous check in that configuration; only the identity of
//! the code path exercised (promotion vs. plain medium-ladder move-leg)
//! differs, which is why this file, unlike
//! `tests/r14_4_promotion_move_leg_reduction.rs`, does not need a
//! `HAS_PROMOTION`-gated early return — neither assertion here depends on
//! WHICH path was taken, only that the result is correct.
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
use sefer_alloc::SeferAlloc;

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

/// Canary pattern survives the growth copy (promotion to Large under
/// `HAS_PROMOTION`, or an ordinary medium-ladder move-leg otherwise — see
/// `HAS_PROMOTION`'s doc), and the grown block frees cleanly with no leak
/// (segment counters balance) and no corruption of a later, unrelated
/// allocation.
#[test]
fn canary_survives_promotion_and_free_leaves_no_leak() {
    let _guard = serial();
    let a = SeferAlloc::new();

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

    // The strengthened per-base leak proof below needs `dbg_live_count_for`
    // (`alloc-decommit`-gated) and `dbg_contains_base`
    // (`alloc-global + alloc-xthread`-gated) — a strictly narrower feature
    // set than this file's own top-level `#![cfg(all(feature = "alloc-global",
    // feature = "medium-classes"))]` gate, which deliberately stays loose so
    // this file keeps compiling (and exercising the ORIGINAL `released_delta
    // <= reserved_delta` double-release guard) under the CI-tested `hardened
    // medium-classes` combination (`hardened = ["fastbin"]` = `alloc-global +
    // alloc-xthread`, WITHOUT `alloc-decommit` — see
    // `.github/workflows/ci.yml`'s `test (--features "hardened
    // medium-classes")` step). Gating narrower, rather than widening the
    // file's own top-level gate, keeps that CI row's existing coverage of
    // this test unchanged (confirmed via `cargo test --no-run --features
    // "hardened medium-classes" --test r14_4_promotion_free_correctness`,
    // which fails to compile WITHOUT this `#[cfg]` — `dbg_live_count_for`
    // does not exist under that feature set). The actual `a.dealloc(grown,
    // ..)` call below stays UNCONDITIONAL either way — it is what the
    // pre-existing `released_delta <= reserved_delta` assertion needs
    // regardless of which combination is compiled.
    //
    // Snapshot this thread's TLS-bound `*mut HeapCore` via the established
    // save/poison/restore hook (`tests/dealloc_only_no_bind_torn.rs` uses the
    // identical pattern) — `SeferAlloc` itself exposes no direct `HeapCore`
    // accessor, but binding is per-THREAD (TLS), not per-`SeferAlloc`-
    // instance (see `SeferAlloc::with_config`'s "Binding semantics" doc), so
    // the pointer this yields for the CURRENT thread is exactly the same
    // `HeapCore` `a.alloc`/`a.dealloc` above already routed through. Poisons
    // `LOCAL` to `TORN` as a side effect; restored immediately below before
    // any further allocator use on this thread.
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
    #[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
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

    let grown_layout = layout(new_size);
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
    //       the gap this assertion closes: the OLD `released_delta <=
    //       reserved_delta` check is satisfied trivially by
    //       `reserved_delta=1, released_delta=0` and would not catch it, but
    //       this per-base check would.
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
    #[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
    {
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
    // provably correctly freed (`still_registered=false`, per-base proof above
    // passed) — a window-crossing FALSE POSITIVE, not a leak or double-release.
    //
    // The windowed deltas are retained BELOW as diagnostic context only. The
    // SOUND double-release invariant is GLOBAL and CUMULATIVE, not windowed:
    // process-wide `segments_released_total` can NEVER exceed
    // `segments_reserved_total` (every release corresponds to a prior reserve;
    // only a genuine double-release of the same OS reservation could push
    // released past reserved). This is window-independent and exactly captures
    // the guard's stated intent. Leak detection is NOT this counter's job — it
    // is the per-base proof's job above (reliable, segment-specific).
    let reserved_delta =
        stats_after_free.segments_reserved_total - stats_before.segments_reserved_total;
    let released_delta =
        stats_after_free.segments_released_total - stats_before.segments_released_total;
    let no_double_release =
        stats_after_free.segments_released_total <= stats_after_free.segments_reserved_total;
    if !no_double_release {
        // R29-1: failure-path-only diagnostics (zero pass-path cost). A
        // GLOBAL released > reserved is a genuine double-release, so print the
        // full process-wide cumulative trajectory plus `grown`'s OWN per-base
        // registration/live-count state to localize it.
        eprintln!(
            "[r14_4 diag] GLOBAL double-release invariant FAILED: \
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
        #[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
        {
            // SAFETY: `heap` is this thread's own live, bound `HeapCore`;
            // read-only diagnostic probes of `grown_base`'s segment (re-read
            // here because `still_registered`/`live_count_after_trim` above are
            // block-scoped to the per-base proof block and not in scope here).
            let reg_now = unsafe { (*heap).dbg_contains_base(grown_base) };
            let lc_now = unsafe { (*heap).dbg_live_count_for(grown_base) };
            eprintln!(
                "[r14_4 diag] grown's OWN segment state at failure: \
                 grown_base={grown_base:?} still_registered={} \
                 live_count_before_free={live_count_before_free:?} \
                 live_count_after_trim_recheck={:?} \
                 (still_registered=false => grown's own segment is NOT the \
                 double-released one; the released>reserved discrepancy \
                 originates elsewhere — investigate the trajectory above)",
                reg_now, lc_now
            );
        }
    }
    assert!(
        no_double_release,
        "segments_released_total ({}) must not exceed segments_reserved_total ({}) — \
         a process-wide released > reserved is a double-release / corruption \
         signal (windowed deltas since stats_before, shown for context only, \
         were reserved_delta={reserved_delta}, released_delta={released_delta})",
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
