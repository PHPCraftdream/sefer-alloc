//! R11-4 zero-trust review follow-up — `HeapCore::dealloc_batch`'s batched
//! fast path (`dealloc_batch_small`, `src/registry/heap_core_dealloc_batch.rs`)
//! must honour the SAME two `hardened` guards the scalar
//! `dealloc_own_thread_with_base` path applies BEFORE the M2 oracles:
//!
//! - **F7** (task #25): a pointer that actually lives in a LARGE segment,
//!   freed via a Small-classified `layout` (a `GlobalAlloc`-contract
//!   violation on the caller's side) — must be a no-op, not corruption of
//!   the Large block's payload.
//! - **H1** (task #167): an INTERIOR pointer (not a block start) — must be a
//!   no-op, not a silent aliasing double-issue.
//!
//! This matters specifically for `dealloc_batch` because its ownership gate
//! is `AllocCore::contains_base`, which does NOT distinguish Small vs. Large
//! segments (both are "this heap's registered segments") — so unlike a
//! genuinely foreign pointer (rejected by the ownership gate itself), these
//! two cases pass the ownership gate and must be caught by F7/H1 specifically
//! inside the batched fast path, mirroring
//! `tests/regression_hardened_large_kind_own_free.rs` and
//! `tests/regression_hardened_interior_ptr.rs`'s scalar-path scenarios but
//! routed through `dealloc_batch`, and interleaved with legitimate
//! this-heap-owned Small blocks in the SAME call to prove the guard degrades
//! that one entry to a no-op without aborting or corrupting the rest of the
//! batch.
//!
//! Gated to `hardened` (which pulls `fastbin`): only that build compiles
//! either guard. Also requires `batch-api` directly (R14 hotfix, task #299):
//! `hardened` does NOT pull in `batch-api`, and this file's whole point is
//! exercising `HeapCore::dealloc_batch` — without the extra gate,
//! `cargo test --features hardened` alone (CI's "test (hardened tier)" job)
//! fails to compile (E0599: `dealloc_batch` does not exist without
//! `batch-api`). See `.github/workflows/ci.yml`'s "test (hardened tier)" job
//! for the companion `--features "hardened batch-api"` step this gate needs.

#![cfg(all(
    feature = "hardened",
    feature = "alloc-global",
    feature = "fastbin",
    feature = "batch-api"
))]

use std::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::SegmentLayout;

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

/// F7 through `dealloc_batch`: a batch mixing legitimate owned Small blocks
/// with ONE pointer that actually lives in a LARGE segment, all freed via a
/// SMALL-classified `layout` (same 64 B choice as
/// `regression_hardened_large_kind_own_free.rs`, deliberately isolating F7
/// from H1 — see that test's comment for why 64 B is page-multiple-safe).
/// The Large entry must be a no-op (Large payload untouched, still freeable
/// normally afterward); every owned Small entry must still be freed
/// correctly.
#[test]
fn dealloc_batch_large_via_small_layout_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    const LARGE_SIZE: usize = 2 * 1024 * 1024;
    let large_layout = Layout::from_size_align(LARGE_SIZE, 8).unwrap();
    let small_layout = Layout::from_size_align(64, 8).unwrap();

    // SAFETY: valid layout; `heap` is the calling thread's own slot.
    let large = unsafe { (*heap).alloc(large_layout) };
    assert!(!large.is_null(), "large alloc returned null");
    // Fill the payload so a wrongly-run oracle would visibly corrupt it.
    // SAFETY: `large` is a live LARGE_SIZE-byte allocation.
    unsafe { std::ptr::write_bytes(large, 0xCC, LARGE_SIZE) };

    // Legitimate owned Small blocks, interleaved around the hazardous entry.
    let owned_n = 20usize;
    let mut owned: Vec<*mut u8> = Vec::with_capacity(owned_n);
    for _ in 0..owned_n {
        // SAFETY: valid layout; `heap` is the calling thread's own slot.
        let p = unsafe { (*heap).alloc(small_layout) };
        assert!(!p.is_null(), "owned alloc returned null");
        owned.push(p);
    }

    let mut batch: Vec<*mut u8> = Vec::with_capacity(owned_n + 1);
    batch.push(owned[0]);
    batch.push(large); // hazardous: Large pointer, Small-classified layout
    batch.extend_from_slice(&owned[1..]);

    // SAFETY: every `owned[i]` is a live Small allocation of `small_layout`
    // made by `heap`, freed at most once. `large` is a live allocation but
    // NOT of `small_layout` (a deliberate GlobalAlloc-contract violation on
    // the caller side) — the F7 guard's documented job is to degrade this to
    // a safe no-op rather than corruption, exercised here through the
    // batched entry point specifically.
    unsafe { (*heap).dealloc_batch(small_layout, &batch) };

    // The Large payload must be untouched.
    // SAFETY: `large` is still a live allocation (the free above must have
    // been a no-op); reading its first byte is in-bounds.
    unsafe {
        assert_eq!(
            large.read(),
            0xCC,
            "Large payload mutated by the mismatched small-layout dealloc_batch entry"
        );
    }

    // Cold-storm of small allocations: none may alias any byte of the live
    // Large block (would happen if the Large address were pushed into the
    // small magazine / bitmap by mistake).
    const N: usize = 8192;
    let large_lo = large as usize;
    let large_hi = large_lo + LARGE_SIZE;
    let mut issued: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        // SAFETY: valid layout.
        let p = unsafe { (*heap).alloc(small_layout) };
        assert!(!p.is_null(), "cold-storm small alloc returned null");
        let a = p as usize;
        assert!(
            !(large_lo..large_hi).contains(&a),
            "LARGE-KIND GUARD BROKEN (via dealloc_batch): a small alloc was \
             handed an address inside the live Large block"
        );
        issued.push(p);
    }
    let distinct: HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        issued.len(),
        "DUPLICATE small pointer during the cold-storm"
    );

    // The Large block is still ours to free legitimately.
    // SAFETY: `large` is still live and owned by `heap`; freed once here.
    unsafe {
        assert_eq!(
            large.read(),
            0xCC,
            "Large payload corrupted by the cold-storm"
        );
        (*heap).dealloc(large, large_layout);
    }
    // SAFETY: every entry of `issued` was allocated above with
    // `small_layout`; freed exactly once here.
    for &p in &issued {
        unsafe { (*heap).dealloc(p, small_layout) };
    }

    // SAFETY: `heap` was claimed above; recycled whole here.
    unsafe { HeapRegistry::recycle(heap) };
}

