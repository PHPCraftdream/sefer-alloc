//! Black-box invariant tests over the public API (Phase 1).
//!
//! These encode invariants I1–I5 from `docs/INVARIANTS.md` as observable
//! properties of [`Region`]/[`Handle`], with no access to private fields.
//! Generation-saturation / slot-retirement is now `slotmap`'s responsibility,
//! so the former white-box saturation tests are gone — saturation is asserted
//! only as a black-box property (a reused slot never honours a stale handle).

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

/// I1 / I2: insert→get→remove keeps other handles valid; a removed handle is
/// `None` forever; a second remove is a no-op `None`.
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
