//! Installed-Sefer Miri witness for the B4 magazine depot (DP-3/DP-4).
//!
//! Covers the one new `unsafe` (the `&'static mut Depot` bound out of the
//! slot's `UnsafeCell` at materialisation) and the depot paths on real
//! `Box` drops: park on overflow while the dropping frame is live, reload on
//! a magazine miss, drain on thread exit (slot recycle), and reuse of the
//! retained binding by the next thread that claims the recycled slot.
//! Path-activation oracle: the depot must hold parked magazines before each
//! reload / exit. One scenario per process; no libtest is linked.

use sefer_alloc::global::tls_heap;
use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Class 0 (16 B): `FREE_PARK_CAP == TCACHE_CAP == 16`, `DEPOT_K == 8`.
const MAG: usize = 16;
const ROUNDS_TO_FILL: usize = 8;

fn parked_class0() -> usize {
    match tls_heap::current_for_alloc() {
        // SAFETY: this thread's own bound heap, read on this thread only.
        tls_heap::CurrentHeap::Own(h) => unsafe { (*h).dbg_depot_len(0) },
        tls_heap::CurrentHeap::Fallback => 0,
    }
}

/// Drops enough 16 B boxes to park `ROUNDS_TO_FILL` full magazines, then
/// re-allocates them (each miss reloads a parked magazine).
fn cycle(tag: u8) {
    let n = MAG * (ROUNDS_TO_FILL + 1) + 1;
    let mut boxes: Vec<Box<[u8; 16]>> = Vec::with_capacity(n);
    for i in 0..n {
        boxes.push(Box::new([tag ^ i as u8; 16]));
    }
    while let Some(b) = boxes.pop() {
        drop(b);
    }
    assert!(parked_class0() > 0, "depot park must have executed");
    for i in 0..n {
        boxes.push(Box::new([tag.wrapping_add(i as u8); 16]));
    }
    for (i, b) in boxes.iter().enumerate() {
        assert_eq!(b[0], tag.wrapping_add(i as u8));
    }
    // Leave parked magazines behind for the thread-exit drain.
    while let Some(b) = boxes.pop() {
        drop(b);
    }
    assert!(parked_class0() > 0, "depot holds magazines before exit");
}

fn main() {
    cycle(1);
    // Thread exit drains its depot; the next thread may claim the recycled
    // slot and reuse its retained binding.
    std::thread::spawn(|| cycle(2)).join().unwrap();
    std::thread::spawn(|| cycle(3)).join().unwrap();
    println!("[miri_depot_cycle] COMPLETE depot_cycle");
}
