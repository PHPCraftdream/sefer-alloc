//! Ph4b (task #2092, chunk B): late sidecar publication across slot recycle is
//! consumed EXACTLY ONCE (owner decision §2.2 — re-claim via the last
//! recycled-slot hint).
//!
//! Scenario (real `#[global_allocator]`, native threads):
//!   1. A blocker thread binds a slot and STAYS ALIVE, so it pins every FREE
//!      slot but the producer's (its own slot is LIVE; hint ordering is stable).
//!   2. Thread A allocates 32 × 2 MiB Large blocks (each gets its own
//!      reservation — the Large/sidecar path), tags them, hands them over and
//!      exits → its slot is recycled (LIVE → FREE Release, hint = that slot).
//!   3. Thread B (dealloc-only, binds nothing) frees HALF the blocks while the
//!      slot is FREE: each free becomes a pending terminal publication on the
//!      FREE slot's sidecar (no worker exists to consume it).
//!   4. Thread C allocates once — its claim re-claims slot A via the hint, and
//!      the re-claim `trim_for_recycle` drains exactly the pending half-1
//!      publications. C then frees the SECOND half (under its own live lease)
//!      and calls `trim_current_thread()` — draining half-2.
//!
//! Oracles:
//!   - tag read-back before every free: no block was reused/corrupted in
//!     flight (no double accounting while pending or leased);
//!   - `foreign_or_unroutable_frees` +0: every cross-thread free routed;
//!   - `segments_released_total` grows by EXACTLY 32 across the frees: each
//!     Large block retired exactly once — a lost publication leaves the
//!     segment unreleased (delta < 32), a double consume would release it
//!     twice (delta > 32), and both turn this red;
//!   - no abort/corruption anywhere (the allocator's own lost-CAS aborts fire
//!     on a broken single-writer protocol).
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-stats"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::mpsc;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const BLOCK: usize = 2 * 1024 * 1024;
const COUNT: usize = 32;
const HALF: usize = COUNT / 2;

fn layout() -> Layout {
    Layout::from_size_align(BLOCK, 16).expect("valid layout")
}

#[test]
fn late_publication_across_recycle_is_consumed_exactly_once() {
    let layout = layout();
    let stats_start = GLOBAL.stats();

    // 1. Blocker: binds a slot and holds it, so the hint lands on slot A only.
    let (blocker_tx, blocker_rx) = mpsc::channel::<()>();
    let (blocker_hold_tx, blocker_hold_rx) = mpsc::channel::<()>();
    let blocker_send = blocker_tx.clone();
    let blocker = std::thread::spawn(move || {
        let warm = unsafe { GLOBAL.alloc(Layout::from_size_align(64, 8).expect("valid")) };
        assert!(!warm.is_null());
        blocker_send.send(()).expect("main still waits");
        blocker_hold_rx.iter().for_each(|(): ()| {});
        // SAFETY: the warm block outlived every phase below.
        unsafe { GLOBAL.dealloc(warm, Layout::from_size_align(64, 8).expect("valid")) };
    });
    blocker_rx.recv().expect("blocker bound its slot");

    // 2. Producer A: allocate + tag N Large blocks, hand over, exit.
    let (tx, rx) = mpsc::channel::<(usize, u64)>();
    let producer = std::thread::spawn(move || {
        for index in 0..COUNT {
            // SAFETY: valid non-zero layout.
            let pointer = unsafe { GLOBAL.alloc(layout) };
            assert!(!pointer.is_null(), "producer alloc returned null");
            // SAFETY: the block is COUNT * BLOCK bytes, well above a u64.
            unsafe { std::ptr::write(pointer as *mut u64, index as u64 + 1) };
            if tx.send((pointer as usize, index as u64 + 1)).is_err() {
                break;
            }
        }
    });
    let blocks: Vec<(usize, u64)> = rx.iter().collect();
    producer.join().expect("producer exited normally");
    assert_eq!(blocks.len(), COUNT);

    // Snapshot AFTER all reservations exist: only our retirements may move
    // the release counter from here on.
    let before_frees = GLOBAL.stats();

    // 3. Consumer B: frees the FIRST half while slot A is FREE. B's first
    //    allocator call is a dealloc, so it binds nothing and cannot steal
    //    the recycled slot before C.
    let first_half: Vec<(usize, u64)> = blocks[..HALF].to_vec();
    std::thread::spawn(move || {
        for (address, tag) in first_half {
            let pointer = core::ptr::without_provenance_mut::<u8>(address);
            // SAFETY: the tagged prefix belongs to this live allocation.
            let read_back = unsafe { std::ptr::read(pointer as *const u64) };
            assert_eq!(
                read_back, tag,
                "tag corruption on pending block {address:#x} (reuse while pending)"
            );
            // SAFETY: allocated by A with `layout`, handed over by value.
            unsafe { GLOBAL.dealloc(pointer, layout) };
        }
    })
    .join()
    .expect("half-1 consumer exited normally");

    // 4. Thread C: first allocation re-claims slot A via the hint (its
    //    re-claim trim consumes the half-1 pending publications exactly once),
    //    then frees the second half under its live lease and trims.
    let second_half: Vec<(usize, u64)> = blocks[HALF..].to_vec();
    std::thread::spawn(move || {
        let allocator = SeferAlloc::new();
        let own = unsafe { allocator.alloc(Layout::from_size_align(64, 8).expect("valid")) };
        assert!(!own.is_null(), "claimer alloc returned null");
        for (address, tag) in second_half {
            let pointer = core::ptr::without_provenance_mut::<u8>(address);
            // SAFETY: the tagged prefix belongs to this live allocation.
            let read_back = unsafe { std::ptr::read(pointer as *const u64) };
            assert_eq!(
                read_back, tag,
                "tag corruption on leased block {address:#x} (double accounting)"
            );
            // SAFETY: allocated by A with `layout`, handed over by value.
            unsafe { allocator.dealloc(pointer, layout) };
        }
        allocator.trim_current_thread();
        // SAFETY: C's own warm block, original layout.
        unsafe { allocator.dealloc(own, Layout::from_size_align(64, 8).expect("valid")) };
    })
    .join()
    .expect("claimer thread exited normally");

    drop(blocker_tx);
    drop(blocker_hold_tx);
    blocker.join().expect("blocker exited normally");

    let after = GLOBAL.stats();
    assert_eq!(
        after.foreign_or_unroutable_frees, before_frees.foreign_or_unroutable_frees,
        "every cross-thread free must route; none may be dropped as unroutable"
    );
    assert_eq!(
        after.segments_released_total,
        before_frees.segments_released_total + COUNT as u64,
        "each of the {COUNT} Large blocks must be retired EXACTLY once: \
         a lost publication under-releases, a double consume over-releases"
    );
    assert!(
        after.segments_reserved_total >= stats_start.segments_reserved_total,
        "reservation total is monotonic"
    );
}
