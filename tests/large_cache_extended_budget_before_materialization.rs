//! R14-5 (task #290) — two hardening fixes to the `large-cache-extended`
//! sidecar's admission path, verified here:
//!
//! 1. **Budget-before-materialisation ordering** (Round 13 review finding
//!    @fm P3): a deposit that the byte-budget will unconditionally reject
//!    (because the deposit's `usable_size` alone exceeds the configured
//!    budget, so even a fully-evicted cache could never admit it) must NEVER
//!    trigger sidecar materialisation. Before this task, both admission call
//!    sites (`AllocCore::dealloc`'s Large branch, `reclaim_large_segment`)
//!    called `large_cache_find_free_slot()` — which lazily reserves a whole
//!    OS page for the sidecar the first time the base 8 slots are full —
//!    BEFORE ever checking whether the budget would accept the deposit at
//!    all. A tiny/zero budget under high-turnover Large churn therefore paid
//!    a real page reservation on every rejected deposit once the base 8
//!    filled, for a sidecar that could never hold anything (every deposit
//!    into it would also be budget-rejected).
//! 2. **Finite default budget for `large-cache-extended`**: with the feature
//!    compiled in and no explicit `.budget_bytes(..)` call, the config now
//!    resolves to `DEFAULT_EXTENDED_BUDGET_BYTES` (5x the 256 MiB headroom
//!    default = 1280 MiB) instead of `None` (unbounded) — see
//!    `large_cache_config.rs`'s doc for the full rationale. An explicit
//!    `.budget_bytes(..)` call always overrides this fallback.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "large-cache-extended"
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, LargeCacheConfig, SegmentLayout};

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).unwrap()
}

/// Same runtime-computed, density-agnostic size list the sibling
/// `large_cache_extended_*` test files use — see
/// `large_cache_extended_materializes_on_overflow.rs`'s module doc for the
/// full derivation rationale (kept duplicated per that file's own stated
/// reason: no shared test-support crate exists for `tests/`).
fn large_test_sizes(n: usize) -> Vec<usize> {
    let segment = SegmentLayout::SEGMENT;
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let mut size = (2 * small_max).div_ceil(segment).max(1) * segment;
    let mut sizes = Vec::with_capacity(n);
    for _ in 0..n {
        sizes.push(size);
        size = (2 * size + 1).div_ceil(segment) * segment;
    }
    sizes
}

/// A budget of exactly `0` (cache disabled — every deposit is unconditionally
/// rejected, per `LargeCacheConfig::budget_bytes`'s own doc) must NEVER
/// materialise the extension sidecar, even after depositing well past the
/// base 8 slots' worth of distinct sizes. With budget=0 the base 8 slots
/// never actually fill (every deposit is evicted/rejected immediately), so
/// this exercises the "budget=0 is checked before ANY slot search" case —
/// distinct from the fuller counterfactual below, which specifically forces
/// the base 8 to be genuinely OCCUPIED before the infeasible deposit lands.
#[test]
fn zero_budget_never_materialises_extension_even_past_base_eight() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(Some(0));

    assert!(
        !ac.dbg_large_cache_extension_materialised(),
        "extension must start unmaterialised"
    );

    for &bytes in &large_test_sizes(16) {
        let l = layout(bytes);
        let p = ac.alloc(l);
        if p.is_null() {
            eprintln!("OOM at {bytes} bytes — stopping early (host memory pressure)");
            break;
        }
        // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
        // freed exactly once here. With budget=0 the cache admits nothing,
        // so this releases the span to the OS immediately.
        unsafe { ac.dealloc(p, l) };

        assert!(
            !ac.dbg_large_cache_extension_materialised(),
            "budget=0 must never materialise the sidecar for size {bytes} — every \
             deposit is unconditionally budget-infeasible and must be rejected \
             BEFORE `large_cache_find_free_slot` is ever called"
        );
        assert_eq!(
            ac.dbg_large_cache_used(),
            0,
            "budget=0 must keep the cache empty at all times"
        );
    }

    assert_eq!(
        ac.dbg_large_cache_total_slots(),
        8,
        "total slots must remain 8 (base only) — the sidecar was never materialised"
    );
}

