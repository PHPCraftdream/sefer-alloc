//! B4 X-mix diagnosis (worktree b4-judges): tests for the `bench-internals`
//! magazine slow-path counters read via `SeferAlloc::dbg_xmix_counters()`
//! (positional layout, pairs in bucket order `[≤512, >512]`):
//! `[0..2]` refill events, `[2..4]` refill blocks, `[4..6]` overflow events,
//! `[6..8]` flushed blocks, `[8..10]` R1-01 events.
//!
//! Counters are process-global and monotonic; each test file is its own
//! binary, and the counter-sensitive scenario phases run sequentially inside
//! ONE `#[test]` so parallel test threads cannot interleave allocations into
//! the measured deltas.
//!
//! Run: `cargo test --test b4_judges_xmix_counters --features "internals
//! bench-internals alloc-global fastbin alloc-xthread"`
#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

fn snapshot() -> [u64; 10] {
    SeferAlloc::dbg_xmix_counters()
}

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, 8).expect("valid layout")
}

/// Phase 1 — refill counter: grows when the magazine is exhausted (alloc past
/// depth 0) and does NOT grow on pure magazine hits (alloc within a deep
/// magazine). Uses the 64 B class (block ≤ 512 B bucket).
#[test]
fn xmix_refill_and_hit_phases_with_bucket_split_and_overflow() {
    // ── Phase 1: refill vs pure hit (64 B, ≤512 bucket) ─────────────────
    let s0 = snapshot();
    // First allocation of the class fires a refill (magazine at depth 0).
    let a = unsafe { GLOBAL.alloc(layout(64)) };
    assert!(!a.is_null());
    let s1 = snapshot();
    assert!(
        s1[0] > s0[0],
        "refill_events(≤512) must grow on the first alloc after magazine exhaustion"
    );
    assert!(
        s1[1] == s0[1],
        "refill_events(>512) must NOT grow for a 64 B class"
    );
    assert!(
        s1[2] > s0[2],
        "refill_blocks(≤512) must grow with the refill"
    );
    let refill_blocks_first = s1[2] - s0[2];
    assert!(
        (1..=16).contains(&refill_blocks_first),
        "one refill writes 1..=16 blocks, got {refill_blocks_first}"
    );

    // Pure-hit window: free then re-alloc the same class repeatedly. The
    // free pushes the block back (depth ≥ 1), so every alloc is a hit.
    let s2 = snapshot();
    for _ in 0..64 {
        // SAFETY: live block, freed once, immediately re-issued.
        unsafe { GLOBAL.dealloc(a, layout(64)) };
        let b = unsafe { GLOBAL.alloc(layout(64)) };
        assert_eq!(b, a, "magazine hit must re-issue the just-freed block");
    }
    let s3 = snapshot();
    assert_eq!(
        s3[0], s2[0],
        "refill_events(≤512) must NOT grow on pure magazine hits"
    );
    assert_eq!(
        s3[2], s2[2],
        "refill_blocks(≤512) must NOT grow on pure magazine hits"
    );

    // ── Phase 2: overflow (64 B, ≤512 bucket) and R1-01 (8 KiB, >512) ───
    // Drain, then over-free: TCACHE_CAP = 16 for the 64 B class, so the 17th
    // free into a full magazine half-flushes (overflow event + FLUSH_N
    // flushed blocks).
    let mut blocks: Vec<*mut u8> = Vec::new();
    for _ in 0..64 {
        let p = unsafe { GLOBAL.alloc(layout(64)) };
        assert!(!p.is_null());
        blocks.push(p);
    }
    let s4 = snapshot();
    // Freeing 18 live blocks into an empty(ish) magazine: pushes fill it to
    // cap, further frees overflow.
    for p in blocks.iter().take(18) {
        // SAFETY: live block, freed exactly once.
        unsafe { GLOBAL.dealloc(*p, layout(64)) };
    }
    let s5 = snapshot();
    assert!(
        s5[4] > s4[4],
        "overflow_events(≤512) must grow when frees exceed the magazine cap"
    );
    assert!(
        s5[6] > s4[6],
        "flush_blocks(≤512) must grow with each overflow half-flush"
    );

    // R1-01: the 6 KiB class has FREE_PARK_CAP < TCACHE_CAP (64 KiB / 6 KiB
    // = 10). A refill parks cap-1 = 9 blocks; frees fill the magazine back to
    // cap, then the free AFTER that fires R1-01 (single-block flush straight
    // to the substrate). 6144 B (not 8192 B) stays inside the small-class
    // ladder (the larson mix tops out at 8191 B). Frees only — no intervening
    // allocs — so the depth monotonically climbs past cap.
    let mut big: Vec<*mut u8> = Vec::new();
    for _ in 0..12 {
        let p = unsafe { GLOBAL.alloc(layout(6144)) };
        assert!(!p.is_null());
        big.push(p);
    }
    let s6 = snapshot();
    for p in &big {
        // SAFETY: live block, freed exactly once.
        unsafe { GLOBAL.dealloc(*p, layout(6144)) };
    }
    let s7 = snapshot();
    assert!(
        s7[9] > s6[9],
        "r101_events(>512) must grow when a free hits the per-class byte budget cap"
    );
    assert!(
        s7[1] == s6[1] && s7[0] == s6[0],
        "no refill expected while freeing into stocked magazines"
    );

    // ── Phase 3: bucket split on 1024 B (>512) vs 64 B (≤512) refills ────
    // Drain a fresh class completely so a refill provably fires, then check
    // the >512 bucket moved while the ≤512 refill buckets did not.
    let mut mid: Vec<*mut u8> = Vec::new();
    for _ in 0..40 {
        let p = unsafe { GLOBAL.alloc(layout(1024)) };
        assert!(!p.is_null());
        mid.push(p);
    }
    let s8 = snapshot();
    for p in &mid {
        // SAFETY: live block, freed exactly once.
        unsafe { GLOBAL.dealloc(*p, layout(1024)) };
    }
    let mut drained: Vec<*mut u8> = Vec::new();
    // The magazine (≤16 blocks) plus refill leftovers must eventually empty:
    // allocate well past the parked stock to force a fresh >512 refill.
    for _ in 0..200 {
        let p = unsafe { GLOBAL.alloc(layout(1024)) };
        assert!(!p.is_null());
        drained.push(p);
    }
    let s9 = snapshot();
    assert!(
        s9[1] > s8[1],
        "refill_events(>512) must grow when the 1024 B magazine is exhausted"
    );
    assert!(
        s9[0] == s8[0],
        "refill_events(≤512) must NOT move during a 1024 B-only drain"
    );

    // Cleanup: release everything still live.
    fn freed(p: *mut u8, size: usize) {
        // SAFETY: live block, freed exactly once.
        unsafe { GLOBAL.dealloc(p, layout(size)) };
    }
    for &p in &drained {
        freed(p, 1024);
    }
    for &p in &mid {
        freed(p, 1024);
    }
    for &p in blocks.iter().skip(18) {
        freed(p, 64);
    }
}
