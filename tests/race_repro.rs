//! Cross-thread-free reclaim GATE (task #33/#36) — now GREEN.
//!
//! Originally a diagnostic repro of the cross-thread-free drain-reclaim crash;
//! it is now the regression gate for the Phase-12.6 fix. It exercises the
//! installed `#[global_allocator]` under producer/consumer cross-thread free +
//! slot recycle and asserts no corruption (tag checksums + no UAF). It was RED
//! (non-deterministic `STATUS_ACCESS_VIOLATION` / subtract-overflow) before the
//! fix and is GREEN after — see `RACE_DRAIN_RECLAIM.md` §13 (root: `page_map`
//! class unreliable for mixed-class pages) / §14 (fix: carry the class through
//! the ring). The historical hypothesis text below is kept for context.
//!
//! ## The hypothesis under test (§2 of RACE_DRAIN_RECLAIM.md)
//!
//! A block's intrusive first word is contended between:
//!   - a cross-thread freer C (pushes block X to a slot's TFS, writing
//!     X.next = old TFS head), and
//!   - the slot's current owner B (drained X, popped X from the BinTable,
//!     handed X to the app, which writes user data into X.first),
//!
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
//!
//! ## Crashes under heavy system load — most likely the test's own watchdog (R18-1, task #329)
//!
//! Two independent `STATUS_STACK_BUFFER_OVERRUN` (0xC0000409) crashes in
//! `drain_reclaim_uaf_repro_tight_handoff` were observed during full-suite
//! (`cargo test --release --features production`) runs under heavy concurrent
//! CPU load in this shared dev workspace — one during Round 14 (task #289,
//! see `docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` §4) and one during
//! Round 17 (task #326). Both reran clean in isolation (3/3 and 20/20
//! respectively); a dedicated Round-17 follow-up additionally ran 80 process
//! invocations under three deliberately harsher load profiles (CPU busy-loop
//! stressors, a real concurrent `cargo check --all-features`, and 4-way
//! parallel full-binary runs) with zero reproductions. This file is unchanged
//! since its original Phase-12.6 fix commit (`ea3a4ba`, June 2026) across both
//! incidents, ruling out a code regression.
//!
//! **Most likely explanation:** the test's OWN watchdog firing after its
//! `DEADLINE_SECS` budget under heavy load, not allocator corruption. This
//! file historically contained a watchdog (see `Watchdog` below) that, after
//! a fixed 20 s deadline, called `std::process::abort()`. On Windows/MSVC,
//! `std::process::abort()` is implemented via `__fastfail`, and the resulting
//! exception carries the code **literally `STATUS_STACK_BUFFER_OVERRUN`
//! (0xC0000409)** — byte-for-byte indistinguishable on the crash surface from
//! a genuine stack-corruption crash. Both observed crashes happened precisely
//! under the conditions (severe multi-process CPU contention) in which a
//! normally-fast stress test can legitimately overrun a 20 s budget — exactly
//! when this watchdog would fire. This is a far stronger fit than the prior
//! "rare Windows scheduler/stack-guard artifact" framing, and it is why three
//! independent reviews (oh / r17-readonly / crush, 2026-07-25) all flagged it.
//!
//! **Status of the hypothesis — OBSERVED vs INFERRED:**
//! - OBSERVED (fact in this file): the watchdog, the 20 s `DEADLINE_SECS`,
//!   and the historical `std::process::abort()` call.
//! - INFERRED (from documented Rust/Windows platform behaviour, NOT
//!   confirmed by a run in this repo): the mapping `abort()` → `__fastfail`
//!   → exception code `0xC0000409` on this MSVC toolchain.
//! - Supporting but not conclusive: the checksum oracle below (a non-vacuous
//!   corruption signal — a lost/corrupted/double-freed box changes the
//!   checksum) stayed green throughout every one of the ~100 cumulative
//!   reproduction attempts. That is hard to reconcile with a corruption bug
//!   severe enough to crash the process, but does not formally prove the
//!   watchdog theory.
//! - Refutation test NOT performed: the watchdog historically `eprintln!`ed
//!   a "TEST EXCEEDED ... Aborting process" line BEFORE `abort()`; grepping
//!   the original crash stderr for that line would have strengthened
//!   (present) or weakened (absent) the hypothesis. No preserved stderr
//!   dump of either original crash exists in the repo, and under heavy load
//!   a line-buffered stderr may not flush before `__fastfail` anyway — so
//!   even a preserved dump's silence would not fully refute it.
//!
//! **The fix landed in R18-1 (task #329):** the watchdog no longer calls
//! `abort()`. It now prints elapsed time + in-flight progress and exits with
//! code `124` (the conventional `timeout(1)` exit code), which is distinct
//! from any abort/SIGABRT/`__fastfail` signal a genuine memory-corruption
//! crash would produce. If this test ever overruns its budget again, the
//! resulting process exit will be unambiguously identifiable as a watchdog
//! timeout rather than masquerading as corruption. `DEADLINE_SECS` is also
//! now overridable via `RACE_REPRO_DEADLINE_SECS` for overloaded runners.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::alloc::GlobalAlloc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use sefer_alloc::SeferAlloc;

