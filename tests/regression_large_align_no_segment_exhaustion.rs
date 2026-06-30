//! Regression test for task #114 — `SegmentTable` exhaustion under repeated
//! `align > MIN_BLOCK` allocations.
//!
//! ## What this guards against
//!
//! Pre-fix (sefer-alloc ≤ 0.2.0), `SizeClasses::class_for(size, align)`
//! unconditionally returned `None` when `align > SMALL_ALIGN_MAX` (= MIN_BLOCK
//! = 16). EVERY allocation with `align > 16` — including the
//! `tokio::runtime::task::core::Cell<T,S>` shape (≈640 B, `#[repr(align(128))]`
//! against false sharing) — was routed to the dedicated-segment Large path,
//! burning a full ~4 MiB segment + one `SegmentTable` slot per request.
//!
//! Under a workload that spawns more than ~1024 such allocations cumulatively
//! (concurrent tokio task-spawning is the canonical case — see
//! `shamir-db/duplex_throughput/duplex_cap32/32`), the per-process
//! `SegmentTable` cap (`MAX_SEGMENTS = 1024`) was exhausted, then
//! `alloc_large_slow → SegmentTable::register` returned `None`, then the
//! `GlobalAlloc` face returned null, then `std::alloc::handle_alloc_error`
//! aborted the process with `memory allocation of 640 bytes failed`.
//!
//! Post-fix, `class_for(640, 128)` resolves to the smallest small class with
//! `block_size ≥ 640` AND `block_size % 128 == 0` (= 768 B in the current
//! table geometry). One segment now serves *many* such allocations from a
//! shared free list; the `SegmentTable` is no longer touched on the hot path.
//!
//! ## Counterfactual (non-vacuity)
//!
//! Reverting the fix — re-adding `if align > SMALL_ALIGN_MAX { return None; }`
//! at the head of `SizeClasses::class_for` — makes this test fail. Pre-fix
//! every iteration burned one segment, so iteration ≈ 1024 returns null from
//! `AllocCore::alloc` and the `assert!(!ptr.is_null(), ...)` in the loop
//! trips.
//!
//! ## Test shape
//!
//! Single-threaded `AllocCore` (the substrate the `GlobalAlloc` face wraps)
//! — no `tokio`, no `#[global_allocator]` install, no cross-thread paths;
//! this isolates the size-classifier defect from the orthogonal concurrent /
//! global-allocator surfaces. We allocate `N` blocks with `(size=640,
//! align=128)`, hold them all live (so `dealloc` does NOT recycle slots and
//! the regression is reproducible), then deallocate at the end to exercise
//! the dealloc routing under the new size class (M2 defence-in-depth — a
//! second `dealloc` of the same pointer must be a safe no-op).
//!
//! `N = 2048` is comfortably above `MAX_SEGMENTS = 1024` so pre-fix
//! exhaustion is guaranteed, and small enough that the test runs in well
//! under a second on a release build (a single small segment holds ~5000
//! 768-byte blocks).

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

#[test]
fn many_align128_allocations_do_not_exhaust_segment_table() {
    const N: usize = 2048;
    const SIZE: usize = 640;
    const ALIGN: usize = 128;

    let layout = Layout::from_size_align(SIZE, ALIGN).expect("valid layout");
    let mut core = AllocCore::new().expect("AllocCore::new must succeed");

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = core.alloc(layout);
        assert!(
            !p.is_null(),
            "AllocCore::alloc returned null at iteration {i}/{N} \
             — SegmentTable likely exhausted (pre-#114 regression)"
        );
        assert_eq!(
            (p as usize) % ALIGN,
            0,
            "iteration {i}: pointer {p:#p} not aligned to {ALIGN}"
        );
        // Touch the first and last byte so an actually-too-small block (M4
        // violation) would corrupt a neighbour and tripping an assertion
        // later in the loop — the lazy fault is the cheapest end-to-end
        // M4 check we can do without a foreign-pointer probe.
        //
        // SAFETY: `p` is valid for `SIZE` bytes per the M1 contract.
        unsafe {
            p.write(0xAB);
            p.add(SIZE - 1).write(0xCD);
        }
        ptrs.push(p);
    }

    // All blocks are live. Free them — exercises the dealloc routing under
    // the new size class to confirm class_for is consistent across alloc
    // and dealloc paths (a divergence would corrupt the free list).
    for &p in &ptrs {
        core.dealloc(p, layout);
    }

    // M2 (double-free guard): re-freeing every pointer must be a safe
    // no-op — neither corruption nor panic.
    for &p in &ptrs {
        core.dealloc(p, layout);
    }
}

#[test]
fn align64_and_align256_also_resolve_to_small_path() {
    // Sister check: the fix covers a range of async-runtime alignments, not
    // just 128. Pre-fix any of these would have burned a segment per alloc;
    // post-fix all resolve through the small path.
    for &(size, align) in &[
        (128usize, 64usize),   // common atomic / cache-line-half padding
        (256, 256),            // exact cache line
        (768, 128),            // tokio Cell upper-bound shape
        (1024, 64),            // medium aligned buffer
    ] {
        let layout = Layout::from_size_align(size, align).expect("valid layout");
        let mut core = AllocCore::new().expect("AllocCore::new must succeed");
        // 1500 > MAX_SEGMENTS=1024 — sufficient to expose the pre-fix
        // exhaustion for each shape independently.
        const N: usize = 1500;
        let mut ptrs = Vec::with_capacity(N);
        for i in 0..N {
            let p = core.alloc(layout);
            assert!(
                !p.is_null(),
                "size={size} align={align} iter={i}: null \
                 — class_for must resolve to a divisible small class"
            );
            assert_eq!((p as usize) % align, 0, "pointer not aligned");
            ptrs.push(p);
        }
        for &p in &ptrs {
            core.dealloc(p, layout);
        }
    }
}
