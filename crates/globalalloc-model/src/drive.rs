//! The differential-testing loop: replays an op stream against a
//! [`RawAllocator`] and a reference model, asserting the M1–M4 oracles.

use alloc::vec::Vec;
use core::alloc::Layout;

use crate::config::Config;
use crate::op::Op;
use crate::peak_live_count::peak_live_count;
use crate::raw_allocator::RawAllocator;

/// A live allocation in the reference model: its pointer, size, align, and the
/// fill identifier whose [`pattern_byte`] pattern covers the whole block (so
/// M3 contamination is detectable).
///
/// Deliberately private: publishing a struct carrying an owned allocation
/// handle (let alone a `Send` impl) promises more than the model provides —
/// moving such a handle across threads is exactly what is unsound for a
/// per-thread-cached allocator.
#[derive(Clone, Copy, Debug)]
struct Live {
    /// The pointer returned by the allocator.
    ptr: *mut u8,
    /// The allocation's size in bytes.
    size: usize,
    /// The allocation's alignment.
    align: usize,
    /// The fill identifier whose [`pattern_byte`] pattern covers the whole block.
    fill: u8,
    /// The op index whose fill the block carries: the op that allocated it,
    /// or the realloc that re-filled it. Reported by the run-end sweep and
    /// teardown messages, where a position in the surviving vector would
    /// shift with `swap_remove` and identify nothing.
    fill_op: usize,
}

/// Whether `[a, a+asize)` and `[b, b+bsize)` overlap. Touching endpoints are
/// adjacent, NOT overlapping. Saturating adds keep an overflowing end from
/// wrapping around into a false "no overlap".
#[must_use]
fn ranges_overlap(a: usize, asize: usize, b: usize, bsize: usize) -> bool {
    !(a.saturating_add(asize) <= b || b.saturating_add(bsize) <= a)
}

/// The next fill identifier: always in `1..=255` (0 is reserved so the
/// identifier — and with it the byte at offset 0 of every block — can never
/// be confused with zeroed memory), cycling.
fn next_fill(cycle: &mut u8) -> u8 {
    let current = *cycle;
    *cycle = cycle.wrapping_add(1).max(1);
    current
}

/// The per-byte expected value for a block carrying fill identifier `fill`:
/// `fill.wrapping_add(offset as u8)` — position-dependent, so a wrong copy
/// algorithm can no longer reproduce a whole block by repeating one byte
/// (an adversarial `realloc` that fills the new range with `old[0]`, or that
/// shifts or permutes the copied prefix, now reads back wrong at almost every
/// offset). The value is computed on the fly from `(fill, offset)` alone; no
/// second copy of a block's expected contents is stored.
///
/// Residual limitation, stated honestly: the pattern has period 256 in the
/// offset (`offset as u8` truncates), so a corruption that shifts or permutes
/// bytes by an EXACT multiple of 256 positions can still collide with the
/// expected values, and marker reuse after 255 fill assignments (see the
/// crate-level limits section) still applies — the scheme is not airtight.
fn pattern_byte(fill: u8, offset: usize) -> u8 {
    fill.wrapping_add(offset as u8)
}

/// Build the `Layout` for an op's size/align pair, naming the op index if the
/// pair is rejected (align 0 or non-power-of-two, or an overflowing size).
fn layout_for(size: usize, align: usize, op_idx: usize) -> Layout {
    Layout::from_size_align(size, align).unwrap_or_else(|e| {
        panic!("op #{op_idx}: Layout::from_size_align(size={size}, align={align}) rejected: {e}")
    })
}

/// Reject an op align that `Layout` can never admit — zero, not a power of
/// two, or one whose round-up overflows `isize` — naming the op index.
/// Unlike a size, an align cannot be clamped into range, so this stays a
/// harness rejection (see `drive`'s `# Panics` section). Called before any
/// size is clamped against a ceiling derived from the align.
fn validate_align(align: usize, op_idx: usize) {
    if Layout::from_size_align(1, align).is_err() {
        panic!(
            "op #{op_idx}: align {align} is not a usable Layout alignment \
             (must be a non-zero power of two whose round-up fits isize)"
        );
    }
}

