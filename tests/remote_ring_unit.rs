//! **Isolated unit test of [`RemoteFreeRing`] — task #36, step 2.**
//!
//! This test does NOT use `#[global_allocator]` and does NOT touch the
//! allocator at all. It constructs a `RemoteFreeRing` over a plain
//! heap-allocated aligned byte buffer (`Box<[u8]>` of `FOOTPRINT` bytes) and
//! drives the MPSC protocol directly: many producers push UNIQUE offsets
//! (each producer owns a disjoint offset range), one consumer drains in a
//! loop.
//!
//! ## What it proves
//!
//! The ring is correct *as a data structure*, in isolation from:
//!   - the allocator (no segment, no BinTable, no reclaim_offset),
//!   - the §8 intrusive-word race (no block bytes are touched),
//!   - ABA-by-address (offsets are never recycled within a run).
//!
//! If this test is GREEN, the ring itself is sound; any double-reclaim under
//! the real allocator therefore traces to the RECLAIM PATH (the allocator
//! handing the same block out twice) or an ABA across block lifetimes — NOT to
//! the ring re-emitting an offset it already drained.
//!
//! ## Invariants checked
//!
//! 1. Every offset that a producer pushed with `Ok(())` is reclaimed **exactly
//!    once** (a per-offset counter asserts `== 1`; loss → 0, double → 2).
//! 2. The `overflow` counter equals the number of `Err(PushOverflow)` returns
//!    (overflow accounting is exact).
//! 3. `reclaimed_count + overflow_count == total_push_attempts` (no offset is
//!    lost to the ring; the only loss channel is the documented overflow).
//!
//! ## Non-vacuousness
//!
//! The counterfactual is the `drain_broken_*` test in `loom_remote_ring.rs`
//! (the loom model catches a buggy drain). Here, the assertion
//! `reclaimed == pushed - overflowed` is what would FAIL if the ring had a
//! wrap bug (`while h < t`), a missing clear, or a missing break on an
//! unpublished slot — those bugs either lose an offset (reclaimed too few) or
//! re-emit one (reclaimed too many). The wrap-around path is exercised by
//! pushing more than `RING_CAP` offsets total (the consumer drains
//! concurrently, so the ring wraps multiple times).

#![cfg(all(feature = "alloc-core", feature = "alloc-xthread"))]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use sefer_alloc::alloc_core::remote_free_ring::{RemoteFreeRing, FOOTPRINT};

/// A `Send` + `Sync` wrapper for a `RemoteFreeRing` view. The view is a thin
/// pointer into a shared buffer (an `Arc<Box<[u8]>>` whose storage outlives all
/// threads); every field access goes through `Node::atomic_u32_at`, so
/// concurrent access is race-free atomics. The wrapper lets the view cross the
/// thread boundary — `RemoteFreeRing` itself is `!Send` (it holds a raw
/// pointer) and the crate is `#![deny(unsafe_code)]`, so the `unsafe impl`
/// lives here in the test (which is a separate crate, not under the deny).
///
/// SAFETY: the underlying buffer is shared and accessed only through atomic
/// `&AtomicU32` refs from the node seam; there is no mutable shared state and
/// no data race. The buffer outlives the view (held by the main thread's
/// `Arc`, dropped only after all threads join).
struct SendRing(RemoteFreeRing);
unsafe impl Send for SendRing {}
unsafe impl Sync for SendRing {}

// A simple bounded fail-fast watchdog (see task #36 step 3): a watcher thread
// aborts the process after `DEADLINE_SECS` so a deadlock fails fast instead of
// hanging the suite. It is started per-test and joined (cancelled) on success.
const DEADLINE_SECS: u64 = 20;

