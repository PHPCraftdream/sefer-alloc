//! R30-7 (task #456), reworked R31-9 (task #473) — config-verification
//! tests for [`Profile`]'s two independent axes ([`SmallPoolPolicy`] /
//! [`LargeCachePolicy`]).
//!
//! Each axis claims to set its own knob(s) — [`SmallPoolPolicy`] the
//! small-pool pair (`pool_segments` + `pool_byte_cap`), [`LargeCachePolicy`]
//! the large-cache `headroom_bytes` — per the module doc's citations to
//! R27-3/R27-4 (small pool) and R30-6/R31-1 (large cache). This file proves
//! the RESOLVED config (read back via `AllocCore::dbg_pool_cap` /
//! `AllocCore::dbg_decay_config`, the same diagnostic surface
//! `tests/small_segment_pool.rs` and `tests/large_cache_config_knobs.rs`
//! already use for this exact purpose — not merely the requested builder
//! value) matches what each axis's docs claim, that the two axes compose
//! independently (all six 2×3 combinations resolve as expected, not just
//! the two combinations R30-7's original three-arm enum happened to name),
//! and that the R27-1 no-op trap does NOT reappear for any small-pool
//! policy (i.e. `pool_byte_cap` is large enough that `pool_segments`
//! actually binds).

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeCachePolicy, Profile, SmallPoolPolicy};

const MIB: usize = 1024 * 1024;

/// `Profile::new()` (== `Profile::DEFAULT`) resolves to the `production`
/// small-pool default (4 segments / 16 MiB, i.e. effective cap 4) + the
/// `production` large-cache default (256 MiB headroom) — byte-identical to
/// `SeferAlloc::new()` / `LargeCacheConfig::DEFAULT`.
#[test]
fn default_profile_matches_seferalloc_new_defaults() {
    let cfg = LargeCacheConfig::for_profile(Profile::new());
    let ac = AllocCore::new_with_config(cfg).expect("primordial");

    assert_eq!(
        ac.dbg_pool_cap(),
        4,
        "Profile::new() must resolve the small-pool cap to the production \
         default (4 segments)"
    );

    let (_, _, headroom_bytes) = ac.dbg_decay_config();
    assert_eq!(
        headroom_bytes,
        256 * MIB,
        "Profile::new() must resolve the large-cache headroom to the \
         production default (256 MiB) — it must NOT silently narrow the \
         cache the way the old Profile::Throughput did"
    );

    // Byte-identical to LargeCacheConfig::DEFAULT's own resolution.
    let default_cfg = LargeCacheConfig::DEFAULT;
    let default_ac = AllocCore::new_with_config(default_cfg).expect("primordial");
    assert_eq!(ac.dbg_pool_cap(), default_ac.dbg_pool_cap());
    assert_eq!(ac.dbg_decay_config(), default_ac.dbg_decay_config());
}

/// `Profile::default()` (the `Default` trait impl) matches `Profile::DEFAULT`
/// / `Profile::new()`.
#[test]
fn profile_default_trait_matches_the_const() {
    let cfg_trait = LargeCacheConfig::for_profile(Profile::default());
    let cfg_const = LargeCacheConfig::for_profile(Profile::DEFAULT);
    let ac_trait = AllocCore::new_with_config(cfg_trait).expect("primordial");
    let ac_const = AllocCore::new_with_config(cfg_const).expect("primordial");
    assert_eq!(ac_trait.dbg_pool_cap(), ac_const.dbg_pool_cap());
    assert_eq!(ac_trait.dbg_decay_config(), ac_const.dbg_decay_config());
}

/// `LargeCachePolicy::LowHeadroom` alone (small pool left at `Default`)
/// resolves the small pool to the production default (4 segments) and the
/// large-cache headroom to 16 MiB — the two axes are independent: setting
/// ONLY the large-cache axis must not perturb the small-pool axis.
#[test]
fn low_headroom_alone_does_not_perturb_small_pool_axis() {
    let cfg =
        LargeCacheConfig::for_profile(Profile::new().large_cache(LargeCachePolicy::LowHeadroom));
    let ac = AllocCore::new_with_config(cfg).expect("primordial");

    assert_eq!(
        ac.dbg_pool_cap(),
        4,
        "setting only LargeCachePolicy must not change the small-pool axis"
    );

    let (_, _, headroom_bytes) = ac.dbg_decay_config();
    assert_eq!(
        headroom_bytes,
        16 * MIB,
        "LargeCachePolicy::LowHeadroom must resolve the large-cache headroom \
         to 16 MiB"
    );
}

