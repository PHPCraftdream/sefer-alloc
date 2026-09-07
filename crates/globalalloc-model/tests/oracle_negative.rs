//! Negative-oracle suite: a deliberately-broken `RawAllocator` per oracle.
//!
//! This crate's product is *detection*: each test here proves the matching
//! oracle in `drive` actually fires, and pins the exact failure-message prefix
//! as behaviour. Every case is counterfactual — deleting the corresponding assert (or,
//! for the clamp and realloc-skip pins, the corresponding branch) in
//! `drive` must fail the matching test (verified during development); the
//! one infrastructure exception is `honest_arena_passes_drive`, which
//! proves the fake itself is oracle-clean rather than pinning a `drive`
//! check. Two checks genuinely have no in-op counterfactual and are
//! pinned at its next observable read instead: the M1 fill read-back's
//! op-time check and the alloc_zeroed arm's identical fill read-back
//! (review run 3, P3-1) cannot be broken by any sound sequential fault
//! (no allocator call can intervene between `fill_block` and
//! `verify_block`),
//! so the fill-persistence tests below corrupt the block AFTER its
//! read-back passed and pin the run-end sweep — the next read of the
//! block, and the check a lost write would otherwise escape through.
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
    /// Every `alloc`/`alloc_zeroed` call after the first behaves honestly for ITS OWN
    /// block, but silently scribbles the FIRST block the arena ever handed
    /// out with a foreign byte (0xCC). The returned pointer is honest and
    /// model-disjoint, so `drive`'s INCREMENTAL overlap check passes at
    /// insertion; only the run-end M3 sweep ever re-reads the clobbered
    /// block's bytes. This is the counterfactual for the run-end sweep.
    ClobberOnLaterAlloc,
    /// Every `alloc`/`alloc_zeroed` call after the first silently re-zeroes
    /// the FIRST block the arena ever handed out: whatever `drive` wrote
    /// there "does not stick". Honest at hand-out, invisible to the
    /// incremental overlap check; the run-end M3 sweep is the next read of
    /// the lost block, so this is the counterfactual for fill persistence.
    WritesDoNotStick,
    /// `realloc` returns null (the documented realloc-failure signal). Used
    /// by a POSITIVE test: the run must complete with the old blocks still
    /// live and verifying, proving the null-realloc `continue` skip keeps
    /// the bookkeeping coherent instead of skipping it.
    NullRealloc,
}

/// A bump-arena allocator with one injected fault.
struct Faulty {
    arena: Arena,
    fault: Fault,
    /// Offset and size of the FIRST block the arena handed out — the
    /// corruption target of `ClobberOnLaterAlloc` and `WritesDoNotStick`.
    first_block: Cell<Option<(usize, usize)>>,
}

impl Faulty {
    /// Shared bookkeeping for the honest hand-out paths of `alloc` and
    /// `alloc_zeroed`: remember the first handed-out block, and — for the
    /// two corruption faults — silently corrupt it on every LATER hand-out.
    /// The block being handed out by THIS call is always honest.
    fn fault_touch_first_block(&self, new_off: usize, new_size: usize) {
        match (self.first_block.get(), self.fault) {
            (Some((off, len)), Fault::ClobberOnLaterAlloc) => {
                // SAFETY: `off..off+len` is inside the arena: the offset
                // was bounds-asserted by `bump_aligned` (its `p + n <= cap`
                // check) before it was ever handed out — the later
                // `at_len` on the alloc path runs AFTER this corruption,
                // so it is not what bounds this write.
                unsafe { ptr::write_bytes(self.arena.base.add(off), 0xCC, len) };
            }
            (Some((off, len)), Fault::WritesDoNotStick) => {
                // The earlier block's writes are "lost": the fake re-zeroes
                // its whole extent before handing out the new block.
                // SAFETY: same bounds argument as the arm above.
                unsafe { ptr::write_bytes(self.arena.base.add(off), 0x00, len) };
            }
            (None, _) => self.first_block.set(Some((new_off, new_size))),
            _ => {}
        }
    }
}

