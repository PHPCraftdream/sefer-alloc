// R21-1 shared single-hot-buffer workload for the `paired_ab_hot_buffer_off` /
// `paired_ab_hot_buffer_on` process-level A/B/B/A judge binaries.
//
// ## Why this file exists (and why it is `include!`d, not a real module)
//
// `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §5.3/§6.1 point 2/§8 step 2
// identified that the EXISTING medium-classes gate harness
// (`examples/_shared/paired_ab_medium_workload.rs`, used by R10-2/R18-2/R20-2)
// is deliberately adversarial — 16 SIMULTANEOUSLY-LIVE objects, chosen
// specifically to force Large-cache misses and expose the baseline's
// Large-path cost cleanly. That harness's own design doc (§5.2 of the R20-3
// document) proves it is structurally the WRONG instrument for evaluating the
// proposed OPT-H in-place-medium-grow mechanism (`docs/perf/
// R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §2): with 16 objects competing for
// tail-of-segment adjacency, at most one object per segment is ever eligible
// for OPT-H's tail-adjacency precondition at a time, so that harness's own
// hit-rate would look artificially small no matter how correct the mechanism
// is.
//
// THIS file is the direct realization of the "single-hot-buffer" workload
// profile R10-2 §5 itself named (*"Buffer construction (grow to target, then
// operate)"*) and R20-3 §6.1 point 2 specifies concretely: ONE buffer,
// repeatedly grown through the medium-class ladder, with NOTHING ELSE
// allocated concurrently in the timed region. Deliberately the opposite shape
// from `paired_ab_medium_workload.rs`'s `WS_LEN = 16` adversarial design — see
// that file's module doc for the contrasting rationale.
//
// `include!` (not a shared crate module) is used for exactly the same reason
// `paired_ab_medium_workload.rs` / `paired_ab_workload.rs` document: Cargo
// examples are independent compilation units with no shared `examples/`-
// support crate in this project, and duplicating the workload body across two
// wrappers would risk the two-binaries-silently-drift-apart failure mode.
// Both wrappers (`paired_ab_hot_buffer_off.rs`, `paired_ab_hot_buffer_on.rs`)
// `include!` this file verbatim, so the workload code is byte-for-byte
// identical in both binaries — the ONLY difference is which Cargo feature set
// the binary was compiled with.
//
// ## What this measures
//
// - **Setup phase (untimed):** allocate ONE buffer at 256 KiB (the smallest
//   medium-class rung under `medium-classes`; without `medium-classes` this
//   size is well below the ~4 MiB Large threshold too, so BOTH arms route it
//   through their respective non-Large paths at this size — see the module
//   doc of the `off`/`on` wrappers for the exact per-arm routing this file is
//   agnostic to).
// - **Realloc-grow phase (timed):** grow the SAME buffer, one `realloc` call
//   per rung, through every successive rung of the ladder — 256 -> 320 -> 384
//   -> 512 -> 768 -> 1024 KiB — then reset back down to 256 KiB (untimed) and
//   repeat for `ROUNDS` rounds. At every instant exactly ONE object is live;
//   no other allocation happens anywhere in the timed region.
// - **Teardown (untimed):** free the buffer once, at the end.
//
// This directly exercises `MEDIUM_REALLOC_PROMOTION_THRESHOLD`
// (`src/registry/heap_core_free.rs`, 256 KiB) under `medium-classes`: the
// very first grow step (256 -> 320 KiB, or immediately 256 -> 384 KiB
// depending on ladder walk) already crosses the promotion threshold on the
// FIRST round, so subsequent rounds repeat the promote-to-Large-then-shrink-
// back-to-256KiB-and-grow-again cycle every round. This is intentional: it is
// the same first-crossing promotion cost R10-2/R18-2/R20-2 measured, but now
// under the single-object tail-adjacency condition R20-3 §5.3 predicts OPT-H
// should actually help.
//
// ## Determinism
//
// No PRNG is needed: the size sequence is a fixed walk over the six medium
// classes, identical in both arms by construction (same `include!`d source).
//
// ## Reset-to-base between rounds: why realloc back down, not free+realloc
//
// Reallocating back down to `REALLOC_BASE` (rather than freeing and
// re-allocating a fresh buffer) keeps the SAME pointer/block identity moving
// through the ladder across the entire realloc-grow phase where possible
// (mirroring the "one buffer, reused" contract this workload's name promises)
// and — under the baseline arm — exercises the Large path's own realloc-
// shrink handling identically every round, so the phase's per-round shape is
// uniform in both arms.

