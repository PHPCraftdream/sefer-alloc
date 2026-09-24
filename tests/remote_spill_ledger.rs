//! Deterministic ledger for the third-tier intrusive remote-free spill.

#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    not(miri)
))]

use std::alloc::{GlobalAlloc, Layout};
use std::thread;

use sefer_alloc::registry::{HeapRegistry, DBG_RING_PUSH_RETRY_EXHAUSTED};
use sefer_alloc::SeferAlloc;
use std::sync::atomic::Ordering;

const N: usize = 5_000;
const BLOCK: usize = 16;
const SEGMENT: usize = 4 * 1024 * 1024;

fn ledger(owner_exited: bool) {
    let layout = Layout::from_size_align(BLOCK, 8).unwrap();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    // SAFETY: this thread exclusively owns the claimed heap.
    let anchor = unsafe { (*heap).alloc(layout) };
    assert!(!anchor.is_null());
    let mut blocks = Vec::with_capacity(N);
    for _ in 0..N {
        // SAFETY: the same claim remains exclusively owned here.
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null());
        assert_eq!(p.addr() % BLOCK, 0, "smallest-class block alignment");
        assert_eq!(p.addr() & !(SEGMENT - 1), anchor.addr() & !(SEGMENT - 1));
        blocks.push(p);
    }
    // SAFETY: the claim and anchor remain live.
    let initial_live = unsafe { (*heap).dbg_live_count_for(anchor) }.unwrap();
    // SAFETY: this thread is the heap's owner.
    let spill_before = unsafe { (*heap).dbg_spill_ledger_for_test() };
    // SAFETY: this thread is the heap's owner.
    let cursors_before = unsafe { (*heap).dbg_overflow_cursors_for_test() };
    let lost_before = DBG_RING_PUSH_RETRY_EXHAUSTED.load(Ordering::Relaxed);

    if owner_exited {
        // Recycling retains the whole heap, its segments, and pending notes.
        // SAFETY: this thread owns the live claim and stops accessing it
        // until it reclaims the same slot below.
        unsafe { HeapRegistry::recycle(heap) };
    }

    // This thread has no TLS-bound heap. The GlobalAlloc dealloc-only path
    // routes directly to the original owner's slot without claiming it.
    let addresses: Vec<usize> = blocks.iter().map(|p| p.addr()).collect();
    thread::spawn(move || {
        let alloc = SeferAlloc::new();
        for addr in addresses {
            // SAFETY: each address is one distinct live allocation with its
            // original Layout, transferred exclusively to this thread.
            unsafe { alloc.dealloc(addr as *mut u8, layout) };
        }
    })
    .join()
    .unwrap();

    let heap = if owner_exited {
        let claimed = HeapRegistry::claim();
        assert_eq!(claimed, heap, "the exited owner's slot must be reclaimed");
        claimed
    } else {
        heap
    };
    // SAFETY: this thread owns the claimed heap after the producer joined.
    let spill_pending = unsafe { (*heap).dbg_spill_pending_for_test() };
    assert!(
        spill_pending,
        "the burst must pass both bounded rings before the owner drains"
    );
    // SAFETY: this thread owns the claimed heap.
    let spill_after_push = unsafe { (*heap).dbg_spill_ledger_for_test() };
    assert!(spill_after_push.0 > spill_before.0);
    assert_eq!(spill_after_push.1, spill_before.1);
    // SAFETY: this thread owns the claimed heap.
    let cursors_after_push = unsafe { (*heap).dbg_overflow_cursors_for_test() };
    assert!(cursors_after_push.1 > cursors_before.1);
    assert_eq!(
        DBG_RING_PUSH_RETRY_EXHAUSTED.load(Ordering::Relaxed),
        lost_before,
        "a legal remote free was discarded"
    );
    // SAFETY: the anchor keeps its segment live and this claim is exclusive.
    let live_after_push = unsafe { (*heap).dbg_live_count_for(anchor) };
    assert_eq!(
        live_after_push,
        Some(initial_live),
        "remote publication must not modify owner-only live_count"
    );

    // Owner-side forced drains run the production reclaim primitives.
    // SAFETY: only this thread owns/drains the claimed heap.
    unsafe {
        (*heap).dbg_drain_all_rings();
        (*heap).dbg_drain_heap_overflow_for_test();
    }
    // SAFETY: this thread still owns the claimed heap.
    let spill_pending = unsafe { (*heap).dbg_spill_pending_for_test() };
    assert!(
        spill_pending,
        "one budgeted drain must leave a nonempty residual for the next pass"
    );
    // SAFETY: this thread still owns the claimed heap.
    unsafe { (*heap).dbg_drain_heap_overflow_for_test() };
    // SAFETY: this thread still owns the claimed heap.
    let spill_pending = unsafe { (*heap).dbg_spill_pending_for_test() };
    // SAFETY: this thread still owns the claimed heap.
    let spill_after_drain = unsafe { (*heap).dbg_spill_ledger_for_test() };
    assert!(
        !spill_pending,
        "spill ledger after drain: {spill_after_drain:?}"
    );
    assert_eq!(
        spill_after_drain.1 - spill_before.1,
        spill_after_push.0 - spill_before.0,
        "each published spill note must be popped exactly once"
    );
    // SAFETY: this thread still owns the claimed heap.
    let (head, tail) = unsafe { (*heap).dbg_overflow_cursors_for_test() };
    assert_eq!(head, tail, "spill drain must not roll back the ring cursor");
    // SAFETY: the anchor remains live, so its segment is still mapped.
    let live_after_drain = unsafe { (*heap).dbg_live_count_for(anchor) };
    assert_eq!(
        live_after_drain,
        Some(initial_live - N as u32),
        "every remote note must decrement live_count exactly once"
    );
    for (i, &p) in blocks.iter().enumerate() {
        // SAFETY: the anchor keeps this segment mapped; only its owner reads.
        let is_free = unsafe { (*heap).dbg_is_free_for(p) };
        assert!(is_free, "remote block {i} was not reclaimed");
    }

    // SAFETY: this thread still owns the claim and anchor allocation.
    unsafe { (*heap).dealloc(anchor, layout) };
    // SAFETY: the claim has not been recycled since it was obtained.
    unsafe { HeapRegistry::recycle(heap) };
}

#[test]
fn paused_and_exited_owner_remote_frees_are_lossless() {
    ledger(false);
    ledger(true);
}
