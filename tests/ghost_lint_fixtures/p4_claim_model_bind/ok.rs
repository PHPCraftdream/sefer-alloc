// ghost-bind: heap-lease/claim-free-live
x.compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire);
// ghost-bind: heap-lease/drop-release
x.compare_exchange(LIVE, FREE, Ordering::Release, Ordering::Relaxed);
// ghost-bind: registry/hint-take-spill
x.hint.swap(NO_HINT, Ordering::AcqRel);
// ghost-bind: saturation/mark-cas
const SAT_MARK: (Ordering, Ordering) = (Ordering::AcqRel, Ordering::Acquire);
// ghost-bind: saturation/publish-acqrel
const SAT_PUBLISH: (Ordering, Ordering) = (Ordering::AcqRel, Ordering::Acquire);
// ghost-bind: saturation/rmw-read
x.saturation.fetch_or(0, Ordering::Acquire);