use std::alloc::Layout;
use std::hint::black_box;
use std::time::Instant;

// ---------------------------------------------------------------------------
// RSS / commit-charge probes — thin KiB wrappers over the `proc-probe` crate's
// re-export of `proc-memstat`'s same-instant `snapshot()` (bytes). Identical
// to `paired_ab_medium_workload.rs`'s wrappers — kept here so this file is
// self-contained when `include!`d.
// ---------------------------------------------------------------------------

fn rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

fn commit_kib() -> u64 {
    proc_probe::snapshot().commit / 1024
}

// ---------------------------------------------------------------------------
// Constants — the single-buffer ladder walk
// ---------------------------------------------------------------------------

const KIB: usize = 1024;

/// The six exact medium-class sizes (`src/alloc_core/size_classes.rs` EXTRAS
/// block, `medium-classes` arm) — 256 / 320 / 384 / 512 / 768 KiB / 1 MiB.
/// Confirmed against that file directly (R21-1). The ladder-walk order below
/// grows through every rung in ascending order, exactly mirroring
/// `paired_ab_medium_workload.rs`'s `REALLOC_STEPS` shape but starting from,
/// and returning to, the smallest rung each round.
const MEDIUM_SIZES: &[usize] = &[
    256 * KIB,
    320 * KIB,
    384 * KIB,
    512 * KIB,
    768 * KIB,
    1024 * KIB,
];

/// The buffer's starting (and per-round reset) size: the smallest medium
/// class. A realistic starting size for a growable buffer that will be
/// realloc-grown into the medium range.
const REALLOC_BASE: usize = MEDIUM_SIZES[0];

/// The grow-step sequence within one round: every rung AFTER the base, in
/// ascending order — 320 -> 384 -> 512 -> 768 -> 1024 KiB. One `realloc` call
/// per rung, exactly mirroring `paired_ab_medium_workload.rs`'s per-step
/// shape (that harness's `REALLOC_STEPS` also grows through successive rungs
/// one call at a time), but here there is only ONE buffer, not `WS_LEN`.
/// Written as a literal (not a `MEDIUM_SIZES[1..]` slice) because const slice
/// indexing is not yet stable; kept in exact lockstep with `MEDIUM_SIZES`'s
/// tail by construction (both are hand-written from the same
/// `size_classes.rs` EXTRAS values, verified identical at review time).
const GROW_STEPS: &[usize] = &[320 * KIB, 384 * KIB, 512 * KIB, 768 * KIB, 1024 * KIB];

/// Rounds: each round is one full grow-through-the-ladder (timed) followed by
/// an untimed reset back to `REALLOC_BASE`. Chosen so the timed phase's total
/// wall-clock is comfortably multi-millisecond without inflating per-process
/// launch time — same rationale as `paired_ab_medium_workload.rs::ROUNDS`.
const ROUNDS: usize = 20;

/// Alignment for the buffer. Matches `paired_ab_medium_workload.rs`.
const ALIGN: usize = 8;

/// Sentinel write value (same pattern as `paired_ab_medium_workload.rs`).
const TOUCH: u64 = 0xA5A5_A5A5_A5A5_A5A5;

// ---------------------------------------------------------------------------
// Allocation primitives — direct `std::alloc` calls routed through the
// installed `#[global_allocator]` (SeferAlloc in both arms). Byte-for-byte
// the same primitives as `paired_ab_medium_workload.rs` (kept duplicated here
// rather than factored into a second shared file, per this workload file's
// own `include!`-verbatim self-containment convention).
// ---------------------------------------------------------------------------

