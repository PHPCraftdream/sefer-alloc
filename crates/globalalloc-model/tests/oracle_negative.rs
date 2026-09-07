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
//! pinned at their next observable read instead: the M1 fill read-back's
//! op-time check and the alloc_zeroed arm's identical fill read-back
//! (review run 3, P3-1) cannot be broken by any sound sequential fault
//! (no allocator call can intervene between `fill_block` and
//! `verify_block`),
//! so the fill-persistence tests below corrupt the block AFTER its
//! read-back passed and pin the run-end sweep — the next read of the
//! block, and the check a lost write would otherwise escape through.
//!
//! The null/align oracles of the block-creating arms are pinned together in
//! one exhaustive grid (`Arm × Shape` in `null_align_cell`, replayed by a
//! single test over every cell). The coverage question is forced
//! structurally, at two compile-time links: `arm_of` classifies every `Op`
//! variant exhaustively, so a new `Op` variant is a compile error HERE until
//! it is classified block-creating or not — the decision whether it needs
//! its own grid cells cannot be skipped silently; and each axis enum is
//! defined together with its `ALL` slice from one variant list
//! (`grid_axis!`), so a new `Arm`/`Shape` variant is replayed by the grid
//! loop automatically and is a compile error in `null_align_cell` until its
//! cells are written — no hand-kept list left to forget. Every cell also
//! asserts its op stream really issues its own arm's op. That is the fix
//! for the per-arm gap review run 5's P3-1 documented (three consecutive
//! rounds each found a block-creating arm edited without matching coverage;
//! the `alloc_zeroed` null branch's removal is outright undefined behaviour
//! — a null dereference in `verify_zeroed_block` — so its cell must fail
//! with a crash, not a message mismatch, if that branch is ever deleted).
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
    /// `alloc_zeroed` returns null — the twin of [`Fault::NullAlloc`] on the
    /// zeroed path (review run 5, P3-1: without it, `drive`'s alloc_zeroed
    /// null branch had no counterfactual anywhere, and deleting that branch
    /// is a null dereference in `verify_zeroed_block`, not just an
    /// unpinned oracle).
    NullAllocZeroed,
    /// `alloc_zeroed` returns a pointer offset by `k` bytes (misaligned),
    /// the twin of [`Fault::MisalignedBy`] on the zeroed path.
    MisalignedZeroedBy(usize),
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
    /// check would fire immediately — `drive` filled block 0 (fill 1) at
    /// op 0, so the overlapping block's first byte reads
    /// the nonzero `pattern_byte(1, 16)`, not 0, and
    /// the panic message becomes `alloc_zeroed:` instead of `M3: op #`.
    /// That message change is exactly what the `#[should_panic]` pin needs:
    /// the test still fails without the overlap assert, just via the other
    /// oracle.
    OverlapZeroedAt(usize),
    /// `realloc` copies the prefix to `base + off` and returns that pointer.
    /// The tests using it differ only in the offset and in the outcome
    /// they pin: `overlap_on_realloc_panics` picks an offset inside ANOTHER
    /// live block (a foreign block — the M3 oracle must fire), and there
    /// the copy makes every check EXCEPT overlap pass, so the overlap
    /// assert is the only thing that can fire;
    /// `in_place_realloc_inside_own_old_block_passes` picks one inside the
    /// OLD block only (a legal in-place shape the `skip: Some(i)` exclusion
    /// must tolerate). A third use pins the realloc arm's ALIGN assert:
    /// `ReallocAt(9)` against an `align: 8` block returns a misaligned
    /// pointer that the align check rejects BEFORE the overlap check and the
    /// prefix read run — and with that assert deleted the whole realloc
    /// completes cleanly, so only that assert can make the misaligned grid
    /// cell pass.
    ReallocAt(usize),
    /// `realloc` hands out a fresh honest block WITHOUT copying (loses the
    /// prefix).
    ReallocNoCopy,
    /// `realloc` hands out a fresh honest block but fills the ENTIRE new range
    /// with a copy of the old block's byte 0, instead of copying the prefix —
    /// the review-run-2 P3-1 "repeat first byte" allocator. Under the old
    /// uniform fill this reproduced every expected byte and was invisible; under
    /// the position-dependent pattern it mismatches at every offset except 0.
    ReallocRepeatFirstByte,
    /// `realloc` copies `old[1..keep]` to `new[0..keep-1]` and duplicates
    /// `old[keep-1]` into `new[keep-1]` — the prefix copied SHIFTED LEFT by one
    /// with the tail patched, so every read stays inside initialized memory. A
    /// uniform block is invariant under this fault; the pattern is not.
    ReallocShiftLeft,
    /// `realloc` copies the old prefix REVERSED (`new[i] = old[keep-1-i]`) — a
    /// permutation rather than a shift. A uniform block is invariant under any
    /// permutation; the pattern is not.
    ReallocPermute,
    /// `realloc` ignores its real source pointer and instead copies from ONE BYTE
    /// PAST the start of the FIRST block the arena ever handed out — the
    /// review-run-3 P3-1 counterexample: under the OLD additive pattern, a
    /// foreign block one fill-id below the real source, read from offset 1,
    /// reproduced the real source's entire expected pattern. Requires the first
    /// handed-out block to still be live and at least `keep + 1` bytes long; the
    /// test using this fault arranges exactly that shape.
    ReallocFromForeignBlockPlusOne,
    /// Every `alloc`/`alloc_zeroed` call after the first behaves honestly for ITS OWN
    /// block, but silently scribbles the FIRST block the arena ever handed
    /// out with a foreign byte (0xCC). The returned pointer is honest and
    /// model-disjoint, so `drive`'s INCREMENTAL overlap check passes at
    /// insertion; only the run-end M3 sweep ever re-reads the clobbered
    /// block's bytes. This is the counterfactual for the run-end sweep.
    ClobberOnLaterAlloc,
    /// Every later `alloc`/`alloc_zeroed` corrupts only the final half of the
    /// first block. A shrink below that half used to discard the evidence
    /// before the run-end sweep could observe it.
    ClobberSuffixOnLaterAlloc,
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
    /// Every `dealloc` corrupts one initialized arena byte at `off`. The
    /// teardown test points this at the next survivor, proving one teardown
    /// free cannot silently damage a block that has not yet been freed.
    ClobberAtOnDealloc(usize),
}

