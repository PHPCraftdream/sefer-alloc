// ghost-bind: heap-lease/claim-free-live
const CLAIM: (Ordering, Ordering) = (Ordering::AcqRel, Ordering::Acquire);
// ghost-bind: heap-lease/drop-release
const DROP_LEASE: (Ordering, Ordering) = (Ordering::Release, Ordering::Relaxed);

fn owner_lifetime(slot: &Slot, p: Protocol) {
    let Some(lease) = claim(slot, p) else {
        return;
    };
    if p.drain_before_release {
        // ghost-bind: teardown/trim-before-release
        trim_for_recycle(lease.slot);
        drop(lease);
    } else {
        drop(lease);
        trim_for_recycle(slot);
    }
}
