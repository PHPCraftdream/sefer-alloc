//! R14-4 (task #289) test (a) — move-leg reduction: growing a
//! `medium-classes` block past `MEDIUM_REALLOC_PROMOTION_THRESHOLD` (256
//! KiB) promotes it directly to Large, and every SUBSEQUENT grow within the
//! promoted block's headroom rides the existing OPT-G Large-grow-in-span
//! in-place fast path (no move) rather than another ladder-walk move-leg.
//!
//! Oracle: pointer identity. OPT-G never moves a block (it only mutates the
//! header in place); a ladder-walk move-leg ALWAYS allocates a fresh block.
//! So `grown_ptr_2 == grown_ptr_1` after a second in-headroom grow is a
//! strong, simple, non-vacuous signal that the second grow took OPT-G, not
//! the move leg — mirroring the oracle `examples/r11_3_promotion_probe.rs`
//! used (`debug_assert_eq!(p, before)`).
//!
//! Whole file is a no-op without `medium-classes` (see `#![cfg(...)]` below)
//! — run with:
//!   cargo test --release --features "production medium-classes" --test r14_4_promotion_move_leg_reduction

#![cfg(all(feature = "alloc-global", feature = "medium-classes"))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

const ALIGN: usize = 8;

/// The exact threshold `try_promote_to_large`'s call site checks
/// (`MEDIUM_REALLOC_PROMOTION_THRESHOLD` in `src/registry/heap_core_free.rs`)
/// — kept in sync manually since the constant itself is private to `src/`.
const PROMOTION_THRESHOLD: usize = 256 * 1024;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, ALIGN).unwrap()
}

/// Growing a medium-classified block PAST the promotion threshold, then
/// growing AGAIN within the promoted (now-Large) block's committed span,
/// must hit OPT-G on the second grow: SAME pointer, no move.
#[test]
fn second_grow_past_threshold_hits_opt_g_no_move() {
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
    // necessarily copies (either via promotion or the old ladder-walk), so we
    // do not assert pointer identity here.
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

    // Grow AGAIN, staying comfortably within the Large segment's committed
    // span (a 4 MiB segment; promote_size is far below that). If promotion
    // worked, this hits OPT-G: same pointer, in-place header mutation only.
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
/// special case.
#[test]
fn repeated_post_promotion_grows_all_hit_opt_g() {
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
