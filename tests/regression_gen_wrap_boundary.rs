//! X7 Ф5 (task #193) — the wrap-1/256 accepted-residual boundary test.
//!
//! The X7 per-granule generation counter is a `u8` (`AtomicU8` in the segment
//! metadata gen table). It WRAPS at 256: a stale ring note whose stamped
//! generation coincidentally equals the block's CURRENT generation modulo 256
//! is wrongly honoured by the drain's `stamped_gen == current_gen` compare
//! (Ф3's third touch). This is the accepted, documented probabilistic residual
//! of the X7 arc — plan §2.5 explicitly rejected doubling the ring footprint
//! (a `u64` generation note) to close a residual that only fires under
//! adversarial cross-thread-free timing after ≥256 re-issues of the SAME block
//! with no intervening drain of the stale note.
//!
//! This file pins the EXACT boundary. The drain's compare is
//!
//! ```text
//!   if stamped_gen == current_gen { honour the note } else { drop it }
//! ```
//!
//! So a stale note collides (is wrongly honoured) IFF the number of bumps
//! between stamp and drain is a multiple of 256. The smallest such multiple
//! is 256 itself. This test asserts:
//!
//! - **Const-derived boundary:** `1 << ENTRY_GEN_BITS == 256` — the wrap point
//!   is a compile-time fact derivable from the gen field width (`ENTRY_GEN_BITS
//!   == 8`, itself pinned by Ф2's `bit_widths_sum_to_exactly_32`). Not a fuzzy
//!   "eventually wraps" — the EXACT modulus.
//! - **State-derived boundary (the drain decision, exactly):** stamp a note at
//!   generation `N`, bump the granule `k` times, and confirm the drain
//!   comparison `stamped_gen == gen_at(...)` is
//!   - `true`  (WRONG — collision) at exactly `k = 256`,
//!   - `false` (correct drop)      at `k = 255` and `k = 257`,
//!
//!   for `N` swept across the u8 range (0, 1, 127, 128, 200, 255) so the
//!   boundary is pinned at every starting generation, including the wrap-edge
//!   `N = 255` (where +1 ⇒ 0, the u8 overflow point itself).
//!
//! ## Why a new file (not extending `regression_gen_table_layout.rs`)
//!
//! `regression_gen_table_layout.rs` (Ф1) covers the gen TABLE — its existence,
//! footprint, and byte-level accessors. Its `gen_roundtrip_and_wrap` already
//! asserts "300 bumps ⇒ final = 300 mod 256" and "256 bumps ⇒ same value"
//! (the wrap mechanic). This file covers a DIFFERENT concept: the DRAIN
//! DECISION boundary — the exact modulus at which a stale note is honoured
//! vs dropped, derived from `ENTRY_GEN_BITS` and modelled as the
//! `stamped_gen == current_gen` compare Ф3's drain performs. Per the repo's
//! "one file, one concept" convention (CLAUDE.md) this is a distinct concept
//! in a distinct module (the `remote_free_ring` drain compare, not the
//! `segment_header` table storage), so it gets its own file. It is also a
//! LIGHTER, more targeted test than Ф1's: it does not exercise the live
//! allocator (no magazine pop, no real ring push/drain) — it directly models
//! the arithmetic the drain consults, which is faster and pins the boundary
//! more precisely than forcing 256 real re-issues through the allocator.
//!
//! ## Counterfactual (non-vacuity)
//!
//! - If `ENTRY_GEN_BITS` were widened (e.g. to 9, closing the residual by
//!   doubling the gen field — the change plan §2.5 rejected), the
//!   `const_wrap_point_is_exactly_256` test would FAIL: `1 << 9 = 512 != 256`.
//!   The test therefore pins the ACCEPTED residual's exact modulus — a future
//!   widening surfaces as a test delta, not a silent change.
//! - If the drain compared `stamped_gen != current_gen` to HONOUR (inverted
//!   sense), `drain_compare_at_exact_wrap_collides` would fail at `k = 256`
//!   (it would assert `true` but the inverted compare returns `false`).
//! - If the gen counter did NOT wrap (e.g. a hypothetical `saturating_add`
//!   instead of `fetch_add`), `drain_compare_at_exact_wrap_collides` at
//!   `N = 255, k = 256` would fail: `gen_at` would read 255 (saturated), not
//!   0, so the compare at `k = 256` would be `255 == 255` ⇒ true at the WRONG
//!   bump count (1, not 256) and the `k = 255` case would also collide
//!   (false positive — the test asserts it must NOT).
//!
//! ## Gates
//!
//! The gen-table accessors and the `ENTRY_GEN_BITS` constant exist ONLY under
//! `#[cfg(feature = "hardened")]` (Ф1/Ф2 discipline). The whole file is
//! hardened-gated: there is no non-hardened companion because the wrap residual
//! is a hardened-only concept (under `production` the generation counter does
//! not exist and the ring note has no gen field). Miri-clean: the test buffer
//! is allocated via the raw global allocator (exposed-provenance, the
//! standalone-buffer analogue of an OS segment), matching Ф1's miri-clean
//! `SegmentBuffer` helper.