struct Watchdog;
impl Watchdog {
    /// Start a watcher that aborts the process after `DEADLINE_SECS`. Returns
    /// a guard whose Drop joins the watcher (cancelling the abort) — the
    /// process is allowed to continue. The watcher prints a diagnostic before
    /// aborting so the failure reason is obvious.
    fn start(label: &'static str) -> WatchdogHandle {
        let done = Arc::new(AtomicBool::new(false));
        let done_w = Arc::clone(&done);
        let handle = std::thread::Builder::new()
            .name(format!("watchdog-{label}"))
            .spawn(move || {
                let start = std::time::Instant::now();
                while start.elapsed().as_secs() < DEADLINE_SECS {
                    if done_w.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                eprintln!(
                    "\n[watchdog-{label}] TEST EXCEEDED {DEADLINE_SECS}s — likely deadlock. \
                     Aborting process to fail fast (task #36 watchdog)."
                );
                std::process::abort();
            })
            .expect("spawn watchdog");
        WatchdogHandle {
            done,
            handle: Some(handle),
        }
    }
}
struct WatchdogHandle {
    done: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl Drop for WatchdogHandle {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Allocate a `FOOTPRINT`-byte aligned buffer for a ring, zeroed, and return
/// its base pointer. The `Box<[u8]>` owns the storage; the caller keeps it
/// alive for the test duration. `alloc::vec!` goes through the System
/// allocator (this test does NOT install sefer as global), so there is no
/// reentrancy concern.
fn ring_buffer() -> Box<[u8]> {
    // The ring slot/cursor accesses are 4-byte aligned (each field is a u32).
    // A `Vec<u8>` allocation is at least `align_of::<u32>()`-aligned for sizes
    // >= 4 (the System allocator aligns to at least word size); verify it to
    // be safe.
    let mut buf: Vec<u8> = vec![0u8; FOOTPRINT];
    assert!(
        (buf.as_mut_ptr() as usize).is_multiple_of(core::mem::align_of::<u32>()),
        "ring buffer must be 4-byte aligned"
    );
    buf.into_boxed_slice()
}

/// N producers push UNIQUE offsets (disjoint ranges), 1 consumer drains in a
/// loop. Every Ok-push must be reclaimed exactly once; overflow accounting is
/// exact; reclaimed + overflowed == attempted.
#[test]
fn ring_isolated_mpsc_no_loss_no_dup() {
    let _wd = Watchdog::start("mpsc");

    const PRODUCERS: usize = 4;
    // Push more than RING_CAP * PRODUCERS total so the ring wraps several
    // times (the consumer drains concurrently) — exercises the wrap path that
    // `while h < t` would have broken.
    const OFFSETS_PER_PRODUCER: usize = 2_000;
    // Offsets are synthetic but must be valid (a real offset is < SEGMENT;
    // use small multiples of 16, the MIN_BLOCK, well under the sentinel).
    const OFFSET_STRIDE: u32 = 16;

    let buf = Arc::new(ring_buffer());
    // SAFETY: `buf` lives for the whole test (held in `Arc` by both threads);
    // its base points to FOOTPRINT writable, 4-byte-aligned bytes. The ring
    // only touches bytes within `[base, base+FOOTPRINT)`.
    let base = buf.as_ptr() as *mut u8;
    RemoteFreeRing::init_test_buffer(base);
    // One shared view, wrapped `Send`+`Sync` so it can be shared across threads
    // via `Arc`. All field access is race-free atomics via the node seam.
    let ring = Arc::new(SendRing(RemoteFreeRing::over_test_buffer(base)));

    // Attempt counter (every push call, Ok or Err).
    let attempted = Arc::new(AtomicU64::new(0));
    // Succeeded (Ok) counter.
    let succeeded = Arc::new(AtomicU64::new(0));

    // The consumer records reclaimed offsets in a Mutex<HashMap>. The mutex is
    // held ONLY across the bookkeeping inside the consumer thread (never
    // across a push/drain that another thread waits on) — no lock-order
    // hazard with the allocator (there is no allocator here).
    let reclaimed_map = Arc::new(Mutex::new(std::collections::HashMap::<u32, u32>::new()));
    let reclaimed_count = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));

    // --- Consumer (1 thread): drain in a loop until producers are done AND
    //     the ring is empty. ---
    let reclaimed_map_c = Arc::clone(&reclaimed_map);
    let reclaimed_count_c = Arc::clone(&reclaimed_count);
    let stop_c = Arc::clone(&stop);
    let ring_c = Arc::clone(&ring);
    let consumer = std::thread::Builder::new()
        .name("ring-consumer".into())
        .spawn(move || {
            loop {
                let mut local = 0u64;
                ring_c.0.drain(|off| {
                    let mut m = reclaimed_map_c.lock().expect("reclaim map poisoned");
                    let e = m.entry(off).or_insert(0);
                    *e += 1;
                    local += 1;
                });
                reclaimed_count_c.fetch_add(local, Ordering::Relaxed);
                if stop_c.load(Ordering::Acquire) {
                    // Final drain: the producers set stop after their last push;
                    // one more drain captures any offset published between the
                    // last drain's tail load and the stop store.
                    let mut local2 = 0u64;
                    ring_c.0.drain(|off| {
                        let mut m = reclaimed_map_c.lock().expect("reclaim map poisoned");
                        let e = m.entry(off).or_insert(0);
                        *e += 1;
                        local2 += 1;
                    });
                    reclaimed_count_c.fetch_add(local2, Ordering::Relaxed);
                    return;
                }
                // Brief yield so producers make progress; the ring is the
                // synchronisation primitive, not sleep timing.
                std::thread::yield_now();
            }
        })
        .expect("spawn consumer");

