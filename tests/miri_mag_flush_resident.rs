//! GR-1: the magazine flush skips the per-block span / state-byte re-checks
//! (`AllocCore::flush_magazine_internal`) because every magazine entry is
//! MAG-resident (invariant I-MAG). Under Miri (a ghost build) the ghost check
//! `mag-flush/mag-resident` asserts I-MAG for every flushed block, on real
//! `Box` drops through the installed allocator. Path-activation oracle: each of
//! the three magazine flush sites records a ghost event, and each must fire:
//! key 1 = overflow half-flush, key 2 = cap-branch eviction, key 3 = teardown.
//! One scenario per process; no libtest is linked.
//!
//! Native ghost run:
//! `RUSTFLAGS="--cfg sefer_ghost" cargo test --features "alloc-global fastbin internals bench-internals" --test miri_mag_flush_resident`.

#[cfg(any(miri, sefer_ghost))]
use sefer_alloc::ghost;
use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

#[cfg(any(miri, sefer_ghost))]
fn flushed(site: u64) -> i64 {
    ghost::count("mag-flush/mag-resident", site)
}
#[cfg(not(any(miri, sefer_ghost)))]
fn flushed(_site: u64) -> i64 {
    0
}

/// 16 B class: `FREE_PARK_CAP == TCACHE_CAP == 16`; more frees than the
/// magazine and its depot hold force the overflow half-flush.
fn churn_small(tag: u8) {
    let n = 16 * 12;
    let mut boxes: Vec<Box<[u8; 16]>> = (0..n).map(|i| Box::new([tag ^ i as u8; 16])).collect();
    while let Some(b) = boxes.pop() {
        drop(b);
    }
    let again: Vec<Box<[u8; 16]>> = (0..n).map(|i| Box::new([tag.wrapping_add(i as u8); 16])).collect();
    for (i, b) in again.iter().enumerate() {
        assert_eq!(b[15], tag.wrapping_add(i as u8), "reissued block keeps its new contents");
    }
}

/// 8 KiB class: `FREE_PARK_CAP == 8 < TCACHE_CAP`; the ninth free takes the
/// cap branch and evicts a resident block (Small-kind only under Miri).
fn cap_evict() {
    let mut boxes: Vec<Box<[u8; 8192]>> = (0..=8u8).map(|i| Box::new([i; 8192])).collect();
    while let Some(b) = boxes.pop() {
        drop(b);
    }
}

fn main() {
    let before = [flushed(1), flushed(2), flushed(3)];
    churn_small(1);
    cap_evict();
    // Thread exit trims the spawned thread's magazine (teardown flush).
    std::thread::spawn(|| churn_small(2)).join().unwrap();
    #[cfg(any(miri, sefer_ghost))]
    {
        assert!(flushed(1) > before[0], "overflow half-flush must have executed");
        assert!(flushed(3) > before[2], "teardown flush must have executed");
    }
    #[cfg(miri)]
    assert!(flushed(2) > before[1], "cap-branch eviction must have executed");
    let _ = before;
    println!("[miri_mag_flush_resident] COMPLETE mag_flush_resident");
}