/// The DISCRIMINATING counterfactual for the budget-before-materialisation
/// ordering fix: fills the base 8 slots with 8 distinct, GENUINELY RESIDENT
/// spans (a large enough budget that none of them get evicted), THEN
/// deposits a 9th, much-larger span under a budget too small to EVER admit
/// it (`usable_size_9 > budget`) even against a fully-evicted cache.
///
/// Under the OLD ordering (materialise-before-budget-check), this 9th
/// deposit would have found the base 8 full, materialised the extension
/// sidecar looking for a free slot, discovered the sidecar's slot was ALSO
/// budget-infeasible, and only then given up — paying a real OS page
/// reservation for a deposit that could never have been admitted. Under the
/// FIXED ordering, `large_cache_deposit_budget_infeasible` rejects the 9th
/// deposit before `large_cache_find_free_slot` is ever called, so the
/// sidecar must stay unmaterialised and the base 8 must remain untouched
/// (the 9th deposit's rejection releases it to the OS directly, without
/// touching/evicting any of the resident 8).
#[test]
fn budget_infeasible_deposit_after_base_eight_full_never_materialises_extension() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None); // isolate: fill the base 8 first, unbounded

    let sizes = large_test_sizes(9);
    let base_eight = &sizes[..8];
    let ninth = sizes[8];

    for &bytes in base_eight {
        let l = layout(bytes);
        let p = ac.alloc(l);
        assert!(!p.is_null(), "alloc of {bytes} bytes failed unexpectedly");
        // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
        // freed exactly once here.
        unsafe { ac.dealloc(p, l) };
    }
    assert_eq!(
        ac.dbg_large_cache_slot_sizes()
            .iter()
            .filter(|s| s.is_some())
            .count(),
        8,
        "all 8 base slots must be genuinely resident before the 9th deposit"
    );
    assert!(
        !ac.dbg_large_cache_extension_materialised(),
        "sidecar must not have materialised yet — the base 8 alone absorbed all 8 deposits"
    );

    // Budget = the ACTUAL accumulated `usable_size` across the resident base
    // 8 (read back via `dbg_large_cache_used`, not recomputed from the raw
    // `bytes` requested — the cache's real `usable_size` per slot can differ
    // from the raw request after page/segment rounding, so re-deriving from
    // the raw list would silently mismatch the real admission arithmetic).
    // This budget exactly covers the resident 8 (so none of them individually
    // violates it) but is smaller than the 9th deposit's own `usable_size`
    // alone — unconditionally infeasible for the 9th, even against a
    // fully-evicted cache. `large_test_sizes` grows each step `> 2x` the
    // previous one, so the 9th alone dwarfs the sum of the first 8 —
    // verified by the assertion immediately below.
    let tight_budget = ac.dbg_large_cache_used();
    ac.dbg_set_large_cache_budget(Some(tight_budget));

    let l9 = layout(ninth);
    let p9 = ac.alloc(l9);
    assert!(!p9.is_null(), "alloc of {ninth} bytes failed unexpectedly");
    let ninth_usable = {
        // Read the actual usable_size the header carries for this pointer by
        // depositing it and inspecting which case applies: since we expect
        // it to be REJECTED (not cached), we instead just assert on the
        // budget/request relationship directly — the raw requested `ninth`
        // is already a lower bound on the real `usable_size` (rounding only
        // grows a request, never shrinks it), so `ninth > tight_budget`
        // below is sufficient without reading the header back.
        ninth
    };
    assert!(
        ninth_usable > tight_budget,
        "test precondition: the 9th size ({ninth_usable}) must exceed the resident \
         base-8 budget ({tight_budget}) for this counterfactual to be meaningful \
         (rounding only grows a request, so this bound is conservative)"
    );
    // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
    // freed exactly once here. Budget-infeasible: must release to the OS
    // without ever touching the sidecar.
    unsafe { ac.dealloc(p9, l9) };

    assert!(
        !ac.dbg_large_cache_extension_materialised(),
        "an unconditionally budget-infeasible 9th deposit (usable_size > budget \
         even against a fully-evicted cache) must NEVER materialise the sidecar"
    );
    assert_eq!(
        ac.dbg_large_cache_slot_sizes()
            .iter()
            .filter(|s| s.is_some())
            .count(),
        8,
        "the resident base 8 must be untouched by the rejected 9th deposit \
         (an infeasible deposit must not trigger any eviction either — \
         eviction only happens inside the admission loop, which the \
         pre-check now skips entirely)"
    );
}