/// A bump-arena allocator with one injected fault.
struct Faulty {
    arena: Arena,
    fault: Fault,
    /// Offset and size of the FIRST block the arena handed out — the
    /// corruption target of `ClobberOnLaterAlloc`,
    /// `ClobberSuffixOnLaterAlloc`, and `WritesDoNotStick`, and the copy
    /// SOURCE of `ReallocFromForeignBlockPlusOne`.
    first_block: Cell<Option<(usize, usize)>>,
}

impl Faulty {
    /// Read one arena byte back, for test-side verification of what `drive`
    /// actually left in the fake's memory after a clean run.
    fn arena_byte(&self, off: usize) -> u8 {
        assert!(
            off < self.arena.cap,
            "arena read [{off}] exceeds cap {}",
            self.arena.cap
        );
        // SAFETY: `off` is bounds-checked against the arena allocation.
        unsafe { self.arena.base.add(off).read() }
    }

    /// Shared bookkeeping for the honest hand-out paths of `alloc` and
    /// `alloc_zeroed`: remember the first handed-out block, and — for the
    /// three corruption faults — silently corrupt it on every LATER hand-out.
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
            (Some((off, len)), Fault::ClobberSuffixOnLaterAlloc) => {
                let suffix = len / 2;
                // SAFETY: `off..off+len` was bounds-checked when the first
                // block was handed out; `suffix <= len` keeps this subrange
                // within that initialized block.
                unsafe {
                    ptr::write_bytes(self.arena.base.add(off).add(suffix), 0xCD, len - suffix)
                };
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
        match self.fault {
            Fault::OverlapZeroedAt(off) => self.arena.at_len(off, size),
            // Null with no arena contact: only `drive`'s M1 null check can
            // fire, which is exactly what the grid cell exercises.
            Fault::NullAllocZeroed => ptr::null_mut(),
            // Length-checked even though `drive` panics at the align check
            // before any access (same shape as `MisalignedBy`).
            Fault::MisalignedZeroedBy(k) => self.arena.at_len(k, size),
            _ => {
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
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump arena: never frees; `drive` never hands out duplicate pointers
        // unless a fault makes it, and the overlap oracle catches that first.
        if let Fault::ClobberAtOnDealloc(off) = self.fault {
            let target = self.arena.at_len(off, 1);
            // SAFETY: `target` names one initialized byte inside the arena.
            unsafe { target.write(0xDD) };
        }
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
                let dst = self.arena.at_len(off, new_size);
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
            Fault::ReallocRepeatFirstByte => {
                let p = self.arena.bump_aligned(new_size, old_layout.align());
                let dst = self.arena.at_len(p, new_size);
                // SAFETY: `ptr` is valid for one initialized read (drive filled the
                // block just before the realloc); `dst` for `new_size` writes inside
                // the arena.
                unsafe {
                    let first = ptr.read();
                    ptr::write_bytes(dst, first, new_size);
                }
                dst
            }
            Fault::ReallocShiftLeft => {
                let p = self.arena.bump_aligned(new_size, old_layout.align());
                let dst = self.arena.at_len(p, new_size);
                // SAFETY: `ptr..ptr+keep` are initialized reads (drive's fill); every
                // write lands in `dst..dst+keep` inside the arena. `keep >= 1`
                // (`drive` clamps both sizes to >= 1), so `keep - 1` cannot underflow.
                unsafe {
                    ptr::copy(ptr.add(1), dst, keep - 1);
                    dst.add(keep - 1).write(ptr.add(keep - 1).read());
                }
                dst
            }
            Fault::ReallocPermute => {
                let p = self.arena.bump_aligned(new_size, old_layout.align());
                let dst = self.arena.at_len(p, new_size);
                // SAFETY: same read/write extent argument as `ReallocShiftLeft`.
                unsafe {
                    for b in 0..keep {
                        dst.add(b).write(ptr.add(keep - 1 - b).read());
                    }
                }
                dst
            }
            Fault::ReallocFromForeignBlockPlusOne => {
                let (foreign_off, foreign_len) = self
                    .first_block
                    .get()
                    .expect("fault requires a first block to already exist");
                let p = self.arena.bump_aligned(new_size, old_layout.align());
                let dst = self.arena.at_len(p, new_size);
                let src = self.arena.at_len(foreign_off + 1, foreign_len - 1);
                // SAFETY: `src` is valid for `keep` reads (the test arranges the first
                // block to be at least `keep + 1` bytes, all initialized by `drive`'s
                // fill); `dst` is valid for `new_size` writes inside the arena.
                unsafe { ptr::copy(src, dst, keep) };
                dst
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

/// The test-side restatement of `drive`'s position-dependent expectation
/// (`drive::pattern_byte` is private). Deliberately duplicated rather than
/// exposed: an independent copy is the stronger oracle — if the crate's
/// formula changes shape, every exact byte pin below fails and forces a
/// conscious re-derivation instead of silently tracking the new scheme.
fn pattern_byte(fill: u8, offset: usize) -> u8 {
    let mut x = (fill as u32) ^ (offset as u32).wrapping_mul(0x9E37_79B1);
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    x as u8
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

/// What a grid cell must observe when its op stream is replayed.
#[derive(Clone, Copy, Debug)]
enum Expect {
    /// `drive` must panic, naming this oracle: the same substring pin a
    /// standalone `#[should_panic(expected = ...)]` uses, asserted per cell
    /// inside the loop.
    Panics(&'static str),
    /// `drive` must complete. The realloc-null cell: a null realloc is the
    /// documented failure signal, the skip is the behaviour under test, and
    /// if the skip were deleted the cell fails with a null dereference (a
    /// crash), not a message mismatch.
    Completes,
}

/// One cell of the exhaustive null/align grid: the fault that breaks the arm
/// in that shape, the op stream that reaches the oracle, and what must be
/// observed.
struct NullAlignCase {
    /// The injected fault.
    fault: Fault,
    /// The op stream replayed.
    ops: &'static [Op],
    /// What `drive` must do on that stream.
    expect: Expect,
}

/// Defines a grid-axis enum together with its `ALL` slice from ONE variant
/// list, so the two cannot drift: a variant added to the invocation is
/// automatically in `ALL` and so replayed by the grid loop below, while
/// `null_align_cell`'s exhaustive match refuses to compile until its cells
/// are written. This replaces `Arm::ALL`'s hand-written "keep in sync"
/// array, which nothing enforced (review run 6, P3-1, second link).
macro_rules! grid_axis {
    (
        $(#[$enum_meta:meta])*
        $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident
            ),* $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum $name {
            $(
                $(#[$variant_meta])*
                $variant
            ),*
        }

        impl $name {
            /// Every value of this axis (the grid loop iterates `ALL`).
            /// Generated from the same variant list as the enum, so it
            /// cannot forget a variant.
            const ALL: &'static [Self] = &[$(Self::$variant),*];
        }
    };
}

grid_axis!(
    /// The block-creating arms of [`Op`], one axis of the grid below.
    Arm {
        /// [`Op::Alloc`].
        Alloc,
        /// [`Op::AllocZeroed`].
        AllocZeroed,
        /// [`Op::Realloc`].
        Realloc,
    }
);

grid_axis!(
    /// The fault shapes, the other axis of the grid below.
    Shape {
        /// The op returns null.
        Null,
        /// The op returns an in-arena pointer at a non-conforming offset.
        Misaligned,
    }
);

/// The grid arm an op belongs to (`None` = not block-creating). Exhaustive
/// over `Op`: a new variant is a compile error HERE until it is classified,
/// which is the event that forces the grid-coverage decision a new
/// block-creating op needs — the grid's own match is exhaustive over `Arm`
/// only, so without this classifier an `Op` change would compile everywhere
/// in this file (review run 6, P3-1, first link).
fn arm_of(op: &Op) -> Option<Arm> {
    match op {
        Op::Alloc { .. } => Some(Arm::Alloc),
        Op::AllocZeroed { .. } => Some(Arm::AllocZeroed),
        Op::Realloc { .. } => Some(Arm::Realloc),
        Op::Dealloc(_) => None,
    }
}

/// The ONE grid cell for `(arm, shape)`. The match is exhaustive over
/// `Arm × Shape`: a new variant on either axis is a compile error HERE until
/// its cells are written, and the `grid_axis!` definitions put every variant
/// in its `ALL` slice, so those cells are then replayed by the grid loop —
/// the coverage question is forced by the compiler instead of by the next
/// review round (`arm_of` is the matching link on the `Op` side). This is
/// the structural fix for review run 5's P3-1 meta-pattern: three
/// consecutive rounds each found a block-creating arm edited without
/// matching null/align coverage, and the `alloc_zeroed` null branch this
/// grid now covers was the one whose removal is undefined behaviour (a null
/// dereference in `verify_zeroed_block`), not just an unpinned message.
fn null_align_cell(arm: Arm, shape: Shape) -> NullAlignCase {
    match (arm, shape) {
        (Arm::Alloc, Shape::Null) => NullAlignCase {
            // Null with no arena contact: only M1's null check can fire, and
            // deleting it makes `fill_block(null, ..)` the first fault — the
            // cell then fails with a crash, not a message mismatch.
            fault: Fault::NullAlloc,
            ops: &[Op::Alloc { size: 32, align: 8 }],
            expect: Expect::Panics("M1: op #0 alloc(size=32, align=8) returned null"),
        },
        (Arm::Alloc, Shape::Misaligned) => NullAlignCase {
            // Nothing else is live, so the overlap oracle cannot fire in this
            // cell's place: the M1/M4 alloc align assert is the only thing
            // that can (and with it deleted the run completes cleanly).
            fault: Fault::MisalignedBy(1),
            ops: &[Op::Alloc { size: 32, align: 8 }],
            expect: Expect::Panics("M1/M4:"),
        },
        (Arm::AllocZeroed, Shape::Null) => NullAlignCase {
            // The soundness-critical cell (review run 5, P3-1): a null passes
            // the align check (0 % align == 0) and the overlap check (nothing
            // else is live), so deleting `drive`'s alloc_zeroed null branch
            // makes `verify_zeroed_block(null, ..)` dereference null — the
            // cell fails with a crash, never silently.
            fault: Fault::NullAllocZeroed,
            ops: &[Op::AllocZeroed { size: 32, align: 8 }],
            expect: Expect::Panics("M1: op #0 alloc_zeroed(size=32, align=8) returned null"),
        },
        (Arm::AllocZeroed, Shape::Misaligned) => NullAlignCase {
            // Twin of the (Alloc, Misaligned) cell: single op, nothing else
            // live, so only the M1/M4 alloc_zeroed align assert can fire.
            fault: Fault::MisalignedZeroedBy(1),
            ops: &[Op::AllocZeroed { size: 32, align: 8 }],
            expect: Expect::Panics("M1/M4:"),
        },
        (Arm::Realloc, Shape::Null) => NullAlignCase {
            // The realloc-null SKIP is this cell's behaviour under test: both
            // null reallocs (one per live block) must be skipped with the
            // bookkeeping left coherent — both blocks re-verified at the
            // run-end sweep and freed exactly once at teardown. Deleting the
            // skip dereferences the null in `verify_prefix_block` (crash).
            fault: Fault::NullRealloc,
            ops: &[
                Op::Alloc { size: 64, align: 8 },
                Op::Realloc {
                    i: 0,
                    new_size: 128,
                }, // null: skipped, block 0 stays live
                Op::Alloc { size: 32, align: 8 },
                Op::Realloc { i: 1, new_size: 16 }, // null again, on the second block
            ],
            expect: Expect::Completes,
        },
        (Arm::Realloc, Shape::Misaligned) => NullAlignCase {
            // Offset 9 against an align-8 block: a misaligned pointer the
            // M1/M4 realloc align assert rejects BEFORE the overlap check and
            // the prefix read run. With that assert deleted the realloc
            // completes cleanly (the copy lands in bounds, the prefix is
            // preserved), so only that assert can make this cell pass.
            fault: Fault::ReallocAt(9),
            ops: &[
                Op::Alloc { size: 64, align: 8 },
                Op::Realloc { i: 0, new_size: 64 },
            ],
            expect: Expect::Panics("M1/M4:"),
        },
    }
}

/// The exhaustive `{Alloc, AllocZeroed, Realloc} × {null, misaligned}`
/// negative-oracle grid: every block-creating arm's null check and align
/// assert must fire on its fault, and the realloc-null skip must hold. One
/// run asserts all six cells and names the failing cell; each panicking
/// cell's `Faulty` (and its arena) still drops during the unwind, so the
/// file stays leak-free under miri's default leak check.
#[test]
fn null_and_align_oracles_fire_for_every_block_creating_arm() {
    for &arm in Arm::ALL {
        for &shape in Shape::ALL {
            let case = null_align_cell(arm, shape);
            // Cell/stream consistency (review run 6, P3-1): the cell must
            // actually issue its own arm's block-creating op — a stream that
            // never reaches the arm under test would make the cell vacuous.
            assert!(
                case.ops.iter().filter_map(arm_of).any(|a| a == arm),
                "grid cell {arm:?}x{shape:?}: op stream never issues its own arm's op"
            );
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                drive(&faulty(4096, case.fault), Config::default(), case.ops);
            }));
            match case.expect {
                Expect::Panics(prefix) => {
                    let err = result.err().unwrap_or_else(|| {
                        panic!(
                            "grid cell {arm:?}x{shape:?}: drive COMPLETED; expected a \
                             panic matching {prefix:?}"
                        )
                    });
                    let msg = panic_message(&*err);
                    assert!(
                        msg.contains(prefix),
                        "grid cell {arm:?}x{shape:?}: panic message does not name the \
                         expected oracle\n  expected substring: {prefix:?}\n  actual: {msg}"
                    );
                }
                Expect::Completes => {
                    if let Err(err) = result {
                        panic!(
                            "grid cell {arm:?}x{shape:?}: drive must complete (the \
                             null-realloc skip), but it panicked: {}",
                            panic_message(&*err)
                        );
                    }
                }
            }
        }
    }
}

/// Extract a caught panic payload's message. Both observed shapes are
/// covered: a plain `panic!("literal")` arrives as a `&'static str`, and
/// `drive`'s formatted `panic!("... {x}")` arrives as a `String` (both
/// probe-verified on rustc 1.97.0; toolchains have swapped which shape a
/// formatted panic carries across releases, so both arms stay). Call sites
/// pass `&*err`: with `err: Box<dyn Any + Send>` that is a
/// `&(dyn Any + Send)` pointing at the payload, which is exactly what the
/// two downcast arms below search.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Run `drive` and require its entire panic message to equal `expected`.
fn assert_drive_panic_eq(fault: Fault, ops: &[Op], expected: &str) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drive(&faulty(4096, fault), Config::default(), ops);
    }));
    let err = match result {
        Ok(()) => panic!("drive completed; expected panic {expected:?}"),
        Err(err) => err,
    };
    assert_eq!(panic_message(&*err), expected);
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
#[should_panic(
    expected = "lost prefix byte 1 (preserved 64 of old 64 -> new 128): read 0xb7, \
                expected 0xca (pattern fill 0x01, offset 1)"
)]
fn realloc_repeat_first_byte_is_caught_by_pattern() {
    // The review-run-2 P3-1 counterexample: the new range is filled with
    // old[0] instead of copying. Under the old uniform fill every checked
    // byte read back the expected constant and this allocator PASSED; the
    // pattern check fires at byte 1 — byte 0 still happens to match
    // (pattern_byte(1, 0) = the copied old[0]) — with read 0xb7 = old[0]
    // where pattern_byte(1, 1) = 0xca is expected.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc {
            i: 0,
            new_size: 128,
        },
    ];
    drive(
        &faulty(4096, Fault::ReallocRepeatFirstByte),
        Config::default(),
        &ops,
    );
}

