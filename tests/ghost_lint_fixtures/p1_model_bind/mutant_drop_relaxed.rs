// expect: prod rule requires ["Release", "Relaxed"]
fn claim(slot: &Slot) -> bool {
    // ghost-bind: heap-lease/claim-free-live
    cas(slot, Free, Live, Ordering::AcqRel, Ordering::Acquire)
}

fn release(slot: &Slot) {
    // ghost-bind: heap-lease/drop-release
    let won = cas(slot, Live, Free, Ordering::Relaxed, Ordering::Relaxed);
    assert!(won);
}
