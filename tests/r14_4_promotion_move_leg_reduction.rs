//! R14-4 (task #289) test (a) — move-leg reduction: growing a
//! `medium-classes` block past `MEDIUM_REALLOC_PROMOTION_THRESHOLD` (256
//! KiB) promotes it directly to Large, and every SUBSEQUENT grow within the
//! promoted block's headroom rides the existing OPT-G Large-grow-in-span
//! in-place fast path (no move) rather than another ladder-walk move-leg —
//! PROVIDED the build has grow headroom past the promoted block's committed
//! span (see the `HAS_PROMOTION` note below for the documented exclusion).
//!
//! Oracle: pointer identity. OPT-G never moves a block (it only mutates the
//! header in place); a ladder-walk move-leg ALWAYS allocates a fresh block.
//! So `grown_ptr_2 == grown_ptr_1` after a second in-headroom grow is a
//! strong, simple, non-vacuous signal that the second grow took OPT-G, not
//! the move leg — mirroring the oracle `examples/r11_3_promotion_probe.rs`
//! used (`debug_assert_eq!(p, before)`).
//!
//! ## `HAS_PROMOTION` (R15-3, task #305, review finding P2-3 — supersedes the
//! task #302 `HAS_LARGE_GROW_HEADROOM` hotfix)
//!
//! `exact-span-large` (`Cargo.toml`) sizes a fresh Large segment's committed
//! `span_usable` to the EXACT page-rounded request — by design, with ZERO
//! spare headroom beyond what was asked for (see that feature's own doc
//! comment: "OPT-G in-place realloc growth has less committed headroom to
//! grow into before falling back to the slow path" — an explicitly
//! documented, intentional trade-off, not a bug). `try_promote_to_large`
//! (`src/registry/heap_core_free.rs`) pads the promoted block to exactly
//! `new_size`, so under `exact-span-large` the promoted segment's
//! `span_usable` equals the promotion request with no slack at all — the
//! VERY NEXT grow (which by construction asks for strictly more) can never
//! fit the committed span, so OPT-G's `payload_off + new_eff <= span_usable`
//! check structurally fails every time.
//!
//! `large-reserved-capacity` (R12-4) exists specifically to restore that
//! headroom on top of `exact-span-large` (it reserves a geometric multiple of
//! the request as uncommitted VA and commits the missing tail on demand — see
//! `try_grow_large_reserved_capacity` in `src/alloc_core/alloc_core.rs`) —
//! BUT `alloc_core_large.rs`'s own `LARGE_RESERVED_CAP_BYTES`/
//! `LARGE_RESERVED_CAP_GROWTH_FACTOR` doc comments spell out a second,
//! independent exclusion: under `numa-aware`, the reservation always takes
//! the eager `numa::reserve_aligned_on_node` arm with `reserved_capacity ==
//! usable` — i.e. `large-reserved-capacity`'s extra headroom is itself
//! disabled whenever `numa-aware` is also on, NUMA placement taking priority
//! over the lazy-capacity optimisation (mirrors the A3 directory's own
//! NUMA-first precedent).
//!
//! Originally (task #302) this zero-headroom combination was handled by
//! WEAKENING this test's assertion to "grow succeeds and bytes survive"
//! (still true, but no longer distinguishes OPT-G from a move). That was a
//! test-only patch over a real mechanism problem: a promoted block with zero
//! headroom pessimizes EVERY subsequent grow into a move leg, even small
//! steps that would have stayed in-place via OPT-F (small same-class carve)
//! on the ordinary medium ladder had promotion never fired at all — see
//! `docs/reviews/2026-07-24-r15-plan.md` finding P2-3. R15-3 (task #305) is
//! the code-level fix: `try_promote_to_large` and its call site in
//! `HeapCore::realloc` (`src/registry/heap_core_free.rs`) are now gated by
//! the SAME extended `#[cfg]` predicate as `HAS_PROMOTION` below, so in the
//! zero-headroom combination the promotion mechanism does not compile in at
//! all — growth of a medium-classified block instead behaves exactly like
//! plain `production` without `medium-classes`: an ordinary ladder-walk
//! move-leg, with OPT-F available for subsequent same-class grows. This file
//! now asserts a DIFFERENT, still-meaningful oracle for that configuration
//! (see `promotion_off_second_grow_same_class_hits_opt_f_no_move` and
//! `promotion_off_repeated_same_class_grows_all_hit_opt_f` below) rather than
//! a weak "just check success" fallback.
//!
//! So headroom (and hence promotion) holds iff `exact-span-large` is off
//! (SEGMENT-rounding is headroom by construction — this holds regardless of
//! `numa-aware`, since the NUMA arm reserves exactly `usable` and `usable`
//! itself is already SEGMENT-sized there), OR `exact-span-large` is on
//! together with `large-reserved-capacity` on AND `numa-aware` off. Put
//! differently: it is only the COMBINATION `exact-span-large` + `numa-aware`
//! (with or without `large-reserved-capacity`, since `numa-aware` disables
//! that mechanism too), or plain `exact-span-large` without
//! `large-reserved-capacity` at all, that removes headroom — and, as of
//! R15-3, removes the promotion mechanism itself, compiled out entirely.
//!
//! Whole file is a no-op without `medium-classes` (see `#![cfg(...)]` below)
//! — run with:
//!   cargo test --release --features "production medium-classes" --test r14_4_promotion_move_leg_reduction

