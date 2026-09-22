//! Task #2002 regression — before this task, `HeapCore::dealloc_batch`'s
//! Small-classified fast path (`dealloc_batch_small`,
//! `src/registry/heap_core/free/dealloc_batch.rs`) had NO equivalent to the
//! scalar `dealloc_own_thread_with_base`'s F7 branch (A): a legitimately
//! promoted-and-grown Large block (a `realloc` that crosses
//! `MEDIUM_REALLOC_PROMOTION_THRESHOLD` under `medium-classes`, reclassifying
//! its segment to `SegmentKind::Large`) freed through `dealloc_batch` fell
//! straight into the M2 small-segment oracles — which read/write the Large
//! block's own payload bytes as if they were a Small segment's bitmap —
//! instead of being routed to the real Large free. Fixed by extracting the
//! full F7(A+B)/H1/M2 guard chain into `small_free_guard`
//! (`src/registry/heap_core/free/dealloc_own_base.rs`), shared verbatim by
//! both the scalar and batched paths.
//!
//! ## Reproduction strategy
//!
//! Deliberately gated `not(alloc-decommit)`: under `alloc-decommit`, a freed
//! Large span may be ADMITTED into the large-cache instead of eagerly
//! released, making a per-free "did `segments_released_total` advance?"
//! check non-deterministic (mirrors
//! `tests/regression_own_thread_large_no_leak.rs`'s own
//! `not(alloc-decommit)`-gated variant and its module doc's explanation of
//! why). Without `alloc-decommit`, the large-cache does not exist at all —
//! EVERY correct Large free eagerly releases the segment, incrementing the
//! process-wide `segments_released_total` counter by exactly one. The pre-fix
//! bug either silently no-ops (a misread M2 oracle) or corrupts the segment's
//! own header/payload bytes without ever calling the real Large-free path —
//! in both cases `segments_released_total` does NOT advance — making an
//! exact per-round delta assertion a decisive, deterministic counterfactual
//! (fails pre-fix, passes post-fix) without needing a large/expensive
//! iteration count.
//!
//! Deliberately does NOT require `hardened` — the gap this task fixed was
//! reachable in ANY build with `medium-classes` promotion reachable, which
//! the existing `dealloc_batch_large_via_small_layout_is_noop`
//! (`tests/r11_4_dealloc_batch_hardened_guards.rs`, gated on `hardened`)
//! never exercised: that test's own F7 case is branch (A) forced by
//! `cfg!(hardened)` being true regardless of size, not by an actually
//! promoted-and-grown block reaching branch (A)'s `layout.size() >=
//! MEDIUM_REALLOC_PROMOTION_THRESHOLD` arm on a non-`hardened` build.
//!
//! Run with (matches `.verify_final.sh`'s dedicated task #2002 row):
//!   cargo test --features "batch-api medium-classes fastbin alloc-global internals" \
//!     --test regression_dealloc_batch_promoted_large_free

#![cfg(all(
    feature = "internals",
    feature = "alloc-global",
    feature = "fastbin",
    feature = "batch-api",
    feature = "medium-classes",
    not(feature = "alloc-decommit")
))]

use std::alloc::Layout;

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::SeferAlloc;

const ALIGN: usize = 8;
/// Mirrors `tests/r14_4_promotion_free_correctness.rs`'s constant of the same
/// name/value — the internal `MEDIUM_REALLOC_PROMOTION_THRESHOLD` is not
/// public API, so both files hardcode its known value.
const PROMOTION_THRESHOLD: usize = 256 * 1024;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, ALIGN).unwrap()
}

#[test]
fn dealloc_batch_promoted_large_free_releases_eagerly() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let a = SeferAlloc::new();

    for round in 0..5 {
        let old_size = 96 * 1024;
        let old_layout = layout(old_size);
        // SAFETY: valid layout; `heap` is the calling thread's own slot.
        let p = unsafe { (*heap).alloc(old_layout) };
        assert!(!p.is_null(), "round {round}: initial alloc failed");
        // SAFETY: p valid for old_size bytes.
        unsafe { p.write(0xAB) };

        let new_size = PROMOTION_THRESHOLD + 1024 * (round + 1);
        // SAFETY: p live, old_layout matches, freed at most once on success.
        let grown = unsafe { (*heap).realloc(p, old_layout, new_size) };
        assert!(!grown.is_null(), "round {round}: growing realloc failed");
        // SAFETY: grown valid for new_size >= old_size bytes; the growth
        // copy must have preserved byte 0.
        assert_eq!(
            unsafe { grown.read() },
            0xAB,
            "round {round}: canary byte lost during the growth copy"
        );
        let grown_layout = layout(new_size);

        let released_before = a.stats().segments_released_total;

        // SAFETY: `grown` is a live allocation of `grown_layout` (the exact
        // layout the growing `realloc` above returned it for), made by
        // `heap`'s own realloc, freed exactly once here — through the
        // batched entry point specifically under test.
        unsafe { (*heap).dealloc_batch(grown_layout, &[grown]) };

        let released_after = a.stats().segments_released_total;
        assert_eq!(
            released_after,
            released_before + 1,
            "round {round}: dealloc_batch of a promoted-and-grown Large \
             block did not take the eager-release Large-free path exactly \
             once (F7 branch (A) regression — task #2002: dealloc_batch_small \
             previously had no Large-kind guard reachable under \
             medium-classes promotion without hardened, and fell through to \
             the M2 small-segment oracles instead of routing to the real \
             Large free)"
        );
    }

    // SAFETY: `heap` was claimed above; recycled whole here.
    unsafe { HeapRegistry::recycle(heap) };
}