    // --- Producers (N threads): each pushes a disjoint range of unique
    //     offsets. ---
    let mut producers = Vec::with_capacity(PRODUCERS);
    for p in 0..PRODUCERS {
        let attempted = Arc::clone(&attempted);
        let succeeded = Arc::clone(&succeeded);
        // SAFETY: the shared `SendRing` view is accessed concurrently by the
        // producers and the consumer, but every access is through a
        // `&AtomicU32` from `Node::atomic_u32_at` (the node seam), so the
        // accesses are race-free atomics. The buffer outlives all producers
        // (held by the main thread's `Arc`).
        let ring_p = Arc::clone(&ring);
        producers.push(
            std::thread::Builder::new()
                .name(format!("ring-producer-{p}"))
                .spawn(move || {
                    // Each producer's offsets are in its own disjoint band:
                    // producer p owns [p*BAND, (p+1)*BAND). No two producers push
                    // the same offset → a double-reclaim can ONLY come from the
                    // ring re-emitting, not from two producers pushing the same.
                    let band_base = (p as u32) * (OFFSETS_PER_PRODUCER as u32) * OFFSET_STRIDE;
                    for i in 0..OFFSETS_PER_PRODUCER {
                        let off = band_base + (i as u32) * OFFSET_STRIDE;
                        assert_ne!(off, u32::MAX, "offset must not equal the sentinel");
                        attempted.fetch_add(1, Ordering::Relaxed);
                        match ring_p.0.push(off) {
                            Ok(()) => {
                                succeeded.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(_) => {
                                // Overflow: the consumer will not reclaim this
                                // offset (it was discarded). Counted via the ring's
                                // overflow counter.
                            }
                        }
                    }
                })
                .expect("spawn producer"),
        );
    }

    for h in producers {
        h.join().expect("producer must not abort");
    }
    // Signal the consumer to do a final drain and exit.
    stop.store(true, Ordering::Release);
    consumer.join().expect("consumer must not abort");

    // --- Verify the three invariants. ---
    let attempted = attempted.load(Ordering::Acquire);
    let succeeded = succeeded.load(Ordering::Acquire);
    let reclaimed = reclaimed_count.load(Ordering::Acquire);
    let overflow = ring.0.overflow_count() as u64;

    let map = reclaimed_map.lock().expect("reclaim map poisoned");
    // 1. Every reclaimed offset reclaimed EXACTLY once.
    let mut doubles = 0u64;
    let mut distinct = 0u64;
    for (_off, &cnt) in map.iter() {
        if cnt > 1 {
            doubles += 1;
        }
        distinct += 1;
    }
    assert_eq!(
        doubles, 0,
        "RING DOUBLE-RECLAIM: {doubles} offsets reclaimed >1 time — the ring \
         re-emitted an offset it already drained (the drain wrap/clear bug, \
         or ABA). This is the precise symptom task #36 disentangles."
    );
    // reclaimed_count counts reclaims (with doubles); distinct counts unique
    // offsets. They must be equal (no doubles).
    assert_eq!(
        reclaimed, distinct,
        "reclaimed ({reclaimed}) != distinct offsets ({distinct}) — a double \
         occurred despite the doubles-count being zero (bookkeeping bug)"
    );

    // 2. overflow counter == succeeded-but-discarded? The overflow counter is
    //    bumped on every Err(PushOverflow). reclaimed must equal succeeded
    //    (every Ok push is eventually drained, since the consumer runs a final
    //    drain after producers finish). overflow must equal attempted -
    //    succeeded.
    assert_eq!(
        succeeded, reclaimed,
        "RING LOSS: {succeeded} pushes succeeded but only {reclaimed} were \
         reclaimed — an offset was pushed Ok (reserved + published) but never \
         drained (the drain stop-on-unpublished break, or the wrap bug, lost it)"
    );
    let overflow_expected = attempted - succeeded;
    assert_eq!(
        overflow, overflow_expected,
        "RING OVERFLOW MISMATCH: overflow counter = {overflow}, but \
         attempted({attempted}) - succeeded({succeeded}) = {overflow_expected}"
    );

    // 3. The master identity: reclaimed + overflow == attempted.
    assert_eq!(
        reclaimed + overflow,
        attempted,
        "RING IDENTITY BROKEN: reclaimed({reclaimed}) + overflow({overflow}) \
         != attempted({attempted}) — an offset vanished into the ring (neither \
         reclaimed nor overflowed)"
    );

    eprintln!(
        "ring_isolated_mpsc: attempted={attempted} succeeded={succeeded} \
         reclaimed={reclaimed} overflow={overflow} distinct={distinct}"
    );
}

/// Single-threaded smoke: push N (<= RING_CAP) then drain once → all N
/// reclaimed in order, overflow == 0. Confirms the happy path and that the
/// `h != t` fix did not regress the non-wrap case.
#[test]
fn ring_isolated_single_thread_basic() {
    let _wd = Watchdog::start("basic");
    let buf = ring_buffer();
    let base = buf.as_ptr() as *mut u8;
    RemoteFreeRing::init_test_buffer(base);
    let ring = RemoteFreeRing::over_test_buffer(base);

    const N: u32 = 64;
    for i in 0..N {
        let off = i * 16;
        let r = ring.push(off);
        assert!(r.is_ok(), "push of {off} failed inside RING_CAP");
    }
    assert_eq!(
        ring.overflow_count(),
        0,
        "no overflow expected below RING_CAP"
    );

    let mut reclaimed = Vec::new();
    ring.drain(|off| reclaimed.push(off));

    assert_eq!(reclaimed.len(), N as usize, "drained all N");
    // Order is FIFO by reservation index — offsets come out in push order.
    for (i, &off) in reclaimed.iter().enumerate() {
        assert_eq!(off, (i as u32) * 16, "FIFO order broken at {i}");
    }

    // A second drain of a quiescent ring reclaims nothing.
    let mut second = 0;
    ring.drain(|_| second += 1);
    assert_eq!(second, 0, "second drain of quiescent ring must be empty");
}