#![cfg(all(feature = "alloc-global", feature = "medium-classes"))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

const ALIGN: usize = 8;

/// Mirrors the EXACT `#[cfg]` predicate now gating `try_promote_to_large` and
/// its call site in `src/registry/heap_core_free.rs` (R15-3, task #305).
/// `true` iff the promotion mechanism is compiled in for this build: either
/// there is no `exact-span-large` tightness to begin with, or
/// `large-reserved-capacity` is present AND `numa-aware` is not overriding it
/// (see the module doc's derivation, carried over from the task #302
/// hotfix's `HAS_LARGE_GROW_HEADROOM`). When `false`, promotion does not
/// exist in this build at all — growth of a medium-classified block takes
/// the ordinary ladder-walk move-leg, same as plain `production` without
/// `medium-classes`.
const HAS_PROMOTION: bool = !cfg!(feature = "exact-span-large")
    || (cfg!(feature = "large-reserved-capacity") && !cfg!(feature = "numa-aware"));

/// The exact threshold `try_promote_to_large`'s call site checks
/// (`MEDIUM_REALLOC_PROMOTION_THRESHOLD` in `src/registry/heap_core_free.rs`)
/// — kept in sync manually since the constant itself is private to `src/`
/// (and, as of R15-3, only compiled in under the same `HAS_PROMOTION`
/// predicate — this test-side copy is unconditional so it can be used to
/// derive sizes in BOTH configurations).
const PROMOTION_THRESHOLD: usize = 256 * 1024;

/// One of the exact `medium-classes` EXTRAS classes (`src/alloc_core/
/// size_classes.rs`) strictly above `PROMOTION_THRESHOLD`: 320 KiB. Used only
/// by the `HAS_PROMOTION == false` tests below to pick two sizes that land in
/// the SAME medium class (256 KiB < size <= 320 KiB), so a second grow within
/// that class is a meaningful OPT-F (same-class in-place) probe.
const SAME_CLASS_CEILING: usize = 320 * 1024;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, ALIGN).unwrap()
}