/// Companion case: a small but NON-zero budget (smaller than any of the
/// test's Large sizes) is likewise unconditionally infeasible for every
/// deposit, so the sidecar must stay unmaterialised — this is the general
/// form of the `budget=0` case above (any budget smaller than the smallest
/// deposit size behaves the same way for THAT deposit).
#[test]
fn budget_smaller_than_every_span_never_materialises_extension() {
    let mut ac = AllocCore::new().expect("primordial");
    let sizes = large_test_sizes(16);
    let smallest = *sizes.iter().min().unwrap();
    // One byte smaller than the smallest test size: every deposit in this
    // test is therefore unconditionally infeasible under this budget.
    ac.dbg_set_large_cache_budget(Some(smallest - 1));

    for &bytes in &sizes {
        let l = layout(bytes);
        let p = ac.alloc(l);
        if p.is_null() {
            eprintln!("OOM at {bytes} bytes — stopping early (host memory pressure)");
            break;
        }
        // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
        // freed exactly once here.
        unsafe { ac.dealloc(p, l) };
        assert!(
            !ac.dbg_large_cache_extension_materialised(),
            "a budget smaller than every deposited span must never materialise \
             the sidecar (deposit of {bytes} bytes was unconditionally \
             budget-infeasible)"
        );
    }
}

/// Sanity companion: with an UNBOUNDED budget (explicit `Some(usize::MAX)`,
/// recovering the pre-R14-5 unbounded behaviour per
/// `DEFAULT_EXTENDED_BUDGET_BYTES`'s doc), the sidecar DOES still
/// materialise once the base 8 overflow — proving the budget-infeasibility
/// pre-check does not accidentally suppress materialisation for a genuinely
/// admissible deposit.
#[test]
fn effectively_unbounded_budget_still_materialises_extension_on_overflow() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(Some(usize::MAX));

    let sizes = large_test_sizes(9);
    for &bytes in &sizes {
        let l = layout(bytes);
        let p = ac.alloc(l);
        assert!(!p.is_null(), "alloc of {bytes} bytes failed unexpectedly");
        // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
        // freed exactly once here.
        unsafe { ac.dealloc(p, l) };
    }

    assert!(
        ac.dbg_large_cache_extension_materialised(),
        "a genuinely admissible 9th deposit must still materialise the \
         sidecar — the budget-infeasibility pre-check must not suppress \
         legitimate materialisation"
    );
}

/// R14-5 item 2: with `large-cache-extended` compiled in and NO explicit
/// `.budget_bytes(..)` call, `AllocCore::new()` (which threads
/// `LargeCacheConfig::DEFAULT` through `new_with_config`) must resolve to
/// the finite `DEFAULT_EXTENDED_BUDGET_BYTES` fallback, not `None`
/// (unbounded).
#[test]
fn default_config_resolves_finite_budget_when_extension_compiled_in() {
    let ac = AllocCore::new().expect("primordial");
    let resolved = ac.dbg_large_cache_budget();
    assert!(
        resolved.is_some(),
        "large-cache-extended must resolve a FINITE default budget when the \
         caller never calls .budget_bytes(..) — got None (unbounded), which \
         reintroduces the unbounded-RSS-retention hazard this task closes"
    );
    let budget = resolved.unwrap();
    assert!(
        budget > 0,
        "the finite default must be a genuine positive ceiling, not budget=0 \
         (which would silently disable caching entirely)"
    );
    // 5x the 256 MiB headroom default, per `DEFAULT_EXTENDED_BUDGET_BYTES`'s
    // doc — pin the exact value so a future change to the ratio is a visible,
    // deliberate diff here, not a silent drift.
    const EXPECTED: usize = 5 * 256 * 1024 * 1024;
    assert_eq!(
        budget, EXPECTED,
        "default extended-cache budget must be exactly 5x the 256 MiB headroom \
         default (1280 MiB) per large_cache_config.rs's documented policy"
    );
}

/// An explicit `.budget_bytes(..)` call must always override the
/// `large-cache-extended` default fallback — including a value LARGER than
/// the default (recovering effectively-unbounded behaviour for a caller who
/// has measured their own workload).
#[test]
fn explicit_budget_bytes_overrides_the_extended_default() {
    let cfg = LargeCacheConfig::new().budget_bytes(usize::MAX);
    let ac = AllocCore::new_with_config(cfg).expect("primordial");
    assert_eq!(
        ac.dbg_large_cache_budget(),
        Some(usize::MAX),
        "an explicit .budget_bytes(..) call must always win over the \
         large-cache-extended default fallback"
    );

    let cfg_small = LargeCacheConfig::new().budget_bytes(4096);
    let ac_small = AllocCore::new_with_config(cfg_small).expect("primordial");
    assert_eq!(
        ac_small.dbg_large_cache_budget(),
        Some(4096),
        "an explicit small .budget_bytes(..) call must also win over the default"
    );
}
