//! Round-trip pin for the owner-stamp word (`bug #1981`): `pack_owner` used to
//! shift `owner_id` left by `OWNER_ID_SHIFT = 1` WITHOUT masking to the 31-bit
//! id field, so `pack_owner(OWNER_STATE_LIVE, u32::MAX, 0)` produced
//! `0x1_FFFF_FFFE` — bit 32 (the generation field's LSB, `OWNER_GEN_SHIFT`)
//! leaked out of the id field, and `unpack_owner_id` (which masks with
//! `OWNER_ID_MASK`) returned `0x7FFF_FFFF` (`OWNER_ID_NONE`), never `u32::MAX`.
//!
//! ## Consequences of the defect
//!
//! The process-global fallback heap was constructed with the id `u32::MAX`
//! (src/global/fallback.rs). Since that id is outside the representable
//! 31-bit id domain, `stamp_segment_owner`'s OPT-C fast path
//! (src/registry/heap_core/state/ownership.rs) — `unpack_owner_id(cur) ==
//! self.id` — could NEVER hit for fallback-stamped segments: its stamp cache
//! was permanently cold. Worse, the leaked bit 32 silently wrote into the
//! (currently unread) generation field — a latent trap for any future reader
//! of that field.
//!
//! ## The fix under test
//!
//! 1. `pack_owner` now masks `owner_id` to the 31-bit field before shifting,
//!    so NO input can leak across a field boundary; an id >= 2^31 clamps to
//!    `id & 0x7FFF_FFFF` instead of round-tripping — which is why every id
//!    actually stamped into an `owner_state` word must be < 2^31.
//! 2. The fallback heap now uses the dedicated sentinel `OWNER_ID_FALLBACK`
//!    (0x7FFF_FFFE) — < 2^31 so it round-trips exactly (the property the
//!    OPT-C compare needs), `!= OWNER_ID_NONE`, and >= 4096 (`MAX_HEAPS`) so
//!    owner-id→slot resolution keeps treating fallback-stamped segments as
//!    out-of-range, exactly as the old masks-to-`OWNER_ID_NONE` behaviour did.
//!
//! ## NON-VACUOUSNESS (counterfactual)
//!
//! These tests are falsifiable, not tautological: reverting ONLY the mask in
//! `pack_owner` back to the unmasked `((owner_id as u64) << OWNER_ID_SHIFT)`
//! (leaving `OWNER_ID_FALLBACK` in place, so everything still compiles) makes
//! `u32_max_stamp_never_leaks_into_generation_field` FAIL (pre-fix word is
//! `0x1_FFFF_FFFE`, not `0xFFFF_FFFE`). `u32_max_clamps_instead_of_leaking`
//! does NOT fire under the revert: masking and truncation agree on the
//! unpacked VALUE (`unpack_owner_id` masks with `OWNER_ID_MASK` either way),
//! which is precisely the latent-trap point — the id FIELD looked unchanged
//! while bit 32 bled into the generation field — so the pinned exact word in
//! test 1 is the load-bearing falsification.
//! `fresh_segment_stamp_word_is_byte_stable` stays green across the revert
//! (masking is a no-op for `OWNER_ID_NONE`). Verified empirically: with the
//! mask reverted, test 1 fails; with the mask restored, all pass.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use sefer_alloc::alloc_core::segment_header::{
    pack_owner, unpack_owner_id, OWNER_ID_FALLBACK, OWNER_ID_NONE, OWNER_STATE_LIVE,
};

/// Test 1 — THE falsification core: the pre-fix value of
/// `pack_owner(OWNER_STATE_LIVE, u32::MAX, 0)` was `0x1_FFFF_FFFE` (bit 32,
/// the generation field's LSB, leaked out of the 31-bit id field). With the
/// mask, the word must be exactly `0xFFFF_FFFE`: state bit clear, generation
/// field (`>> 32`) zero, no leakage.
#[test]
fn u32_max_stamp_never_leaks_into_generation_field() {
    let word = pack_owner(OWNER_STATE_LIVE, u32::MAX, 0);
    // Exact pin (pre-fix: 0x1_FFFF_FFFE).
    assert_eq!(word, 0xFFFF_FFFE_u64);
    // The generation field's LSB (bit 32) must stay clear.
    assert_eq!(word >> 32, 0);
    // The state bit (bit 0) holds OWNER_STATE_LIVE (0).
    assert_eq!(word & 1, 0);
}

