//! DIAGNOSTIC repro for the cross-thread-free drain-reclaim UAF (task #33).
//!
//! Goal: PROVE the exact interleaving that corrupts when `drain_thread_free`
//! re-injects drained blocks into the BinTable (the naive restore), under the
//! shard-reuse model (slot release → claim reuse).
//!
//! ## The hypothesis under test (§2 of RACE_DRAIN_RECLAIM.md)
//!
//! A block's intrusive first word is contended between:
//!   - a cross-thread freer C (pushes block X to a slot's TFS, writing
//!     X.next = old TFS head), and
//!   - the slot's current owner B (drained X, popped X from the BinTable,
//!     handed X to the app, which writes user data into X.first),
//! across the release→claim boundary (the slot's TFS head address is stable,
//! so a push by C after B died lands on the SAME head the new owner D reads).
//!
//! ## Shape (NO mutex held across alloc/free — that deadlocked the prior
//! attempt)
//!
//! A pool of short-lived PRODUCER threads: each allocates a handful of
//! `Box<u64>`, hands them to a long-lived CONSUMER via an unbounded channel,
//! and EXITS immediately (releasing its registry slot). The consumer frees
//! every box it receives (cross-thread free → the producer's slot's TFS).
//! Because producers exit fast and new producers spawn to reuse the released
//! slots, the new owner of a recycled slot drains a TFS that contains blocks
//! pushed by the consumer AFTER the previous owner died — the exact
//! handoff window.
//!
//! Bounded: producers send a fixed total number of boxes, then everyone
//! drains and joins. No per-iter spawn/join inside the hot loop (spawn is
//! per-wave, not per-box).
//!
//! ## Gating
//!
//! `alloc-global,alloc-xthread`. The naive restore in `heap_core.rs` must be
//! in place (this test is meaningless under the shipped discard).

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::alloc::GlobalAlloc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use sefer_alloc::SeferMalloc;

// Install sefer-alloc as the process-wide global allocator for this binary.
#[global_allocator]
static GLOBAL: SeferMalloc = SeferMalloc::new();

// Serialise against the other registry-touching tests (the registry is a
// process-global static; reset_for_test in sibling tests would interfere).
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

/// The tight 3-thread handoff that maximises the chance of catching the
/// intrusive-word race: a WAVE of producer threads each allocates a small
/// batch of boxes, sends them to the consumer, and EXITS (releasing the
/// slot). The consumer frees them as they arrive (cross-thread free → TFS
/// of a slot whose owner is dying / dead). The next wave's producers reuse
/// the released slots and drain the TFS on their first alloc — the window.
#[test]
fn drain_reclaim_uaf_repro_tight_handoff() {
    let _serial = SerialGuard::acquire();

    const WAVES: usize = 64;
    const PRODUCERS_PER_WAVE: usize = 3;
    const BOXES_PER_PRODUCER: usize = 64;

    let total_sent = Arc::new(AtomicU64::new(0));
    let total_recv = Arc::new(AtomicU64::new(0));

    for wave in 0..WAVES {
        // Unbounded channel: producers never block on send (no lock-order
        // hazard with the allocator — the channel's internal Mutex is NOT
        // held across the producer's alloc, only across the send itself).
        let (tx, rx) = mpsc::channel::<Box<u64>>();

        let producers: Vec<_> = (0..PRODUCERS_PER_WAVE)
            .map(|p| {
                let tx = tx.clone();
                let total_sent = Arc::clone(&total_sent);
                let worker_id = (wave * PRODUCERS_PER_WAVE + p) as u64;
                std::thread::spawn(move || {
                    let mut local_sent: u64 = 0;
                    for i in 0..BOXES_PER_PRODUCER {
                        // Each box is allocated on THIS producer's heap. The
                        // segment header is stamped with this slot's TFS head.
                        let val = worker_id
                            .wrapping_mul(1_000_003)
                            .wrapping_add(i as u64);
                        let b = Box::new(val);
                        local_sent = local_sent.wrapping_add(val);
                        // Send; ignore closed-channel (consumer died).
                        if tx.send(b).is_err() {
                            return local_sent;
                        }
                    }
                    // Producer returns; on thread exit the AbandonGuard drops,
                    // recycling the slot. The HeapCore (segments + inline TFS)
                    // stays whole for the next claimant — late cross-thread
                    // frees from the consumer land on this slot's TFS after
                    // we are gone.
                    total_sent.fetch_add(local_sent, Ordering::Relaxed);
                    local_sent
                })
            })
            .collect();
        drop(tx); // close so the consumer's rx iter ends with the wave

        // The consumer receives every box, sums, and drops it. The drop is a
        // cross-thread free: the box was allocated on a producer's heap, so
        // dealloc_routing reads the segment's stamped owner_thread_free and
        // pushes onto that (producer's) slot's TFS. If the producer has
        // already exited, the push lands on a slot whose owner is in
        // transition (released, about to be reclaimed) — the handoff window.
        let mut wave_recv: u64 = 0;
        for b in rx {
            wave_recv = wave_recv.wrapping_add(*b);
            // `b` drops here — cross-thread free.
        }
        total_recv.fetch_add(wave_recv, Ordering::Relaxed);

        for h in producers {
            let _ = h.join().expect("producer must not abort/panic");
        }
    }

    let sent = total_sent.load(Ordering::Acquire);
    let recv = total_recv.load(Ordering::Acquire);
    // Non-vacuous: a corrupted/double-freed/lost box changes the checksum.
    assert_eq!(
        sent, recv,
        "checksum mismatch: sent={sent} recv={recv} — a box was lost, \
         corrupted, or double-freed under drain-reclaim + shard reuse"
    );
}

