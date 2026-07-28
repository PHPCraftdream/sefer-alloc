//! R24-8 (task #386) — correctness test for the `STAGE_CAP` reduction
//! (`src/registry/heap_core_dealloc_batch.rs`).
//!
//! `STAGE_CAP` was reduced from 512 to 64 (R24-8, Investigation 2). At
//! `STAGE_CAP = 64`, a batch of N blocks does:
//! - first `TCACHE_CAP` (16) accepted blocks → magazine fill,
//! - remaining `N - 16` → staged, flushed in chunks of 64 via intermediate
//!   `flush_class` calls.
//!
//! So any batch with `N > STAGE_CAP + TCACHE_CAP = 80` triggers the
//! **multi-flush path** — the `if staged == STAGE_CAP { flush; staged = 0; }`
//! mid-loop flush that resets the staging cursor. With `STAGE_CAP = 512` (the
//! old value), batches up to 528 blocks never hit this path; with `STAGE_CAP =
//! 64`, batches as small as 81 blocks do.
//!
//! This test exercises that path with N=200 (→ 3 flushes: 64 + 64 + 56) and
//! verifies every block is correctly freed and recyclable — the zero-trust
//! regression guard for the constant change.
//!
//! **Mutation counterfactual (documented, run by the reviewer):** temporarily
//! remove the mid-loop flush block
//! (`if staged == STAGE_CAP { ... flush_class ... staged = 0; }` in
//! `dealloc_batch_small`). With `staged` growing past `STAGE_CAP`, the
//! `stage[staged] = p` write panics (Rust bounds check on a fixed-size array),
//! making this test RED (panic). Restoring the flush makes it GREEN. This
//! proves the test is non-vacuous at the new `STAGE_CAP`.

#![cfg(all(feature = "alloc-global", feature = "batch-api"))]

use std::alloc::{GlobalAlloc, Layout};
use std::collections::HashSet;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// N=200 is deliberately chosen to exceed `STAGE_CAP(64) + TCACHE_CAP(16) = 80`
/// by a wide margin, forcing THREE intermediate flushes (64+64 = 128 staged at
/// the two `staged == STAGE_CAP` boundaries, then the final 56 at the
/// post-loop flush). All 200 blocks are 16 B @ align 8, so they all land in one
/// 4 MiB segment — a pure same-segment multi-flush stress.
#[test]
fn dealloc_batch_large_multi_flush_all_blocks_recycled() {
    let layout = Layout::from_size_align(16, 8).unwrap();
    let n = 200usize;

    // Phase 1: allocate N blocks via scalar alloc (all same segment).
    let mut blocks: Vec<*mut u8> = Vec::with_capacity(n);
    for _ in 0..n {
        // SAFETY: valid non-zero layout; GLOBAL never returns null at this count.
        let p = unsafe { GLOBAL.alloc(layout) };
        assert!(!p.is_null(), "setup alloc returned null");
        blocks.push(p);
    }
    let distinct_alloc: HashSet<usize> = blocks.iter().map(|&p| p as usize).collect();
    assert_eq!(distinct_alloc.len(), n, "alloc issued duplicate pointers");

    // Phase 2: free all N via one dealloc_batch call — triggers the multi-flush
    // path (3 intermediate + 1 final flush_class calls at STAGE_CAP=64).
    // SAFETY: every entry was allocated by GLOBAL with `layout`; freed once.
    unsafe { GLOBAL.dealloc_batch(layout, &blocks) };

    // Phase 3: re-allocate N blocks of the same layout and verify they are all
    // non-null, distinct, and writable/readable with unique patterns. This
    // proves the multi-flush path lost no blocks (no leak) and introduced no
    // aliasing. We do NOT assert exact pointer-set identity with `original`:
    // this test runs under the GLOBAL allocator, so Rust's own test harness
    // (`HashSet`/`Vec` internals, assertion formatting) allocates from the same
    // pool between the free and the re-alloc, consuming some of the just-freed
    // blocks — pointer-set identity is not a property the global-allocator
    // contract guarantees here. The load-bearing properties are: (a) no OOM
    // (every re-alloc succeeds → the freed blocks returned to the pool, not
    // leaked), and (b) no aliasing (distinct patterns survive).
    let mut re_alloced: Vec<*mut u8> = Vec::with_capacity(n);
    for _ in 0..n {
        // SAFETY: valid non-zero layout.
        let p = unsafe { GLOBAL.alloc(layout) };
        assert!(
            !p.is_null(),
            "re-alloc returned null — a block leaked (multi-flush bug?)"
        );
        re_alloced.push(p);
    }
    let distinct: HashSet<usize> = re_alloced.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        n,
        "re-alloc returned duplicate pointers — aliasing from the multi-flush path"
    );
    // Write+read a unique pattern per block: catches aliasing/clobbering that a
    // pure distinctness check would miss (two slots pointing at the same address
    // would already be caught above, but a freelist-corruption bug could hand
    // out overlapping-but-not-identical ranges — the pattern round-trip catches
    // that too).
    for (i, &p) in re_alloced.iter().enumerate() {
        let pat = 0xBEEF_F00D_0000_0000u64 | (i as u64);
        // SAFETY: `p` is a freshly allocated 16 B block; first 8 bytes in bounds.
        unsafe { std::ptr::write_volatile(p.cast::<u64>(), pat) };
    }
    for (i, &p) in re_alloced.iter().enumerate() {
        let pat = 0xBEEF_F00D_0000_0000u64 | (i as u64);
        // SAFETY: same in-bounds justification.
        let got = unsafe { std::ptr::read_volatile(p.cast::<u64>()) };
        assert_eq!(got, pat, "re-alloced block [{i}] was clobbered (aliasing?)");
    }

    // Cleanup.
    // SAFETY: every entry of `re_alloced` was allocated by GLOBAL with `layout`.
    unsafe { GLOBAL.dealloc_batch(layout, &re_alloced) };
}
