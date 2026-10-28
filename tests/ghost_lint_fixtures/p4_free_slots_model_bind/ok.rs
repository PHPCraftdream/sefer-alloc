// ghost-bind: heap-lease/claim-free-live
x.compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire);
// ghost-bind: heap-lease/drop-release
x.compare_exchange(LIVE, FREE, Ordering::Release, Ordering::Relaxed);