/// M3 incremental check: no two live blocks share a byte.
///
/// Two roles, both load-bearing:
///
/// 1. **Oracle** — no two simultaneously-live allocations may overlap.
/// 2. **Soundness** — `drive`'s teardown frees every `live` entry exactly
///    once, which is only sound if `live` holds no duplicate pointer; a broken
///    allocator returning an already-live pointer must panic HERE rather than
///    be pushed twice and double-freed at teardown.
///
/// `skip` excludes the entry being replaced by a `realloc` (its own old
/// extent legitimately overlaps the new one when the realloc was in place).
fn assert_no_overlap(live: &[Live], skip: Option<usize>, ptr: *mut u8, size: usize, op_idx: usize) {
    let start = ptr.addr();
    let end = start.saturating_add(size);
    for (i, other) in live.iter().enumerate() {
        if skip == Some(i) {
            continue;
        }
        let other_start = other.ptr.addr();
        let other_end = other_start.saturating_add(other.size);
        assert!(
            !ranges_overlap(start, size, other_start, other.size),
            "M3: op #{op_idx}: new block [{start:#x}..{end:#x}) overlaps live block {other:?} \
             [{other_start:#x}..{other_end:#x})"
        );
    }
}

/// Write the [`pattern_byte`] pattern for `fill` across the whole block.
///
/// This used to be one `write_bytes` call over a uniform byte; it is now a
/// per-byte pointer-write loop because the value varies by offset. That is a
/// diagnostics-for-cost tradeoff (stronger copy-algorithm oracles for a
/// slower fill) whose wall-clock cost has NOT been measured — no performance
/// claim is made here.
///
/// # Safety
/// `ptr` must be valid for writes of `size` bytes.
unsafe fn fill_block(ptr: *mut u8, size: usize, fill: u8) {
    for offset in 0..size {
        // SAFETY: caller guarantees `ptr` is valid for writes of `size`
        // bytes; `offset < size` keeps every write in range.
        unsafe { ptr.add(offset).write(pattern_byte(fill, offset)) };
    }
}

/// Read the block's [`pattern_byte`] fill back, byte by byte, naming the
/// oracle/op/offset on mismatch. The expected value varies by offset (it is
/// `pattern_byte(fill, offset)`), so the message names the offset-derived
/// expectation and the fill identifier it derives from.
///
/// # Safety
/// `ptr` must remain valid for reads of `size` bytes, and every byte in that
/// range must be initialized. The fill may have happened earlier: callers
/// rely on the [`RawAllocator`] contract keeping a live block valid and on
/// [`fill_block`] having initialized its whole modeled extent.
unsafe fn verify_block(ptr: *mut u8, size: usize, fill: u8, oracle: &str, step: usize, what: &str) {
    // SAFETY: caller guarantees `ptr` is valid for reads of `size` fully
    // initialized bytes in one live allocation.
    let block = unsafe { core::slice::from_raw_parts(ptr, size) };
    for (off, &read) in block.iter().enumerate() {
        let expected = pattern_byte(fill, off);
        assert!(
            read == expected,
            "{oracle}: step #{step} {what}: {ptr:p} (size {size}): byte {off} read \
             {read:#04x}, expected {expected:#04x} (pattern fill {fill:#04x} + offset {off})"
        );
    }
}

/// Check every byte of an `alloc_zeroed` block reads as 0.
///
/// Reads raw single bytes, NOT a slice: the failure message can name the
/// exact failing byte offset, and no reference to the whole block is
/// materialized. That is a message-quality choice, not a soundness one —
/// a read of memory the allocator never initialized is undefined through
/// either form. The definedness of these reads rests on exactly one thing:
/// the [`RawAllocator`](crate::RawAllocator) contract, which makes a
/// non-null return valid for reads of `layout.size()` bytes AND (for
/// `alloc_zeroed`) obliges those bytes to be *initialized* — whether they
/// are zero is the oracle checked here; initializedness is the obligation.
/// An allocator that hands back genuinely uninitialized memory violates the
/// obligation: the read is UB both natively and under Miri; a diagnostic is
/// not guaranteed (see the crate-level `Safety and oracle limits` section). What the read
/// is FOR is the zero oracle: a broken `alloc_zeroed` that hands back
/// bytes which are not 0 is reported here, never tolerated.
///
/// # Safety
/// `ptr` must be valid for reads of `size` fully initialized bytes. For a
/// non-null `alloc_zeroed` result, initializedness is a [`RawAllocator`]
/// safety guarantee; whether each byte is zero is the oracle checked here.
unsafe fn verify_zeroed_block(ptr: *mut u8, size: usize, op_idx: usize) {
    for b in 0..size {
        // SAFETY: caller guarantees `ptr` is valid for reads of `size` fully
        // initialized bytes.
        let read = unsafe { ptr.add(b).read() };
        assert_eq!(
            read, 0,
            "alloc_zeroed: op #{op_idx}: {ptr:p} (size {size}): byte {b} read {read:#04x}, \
             expected 0"
        );
    }
}