/// Variant that keeps the consumer thread ALIVE across waves (its slot is
/// never released), so the producer-side slot churn is the only source of
/// release→claim. This isolates the producer-slot handoff from consumer-slot
/// churn.
#[test]
fn drain_reclaim_uaf_repro_long_lived_consumer() {
    let _serial = SerialGuard::acquire();

    const WAVES: usize = 128;
    const PRODUCERS_PER_WAVE: usize = 2;
    const BOXES_PER_PRODUCER: usize = 32;

    let (tx, rx) = mpsc::channel::<Box<u64>>();
    let total_sent = Arc::new(AtomicU64::new(0));

    // The long-lived consumer: drains the channel across ALL waves, freeing
    // every box (cross-thread free → producer slot's TFS). It stays alive
    // until the main thread drops the final `tx` clone and joins it.
    let total_recv = Arc::new(AtomicU64::new(0));
    let total_recv_consumer = Arc::clone(&total_recv);
    let consumer = std::thread::spawn(move || {
        let mut acc: u64 = 0;
        for b in rx {
            acc = acc.wrapping_add(*b);
            // `b` drops here — cross-thread free.
        }
        total_recv_consumer.store(acc, Ordering::Release);
    });

    for wave in 0..WAVES {
        let producers: Vec<_> = (0..PRODUCERS_PER_WAVE)
            .map(|p| {
                let tx = tx.clone();
                let total_sent = Arc::clone(&total_sent);
                let worker_id = (wave * PRODUCERS_PER_WAVE + p) as u64;
                std::thread::spawn(move || {
                    let mut local_sent: u64 = 0;
                    for i in 0..BOXES_PER_PRODUCER {
                        let val = worker_id
                            .wrapping_mul(9_973)
                            .wrapping_add((i as u64).wrapping_mul(17));
                        let b = Box::new(val);
                        local_sent = local_sent.wrapping_add(val);
                        if tx.send(b).is_err() {
                            return local_sent;
                        }
                    }
                    total_sent.fetch_add(local_sent, Ordering::Relaxed);
                    local_sent
                })
            })
            .collect();
        for h in producers {
            let _ = h.join().expect("producer must not abort");
        }
    }

    drop(tx); // close the channel → consumer's rx iter ends
    consumer.join().expect("consumer must not abort");

    let sent = total_sent.load(Ordering::Acquire);
    let recv = total_recv.load(Ordering::Acquire);
    assert_eq!(
        sent, recv,
        "checksum mismatch: sent={sent} recv={recv} — drain-reclaim corruption"
    );
}

