//! R13-7 (task #277) — `large_cache_budget_bytes` remains the PRIMARY control
//! on total cached RSS even once the `large-cache-extended` sidecar exists:
//! a small budget still bounds the cache tightly, regardless of how many
//! slots (8 base, or up to 40 with the extension materialised) are
//! available. Extending slot COUNT must never bypass the byte-budget check.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "large-cache-extended"
))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

const MIB: usize = 1024 * 1024;

fn layout(mib: usize) -> Layout {
    Layout::from_size_align(mib * MIB, 8).unwrap()
}

/// Check the running invariant across BOTH base and extension slots:
/// `dbg_large_cache_used == sum(base slot sizes) + sum(extension slot sizes)`.
fn assert_used_bytes_invariant(ac: &AllocCore) {
    let base_sum: usize = ac
        .dbg_large_cache_slot_sizes()
        .iter()
        .filter_map(|s| *s)
        .sum();
    let ext_sum: usize = ac
        .dbg_large_cache_extended_slot_sizes()
        .iter()
        .filter_map(|s| *s)
        .sum();
    let expected = base_sum + ext_sum;
    assert_eq!(
        ac.dbg_large_cache_used(),
        expected,
        "large_cache_used_bytes invariant violated across base+extension: counter={}, sum={}",
        ac.dbg_large_cache_used(),
        expected
    );
}

/// With a budget of exactly ONE span, cycling through 16 distinct
/// non-aliasing sizes must NEVER let more than one span's worth of bytes be
/// resident at once — even though the extension sidecar has plenty of FREE
/// SLOTS to admit more, the byte-budget check must still evict/reject before
/// any deposit that would exceed it.
#[test]
fn small_budget_caps_cache_even_with_extension_available() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    // Discover one span's real usable_size (4 MiB request).
    let l4 = layout(4);
    let p = ac.alloc(l4);
    let Some(p) = (!p.is_null()).then_some(p) else {
        eprintln!("OOM — skip small_budget_caps_cache_even_with_extension_available");
        return;
    };
    // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
    // freed exactly once here.
    unsafe { ac.dealloc(p, l4) };
    let one_span = ac.dbg_large_cache_used();
    assert!(one_span > 0, "first dealloc must cache the span");

    // Now cap the budget at exactly one span and cycle 16 distinct sizes —
    // more than the 8 base slots, so the extension WOULD materialise on a
    // slot-count basis alone if the budget did not intervene first.
    ac.dbg_set_large_cache_budget(Some(one_span));

    let sizes_mib: [usize; 16] = [
        4, 16, 64, 256, 1024, 4096, 16384, 65536, 262144, 1048576, 20, 80, 320, 1280, 5120, 20480,
    ];

    for &mib in &sizes_mib {
        let l = layout(mib);
        let pp = ac.alloc(l);
        if pp.is_null() {
            eprintln!("OOM at {mib} MiB — stopping cycle early");
            break;
        }
        // SAFETY (R6-MS-1/2): pointer from the alloc immediately above,
        // live, freed exactly once here.
        unsafe { ac.dealloc(pp, l) };
        assert_used_bytes_invariant(&ac);
        assert!(
            ac.dbg_large_cache_used() <= one_span,
            "used_bytes {} must never exceed the one-span budget {one_span} \
             (checked after depositing {mib} MiB)",
            ac.dbg_large_cache_used()
        );
    }

    // Final check: total occupied slots (base + extension) times their sizes
    // must still respect the budget.
    assert_used_bytes_invariant(&ac);
    assert!(ac.dbg_large_cache_used() <= one_span);
}

/// A budget of `2 * one_span` with 16 distinct sizes: at most 2 spans'
/// worth of bytes may be resident, but which 2 slots hold them can spill
/// into the extension once the base 8 are exercised — the OCCUPIED SLOT
/// COUNT is allowed to vary, but total bytes must not.
#[test]
fn budget_bounds_bytes_not_slot_count() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    let l4 = layout(4);
    let p = ac.alloc(l4);
    let Some(p) = (!p.is_null()).then_some(p) else {
        eprintln!("OOM — skip budget_bounds_bytes_not_slot_count");
        return;
    };
    // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
    // freed exactly once here.
    unsafe { ac.dealloc(p, l4) };
    let one_span = ac.dbg_large_cache_used();
    assert!(one_span > 0);

    ac.dbg_set_large_cache_budget(Some(one_span * 2));

    let sizes_mib: [usize; 16] = [
        4, 16, 64, 256, 1024, 4096, 16384, 65536, 262144, 1048576, 20, 80, 320, 1280, 5120, 20480,
    ];
    for &mib in &sizes_mib {
        let l = layout(mib);
        let pp = ac.alloc(l);
        if pp.is_null() {
            break;
        }
        // SAFETY (R6-MS-1/2): pointer from the alloc immediately above,
        // live, freed exactly once here.
        unsafe { ac.dealloc(pp, l) };
        assert!(
            ac.dbg_large_cache_used() <= one_span * 2,
            "used_bytes {} must never exceed the 2-span budget {}",
            ac.dbg_large_cache_used(),
            one_span * 2
        );
    }
}
