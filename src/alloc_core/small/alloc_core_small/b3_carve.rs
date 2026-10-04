//! B3/B2 carve helpers of the small path (mechanical split of
//! `alloc_core_small_impl.rs`; pure code movement, no behavior changed):
//! the sub-leaf carving primitives (`b3_next_leaf` … `b3_class_block_next_cursor`,
//! `B3BlockPlan`), the B3/B3p bench-internals oracles, and the B2
//! lazy-commit carve constants shared by `carve_block`/`carve_batch`.

use crate::alloc_core::os::SEGMENT;
#[cfg(all(feature = "bench-internals", feature = "internals"))]
use crate::alloc_core::os;
use crate::alloc_core::alloc_bitmap::AllocBitmap;
use crate::alloc_core::segment_header::{align_up, B3Free, B3_LEAF_BYTES, B3_LEAF_WORDS, B3_LEAVES};
#[cfg(all(feature = "bench-internals", feature = "internals"))]
use crate::alloc_core::segment_header::SegmentMeta;
use crate::alloc_core::size_classes::MIN_BLOCK;

/// B2 (R7 Workstream B): process-wide count of successful `commit_pages` calls
/// on the grow-on-carve path. Diagnostic-only (relaxed), gated on
/// `any(primordial-lazy-commit, small-segment-lazy-commit)` (R12-9, task
/// #260: the split siblings of the former single `alloc-lazy-commit`, which
/// is now a pure alias for "both together"). Tests observe this to verify
/// that `carve_batch` does ONE commit per batch (not per block) and that
/// chunk boundary crossings trigger the expected number of commits.
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
pub(crate) static GROW_COMMIT_COUNT: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// B1 (R7 Workstream B): the size of the FIRST committed payload chunk when a
/// small segment is lazily reserved under `small-segment-lazy-commit`, or
/// the primordial segment under `primordial-lazy-commit` (R12-9, task #260 —
/// this constant is SHARED verbatim by both split policies; see
/// `bootstrap::primordial`'s identical `LAZY_FIRST_CHUNK` use). Only the
/// metadata region `[0, small_meta_end)` plus this chunk are committed at
/// reservation time; the rest of the 4 MiB segment stays reserved-but-
/// uncommitted. B2's grow-on-carve logic commits additional chunks
/// (sized by [`GROW_CHUNK`]) as the bump cursor advances past the frontier.
///
/// 256 KiB is large enough to hold many initial carve batches without faulting
/// (the refill batch is 31 blocks; even at the largest small class of ~8 KiB
/// that is ~248 KiB, comfortably within one chunk). B5 will sweep this value
/// (64/128/256/512 KiB) against the first-heap commit judge; 256 KiB is the
/// conservative default.
///
/// The value MUST be a non-zero multiple of `aligned_vmem::PAGE` (4 KiB).
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
pub(crate) const LAZY_FIRST_CHUNK: usize = 256 * 1024;

/// B2 (R7 Workstream B): the chunk size used when GROWING the commit frontier
/// past its initial value during bump-carve. When a `carve_block` or
/// `carve_batch` would write past `committed_payload_end`, the grow logic
/// commits `[frontier, round_up(carve_end, GROW_CHUNK))` (clamped to
/// `SEGMENT`) and advances the frontier.
///
/// Set equal to `LAZY_FIRST_CHUNK` (256 KiB): both are B5-swept constants
/// (64/128/256/512 KiB); using the same value for initial and grow chunks
/// keeps the model simple and the sweep surface small. A separate value is
/// a trivial constant rename if B5 data motivates it.
///
/// The value MUST be a non-zero multiple of `aligned_vmem::PAGE` (4 KiB).
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
pub(crate) const GROW_CHUNK: usize = LAZY_FIRST_CHUNK;