/// Check the preserved `min(old, new)` realloc prefix byte by byte against
/// the old block's [`pattern_byte`] pattern. The expected value varies by
/// offset; the message names the offset-derived expectation.
///
/// Raw reads (not a slice): same rationale as [`verify_zeroed_block`] —
/// the message names the exact lost byte, and the reads' definedness rests
/// on the `RawAllocator` contract's initialization obligation for `realloc`'s
/// first `min(old, new)` bytes (see the crate-level limits section), not on
/// the read form.
///
/// # Safety
/// `ptr` must be valid for reads of `len` fully initialized bytes. A non-null
/// realloc result's preserved prefix has that initializedness guarantee under
/// [`RawAllocator`]; equality with the offset-derived pattern is the oracle
/// checked here.
unsafe fn verify_prefix_block(
    ptr: *mut u8,
    len: usize,
    fill: u8,
    op_idx: usize,
    old_size: usize,
    new_size: usize,
) {
    for b in 0..len {
        // SAFETY: caller guarantees `ptr` is valid for reads of `len` fully
        // initialized bytes.
        let read = unsafe { ptr.add(b).read() };
        let expected = pattern_byte(fill, b);
        assert!(
            read == expected,
            "realloc: op #{op_idx}: {ptr:p} lost prefix byte {b} (preserved {len} of old \
             {old_size} -> new {new_size}): read {read:#04x}, expected {expected:#04x} \
             (pattern fill {fill:#04x} + offset {b})"
        );
    }
}

