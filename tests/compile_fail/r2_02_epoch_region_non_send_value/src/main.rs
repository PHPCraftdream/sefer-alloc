// R2-02 (independent src review round 2, task #2004) — compile-fail bait for
// the FIXED `T: Send + 'static` bound on `EpochRegion::insert`/`remove`. This
// crate exists SOLELY as compile-fail bait, built by
// tests/regression_r2_02_epoch_region_non_send_value.rs as a child process;
// never part of the sefer-alloc workspace or its own target set (see this
// fixture's own Cargo.toml).
//
// Before task #2004, `EpochRegion<T>::insert`/`remove`/`remote_evict` (and
// `AtomicSlot<T>::install`/`try_evict_at` underneath them) had no `T: Send`
// bound, even though a removed/evicted value is handed to
// `crossbeam-epoch`'s GLOBAL collector via `guard.defer_destroy` — which may
// run the destructor on ANY thread at a later epoch boundary, regardless of
// whether the `EpochRegion` value itself ever crossed threads. Confirmed
// empirically before the fix: this exact `Rc<i32>` insert+remove sequence
// compiled and ran with zero errors.
//
// `main` inserts an `Rc<i32>` (a `!Send` type) — this MUST fail to compile
// with E0277 ("`Rc<i32>` cannot be sent between threads safely") after the
// fix, and did NOT fail before it.

use std::rc::Rc;

use sefer_alloc::EpochRegion;

fn main() {
    let region: EpochRegion<Rc<i32>> = EpochRegion::with_capacity(4);
    let handle = region.insert(Rc::new(1)).unwrap();
    let _ = region.remove(handle);
}