#[test]
#[should_panic(
    expected = "lost prefix byte 0 (preserved 64 of old 64 -> new 128): read 0xca, \
                expected 0xb7 (pattern fill 0x01, offset 0)"
)]
fn realloc_shifted_copy_is_caught_by_pattern() {
    // The prefix lands shifted left by one (tail patched so no read leaves
    // the initialized range). A uniform block is invariant under this fault
    // — under the old fill every byte still read back the one expected
    // constant — so only the position-dependent pattern sees it: byte 0
    // reads 0xca (= pattern_byte(1, 1) = old[1]) where pattern_byte(1, 0)
    // = 0xb7 is expected.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc {
            i: 0,
            new_size: 128,
        },
    ];
    drive(
        &faulty(4096, Fault::ReallocShiftLeft),
        Config::default(),
        &ops,
    );
}

#[test]
#[should_panic(
    expected = "lost prefix byte 0 (preserved 64 of old 64 -> new 128): read 0xfc, \
                expected 0xb7 (pattern fill 0x01, offset 0)"
)]
fn realloc_permuted_copy_is_caught_by_pattern() {
    // The prefix is copied REVERSED — a permutation. Any permutation of a
    // uniform block is indistinguishable from the original, so the old fill
    // could not see this fault; the pattern fires at byte 0 (read
    // 0xfc = pattern_byte(1, 63) = the reversed-in old[63], where
    // pattern_byte(1, 0) = 0xb7 is expected).
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc {
            i: 0,
            new_size: 128,
        },
    ];
    drive(
        &faulty(4096, Fault::ReallocPermute),
        Config::default(),
        &ops,
    );
}