#![cfg(feature = "hardened")]

use sefer_alloc::alloc_core::remote_free_ring::ENTRY_GEN_BITS;
use sefer_alloc::alloc_core::segment_header::{bump_gen, gen_at};
use sefer_alloc::SegmentLayout;

/// Allocate a `SEGMENT`-byte, `MIN_BLOCK`-aligned, ZEROED buffer via the raw
/// global allocator and return its base pointer (with a guard that frees it on
/// drop). The raw `alloc` call returns an exposed-provenance pointer — the
/// standalone-buffer analogue of an OS-mmap'd segment — which is what
/// `Node::atomic_u8_at`'s `&AtomicU8` cast + Relaxed RMW requires under miri
/// Stacked Borrows. Mirrors Ф1's `SegmentBuffer` exactly.
struct SegmentBuffer {
    ptr: *mut u8,
    layout: std::alloc::Layout,
}
impl SegmentBuffer {
    fn new() -> Self {
        let layout =
            std::alloc::Layout::from_size_align(SegmentLayout::SEGMENT, SegmentLayout::MIN_BLOCK)
                .expect("SEGMENT/MIN_BLOCK layout is valid");
        // SAFETY: `layout` has non-zero size (SEGMENT = 4 MiB); `alloc` returns
        // either a valid, `layout`-aligned, zeroed-by-us pointer or null (we
        // abort on null). The bytes are initialised to 0 by `write_bytes`.
        let ptr = unsafe { std::alloc::alloc(layout) };
        assert!(
            !ptr.is_null(),
            "raw alloc of a SEGMENT-byte buffer must succeed"
        );
        unsafe { core::ptr::write_bytes(ptr, 0, SegmentLayout::SEGMENT) };
        Self { ptr, layout }
    }
    fn base(&self) -> *mut u8 {
        self.ptr
    }
}
impl Drop for SegmentBuffer {
    fn drop(&mut self) {
        // SAFETY: `self.ptr` was allocated by `alloc(self.layout)` and is still
        // valid; `dealloc` with the same layout frees it.
        unsafe { std::alloc::dealloc(self.ptr, self.layout) };
    }
}

/// Bump granule `off` exactly `n` times. After this, `gen_at(base, off)` reads
/// `initial + n` (mod 256). Mirrors Ф1's `bump_n` but does not return the
/// pre-increment value — this test cares only about the FINAL readable state.
fn bump_n_times(base: *mut u8, off: usize, n: u32) {
    for _ in 0..n {
        let _pre = unsafe { bump_gen(base, off) };
    }
}

