// expect: model step teardown/trim-before-release
fn claim(s: &State) {
    // ghost-bind: heap-lease/claim-free-live
    const CLAIM: (Ordering, Ordering) = (Ordering::AcqRel, Ordering::Acquire);
    s.compare_exchange(SlotState::Free, SlotState::Live, CLAIM.0, CLAIM.1);
}
fn release(s: &State) {
    // ghost-bind: heap-lease/drop-release
    const DROP_LEASE: (Ordering, Ordering) = (Ordering::Release, Ordering::Relaxed);
    s.compare_exchange(SlotState::Live, SlotState::Free, DROP_LEASE.0, DROP_LEASE.1);
}
fn trim_for_recycle() {}
struct Lease;
fn teardown(lease: Lease) {
    // ghost-bind: teardown/trim-before-release
    drop(lease);
    trim_for_recycle();
}
