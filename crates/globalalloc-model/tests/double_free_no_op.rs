//! Exercises the M2 double-free-is-a-no-op oracle path (`Config::double_free
//! = true`), which no other test in this crate's own suite reaches — every
//! other test uses `Config::default()` (`double_free: false`), since a real
//! `System` allocator double-free is genuine undefined behaviour, not a
//! no-op. `drive()`'s `if config.double_free { ... }` branch was therefore
//! dead from this crate's own test suite's perspective before this file.
//!
//! Uses a trivial leak-everything allocator instead of `System`: `dealloc` is
//! a complete no-op regardless of how many times (or with what pointer) it is
//! called, which trivially satisfies the M2 contract by construction — the
//! same category the crate's own doc names as a valid `double_free: true`
//! consumer ("an allocator whose documented contract is that this is a
//! no-op").

use std::alloc::{GlobalAlloc, Layout, System};

use globalalloc_model::{drive, Config, Op, RawAllocator};

/// Forwards `alloc`/`alloc_zeroed`/`realloc` to `System`; `dealloc` never
/// frees anything, so a second (or third, ...) `dealloc` of the same pointer
/// is always a safe no-op.
struct LeakyAllocator;

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
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Deliberately does nothing: this allocator leaks by design, so a
        // repeated dealloc of the same pointer can never double-free.
    }
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarding under the identical `GlobalAlloc::realloc` contract.
        unsafe { GlobalAlloc::realloc(&System, ptr, old_layout, new_size) }
    }
}

#[test]
fn double_free_is_a_no_op_against_a_leaky_allocator() {
    let ops = vec![
        Op::Alloc { size: 32, align: 8 },
        Op::AllocZeroed {
            size: 64,
            align: 16,
        },
        Op::Dealloc(0), // triggers the M2 second-dealloc-of-the-same-pointer branch
        Op::Alloc {
            size: 128,
            align: 64,
        },
        Op::Realloc {
            i: 0,
            new_size: 256,
        },
        Op::Dealloc(0),
        Op::Dealloc(0), // model empties, drive()'s Dealloc no-ops on an empty `live`
    ];
    let config = Config {
        double_free: true,
        ..Config::default()
    };
    drive(&LeakyAllocator, config, &ops);
}