// SAFETY: within the arena's bounds every returned pointer is valid for the
// requested (fault-adjusted) extent, which is exactly what each oracle case
// exercises; `drive` is the only caller.
unsafe impl RawAllocator for Faulty {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        assert!(
            size > 0,
            "GlobalAlloc precondition violated: Faulty::alloc called with a zero-size \
             layout (drive's zero-clamp must run before the allocator is reached)"
        );
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
                self.fault_touch_first_block(p, size);
                self.arena.at_len(p, size)
            }
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        assert!(
            layout.size() > 0,
            "GlobalAlloc precondition violated: Faulty::alloc_zeroed called with a \
             zero-size layout (drive's zero-clamp must run before the allocator is \
             reached)"
        );
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
        self.fault_touch_first_block(p, size);
        ptr
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump arena: never frees; `drive` never hands out duplicate pointers
        // unless a fault makes it, and the overlap oracle catches that first.
    }

    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        assert!(
            new_size > 0,
            "GlobalAlloc precondition violated: Faulty::realloc called with new_size=0 \
             (drive's zero-clamp must run before the allocator is reached)"
        );
        assert!(
            new_size <= (isize::MAX as usize / old_layout.align()) * old_layout.align(),
            "GlobalAlloc precondition violated: Faulty::realloc called with new_size={} \
             whose round-up to align {} overflows isize (drive's clamp must run first)",
            new_size,
            old_layout.align()
        );
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
            Fault::NullRealloc => ptr::null_mut(),
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
        first_block: Cell::new(None),
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

#[test]
#[should_panic(expected = "M3: step #0 run-end sweep: live block clobbered")]
fn clobbered_by_later_alloc_reaches_run_end_sweep() {
    // Counterfactual for `drive`'s RUN-END M3 sweep: the second alloc's
    // returned pointer is honest and model-disjoint, so the INCREMENTAL
    // overlap check passes at op 1; the 0xCC scribble over block 0 is
    // invisible until the run-end sweep reads block 0's fill back. Deleting
    // the run-end sweep from `drive` lets this test pass spuriously —
    // nothing else ever re-reads block 0's bytes.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Alloc { size: 32, align: 8 },
    ];
    drive(
        &faulty(4096, Fault::ClobberOnLaterAlloc),
        Config::default(),
        &ops,
    );
}

#[test]
#[should_panic(expected = "M3: step #0 run-end sweep: live block clobbered")]
fn alloc_fill_that_does_not_stick_is_caught_at_run_end() {
    // Counterfactual for FILL PERSISTENCE behind the M1 write-read-back:
    // the fake re-zeroes block 0 during op 1, so op 0's fill is lost after
    // op 0's own read-back has already passed (see the module doc for why
    // the op-time read-back itself is unpinnable by a sequential fault).
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Alloc { size: 32, align: 8 },
    ];
    drive(
        &faulty(4096, Fault::WritesDoNotStick),
        Config::default(),
        &ops,
    );
}

#[test]
#[should_panic(expected = "M3: step #0 run-end sweep: live block clobbered")]
fn alloc_zeroed_fill_that_does_not_stick_is_caught_at_run_end() {
    // The same lost-write fault through the alloc_zeroed arm (whose fill
    // read-back is the check review run 3's P3-1 added): op 0's zero-check
    // passes, its fill read-back passes, and the re-zero at op 1 destroys
    // the fill — caught by the run-end sweep, never tolerated.
    let ops = [
        Op::AllocZeroed { size: 64, align: 8 },
        Op::AllocZeroed { size: 32, align: 8 },
    ];
    drive(
        &faulty(4096, Fault::WritesDoNotStick),
        Config::default(),
        &ops,
    );
}