// Install sefer-alloc as the process-wide global allocator for this binary.
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Serialise against the other registry-touching tests (the registry is a
// process-global static; reset_for_test in sibling tests would interfere).
static SERIAL: AtomicBool = AtomicBool::new(false);

// A bounded fail-fast watchdog (task #36 step 3, R18-1/task #329): a watcher
// thread terminates the process after `DEADLINE_SECS` so a deadlock or runaway
// loop fails fast instead of hanging the suite. Started per-test and joined
// (cancelled) on success — the process is allowed to continue. The watcher
// prints a diagnostic (elapsed time + in-flight progress captured by the
// caller-supplied `progress` closure) before terminating.
//
// R18-1/task #329: the terminator is `std::process::exit(124)`, NOT
// `std::process::abort()`. On Windows/MSVC `abort()` is implemented via
// `__fastfail`, whose exception code is literally `STATUS_STACK_BUFFER_OVERRUN`
// (0xC0000409) — byte-for-byte indistinguishable on the crash surface from a
// genuine stack-corruption crash, which is almost certainly why two prior
// "unexplained" `STATUS_STACK_BUFFER_OVERRUN` crashes under heavy load (Round
// 14 / task #289, Round 17 / task #326) were widely misread as possible
// allocator corruption rather than as this very watchdog firing after its
// 20 s budget. `exit(124)` is the conventional timeout exit code (matching
// GNU `timeout(1)`), distinct from any abort/SIGABRT/`__fastfail` signal, so a
// future watchdog firing can never be confused with memory corruption again.
//
// `DEADLINE_SECS` is overridable via the `RACE_REPRO_DEADLINE_SECS` env var so
// an overloaded runner can raise the budget without editing code.
const DEFAULT_DEADLINE_SECS: u64 = 20;

fn deadline_secs() -> u64 {
    std::env::var("RACE_REPRO_DEADLINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_DEADLINE_SECS)
}

struct Watchdog {
    done: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl Watchdog {
    fn start(label: &'static str, progress: impl Fn() -> String + Send + 'static) -> Self {
        let deadline = deadline_secs();
        let done = Arc::new(AtomicBool::new(false));
        let done_w = Arc::clone(&done);
        let handle = std::thread::Builder::new()
            .name(format!("watchdog-{label}"))
            .spawn(move || {
                let start = std::time::Instant::now();
                while start.elapsed().as_secs() < deadline {
                    if done_w.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                let elapsed = start.elapsed().as_secs_f32();
                let prog = progress();
                eprintln!(
                    "\n[watchdog-{label}] TEST EXCEEDED DEADLINE: {deadline}s \
                     (elapsed {elapsed:.1}s) — likely deadlock or runaway loop in \
                     drain-reclaim. Progress at deadline: {prog}. \
                     Exiting with code 124 (conventional timeout code, deliberately \
                     distinct from abort/__fastfail/STATUS_STACK_BUFFER_OVERRUN so a \
                     watchdog firing cannot be confused with allocator corruption)."
                );
                // exit(124), NOT abort(): see the block comment above. On
                // Windows/MSVC abort() → __fastfail → exception code
                // STATUS_STACK_BUFFER_OVERRUN (0xC0000409), identical to a
                // real stack-corruption crash. 124 is the conventional
                // timeout(1) code, distinct from any corruption signal.
                std::process::exit(124);
            })
            .expect("spawn watchdog");
        Watchdog {
            done,
            handle: Some(handle),
        }
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            // R18-1/task #329: never silently swallow a panicked watcher —
            // it is diagnostically meaningful in its own right. The test
            // itself completed (else `done` wouldn't be set and we wouldn't
            // be dropping normally), but the watchdog thread panicked on
            // the way out; recover the payload if it's a string.
            if let Err(err) = h.join() {
                let msg: String = err
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| err.downcast_ref::<&'static str>().map(|s| (*s).to_string()))
                    .unwrap_or_else(|| format!("{err:?}"));
                eprintln!(
                    "[watchdog] watcher thread did not exit cleanly: the test itself \
                     completed, but the watchdog panicked on the way out. Payload: {msg}"
                );
            }
        }
    }
}

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
    let cur_wave = Arc::new(AtomicU64::new(0));
    // Snapshot of in-flight progress, printed only if the deadline fires.
    let _wd = Watchdog::start("tight-handoff", {
        let sent = Arc::clone(&total_sent);
        let recv = Arc::clone(&total_recv);
        let wave = Arc::clone(&cur_wave);
        move || {
            format!(
                "wave {}/{} sent={} recv={}",
                wave.load(Ordering::Relaxed),
                WAVES,
                sent.load(Ordering::Relaxed),
                recv.load(Ordering::Relaxed),
            )
        }
    });

    for wave in 0..WAVES {
        cur_wave.store(wave as u64, Ordering::Relaxed);
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
                        let val = worker_id.wrapping_mul(1_000_003).wrapping_add(i as u64);
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

    let cur_wave = Arc::new(AtomicU64::new(0));
    // recv stays 0 until the consumer finalizes at end-of-test, so wave +
    // sent are the live progress signal here.
    let _wd = Watchdog::start("long-lived-consumer", {
        let sent = Arc::clone(&total_sent);
        let recv = Arc::clone(&total_recv);
        let wave = Arc::clone(&cur_wave);
        move || {
            format!(
                "wave {}/{} sent={} recv={}",
                wave.load(Ordering::Relaxed),
                WAVES,
                sent.load(Ordering::Relaxed),
                recv.load(Ordering::Relaxed),
            )
        }
    });

    for wave in 0..WAVES {
        cur_wave.store(wave as u64, Ordering::Relaxed);
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

/// Direct-API variant (NOT installed as global_allocator): drives SeferAlloc
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
    // registry state. SeferAlloc is zero-sized; the static is just a vtable
    // anchor for the `GlobalAlloc` calls.
    static DIRECT: SeferAlloc = SeferAlloc::new();
    let layout = std::alloc::Layout::from_size_align(SIZE, 8).unwrap();
    let total_sent = Arc::new(AtomicU64::new(0));
    let total_recv = Arc::new(AtomicU64::new(0));
    let cur_wave = Arc::new(AtomicU64::new(0));
    // Snapshot of in-flight progress, printed only if the deadline fires.
    let _wd = Watchdog::start("direct-api", {
        let sent = Arc::clone(&total_sent);
        let recv = Arc::clone(&total_recv);
        let wave = Arc::clone(&cur_wave);
        move || {
            format!(
                "wave {}/{} sent={} recv={}",
                wave.load(Ordering::Relaxed),
                WAVES,
                sent.load(Ordering::Relaxed),
                recv.load(Ordering::Relaxed),
            )
        }
    });

    for wave in 0..WAVES {
        cur_wave.store(wave as u64, Ordering::Relaxed);
        // Wrap the raw pointer so it can cross the thread boundary via the
        // channel. SAFETY of the Send impl: the pointer is a freshly-allocated
        // block from SeferAlloc; ownership is transferred to exactly one
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
                        // SAFETY: SeferAlloc implements GlobalAlloc; layout is valid.
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
