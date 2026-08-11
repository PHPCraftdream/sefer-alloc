//! Black-box invariant tests over the public API (Phase 1).
//!
//! These encode invariants I1–I7 from `docs/INVARIANTS.md` as observable
//! properties of [`Region`]/[`Handle`], with no access to private fields.
//! Generation wrap is `slotmap`'s responsibility (32-bit generation wraps
//! after ~2^31 cycles), so saturation is asserted only as a black-box
//! property (a reused slot does not honour a stale handle until wrap).
//!
//! This file is exercised under miri by CI's `miri-core` job (`cargo miri
//! test --test region_invariants`), so the I7 case below is also the only
//! place I7 (instance isolation) is checked under miri — including through
//! the `sefer_alloc::Region` re-export this file uses, not just the
//! `sefer-region` crate directly.

use std::cell::Cell;
use std::rc::Rc;

use sefer_alloc::Region;

/// A payload that counts how many times it is dropped, to check I5.
struct DropCounter(Rc<Cell<usize>>);

impl Drop for DropCounter {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

/// I1 / I2: insert→get→remove keeps other handles valid; a removed handle
/// resolves to `None` for roughly `2^31` reuse cycles of that slot; a second
/// remove is a no-op `None`.
#[test]
fn insert_get_remove_keeps_others_valid() {
    let mut r = Region::new();
    let a = r.insert(10u32);
    let b = r.insert(20u32);
    let c = r.insert(30u32);

    assert_eq!(r.len(), 3);
    assert_eq!(r.get(a), Some(&10));
    assert_eq!(r.get(b), Some(&20));
    assert_eq!(r.get(c), Some(&30));

    // Removing the middle handle must not disturb the others (I1 preserved
    // for survivors — the dense store stays compact).
    assert_eq!(r.remove(b), Some(20));
    assert_eq!(r.len(), 2);
    assert_eq!(r.get(b), None); // I2
    assert_eq!(r.remove(b), None); // I2: removing twice is a no-op
    assert_eq!(r.get(a), Some(&10));
    assert_eq!(r.get(c), Some(&30));
}

/// I3 (ABA): a handle whose slot was reused after removal does not resolve.
#[test]
fn stale_handle_after_reuse_is_none() {
    let mut r = Region::new();
    let a = r.insert(1u32);
    assert_eq!(r.remove(a), Some(1));
    let b = r.insert(2u32); // reuses a's slot with a bumped generation
    assert_eq!(r.get(a), None, "stale generation must not resolve");
    assert_eq!(r.get(b), Some(&2));
    assert_ne!(a, b, "a fresh handle to the reused slot must differ");
}

/// `get_mut` mutates the value in place.
#[test]
fn get_mut_mutates_in_place() {
    let mut r = Region::new();
    let h = r.insert(String::from("a"));
    r.get_mut(h).unwrap().push_str("bc");
    assert_eq!(r.get(h).map(String::as_str), Some("abc"));
}

/// I5 (drop-once): a drop-counting payload is dropped exactly once — on remove
/// or on `Region` drop — never twice, never leaked.
#[test]
fn drops_each_value_exactly_once() {
    let counter = Rc::new(Cell::new(0));
    {
        let mut r = Region::new();
        let _a = r.insert(DropCounter(counter.clone()));
        let b = r.insert(DropCounter(counter.clone()));
        let _c = r.insert(DropCounter(counter.clone()));
        drop(r.remove(b)); // drops exactly one here
        assert_eq!(counter.get(), 1);
        // region drops the remaining two on scope exit
    }
    assert_eq!(
        counter.get(),
        3,
        "expected exactly three drops, no double-free, no leak"
    );
}

/// I7 (instance isolation): a handle minted by one `Region` instance must be
/// rejected by a *different* `Region` instance of the same type, on every
/// accessor (`get`, `get_mut`, `remove`, `contains`) — even when the raw
/// underlying key collides (the first insert into a fresh `Region` commonly
/// produces the same key as the first insert into any other fresh `Region`).
/// This is the miri-covered instance of I7 — `region_invariants.rs` runs
/// under CI's `miri-core` job, so this is also the only I7 check exercised
/// under miri, through the `sefer_alloc::Region` re-export.
#[test]
fn handle_from_different_region_is_rejected_on_every_accessor() {
    let mut region_a = Region::new();
    let mut region_b = Region::new();

    let h_a = region_a.insert(1u32);
    let h_b = region_b.insert(2u32);

    // get
    assert_eq!(region_b.get(h_a), None, "I7: cross-region get must fail");
    assert_eq!(region_a.get(h_b), None, "I7: cross-region get must fail");

    // get_mut
    assert_eq!(
        region_b.get_mut(h_a),
        None,
        "I7: cross-region get_mut must fail"
    );
    assert_eq!(
        region_a.get_mut(h_b),
        None,
        "I7: cross-region get_mut must fail"
    );

    // contains
    assert!(
        !region_b.contains(h_a),
        "I7: cross-region contains must be false"
    );
    assert!(
        !region_a.contains(h_b),
        "I7: cross-region contains must be false"
    );

    // remove
    assert_eq!(
        region_b.remove(h_a),
        None,
        "I7: cross-region remove must fail"
    );
    assert_eq!(
        region_a.remove(h_b),
        None,
        "I7: cross-region remove must fail"
    );

    // Nothing was disturbed: both handles still resolve in their own region.
    assert_eq!(region_a.get(h_a), Some(&1));
    assert_eq!(region_b.get(h_b), Some(&2));
}

/// `clear` invalidates all outstanding handles and the region is reusable.
#[test]
fn clear_invalidates_all_handles() {
    let mut r = Region::new();
    let a = r.insert(1u32);
    let b = r.insert(2u32);
    r.clear();
    assert!(r.is_empty());
    assert_eq!(r.get(a), None);
    assert_eq!(r.get(b), None);
    assert!(!r.contains(a));
    assert!(!r.contains(b));

    // Region is reusable after clear.
    let c = r.insert(3u32);
    assert_eq!(r.get(c), Some(&3));
    assert_eq!(r.len(), 1);
}
