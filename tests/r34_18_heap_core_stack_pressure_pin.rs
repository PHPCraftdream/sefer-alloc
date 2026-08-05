//! Layout pin (R34-18/task #537, release-stabilization finding F-6 [low]):
//! `size_of::<HeapCore>()` must stay within a 9 KiB stack-pressure budget,
//! unconditionally across every feature composition.
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
//! materialises the temporary, that `HeapCore::new(..)` can place one
//! multi-KiB copy on a first-allocation frame — often very early in a
//! thread's life. Threads with small stacks (embedded-class 16–64 KiB, or a
//! constrained thread pool) are a realistic deployment, so a single such
//! temporary must not approach the stack limit.
//!
//! ## The two-layer guard
//!
//! 1. A **compile-time** `const _: () = assert!(size_of::<HeapCore>() <= 9216)`
//!    in `src/registry/heap_core.rs` (right after the struct definition) — a
//!    future field addition that grows `HeapCore` past 9 KiB fails the BUILD,
//!    not a downstream deployment. This mirrors the established
//!    `SegmentHeader` pin pattern (`src/alloc_core/segment_header.rs`).
//! 2. This **runtime** `#[test]` — reads `size_of::<HeapCore>()` for the
//!    feature set under test and asserts the same budget. It is deliberately
//!    non-vacuous: it also enforces a lower bound, so a field REMOVAL (a
//!    suspicious shrink that would silently weaken the magazine/cache) is
//!    caught here too, and it prints the measured size for the record.
//!
//! ## Measured sizes (as of this correction) and why the budget is 9216
//!
//! **Correction (task #572/H2's own follow-up wave — an independent readonly
//! review, `docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`
//! finding F1, caught this):** the ORIGINAL R34-18/#537 pin used a fixed
//! 8192 B budget; fix #571/H1 added a `#[cfg(not(any(experimental, pinning,
//! bench-internals, batch-api)))]` exclusion to dodge `--all-features`'s
//! 8840 B size, on the premise "8192 covers every SHIPPING composition".
//! That premise was false: `medium-classes` is a genuine shipping opt-in
//! (four dedicated CI rows) reaching 8408 B under plain `production
//! medium-classes` — over budget and NOT covered by the exclusion list, so
//! two live `ci.yml` commands were silently red. Enumerating "which features
//! are experimental" by name is inherently fragile; it missed one shipping
//! feature and broke CI.
//!
//! Fixed structurally: raised the budget to cover the TRUE global maximum
//! (`--all-features`, the union of every feature this crate has) plus real
//! headroom, and made both the compile-time assert AND this runtime test's
//! upper-bound assertion unconditional — no future feature, shipping or
//! experimental, can silently evade either one again. Measured sizes across
//! every composition tried: plain `production` = 7576 B, `production
//! medium-classes` = 8408 B, `production medium-classes numa-aware` =
//! 8416 B, `production medium-classes-wide numa-aware` = 8832 B,
//! `--all-features` = 8840 B (the confirmed maximum). Budget 9216 leaves
//! 376 B (~4%) headroom above that maximum.
//!
//! R34-24/fix #571: This test requires `internals` to reach `HeapCore`,
//! which was gated behind that feature in R34-3/Sol-F1.

#![cfg(all(feature = "production", feature = "internals"))]

use core::mem::size_of;

use sefer_alloc::registry::HeapCore;

/// `HeapCore` must fit within the 9 KiB stack-pressure budget. This mirrors
/// the compile-time `const _: () = assert!(size_of::<HeapCore>() <= 9216)`
/// pin in `src/registry/heap_core.rs` at runtime, for the feature set
/// actually under test — both are unconditional across every composition.
/// Non-vacuous: the measured size is positive and meaningfully close to the
/// budget, and a lower bound catches a suspicious shrink.
#[test]
fn heap_core_size_within_stack_pressure_budget() {
    let size = size_of::<HeapCore>();

    // Upper bound — the stack-pressure budget. Unconditional: covers every
    // feature composition, including `--all-features` (the confirmed global
    // maximum, 8840 B). A HeapCore temporary is constructed by-value on a
    // thread's first-allocation frame; 9 KiB is still just over half of a
    // 16 KiB embedded stack — the risk this pin guards.
    assert!(
        size <= 9216,
        "HeapCore grew to {size} bytes — exceeds the 9 KiB (9216) stack-pressure \
         budget (R34-18/F-6, corrected by the F1 fix in the wave-3 follow-up \
         review). A multi-KiB-by-value construction on a small-stack thread's \
         first-allocation frame is the stack-overflow risk this pin guards; \
         bump the budget ONLY with a comment recording the new stack-pressure \
         implication."
    );

    // Lower bound — non-vacuousness / suspicious-shrink guard. The measured
    // minimum (plain `production`) is 7576; a configuration materially below
    // 4 KiB would mean the magazine cache (`tcache`, 6664 B under production)
    // or another core field silently disappeared.
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
        "size_of::<HeapCore>() = {size} bytes (budget 9216, headroom {} B)",
        9216_i64 - size as i64
    );
}