/// Run the op stream against `alloc` and the reference model, asserting the
/// M1–M4 oracles at allocation and block-lifecycle observation points.
/// Panics on detected violations, the failure signal for proptest and libFuzzer.
///
/// All survivors are freed and the model dropped before returning (no UAF in a
/// teardown walk). `config.double_free` — `Some(DoubleFreeOk::new())` vs
/// `None` — selects whether the M2 double-free-is-no-op oracle is exercised
/// (off by default — a real malloc would corrupt; the enabling token is
/// unforgeable in safe code); the other `config` fields shape the
/// generators, not `drive`.
///
/// `drive` normalizes sizes in hand-built `Op` values: a size of `0`, an
/// oversized size, a `new_size` of `0`, and a `new_size` whose round-up
/// overflows `isize` are all clamped into the range `GlobalAlloc`'s own
/// contract permits, so the allocator is never invoked outside its
/// documented preconditions. The one input that cannot be clamped is an
/// op's align: a never-admissible align panics as a harness rejection
/// (see the `# Panics` section).
///
/// # Reentrancy
///
/// `drive` allocates its own bookkeeping (one `Vec<Live>`) through the
/// *global* allocator. If it is also the allocator under test, bookkeeping
/// allocations interleave with the modeled operations. Calling this driver
/// from inside an allocation hook can recurse or deadlock; invoke it from
/// ordinary test code and use a separate global allocator for bookkeeping
/// when isolation is needed.
///
/// After an oracle panic, outstanding tested allocations are not freed by
/// this driver: invalid or overlapping allocator results cannot be reclaimed
/// generically with confidence. Reclaim a test arena or discard the test
/// process before replaying failing cases repeatedly.
///
/// # Panics
///
/// On any oracle violation:
///
/// - a null from `alloc`/`alloc_zeroed` (M1; the harness does not model OOM,
///   so any such null is a failure — a null from `realloc` is the documented
///   failure signal and is instead skipped, leaving the old block live),
/// - a misaligned pointer (M1/M4),
/// - a live-block overlap or clobbered fill (M3, incrementally, before a
///   destructive op, at run end, or during teardown),
/// - a byte that does not read back (M1),
/// - a non-zero byte from `alloc_zeroed`,
/// - a lost realloc prefix byte.
///
/// Every message names the op being applied and its operands. During the live
/// replay that is the op's index in `ops`; the run-end sweep and the teardown
/// walk instead name the index of the op whose fill the surviving block
/// carries (the op that allocated it or last realloc'd it) — after
/// intervening deallocations that is deliberately NOT the block's position in
/// the surviving vector, which `swap_remove` shifts and which identifies
/// nothing. Also panics when an op
/// carries an align `Layout` can never admit (zero, non-power-of-two, or one
/// whose round-up overflows `isize`): unlike sizes, an align cannot be
/// clamped into range.
pub fn drive<A: RawAllocator>(alloc: &A, config: Config, ops: &[Op]) {
    // `live` never holds more than the peak number of simultaneously-live
    // blocks the stream can reach: every entry traces to exactly one
    // `Op::Alloc`/`Op::AllocZeroed` that hasn't since been freed. Reserving
    // that peak (see [`peak_live_count`]) rather than the whole history
    // length makes `live` grow-once (or never grow at all) for the whole
    // call, without paying for ops that only ever freed.
    let mut live: Vec<Live> = Vec::with_capacity(peak_live_count(ops));
    let mut cycle: u8 = 1;

    for (op_idx, op) in ops.iter().enumerate() {
        match *op {
            Op::Alloc { size, align } => {
                // Review run 2, P2-4 (extending review run 1's P0-1
                // zero-clamp): `GlobalAlloc` requires a non-zero layout size
                // whose round-up to `align` does not overflow `isize`.
                // Reject a never-admissible align FIRST, naming the op (an
                // align cannot be clamped into range); then clamp the size
                // into the admissible ceiling — the same
                // `(isize::MAX / align) * align` bound the realloc arm
                // clamps to — so a hand-built op of ANY size stays inside
                // the allocator's contract (the "total over every
                // hand-built `Op`" promise).
                validate_align(align, op_idx);
                let original_size = size;
                let size = size.clamp(1, (isize::MAX as usize / align) * align);
                // Infallible after the two steps above.
                let layout = layout_for(size, align, op_idx);
                // SAFETY: `layout` is valid; the returned pointer is checked for
                // null and used only for `size` bytes, as the contract permits.
                let ptr = unsafe { alloc.alloc(layout) };
                if ptr.is_null() {
                    if size > original_size {
                        // Clamped UP from 0: `GlobalAlloc` forbids a
                        // zero-size layout, so the request that reached the
                        // allocator was the minimal 1-byte one. A null here
                        // is NOT proof of a `GlobalAlloc` violation — the
                        // trait permits null for any reason — but this
                        // harness's deliberately strict POLICY is that any
                        // null from `alloc` fails the run, so the message
                        // carries NO OOM note (review run 4, P3-1; wording
                        // Sol-codex run 2, P4-5).
                        panic!(
                            "M1: op #{op_idx} alloc(size={size} [clamped from {original_size} — \
                             GlobalAlloc forbids a zero-size layout], align={align}) returned null"
                        );
                    }
                    if size < original_size {
                        // Clamped DOWN from something absurd (e.g.
                        // `usize::MAX`): a null is the expected outcome for
                        // a size no real allocator can serve, so the note
                        // explains the harness artifact rather than blaming
                        // the allocator.
                        panic!(
                            "M1: op #{op_idx} alloc(size={size} [clamped from {original_size}], \
                             align={align}) returned null — note: the harness does not model \
                             OOM, so a hand-built size beyond the allocator's real capacity \
                             reports here"
                        );
                    }
                    panic!("M1: op #{op_idx} alloc(size={size}, align={align}) returned null");
                }
                assert_eq!(
                    ptr.addr() % align,
                    0,
                    "M1/M4: op #{op_idx} alloc(size={size}, align={align}) returned {ptr:p}, \
                     which is not {align}-aligned"
                );
                assert_no_overlap(&live, None, ptr, size, op_idx);
                let fill = next_fill(&mut cycle);
                // SAFETY: `ptr` is non-null and valid for `size` bytes (just
                // allocated for `layout`); we write then read those bytes only.
                unsafe {
                    fill_block(ptr, size, fill);
                    verify_block(ptr, size, fill, "M1", op_idx, "alloc");
                }
                live.push(Live {
                    ptr,
                    size,
                    align,
                    fill,
                    fill_op: op_idx,
                });
            }
            Op::AllocZeroed { size, align } => {
                // Same two-step clamp as the `Alloc` arm.
                validate_align(align, op_idx);
                let original_size = size;
                let size = size.clamp(1, (isize::MAX as usize / align) * align);
                // Infallible after the two steps above.
                let layout = layout_for(size, align, op_idx);
                // SAFETY: `layout` valid; pointer checked for null, used only for
                // `size` bytes.
                let ptr = unsafe { alloc.alloc_zeroed(layout) };
                if ptr.is_null() {
                    if size > original_size {
                        // Clamped UP from 0: same split as the `Alloc` arm
                        // — no OOM note for a 1-byte request.
                        panic!(
                            "M1: op #{op_idx} alloc_zeroed(size={size} [clamped from {original_size} — \
                             GlobalAlloc forbids a zero-size layout], align={align}) returned null"
                        );
                    }
                    if size < original_size {
                        // Clamped DOWN: same message shape as the `Alloc`
                        // arm above.
                        panic!(
                            "M1: op #{op_idx} alloc_zeroed(size={size} [clamped from {original_size}], \
                             align={align}) returned null — note: the harness does not model \
                             OOM, so a hand-built size beyond the allocator's real capacity \
                             reports here"
                        );
                    }
                    panic!(
                        "M1: op #{op_idx} alloc_zeroed(size={size}, align={align}) returned null"
                    );
                }
                assert_eq!(
                    ptr.addr() % align,
                    0,
                    "M1/M4: op #{op_idx} alloc_zeroed(size={size}, align={align}) returned \
                     {ptr:p}, which is not {align}-aligned"
                );
                assert_no_overlap(&live, None, ptr, size, op_idx);
                // SAFETY: `ptr` valid for `size` freshly-zeroed bytes.
                unsafe { verify_zeroed_block(ptr, size, op_idx) };
                let fill = next_fill(&mut cycle);
                // SAFETY: `ptr` valid for `size` bytes; re-fill so later M3
                // contamination checks have a marker, then read the fill back
                // — the same M1 write-read-back the `alloc` arm does (review
                // run 3, P3-1: the zero-check alone proves readability and the
                // fill proves writability; only the read-back proves the write
                // persisted).
                unsafe {
                    fill_block(ptr, size, fill);
                    verify_block(ptr, size, fill, "M1", op_idx, "alloc_zeroed");
                }
                live.push(Live {
                    ptr,
                    size,
                    align,
                    fill,
                    fill_op: op_idx,
                });
            }
            Op::Dealloc(i) => {
                if !live.is_empty() {
                    let i = i % live.len();
                    let l = live[i];
                    // A later allocator operation may have clobbered this
                    // block. Verify its full modeled extent before removing
                    // the only record that can expose that corruption.
                    // SAFETY: `l.ptr` is still live and all `l.size` bytes
                    // were initialized by the most recent fill.
                    unsafe {
                        verify_block(
                            l.ptr,
                            l.size,
                            l.fill,
                            "M3",
                            op_idx,
                            "before dealloc: live block clobbered",
                        )
                    };
                    let l = live.swap_remove(i);
                    let layout = layout_for(l.size, l.align, op_idx);
                    // SAFETY: `l.ptr` is a live block allocated with `layout`,
                    // freed exactly once here (the swap_remove drops it from the
                    // model), honoring the `dealloc` contract.
                    unsafe { alloc.dealloc(l.ptr, layout) };
                    if config.double_free.is_some() {
                        // M2: a second dealloc of the same pointer must be a
                        // no-op that does not corrupt the allocator. Opt-in
                        // via `Config::double_free: Some(DoubleFreeOk::new())`
                        // — a stronger-than-`GlobalAlloc` guarantee; a real
                        // malloc would corrupt here. The `Option<DoubleFreeOk>`
                        // field type makes the enabling value unforgeable in
                        // safe code (review run 7, P0-1).
                        // SAFETY: intentional M2 exercise — for an allocator whose
                        // documented contract is that this is a no-op.
                        unsafe { alloc.dealloc(l.ptr, layout) };
                    }
                }
            }
            Op::Realloc { i, new_size } => {
                if !live.is_empty() {
                    let i = i % live.len();
                    let l = live[i];
                    // Verify the whole old block before realloc can consume
                    // it. This must precede shrinking too: prefix validation
                    // cannot observe corruption in a discarded suffix.
                    // SAFETY: `l.ptr` is still live and all `l.size` bytes
                    // were initialized by the most recent fill.
                    unsafe {
                        verify_block(
                            l.ptr,
                            l.size,
                            l.fill,
                            "M3",
                            op_idx,
                            "before realloc: live block clobbered",
                        )
                    };
                    let old_layout = layout_for(l.size, l.align, op_idx);
                    // P0-1: `GlobalAlloc::realloc` requires `new_size > 0` and
                    // its round-up to `old_layout.align()` not to overflow
                    // `isize`; clamp into that range. An oversized `new_size`
                    // then simply OOM-fails to null, which `drive` already
                    // tolerates as realloc failure.
                    //
                    // The upward end is deliberately the maximal admissible
                    // size, not a smaller "sane" cap — any lower bound would
                    // be arbitrary and would mask a genuine growth request —
                    // so a bogus near-`usize::MAX` `new_size` becomes a
                    // near-`isize::MAX` REQUEST. An allocator is expected to
                    // answer that with null (tolerated here as documented
                    // realloc failure); one that ABORTS on OOM instead aborts
                    // by its own OOM policy, which is not an oracle failure.
                    // The alloc arms clamp to the same ceiling.
                    let new_size = new_size.clamp(
                        1,
                        (isize::MAX as usize / old_layout.align()) * old_layout.align(),
                    );
                    // SAFETY: `l.ptr` is a live block allocated with `old_layout`;
                    // it is consumed on a non-null return per the realloc contract.
                    let new_ptr = unsafe { alloc.realloc(l.ptr, old_layout, new_size) };
                    if new_ptr.is_null() {
                        // Realloc failed: the old block is still live & valid.
                        continue;
                    }
                    assert_eq!(
                        new_ptr.addr() % l.align,
                        0,
                        "M1/M4: op #{op_idx} realloc(old_size={}, new_size={new_size}, \
                         align={}) returned {new_ptr:p}, which is not {}-aligned",
                        l.size,
                        l.align,
                        l.align
                    );
                    // skip = Some(i): the new extent may legitimately overlap
                    // the OLD block (legal in-place realloc), but must not
                    // touch any OTHER live block. Checked before any
                    // read/write through `new_ptr` (soundness: a duplicate
                    // pointer must panic here, not be double-freed at teardown).
                    assert_no_overlap(&live, Some(i), new_ptr, new_size, op_idx);
                    let keep = l.size.min(new_size);
                    // The preserved prefix must still hold the old fill's position-dependent pattern.
                    // SAFETY: `new_ptr` is valid for `new_size >= keep` bytes.
                    unsafe { verify_prefix_block(new_ptr, keep, l.fill, op_idx, l.size, new_size) };
                    // Re-establish a fresh fill across the whole new extent so
                    // M3 contamination checks and later reallocs stay coherent
                    // (the grown tail is legitimately uninitialised otherwise).
                    let fill = next_fill(&mut cycle);
                    // SAFETY: `new_ptr` is valid for `new_size` bytes.
                    unsafe {
                        fill_block(new_ptr, new_size, fill);
                        verify_block(new_ptr, new_size, fill, "M1", op_idx, "realloc");
                    }
                    live[i] = Live {
                        ptr: new_ptr,
                        size: new_size,
                        align: l.align,
                        fill,
                        fill_op: op_idx,
                    };
                }
            }
        }
    }

    // M3 at run end: every survivor still holds its own fill (no block was
    // silently clobbered by another live allocation).
    for l in live.iter() {
        // SAFETY: `l.ptr` is live and valid for `l.size` bytes.
        unsafe {
            verify_block(
                l.ptr,
                l.size,
                l.fill,
                "M3",
                l.fill_op,
                "run-end sweep: live block clobbered",
            )
        };
    }

    // Free all survivors (the model drops right after — M2: no double-free,
    // no UAF in a teardown walk).
    for l in live.iter() {
        // Each survivor is checked immediately before its own free: an
        // earlier teardown dealloc may have corrupted a later survivor after
        // the run-end sweep already passed.
        // SAFETY: `l.ptr` remains live until the dealloc below, and all
        // `l.size` bytes were initialized by the most recent fill.
        unsafe {
            verify_block(
                l.ptr,
                l.size,
                l.fill,
                "M3",
                l.fill_op,
                "teardown: live block clobbered before dealloc",
            )
        };
        let layout = Layout::from_size_align(l.size, l.align)
            .expect("teardown: every model layout was validated when its block was created");
        // SAFETY: `l.ptr` is a live block allocated with `layout`, freed exactly
        // once here.
        unsafe { alloc.dealloc(l.ptr, layout) };
    }
}