// B1/B2: compile-time sanity — the initial commit (metadata + first chunk) must
// fit within one segment, and both chunk constants must be page-aligned and
// non-zero.
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
const _: () = {
    assert!(
        LAZY_FIRST_CHUNK > 0 && LAZY_FIRST_CHUNK.is_multiple_of(crate::alloc_core::os::PAGE),
        "LAZY_FIRST_CHUNK must be a non-zero multiple of PAGE"
    );
    // Task #1074: the page-rounded `lazy_initial_commit` adds at most one
    // MAX_REALISTIC_PAGE_SIZE (64 KiB) of rounding on top of the tight sum,
    // so the slack is part of the bound: this pins the ROUNDED initial
    // commit within one SEGMENT for every page size this crate supports.
    assert!(
        crate::alloc_core::segment_header::Layout::small_meta_end()
            + LAZY_FIRST_CHUNK
            + crate::alloc_core::os::MAX_REALISTIC_PAGE_SIZE
            <= crate::alloc_core::os::SEGMENT,
        "metadata + LAZY_FIRST_CHUNK + one MAX_REALISTIC_PAGE_SIZE of page-rounding \
         slack must fit within one SEGMENT"
    );
    assert!(
        GROW_CHUNK > 0 && GROW_CHUNK.is_multiple_of(crate::alloc_core::os::PAGE),
        "GROW_CHUNK must be a non-zero multiple of PAGE"
    );
};

/// B3 spike oracle (bench-internals): blocks served by the sub-leaf free
/// structure on the refill (drain) path.
#[cfg(feature = "bench-internals")]
pub(crate) static B3_DRAIN_FROM_FREE: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// B3 spike oracle: bench-internals reader for the drain-from-free counter.
#[cfg(all(feature = "bench-internals", feature = "internals"))]
#[doc(hidden)]
#[allow(dead_code)] // spike oracle reader (root fn forwards to the counter)
pub(crate) fn b3s_oracle_counter() -> u64 {
    B3_DRAIN_FROM_FREE.load(core::sync::atomic::Ordering::Relaxed)
}

/// B3p carve-waste oracle (bench-internals): per-class bytes TAKEN from the
/// shared bump frontier by fresh k-sub-leaf runs (k*4096 per new run).
/// Diagnostic-only (relaxed); the waste oracle subtracts issued bytes.
#[cfg(feature = "bench-internals")]
pub(crate) static B3P_RUN_BYTES: [core::sync::atomic::AtomicU64; crate::alloc_core::size_classes::SMALL_CLASS_COUNT] =
    [const { core::sync::atomic::AtomicU64::new(0) }; crate::alloc_core::size_classes::SMALL_CLASS_COUNT];

/// B3p oracle: bench-internals readers for the per-class run-byte counters
/// and the per-class run length.
#[cfg(all(feature = "bench-internals", feature = "internals"))]
#[doc(hidden)]
#[allow(dead_code)] // spike oracle readers (root fns forward to these)
pub(crate) fn b3p_run_bytes(class_idx: usize) -> u64 {
    B3P_RUN_BYTES[class_idx].load(core::sync::atomic::Ordering::Relaxed)
}

#[cfg(all(feature = "bench-internals", feature = "internals"))]
#[doc(hidden)]
#[allow(dead_code)]
pub(crate) fn b3p_run_leaves(class_idx: usize) -> u32 {
    crate::alloc_core::segment_header::B3Free::RUN_LEAVES[class_idx]
}

/// B3p oracle: leaf-to-class binding of the 4 KiB sub-leaf containing the
/// first byte of `ptr`'s segment offset (0 = unbound, else class_idx + 1).
#[cfg(all(feature = "bench-internals", feature = "internals"))]
#[doc(hidden)]
#[allow(dead_code)]
pub(crate) fn b3p_leaf_class_of(ptr: *mut u8) -> u8 {
    let base = os::segment_base_of_ptr(ptr);
    let off = ptr as usize - base as usize;
    SegmentMeta::new(base).b3_free().leaf_class(off / B3_LEAF_BYTES)
}

