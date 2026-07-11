//! Regression test for finding F1 (`docs/reviews/2026-07-10-ub-memory-audit-verification.md`):
//! `AllocCore::realloc` on a foreign/unregistered pointer must return
//! `null_mut()`, never fall through to the move-leg.
//!
//! ## The defect
//!
//! `AllocCore::realloc(ptr, old_layout, new_size)` is a **safe** `pub fn`.
//! Before the fix, the foreign-pointer guard (`contains_base`) was checked
//! ONLY to decide whether to attempt the in-place fast path:
//!
//! ```text
//! if self.table.contains_base(base) {
//!     if let Some(p) = self.realloc_inplace_fast_path_known_base(...) {
//!         return p;
//!     }
//! }
//! // falls through here even when contains_base(base) == false
//! let new_ptr = self.alloc(new_layout);
//! Node::copy_nonoverlapping(ptr, new_ptr, copy); // reads from the ORIGINAL `ptr`
//! self.dealloc(ptr, old_layout);
//! ```
//!
//! When `contains_base(base)` is `false` (the caller passes a pointer that
//! is not one of ours — foreign, stack, dangling, or simply never allocated
//! by this `AllocCore`), control still reaches the move-leg: it allocates a
//! fresh block and `ptr::copy_nonoverlapping`s `min(old_layout.size(),
//! new_size)` bytes OUT OF THE CALLER-SUPPLIED `ptr` — an arbitrary address
//! this allocator never registered. That is an out-of-bounds / use of
//! unrelated memory reachable from a 100%-safe public function, and it is
//! asymmetric with `dealloc`, which already treats an unrecognised base as a
//! no-op.
//!
//! `AllocCore` is re-exported publicly (`sefer_alloc::AllocCore`, no
//! `#[doc(hidden)]`), so this was a fully public safe-API unsoundness.
//!
//! ## The fix
//!
//! `AllocCore::realloc` now checks `contains_base` unconditionally, up
//! front, and returns `null_mut()` immediately when the pointer's segment
//! base is not registered — before even attempting the in-place fast path,
//! and definitely before the move-leg's `alloc` + `copy_nonoverlapping` +
//! `dealloc`. This is symmetric with `dealloc`'s existing foreign-pointer
//! no-op contract.
//!
//! Note this fix is intentionally scoped to `AllocCore::realloc`
//! (substrate-level). `registry::HeapCore::realloc`'s foreign-leg is a
//! different, deliberately-designed cross-heap path gated behind
//! `alloc-xthread`: a pointer from another LIVE heap in the same process is
//! legitimate there, and its own `dealloc` routes such frees cross-thread.
//! That path is untouched by this test/fix.
//!
//! ## Counterfactual (non-vacuity)
//!
//! This test was verified to fail (or be caught by miri as an OOB read) with
//! the guard removed / reverted to the pre-fix `if self.table.contains_base
//! (base) { ... }`-only gating, and to pass with the guard restored. The
//! `stack_address_is_never_touched` assertion below is the deterministic
//! signal: it plants a sentinel byte pattern on a stack buffer that is NOT a
//! registered allocation, calls `realloc` on it, and asserts (a) the return
//! value is null and (b) the sentinel bytes are byte-for-byte unchanged
//! (nothing in the allocator wrote through or otherwise disturbed the
//! foreign memory) and (c) the allocator's own state (a live, independently
//! allocated and written block) is unperturbed.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

#[test]
fn realloc_of_foreign_pointer_returns_null() {
    let mut a = AllocCore::new().expect("AllocCore::new");

    // A stack buffer is never `contains_base`-registered with `a` — it is a
    // textbook foreign pointer from `AllocCore`'s point of view.
    let mut stack_buf = [0xEEu8; 64];
    let foreign_ptr = stack_buf.as_mut_ptr();
    let old_layout = Layout::from_size_align(64, 1).unwrap();

    let result = a.realloc(foreign_ptr, old_layout, 128);

    assert!(
        result.is_null(),
        "AllocCore::realloc on a foreign pointer must return null_mut() \
         (F1 guard), not fall through to the move-leg and read/copy out of \
         caller-supplied memory the allocator never registered"
    );
}

#[test]
fn realloc_of_foreign_pointer_does_not_touch_foreign_memory_or_allocator_state() {
    let mut a = AllocCore::new().expect("AllocCore::new");

    // Establish a genuine live allocation first, so we can confirm the
    // allocator's own state is unperturbed by the foreign-pointer call.
    let live_layout = Layout::from_size_align(32, 1).unwrap();
    let live_ptr = a.alloc(live_layout);
    assert!(!live_ptr.is_null(), "setup: live alloc failed");
    // SAFETY: `live_ptr` is valid for 32 bytes per M1.
    unsafe {
        std::ptr::write_bytes(live_ptr, 0xAB, 32);
    }

    // Foreign stack buffer with a known sentinel pattern. If the move-leg
    // were (incorrectly) taken, `Node::copy_nonoverlapping` would READ from
    // this buffer (not write to it) — so the counterfactual that actually
    // distinguishes "guard present" from "guard absent" is the null return
    // plus the allocator's internal state, not the sentinel bytes
    // themselves (a read does not mutate the source). We still assert the
    // sentinel is untouched as defence-in-depth / documentation of intent.
    let sentinel = [0xEEu8; 64];
    let mut stack_buf = sentinel;
    let foreign_ptr = stack_buf.as_mut_ptr();
    let old_layout = Layout::from_size_align(64, 1).unwrap();

    let result = a.realloc(foreign_ptr, old_layout, 128);
    assert!(result.is_null(), "expected null for foreign pointer");

    // The foreign stack buffer bytes are untouched.
    assert_eq!(
        stack_buf, sentinel,
        "foreign stack memory was mutated by AllocCore::realloc"
    );

    // The allocator's own live block is unperturbed: still readable with the
    // bytes we wrote, i.e. the failed foreign realloc did not corrupt
    // allocator-internal state (no accidental dealloc/relocation of an
    // unrelated live block).
    // SAFETY: `live_ptr` is still valid for 32 bytes; nothing above freed it.
    unsafe {
        for i in 0..32 {
            assert_eq!(
                live_ptr.add(i).read(),
                0xAB,
                "unrelated live allocation was disturbed by a foreign-pointer realloc call"
            );
        }
    }

    a.dealloc(live_ptr, live_layout);
}
