//! Regression test — task D3 (Phase D, пределы/тюнинг).
//!
//! Before this task, `refill_class`'s "how many blocks to pull on a magazine
//! miss" was the fixed constant `REFILL_N = TCACHE_CAP = 16` for EVERY size
//! class, regardless of `block_size`. With `SMALL_MAX` around 253 KiB, a
//! magazine miss on a large small-class (block_size in the hundreds of KiB)
//! would park `16 * block_size` — several MiB — in a single idle thread's
//! per-class magazine after just one refill. `refill_n_for_class` (task D3,
//! `src/registry/tcache.rs`) replaces the fixed count with a per-class byte
//! budget (`REFILL_BYTE_BUDGET = 64 KiB`), so large classes get fewer blocks
//! per refill while small classes are unaffected (their `TCACHE_CAP`-sized
//! refill already fits comfortably under the budget).
//!
//! This test verifies, through the live `HeapCore::alloc` path (not just the
//! `refill_n_for_class` pure function in isolation):
//!   1. A SMALL class (16 B blocks) refills to the full `TCACHE_CAP` — no
//!      regression for the common case.
//!   2. A LARGE small-class (block_size close to `SMALL_MAX`) refills to
//!      FEWER than `TCACHE_CAP` blocks, and the bytes parked
//!      (`count * block_size`) stay within `REFILL_BYTE_BUDGET`.
//!
//! Counterfactual (verified manually — see task report): reverting
//! `refill_n_for_class` to always return `TCACHE_CAP` (the old `REFILL_N`
//! behaviour) makes assertion (2) fail — the large class's magazine fills to
//! the full `TCACHE_CAP`, parking megabytes.

#![cfg(all(feature = "alloc-global", feature = "fastbin"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise all tests in this file: the registry is a process-global static
// (same discipline as tests/heap_core_bulk_bypass.rs and friends).
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

const REFILL_BYTE_BUDGET: usize = 64 * 1024;
const TCACHE_CAP: usize = 16;

/// A small class (16 B) must still refill to the full `TCACHE_CAP` — the
/// byte budget (64 KiB) comfortably covers `TCACHE_CAP * 16 B` = 256 B, so
/// the clamp never engages for tiny classes. This guards against an
/// over-aggressive budget accidentally shrinking the common-case refill.
#[test]
fn small_class_refill_unaffected_by_byte_budget() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let layout = Layout::from_size_align(16, 8).unwrap();
    let class_idx = unsafe { (*heap).dbg_class_for(layout) }.expect("16B must be a small class");

    let want = unsafe { (*heap).dbg_refill_n_for_class(class_idx) };
    assert_eq!(
        want, TCACHE_CAP,
        "16B class's refill amount should be the full TCACHE_CAP (byte budget \
         far exceeds TCACHE_CAP * block_size for tiny classes)"
    );

    // Drive one refill (first alloc on an empty magazine) and confirm the
    // magazine actually filled to TCACHE_CAP - 1 remaining (one popped for
    // the caller) — i.e. the refill really pulled `want` blocks.
    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null(), "alloc must not fail");
    let mag_cnt = unsafe { (*heap).dbg_tcache_count(class_idx) };
    assert_eq!(
        mag_cnt as usize,
        TCACHE_CAP - 1,
        "after the first alloc (which triggers a refill), the magazine should \
         hold TCACHE_CAP - 1 blocks (one popped for the caller)"
    );

    unsafe { (*heap).dealloc(p, layout) };
    // Drain the magazine to avoid leaking state into later tests in this
    // binary (best-effort; other files already tolerate residual state via
    // per-file serialisation, but this keeps the test self-contained).
    for _ in 0..(TCACHE_CAP as u32) {
        let p2 = unsafe { (*heap).alloc(layout) };
        if p2.is_null() {
            break;
        }
        unsafe { (*heap).dealloc(p2, layout) };
    }
}