#[test]
fn null_realloc_completes_with_old_block_intact() {
    // Counterfactual for the SOUNDNESS-CRITICAL null-realloc skip
    // (`if new_ptr.is_null() { continue; }` in drive's Realloc arm): with
    // the skip deleted, drive falls through to `verify_prefix_block(null,
    // keep, ...)` and DEREFERENCES NULL. This test must complete cleanly
    // instead: both reallocs return null, both old blocks stay live with
    // their fills intact, and the run-end sweep plus teardown prove the
    // skip left the bookkeeping coherent rather than skipping it.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc {
            i: 0,
            new_size: 128,
        }, // null: skipped, block 0 stays live
        Op::Alloc { size: 32, align: 8 },
        Op::Realloc { i: 1, new_size: 16 }, // null again, on the second block
    ];
    drive(&faulty(4096, Fault::NullRealloc), Config::default(), &ops);
}

#[test]
fn zero_size_alloc_is_clamped_up_not_rejected() {
    // Counterfactual for the UP direction of `drive`'s P0-1 totality clamp:
    // a hand-built `size: 0` must be clamped to the 1-byte minimum
    // `GlobalAlloc` permits, so the op REACHES the allocator as a
    // well-formed request instead of violating its precondition.
    // `Faulty::alloc`'s assert documents that precondition — deleting the
    // clamp fails this test natively with "GlobalAlloc precondition
    // violated" instead of handing a zero-size layout to a real allocator
    // (UB).
    let ops = [Op::Alloc { size: 0, align: 8 }];
    drive(&faulty(4096, Fault::Honest), Config::default(), &ops);
}

#[test]
fn zero_size_alloc_zeroed_is_clamped_up_not_rejected() {
    // The same zero-clamp counterfactual through the alloc_zeroed arm.
    let ops = [Op::AllocZeroed { size: 0, align: 8 }];
    drive(&faulty(4096, Fault::Honest), Config::default(), &ops);
}

#[test]
fn zero_size_realloc_is_clamped_up_not_rejected() {
    // The same zero-clamp counterfactual for `new_size: 0` through the
    // realloc arm: clamped to a well-formed 1-byte resize, which must
    // complete cleanly (old block freed at teardown, prefix verified).
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc { i: 0, new_size: 0 },
    ];
    drive(&faulty(4096, Fault::Honest), Config::default(), &ops);
}

#[test]
#[should_panic(
    expected = "[clamped from 18446744073709551615], align=8) returned null — note: the harness does not model"
)]
fn clamped_down_null_alloc_names_oom_note() {
    // Pins the clamp-DOWN null message from a simulated null-returning
    // allocator: `oversized_size_is_clamped_not_rejected` pins only the fake
    // arena's own "arena exhausted" failure, so the OOM-note message itself
    // had no content pin. `Fault::NullAlloc` returns null before touching
    // the arena, so the huge clamped size never exhausts anything and the
    // M1 message is what fires (18446744073709551615 = usize::MAX).
    let ops = [Op::Alloc {
        size: usize::MAX,
        align: 8,
    }];
    drive(&faulty(4096, Fault::NullAlloc), Config::default(), &ops);
}

#[test]
#[should_panic(
    expected = "M1: op #0 alloc(size=1 [clamped from 0 — GlobalAlloc forbids a zero-size layout], align=8) returned null"
)]
fn clamped_up_null_alloc_gets_no_oom_note() {
    // Pins the clamp-UP null message (review run 4, P3-1): a null for a
    // size clamped UP from 0 is a genuine allocator defect — a 1-byte
    // allocation failed — so the message must carry the zero-layout
    // bracket and NO "harness does not model OOM" note (the pre-fix shared
    // message blamed the allocator for the harness's own rewrite). The pin
    // matches the new bracket text, which the old message lacks, so
    // reverting the message split fails this test.
    let ops = [Op::Alloc { size: 0, align: 8 }];
    drive(&faulty(4096, Fault::NullAlloc), Config::default(), &ops);
}