/// H1 through `dealloc_batch`: a batch mixing legitimate owned Small blocks
/// with ONE interior pointer (16 B into a 48 B-class block — same
/// construction as `regression_hardened_interior_ptr.rs`). The interior
/// entry must be a no-op (never re-issued, never aliases a real block);
/// every owned entry (including the interior pointer's own anchor block,
/// which is NOT in this batch and stays live) must still be handled
/// correctly.
#[test]
fn dealloc_batch_interior_pointer_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());

    // 48 B request → block_size 48 (non-power-of-two; a 16 B-aligned
    // interior offset exists but is not a whole multiple of 48).
    let layout = Layout::from_size_align(48, 8).unwrap();

    // SAFETY: valid layout; `heap` is the calling thread's own slot.
    let anchor = unsafe { (*heap).alloc(layout) };
    assert!(!anchor.is_null());
    let base = (anchor as usize) & !(SEGMENT - 1);
    let off = (anchor as usize) - base;
    assert!(off + 16 < off + 48);
    let interior = (anchor as usize + 16) as *mut u8;
    assert_ne!(interior, anchor);

    // Legitimate owned Small blocks (a DIFFERENT anchor set, so the real
    // `anchor` block stays live and unaffected by this batch).
    let owned_n = 20usize;
    let mut owned: Vec<*mut u8> = Vec::with_capacity(owned_n);
    for _ in 0..owned_n {
        // SAFETY: valid layout.
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "owned alloc returned null");
        owned.push(p);
    }

    let mut batch: Vec<*mut u8> = Vec::with_capacity(owned_n + 1);
    batch.push(owned[0]);
    batch.push(interior); // hazardous: interior pointer, not a block start
    batch.extend_from_slice(&owned[1..]);

    // SAFETY: every `owned[i]` is a live allocation of `layout` made by
    // `heap`, freed at most once. `interior` is NOT the start pointer of any
    // allocation (a deliberate contract violation) — the H1 guard's
    // documented job is to degrade this to a safe no-op, exercised here
    // through the batched entry point specifically.
    unsafe { (*heap).dealloc_batch(layout, &batch) };

    // (i) The next alloc of this class must NOT hand back the interior
    // pointer.
    // SAFETY: valid layout.
    let after = unsafe { (*heap).alloc(layout) };
    assert!(!after.is_null());
    assert_ne!(
        after, interior,
        "INTERIOR-PTR GUARD BROKEN (via dealloc_batch): pushed into the \
         magazine and re-issued by the next alloc."
    );

    // (ii) Cold-storm distinctness: an interior pointer that slipped into
    // circulation aliases the tail of the anchor block → a duplicate.
    const N: usize = 4096;
    let mut issued: Vec<*mut u8> = Vec::with_capacity(N + 2);
    issued.push(anchor);
    issued.push(after);
    for _ in 0..N {
        // SAFETY: valid layout.
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "cold-storm alloc returned null");
        issued.push(p);
    }
    let distinct: HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        issued.len(),
        "DUPLICATE POINTER (via dealloc_batch): an interior pointer aliased \
         a real block — silent double-issue (interior-ptr guard missing)."
    );
    assert!(
        !distinct.contains(&(interior as usize)),
        "interior pointer was issued during the cold-storm"
    );

    // SAFETY: every entry of `issued` was allocated above with `layout`;
    // freed exactly once here. Every entry of `owned` was already freed via
    // the `dealloc_batch` call above (all of `owned` was included in
    // `batch`), so nothing further to free for it.
    for &p in &issued {
        unsafe { (*heap).dealloc(p, layout) };
    }

    // SAFETY: `heap` was claimed above; recycled whole here.
    unsafe { HeapRegistry::recycle(heap) };
}
