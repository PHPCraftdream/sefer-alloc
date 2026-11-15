//! Counterfactual test for the M2 double-free-is-a-no-op oracle path
//! (`Config::double_free = Some(DoubleFreeOk::new())`) and drive's empty-model skip.
//!
//! Uses a trivial leak-everything allocator instead of `System`: `dealloc` is
//! a complete no-op regardless of how many times (or with what pointer) it is
//! called, which trivially satisfies the M2 contract by construction — the
//! same category the crate's own doc names as a valid `double_free`
//! consumer ("an allocator whose documented contract is that this is a
//! no-op").
//!
//! This is a counterfactual test, not just coverage: deleting the
//! `if config.double_free.is_some()` block in `drive` yields 3 dealloc calls instead of
//! 6 and fails the assertion below; breaking the empty-model skip in drive's
//! `Dealloc` arm changes the count too.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::cell::RefCell;

use globalalloc_model::{drive, Config, DoubleFreeOk, Op, RawAllocator};

/// Forwards `alloc`/`alloc_zeroed`/`realloc` to `System`; `dealloc` never
/// frees anything, but records every pointer it is called with.
struct LeakyAllocator {
    /// How many times `dealloc` was called.
    dealloc_count: Cell<usize>,
    /// The `ptr.addr()` of every `dealloc` call, in order.
    frees: RefCell<Vec<usize>>,
}

// SAFETY: `alloc`/`alloc_zeroed`/`realloc` forward to `System` under its
// identical `GlobalAlloc` contract. `dealloc` deliberately never frees, so it
// satisfies `RawAllocator`'s contract for a pointer allocated by this type
// trivially: any repeated call is a no-op, by construction, not by luck.
unsafe impl RawAllocator for LeakyAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding under the identical `GlobalAlloc::alloc` contract.
        unsafe { GlobalAlloc::alloc(&System, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding under the identical `GlobalAlloc::alloc_zeroed` contract.
        unsafe { GlobalAlloc::alloc_zeroed(&System, layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // Deliberately does nothing: this allocator leaks by design (fine
        // natively and under the CI miri job's `-Zmiri-ignore-leaks`), so a
        // repeated dealloc of the same pointer can never double-free.
        self.dealloc_count.set(self.dealloc_count.get() + 1);
        self.frees.borrow_mut().push(ptr.addr());
    }
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarding under the identical `GlobalAlloc::realloc` contract.
        unsafe { GlobalAlloc::realloc(&System, ptr, old_layout, new_size) }
    }
}

#[test]
fn double_free_is_a_no_op_against_a_leaky_allocator() {
    // After the realloc the model holds two blocks; the first `Dealloc(0)`
    // removes one, the second removes the last, and the (new) third finds the
    // model empty and must be skipped entirely by drive's `live.is_empty()`
    // guard.
    let ops = vec![
        Op::Alloc { size: 32, align: 8 }, // block A
        Op::AllocZeroed {
            size: 64,
            align: 16,
        }, // block Z
        Op::Dealloc(0),                   // frees A (+ the M2 second dealloc of the same pointer)
        Op::Alloc {
            size: 128,
            align: 64,
        }, // block B
        Op::Realloc {
            i: 0,
            new_size: 256,
        }, // Z moves -> Z'
        Op::Dealloc(0),                   // frees Z' (+ M2 repeat)
        Op::Dealloc(0),                   // frees B (+ M2 repeat); model now empty
        Op::Dealloc(0),                   // model empty: skipped entirely by drive
    ];
    let alloc = LeakyAllocator {
        dealloc_count: Cell::new(0),
        frees: RefCell::new(Vec::new()),
    };
    let config = Config {
        // SAFETY: `LeakyAllocator`'s `dealloc` never frees anything, so a
        // repeated `dealloc` is a no-op by construction — `DoubleFreeOk::new`'s
        // contract holds.
        double_free: Some(unsafe { DoubleFreeOk::new() }),
        ..Config::default()
    };
    drive(&alloc, config, &ops);

    // 3 Dealloc ops x 2 frees each (double_free ON); the 4th op was skipped.
    assert_eq!(alloc.dealloc_count.get(), 6);
    let frees = alloc.frees.borrow();
    assert_eq!(frees.len(), 6);
    // Consecutive pairs are the M2 repeats, in free order: [A, A, Z', Z', B, B].
    assert_eq!(frees[0], frees[1]);
    assert_eq!(frees[2], frees[3]);
    assert_eq!(frees[4], frees[5]);
    // The three pairs are three distinct pointers.
    assert_ne!(frees[1], frees[2]);
    assert_ne!(frees[3], frees[4]);
}