/// `SmallPoolPolicy::Throughput` alone (large cache left at `Default`)
/// resolves the small pool to the R27-4 paired candidate (8 segments / 32
/// MiB) and the large-cache headroom to the production default (256 MiB) —
/// the two axes are independent: setting ONLY the small-pool axis must not
/// perturb the large-cache axis. This is the concrete fix for defect #2
/// (the old bundled `Profile::Throughput` silently lowered the large-cache
/// window to 64 MiB with no same-regime evidence at the time; now that
/// choice is opt-in and separate).
#[test]
fn throughput_small_pool_alone_does_not_perturb_large_cache_axis() {
    let cfg = LargeCacheConfig::for_profile(Profile::new().small_pool(SmallPoolPolicy::Throughput));
    let ac = AllocCore::new_with_config(cfg).expect("primordial");

    assert_eq!(
        ac.dbg_pool_cap(),
        8,
        "SmallPoolPolicy::Throughput must resolve the small-pool cap to 8 — \
         if this is 4, the R27-1 no-op trap has reappeared (pool_byte_cap \
         did not rise together with pool_segments)"
    );

    let (_, _, headroom_bytes) = ac.dbg_decay_config();
    assert_eq!(
        headroom_bytes,
        256 * MIB,
        "setting only SmallPoolPolicy must not change the large-cache axis \
         away from the production default"
    );
}

/// Both axes set together (`SmallPoolPolicy::Throughput` +
/// `LargeCachePolicy::Trimmed64MiB`) resolves to (8 segments / 32 MiB)
/// small pool + 64 MiB large-cache headroom — the exact combination the OLD
/// bundled `Profile::Throughput` enum variant used to set, now reachable by
/// explicitly composing both axes.
#[test]
fn both_axes_together_compose_as_expected() {
    let cfg = LargeCacheConfig::for_profile(
        Profile::new()
            .small_pool(SmallPoolPolicy::Throughput)
            .large_cache(LargeCachePolicy::Trimmed64MiB),
    );
    let ac = AllocCore::new_with_config(cfg).expect("primordial");

    assert_eq!(ac.dbg_pool_cap(), 8);
    let (_, _, headroom_bytes) = ac.dbg_decay_config();
    assert_eq!(headroom_bytes, 64 * MIB);
}

/// All six 2x3 combinations of the two axes (2 `SmallPoolPolicy` values x 3
/// `LargeCachePolicy` values) resolve independently and correctly — the
/// full cross product, not just the two points R30-7's original three-arm
/// enum happened to name.
#[test]
fn all_axis_combinations_resolve_independently() {
    for small_pool in [SmallPoolPolicy::Default, SmallPoolPolicy::Throughput] {
        for large_cache in [
            LargeCachePolicy::LowHeadroom,
            LargeCachePolicy::Trimmed64MiB,
            LargeCachePolicy::Default,
        ] {
            let profile = Profile::new()
                .small_pool(small_pool)
                .large_cache(large_cache);
            let cfg = LargeCacheConfig::for_profile(profile);
            let ac = AllocCore::new_with_config(cfg).expect("primordial");

            let expected_pool_cap = match small_pool {
                SmallPoolPolicy::Default => 4,
                SmallPoolPolicy::Throughput => 8,
                _ => unreachable!("test only iterates the two known SmallPoolPolicy variants"),
            };
            let expected_headroom = match large_cache {
                LargeCachePolicy::LowHeadroom => 16 * MIB,
                LargeCachePolicy::Trimmed64MiB => 64 * MIB,
                LargeCachePolicy::Default => 256 * MIB,
                _ => unreachable!("test only iterates the three known LargeCachePolicy variants"),
            };

            assert_eq!(
                ac.dbg_pool_cap(),
                expected_pool_cap,
                "small_pool={small_pool:?} large_cache={large_cache:?}: \
                 wrong resolved pool cap"
            );
            let (_, _, headroom_bytes) = ac.dbg_decay_config();
            assert_eq!(
                headroom_bytes, expected_headroom,
                "small_pool={small_pool:?} large_cache={large_cache:?}: \
                 wrong resolved headroom"
            );
        }
    }
}

/// Every `SmallPoolPolicy` value's resolved cap must differ from the
/// byte-cap-starved no-op value (R27-1) — i.e. for `Throughput` (which
/// requests `pool_segments > production_default`), the paired
/// `pool_byte_cap` must have risen enough to let it actually bind. Mirrors
/// `tests/small_segment_pool.rs::paired_knob_promotion_is_not_a_noop`,
/// applied to every `SmallPoolPolicy` value.
#[test]
fn no_small_pool_policy_reintroduces_the_r27_1_noop_trap() {
    for policy in [SmallPoolPolicy::Default, SmallPoolPolicy::Throughput] {
        let cfg = LargeCacheConfig::for_profile(Profile::new().small_pool(policy));
        let ac = AllocCore::new_with_config(cfg).expect("primordial");
        let resolved_cap = ac.dbg_pool_cap();

        let expected_min = match policy {
            SmallPoolPolicy::Default => 4,
            SmallPoolPolicy::Throughput => 8,
            _ => unreachable!("test only iterates the two known SmallPoolPolicy variants"),
        };
        assert_eq!(
            resolved_cap, expected_min,
            "{policy:?} resolved to an unexpected small-pool cap — check \
             pool_segments/pool_byte_cap moved together"
        );
    }
}