/// Growing a medium-classified block PAST the promotion threshold, then
/// growing AGAIN within the promoted (now-Large) block's committed span,
/// must hit OPT-G on the second grow (SAME pointer, no move) — this test only
/// runs its identity assertion when `HAS_PROMOTION` is true; see
/// `promotion_off_second_grow_same_class_hits_opt_f_no_move` for the dual
/// scenario when it is false.
#[test]
fn second_grow_past_threshold_hits_opt_g_no_move() {
    if !HAS_PROMOTION {
        // Promotion does not compile in for this build (see module doc) —
        // covered instead by `promotion_off_second_grow_same_class_hits_opt_f_no_move`.
        return;
    }

    let a = SeferAlloc::new();

    // Start well within the medium range, below the threshold.
    let old_size = 64 * 1024;
    let old_layout = layout(old_size);
    // SAFETY: valid, non-zero-size layout.
    let p0 = unsafe { a.alloc(old_layout) };
    assert!(!p0.is_null(), "initial 64 KiB alloc failed");
    // SAFETY: p0 valid for old_size bytes.
    unsafe {
        std::ptr::write_bytes(p0, 0x11, old_size);
    }

    // Grow PAST the promotion threshold — this is the promotion step. It
    // necessarily copies, so we do not assert pointer identity here.
    let promote_size = PROMOTION_THRESHOLD + 4096; // just past the threshold
                                                   // SAFETY: p0 is live, old_layout matches, freed at most once on success.
    let p1 = unsafe { a.realloc(p0, old_layout, promote_size) };
    assert!(!p1.is_null(), "promoting realloc failed");
    // SAFETY: p1 valid for promote_size bytes.
    unsafe {
        for i in 0..old_size {
            assert_eq!(
                p1.add(i).read(),
                0x11,
                "byte {i} lost across the promoting grow"
            );
        }
    }

    // Grow AGAIN. `HAS_PROMOTION` guarantees the promoted Large segment has
    // committed span to grow into (either a whole 4 MiB `SEGMENT` without
    // `exact-span-large`, or `large-reserved-capacity`'s reserved-but-
    // uncommitted VA on top of it) and this hits OPT-G: same pointer,
    // in-place header mutation only.
    let second_size = promote_size + 64 * 1024;
    let promote_layout = layout(promote_size);
    // SAFETY: p1 is live, promote_layout matches, freed at most once on success.
    let p2 = unsafe { a.realloc(p1, promote_layout, second_size) };
    assert!(!p2.is_null(), "second (post-promotion) grow failed");
    assert_eq!(
        p1, p2,
        "second grow after promotion must hit OPT-G in-place (no move) — \
         a differing pointer means the block was NOT promoted to Large and \
         instead took another ladder-walk move-leg"
    );

    // SAFETY: p2 valid for second_size bytes.
    unsafe {
        for i in 0..old_size {
            assert_eq!(
                p2.add(i).read(),
                0x11,
                "byte {i} lost across the post-promotion in-place grow"
            );
        }
    }

    let second_layout = layout(second_size);
    // SAFETY: p2 live, second_layout matches, freed exactly once.
    unsafe { a.dealloc(p2, second_layout) };
}

/// A THIRD consecutive in-headroom grow must ALSO hit OPT-G (not just the
/// first post-promotion grow) — proving the promoted block behaves as an
/// ordinary, durable Large allocation from that point on, not a one-shot
/// special case. Only runs when `HAS_PROMOTION` is true; see
/// `promotion_off_repeated_same_class_grows_all_hit_opt_f` for the dual.
#[test]
fn repeated_post_promotion_grows_all_hit_opt_g() {
    if !HAS_PROMOTION {
        return;
    }

    let a = SeferAlloc::new();

    let old_size = 32 * 1024;
    let old_layout = layout(old_size);
    // SAFETY: valid, non-zero-size layout.
    let p0 = unsafe { a.alloc(old_layout) };
    assert!(!p0.is_null());

    let promote_size = PROMOTION_THRESHOLD + 1024;
    // SAFETY: p0 live, old_layout matches, freed at most once on success.
    let mut p = unsafe { a.realloc(p0, old_layout, promote_size) };
    assert!(!p.is_null());
    let mut cur_size = promote_size;

    for step in 1..=3 {
        let next_size = cur_size + 32 * 1024;
        let cur_layout = layout(cur_size);
        let before = p;
        // SAFETY: p live, cur_layout matches, freed at most once on success.
        p = unsafe { a.realloc(p, cur_layout, next_size) };
        assert!(!p.is_null(), "grow step {step} failed");
        assert_eq!(
            p, before,
            "post-promotion grow step {step} must stay in-place (OPT-G)"
        );
        cur_size = next_size;
    }

    let final_layout = layout(cur_size);
    // SAFETY: p live, final_layout matches, freed exactly once.
    unsafe { a.dealloc(p, final_layout) };
}

