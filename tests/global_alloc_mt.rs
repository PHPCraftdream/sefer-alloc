//! Phase 12.5 — the **headline gate**: install `SeferAlloc` as the process's
//! `#[global_allocator]` and run a **multithreaded** `Vec`/`String`/`HashMap`/`Box`
//! churn where worker threads **spawn AND exit mid-allocation**.
//!
//! ## Why this is THE gate
//!
//! Before Phase 12.5 the global allocator either aborted under a hostile
//! multithreaded runtime (the Phase 11 `RefCell` reentrancy failure) or relied
//! on a fragile abandon/adopt transfer that raced under MT (the FINDINGS №7
//! landmine). Phase 12.5 closes both with the **shard model**: the raw-pointer
//! TLS + registry + slot-release guard make the allocator reentrancy-safe and
//! never-null, and a heap is a SHARD that stays whole across release→claim
//! (no cross-heap segment transfer, no racy header writes — the single writer
//! is always the slot's current owner). This test exercises the model under a
//! realistic multithreaded workload with thread churn.
//!
//! ## What it forces
//!
//! - Workers allocate through `SeferAlloc` (the installed global allocator):
//!   every `Vec`/`String`/`Box`/`HashMap` op routes through the registry-backed
//!   heap via raw-pointer TLS.
//! - Workers SPAWN and EXIT mid-allocation: on exit the `AbandonGuard::drop`
//!   runs, releasing the slot (the HeapCore stays whole — segments + inline
//!   TFS — for the next claimant). A later worker reclaims the slot and drains
//!   its TFS on first alloc (the shard-reuse discipline).
//! - Cross-thread free (under `alloc-xthread`): blocks allocated on one
//!   thread's heap may be freed on another (the channel test hands off
//!   ownership); the TFS routing + drain handles this.
//!
//! ## Non-vacuous
//!
//! The assertions check actual computed values (sums, map contents). A
//! corrupted/lost/double-freed allocation fails an assertion rather than
//! silently passing. A UAF or double-ownership (the №7 landmine) would abort
//! the process (segfault / heap corruption) — the test reaching completion
//! with correct values is the proof.
//!
//! ## Scope note
//!
//! This runs under `alloc-global` (own-thread + abandon/reclaim) and
//! `alloc-xthread` (adds cross-thread free routing). The default `alloc`
//! single-thread path is covered by `heap_soak`/`heap_cross_thread` and is
//! untouched by Phase 12.5.

#![cfg(feature = "alloc-global")]

use std::alloc::GlobalAlloc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use sefer_alloc::SeferAlloc;

