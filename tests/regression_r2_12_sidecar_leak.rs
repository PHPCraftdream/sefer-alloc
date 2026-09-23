//! R2-12 (P2, `docs/reviews/2026-09-22-120730-src-review-xa-round-2.md`) —
//! owner-only sidecar reservations must be RELEASED with the owning
//! standalone `AllocCore`, not leaked for the process lifetime.
//!
//! ## The defect (pre-fix)
//!
//! `SegmentDirectory` (the directory sidecar, including its NUMA-flavored
//! `NODE_BITMAPS`-multiplied span under `numa-aware`) and
//! `LargeCacheExtension` (the extended-cache sidecar) were reserved via
//! `aligned_vmem::leak_zeroed_pages` and never released: `AllocCore`'s
//! `Drop` released the segments and the cached large entries, but NOT the
//! sidecar spans themselves. The "one-time-per-heap, not a growing leak"
//! justification only holds for the bounded population of process-lifetime
//! registry heaps (bounded by `MAX_HEAPS`); it does NOT hold for the public
//! standalone `AllocCore::new`/drop path, which can be churned arbitrarily —
//! every cycle that materialises a sidecar permanently lost one VM span.
//!
//! ## The fix under test
//!
//! Each owner-only sidecar now owns its span through an
//! `AccountedSidecar` token (`platform::sidecar`), stored by the core and
//! dropped with it; references handed back by the deref boundary are
//! owner-tied instead of `'static`.
//!
//! ## The acceptance oracle (this file)
//!
//! `platform::sidecar_stats`' process-wide reservation/release counters,
//! broken out per sidecar kind (plain directory / NUMA directory /
//! extended cache), are read as DELTAS around a repeated standalone-core
//! create/drop lifecycle that materialises the sidecar under test:
//!
//! 1. every iteration's materialisation bumps its kind's `*_reservations`;
//! 2. after all cores dropped, `*_releases` has caught up EXACTLY —
//!    `releases_delta == reservations_delta` — i.e. every owner-scoped span
//!    is accounted released and nothing owner-scoped remains reserved.
//!
//! ## Why this is a genuine counterfactual (not vacuous)
//!
//! Before the fix, the release hook did not exist: `*_releases` stayed 0
//! forever while `*_reservations` grew by `CORES`, so assertion 2 fails
//! immediately (`left: 0, right: N`) while assertion 1 still passes — the
//! test pinpoints the missing release, not a missing materialisation.
//! The materialisation itself is separately proven non-vacuous per
//! iteration via the pre-existing `dbg_directory_is_materialised()` /
//! `dbg_large_cache_extension_materialised()` introspection (the counter
//! assertions are never trusted without the sidecar being observed
//! materialised).

#![cfg(feature = "internals")]
// The R2-12 accounting increments (and the `alloc_core` module path itself,
// R34-3) are `internals`-gated: with the feature off this file compiles to
// nothing.

#[cfg(any(feature = "alloc-segment-directory", feature = "large-cache-extended"))]
use std::sync::Mutex;

// Top-level import: `push_past_threshold`'s signature names `AllocCore`, so
// the path must be in scope at module level (mirrors the top-level
// `use sefer_alloc::{AllocCore, SegmentLayout};` in
// `tests/directory_authoritative_miss.rs`, the helper's source). Gated like
// `TEST_LOCK` so no unused import survives in any feature combination.
#[cfg(any(feature = "alloc-segment-directory", feature = "large-cache-extended"))]
use sefer_alloc::AllocCore;

/// Serialises the churn tests within this binary: the R2-12 counters are
/// PROCESS-GLOBAL statics shared by every `AllocCore` in the process, and
/// the assertions compare deltas across multi-step sequences (mirrors
/// `tests/directory_authoritative_miss.rs`'s `TEST_LOCK` discipline).
#[cfg(any(feature = "alloc-segment-directory", feature = "large-cache-extended"))]
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Allocate `SMALL_MAX`-sized blocks until `table.count()` crosses the
/// directory materialisation threshold (copied from
/// `tests/directory_authoritative_miss.rs`'s helper of the same name — the
/// established materialisation recipe: `SMALL_MAX` is the largest small
/// class, so each segment holds only a handful of such blocks and a few
/// hundred allocations span the required 32+ segments).
#[cfg(feature = "alloc-segment-directory")]
fn push_past_threshold(core: &mut AllocCore) -> Vec<*mut u8> {
    use std::alloc::Layout;

    use sefer_alloc::alloc_core::SegmentLayout;

    let threshold = AllocCore::dbg_directory_materialize_threshold() as usize;
    let small_max = SegmentLayout::SMALL_MAX;
    let layout = Layout::from_size_align(small_max, 1).unwrap();

    let mut ptrs: Vec<*mut u8> = Vec::new();
    let max_allocs = (threshold + 5) * 20;
    for _ in 0..max_allocs {
        let p = core.alloc(layout);
        assert!(!p.is_null(), "alloc returned null");
        ptrs.push(p);
        if core.dbg_table_count() > threshold as u32 {
            break;
        }
    }
    assert!(
        core.dbg_table_count() > threshold as u32,
        "failed to push table count past threshold"
    );
    ptrs
}