/// Dual of `second_grow_past_threshold_hits_opt_g_no_move` for
/// `HAS_PROMOTION == false` (R15-3, task #305): with the promotion mechanism
/// compiled out, growing past `PROMOTION_THRESHOLD` stays on the ordinary
/// medium ladder (no Large classification at all). A first grow into the 320
/// KiB medium class necessarily moves (crossing from a smaller class), but a
/// SECOND grow that stays WITHIN that same 320 KiB class must hit OPT-F
/// (Small/medium same-class in-place carve, `src/alloc_core/alloc_core.rs`'s
/// `realloc_inplace_fast_path_known_base`) — same pointer, no move. This is
/// the concrete, non-vacuous replacement for the old "just check success"
/// fallback: it proves growth in this configuration behaves like ordinary
/// medium-ladder growth (OPT-F available), not like a degraded/pessimized
/// promoted-but-headroom-less Large block.
#[test]
fn promotion_off_second_grow_same_class_hits_opt_f_no_move() {
    if HAS_PROMOTION {
        // Promotion compiles in for this build — covered instead by
        // `second_grow_past_threshold_hits_opt_g_no_move`.
        return;
    }

    let a = SeferAlloc::new();

    let old_size = 64 * 1024;
    let old_layout = layout(old_size);
    // SAFETY: valid, non-zero-size layout.
    let p0 = unsafe { a.alloc(old_layout) };
    assert!(!p0.is_null(), "initial 64 KiB alloc failed");
    // SAFETY: p0 valid for old_size bytes.
    unsafe {
        std::ptr::write_bytes(p0, 0x11, old_size);
    }

    // Grow past the threshold, landing inside the 320 KiB medium class
    // (256 KiB < size <= 320 KiB) — this crosses classes, so it necessarily
    // moves (no assertion on pointer identity here, exactly like the
    // promotion step in the `HAS_PROMOTION == true` twin).
    let promote_size = PROMOTION_THRESHOLD + 4096; // 260 KiB, class ceiling 320 KiB
    assert!(promote_size <= SAME_CLASS_CEILING);
    // SAFETY: p0 is live, old_layout matches, freed at most once on success.
    let p1 = unsafe { a.realloc(p0, old_layout, promote_size) };
    assert!(!p1.is_null(), "growth past threshold failed");
    // SAFETY: p1 valid for old_size bytes.
    unsafe {
        for i in 0..old_size {
            assert_eq!(
                p1.add(i).read(),
                0x11,
                "byte {i} lost across the first grow"
            );
        }
    }

    // Grow AGAIN, staying within the SAME 320 KiB medium class — must hit
    // OPT-F in-place (same pointer), proving this build's medium ladder
    // still carves same-class grows for free rather than pessimizing every
    // grow into a move (the exact hazard a headroom-less promoted Large
    // block would have had).
    let second_size = SAME_CLASS_CEILING; // still <= 320 KiB, same class as promote_size
    assert!(second_size > promote_size, "test sizes must actually grow");
    let promote_layout = layout(promote_size);
    // SAFETY: p1 is live, promote_layout matches, freed at most once on success.
    let p2 = unsafe { a.realloc(p1, promote_layout, second_size) };
    assert!(!p2.is_null(), "second (same-class) grow failed");
    assert_eq!(
        p1, p2,
        "second grow within the same medium class must hit OPT-F in-place (no move) — \
         with promotion compiled out, this build's medium ladder must behave like \
         ordinary same-class growth, not a headroom-less promoted Large block"
    );

    // SAFETY: p2 valid for old_size bytes.
    unsafe {
        for i in 0..old_size {
            assert_eq!(
                p2.add(i).read(),
                0x11,
                "byte {i} lost across the same-class in-place grow"
            );
        }
    }

    let second_layout = layout(second_size);
    // SAFETY: p2 live, second_layout matches, freed exactly once.
    unsafe { a.dealloc(p2, second_layout) };
}