/// Allocate `size` bytes, write the first 16 bytes (volatile), return the
/// pointer. Panics on allocation failure (the probe is invalid if alloc
/// fails).
fn alloc_one(size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `layout` has non-zero size and valid (power-of-two, <=
    // usize::MAX/2) alignment (8), satisfying `GlobalAlloc::alloc`'s
    // preconditions.
    let p = unsafe { std::alloc::alloc(layout) };
    assert!(!p.is_null(), "alloc({size}) failed — probe is invalid");
    touch16(p);
    p
}

/// Free a pointer allocated with `alloc_one(size)` / `realloc_one(_, size)`.
fn dealloc_one(p: *mut u8, size: usize) {
    if p.is_null() {
        return;
    }
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `p` was allocated with this exact `layout` (same size, same
    // align) by `alloc_one` or `realloc_one`, and is freed exactly once.
    unsafe { std::alloc::dealloc(p, layout) };
}

/// Realloc `p` from `old_size` to `new_size` (grow OR shrink). Writes the
/// first 8 bytes of the result (volatile) to fence the realloc. Panics on
/// failure.
fn realloc_one(p: *mut u8, old_size: usize, new_size: usize) -> *mut u8 {
    let old_layout = Layout::from_size_align(old_size, ALIGN).unwrap();
    // SAFETY: `p` was allocated with `old_layout` (or grown/shrunk to
    // `old_size` by a prior `realloc_one`), and `new_size > 0`, satisfying
    // `GlobalAlloc::realloc`'s preconditions.
    let q = unsafe { std::alloc::realloc(p, old_layout, new_size) };
    assert!(
        !q.is_null(),
        "realloc({old_size} -> {new_size}) failed — probe is invalid"
    );
    // SAFETY: `q` is a freshly allocated/relocated block of at least 8 bytes
    // (every size in this workload is >= 256 KiB); `write_volatile` prevents
    // the store (and thus the realloc) being optimized away.
    unsafe { std::ptr::write_volatile(q.cast::<u64>(), TOUCH) };
    q
}

/// Write the first 16 bytes (two u64 words) via `write_volatile` — proves the
/// allocation is real and touches the first cache line.
/// SAFETY: caller guarantees `p` points to at least 16 writable bytes (every
/// size in this workload is >= 256 KiB).
fn touch16(p: *mut u8) {
    unsafe {
        std::ptr::write_volatile(p.cast::<u64>(), TOUCH);
        std::ptr::write_volatile(p.cast::<u64>().add(1), TOUCH);
    }
}

// ---------------------------------------------------------------------------
// The single-hot-buffer phase — grows the ONE live buffer through every rung
// of `GROW_STEPS`, updating `cur` as it goes. The caller times this
// function's wall-clock; this is the ONLY thing timed in this workload.
// ---------------------------------------------------------------------------

/// Grow `p` (currently `cur` bytes) through every step in `GROW_STEPS` in
/// order, returning the final `(ptr, size)`. Exactly one live object for the
/// whole call — no other allocation happens here.
fn grow_through_ladder(p: *mut u8, cur: usize) -> (*mut u8, usize) {
    let mut p = p;
    let mut cur = cur;
    for &step in GROW_STEPS {
        p = realloc_one(p, cur, step);
        cur = step;
    }
    black_box(p);
    (p, cur)
}

// ---------------------------------------------------------------------------
// The workload driver — called by both wrapper binaries' `main`. Returns
// `(elapsed_ns, realloc_ns)` so the wrapper can emit one `RESULT` line per
// timed region. `elapsed_ns` and `realloc_ns` are identical here (there is
// only one timed phase in this workload — unlike
// `paired_ab_medium_workload.rs`'s three-phase alloc/free/realloc split,
// this harness measures ONLY the realloc-grow region by design, per R20-3
// §6.1 point 2's "nothing else allocated concurrently in the timed region"
// requirement — there is no separate alloc/free phase to time because the
// setup/teardown alloc and final dealloc are each a single untimed call).
//
// Structure: ONE buffer is allocated (untimed setup). Then, per round: grow
// through `GROW_STEPS` (timed), then reset back to `REALLOC_BASE` via one
// more `realloc` call (untimed) so the next round starts from the same base
// size. After `ROUNDS` rounds, free the buffer once (untimed teardown).
// ---------------------------------------------------------------------------

pub fn run_hot_buffer_workload() -> (u128, u128) {
    let mut realloc_ns: u128 = 0;

    // Untimed setup: allocate the ONE buffer at the base size.
    let mut p = alloc_one(REALLOC_BASE);
    let mut cur = REALLOC_BASE;

    for _ in 0..ROUNDS {
        // Timed: grow through every rung of the ladder. Exactly one live
        // object; nothing else allocated concurrently.
        let t_realloc = Instant::now();
        let (grown_p, grown_size) = grow_through_ladder(p, cur);
        realloc_ns += t_realloc.elapsed().as_nanos();
        p = grown_p;
        cur = grown_size;

        // Untimed: reset back down to REALLOC_BASE so the next round starts
        // from the same base size. Still exactly one live object.
        p = realloc_one(p, cur, REALLOC_BASE);
        cur = REALLOC_BASE;
    }

    // Untimed teardown: free the buffer once.
    dealloc_one(p, cur);
    black_box(p);

    let elapsed_ns = realloc_ns;
    (elapsed_ns, realloc_ns)
}
