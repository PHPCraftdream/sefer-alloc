//! Layout pin (R34-18/task #537, release-stabilization finding F-6 [low]):
//! `size_of::<HeapCore>()` must stay within an 8 KiB stack-pressure budget.
//!
//! ## The risk this guards
//!
//! `HeapCore` is constructed BY VALUE on the stack of the frame that triggers
//! a thread's FIRST allocation — `HeapRegistry::claim` does
//! `HeapCore::new(idx) → heap_ptr.cast::<HeapCore>().write(hc)`
//! (`src/registry/heap_registry.rs`, both `claim` and `claim_with_config`),
//! and the process-global fallback does the same inside a
//! `MaybeUninit<HeapCore>` (`src/global/fallback.rs`). Rust does NOT guarantee
//! return-value/move elision: on a debug build, or any toolchain/backend that
//! materialises the temporary, that `HeapCore::new(..)` can place one ~7 KiB
//! copy on a first-allocation frame — often very early in a thread's life.
//! Threads with small stacks (embedded-class 16–64 KiB, or a constrained
//! thread pool) are a realistic deployment, so a single such temporary must
//! not approach the stack limit.
//!
//! ## The two-layer guard
//!
//! 1. A **compile-time** `const _: () = assert!(size_of::<HeapCore>() <= 8192)`
//!    in `src/registry/heap_core.rs` (right after the struct definition) — a
//!    future field addition that grows `HeapCore` past 8 KiB fails the BUILD,
//!    not a downstream deployment. This mirrors the established
//!    `SegmentHeader` pin pattern (`src/alloc_core/segment_header.rs`).
//! 2. This **runtime** `#[test]` — reads `size_of::<HeapCore>()` for the
//!    feature set under test and asserts the same budget. It is deliberately
//!    non-vacuous: it also enforces a lower bound, so a field REMOVAL (a
//!    suspicious shrink that would silently weaken the magazine/cache) is
//!    caught here too, and it prints the measured size for the record.
//!
//! ## Measured baseline (as of R34-18)
//!
//! `size_of::<HeapCore>() == 7576` under `production` (the maximum feature
//! composition; the struct has `#[cfg]`-gated fields, so smaller compositions
//! are strictly below). Breakdown: `core: AllocCore` = 864 B, `tcache: Tcache`
//! = 6664 B (the dominant per-class magazine cache), plus `id`/handles ≈ 48 B.
//! Budget 8192 leaves ~8 % headroom (616 B): minor field additions don't trip
//! it; material bloat (a new array/sub-struct, or `Tcache` growing another
//! class) does, forcing a deliberate budget bump with a recorded stack-pressure
//! note.
//!
//! R34-24/fix #571: This test requires `internals` to reach `HeapCore`,
//! which was gated behind that feature in R34-3/Sol-F1. The test exercises
//! the production feature set (the maximum shipping configuration).

#![cfg(all(feature = "production", feature = "internals"))]

use core::mem::size_of;

use sefer_alloc::registry::HeapCore;

/// `HeapCore` must fit within the 8 KiB stack-pressure budget. This mirrors the
/// compile-time `const _: () = assert!(size_of::<HeapCore>() <= 8192)` pin in
/// `src/registry/heap_core.rs` at runtime, for the feature set actually under
/// test. Non-vacuous: the measured size is positive and meaningfully close to
/// the budget (within ~8 %), and a lower bound catches a suspicious shrink.
#[test]
fn heap_core_size_within_stack_pressure_budget() {
    let size = size_of::<HeapCore>();

    // Upper bound — the stack-pressure budget. Matches the compile-time pin.
    // A HeapCore temporary is constructed by-value on a thread's
    // first-allocation frame; 8 KiB is half of a 16 KiB embedded stack.
    assert!(
        size <= 8192,
        "HeapCore grew to {size} bytes — exceeds the 8 KiB (8192) stack-pressure \
         budget (R34-18/F-6). A ~7 KiB-by-value construction on a small-stack \
         thread's first-allocation frame is the stack-overflow risk this pin \
         guards; bump the budget ONLY with a comment recording the new \
         stack-pressure implication."
    );

    // Lower bound — non-vacuousness / suspicious-shrink guard. The measured
    // maximum (production) is 7576; a configuration materially below 4 KiB
    // would mean the magazine cache (`tcache`, 6664 B under production) or
    // another core field silently disappeared. `production` is the canonical
    // size; under a leaner feature set the struct is smaller, hence the floor
    // sits well under the production baseline rather than at it.
    assert!(
        size >= 4096,
        "HeapCore shrank to {size} bytes — below the 4 KiB floor. Under \
         `production` the struct is 7576 B; a value this small suggests a \
         core field (the magazine cache / AllocCore substrate) was \
         accidentally cfg-gated out for this build. Investigate before \
         treating this as an improvement."
    );

    // Record the measured size for the feature set under test.
    eprintln!(
        "size_of::<HeapCore>() = {size} bytes (budget 8192, headroom {} B)",
        8192 - size
    );
}
