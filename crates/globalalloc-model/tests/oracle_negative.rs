//! Negative-oracle suite: a deliberately-broken `RawAllocator` per oracle.
//!
//! This crate's product is *detection*: each test here proves the matching
//! oracle in `drive` actually fires, and pins the exact failure-message prefix
//! as behaviour. Every case is counterfactual — deleting the corresponding
//! assert in `drive` must fail the matching test (verified during development).
//!
//! The fake arena owns its memory through raw pointers only (no `Vec`/references
//! into it), so all writes through the pointers `drive` hands around are sound
//! and this file is miri/strict-provenance clean. The arena frees in `Drop`
//! even on panic-unwind, so no `-Zmiri-ignore-leaks` is needed.

use core::alloc::Layout;
use core::cell::Cell;
use core::ptr;

use globalalloc_model::{drive, Config, Op, RawAllocator};

/// A bump arena handing out raw slices of one allocation, pre-filled with 0.
struct Arena {
    base: *mut u8,
    cap: usize,
    layout: Layout,
    cursor: Cell<usize>,
}

impl Arena {
    fn new(cap: usize) -> Self {
        let layout = Layout::from_size_align(cap, 16).unwrap();
        // SAFETY: `layout` has non-zero size; the memory is immediately
        // zero-filled so all reads through returned pointers are defined.
        let base = unsafe { std::alloc::alloc(layout) };
        assert!(!base.is_null());
        // SAFETY: `base` is valid for `cap` bytes of writes.
        unsafe { ptr::write_bytes(base, 0, cap) };
        Arena {
            base,
            cap,
            layout,
            cursor: Cell::new(0),
        }
    }

    /// Advance the cursor by `n` bytes, returning the previous offset.
    fn bump(&self, n: usize) -> usize {
        let p = self.cursor.get();
        assert!(p + n <= self.cap, "arena exhausted");
        self.cursor.set(p + n);
        p
    }

    fn at(&self, off: usize) -> *mut u8 {
        assert!(off <= self.cap);
        // SAFETY: `base + off` stays within the allocation for `off <= cap`.
        unsafe { self.base.add(off) }
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        // SAFETY: `base` was allocated with `self.layout` and freed nowhere else.
        unsafe { std::alloc::dealloc(self.base, self.layout) };
    }
}

/// The fault each test injects into the fake allocator.
#[derive(Clone, Copy)]
enum Fault {
    /// Behave as a correct allocator.
    Honest,
    /// `alloc` returns null.
    NullAlloc,
    /// `alloc` returns a pointer offset by `k` bytes (misaligned).
    MisalignedBy(usize),
    /// `alloc_zeroed` hands out an honest block but scribbles 0xAA instead of
    /// zeroing.
    NotZeroed,
    /// `alloc` registers `size - 1` bytes with the arena but returns the old
    /// cursor, so the NEXT allocation starts 1 byte inside the previous
    /// block's model extent (a short block).
    ShortBlock,
    /// `alloc_zeroed` returns `base + off` (inside block 0's extent). The
    /// arena is zero-filled, so if the overlap check were missing the
    /// zero-check would pass and only the run-end sweep would notice.
    OverlapZeroedAt(usize),
    /// `realloc` copies the prefix to `base + off` and returns that pointer:
    /// a foreign block overlapping another live allocation. The copy makes
    /// every check EXCEPT overlap pass, so the overlap assert is the only
    /// thing that can fire.
    ReallocToForeign { off: usize },
    /// `realloc` copies the prefix to `base + off` and returns it — a legal
    /// in-place shape whose new extent overlaps only the OLD block.
    ReallocInPlace(usize),
    /// `realloc` hands out a fresh honest block WITHOUT copying (loses the
    /// prefix).
    ReallocNoCopy,
}

/// A bump-arena allocator with one injected fault.
struct Faulty {
    arena: Arena,
    fault: Fault,
}