#[test]
#[should_panic(expected = "lost prefix byte")]
fn realloc_from_other_block_shifted_is_caught_by_pattern() {
    // Review run 3, P3-1: under the OLD `fill.wrapping_add(offset as u8)`
    // pattern, `pattern(1, o+1) == pattern(2, o)` for EVERY `o` — a defective
    // realloc that copies from a FOREIGN block (A, fill 1) one byte past its
    // start, instead of from the real source block (B, fill 2), reproduced
    // B's entire expected pattern and passed undetected. The mixed-hash
    // pattern must catch this.
    let ops = [
        Op::Alloc { size: 65, align: 8 },   // A: op #0, fill 1
        Op::Alloc { size: 64, align: 8 },   // B: op #1, fill 2
        Op::Realloc { i: 1, new_size: 64 }, // targets B (live = [A, B], 1 % 2 == 1)
    ];
    drive(
        &faulty(4096, Fault::ReallocFromForeignBlockPlusOne),
        Config::default(),
        &ops,
    );
}

#[test]
fn pattern_fill_passes_honest_grow_realloc() {
    // Positive control for the pattern oracles: an honest grow realloc must
    // complete, and the fake's memory must afterwards hold the NEW fill
    // identifier's pattern across the whole grown extent (drive re-fills it
    // after the prefix check). Block 0 got fill 1; the realloc re-fill got
    // fill 2. The bump arena never frees, so the grown block lands at arena
    // offset 64 (right after the abandoned 64-byte old block, whose bytes
    // still hold fill 1's pattern).
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc {
            i: 0,
            new_size: 128,
        },
    ];
    let alloc = faulty(4096, Fault::Honest);
    drive(&alloc, Config::default(), &ops);
    for off in 0..128 {
        assert_eq!(
            alloc.arena_byte(64 + off),
            pattern_byte(2, off),
            "grown extent byte {off}"
        );
    }
    for off in 0..64 {
        assert_eq!(
            alloc.arena_byte(off),
            pattern_byte(1, off),
            "abandoned old block byte {off}"
        );
    }
}

