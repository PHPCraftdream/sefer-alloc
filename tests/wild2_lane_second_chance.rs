//! WILD-2: the own-thread small free lane's second chance. On a memo miss the
//! lane probes the direct-mapped `own_cache`; an exact-base hit arms the memo
//! and runs the lane's acceptance steps on that root. The oracle is the same as
//! `as1_fast_free_lane.rs`: the magazine is LIFO, so an accepted free hands the
//! same block to the next allocation of its layout and a rejected one must not.
//! `dbg_lane_second_chance_hits` proves which exit ran, and the cases replay the
//! lane's reject boundaries through the second-chance exit.
//! `exact-object-proto` aborts on the deliberately invalid frees below, so it
//! is excluded; `hardened` compiles the lane out.
#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "hardened"),
    not(feature = "exact-object-proto"),
    not(miri)
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::Mutex;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::SeferAlloc;

static A: SeferAlloc = SeferAlloc::new();
/// The counters are process-wide and the cases below count deltas.
static SERIAL: Mutex<()> = Mutex::new(());

const SEGMENT: usize = 4 * 1024 * 1024;
/// 2 KiB blocks: about two thousand per segment, so a few thousand allocations
/// span two Small segments.
const BLOCK: usize = 2048;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, 8).unwrap()
}

fn base_of(p: *mut u8) -> usize {
    p as usize & !(SEGMENT - 1)
}

/// Live blocks of `BLOCK` bytes in two different segments: `(x, y)` with at
/// least `n` blocks each; the rest are returned in `rest` and stay live.
fn two_segments(n: usize) -> (Vec<*mut u8>, Vec<*mut u8>, Vec<*mut u8>) {
    let mut all = Vec::new();
    // SAFETY: nonzero size; every block is freed by the caller.
    for _ in 0..40_000 {
        let p = unsafe { A.alloc(layout(BLOCK)) };
        assert!(!p.is_null());
        all.push(p);
        let first = base_of(all[0]);
        let other: Vec<usize> = all
            .iter()
            .map(|q| base_of(*q))
            .filter(|b| *b != first)
            .collect();
        if let Some(&second) = other.first() {
            let in_first = all.iter().filter(|q| base_of(**q) == first).count();
            let in_second = all.iter().filter(|q| base_of(**q) == second).count();
            if in_first >= n && in_second >= n {
                break;
            }
        }
    }
    let first = base_of(all[0]);
    let second = all
        .iter()
        .map(|q| base_of(*q))
        .find(|b| *b != first)
        .expect("40 000 blocks of 2 KiB must span two segments");
    let (mut x, mut y, mut rest) = (Vec::new(), Vec::new(), Vec::new());
    for p in all {
        let b = base_of(p);
        if b == first && x.len() < n {
            x.push(p);
        } else if b == second && y.len() < n {
            y.push(p);
        } else {
            rest.push(p);
        }
    }
    assert!(
        x.len() == n && y.len() == n,
        "need {n} blocks in each segment"
    );
    (x, y, rest)
}

/// The next allocation of `BLOCK` returns `p` iff `p` is on top of the magazine.
fn reissued(p: *mut u8) -> bool {
    // SAFETY: nonzero size; the block is freed again before return.
    let q = unsafe { A.alloc(layout(BLOCK)) };
    let hit = q == p;
    // SAFETY: q was just allocated with this layout.
    unsafe { A.dealloc(q, layout(BLOCK)) };
    hit
}

fn release(blocks: impl IntoIterator<Item = *mut u8>) {
    for p in blocks {
        // SAFETY: every block is live with this layout and freed once.
        unsafe { A.dealloc(p, layout(BLOCK)) };
    }
}

#[test]
fn memo_miss_with_own_cache_hit_is_accepted_by_the_second_chance() {
    let _g = SERIAL.lock().unwrap();
    let (x, y, rest) = two_segments(8);
    // Prime: one free per segment fills `own_cache`, the second free in a
    // segment arms the memo; alternate afterwards so every free is a memo miss.
    // SAFETY: each block is live with this layout and freed once.
    unsafe {
        for i in 0..4 {
            A.dealloc(x[i], layout(BLOCK));
            A.dealloc(y[i], layout(BLOCK));
        }
    }
    let before = AllocCore::dbg_lane_second_chance_hits();
    // x[4] follows a free in the other segment: memo miss, own_cache hit.
    // SAFETY: live, freed once.
    unsafe {
        A.dealloc(y[4], layout(BLOCK));
        A.dealloc(x[4], layout(BLOCK));
    }
    let after = AllocCore::dbg_lane_second_chance_hits();
    assert!(
        after > before,
        "an alternating free must take the second chance ({before} -> {after})"
    );
    assert!(
        reissued(x[4]),
        "a free accepted by the second chance must reach the magazine top"
    );
    // The segment memo now holds x's segment.
    // SAFETY: live, freed once.
    unsafe { A.dealloc(y[5], layout(BLOCK)) };
    assert!(reissued(y[5]), "the other segment is accepted the same way");
    release(x[5..].iter().copied().chain(y[6..].iter().copied()));
    release(rest);
}

