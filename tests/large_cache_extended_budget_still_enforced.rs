//! A materialised extension does not bypass the byte budget.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "large-cache-extended",
    feature = "internals"
))]

#[path = "support/r6_bounded_large_cache.rs"]
mod bounded;

use sefer_alloc::AllocCore;

fn check_budget(span_limit: usize) {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);
    let mut live = bounded::allocate_live(&mut ac, &[bounded::large_request(); 10]);
    let newcomer = live.pop().expect("tenth live reservation");
    bounded::deposit_all(&mut ac, live);
    assert_eq!(bounded::occupied(&ac), (8, 1));
    let used = ac.dbg_large_cache_used();
    assert_eq!(used % 9, 0);
    let one_span = used / 9;
    ac.dbg_set_large_cache_budget(Some(span_limit * one_span));

    bounded::deposit_all(&mut ac, vec![newcomer]);
    assert!(ac.dbg_large_cache_extension_materialised());
    assert_eq!(ac.dbg_large_cache_used(), span_limit * one_span);
    let (base, ext) = bounded::occupied(&ac);
    assert_eq!(base + ext, span_limit);
    bounded::assert_used(&ac);
}

#[test]
fn small_budget_caps_cache_even_with_extension_available() {
    check_budget(1);
}

#[test]
fn budget_bounds_bytes_not_slot_count() {
    check_budget(2);
}