/// **Test 1 — the wrap point is EXACTLY 256, derived from `ENTRY_GEN_BITS`.**
///
/// The accepted residual's modulus is not a magic number — it is `1 <<
/// ENTRY_GEN_BITS`, a compile-time fact. `ENTRY_GEN_BITS == 8` (pinned by Ф2's
/// `bit_widths_sum_to_exactly_32` and the source const-assert at
/// `remote_free_ring.rs`'s `ENTRY_GEN_BITS == 8` line), so the wrap point is
/// exactly 256. This test asserts the derivation directly: if the gen field
/// were ever widened (closing the residual — plan §2.5's rejected alternative)
/// or narrowed (silently re-introducing a tighter residual), this test fails
/// with a clear delta.
#[test]
fn const_wrap_point_is_exactly_256() {
    assert_eq!(
        ENTRY_GEN_BITS, 8,
        "ENTRY_GEN_BITS must be 8 (the Ф1 u8 generation counter; Ф2 const-asserted)"
    );
    let wrap = 1u32 << ENTRY_GEN_BITS;
    assert_eq!(
        wrap, 256,
        "the gen wrap point must be exactly 1 << ENTRY_GEN_BITS = 256 — \
         this is the accepted residual's modulus (X7 §2.5); a different value \
         means the gen field width changed and the residual boundary moved"
    );
}

/// **Test 2 — the drain decision boundary, pinned EXACTLY.**
///
/// Models the drain's `stamped_gen == current_gen` compare (Ф3's third touch)
/// at the exact wrap boundary. The protocol:
///
/// 1. Bump the granule from 0 to a starting generation `N` (so the stamped note
///    carries `N`).
/// 2. Bump the granule `k` MORE times (the re-issues-without-drain that advance
///    the block's life past the note's stamp).
/// 3. Read `current = gen_at(...)` and evaluate `N == current` — the EXACT
///    compare the drain performs.
///
/// The compare is TRUE (WRONG — collision, stale note honoured) IFF `k` is a
/// multiple of 256; FALSE (correct drop) otherwise. This test pins the smallest
/// collision modulus (256) and its immediate neighbours (255, 257):
///
/// - `k = 255` ⇒ `current = N + 255 (mod 256) != N` ⇒ compare FALSE (drop). ✔
/// - `k = 256` ⇒ `current = N (mod 256) == N`       ⇒ compare TRUE  (collide). ✘
/// - `k = 257` ⇒ `current = N + 1 (mod 256) != N`   ⇒ compare FALSE (drop). ✔
///
/// Swept across `N ∈ {0, 1, 127, 128, 200, 255}` so the boundary is pinned at
/// every starting generation, including the wrap-edge `N = 255` (where the
/// counter rolls 255 → 0 → 1, exercising the u8 overflow point itself).
///
/// This is the accepted residual's EXACT shape: a stale note collides ONLY at
/// multiples of 256 bumps-without-drain, never at any other count. The 255/257
/// cases are the counterfactual — they prove the test is not vacuously
/// asserting "everything collides"; only the exact 256-modulus does.
#[test]
fn drain_compare_at_exact_wrap_collides() {
    let buf = SegmentBuffer::new();
    let base = buf.base();
    let off = SegmentLayout::MIN_BLOCK; // first payload granule (offset 16)

    // The starting generations to sweep. Includes the wrap-edge N=255 (where
    // +1 ⇒ 0, the u8 overflow point) and the mid-range values.
    let start_gens: [u8; 6] = [0, 1, 127, 128, 200, 255];

    for &n in &start_gens {
        // Reset the granule to 0 by re-allocating the buffer state is not
        // possible mid-test (the buffer is shared); instead we drive the
        // counter from its CURRENT value up to `n` by bumping until `gen_at`
        // reads `n`. Because `fetch_add` returns the pre-value, the simplest
        // correct reset is: bump until `gen_at(base, off) == n`. Since we start
        // each iteration at a known value (the previous iteration's final
        // state), we bump the delta. The first iteration starts at 0 (freshly
        // zeroed buffer).
        let cur = unsafe { gen_at(base, off) };
        let delta = (n as u32).wrapping_sub(cur as u32) & 0xFF;
        bump_n_times(base, off, delta);
        let stamped = unsafe { gen_at(base, off) };
        assert_eq!(
            stamped, n,
            "setup: granule should be at starting gen {n} (was {cur}, bumped {delta})"
        );

        // --- k = 255: NOT a collision (drain correctly DROPS the stale note).
        bump_n_times(base, off, 255);
        let current_255 = unsafe { gen_at(base, off) };
        // current_255 = (n + 255) mod 256 = n - 1 (mod 256) != n (unless n wraps, but n+255 != n mod 256 for any n).
        assert_ne!(
            n, current_255,
            "k=255: stamped gen {n} must NOT equal current gen {current_255} — \
             only an exact 256-multiple collides"
        );
        assert!(
            n != current_255,
            "k=255 drain compare: must be FALSE (drop the stale note)"
        );

        // --- k = 256 (i.e. 255 + 1 more): COLLISION (drain WRONGLY honours).
        // This is the accepted residual: 256 re-issues-without-drain ⇒ the
        // stale note's stamped gen coincidentally matches the current gen.
        let _one_more = unsafe { bump_gen(base, off) }; // 255 → 256 ≡ 0 mod 256 from the stamp point
        let current_256 = unsafe { gen_at(base, off) };
        assert_eq!(
            n, current_256,
            "k=256: stamped gen {n} MUST equal current gen {current_256} — \
             this is the accepted 1/256 wrap residual (X7 §2.5), the exact \
             modulus at which a stale note is wrongly honoured"
        );
        assert!(
            n == current_256,
            "k=256 drain compare: must be TRUE (collision — the accepted residual)"
        );

        // --- k = 257 (256 + 1 more): NOT a collision again (drain drops).
        let _ = unsafe { bump_gen(base, off) };
        let current_257 = unsafe { gen_at(base, off) };
        assert_ne!(
            n, current_257,
            "k=257: stamped gen {n} must NOT equal current gen {current_257} — \
             the collision is ONLY at exact 256-multiples, not ±1"
        );
        assert!(
            n != current_257,
            "k=257 drain compare: must be FALSE (drop the stale note)"
        );

        // Leave the granule at n+1 (mod 256) so the next iteration's setup
        // delta is well-defined. The loop's setup recomputes the delta from
        // `gen_at`, so no explicit reset is needed.
    }
}

