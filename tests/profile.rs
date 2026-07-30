//! R30-7 (task #456) — config-verification tests for [`Profile`]'s three
//! named presets (`Rss`/`Balanced`/`Throughput`).
//!
//! Each profile claims to set the small-pool pair (`pool_segments` +
//! `pool_byte_cap`) AND the large-cache `headroom_bytes` together,
//! coherently, per the module doc's citations to R27-3/R27-4 (small pool)
//! and R30-6 (large cache). This file proves the RESOLVED config (read back
//! via `AllocCore::dbg_pool_cap` / `AllocCore::dbg_decay_config`, the same
//! diagnostic surface `tests/small_segment_pool.rs` and
//! `tests/large_cache_config_knobs.rs` already use for this exact purpose —
//! not merely the requested builder value) matches what each profile's docs
//! claim, and additionally proves the R27-1 no-op trap does NOT reappear for
//! any profile (i.e. `pool_byte_cap` is large enough that `pool_segments`
//! actually binds).

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use sefer_alloc::{AllocCore, LargeCacheConfig, Profile};

const MIB: usize = 1024 * 1024;

/// `Profile::Rss` resolves to the `production` small-pool default (4
/// segments / 16 MiB, i.e. effective cap 4) + a 16 MiB large-cache headroom.
#[test]
fn rss_profile_resolves_as_documented() {
    let cfg = LargeCacheConfig::for_profile(Profile::Rss);
    let ac = AllocCore::new_with_config(cfg).expect("primordial");

    assert_eq!(
        ac.dbg_pool_cap(),
        4,
        "Profile::Rss must resolve the small-pool cap to the production \
         default (4 segments)"
    );

    let (_, _, headroom_bytes) = ac.dbg_decay_config();
    assert_eq!(
        headroom_bytes,
        16 * MIB,
        "Profile::Rss must resolve the large-cache headroom to 16 MiB"
    );
}

/// `Profile::Balanced` resolves to the `production` small-pool default (4
/// segments / 16 MiB) + a 64 MiB large-cache headroom (R30-6's measured
/// full-hit-rate-parity point).
#[test]
fn balanced_profile_resolves_as_documented() {
    let cfg = LargeCacheConfig::for_profile(Profile::Balanced);
    let ac = AllocCore::new_with_config(cfg).expect("primordial");

    assert_eq!(
        ac.dbg_pool_cap(),
        4,
        "Profile::Balanced must keep the small pool at the production \
         default (4 segments) — no additional small-pool retention cost"
    );

    let (_, _, headroom_bytes) = ac.dbg_decay_config();
    assert_eq!(
        headroom_bytes,
        64 * MIB,
        "Profile::Balanced must resolve the large-cache headroom to 64 MiB"
    );
}

/// `Profile::Throughput` resolves to the R27-4 paired candidate (8 segments
/// / 32 MiB, i.e. effective cap 8 — NOT the R27-1 no-op) + a 64 MiB
/// large-cache headroom.
#[test]
fn throughput_profile_resolves_as_documented() {
    let cfg = LargeCacheConfig::for_profile(Profile::Throughput);
    let ac = AllocCore::new_with_config(cfg).expect("primordial");

    assert_eq!(
        ac.dbg_pool_cap(),
        8,
        "Profile::Throughput must resolve the small-pool cap to 8 — if this \
         is 4, the R27-1 no-op trap has reappeared (pool_byte_cap did not \
         rise together with pool_segments)"
    );

    let (_, _, headroom_bytes) = ac.dbg_decay_config();
    assert_eq!(
        headroom_bytes,
        64 * MIB,
        "Profile::Throughput must resolve the large-cache headroom to 64 MiB"
    );
}

/// Every profile's resolved cap must differ from the byte-cap-starved
/// no-op value (R27-1) — i.e. for any profile requesting `pool_segments >
/// production_default`, the paired `pool_byte_cap` must have risen enough
/// to let it actually bind. This is the general counterfactual mirroring
/// `tests/small_segment_pool.rs::paired_knob_promotion_is_not_a_noop`,
/// applied to every profile rather than one hand-picked pair.
#[test]
fn no_profile_reintroduces_the_r27_1_noop_trap() {
    for profile in [Profile::Rss, Profile::Balanced, Profile::Throughput] {
        let cfg = LargeCacheConfig::for_profile(profile);
        let ac = AllocCore::new_with_config(cfg).expect("primordial");
        let resolved_cap = ac.dbg_pool_cap();

        // Reconstruct the byte-cap-only clamp `min(pool_segments,
        // pool_byte_cap / SEGMENT)` cannot silently be less than what the
        // profile's own resolved cap already proves — i.e. the resolved cap
        // is never LOWER than what a well-formed (non-no-op) profile must
        // deliver: 4 for Rss/Balanced, 8 for Throughput.
        let expected_min = match profile {
            Profile::Rss | Profile::Balanced => 4,
            Profile::Throughput => 8,
            _ => unreachable!("test only iterates the three known variants"),
        };
        assert_eq!(
            resolved_cap, expected_min,
            "{profile:?} resolved to an unexpected small-pool cap — check \
             pool_segments/pool_byte_cap moved together"
        );
    }
}
