//! H1 (task #167) — the HARDENED interior-pointer free guard.
//!
//! ## The hole this guards
//!
//! A free of an INTERIOR pointer (a pointer into the middle of a live block,
//! not its start) has `off % block_size(class) != 0`. The two own-thread M2
//! oracles in `HeapCore::dealloc_own_thread_with_base` are BLIND to this: the
//! alloc bitmap is indexed at 16 B (`MIN_BLOCK`) granularity, so an interior
//! offset that is still 16 B-aligned maps to a DIFFERENT bit that reads
//! "allocated". The bogus interior pointer therefore falls through the oracles
//! and is pushed into the magazine; a later alloc of that class hands out a
//! mid-block address → silent aliasing / double-issue.
//!
//! The `hardened` feature adds `off % block_size(c) == 0` before the oracles,
//! turning an interior-pointer free into a detected no-op. This is a paid check
//! (a `%` by a non-power-of-two block_size per free), so it is opt-in — NOT on
//! the production hot path.
//!
//! ## Counterfactual (verified RED without the guard)
//!
//! Comment out the `#[cfg(feature = "hardened")]` interior-ptr block in
//! `heap_core.rs::dealloc_own_thread_with_base` and re-run under
//! `--features hardened`: `interior_ptr_free_is_noop` goes RED — the interior
//! pointer is pushed into the magazine and re-issued (or aliases a real block
//! in the cold-storm distinctness check). Restoring the guard → GREEN.
//!
//! Gated to `hardened` (which pulls `fastbin`): only that build compiles the
//! guard, so the test is meaningful only there.

#![cfg(feature = "hardened")]

use std::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::{AllocCore, SegmentLayout};

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

const SEGMENT: usize = SegmentLayout::SEGMENT;

/// Free `anchor + 16` — an interior pointer (offset 16 into a 48 B-class block,
/// still 16 B-aligned so the bitmap-oracle hole applies, but NOT a whole
/// multiple of the 48 B block size). The guard must make it a no-op.
#[test]
fn interior_ptr_free_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());

    // A 48 B request → a class whose block_size is 48 (a non-power-of-two, and
    // > 16 so an interior 16 B-aligned offset exists inside the block).
    let layout = Layout::from_size_align(48, 8).unwrap();

    let anchor = unsafe { (*heap).alloc(layout) };
    assert!(!anchor.is_null());
    let base = (anchor as usize) & !(SEGMENT - 1);

    // Interior pointer: 16 B into the anchor block. 16 is 16 B-aligned (so it
    // hits a valid, "allocated"-reading bitmap bit — the hole), but 16 is not a
    // multiple of the 48 B block size, so a real block start is never here.
    let off = (anchor as usize) - base;
    // Only meaningful if the anchor block is genuinely bigger than 16 B.
    assert!(off + 16 < off + 48);
    let interior = (anchor as usize + 16) as *mut u8;
    assert_ne!(interior, anchor);

    // The hazardous free of the interior pointer — must be a NO-OP.
    unsafe { (*heap).dealloc(interior, layout) };

    // (i) The next alloc of this class must NOT hand back the interior pointer.
    let after = unsafe { (*heap).alloc(layout) };
    assert!(!after.is_null());
    assert_ne!(
        after, interior,
        "INTERIOR-PTR GUARD BROKEN: an interior pointer was pushed into the \
         magazine and re-issued by the next alloc."
    );

    // (ii) Cold-storm: every issued pointer must be globally distinct. An
    // interior pointer that slipped into circulation aliases the tail of the
    // anchor block → a duplicate appears.
    const N: usize = 4096;
    let mut issued: Vec<*mut u8> = Vec::with_capacity(N + 2);
    issued.push(anchor);
    issued.push(after);
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "cold-storm alloc returned null");
        issued.push(p);
    }
    let distinct: HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        issued.len(),
        "DUPLICATE POINTER: an interior pointer aliased a real block — silent \
         double-issue (interior-ptr guard missing)."
    );
    assert!(
        !distinct.contains(&(interior as usize)),
        "interior pointer was issued during the cold-storm"
    );

    for &p in &issued {
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

/// The SUBSTRATE leg of the same guard (`AllocCore::dealloc_small`) — the path
/// any direct `AllocCore` user reaches, which does NOT go through the per-thread
/// magazine. Without the substrate guard an interior-pointer free here slips past
/// the 16 B-granular `is_free` bitmap oracle and is pushed onto the BinTable free
/// list → the next same-class alloc re-issues the mid-block address.
///
/// ## Counterfactual (verified RED without the guard)
///
/// Comment out the `#[cfg(feature = "hardened")]` interior-ptr block in
/// `alloc_core.rs::dealloc_small` and re-run under `--features hardened`:
/// `interior_ptr_free_substrate_is_noop` goes RED (the interior pointer is
/// re-issued / a duplicate appears). Restoring the guard → GREEN.
#[test]
fn interior_ptr_free_substrate_is_noop() {
    let _g = SerialGuard::acquire();
    let mut core = AllocCore::new().expect("primordial reservation");

    // 48 B class: block_size 48 (non-power-of-two, > 16 so a 16 B-aligned
    // interior offset exists but is not a whole multiple of 48).
    let layout = Layout::from_size_align(48, 8).unwrap();

    let anchor = core.alloc(layout);
    assert!(!anchor.is_null());
    let interior = (anchor as usize + 16) as *mut u8;
    assert_ne!(interior, anchor);

    // Hazardous own-thread substrate free of the interior pointer — no-op.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { core.dealloc(interior, layout) };

    // (i) The next alloc must NOT hand back the interior pointer.
    let after = core.alloc(layout);
    assert!(!after.is_null());
    assert_ne!(
        after, interior,
        "SUBSTRATE INTERIOR-PTR GUARD BROKEN: an interior pointer was pushed \
         onto the BinTable free list and re-issued by the next alloc."
    );

    // (ii) Cold-storm distinctness: an interior pointer that slipped onto the
    // free list aliases the tail of the anchor block → a duplicate appears.
    const N: usize = 4096;
    let mut issued: Vec<*mut u8> = Vec::with_capacity(N + 2);
    issued.push(anchor);
    issued.push(after);
    for _ in 0..N {
        let p = core.alloc(layout);
        assert!(!p.is_null(), "cold-storm alloc returned null");
        issued.push(p);
    }
    let distinct: HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        issued.len(),
        "DUPLICATE POINTER: an interior pointer aliased a real block via the \
         substrate free path (interior-ptr guard missing in dealloc_small)."
    );
    assert!(
        !distinct.contains(&(interior as usize)),
        "interior pointer was issued during the cold-storm"
    );

    for &p in &issued {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }
}
