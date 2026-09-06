//! The differential-testing loop: replays an op stream against a
//! [`RawAllocator`] and a reference model, asserting the M1–M4 oracles.

use alloc::vec::Vec;
use core::alloc::Layout;

use crate::config::Config;
use crate::op::Op;
use crate::raw_allocator::RawAllocator;

/// A live allocation in the reference model: its pointer, size, align, and the
/// fill byte written across the whole block (so M3 contamination is detectable).
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
    /// The fill byte written over the whole block.
    fill: u8,
}

/// Whether `[a, a+asize)` and `[b, b+bsize)` overlap. Touching endpoints are
/// adjacent, NOT overlapping. Saturating adds keep an overflowing end from
/// wrapping around into a false "no overlap".
#[must_use]
fn ranges_overlap(a: usize, asize: usize, b: usize, bsize: usize) -> bool {
    !(a.saturating_add(asize) <= b || b.saturating_add(bsize) <= a)
}

/// The next fill byte: always in `1..=255` (0 is reserved so a fill can never
/// be confused with zeroed memory), cycling.
fn next_fill(cycle: &mut u8) -> u8 {
    let current = *cycle;
    *cycle = cycle.wrapping_add(1).max(1);
    current
}

/// Build the `Layout` for an op's size/align pair, naming the op index if the
/// pair is rejected (align 0 or non-power-of-two, or an overflowing size).
fn layout_for(size: usize, align: usize, op_idx: usize) -> Layout {
    Layout::from_size_align(size, align).unwrap_or_else(|e| {
        panic!("op #{op_idx}: Layout::from_size_align(size={size}, align={align}) rejected: {e}")
    })
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

/// Write `byte` across the whole block.
///
/// # Safety
/// `ptr` must be valid for writes of `size` bytes.
unsafe fn fill_block(ptr: *mut u8, size: usize, byte: u8) {
    // SAFETY: caller guarantees `ptr` is valid for writes of `size` bytes.
    unsafe { core::ptr::write_bytes(ptr, byte, size) }
}

/// Read the fill back, byte by byte, naming the oracle/op/offset on mismatch.
///
/// # Safety
/// `ptr` must be valid for reads of `size` bytes AND fully initialized — only
/// call immediately after [`fill_block`].
unsafe fn verify_block(ptr: *mut u8, size: usize, byte: u8, oracle: &str, step: usize, what: &str) {
    // SAFETY: caller guarantees `ptr` is valid for reads of `size` bytes and
    // fully initialized (just filled).
    let block = unsafe { core::slice::from_raw_parts(ptr, size) };
    if let Some(off) = block.iter().position(|&read| read != byte) {
        panic!(
            "{oracle}: step #{step} {what}: {ptr:p} (size {size}): byte {off} read {:#04x}, \
             expected {byte:#04x}",
            block[off]
        );
    }
}

/// Check every byte of an `alloc_zeroed` block reads as 0.
///
/// Uses raw single-byte reads, NOT a slice: a broken allocator's
/// `alloc_zeroed` may return never-written (uninitialized) memory, and a
/// slice over uninitialized memory would make the harness itself unsound.
///
/// # Safety
/// `ptr` must be valid for reads of `size` bytes.
unsafe fn verify_zeroed_block(ptr: *mut u8, size: usize, op_idx: usize) {
    for b in 0..size {
        // SAFETY: caller guarantees `ptr` is valid for reads of `size` bytes.
        let read = unsafe { ptr.add(b).read() };
        assert_eq!(
            read, 0,
            "alloc_zeroed: op #{op_idx}: {ptr:p} (size {size}): byte {b} read {read:#04x}, \
             expected 0"
        );
    }
}

/// Check the preserved `min(old, new)` realloc prefix byte by byte.
///
/// Raw reads (not a slice): a broken realloc may return never-written memory —
/// same justification as [`verify_zeroed_block`].
///
/// # Safety
/// `ptr` must be valid for reads of `len` bytes.
unsafe fn verify_prefix_block(
    ptr: *mut u8,
    len: usize,
    expected: u8,
    op_idx: usize,
    old_size: usize,
    new_size: usize,
) {
    for b in 0..len {
        // SAFETY: caller guarantees `ptr` is valid for reads of `len` bytes.
        let read = unsafe { ptr.add(b).read() };
        assert!(
            read == expected,
            "realloc: op #{op_idx}: {ptr:p} lost prefix byte {b} (preserved {len} of old \
             {old_size} -> new {new_size}): read {read:#04x}, expected {expected:#04x}"
        );
    }
}

/// Run the op stream against `alloc` and the reference model, asserting the
/// M1–M4 oracles on every step. Panics (the natural oracle-failure signal for
/// both proptest and libFuzzer) the moment any oracle is violated.
///
/// All survivors are freed and the model dropped before returning (no UAF in a
/// teardown walk). `config.double_free` selects whether the M2
/// double-free-is-no-op oracle is exercised (off by default — a real malloc
/// would corrupt); the other `config` fields shape the generators, not `drive`.
///
/// `drive` is total over every hand-built `Op` value: sizes of `0`, `new_size`
/// of `0`, and overflowing `new_size`s are clamped into the range
/// `GlobalAlloc`'s own contract permits (P0-1), so the allocator is never
/// invoked outside its documented preconditions.
///
/// # Reentrancy
///
/// `drive` allocates its own bookkeeping (one `Vec<Live>`) through the
/// *global* allocator. If the allocator under test is also the installed
/// `#[global_allocator]`, those internal allocations interleave with the ops
/// under test and a reentrant allocator will deadlock or recurse — drive the
/// *engine* behind your `GlobalAlloc`, not the installed global allocator
/// itself.
///
/// # Panics
///
/// On any oracle violation:
///
/// - a null from `alloc`/`alloc_zeroed` (M1; the harness does not model OOM,
///   so any such null is a failure — a null from `realloc` is the documented
///   failure signal and is instead skipped, leaving the old block live),
/// - a misaligned pointer (M1/M4),
/// - a live-block overlap (M3, incremental or at run end),
/// - a byte that does not read back (M1),
/// - a non-zero byte from `alloc_zeroed`,
/// - a lost realloc prefix byte.
///
/// Every message names the op index and its operands. Also panics when an op
/// carries a size/align pair `Layout::from_size_align` rejects (align 0 or
/// non-power-of-two, or an overflowing size).
pub fn drive<A: RawAllocator>(alloc: &A, config: Config, ops: &[Op]) {
    // Every entry in `live` traces to exactly one `Op::Alloc`/`Op::AllocZeroed`
    // in `ops` that hasn't since been freed, so `live.len() <= ops.len()`
    // always holds -- `ops.len()` is therefore an exact, provably-sufficient
    // upper bound, not an estimate. Pre-sizing to it makes `live` grow-once
    // (or never grow at all) for the whole call, rather than reallocating
    // repeatedly as the op stream is replayed.
    let mut live: Vec<Live> = Vec::with_capacity(ops.len());
    let mut cycle: u8 = 1;

    for (op_idx, op) in ops.iter().enumerate() {
        match *op {
            Op::Alloc { size, align } => {
                // P0-1: `GlobalAlloc` requires a non-zero layout size; clamp
                // before building the `Layout` so a hand-built `size: 0` op
                // never reaches the allocator outside its contract.
                let size = size.max(1);
                let layout = layout_for(size, align, op_idx);
                // SAFETY: `layout` is valid; the returned pointer is checked for
                // null and used only for `size` bytes, as the contract permits.
                let ptr = unsafe { alloc.alloc(layout) };
                assert!(
                    !ptr.is_null(),
                    "M1: op #{op_idx} alloc(size={size}, align={align}) returned null"
                );
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
                });
            }
            Op::AllocZeroed { size, align } => {
                // P0-1: clamp zero sizes, as in the `Alloc` arm.
                let size = size.max(1);
                let layout = layout_for(size, align, op_idx);
                // SAFETY: `layout` valid; pointer checked for null, used only for
                // `size` bytes.
                let ptr = unsafe { alloc.alloc_zeroed(layout) };
                assert!(
                    !ptr.is_null(),
                    "M1: op #{op_idx} alloc_zeroed(size={size}, align={align}) returned null"
                );
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
                // contamination checks have a marker.
                unsafe { fill_block(ptr, size, fill) };
                live.push(Live {
                    ptr,
                    size,
                    align,
                    fill,
                });
            }
            Op::Dealloc(i) => {
                if !live.is_empty() {
                    let i = i % live.len();
                    let l = live.swap_remove(i);
                    let layout = layout_for(l.size, l.align, op_idx);
                    // SAFETY: `l.ptr` is a live block allocated with `layout`,
                    // freed exactly once here (the swap_remove drops it from the
                    // model), honoring the `dealloc` contract.
                    unsafe { alloc.dealloc(l.ptr, layout) };
                    if config.double_free {
                        // M2: a second dealloc of the same pointer must be a
                        // no-op that does not corrupt the allocator. Opt-in
                        // (`Config::double_free`) — a stronger-than-`GlobalAlloc`
                        // guarantee; a real malloc would corrupt here.
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
                    let old_layout = layout_for(l.size, l.align, op_idx);
                    // P0-1: `GlobalAlloc::realloc` requires `new_size > 0` and
                    // its round-up to `old_layout.align()` not to overflow
                    // `isize`; clamp into that range. An oversized `new_size`
                    // then simply OOM-fails to null, which `drive` already
                    // tolerates as realloc failure.
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
                    // The preserved prefix must still hold the old fill byte.
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
                    };
                }
            }
        }
    }

    // M3 at run end: every survivor still holds its own fill (no block was
    // silently clobbered by another live allocation).
    for (block_idx, l) in live.iter().enumerate() {
        // SAFETY: `l.ptr` is live and valid for `l.size` bytes.
        unsafe {
            verify_block(
                l.ptr,
                l.size,
                l.fill,
                "M3",
                block_idx,
                "run-end sweep: live block clobbered",
            )
        };
    }

    // Free all survivors (the model drops right after — M2: no double-free,
    // no UAF in a teardown walk).
    for l in &live {
        let layout = Layout::from_size_align(l.size, l.align)
            .expect("teardown: every model layout was validated when its block was created");
        // SAFETY: `l.ptr` is a live block allocated with `layout`, freed exactly
        // once here.
        unsafe { alloc.dealloc(l.ptr, layout) };
    }
}
