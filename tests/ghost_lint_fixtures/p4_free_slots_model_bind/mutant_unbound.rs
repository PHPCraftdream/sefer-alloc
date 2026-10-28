// expect: ` in fixture/under_test.rs
// ghost-bind: heap-lease/claim-free-live
x.compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire);
x.compare_exchange(LIVE, FREE, Ordering::Release, Ordering::Relaxed);
