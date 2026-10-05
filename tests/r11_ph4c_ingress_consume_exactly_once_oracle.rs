//! Ph4c (receipt 2026-10-05-ph4b, open question M-C): the re-claim-time
//! `trim_for_recycle` consumes the FREE-slot sidecar ingress EXACTLY ONCE.
//!
//! Background: `trim_for_recycle` is idempotent, so the Ph4b oracle
//! (`segments_released_total == +COUNT`) cannot distinguish "0 trims" from
//! "exactly 1 trim" (M-C stayed green on the removed-trim mutant) and says
//! nothing about an ingress step run twice in one trim. This test closes
//! that gap with the `bench-internals` numeric observers
//! `SIDECAR_INGRESS_DRAIN_CALLS` / `SIDECAR_INGRESS_RECORDS_CONSUMED`
//! (surfaced via `SeferAlloc::dbg_sidecar_ingress_stats`):
//!   - `drain_calls` delta across the re-claiming thread's FIRST alloc (the
//!     re-claim trim) must be EXACTLY 1 — a mutant that runs the ingress
//!     step twice inside one trim yields 2 (red), a suppressed trim yields 0
//!     (red);
//!   - `records_consumed` delta must be EXACTLY the pending publication
//!     count (16) — "no trim ran" (0) and over-consumption (>16) are red.
//!
//! Scenario (real `#[global_allocator]`, native threads — same choreography
//! as `r11_ph4b_late_publication_across_recycle_exactly_once`):
//!   1. A blocker thread holds its slot, pinning every FREE slot but A's.
//!   2. Producer A allocates 32 × 2 MiB Large blocks, hands them over, exits
//!      → its slot is recycled (hint = A).
//!   3. Thread B frees HALF the blocks while the slot is FREE → 16 pending
//!      terminal publications on A's sidecar (no worker consumes them).
//!   4. Thread C's first allocation re-claims slot A via the hint; the
//!      re-claim `trim_for_recycle` runs the ingress pass — measured here.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::{mpsc, Mutex};