/// Dual of `repeated_post_promotion_grows_all_hit_opt_g` for
/// `HAS_PROMOTION == false`: repeated small grows that each stay within the
/// CURRENT medium class must each hit OPT-F, one class at a time — proving
/// same-class in-place growth is durable across multiple steps in this
/// configuration too, not a one-shot artifact.
#[test]
fn promotion_off_repeated_same_class_grows_all_hit_opt_f() {
    if HAS_PROMOTION {
        return;
    }

    let a = SeferAlloc::new();

    // Two same-class pairs from the medium EXTRAS ladder
    // (`src/alloc_core/size_classes.rs`): (260 KiB -> 320 KiB) then, after an
    // unavoidable cross-class move, (site inside 384 KiB -> 384 KiB).
    let old_size = 16 * 1024;
    let old_layout = layout(old_size);
    // SAFETY: valid, non-zero-size layout.
    let p0 = unsafe { a.alloc(old_layout) };
    assert!(!p0.is_null());

    // Cross-class growth into the 320 KiB class (necessarily moves).
    let mut cur_size = PROMOTION_THRESHOLD + 1024; // 257 KiB, class ceiling 320 KiB
                                                   // SAFETY: p0 live, old_layout matches, freed at most once on success.
    let mut p = unsafe { a.realloc(p0, old_layout, cur_size) };
    assert!(!p.is_null());

    // Step 1: same-class grow within 320 KiB — must hit OPT-F.
    {
        let next_size = SAME_CLASS_CEILING; // 320 KiB, same class as cur_size
        let cur_layout = layout(cur_size);
        let before = p;
        // SAFETY: p live, cur_layout matches, freed at most once on success.
        p = unsafe { a.realloc(p, cur_layout, next_size) };
        assert!(!p.is_null(), "same-class grow step 1 failed");
        assert_eq!(
            p, before,
            "same-class grow step 1 must stay in-place (OPT-F)"
        );
        cur_size = next_size;
    }

    // Cross-class growth into the 384 KiB class (necessarily moves).
    {
        let next_size = 384 * 1024 - 4096; // inside the 384 KiB class ceiling
        let cur_layout = layout(cur_size);
        // SAFETY: p live, cur_layout matches, freed at most once on success.
        p = unsafe { a.realloc(p, cur_layout, next_size) };
        assert!(!p.is_null(), "cross-class grow into 384 KiB failed");
        cur_size = next_size;
    }

    // Step 2: same-class grow within 384 KiB — must ALSO hit OPT-F, proving
    // this is durable across repeated same-class steps, not a one-shot case.
    {
        let next_size = 384 * 1024;
        let cur_layout = layout(cur_size);
        let before = p;
        // SAFETY: p live, cur_layout matches, freed at most once on success.
        p = unsafe { a.realloc(p, cur_layout, next_size) };
        assert!(!p.is_null(), "same-class grow step 2 failed");
        assert_eq!(
            p, before,
            "same-class grow step 2 must stay in-place (OPT-F)"
        );
        cur_size = next_size;
    }

    let final_layout = layout(cur_size);
    // SAFETY: p live, final_layout matches, freed exactly once.
    unsafe { a.dealloc(p, final_layout) };
}
