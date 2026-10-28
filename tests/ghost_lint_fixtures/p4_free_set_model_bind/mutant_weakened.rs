// expect: prod rule requires
// ghost-bind: free-set/publish-summary
x.summary.fetch_or(1, Ordering::Relaxed);
// ghost-bind: free-set/publish-word
x.word.fetch_or(1 << slot, Ordering::Release);
// ghost-bind: free-set/summary-clear-acqrel
x.summary.fetch_and(!1, Ordering::AcqRel);
// ghost-bind: free-set/take-acqrel
x.word.fetch_and(!bit, Ordering::AcqRel);
// ghost-bind: heap-lease/claim-free-live
x.compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire);
// ghost-bind: heap-lease/drop-release
x.compare_exchange(LIVE, FREE, Ordering::Release, Ordering::Relaxed);
// ghost-bind: maintenance-lease/claim-acqrel
x.compare_exchange(FREE, MAINT, Ordering::AcqRel, Ordering::Acquire);
// ghost-bind: maintenance-lease/drop-release
x.compare_exchange(MAINT, FREE, Ordering::Release, Ordering::Relaxed);
// ghost-bind: registry/bump-count-cas
const BUMP: (Ordering, Ordering) = (Ordering::AcqRel, Ordering::Acquire);
// ghost-bind: registry/hint-take-spill
x.hint.swap(NO_HINT, Ordering::AcqRel);