/// Direct-API variant (NOT installed as global_allocator): drives SeferMalloc
/// via its GlobalAlloc trait directly with a tight 2-thread producer/consumer
/// and explicit Layout. This avoids libtest's harness allocations entirely —
/// a cleaner signal if the installed-global variant is noisy. It also lets us
/// hold a single allocator instance and control sizing precisely.
#[test]
fn drain_reclaim_uaf_repro_direct_api() {
    let _serial = SerialGuard::acquire();

    const WAVES: usize = 200;
    const ALLOCS_PER_PRODUCER: usize = 16;
    const SIZE: usize = 32;

    // A dedicated static instance (separate from GLOBAL) so this test drives
    // the API directly without disturbing the installed global allocator's
    // registry state. SeferMalloc is zero-sized; the static is just a vtable
    // anchor for the `GlobalAlloc` calls.
    static DIRECT: SeferMalloc = SeferMalloc::new();
    let layout = std::alloc::Layout::from_size_align(SIZE, 8).unwrap();
    let total_sent = Arc::new(AtomicU64::new(0));
    let total_recv = Arc::new(AtomicU64::new(0));

    for wave in 0..WAVES {
        // Wrap the raw pointer so it can cross the thread boundary via the
        // channel. SAFETY of the Send impl: the pointer is a freshly-allocated
        // block from SeferMalloc; ownership is transferred to exactly one
        // consumer which frees it exactly once (no concurrent access).
        struct SendPtr(*mut u8);
        unsafe impl Send for SendPtr {}
        let (tx, rx) = mpsc::channel::<(SendPtr, u64)>();

        let producers: Vec<_> = (0..2)
            .map(|p| {
                let tx = tx.clone();
                let total_sent = Arc::clone(&total_sent);
                let wid = (wave * 2 + p) as u64;
                std::thread::spawn(move || {
                    let mut local: u64 = 0;
                    for i in 0..ALLOCS_PER_PRODUCER {
                        // SAFETY: SeferMalloc implements GlobalAlloc; layout is valid.
                        let ptr = unsafe { DIRECT.alloc(layout) };
                        assert!(!ptr.is_null(), "alloc returned null");
                        let val = wid.wrapping_mul(31).wrapping_add(i as u64);
                        // SAFETY: ptr is valid for SIZE bytes; write a tag.
                        unsafe { std::ptr::write(ptr as *mut u64, val) };
                        local = local.wrapping_add(val);
                        if tx.send((SendPtr(ptr), val)).is_err() {
                            // SAFETY: reclaim on closed channel.
                            unsafe { DIRECT.dealloc(ptr, layout) };
                            return local;
                        }
                    }
                    total_sent.fetch_add(local, Ordering::Relaxed);
                    local
                })
            })
            .collect();
        drop(tx);

        let mut wave_recv: u64 = 0;
        for (SendPtr(ptr), val) in rx {
            // Verify the tag survives (catches a wrong-block reuse / corruption).
            // SAFETY: ptr was allocated with `layout` and not yet freed.
            let read_back = unsafe { std::ptr::read(ptr as *const u64) };
            assert_eq!(
                read_back, val,
                "tag corruption: wrote {val:#x} read {read_back:#x} — possible \
                 cross-thread-free drain UAF (block reused while in flight)"
            );
            wave_recv = wave_recv.wrapping_add(val);
            // SAFETY: cross-thread free — allocated on a producer's heap.
            unsafe { DIRECT.dealloc(ptr, layout) };
        }
        total_recv.fetch_add(wave_recv, Ordering::Relaxed);

        for h in producers {
            let _ = h.join().expect("producer must not abort");
        }
    }

    let sent = total_sent.load(Ordering::Acquire);
    let recv = total_recv.load(Ordering::Acquire);
    assert_eq!(
        sent, recv,
        "checksum mismatch: sent={sent} recv={recv} — drain-reclaim corruption"
    );
}