/// Both oracles share the process-wide allocator and its global numeric
/// counters; the harness runs them in parallel, so serialize them to keep
/// each test's counter window exclusive.
static ORACLE_LOCK: Mutex<()> = Mutex::new(());

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
fn reclaim_trim_consumes_ingress_exactly_once() {
    let _guard = ORACLE_LOCK.lock().expect("oracle mutex");

    // Bounded pre-drain: the sibling oracle on this ORACLE_LOCK may leave
    // residual pending publications on the sidecar of a previously used
    // (hinted) slot; without this they land inside the exactly-once window
    // below and inflate `records_delta` by +1 (flake 2026-10-05 ph6a: 17
    // instead of 16). Warm a fresh thread, trim, and repeat until the
    // consumed-records delta settles at 0 — the window is then exclusive.
    for attempt in 0..64 {
        let records_before = GLOBAL.dbg_sidecar_ingress_stats().1;
        std::thread::spawn(|| {
            let allocator = SeferAlloc::new();
            // SAFETY: valid non-zero layout.
            let warm = unsafe { allocator.alloc(Layout::from_size_align(64, 8).expect("valid")) };
            assert!(!warm.is_null(), "pre-drain warm alloc returned null");
            allocator.trim_current_thread();
            // SAFETY: the warm block, original layout.
            unsafe { allocator.dealloc(warm, Layout::from_size_align(64, 8).expect("valid")) };
        })
        .join()
        .expect("pre-drain thread exited normally");
        let records_delta = GLOBAL.dbg_sidecar_ingress_stats().1 - records_before;
        if records_delta == 0 {
            break;
        }
        if attempt == 63 {
            panic!(
                "pre-drain: residual sidecar ingress publications never settled \
                 (still consuming after 64 trim rounds) — exactly-once window \
                 cannot be made exclusive"
            );
        }
    }

    let layout = layout();

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

    // 3. Consumer B: frees the FIRST half while slot A is FREE (B binds
    //    nothing: its first allocator call is a dealloc).
    let first_half: Vec<(usize, u64)> = blocks[..HALF].to_vec();
    std::thread::spawn(move || {
        for (address, tag) in first_half {
            let pointer = core::ptr::without_provenance_mut::<u8>(address);
            // SAFETY: the tagged prefix belongs to this live allocation.
            let read_back = unsafe { std::ptr::read(pointer as *const u64) };
            assert_eq!(read_back, tag, "tag corruption on pending block");
            // SAFETY: allocated by A with `layout`, handed over by value.
            unsafe { GLOBAL.dealloc(pointer, layout) };
        }
    })
    .join()
    .expect("half-1 consumer exited normally");

    // 4. Thread C: snapshot the ingress observers, then let its FIRST
    //    allocation re-claim slot A (the re-claim trim is the measured
    //    window: exactly one full drain pass, exactly HALF records).
    let second_half: Vec<(usize, u64)> = blocks[HALF..].to_vec();
    let (drain_calls_delta, records_delta) = std::thread::spawn(move || {
        let allocator = SeferAlloc::new();
        let (calls_before, records_before) = GLOBAL.dbg_sidecar_ingress_stats();
        let own = unsafe { allocator.alloc(Layout::from_size_align(64, 8).expect("valid")) };
        assert!(!own.is_null(), "claimer alloc returned null");
        let (calls_after, records_after) = GLOBAL.dbg_sidecar_ingress_stats();

        // C continues under its own live lease: free the second half (local,
        // no sidecar publications) and trim — the full trim drains whatever
        // remains (nothing pending), which must NOT add a second consume of
        // half-1's already-consumed records.
        for (address, tag) in second_half {
            let pointer = core::ptr::without_provenance_mut::<u8>(address);
            // SAFETY: the tagged prefix belongs to this live allocation.
            let read_back = unsafe { std::ptr::read(pointer as *const u64) };
            assert_eq!(read_back, tag, "tag corruption on leased block");
            // SAFETY: allocated by A with `layout`, handed over by value.
            unsafe { allocator.dealloc(pointer, layout) };
        }
        allocator.trim_current_thread();
        // SAFETY: C's own warm block, original layout.
        unsafe { allocator.dealloc(own, Layout::from_size_align(64, 8).expect("valid")) };
        (calls_after - calls_before, records_after - records_before)
    })
    .join()
    .expect("claimer thread exited normally");

    drop(blocker_tx);
    drop(blocker_hold_tx);
    blocker.join().expect("blocker exited normally");

    // Oracle 1 (the M-C catcher): the re-claim trim runs the full ingress
    // drain EXACTLY ONCE. 0 = suppressed trim (M-C removal), 2+ = ingress
    // consumed twice in one trim — both mutants must go red here.
    assert_eq!(
        drain_calls_delta, 1,
        "the re-claim trim must run the full sidecar ingress drain EXACTLY \
         once (0 = trim suppressed, 2+ = double consume ingress in one trim)"
    );
    // Oracle 2: that single pass consumed EXACTLY the pending publications.
    assert_eq!(
        records_delta, HALF as u64,
        "the re-claim ingress pass must consume exactly the {HALF} pending \
         publications (0 = trim suppressed, >{HALF} = record double-consumed)"
    );
}