/// **Test 3 — the second collision (k = 512) is also pinned, with ±1 neighbours.**
///
/// The residual is not "collides once at 256 then stops" — it collides at
/// EVERY multiple of 256. This test confirms the second collision point (512 =
/// 2·256) AND its ±1 neighbours (511, 513), to pin that the modulus is
/// periodic, not a one-off. A counter that saturated at 256 (instead of
/// wrapping) would pass Test 2's k=256 case but FAIL here: at k=512 a
/// saturating counter still reads its cap, not `stamped`.
#[test]
fn second_wrap_at_512_also_collides() {
    let buf = SegmentBuffer::new();
    let base = buf.base();
    let off = SegmentLayout::MIN_BLOCK * 3; // a distinct granule from Test 2

    // Start at a non-zero, non-edge generation so the wrap is unambiguous.
    let stamped = 42u8;
    assert_eq!(
        unsafe { gen_at(base, off) },
        0,
        "fresh granule should start at gen 0"
    );
    bump_n_times(base, off, stamped as u32);
    assert_eq!(
        unsafe { gen_at(base, off) },
        stamped,
        "setup to stamped gen {stamped}"
    );

    // --- k = 511: NOT a collision (drain drops).
    bump_n_times(base, off, 511);
    let current_511 = unsafe { gen_at(base, off) };
    assert_ne!(
        stamped, current_511,
        "k=511: must NOT collide — only exact 256-multiples do"
    );

    // --- k = 512 (one more bump): COLLISION (the second period).
    let _ = unsafe { bump_gen(base, off) };
    let current_512 = unsafe { gen_at(base, off) };
    assert_eq!(
        stamped, current_512,
        "k=512: stamped gen {stamped} MUST equal current gen {current_512} — \
         the residual is PERIODIC at every 256-multiple (not just the first)"
    );

    // --- k = 513 (one more bump): NOT a collision again.
    let _ = unsafe { bump_gen(base, off) };
    let current_513 = unsafe { gen_at(base, off) };
    assert_ne!(
        stamped, current_513,
        "k=513: must NOT collide — the second-period boundary has the same ±1 shape"
    );
}