#[test]
fn pattern_fill_passes_honest_shrink_realloc() {
    // The shrink twin: after an honest shrink to 32 bytes, the fresh block
    // (at arena offset 64, the bump arena never frees) holds fill 2's
    // pattern over its surviving 32 bytes; the abandoned old block (offsets
    // 0..64) still holds the fill-1 pattern, which nothing re-verifies and
    // no oracle may false-positive on.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc { i: 0, new_size: 32 },
    ];
    let alloc = faulty(4096, Fault::Honest);
    drive(&alloc, Config::default(), &ops);
    for off in 0..32 {
        assert_eq!(
            alloc.arena_byte(64 + off),
            pattern_byte(2, off),
            "shrunk extent byte {off}"
        );
    }
    for off in 0..64 {
        assert_eq!(
            alloc.arena_byte(off),
            pattern_byte(1, off),
            "abandoned old block byte {off}"
        );
    }
}

#[test]
fn pattern_fill_passes_in_place_realloc() {
    // The in-place shape from `in_place_realloc_inside_own_old_block_passes`
    // (new extent = old extent shifted 8 bytes right, same size) under the
    // pattern: must complete, the new extent must hold fill 2's pattern, and
    // the 8 leading bytes of the old extent (outside the new one) must be
    // untouched fill-1 pattern.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc { i: 0, new_size: 64 },
    ];
    let alloc = faulty(4096, Fault::ReallocAt(8));
    drive(&alloc, Config::default(), &ops);
    for b in 0..64 {
        assert_eq!(
            alloc.arena_byte(8 + b),
            pattern_byte(2, b),
            "in-place extent byte {b}"
        );
    }
    for b in 0..8 {
        assert_eq!(
            alloc.arena_byte(b),
            pattern_byte(1, b),
            "old extent prefix byte {b}"
        );
    }
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
#[should_panic(expected = "M3: step #2 before dealloc: live block clobbered")]
fn corruption_cannot_escape_through_dealloc() {
    // Red before the targeted pre-dealloc verification: op 1 corrupts block
    // 0, then op 2 removes it from `live`, leaving nothing for the run-end
    // sweep to inspect except the untouched second block.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Alloc { size: 32, align: 8 },
        Op::Dealloc(0),
    ];
    drive(
        &faulty(4096, Fault::ClobberOnLaterAlloc),
        Config::default(),
        &ops,
    );
}