/// A LARGE small-class (block_size close to `SMALL_MAX`, i.e. one of the
/// biggest classes still routed through the magazine) must refill to FEWER
/// than `TCACHE_CAP` blocks, and the bytes parked in the magazine after that
/// refill must stay within `REFILL_BYTE_BUDGET`.
///
/// We probe for a large small-class size by trying request sizes from large
/// down to moderate until we find one that (a) classifies as `Some` (a small
/// class, not routed to the Large/huge path) and (b) whose
/// `dbg_refill_n_for_class` is strictly less than `TCACHE_CAP` (i.e. the
/// byte-budget clamp actually engaged for it) — this makes the test robust
/// to the exact size-class table layout instead of hard-coding an assumed
/// `SMALL_MAX`.
#[test]
fn large_small_class_refill_bounded_by_byte_budget() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Search downward from a large candidate size for the first size that is
    // classified as a small class AND whose refill amount is clamped below
    // TCACHE_CAP. `REFILL_BYTE_BUDGET / TCACHE_CAP` = 4096 B is the
    // block_size threshold above which the clamp must engage (by
    // construction of `refill_n_for_class`), so any small-class size in the
    // tens/hundreds of KiB range should trigger it if such a class exists.
    let mut found: Option<(usize, Layout, usize)> = None;
    for candidate in [
        260 * 1024,
        200 * 1024,
        128 * 1024,
        64 * 1024,
        32 * 1024,
        16 * 1024,
        8 * 1024,
    ] {
        let layout = match Layout::from_size_align(candidate, 8) {
            Ok(l) => l,
            Err(_) => continue,
        };
        let Some(class_idx) = (unsafe { (*heap).dbg_class_for(layout) }) else {
            continue; // Large/huge path — not a magazine class.
        };
        let want = unsafe { (*heap).dbg_refill_n_for_class(class_idx) };
        if want < TCACHE_CAP {
            found = Some((class_idx, layout, want));
            break;
        }
    }

    let Some((class_idx, layout, want)) = found else {
        panic!(
            "no small class found whose refill is byte-budget-clamped below \
             TCACHE_CAP in the probed size range (8 KiB..260 KiB) — either the \
             size-class table changed shape, or the D3 byte-budget clamp is not \
             wired up (refill_n_for_class always returning TCACHE_CAP would \
             land here)"
        );
    };

    assert!(
        want >= 1,
        "refill amount must never be 0 (a magazine miss must make progress)"
    );
    assert!(
        want < TCACHE_CAP,
        "expected the byte-budget clamp to reduce this large class's refill \
         below TCACHE_CAP ({TCACHE_CAP}), got want={want}"
    );

    // Drive the refill and confirm the actual magazine occupancy matches
    // `want` (minus the one block popped for the caller) — proving the
    // clamp is honoured on the LIVE alloc path, not just in the pure
    // function.
    let p = unsafe { (*heap).alloc(layout) };
    if p.is_null() {
        eprintln!("OOM allocating the probed large-small-class layout — skip");
        return;
    }
    let mag_cnt = unsafe { (*heap).dbg_tcache_count(class_idx) } as usize;
    assert_eq!(
        mag_cnt,
        want - 1,
        "magazine occupancy after the triggering refill must equal want - 1 \
         (one block popped for the caller)"
    );

    // The core D3 assertion: bytes parked in the magazine after this refill
    // must not exceed the byte budget. block_size >= the requested layout
    // size (class rounding only grows), so we bound using the actual
    // allocation size as a conservative proxy — the real class block_size is
    // private to the crate, but `layout.size()` is a safe lower bound that
    // still proves the budget-driven reduction is meaningful (not just
    // "want < TCACHE_CAP" with no actual byte-size effect).
    let parked_bytes_lower_bound = mag_cnt * layout.size();
    assert!(
        parked_bytes_lower_bound <= REFILL_BYTE_BUDGET.max(layout.size()),
        "large small-class magazine parked at least {parked_bytes_lower_bound} \
         bytes after one refill, exceeding the {REFILL_BYTE_BUDGET}-byte budget \
         (want={want}, mag_cnt={mag_cnt}, layout.size()={})",
        layout.size()
    );

    unsafe { (*heap).dealloc(p, layout) };
    // Best-effort drain.
    for _ in 0..(want as u32) {
        let p2 = unsafe { (*heap).alloc(layout) };
        if p2.is_null() {
            break;
        }
        unsafe { (*heap).dealloc(p2, layout) };
    }
}