/// B3 spike: first sub-leaf of `class_idx` with a mask bit at/after `from`
/// (forward-only scan; `first_leaf` handles the cursor/wraparound entry).
#[inline(always)]
pub(super) fn b3_next_leaf(b3: &B3Free, class_idx: usize, from: usize) -> Option<usize> {
    debug_assert!(from <= B3_LEAVES);
    if from >= B3_LEAVES {
        return None;
    }
    let mut w = from / 64;
    let mut bits = b3.mask_word(class_idx, w) & (u64::MAX << (from % 64));
    loop {
        if bits != 0 {
            return Some(w * 64 + bits.trailing_zeros() as usize);
        }
        w += 1;
        if w >= B3_LEAVES / 64 {
            return None;
        }
        bits = b3.mask_word(class_idx, w);
    }
}

/// B3 spike: locate the first free slot of `class_idx` strictly after scan
/// position `pos` (`None` = the class-mask entry leaf). Returns
/// `(leaf, bitmap_word_idx, bit)`; never mutates anything.
#[inline(always)]
pub(super) fn b3_locate_free_from(
    bm: &AllocBitmap,
    b3: &B3Free,
    class_idx: usize,
    pos: &mut Option<(usize, usize, u32)>,
) -> Option<(usize, usize, u32)> {
    let (mut leaf, mut wi, mut min_bit) = match *pos {
        None => {
            let leaf = b3.first_leaf(class_idx)?;
            (leaf, leaf * B3_LEAF_WORDS, 0u32)
        }
        Some((leaf, wi, bit)) => (leaf, wi, bit.saturating_add(1)),
    };
    loop {
        let leaf_end = leaf * B3_LEAF_WORDS + B3_LEAF_WORDS;
        while wi < leaf_end {
            let word = if min_bit < 64 {
                bm.word(wi) & u64::MAX.unbounded_shl(min_bit)
            } else {
                0
            };
            if word != 0 {
                let bit = word.trailing_zeros();
                *pos = Some((leaf, wi, bit));
                return Some((leaf, wi, bit));
            }
            wi += 1;
            min_bit = 0;
        }
        leaf = b3_next_leaf(b3, class_idx, leaf + 1)?;
        wi = leaf * B3_LEAF_WORDS;
    }
}

/// B3 spike: the segment offset of a located slot.
#[inline(always)]
pub(super) fn b3_slot_offset(leaf: usize, wi: usize, bit: u32) -> u32 {
    ((leaf * 256 + (wi - leaf * B3_LEAF_WORDS) * 64 + bit as usize) * MIN_BLOCK) as u32
}

/// B3 spike: first carve start for `class_idx` at/after `start`, honouring
/// the sub-leaf→class binding (a carve never STARTS in a sub-leaf bound to
/// another class; blocks > 4 KiB span sub-leaves naturally). `None` = no room.
#[inline(always)]
pub(super) fn b3_carve_start(
    b3: &B3Free,
    class_idx: usize,
    block_size: usize,
    mut start: usize,
) -> Option<usize> {
    loop {
        if start + block_size > SEGMENT {
            return None;
        }
        let leaf = start / B3_LEAF_BYTES;
        let bound = b3.leaf_class(leaf);
        if bound != 0 && bound != class_idx as u8 + 1 {
            start = align_up((leaf + 1) * B3_LEAF_BYTES, block_size);
            continue;
        }
        return Some(start);
    }
}