#[test]
fn rejects_through_the_second_chance_match_the_chain() {
    let _g = SERIAL.lock().unwrap();
    let (x, y, rest) = two_segments(8);
    // SAFETY: live, freed once; this makes the memo hold y's segment.
    unsafe {
        A.dealloc(y[0], layout(BLOCK));
        A.dealloc(y[1], layout(BLOCK));
        A.dealloc(x[0], layout(BLOCK));
        A.dealloc(y[2], layout(BLOCK));
    }
    // Double free of x[3] with a memo miss each time: the second one must be
    // rejected by the state byte, not pushed twice.
    // SAFETY: first free is valid; the second is the double free under test.
    unsafe {
        A.dealloc(x[3], layout(BLOCK));
        A.dealloc(y[3], layout(BLOCK));
        A.dealloc(x[3], layout(BLOCK));
    }
    // SAFETY: nonzero size.
    let (a, b) = unsafe { (A.alloc(layout(BLOCK)), A.alloc(layout(BLOCK))) };
    assert_ne!(
        a, b,
        "a double free must not put the block in the magazine twice"
    );
    // SAFETY: both live with this layout.
    unsafe {
        A.dealloc(a, layout(BLOCK));
        A.dealloc(b, layout(BLOCK));
    }
    // Interior granule and misaligned address in x's segment while the memo
    // holds y's: rejected, never issued.
    // SAFETY: deliberately invalid frees inside a live block of x's segment.
    unsafe {
        A.dealloc(y[4], layout(BLOCK));
        A.dealloc(x[5].add(16), layout(16));
        A.dealloc(y[5], layout(BLOCK));
        A.dealloc(x[5].add(8), layout(BLOCK));
    }
    assert!(
        !reissued(x[5].wrapping_add(16)),
        "interior granule must not be issued"
    );
    assert!(
        !reissued(x[5].wrapping_add(8)),
        "misaligned pointer must not be issued"
    );
    // Wrong layout: a 2 KiB block freed as 256 B, memo miss.
    // SAFETY: deliberate wrong-layout free of a live block.
    unsafe {
        A.dealloc(y[6], layout(BLOCK));
        A.dealloc(x[6], layout(256));
    }
    assert!(
        !reissued(x[6]),
        "a wrong-class free must not reissue the block"
    );
    // SAFETY: x[6] is still live with its real layout.
    unsafe { A.dealloc(x[6], layout(BLOCK)) };
    assert!(reissued(x[6]));
    // Metadata offset and an uncarved offset of y's segment (the newest, so
    // nearly empty), memo on x's.
    let base = base_of(y[7]) as *mut u8;
    // SAFETY: deliberately invalid frees; the allocator must ignore them.
    unsafe {
        A.dealloc(x[7], layout(BLOCK));
        A.dealloc(base.wrapping_add(0x1000), layout(BLOCK));
        A.dealloc(base.wrapping_add(SEGMENT - 4096), layout(BLOCK));
    }
    assert!(!reissued(base.wrapping_add(0x1000)));
    assert!(!reissued(base.wrapping_add(SEGMENT - 4096)));
    release([x[1], x[2], x[4], x[5], y[7]]);
    release(rest);
}

#[test]
fn a_free_the_lane_never_accepts_is_not_counted() {
    let _g = SERIAL.lock().unwrap();
    // Over-aligned and Large frees leave at the first lane check and never
    // reach the second chance.
    let before = AllocCore::dbg_lane_second_chance_hits();
    let big = Layout::from_size_align(1 << 20, 8).unwrap();
    let wide = Layout::from_size_align(64, 64).unwrap();
    // SAFETY: nonzero sizes; each block is freed once with its layout.
    unsafe {
        let l = A.alloc(big);
        let w = A.alloc(wide);
        assert!(!l.is_null() && !w.is_null());
        A.dealloc(l, big);
        A.dealloc(w, wide);
    }
    assert_eq!(AllocCore::dbg_lane_second_chance_hits(), before);
}