#[test]
#[should_panic(expected = "M3: step #2 before realloc: live block clobbered")]
fn corrupt_suffix_cannot_escape_through_shrink_realloc() {
    // Red before the targeted pre-realloc verification: only bytes 32..64
    // are corrupt, while the shrink preserves bytes 0..16 and discards the
    // corrupt suffix. Prefix verification and the run-end sweep then pass.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Alloc { size: 32, align: 8 },
        Op::Realloc { i: 0, new_size: 16 },
    ];
    drive(
        &faulty(4096, Fault::ClobberSuffixOnLaterAlloc),
        Config::default(),
        &ops,
    );
}

#[test]
#[should_panic(expected = "M3: step #1 teardown: live block clobbered before dealloc")]
fn teardown_dealloc_cannot_corrupt_a_later_survivor_unobserved() {
    // Red before per-survivor teardown verification: the run-end sweep sees
    // both fills intact, then freeing block 0 corrupts byte 0 of block 1.
    // Without an immediate check before block 1's free, the run completes.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Alloc { size: 32, align: 8 },
    ];
    drive(
        &faulty(4096, Fault::ClobberAtOnDealloc(64)),
        Config::default(),
        &ops,
    );
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
    // a near-`isize::MAX` request and exhausts), never rejected by the harness
    // with the old `Layout::from_size_align(size=..., align=...) rejected`
    // panic. Before the fix this test failed with that harness-rejection
    // message instead.
    let ops = [Op::Alloc {
        size: usize::MAX,
        align: 8,
    }];
    drive(&faulty(4096, Fault::Honest), Config::default(), &ops);
}