/// B3 spike: per-class carve cursor word encoding. `0` = no run; otherwise
/// `(start << 2) | delta` where `start` is the offset of the class's next
/// fresh block (always nonzero: payload begins at `small_meta_end() > 0`)
/// and `delta = start / B3_LEAF_BYTES - run_start_leaf` (0..=3: the cursor
/// may sit in any of the run's `k <= 4` sub-leaves). The delta keeps the run
/// boundary (`run_end = (run_start_leaf + k) * B3_LEAF_BYTES`) recoverable
/// after the cursor crosses into the run's later sub-leaves — recovering it
/// from the bare `start` leaf would overestimate `run_end` there and let
/// blocks overlap the next run taken from the bump frontier.
#[inline(always)]
pub(super) fn b3_cursor_decode(v: u32) -> Option<(usize, usize)> {
    if v == 0 {
        return None;
    }
    let delta = (v & 3) as usize;
    let start = (v >> 2) as usize;
    Some((start, start / B3_LEAF_BYTES - delta))
}

#[inline(always)]
pub(super) fn b3_cursor_encode(start: usize, run_leaf: usize) -> u32 {
    let delta = start / B3_LEAF_BYTES - run_leaf;
    debug_assert!(delta <= 3, "carve cursor escaped its run");
    ((start as u32) << 2) | delta as u32
}

/// B3 spike: one planned fresh block of a `block_size <= B3_LEAF_BYTES`
/// class. `new_run` is `Some((run_start, run_end))` when a NEW run of
/// `RUN_LEAVES[class_idx]` sub-leaves must be taken from the shared bump
/// frontier; its leaves are bound and the bump advanced only AFTER every
/// fallible step (OOM-atomicity). Pure — no mutation — so the batch path's
/// probe/commit replay reproduces the identical sequence.
pub(super) struct B3BlockPlan {
    pub(super) off: usize,
    pub(super) new_run: Option<(usize, usize)>,
}

/// Serve from the current run while the block fits before `run_end` (the run
/// boundary is a block boundary); on exhaustion take a contiguous k-sub-leaf
/// run: `run_start = align_up(bump, B3_LEAF_BYTES)` (the alignment head
/// `[bump, run_start)` is lost — part of the documented fragmentation
/// budget; bump can be only MIN_BLOCK-aligned after a >4 KiB span carve),
/// `run_end = run_start + k*4096`, `start = align_up(run_start, block_size)`
/// (absolute `off % block_size == 0` preserved).
#[inline(always)]
pub(super) fn b3_class_block_plan(
    class_idx: usize,
    block_size: usize,
    bump: usize,
    cursor: u32,
) -> Option<B3BlockPlan> {
    let k = B3Free::RUN_LEAVES[class_idx] as usize;
    if let Some((start, run_leaf)) = b3_cursor_decode(cursor) {
        if start + block_size <= (run_leaf + k) * B3_LEAF_BYTES {
            return Some(B3BlockPlan { off: start, new_run: None });
        }
    }
    let run_start = align_up(bump, B3_LEAF_BYTES);
    let run_end = run_start + k * B3_LEAF_BYTES;
    if run_end > SEGMENT {
        return None; // no room for a new run — caller reserves
    }
    Some(B3BlockPlan {
        off: align_up(run_start, block_size),
        new_run: Some((run_start, run_end)),
    })
}

/// The packed cursor after the planned block is issued: advanced by one
/// block, zeroed once the run remainder is smaller than `block_size` (run
/// exhausted — the tail is part of the loss budget).
#[inline(always)]
pub(super) fn b3_class_block_next_cursor(
    class_idx: usize,
    plan: &B3BlockPlan,
    block_size: usize,
    cursor: u32,
) -> u32 {
    let k = B3Free::RUN_LEAVES[class_idx] as usize;
    let (run_leaf, run_end) = match plan.new_run {
        Some((run_start, run_end)) => (run_start / B3_LEAF_BYTES, run_end),
        None => match b3_cursor_decode(cursor) {
            Some((_, run_leaf)) => (run_leaf, (run_leaf + k) * B3_LEAF_BYTES),
            None => std::process::abort()
        },
    };
    let next = plan.off + block_size;
    if next + block_size <= run_end {
        b3_cursor_encode(next, run_leaf)
    } else {
        0
    }
}