/// Mutant catcher #12 (Ph4c): the INGRESS step of `maintenance_pass` —
/// `lease.with_core(... background_maintenance_step ...)` — must run EXACTLY
/// ONCE per maintained slot.
///
/// Why not the `drain_calls` oracle of the test above: the bounded step calls
/// `drain_sidecar_ingress_bounded`, which (by design) touches NEITHER
/// `SIDECAR_INGRESS_DRAIN_CALLS` NOR `SIDECAR_INGRESS_RECORDS_CONSUMED` —
/// those count only the full trim-path drain. The bounded drain is also
/// cursor-idempotent: a second step can never re-consume a record (Large
/// routes are claimed once, Small words advance the cursor), so retirement
/// counters cannot distinguish one step from two either. The only observable
/// difference between "one `with_core`" and "two" on this path is the NUMBER
/// of bounded steps — hence the dedicated `bench-internals` observer
/// `SeferAlloc::dbg_background_ingress_step_calls`.
///
/// Oracle: for every `maintenance_pass_for_test(&mut cursor, 1)` pass,
/// `Δdbg_background_ingress_step_calls() == returned maintained count`
/// (budget 1 → at most one maintained slot → exactly one step). A mutant
/// that runs `with_core` twice per maintained slot yields 2 and goes red.
/// A suppressed step yields 0 and goes red.
#[test]
fn maintenance_pass_ingress_step_runs_exactly_once_per_maintained_slot() {
    use sefer_alloc::registry::HeapRegistry;

    let _guard = ORACLE_LOCK.lock().expect("oracle mutex");
    let layout = layout();

    // 1. Blocker: binds a slot and holds it (contended — must be skipped).
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

    // 2. Producer A: allocate + tag N Large blocks, hand over, exit → its
    //    slot is FREE (hint = A).
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

    // 3. Consumer B frees the FIRST half while slot A is FREE → HALF pending
    //    terminal publications on A's sidecar, nothing consumes them.
    let first_half: Vec<(usize, u64)> = blocks[..HALF].to_vec();
    std::thread::spawn(move || {
        for (address, tag) in first_half {
            let pointer = core::ptr::without_provenance_mut::<u8>(address);
            // SAFETY: the tagged prefix belongs to this live allocation.
            let read_back = unsafe { std::ptr::read(pointer as *const u64) };
            assert_eq!(read_back, tag, "tag corruption on pending block");
            // SAFETY: allocated by A with `layout`, handed over by value.
            unsafe { GLOBAL.dealloc(pointer, layout) };
        }
    })
    .join()
    .expect("half-1 consumer exited normally");

    // 4. Round-robin maintenance passes of budget 1. Every pass must run
    //    EXACTLY as many bounded ingress steps as it maintained slots (≤ 1);
    //    the pending publications must each be retired exactly once in
    //    total across the sweep.
    let mut cursor = 0usize;
    let mut maintained_visits = 0usize;
    let mut retired_total = 0u64;
    let mut retired_seen = GLOBAL.stats().large_xthread_reclaimed;
    for _ in 0..(8 * COUNT) {
        let steps_before = GLOBAL.dbg_background_ingress_step_calls();
        let maintained = HeapRegistry::maintenance_pass_for_test(&mut cursor, 1);
        let steps_delta = GLOBAL.dbg_background_ingress_step_calls() - steps_before;

        assert_eq!(
            steps_delta, maintained as u64,
            "one maintenance_pass with budget {maintained} must run EXACTLY \
             {maintained} bounded ingress step(s); 0 = suppressed step, 2+ = \
             double with_core / double ingress step per maintained slot"
        );
        let retired_now = GLOBAL.stats().large_xthread_reclaimed - retired_seen;
        retired_seen += retired_now;
        retired_total += retired_now;
        maintained_visits += usize::from(maintained > 0);
        if retired_total >= HALF as u64 {
            break;
        }
    }

    drop(blocker_tx);
    drop(blocker_hold_tx);
    blocker.join().expect("blocker exited normally");

    assert!(
        maintained_visits > 0,
        "the maintenance sweep never reached slot A — scenario broken"
    );
    // The pending publications were consumed (each EXACTLY once — bounded by
    // cursor/CAS idempotence; the re-claim trim may also retire routes of the
    // never-freed half, so only a lower bound is pinned here).
    assert!(
        retired_total >= HALF as u64,
        "the maintenance sweep consumed none of the {HALF} pending publications \
         — scenario broken"
    );
}
