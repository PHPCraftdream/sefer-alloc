// expect: ` in fixture/under_test.rs
fn claim(slot: &Slot) -> bool {
    // ghost-bind: heap-lease/claim-free-live
    cas(slot, Free, Live, Ordering::AcqRel, Ordering::Acquire)
}

fn release(slot: &Slot) {
    let won = cas(slot, Live, Free, Ordering::Release, Ordering::Relaxed);
    assert!(won);
}