/// Test 2 — every id in the representable 31-bit domain round-trips through
/// `pack_owner`/`unpack_owner_id` with NO state-bit or generation-field
/// contamination: boundary ids (0, 1, 4095, 4096), every power of two and
/// power-of-two-minus-one up to 2^30, both dedicated sentinels, and a prime
/// stride sweep (every 6151st id — 6151 is prime — across the full 2^31
/// domain) for coverage no hand-picked list can fake.
#[test]
fn owner_id_round_trips_through_pack_owner() {
    let mut ids: Vec<u32> = vec![0, 1, 4095, 4096, OWNER_ID_FALLBACK, OWNER_ID_NONE];
    for k in 1..=30u32 {
        ids.push(1u32 << k);
        ids.push((1u32 << k) - 1);
    }
    // Stride sweep: every 6151st id from 0 to 2^31 (6151 is prime, so the
    // sampled ids hit every residue class of the small hand-picked list).
    let mut sweep: u64 = 0;
    while sweep < (1u64 << 31) {
        ids.push(sweep as u32);
        sweep += 6151;
    }

    for id in ids {
        let word = pack_owner(OWNER_STATE_LIVE, id, 0);
        assert_eq!(
            unpack_owner_id(word),
            id,
            "id {id:#x} failed to round-trip through pack_owner/unpack_owner_id"
        );
        // State bit is OWNER_STATE_LIVE (0) — no leakage into bit 0.
        assert_eq!(word & 1, 0, "state bit contaminated for id {id:#x}");
        // Generation field must stay zero — no leakage across bit 32.
        assert_eq!(
            word >> 32,
            0,
            "generation field contaminated for id {id:#x}"
        );
    }
}

/// Test 3 — the documented clamp: with the mask, `u32::MAX` packs to
/// `OWNER_ID_NONE` — i.e. `u32::MAX` is OUTSIDE the representable id domain
/// and must never be used as a stamped id (it clamps rather than
/// round-trips). This pins the `HeapCore::id` doc invariant and the
/// motivation for the `OWNER_ID_FALLBACK` sentinel.
#[test]
fn u32_max_clamps_instead_of_leaking() {
    assert_eq!(
        unpack_owner_id(pack_owner(OWNER_STATE_LIVE, u32::MAX, 0)),
        OWNER_ID_NONE
    );
}

/// Test 4 — the fallback heap's id is exactly the property the OPT-C
/// stamp-cache compare needs (`unpack_owner_id(pack_owner(...)) ==
/// self.id`), while remaining distinct from the unstamped sentinel AND
/// outside every real registry slot index (< MAX_HEAPS = 4096) so
/// owner-id→slot resolution keeps treating fallback stamps as out-of-range.
#[test]
fn fallback_heap_id_is_round_trip_stable_and_slot_distinct() {
    // The exact property the OPT-C compare performs.
    assert_eq!(
        unpack_owner_id(pack_owner(OWNER_STATE_LIVE, OWNER_ID_FALLBACK, 0)),
        OWNER_ID_FALLBACK
    );
    // Distinct from the unstamped-segment sentinel.
    assert_ne!(OWNER_ID_FALLBACK, OWNER_ID_NONE);
    // Outside every real registry slot index (>= MAX_HEAPS = 4096) and
    // representable in the 31-bit id field — both compile-time constants,
    // hence const-block asserts (plain `assert!` on constants is denied by
    // clippy `assertions_on_constants` under `-D warnings`).
    const {
        assert!(OWNER_ID_FALLBACK >= 4096);
        assert!(OWNER_ID_FALLBACK < (1u32 << 31));
    }
}

/// Test 5 — byte-stability of the FRESH-segment stamp word: the header
/// constructors (`SegmentHeader::small`/`large`) and the unregister reset
/// stamp `pack_owner(OWNER_STATE_LIVE, OWNER_ID_NONE, 0)`. The new mask must
/// not have changed that word — `OWNER_ID_NONE` (0x7FFF_FFFF) is < 2^31, so
/// masking is a no-op for it and the word stays exactly 0xFFFF_FFFE.
#[test]
fn fresh_segment_stamp_word_is_byte_stable() {
    assert_eq!(
        pack_owner(OWNER_STATE_LIVE, OWNER_ID_NONE, 0),
        0xFFFF_FFFE_u64
    );
}
