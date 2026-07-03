//! P7.4 (Э10) — branchless chunked in-magazine double-free scan: BOUNDS
//! regression.
//!
//! Э10 rewrites the in-magazine DF oracle in `heap_core.rs`
//! (`dealloc_own_thread_with_base`) from a sequential early-exit
//! `for i in 0..cnt` into a branchless chunked scan: chunks of 4 with an
//! OR-combined equality (one branch per 4) plus a scalar tail of `cnt % 4`.
//!
//! The set membership tested is UNCHANGED — `{ slots[c][i] : i < cnt }` — only
//! the evaluation order differs. The load-bearing invariant this file pins is
//! the BOUND: the scan must compare EXACTLY the `cnt` live entries and NEVER an
//! index `>= cnt`.
//!
//! WHY `>= cnt` is poison: `slots[c]` is a fixed `[*mut u8; TCACHE_CAP]` used as
//! a stack. When a block is popped (alloc), `count[c]` is decremented but the
//! slot value is LEFT IN PLACE — so entries at `i >= cnt` hold STALE pointers of
//! blocks that have since been RE-ISSUED and are now live again. If the scan
//! read such a stale slot and matched, a LEGITIMATE free of that now-live block
//! would be mistaken for an in-magazine double-free and silently swallowed
//! (no-op) → the block is never returned to the magazine → a LEAK / lost block.
//!
//! COUNTERFACTUAL (personally verified RED): change the chunk bound to round
//! `cnt` UP to a multiple of 4 (`let chunks = (cnt + 3) & !3;`) or scan a fixed
//! `TCACHE_CAP`/16 — the scan then reads the stale slot at index `cnt` and
//! swallows the legit free below → this test goes RED (`re-alloc did not return
//! the just-freed block`).

#![cfg(all(feature = "alloc-global", feature = "fastbin"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise all tests: the registry is a process-global static.
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

/// A legit free of a live block whose STALE copy sits at an index `>= cnt` must
/// be ACCEPTED (pushed back into the magazine), not swallowed as a false
/// in-magazine double-free.
///
/// Construction (LIFO magazine, `slots[c]` a fixed array; pop leaves the slot
/// value stale):
///
///   1. `a = alloc`, `b = alloc`  — two distinct live blocks (a magazine miss
///      refills the class, so afterwards several residents sit in `slots`, and
///      `b` is the top popped, `a` the next).
///   2. `free(a)` then `free(b)`  — both pushed; `b` is now the top of the
///      stack at the highest occupied index `k`, `cnt = k + 1`.
///   3. `alloc()` pops the top → returns `b`; `cnt` drops to `k`, but
///      `slots[c][k]` STILL HOLDS `b` (stale). `b` is live again.
///   4. `free(b)` — a GENUINE free. `cnt == k`, so a correct scan compares only
///      `slots[c][0..k]` (which does NOT contain `b`) and pushes `b` back at
///      index `k`. A broken scan that reads index `k` (== `cnt`) sees the stale
///      `b` and swallows the free → `b` is lost.
///   5. `alloc()` must return `b` (LIFO top just pushed). If the free was
///      swallowed it returns a DIFFERENT block and `b` is leaked → RED.
#[test]
fn legit_free_with_stale_copy_at_index_ge_cnt_is_not_swallowed() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Two distinct live blocks.
    let a = unsafe { (*heap).alloc(layout) };
    let b = unsafe { (*heap).alloc(layout) };
    assert!(!a.is_null() && !b.is_null());
    assert_ne!(a, b);

    // Push both: b becomes the top of the stack.
    unsafe { (*heap).dealloc(a, layout) };
    unsafe { (*heap).dealloc(b, layout) };

    // Pop the top → returns b; the slot it vacated (index == new cnt) STILL
    // holds a stale copy of b.
    let popped = unsafe { (*heap).alloc(layout) };
    assert_eq!(
        popped, b,
        "LIFO magazine pop should return the last-pushed block"
    );

    // Genuine free of the now-live b. Its stale copy sits at index == cnt.
    // A correct (i < cnt) scan does NOT see it → b is pushed back.
    unsafe { (*heap).dealloc(b, layout) };

    // The just-freed b must be re-issued (it is the LIFO top). If the scan read
    // the stale slot at index >= cnt, the free was swallowed and this returns a
    // different block — b leaked.
    let re = unsafe { (*heap).alloc(layout) };
    assert_eq!(
        re, b,
        "legit free of a live block with a stale copy at index >= cnt was \
         SWALLOWED as a false double-free (branchless scan read past cnt) — \
         block leaked"
    );

    // Cleanup: b (== re) and a are both live now.
    unsafe {
        (*heap).dealloc(re, layout);
        (*heap).dealloc(a, layout);
        HeapRegistry::recycle(heap);
    }
}
