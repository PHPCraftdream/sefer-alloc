// ghost-bind: maintenance-lease/claim-acqrel
// ghost-bind: heap-lease/claim-free-live
const CLAIM: (Ordering, Ordering) = (Ordering::AcqRel, Ordering::Acquire);
// ghost-bind: maintenance-lease/drop-release
// ghost-bind: heap-lease/drop-release
const RECYCLE: (Ordering, Ordering) = (Ordering::Release, Ordering::Relaxed);
