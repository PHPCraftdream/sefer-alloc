//! Regression test for task #125 — own-thread `dealloc` of a Large/huge
//! segment used to defer the OS-reservation release (and the `SegmentTable`
//! slot release) to `AllocCore::drop`, which is the SAME leak class as A1
//! (`tests/regression_xthread_large_free_no_leak.rs`, task #114 follow-up),
//! just on the own-thread path instead of the cross-thread one.
//!
//! ## What this guards against
//!
//! In the Phase 12.5 shard model, a per-thread `AllocCore` lives inside a
//! `HeapRegistry` slot for (practically) the entire process lifetime: the
//! *slot* is recycled between OS threads, but the `AllocCore` value itself is
//! not dropped mid-process — `AllocCore::drop` is therefore, in effect,
//! unreachable outside of process shutdown or a directly-owned (non-registry)
//! `AllocCore` going out of scope.
//!
//! Pre-fix, `AllocCore::dealloc`'s `SegmentKind::Large` branch, on the paths
//! where the large-cache does NOT take ownership of the segment — i.e.
//! *always* under `not(alloc-decommit)`, and under `alloc-decommit` whenever
//! cache admission is declined (no free slot after FIFO eviction, or the
//! byte-budget is too small to admit the span) — only zeroed the segment
//! header's `magic` field and left both:
//!   1. the OS reservation mapped, and
//!   2. the `SegmentTable` slot registered (`base` still occupies a slot),
//!
//! relying on `AllocCore::drop` to walk the table and release the
//! reservation later. Since `drop` effectively never runs for a
//! registry-resident `AllocCore`, every own-thread large free was a
//! PERMANENT leak of one `SegmentTable` slot (`MAX_SEGMENTS = 1024`) plus its
//! backing OS reservation (>= 4 MiB). A workload that allocates-and-frees
//! more than ~1024 large blocks on the SAME thread eventually exhausts the
//! table; `SegmentTable::register` then returns `None`, `alloc_large`
//! returns null, and (through the `GlobalAlloc` face) the process aborts.
//!
//! Post-fix, the "not admitted to cache" / "no cache at all" branches call
//! `self.table.unregister(base)` followed by an immediate
//! `os::release_segment(...)` — mirroring the already-correct cross-thread
//! `AllocCore::reclaim_large_segment` (task A1) pattern. `unregister` runs
//! BEFORE the release, so `AllocCore::drop`'s `table.bases()` walk (which is
//! the sole source of truth for what `drop` still owns) never sees `base`
//! again: no double-free, no leak.
//!
//! ## Counterfactual (non-vacuity)
//!
//! Run manually during development by reverting the own-thread Large-dealloc
//! branches in `src/alloc_core/alloc_core.rs` back to "only zero `magic`,
//! defer release to `Drop`": both tests below fail around iteration 1024
//! (`SegmentTable` exhausted → `AllocCore::alloc` returns null). Restoring
//! the eager `unregister` + `release_segment` fix makes both pass for
//! `N = 1500` (`> MAX_SEGMENTS`).
//!
//! ## Feature gating
//!
//! This file is gated on `feature = "alloc-core"` and explicitly EXCLUDES
//! `feature = "alloc-decommit"` for the primary test
//! (`own_thread_large_dealloc_no_leak_without_decommit`), because that test
//! targets the `#[cfg(not(feature = "alloc-decommit"))]` branch specifically
//! — under `alloc-decommit` the large-cache would normally admit these
//! same-size spans and never reach the "release eagerly" branch at all,
//! making the test vacuous for that build. Run it with:
//! `cargo test --features alloc-core --release --test
//! regression_own_thread_large_no_leak` (NOT `--features production`, which
//! pulls in `alloc-decommit`).
//!
//! A second test, `own_thread_large_dealloc_no_leak_with_decommit_admission_reject`,
//! covers the `alloc-decommit` admission-reject sub-branch: it configures a
//! `LargeCacheConfig` with a `budget_bytes` far smaller than one span's
//! `usable_size`, so every deposit attempt is declined by the budget check
//! and the eager-release fallback fires every iteration. This test is gated
//! on `feature = "alloc-decommit"` (additively — it compiles whenever that
//! feature is present, e.g. under `--features production`).

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

// 512 KiB — comfortably above `SMALL_MAX`, so every allocation is
// unambiguously routed through `AllocCore::alloc_large` / the Large dealloc
// branch.
const SIZE: usize = 512 * 1024;
// > MAX_SEGMENTS (1024): if a single own-thread large free ever failed to
// release its slot, exhaustion would occur strictly before this many
// sequential alloc+dealloc cycles complete.
const N: usize = 1500;

#[cfg(not(feature = "alloc-decommit"))]
#[test]
fn own_thread_large_dealloc_no_leak_without_decommit() {
    let layout = Layout::from_size_align(SIZE, 8).expect("valid layout");
    let mut core = AllocCore::new().expect("AllocCore::new must succeed");

    for i in 0..N {
        let p = core.alloc(layout);
        assert!(
            !p.is_null(),
            "AllocCore::alloc returned null at iteration {i}/{N} — \
             SegmentTable exhausted (task #125 own-thread large-free leak \
             regression: own-thread dealloc failed to release the slot \
             eagerly and Drop never ran to reclaim it)"
        );
        // Touch first/last byte: a too-small block would corrupt a
        // neighbouring live allocation and surface as corruption elsewhere.
        unsafe {
            p.write(0xAB);
            p.add(SIZE - 1).write(0xCD);
        }
        // Free immediately on the SAME thread — this is the own-thread path
        // under test. Sequential (not batched) so a leaking dealloc burns
        // exactly one slot per iteration, guaranteeing exhaustion by
        // iteration ~1024 pre-fix.
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }
}

#[cfg(feature = "alloc-decommit")]
#[test]
fn own_thread_large_dealloc_no_leak_with_decommit_admission_reject() {
    use sefer_alloc::LargeCacheConfig;

    // Budget far smaller than one span's usable_size (>= SEGMENT = 4 MiB), so
    // the byte-budget admission check in `AllocCore::dealloc`'s Large branch
    // NEVER admits a deposit — every free takes the "not admitted, release
    // eagerly" fallback, exercising exactly the sub-branch task #125 fixed.
    let cfg = LargeCacheConfig::new().budget_bytes(4096);
    let layout = Layout::from_size_align(SIZE, 8).expect("valid layout");
    let mut core = AllocCore::new_with_config(cfg).expect("AllocCore::new_with_config");

    for i in 0..N {
        let p = core.alloc(layout);
        assert!(
            !p.is_null(),
            "AllocCore::alloc returned null at iteration {i}/{N} — \
             SegmentTable exhausted (task #125 admission-reject leak \
             regression: own-thread dealloc failed to release the slot \
             eagerly when large-cache admission was declined)"
        );
        unsafe {
            p.write(0xAB);
            p.add(SIZE - 1).write(0xCD);
        }
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }
}