#[test]
#[should_panic(expected = "arena exhausted")]
fn oversized_realloc_new_size_is_clamped_not_rejected() {
    // The realloc arm's twin of `oversized_size_is_clamped_not_rejected`
    // (review run 5, P4-3 — the one clamp direction still without a test): a
    // hand-built `new_size: usize::MAX` must be CLAMPED into the admissible
    // ceiling and reach the allocator (here: the fake arena, which
    // legitimately cannot serve a near-`isize::MAX` request and exhausts),
    // never rejected by the harness with `Faulty::realloc`'s own upper-bound
    // precondition assert — that assert is `GlobalAlloc::realloc`'s contract,
    // and `drive`'s clamp is what upholds it. Deleting the clamp fails this
    // test with the "GlobalAlloc precondition violated" message instead.
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::Realloc {
            i: 0,
            new_size: usize::MAX,
        },
    ];
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
#[should_panic(expected = "M3: step #1 run-end sweep: live block clobbered")]
fn fill_op_survives_swap_remove_reindexing() {
    // Review run 3, P4-3: after `Dealloc(0)` removes A via `swap_remove`, B
    // (allocated at op #1) is swapped into `live[0]`. A regression that
    // reported the SURVIVOR'S POSITION instead of its stored `fill_op` would
    // misname this failure "step #0" (A's original op index, now B's stale
    // position) instead of the correct "step #1" (the op that actually
    // allocated B). `ClobberAtOnDealloc(64)` corrupts the byte at absolute
    // arena offset 64 (B's first byte: A occupies [0..64), align 8, so B
    // lands at exactly offset 64) as a side effect of ANY dealloc call —
    // here, freeing A at op #2.
    let ops = [
        Op::Alloc { size: 64, align: 8 }, // A: op #0, fill 1
        Op::Alloc { size: 32, align: 8 }, // B: op #1, fill 2, arena[64..96)
        Op::Dealloc(0),                   // frees A (index 0); corrupts B's byte 0
    ];
    drive(
        &faulty(4096, Fault::ClobberAtOnDealloc(64)),
        Config::default(),
        &ops,
    );
}

#[test]
#[should_panic(expected = "M3: step #2 run-end sweep: live block clobbered")]
fn fill_op_is_updated_by_realloc_not_left_at_the_original_alloc() {
    // Review run 3, P4-3's second requested case: `fill_op` must track the
    // MOST RECENT op that (re)filled a block, not just its original
    // allocation. After the realloc at op #2, block A' (position 0 in
    // `live`) carries fill_op 2, distinct from its position 0. To ALSO rule
    // out position tracking surviving a LATER reindex, the stream adds a
    // second block C and deallocs it at op #3, forcing a `swap_remove` that
    // does not touch A' itself. `ClobberAtOnDealloc` corrupts A' at its real
    // post-realloc arena offset (72: A at [0..64), C at [64..72), the
    // realloc's fresh honest bump starts from cursor 72) as a side effect of
    // freeing C.
    let ops = [
        Op::Alloc { size: 64, align: 8 },   // A: op #0, fill 1, arena[0..64)
        Op::Alloc { size: 8, align: 8 },    // C: op #1, fill 2, arena[64..72)
        Op::Realloc { i: 0, new_size: 64 }, // op #2: reallocs A -> A', fill 3, fill_op 2, arena[72..136)
        Op::Dealloc(1),                     // frees C (1 % 2 == 1); corrupts A' byte 0
    ];
    drive(
        &faulty(4096, Fault::ClobberAtOnDealloc(72)),
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
fn clamped_down_null_alloc_names_oom_note() {
    // Pins the clamp-DOWN null message from a simulated null-returning
    // allocator: `oversized_size_is_clamped_not_rejected` pins only the fake
    // arena's own "arena exhausted" failure, so the OOM-note message itself
    // had no content pin. `Fault::NullAlloc` returns null before touching
    // the arena, so the huge clamped size never exhausts anything and the
    // M1 message is what fires. Its exact numbers are derived from this
    // target's pointer width instead of embedding a 64-bit `usize::MAX`.
    let ops = [Op::Alloc {
        size: usize::MAX,
        align: 8,
    }];
    let clamped = (isize::MAX as usize / 8) * 8;
    let expected = format!(
        "M1: op #0 alloc(size={clamped} [clamped from {}], align=8) returned null — note: \
         the harness does not model OOM, so a hand-built size beyond the allocator's real \
         capacity reports here",
        usize::MAX
    );
    assert_drive_panic_eq(Fault::NullAlloc, &ops, &expected);
}

#[test]
#[should_panic(
    expected = "M1: op #0 alloc(size=1 [clamped from 0 — GlobalAlloc forbids a zero-size layout], align=8) returned null"
)]
fn clamped_up_null_alloc_gets_no_oom_note() {
    // Pins the clamp-UP null message (review run 4, P3-1): a null for a
    // size clamped UP from 0 means a 1-byte allocation failed. That is not
    // proof of a `GlobalAlloc` violation — the trait permits null for any
    // reason — but this harness's deliberately strict POLICY is that any
    // null from `alloc` fails the run, so the message must carry the
    // zero-layout
    // bracket and NO "harness does not model OOM" note (the pre-fix shared
    // message blamed the allocator for the harness's own rewrite). The pin
    // matches the new bracket text, which the old message lacks, so
    // reverting the message split fails this test.
    let ops = [Op::Alloc { size: 0, align: 8 }];
    drive(&faulty(4096, Fault::NullAlloc), Config::default(), &ops);
}