/// R2-12 acceptance (directory sidecar, BOTH flavors): churn standalone
/// cores through the directory-materialisation threshold and assert the
/// process-wide accounting ends perfectly balanced — every materialised
/// span released with its core. Under `numa-aware` the directory span is
/// the `NODE_BITMAPS`-multiplied one and routes into the NUMA-flavored
/// counters (the plain directory counters must stay flat); non-numa builds
/// exercise the plain pair.
#[test]
#[cfg(feature = "alloc-segment-directory")]
fn standalone_directory_sidecar_churn_releases_every_span() {
    use sefer_alloc::alloc_core::dbg_sidecar_reservation_stats;
    use sefer_alloc::AllocCore;

    let _guard = TEST_LOCK.lock().unwrap();
    let before = dbg_sidecar_reservation_stats();

    const CORES: u64 = 4;
    for _ in 0..CORES {
        let mut core = AllocCore::new().expect("primordial reservation");
        // Crossing the threshold materialises the directory sidecar
        // (proven per-iteration below) and bumps `*_RESERVATIONS`.
        let _ptrs = push_past_threshold(&mut core);
        assert!(
            core.dbg_directory_is_materialised(),
            "directory sidecar must be materialised past the threshold"
        );
        drop(core);
        // The span must be gone NOW (before the next iteration) — asserted
        // aggregate below via the release counter.
    }

    let after = dbg_sidecar_reservation_stats();
    #[cfg(not(feature = "numa-aware"))]
    {
        assert_eq!(
            after.directory_reservations - before.directory_reservations,
            CORES,
            "every churned core must have materialised the directory sidecar exactly once"
        );
        assert_eq!(
            after.directory_releases - before.directory_releases,
            CORES,
            "R2-12: every directory sidecar span must be RELEASED with its core \
             (pre-fix this stays 0 while reservations grow — the leak)"
        );
    }
    #[cfg(feature = "numa-aware")]
    {
        assert_eq!(
            after.numa_directory_reservations - before.numa_directory_reservations,
            CORES,
            "every churned core must have materialised the NUMA directory sidecar exactly once"
        );
        assert_eq!(
            after.numa_directory_releases - before.numa_directory_releases,
            CORES,
            "R2-12: every NODE_BITMAPS-multiplied NUMA directory span must be RELEASED \
             with its core (pre-fix this stays 0 while reservations grow — the leak)"
        );
        assert_eq!(
            after.directory_reservations - before.directory_reservations,
            0,
            "under numa-aware no directory materialisation may route into the \
             non-NUMA counter (the two flavors are disjoint)"
        );
    }
}

/// R2-12 acceptance (extended-cache sidecar): fill the base 8 large-cache
/// slots with 9 same-usable-size large segments and free them one after
/// another (no intervening alloc, so no cache hit can drain a slot) — the
/// 9th deposit has no base slot left and materialises the extension
/// sidecar. Then drop the core and assert the extension span is released.
#[test]
#[cfg(all(feature = "large-cache-extended", feature = "alloc-decommit"))]
fn standalone_large_cache_extension_sidecar_churn_releases_every_span() {
    use std::alloc::Layout;

    use sefer_alloc::alloc_core::dbg_sidecar_reservation_stats;
    use sefer_alloc::alloc_core::SegmentLayout;
    use sefer_alloc::AllocCore;

    let _guard = TEST_LOCK.lock().unwrap();
    let before = dbg_sidecar_reservation_stats();

    const CORES: u64 = 3;
    // LARGE_CACHE_SLOTS is 8 (the `pub(super)` const in
    // `alloc_core/alloc_core/mod.rs`); +1 deposit forces the first
    // EXTENSION slot. The `dbg_large_cache_extension_materialised` asserts
    // below fail loudly if that constant ever changes.
    const BASE_SLOTS_PLUS_ONE: usize = 9;
    // Any size above SMALL_MAX is a Large allocation with its own dedicated
    // segment, and all 9 identical requests resolve to ONE identical usable
    // size (under either usable-size computation — segment-rounded or
    // exact-span). That is fine: deposits take ANY free slot, they are not
    // size-keyed, so 9 simultaneous same-size deposits fill the 8 base
    // slots and overflow into the extension.
    let large_size = SegmentLayout::SMALL_MAX + 1024;
    let layout = Layout::from_size_align(large_size, 16).unwrap();

    for _ in 0..CORES {
        let mut core = AllocCore::new().expect("primordial reservation");
        let mut ptrs: Vec<*mut u8> = Vec::new();
        for _ in 0..BASE_SLOTS_PLUS_ONE {
            let p = core.alloc(layout);
            assert!(!p.is_null(), "large alloc returned null");
            ptrs.push(p);
        }
        assert!(
            !core.dbg_large_cache_extension_materialised(),
            "extension must not materialise before the base slots fill"
        );
        // Freeing all 9 in a row: each dealloc deposits its segment into
        // the cache; the 9th finds every base slot occupied and
        // materialises the extension sidecar.
        for p in ptrs.drain(..) {
            // SAFETY: `p` was returned by `core.alloc(layout)` above with
            // the same layout, and is still live (drained, not yet freed).
            unsafe { core.dealloc(p, layout) };
        }
        assert!(
            core.dbg_large_cache_extension_materialised(),
            "extension sidecar must materialise on the 9th deposit"
        );
        drop(core);
    }

    let after = dbg_sidecar_reservation_stats();
    assert_eq!(
        after.large_cache_extension_reservations - before.large_cache_extension_reservations,
        CORES,
        "every churned core must have materialised the extension sidecar exactly once"
    );
    assert_eq!(
        after.large_cache_extension_releases - before.large_cache_extension_releases,
        CORES,
        "R2-12: every extended-cache sidecar span must be RELEASED with its core \
         (pre-fix this stays 0 while reservations grow — the leak)"
    );
}
