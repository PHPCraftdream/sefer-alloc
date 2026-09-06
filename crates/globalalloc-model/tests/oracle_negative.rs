//! Negative-oracle suite: a deliberately-broken `RawAllocator` per oracle.
//!
//! This crate's product is *detection*: each test here proves the matching
//! oracle in `drive` actually fires, and pins the exact failure-message prefix
//! as behaviour. Every case is counterfactual — deleting the corresponding
//! assert in `drive` must fail the matching test (verified during development).
//!
//! The fake arena owns its memory through raw pointers only (no `Vec`/references
//! into it), so all writes through the pointers `drive` hands around are sound
//! and this file is miri/strict-provenance clean. Every pointer handed to
//! `drive` is length-checked against the arena (`Arena::at_len`), so a future
//! fault that would run past the arena panics in the harness instead of
//! writing past the allocation. The arena frees in `Drop` even on
//! panic-unwind, so no `-Zmiri-ignore-leaks` is needed.

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
        // 4096-aligned base: an offset aligned to any power of two <= 4096
        // is then absolutely aligned, so `bump_aligned`'s offset rounding
        // makes honest paths satisfy M1/M4 for every align these tests use
        // (a larger align would need a larger base; none exist here).
        let layout = Layout::from_size_align(cap, 4096).unwrap();
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
    /// (Fault paths only; honest paths use [`Arena::bump_aligned`] so a
    /// handed-out block always satisfies its requested align.)
    fn bump(&self, n: usize) -> usize {
        let p = self.cursor.get();
        assert!(
            p.checked_add(n).is_some_and(|end| end <= self.cap),
            "arena exhausted"
        );
        self.cursor.set(p + n);
        p
    }

    /// Advance the cursor to the next multiple of `align`, then by `n`
    /// bytes, returning the aligned offset. Honest allocation paths use
    /// this so every handed-out block satisfies its requested
    /// `layout.align()` regardless of earlier ops' sizes — without it, an
    /// unaligned landing offset makes a NO-FAULT test fail M1/M4
    /// spuriously (this file would have to be hand-tuned op by op).
    fn bump_aligned(&self, n: usize, align: usize) -> usize {
        assert!(align.is_power_of_two(), "align must be a power of two");
        let p = self.cursor.get().next_multiple_of(align);
        assert!(
            p.checked_add(n).is_some_and(|end| end <= self.cap),
            "arena exhausted"
        );
        self.cursor.set(p + n);
        p
    }

    /// Pointer at `off`, checked to keep `off + len` inside the arena. Every
    /// path that hands `drive` a pointer the harness will read or write for
    /// a known length goes through this, so a future fault (or a longer op
    /// stream) that would run past the arena panics HERE — a detected
    /// failure — instead of writing past the allocation, which is real UB
    /// in the harness rather than an oracle report.
    fn at_len(&self, off: usize, len: usize) -> *mut u8 {
        assert!(
            off.checked_add(len).is_some_and(|end| end <= self.cap),
            "arena access [{off}..{off}+{len}) exceeds cap {}",
            self.cap
        );
        // SAFETY: `base + off` stays within the allocation for
        // `off + len <= cap`.
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
    /// `alloc` registers `size - 1` bytes with the arena but returns
    /// `(cursor - 1)` rounded DOWN to the requested align, so the align
    /// oracle passes and the SHORTNESS is what fires: the next allocation
    /// starts INSIDE this block's model extent (8 bytes inside in the
    /// actual test below).
    ShortBlock,
    /// `alloc_zeroed` returns `base + off` (inside block 0's extent). Not
    /// an isolation argument: if the overlap check were deleted, the ZERO
    /// check would fire immediately — `drive` filled block 0 with 0x01 at
    /// op 0, so the overlapping block's first byte reads 0x01, not 0, and
    /// the panic message becomes `alloc_zeroed:` instead of `M3: op #`.
    /// That message change is exactly what the `#[should_panic]` pin needs:
    /// the test still fails without the overlap assert, just via the other
    /// oracle.
    OverlapZeroedAt(usize),
    /// `realloc` copies the prefix to `base + off` and returns that pointer.
    /// The two tests using it differ only in the offset and in the outcome
    /// they pin: `overlap_on_realloc_panics` picks an offset inside ANOTHER
    /// live block (a foreign block — the M3 oracle must fire), and there
    /// the copy makes every check EXCEPT overlap pass, so the overlap
    /// assert is the only thing that can fire;
    /// `in_place_realloc_inside_own_old_block_passes` picks one inside the
    /// OLD block only (a legal in-place shape the `skip: Some(i)` exclusion
    /// must tolerate).
    ReallocAt(usize),
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
            Fault::MisalignedBy(k) => {
                // Length-checked even though `drive` panics at the align
                // check before any access: the fault must stay a
                // well-formed pointer for the extent `drive` WOULD use.
                self.arena.at_len(k, size)
            }
            Fault::ShortBlock => {
                // Register size - 1 with the arena but return a pointer at
                // (cursor - 1) rounded DOWN to the requested align, so the
                // align oracle passes and the SHORTNESS is what fires: the
                // NEXT allocation starts inside this block's model extent.
                let align = layout.align();
                let p = (self.arena.cursor.get().saturating_sub(1)) / align * align;
                let _ = self.arena.bump(size - 1);
                self.arena.at_len(p, size)
            }
            _ => {
                let p = self.arena.bump_aligned(size, layout.align());
                self.arena.at_len(p, size)
            }
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        if let Fault::OverlapZeroedAt(off) = self.fault {
            return self.arena.at_len(off, size);
        }
        let p = self.arena.bump_aligned(size, layout.align());
        let ptr = self.arena.at_len(p, size);
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
            Fault::ReallocAt(off) => {
                let dst = self.arena.at_len(off, keep);
                // SAFETY: `ptr` is valid for `keep` reads; `dst` for `keep`
                // writes inside the arena.
                unsafe { ptr::copy(ptr, dst, keep) };
                dst
            }
            Fault::ReallocNoCopy => {
                // Honest fresh aligned bump WITHOUT copying: loses the prefix.
                let p = self.arena.bump_aligned(new_size, old_layout.align());
                self.arena.at_len(p, new_size)
            }
            _ => {
                // Honest: a fresh aligned block with the prefix copied.
                let p = self.arena.bump_aligned(new_size, old_layout.align());
                let dst = self.arena.at_len(p, new_size);
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
        &faulty(4096, Fault::ReallocAt(80)), // inside block 1's [64..128), disjoint from the old block
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
    drive(&faulty(4096, Fault::ReallocAt(8)), Config::default(), &ops);
}

#[test]
#[should_panic(expected = "arena exhausted")]
fn oversized_size_is_clamped_not_rejected() {
    // Pins `drive`'s P2-4 totality clamp on the alloc arms: a hand-built
    // oversized size must be CLAMPED into the admissible ceiling and reach
    // the allocator (here: the fake arena, which legitimately cannot serve
    // ~8 EiB and exhausts), never rejected by the harness with the old
    // `Layout::from_size_align(size=..., align=...) rejected` panic. Before
    // the fix this test failed with that harness-rejection message instead.
    let ops = [Op::Alloc {
        size: usize::MAX,
        align: 8,
    }];
    drive(&faulty(4096, Fault::Honest), Config::default(), &ops);
}