#[test]
#[should_panic(
    expected = "M1: op #0 alloc_zeroed(size=1 [clamped from 0 — GlobalAlloc forbids a zero-size layout], align=8) returned null"
)]
fn clamped_up_null_alloc_zeroed_gets_no_oom_note() {
    // The alloc_zeroed twin of `clamped_up_null_alloc_gets_no_oom_note`
    // (review run 5, P3-1: round 4 pinned the message split only in the
    // alloc arm, leaving the alloc_zeroed copy unreachable by any test).
    // Same division of labour: the pin matches the new zero-layout bracket,
    // which the pre-split shared message lacks.
    let ops = [Op::AllocZeroed { size: 0, align: 8 }];
    drive(
        &faulty(4096, Fault::NullAllocZeroed),
        Config::default(),
        &ops,
    );
}

#[test]
fn clamped_down_null_alloc_zeroed_names_oom_note() {
    // The alloc_zeroed twin of `clamped_down_null_alloc_names_oom_note`:
    // `Fault::NullAllocZeroed` returns null before touching the arena, so
    // the huge clamped size never exhausts anything and the M1 DOWN message
    // is what fires; its exact numbers are computed for the target width.
    let ops = [Op::AllocZeroed {
        size: usize::MAX,
        align: 8,
    }];
    let clamped = (isize::MAX as usize / 8) * 8;
    let expected = format!(
        "M1: op #0 alloc_zeroed(size={clamped} [clamped from {}], align=8) returned null — \
         note: the harness does not model OOM, so a hand-built size beyond the allocator's \
         real capacity reports here",
        usize::MAX
    );
    assert_drive_panic_eq(Fault::NullAllocZeroed, &ops, &expected);
}

// Review run 7, P3-1: `validate_align` is load-bearing — it is the only thing
// standing between a hand-built `Op` and an internal divide-by-zero (`align`
// 0), a raw `Layout::from_size_align` rejection (non-power-of-two), or an
// `Ord::clamp` min>max assertion (round-up overflow) — and it runs at a
// SEPARATE call site in each block-creating arm, so each arm's site carries
// its own deletion risk (the same per-arm asymmetry runs 5 and 6 each found
// once). Every test below pins the named rejection and is counterfactual:
// deleting the arm's `validate_align(align, op_idx)` call changes its panic
// into the internal-arithmetic failure named in its own comment (each
// failure mode is re-derivable directly from `validate_align`'s and the
// surrounding clamp's arithmetic in `drive.rs`; review run 8 independently
// re-derived and confirmed all four).

#[test]
#[should_panic(expected = "op #0: align 0 is not a usable Layout alignment")]
fn zero_align_is_rejected_not_divided_by() {
    // Without the guard, the very next line divides by this align:
    // `(isize::MAX as usize / align) * align` — "attempt to divide by zero".
    // This also pins the guard's ORDER: before any size is clamped against a
    // ceiling derived from the align.
    let ops = [Op::Alloc { size: 32, align: 0 }];
    drive(&faulty(4096, Fault::Honest), Config::default(), &ops);
}

#[test]
#[should_panic(expected = "op #0: align 3 is not a usable Layout alignment")]
fn non_power_of_two_align_is_rejected_not_layout_matched() {
    // Without the guard the panic degrades to the raw
    // "Layout::from_size_align(size=32, align=3) rejected" message — the
    // message-quality failure the totality design deliberately moved away from.
    let ops = [Op::Alloc { size: 32, align: 3 }];
    drive(&faulty(4096, Fault::Honest), Config::default(), &ops);
}

#[test]
fn overflowing_align_is_rejected_not_clamped_into_a_panic() {
    // The top `usize` bit: without the guard,
    // `(isize::MAX as usize / align) * align` evaluates to 0, so
    // `size.clamp(1, 0)` trips `Ord::clamp`'s min <= max assertion — an
    // internal arithmetic panic with no op index and no explanation.
    let align = 1usize << (usize::BITS - 1);
    let ops = [Op::Alloc { size: 32, align }];
    let expected = format!(
        "op #0: align {align} is not a usable Layout alignment (must be a non-zero power \
         of two whose round-up fits isize)"
    );
    assert_drive_panic_eq(Fault::Honest, &ops, &expected);
}

#[test]
#[should_panic(expected = "op #0: align 0 is not a usable Layout alignment")]
fn zero_align_alloc_zeroed_is_rejected_not_divided_by() {
    // The alloc_zeroed arm's own `validate_align` call site — a separate
    // invocation from the alloc arm's, with its own deletion risk.
    let ops = [Op::AllocZeroed { size: 32, align: 0 }];
    drive(&faulty(4096, Fault::Honest), Config::default(), &ops);
}
