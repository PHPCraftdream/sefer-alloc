//! PRF-03 (task r20): counterfactual assertions on the Large-cache
//! payload-change probe deltas, run under a real installed `SeferAlloc`
//! global allocator (the crate's standard test pattern — see
//! `tests/global_alloc_installed.rs`).
//!
//! Counterfactuals:
//! 1. same-size + same-alignment realloc loop → cache HIT counted, and the
//!    changed-payload counter counts NOTHING (payload offset is unchanged);
//! 2. same size re-allocated with a LARGER-than-page alignment (64 KiB —
//!    still < 4 MiB SEGMENT, so still cache-eligible) → the hit IS counted
//!    as a changed-payload hit, because the new payload offset
//!    `align_up(hdr, align.max(page))` exceeds the cached segment's previous
//!    payload offset. (A bare 4096 alignment would NOT change the payload
//!    offset on any host with page >= 4 KiB — `align.max(page)` already
//!    covers it — hence 64 KiB here.)
//! 3. `requests == hits + misses` over the whole window.
//!
//! Measurement-only: reads
//! `sefer_alloc::alloc_core::r20_prf03_large_probe_snapshot` (Relaxed loads
//! over monotonic counters; single sequential test, no interleaving).

#[global_allocator]
static GLOBAL: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();

use sefer_alloc::alloc_core::r20_prf03_large_probe_snapshot;
use std::alloc::{alloc, dealloc, Layout};

const SIZE: usize = 8 * 1024 * 1024; // confirmed Large: > SMALL_MAX (~252 KiB geo-top)

fn raw_alloc(size: usize, align: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, align).expect("valid layout");
    // SAFETY: layout has non-zero size; freed with the same layout below.
    let ptr = unsafe { alloc(layout) };
    assert!(!ptr.is_null(), "Large alloc failed");
    ptr
}

/// SAFETY: `ptr` must have been returned by `raw_alloc(size, align)`.
unsafe fn raw_dealloc(ptr: *mut u8, size: usize, align: usize) {
    let layout = Layout::from_size_align(size, align).expect("valid layout");
    // SAFETY: caller contract above.
    unsafe { dealloc(ptr, layout) };
}

#[test]
fn prf03_payload_change_probe_counterfactuals() {
    let base = r20_prf03_large_probe_snapshot();

    // Round 1: first alloc goes to the slow OS path; dealloc deposits the
    // segment into the large cache (synchronous reclaim, alloc-decommit,
    // default 256 MiB per-heap budget >> 8 MiB).
    let p1 = raw_alloc(SIZE, 16);
    // SAFETY: allocated just above with the identical size/align pair.
    unsafe { raw_dealloc(p1, SIZE, 16) };

    // Round 2: same size + align → best-fit cache HIT with an UNCHANGED
    // payload offset (same `align_up(hdr, max(16, page))` computation).
    let p2 = raw_alloc(SIZE, 16);
    let after_same = r20_prf03_large_probe_snapshot();
    // SAFETY: allocated just above with the identical size/align pair.
    unsafe { raw_dealloc(p2, SIZE, 16) };

    let d_same = [
        after_same.requests - base.requests,
        after_same.hits - base.hits,
        after_same.misses - base.misses,
        after_same.changed_payload_hits - base.changed_payload_hits,
    ];
    assert!(
        d_same[0] >= 2,
        "expected >=2 Large requests, got {d_same:?}"
    );
    assert!(
        d_same[1] >= 1,
        "expected >=1 cache hit from the same-size realloc, got {d_same:?}"
    );
    // Same-payload counterfactual: NO change counted.
    assert_eq!(
        d_same[3], 0,
        "same-align hits must NOT count as changed-payload hits, got {d_same:?}"
    );

    // Round 3: same size, align 64 KiB (> page size, < SEGMENT) → the hit's
    // new payload offset differs from the cached one → activation
    // counterfactual: the change IS counted.
    let p3 = raw_alloc(SIZE, 64 * 1024);
    let after_aligned = r20_prf03_large_probe_snapshot();
    // SAFETY: allocated just above with the identical size/align pair.
    unsafe { raw_dealloc(p3, SIZE, 64 * 1024) };

    let d_aligned = [
        after_aligned.requests - base.requests,
        after_aligned.hits - base.hits,
        after_aligned.misses - base.misses,
        after_aligned.changed_payload_hits - base.changed_payload_hits,
    ];
    assert!(
        d_aligned[3] >= 1,
        "expected >=1 changed-payload hit from the 64KiB-aligned realloc, got {d_aligned:?}"
    );
    // Conservation: every request is either a hit or a miss.
    assert_eq!(
        d_aligned[0],
        d_aligned[1] + d_aligned[2],
        "requests must equal hits + misses, got {d_aligned:?}"
    );
}