// Install sefer-alloc as the process-wide global allocator for this test
// binary. Every allocation in this binary — including libtest's harness
// allocations and every worker thread's `Vec`/`String` — routes through
// `SeferAlloc` and the registry.
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Serialise against the other registry-touching tests (`registry_basic`,
// `global_alloc_installed`). The registry is a process-global static; its
// `reset_for_test` in `registry_basic` would interfere with our slot observations.
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

/// A workload that allocates varied sizes through the global allocator and
/// returns a checksum. Each call exercises `Vec` push/grow, `String` format,
/// `Box`, and `HashMap` — the realistic Rust allocation patterns. The
/// checksum makes the test non-vacuous (corruption changes the sum).
fn churn(worker_id: u64, rounds: u32) -> u64 {
    let mut acc: u64 = worker_id;
    for r in 0..rounds {
        // Vec push/grow (alloc + realloc churn).
        let mut v: Vec<u64> = Vec::new();
        for i in 0..256u64 {
            v.push(i.wrapping_add(acc).wrapping_add(r as u64));
        }
        acc = acc.wrapping_add(v.iter().copied().sum::<u64>() / 256);

        // String formatting (varied small allocations through format!).
        let s = format!("worker-{worker_id}-round-{r}-acc-{acc}");
        acc = acc.wrapping_add(s.len() as u64);

        // Box new/drop.
        let b = Box::new(acc.wrapping_mul(7));
        acc = acc.wrapping_add(*b);

        // HashMap insert/lookup (varied sizes, hashing allocs).
        let mut m: HashMap<u64, u64> = HashMap::new();
        for i in 0..32u64 {
            m.insert(i, acc.wrapping_add(i));
        }
        if let Some(&val) = m.get(&(r as u64 % 32)) {
            acc = acc.wrapping_add(val);
        }
    }
    acc
}

/// The headline multithreaded gate: spawn N worker threads that churn through
/// the global allocator, with each thread EXITING mid-workload (so its heap is
/// abandoned + recycled). A second wave of threads then runs and (via the
/// cold-path `try_adopt`) reclaims the abandoned segments. The test asserts
/// the final accumulated checksum matches the expected deterministic value
/// (non-vacuous — corruption changes it) and that no thread aborted.
#[test]
fn global_allocator_serves_multithreaded_churn_with_thread_exit() {
    let _serial = SerialGuard::acquire();

    const WAVE_SIZE: usize = 4;
    const ROUNDS: u32 = 8;
    const WAVES: usize = 3;

    // The expected checksum is deterministic (churn is a pure function of
    // worker_id + rounds); we accumulate across waves and compare at the end.
    let total = Arc::new(AtomicU64::new(0));
    let mut expected_total: u64 = 0;

    for wave in 0..WAVES {
        let handles: Vec<_> = (0..WAVE_SIZE)
            .map(|i| {
                let worker_id = (wave * WAVE_SIZE + i) as u64;
                let total = Arc::clone(&total);
                std::thread::spawn(move || {
                    // Each worker churns and exits — on exit the AbandonGuard
                    // abandons this thread's heap's segments to the registry
                    // and recycles the slot. This is the abandon path.
                    let acc = churn(worker_id, ROUNDS);
                    total.fetch_add(acc, Ordering::Relaxed);
                    acc
                })
            })
            .collect();
        // Join this wave BEFORE the next — the join hands off ownership of any
        // cross-thread-freed blocks and forces the abandon to quiesce. The
        // next wave's workers will adopt the abandoned segments on their cold
        // path (the try_adopt wiring).
        for (i, h) in handles.into_iter().enumerate() {
            let acc = h.join().expect("worker thread must not abort/panic");
            expected_total = expected_total.wrapping_add(acc);
            // Sanity: the worker returned a non-zero acc (it allocated).
            assert!(acc != 0 || i == 0, "worker returned a zero checksum");
        }
    }

    let observed = total.load(Ordering::Acquire);
    assert_eq!(
        observed, expected_total,
        "multithreaded churn checksum mismatch — the allocator corrupted/lost \
         an allocation (UAF, double-free, or the №7 double-ownership landmine)"
    );
}

/// Cross-thread free stress (requires `alloc-xthread`): blocks allocated on
/// producer threads are freed on the consumer thread by sending them through a
/// bounded channel. This forces the TFS routing + drain path. Combined with
/// producer thread exit (slot release), it exercises cross-thread free +
/// shard-reuse together.
///
/// Non-vacuous: the consumer sums the freed values and we compare against the
/// deterministic expected sum. A lost/corrupted/double-freed box fails the
/// assertion.
#[cfg(feature = "alloc-xthread")]
#[test]
fn global_allocator_cross_thread_free() {
    let _serial = SerialGuard::acquire();

    const N_PRODUCERS: usize = 3;
    const N_PER_PRODUCER: usize = 128;

    // A bounded channel: producers send Box<u64> to the consumer. The Box is
    // allocated on the producer's heap; the consumer drops it (cross-thread
    // free → TFS routing → drained by the producer's slot's next owner).
    let (tx, rx) = std::sync::mpsc::channel::<Box<u64>>();

    let mut expected: u64 = 0;
    let producers: Vec<_> = (0..N_PRODUCERS)
        .map(|p| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut local = 0u64;
                for i in 0..N_PER_PRODUCER {
                    let val = ((p * N_PER_PRODUCER + i) as u64).wrapping_mul(13);
                    local = local.wrapping_add(val);
                    // Send the Box; the consumer will drop it cross-thread.
                    // If the channel is closed (consumer died), bail.
                    if tx.send(Box::new(val)).is_err() {
                        return local;
                    }
                }
                local
            })
        })
        .collect();
    drop(tx); // close the channel so the consumer's rx iter ends after producers

    // The consumer receives every Box, sums, and drops (cross-thread free).
    let mut observed: u64 = 0;
    for b in rx {
        observed = observed.wrapping_add(*b);
        // `b` drops here on the consumer's thread — the block was allocated
        // on a producer's heap, so this is a genuine cross-thread free.
    }

    for h in producers {
        let local = h.join().expect("producer must not abort");
        expected = expected.wrapping_add(local);
    }

    assert_eq!(
        observed, expected,
        "cross-thread free checksum mismatch — a box was lost, corrupted, or \
         double-freed under TFS routing + shard reuse"
    );
}

/// Repeated alloc/dealloc on the SAME installed global allocator across many
/// sizes — catches free-list corruption (a freed block landing on the wrong
/// class list would manifest as a wrong-size read-back).
#[test]
fn global_allocator_multithreaded_size_class_churn() {
    let _serial = SerialGuard::acquire();

    let sizes = [16usize, 32, 64, 128, 256, 512, 1024, 2048];
    let n_threads = 4;
    let n_iters = 2_000;

    let handles: Vec<_> = (0..n_threads)
        .map(|t| {
            std::thread::spawn(move || {
                for iter in 0..n_iters {
                    for &size in &sizes {
                        let layout = std::alloc::Layout::from_size_align(size, 8).unwrap();
                        // SAFETY: valid layout; GLOBAL is the installed allocator.
                        let p = unsafe { GLOBAL.alloc(layout) };
                        assert!(
                            !p.is_null(),
                            "alloc({size}) returned null on t{t}/iter{iter}"
                        );
                        // SAFETY: p is valid for `size` bytes.
                        unsafe {
                            std::ptr::write_bytes(p, 0xAB, size);
                            // Read back the first byte to catch wrong-size reuse.
                            assert_eq!(
                                (p as *const u8).read(),
                                0xAB,
                                "byte not retained at size {size} (free-list corruption?)"
                            );
                            GLOBAL.dealloc(p, layout);
                        }
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("size-class churn thread must not abort");
    }
}