// SAFETY: within the arena's bounds every returned pointer is valid for the
// requested (fault-adjusted) extent, which is exactly what each oracle case
// exercises; `drive` is the only caller.
unsafe impl RawAllocator for Faulty {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        match self.fault {
            Fault::NullAlloc => ptr::null_mut(),
            Fault::MisalignedBy(k) => self.arena.at(k),
            Fault::ShortBlock => {
                // Register size - 1 with the arena but return a pointer at
                // (cursor - 1) rounded DOWN to the requested align, so the
                // align oracle passes and the SHORTNESS is what fires: the
                // NEXT allocation starts inside this block's model extent.
                let align = layout.align();
                let p = (self.arena.cursor.get().saturating_sub(1)) / align * align;
                let _ = self.arena.bump(size - 1);
                self.arena.at(p)
            }
            _ => {
                let p = self.arena.bump(size);
                self.arena.at(p)
            }
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        if let Fault::OverlapZeroedAt(off) = self.fault {
            return self.arena.at(off);
        }
        let p = self.arena.bump(size);
        let ptr = self.arena.at(p);
        if let Fault::NotZeroed = self.fault {
            // Hand out the honest block but scribble instead of zeroing.
            // SAFETY: `ptr` is valid for `size` bytes inside the arena.
            unsafe { ptr::write_bytes(ptr, 0xAA, size) };
        } else {
            // SAFETY: `ptr` is valid for `size` bytes inside the arena.
            unsafe { ptr::write_bytes(ptr, 0, size) };
        }
        ptr
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump arena: never frees; `drive` never hands out duplicate pointers
        // unless a fault makes it, and the overlap oracle catches that first.
    }

    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        let keep = old_layout.size().min(new_size);
        match self.fault {
            Fault::ReallocToForeign { off } => {
                let dst = self.arena.at(off);
                // SAFETY: `ptr` is valid for `keep` reads; `dst` for `keep`
                // writes inside the arena.
                unsafe { ptr::copy(ptr, dst, keep) };
                dst
            }
            Fault::ReallocInPlace(off) => {
                let dst = self.arena.at(off);
                // SAFETY: as above.
                unsafe { ptr::copy(ptr, dst, keep) };
                dst
            }
            Fault::ReallocNoCopy => {
                // Honest fresh bump WITHOUT copying: loses the prefix.
                let p = self.arena.bump(new_size);
                self.arena.at(p)
            }
            _ => {
                // Honest: a fresh block with the prefix copied.
                let p = self.arena.bump(new_size);
                let dst = self.arena.at(p);
                // SAFETY: `ptr` is valid for `keep` reads; `dst` for `keep`
                // writes inside the arena.
                unsafe { ptr::copy(ptr, dst, keep) };
                dst
            }
        }
    }
}

fn faulty(cap: usize, fault: Fault) -> Faulty {
    Faulty {
        arena: Arena::new(cap),
        fault,
    }
}

#[test]
#[should_panic(expected = "M3: op #")]
fn overlap_on_alloc_zeroed_panics() {
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::AllocZeroed { size: 64, align: 8 },
    ];
    drive(
        &faulty(4096, Fault::OverlapZeroedAt(16)),
        Config::default(),
        &ops,
    );
}

#[test]
#[should_panic(expected = "M3: op #")]
fn overlap_on_realloc_panics() {
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc { i: 0, new_size: 64 },
    ];
    drive(
        &faulty(4096, Fault::ReallocToForeign { off: 80 }), // inside block 1's [64..128), disjoint from the old block
        Config::default(),
        &ops,
    );
}

#[test]
#[should_panic(expected = "M3: op #")]
fn undersized_block_overlap_panics() {
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Alloc { size: 64, align: 8 },
    ];
    drive(&faulty(4096, Fault::ShortBlock), Config::default(), &ops);
}

#[test]
#[should_panic(expected = "M1/M4:")]
fn misaligned_alloc_panics() {
    let ops = [Op::Alloc { size: 32, align: 8 }];
    drive(
        &faulty(4096, Fault::MisalignedBy(1)),
        Config::default(),
        &ops,
    );
}

#[test]
#[should_panic(expected = "alloc_zeroed:")]
fn alloc_zeroed_garbage_panics() {
    let ops = [Op::AllocZeroed { size: 32, align: 8 }];
    drive(&faulty(4096, Fault::NotZeroed), Config::default(), &ops);
}

#[test]
#[should_panic(expected = "lost prefix byte")]
fn realloc_without_copy_loses_prefix() {
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc {
            i: 0,
            new_size: 128,
        },
    ];
    drive(&faulty(4096, Fault::ReallocNoCopy), Config::default(), &ops);
}

#[test]
#[should_panic(expected = "M1: op #0 alloc(size=32, align=8) returned null")]
fn null_alloc_panics() {
    let ops = [Op::Alloc { size: 32, align: 8 }];
    drive(&faulty(4096, Fault::NullAlloc), Config::default(), &ops);
}

#[test]
fn honest_arena_passes_drive() {
    // Proves the fake infrastructure itself is oracle-clean: a mixed stream
    // (alloc, alloc_zeroed, dealloc, realloc grow + shrink, double_free OFF)
    // against an honest arena must pass every oracle.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::AllocZeroed {
            size: 32,
            align: 16,
        },
        Op::Alloc {
            size: 128,
            align: 8,
        },
        Op::Dealloc(1),
        Op::Realloc {
            i: 0,
            new_size: 256,
        }, // grow
        Op::Realloc { i: 0, new_size: 32 }, // shrink
        Op::Dealloc(0),
        Op::Dealloc(0),
    ];
    drive(&faulty(64 * 1024, Fault::Honest), Config::default(), &ops);
}

#[test]
fn in_place_realloc_inside_own_old_block_passes() {
    // A legal in-place realloc whose new extent overlaps the OLD block only.
    // Pins the `skip: Some(i)` behaviour: if the skip were dropped, the
    // overlap oracle would fire on this legitimate stream.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc { i: 0, new_size: 64 },
    ];
    drive(
        &faulty(4096, Fault::ReallocInPlace(8)),
        Config::default(),
        &ops,
    );
}
